//! v4 `import-configuration.ts` (`7189a968`) — the configuration-shaped
//! importers: prompt templates, the provider model catalogue, plugin
//! configuration, and instance settings.
//!
//! These carry setup rather than content, so they behave a little differently
//! from the entity importers: nothing is id-remapped (nothing references them
//! by id), and the last three upsert by natural key instead of minting rows.

use rusqlite::Connection;
use serde_json::Value;

use super::{ConflictStrategy, ImportOptions};
use crate::db::prompt_templates::{PromptTemplatesRepository, PtCreate};
use crate::db::provider_models::{PmCreate, ProviderModelsRepository};
use crate::db::DbError;

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn os(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Prompt templates, mirroring the roleplay-template importer. Built-ins never
/// appear in an archive (the writer filters them), so every row here is
/// user-created. Dedup is by NAME, which is what a user recognises;
/// `duplicate` renames to `"<name> (imported)"`.
pub(super) fn import_prompt_templates(
    main: &Connection,
    user_id: &str,
    templates: &[Value],
    options: &ImportOptions,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let repo = PromptTemplatesRepository::new(main);
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for template in templates {
        let name = s(template, "name");
        let out = (|| -> Result<bool, DbError> {
            let existing = crate::db::prompt_templates::find_by_name(main, user_id, &name)?;

            if let Some(existing_id) = &existing {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => return Ok(false),
                    ConflictStrategy::Overwrite => {
                        repo.delete(existing_id)?;
                    }
                    ConflictStrategy::Duplicate => {}
                }
            }

            let final_name =
                if existing.is_some() && options.conflict_strategy == ConflictStrategy::Duplicate {
                    format!("{name} (imported)")
                } else {
                    name.clone()
                };

            let now = crate::clock::now_iso();
            repo.create(
                &PtCreate {
                    user_id: Some(user_id.to_string()),
                    name: final_name,
                    content: s(template, "content"),
                    description: os(template, "description"),
                    is_built_in: template
                        .get("isBuiltIn")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    category: os(template, "category"),
                    model_hint: os(template, "modelHint"),
                    tags: template
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                },
                &crate::db::prompt_templates::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            Ok(true)
        })();
        match out {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                warnings.push(format!("Failed to import prompt template \"{name}\": {e}"));
            }
        }
    }

    Ok(Counts { imported, skipped })
}

/// The provider model catalogue — a **regenerable cache**: every row here is
/// normally minted by a live refetch from the provider, and the next refetch
/// supersedes whatever an import wrote. Upsert by (provider, modelId) with ids
/// and timestamps stripped — nothing references a model row by id.
pub(super) fn import_provider_models(
    main: &Connection,
    models: &[Value],
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let repo = ProviderModelsRepository::new(main);
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for model in models {
        let model_id = s(model, "modelId");
        let out = repo.upsert_model(&PmCreate {
            provider: s(model, "provider"),
            model_id: model_id.clone(),
            model_type: s(model, "modelType"),
            display_name: s(model, "displayName"),
            base_url: os(model, "baseUrl"),
            context_window: model.get("contextWindow").and_then(Value::as_f64),
            max_output_tokens: model.get("maxOutputTokens").and_then(Value::as_f64),
            deprecated: model
                .get("deprecated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            experimental: model
                .get("experimental")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
        match out {
            Ok(_) => imported += 1,
            Err(e) => {
                warnings.push(format!(
                    "Failed to import provider model \"{model_id}\": {e}"
                ));
                skipped += 1;
            }
        }
    }

    Ok(Counts { imported, skipped })
}

/// Plugin configuration. `upsert_for_user_plugin` **merges** into any existing
/// config, which is exactly what we want: the exporter redacts password-typed
/// keys, so a redacted key simply doesn't overwrite whatever secret this
/// instance already holds. `_redactedKeys` is surfaced as an import warning so
/// the user knows which secrets they have to re-enter by hand.
pub(super) fn import_plugin_configs(
    main: &Connection,
    user_id: &str,
    configs: &[Value],
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let repo = crate::db::plugin_config::PluginConfigRepository::new(main);
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for config in configs {
        let plugin_name = s(config, "pluginName");
        let bag = config.get("config").cloned().unwrap_or(Value::Null);
        // The tri-state carry (v4 `7189a968`): an absent `enabled` leaves the
        // stored flag UNTOUCHED on both branches.
        let enabled = config.get("enabled").and_then(Value::as_bool);
        match repo.upsert_for_user_plugin(user_id, &plugin_name, &bag, enabled) {
            Ok(_) => {
                imported += 1;
                let redacted: Vec<&str> = config
                    .get("_redactedKeys")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if !redacted.is_empty() {
                    warnings.push(if redacted.contains(&"*") {
                        format!(
                            "Plugin \"{plugin_name}\" was exported without its settings because its manifest \
                             was unavailable; re-enter them on this instance."
                        )
                    } else {
                        format!(
                            "Plugin \"{plugin_name}\" was imported without these secret settings: \
                             {}. Re-enter them on this instance.",
                            redacted.join(", ")
                        )
                    });
                }
            }
            Err(e) => {
                warnings.push(format!(
                    "Failed to import plugin config for \"{plugin_name}\": {e}"
                ));
                skipped += 1;
            }
        }
    }

    Ok(Counts { imported, skipped })
}

/// Instance settings — the "move my setup" import. Values overwrite the
/// receiving instance's own, unconditionally: that *is* the point of the type,
/// so the conflict strategy does not apply. Keys that only make sense inside
/// the exporting instance never make it into an archive in the first place
/// (`NON_PORTABLE_INSTANCE_SETTING_KEYS`).
pub(super) fn import_instance_settings(
    main: &Connection,
    settings: &[Value],
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for setting in settings {
        let key = s(setting, "key");
        match crate::db::instance_settings::write_instance_setting(main, &key, &s(setting, "value"))
        {
            Ok(()) => imported += 1,
            Err(e) => {
                warnings.push(format!("Failed to import instance setting \"{key}\": {e}"));
                skipped += 1;
            }
        }
    }

    Ok(Counts { imported, skipped })
}
