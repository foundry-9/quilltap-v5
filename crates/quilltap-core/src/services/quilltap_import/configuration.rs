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

/// v4's `_create` validation of a prompt template (`base.repository.ts:
/// 350-368` over `PromptTemplateSchema`) — the ONE parse the `.qtap` import
/// and the backup restore run BEFORE the write (P4.161 Tier 2, R-A; the home
/// a later order may move beside `db::prompt_templates`). `item` is the data
/// handed to `promptTemplates.create` (the raw template minus `id` / `userId`
/// / `createdAt` / `updatedAt`, `userId` set, any rename applied — unknown
/// keys stripped); the entity is `{...item, id, createdAt, updatedAt}` with a
/// minted id. `Err` is the `ZodError.message`; `Ok` the row with the schema's
/// defaults (`isBuiltIn` false, `tags` `[]`).
pub(crate) fn parse_create_prompt_template(item: &Value) -> Result<PtCreate, String> {
    use crate::api::zod_issues::{zod_error_message, zod_prompt_template_issues};
    let mut entity = item.as_object().cloned().unwrap_or_default();
    let now = crate::clock::now_iso();
    entity.insert("id".into(), Value::String(uuid::Uuid::new_v4().to_string()));
    entity.insert("createdAt".into(), Value::String(now.clone()));
    entity.insert("updatedAt".into(), Value::String(now));
    let issues = zod_prompt_template_issues(&entity);
    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    let e = Value::Object(entity);
    Ok(PtCreate {
        user_id: os(&e, "userId"),
        name: s(&e, "name"),
        content: s(&e, "content"),
        description: os(&e, "description"),
        is_built_in: e.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false),
        category: os(&e, "category"),
        model_hint: os(&e, "modelHint"),
        tags: e
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// A refused prompt-template create's three repository ERRORs (validate →
/// `_create` → the repository's wrap), then the caller's warning / WARN.
pub(crate) fn log_refused_prompt_template(user_id: &str, name: Option<&str>, zod: &str) {
    super::log_refused_create("prompt_templates", zod);
    super::log_prompt_template_create_wrap_failure(
        user_id,
        name,
        &DbError::Internal(zod.to_string()),
    );
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
        let out = (|| -> Result<bool, String> {
            let existing = crate::db::prompt_templates::find_by_name(main, user_id, &name)
                .map_err(|e| super::item_error_text(&e))?;

            if let Some(existing_id) = &existing {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => return Ok(false),
                    ConflictStrategy::Overwrite => {
                        repo.delete(existing_id)
                            .map_err(|e| super::item_error_text(&e))?;
                    }
                    ConflictStrategy::Duplicate => {}
                }
            }

            // v4 `import-configuration.ts:56-69`: the raw template minus its
            // id / userId / stamps, `userId` set, the `duplicate` rename —
            // validated WHOLE by `_create` before the insert (P4.161 Tier 2;
            // v5 used to coerce every key and write whatever passed serde).
            let mut item = template.as_object().cloned().unwrap_or_default();
            for k in ["id", "userId", "createdAt", "updatedAt"] {
                item.remove(k);
            }
            item.insert("userId".into(), Value::String(user_id.to_string()));
            if existing.is_some() && options.conflict_strategy == ConflictStrategy::Duplicate {
                item.insert("name".into(), Value::String(format!("{name} (imported)")));
            }
            let create =
                parse_create_prompt_template(&Value::Object(item.clone())).inspect_err(|zod| {
                    log_refused_prompt_template(
                        user_id,
                        item.get("name").and_then(Value::as_str),
                        zod,
                    )
                })?;

            let now = crate::clock::now_iso();
            repo.create(
                &create,
                &crate::db::prompt_templates::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )
            .map_err(|e| super::item_error_text(&e))?;
            Ok(true)
        })();
        match out {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(text) => {
                warnings.push(format!(
                    "Failed to import prompt template \"{name}\": {text}"
                ));
                // v4 `import-configuration.ts:78` (P4.148 Tier 2 item 17).
                tracing::warn!(
                    templateId = super::id_field(template),
                    error = %text,
                    "Failed to import prompt template"
                );
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
                let text = super::item_error_text(&e);
                warnings.push(format!(
                    "Failed to import provider model \"{model_id}\": {text}"
                ));
                // v4 `import-configuration.ts:119` (P4.148 Tier 2 item 17).
                tracing::warn!(modelId = %model_id, error = %text, "Failed to import provider model");
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
                let text = super::item_error_text(&e);
                warnings.push(format!(
                    "Failed to import plugin config for \"{plugin_name}\": {text}"
                ));
                // v4 `import-configuration.ts:178` (P4.148 Tier 2 item 17).
                tracing::warn!(pluginName = %plugin_name, error = %text, "Failed to import plugin config");
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
                let text = super::item_error_text(&e);
                warnings.push(format!(
                    "Failed to import instance setting \"{key}\": {text}"
                ));
                // v4 `import-configuration.ts:219` (P4.148 Tier 2 item 17).
                tracing::warn!(key = %key, error = %text, "Failed to import instance setting");
                skipped += 1;
            }
        }
    }

    Ok(Counts { imported, skipped })
}
