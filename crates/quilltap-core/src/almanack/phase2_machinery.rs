//! The Almanack — Phase 2, "Cataloguing the machinery"
//! (v4 `lib/tools/almanack/phase2-machinery.ts`).
//!
//! Plugins, providers, models, API keys, MCP servers and themes: everything the
//! establishment has bolted on, and the state of the caches that describe it.
//!
//! ## ⚠ The phase with real divergences — recorded, not hidden
//!
//! v4 reads four *runtime registries* v5 does not have, and the honest thing is
//! to report v5's actual state rather than to invent v4's:
//!
//! - **Plugins.** v4 loads npm/bundled plugin manifests at runtime
//!   (`getAllPlugins()`), so it can list them and roll them up by capability.
//!   v5's providers are compiled-in declarative manifests, not loadable plugins;
//!   there is no loader, no `pluginPath`, no per-plugin `enabled` flag. The two
//!   DATABASE-derived figures — `pluginConfigRows` and
//!   `characterPluginDataRows` — are ported exactly; the lists and the
//!   capability roll-up are empty, and the renderer's `*None*` arms handle that
//!   without a special case.
//! - **Themes.** v4's theme registry is server-side; v5's theme packs are
//!   client-side assets. `activeThemeId` and `colorMode` come off
//!   `chat_settings` and are ported exactly; the theme LIST and its
//!   icon-override statistics are empty.
//! - **Live model discovery.** v4's `collectModels` prefers the
//!   `provider_models` cache and falls back to a live API call, then to a
//!   provider's static list. v5 ports the CACHE branch faithfully; a cold cache
//!   yields no entry for that provider rather than spending a network call
//!   inside a diagnostic report. (An instance whose cache is warm — which is
//!   every instance that has ever picked a model — reads identically.)
//! - **`apiKeyTypes` / provider capabilities / image + embedding providers**
//!   come from the compiled-in manifest registry, which IS v5's provider
//!   registry, so these are ported exactly.
//!
//! The tier-2 differential normalizes the plugin list, the theme list and the
//! model lists — and ONLY those — so every ported figure beside them is still
//! asserted equal.

use serde_json::Value;

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::{connection_profiles, embedding_profiles, plugin_config, provider_models, DbError};
use crate::provider_manifest::{Manifest, Registry};

use super::db::{main_rows, num_col, opt_text, text};
use super::types::*;

/// v4 `collectPluginInfo` — see the module header. The two DB counts are v4's
/// queries verbatim; the registry-derived halves are empty in v5.
pub fn collect_plugin_info(db: &Db) -> Result<PluginBreakdown, DbError> {
    let count = |table: &str, context: &str| -> f64 {
        main_rows(
            db,
            &format!(r#"SELECT COUNT(*) AS n FROM "{table}""#),
            &[],
            context,
            |r| num_col(r, 0),
        )
        .first()
        .copied()
        .unwrap_or(0.0)
    };
    Ok(PluginBreakdown {
        enabled: Vec::new(),
        disabled: Vec::new(),
        by_capability: Vec::new(),
        npm_installed: 0.0,
        bundled: 0.0,
        plugin_config_rows: count("plugin_configs", "machinery.pluginConfigs"),
        character_plugin_data_rows: count("character_plugin_data", "machinery.characterPluginData"),
    })
}

/// v4 `collectApiKeyTypes` — every API-key label a registered provider could ask
/// for, sorted. `apiKeyLabel || "<displayName> API Key"` is v4's exact fallback.
pub fn collect_api_key_types(registry: &Registry) -> Vec<String> {
    let mut labels: Vec<String> = registry
        .all_providers()
        .iter()
        .filter(|p| p.config_requirements.requires_api_key)
        .map(|p| {
            p.config_requirements
                .api_key_label
                .clone()
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| format!("{} API Key", p.display_name))
        })
        .collect();
    labels.sort_by(|a, b| locale_compare(a, b));
    labels
}

/// The set of providers this instance has configured: any provider named by an
/// API key OR by a connection profile (v4 reads both).
fn configured_providers(db: &Db, user_id: &str) -> Vec<String> {
    let mut set: Vec<String> = main_rows(
        db,
        r#"SELECT DISTINCT "provider" FROM "api_keys" WHERE "userId" = ?"#,
        &[&user_id],
        "machinery.configuredApiKeyProviders",
        |r| text(r, 0),
    );
    for p in main_rows(
        db,
        r#"SELECT DISTINCT "provider" FROM "connection_profiles" WHERE "userId" = ?"#,
        &[&user_id],
        "machinery.configuredProfileProviders",
        |r| text(r, 0),
    ) {
        if !set.contains(&p) {
            set.push(p);
        }
    }
    set
}

/// v4 `collectProviderInfo` — the provider registry annotated with whether this
/// instance has each provider configured.
pub fn collect_provider_info(
    db: &Db,
    registry: &Registry,
    user_id: &str,
) -> Result<Vec<ProviderInfo>, DbError> {
    let configured = configured_providers(db, user_id);
    let mut rows: Vec<ProviderInfo> = registry
        .all_providers()
        .iter()
        .map(|p| ProviderInfo {
            name: p.id.clone(),
            display_name: p.display_name.clone(),
            configured: configured.contains(&p.id),
            capabilities: ProviderCapabilities {
                chat: p.capabilities.chat,
                image_generation: p.capabilities.image_generation,
                embeddings: p.capabilities.embeddings,
                web_search: p.capabilities.web_search,
            },
            requires_api_key: p.config_requirements.requires_api_key,
            requires_base_url: p.config_requirements.requires_base_url,
        })
        .collect();
    rows.sort_by(|a, b| locale_compare(&a.display_name, &b.display_name));
    Ok(rows)
}

/// Cached model ids of one `modelType` for one provider, sorted (v4's
/// `providerModels.findByProvider(provider, type).map(m => m.modelId)`).
fn cached_models(db: &Db, provider: &str, model_type: &str) -> Vec<String> {
    let rows = db
        .read_main(|c| provider_models::find_by_provider(c, provider))
        .unwrap_or_default();
    rows.iter()
        .filter(|m| m.get("modelType").and_then(Value::as_str) == Some(model_type))
        .filter_map(|m| m.get("modelId").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// v4 `collectModels` — the CACHE branch (see the module header on the live
/// fallback). v4 caps each provider's list at 50 after sorting.
pub fn collect_models(db: &Db, user_id: &str) -> Result<Vec<ModelInfo>, DbError> {
    // v4 keys off the ACTIVE api keys only, one per provider.
    let providers: Vec<String> = main_rows(
        db,
        r#"SELECT DISTINCT "provider" FROM "api_keys" WHERE "userId" = ? AND "isActive" = 1"#,
        &[&user_id],
        "machinery.activeKeyProviders",
        |r| text(r, 0),
    );

    let mut out: Vec<ModelInfo> = Vec::new();
    for provider in providers {
        let mut models = cached_models(db, &provider, "chat");
        if models.is_empty() {
            // Cold cache: v4 would go and ask the provider. See the header.
            continue;
        }
        models.sort_by(|a, b| locale_compare(a, b));
        models.truncate(50);
        out.push(ModelInfo {
            provider,
            models,
            error: None,
        });
    }
    out.sort_by(|a, b| locale_compare(&a.provider, &b.provider));
    Ok(out)
}

/// v4 `collectProviderModelCache` — how fresh the cached model lists are.
///
/// A stale cache is exactly how a silently broken discovery endpoint presents
/// itself, so the newest `updatedAt` per provider gets a column of its own.
pub fn collect_provider_model_cache(db: &Db) -> Result<Vec<ProviderModelCacheRow>, DbError> {
    Ok(main_rows(
        db,
        r#"SELECT "provider"                                                       AS provider,
            COUNT(*)                                                         AS total,
            SUM(CASE WHEN "modelType" = 'chat' THEN 1 ELSE 0 END)            AS chat,
            SUM(CASE WHEN "modelType" = 'image' THEN 1 ELSE 0 END)           AS image,
            SUM(CASE WHEN "modelType" = 'embedding' THEN 1 ELSE 0 END)       AS embedding,
            MAX("updatedAt")                                                 AS newestUpdatedAt
     FROM "provider_models"
     GROUP BY "provider"
     ORDER BY "provider""#,
        &[],
        "machinery.providerModelCache",
        |r| {
            Ok(ProviderModelCacheRow {
                provider: text(r, 0)?,
                total: num_col(r, 1)?,
                chat: num_col(r, 2)?,
                image: num_col(r, 3)?,
                embedding: num_col(r, 4)?,
                newest_updated_at: opt_text(r, 5)?,
            })
        },
    ))
}

/// v4 `collectApiKeyUsage` — API keys per provider, with the never-used ones
/// called out. A key that has never once been used is either a spare or a
/// mistake, and the column that tells them apart already exists.
pub fn collect_api_key_usage(db: &Db, user_id: &str) -> Result<Vec<ApiKeyUsageRow>, DbError> {
    Ok(main_rows(
        db,
        r#"SELECT "provider"                                            AS provider,
            COUNT(*)                                              AS total,
            SUM(CASE WHEN "isActive" = 1 THEN 1 ELSE 0 END)       AS active,
            SUM(CASE WHEN "lastUsed" IS NULL THEN 1 ELSE 0 END)   AS neverUsed,
            MAX("lastUsed")                                       AS lastUsed
     FROM "api_keys"
     WHERE "userId" = ?
     GROUP BY "provider"
     ORDER BY "provider""#,
        &[&user_id],
        "machinery.apiKeyUsage",
        |r| {
            Ok(ApiKeyUsageRow {
                provider: text(r, 0)?,
                total: num_col(r, 1)?,
                active: num_col(r, 2)?,
                never_used: num_col(r, 3)?,
                last_used: opt_text(r, 4)?,
            })
        },
    ))
}

fn designated_from_profile(profile: &Value) -> DesignatedProfileInfo {
    DesignatedProfileInfo {
        provider: profile
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_string),
        model: profile
            .get("modelName")
            .and_then(Value::as_str)
            .map(str::to_string),
        profile_name: profile
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4 `collectCheapLLMInfo` — the profile designated as the cheap background
/// worker.
pub fn collect_cheap_llm_info(db: &Db, _user_id: &str) -> Result<DesignatedProfileInfo, DbError> {
    let profiles = db
        .read_main(connection_profiles::find_all)
        .unwrap_or_default();
    Ok(profiles
        .iter()
        .find(|p| p.get("isCheap").and_then(Value::as_bool).unwrap_or(false))
        .map(designated_from_profile)
        .unwrap_or_default())
}

/// v4 `collectImagePromptLLMInfo` — the separate override used for expanding
/// image prompts, if configured.
pub fn collect_image_prompt_llm_info(
    db: &Db,
    user_id: &str,
) -> Result<DesignatedProfileInfo, DbError> {
    let settings = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))
        .unwrap_or(None)
        .unwrap_or(Value::Null);
    let profile_id = settings
        .get("cheapLLMSettings")
        .and_then(|v| v.get("imagePromptProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let Some(profile_id) = profile_id else {
        return Ok(DesignatedProfileInfo::default());
    };
    let profile = db
        .read_main(|c| connection_profiles::find_by_id(c, profile_id))
        .unwrap_or(None);
    Ok(profile
        .as_ref()
        .map(designated_from_profile)
        .unwrap_or_default())
}

/// v4 `collectEmbeddingInfo` — the default embedding profile.
pub fn collect_embedding_info(db: &Db, user_id: &str) -> Result<DesignatedProfileInfo, DbError> {
    let Some(row) = db
        .read_main(|c| embedding_profiles::find_default(c, user_id))
        .unwrap_or(None)
    else {
        return Ok(DesignatedProfileInfo::default());
    };
    // The scoped row omits `name`, which the section prints — read it beside.
    let name = db
        .read_main(|c| embedding_profiles::find_default_id_name(c, user_id))
        .unwrap_or(None)
        .map(|(_, name)| name);
    Ok(DesignatedProfileInfo {
        provider: Some(row.provider),
        model: Some(row.model_name),
        profile_name: name,
    })
}

/// Providers with a capability, with their cached models (v4's cache-first,
/// static-fallback pair — v5 has no per-provider static model list, so the
/// cache is the only source).
fn providers_by_capability(
    db: &Db,
    registry: &Registry,
    predicate: impl Fn(&Manifest) -> bool,
    model_type: &str,
) -> Vec<(String, String, Vec<String>)> {
    let mut rows: Vec<(String, String, Vec<String>)> = registry
        .all_providers()
        .iter()
        .filter(|p| predicate(p))
        .map(|p| {
            let mut models = cached_models(db, &p.id, model_type);
            models.sort_by(|a, b| locale_compare(a, b));
            (p.id.clone(), p.display_name.clone(), models)
        })
        .collect();
    rows.sort_by(|a, b| locale_compare(&a.1, &b.1));
    rows
}

/// v4 `collectImageProviders`.
pub fn collect_image_providers(
    db: &Db,
    registry: &Registry,
) -> Result<Vec<ImageProviderInfo>, DbError> {
    Ok(
        providers_by_capability(db, registry, |p| p.capabilities.image_generation, "image")
            .into_iter()
            .map(|(provider, display_name, models)| ImageProviderInfo {
                provider,
                display_name,
                models,
            })
            .collect(),
    )
}

/// v4 `collectEmbeddingProviders`.
pub fn collect_embedding_providers(
    db: &Db,
    registry: &Registry,
) -> Result<Vec<EmbeddingProviderInfo>, DbError> {
    Ok(
        providers_by_capability(db, registry, |p| p.capabilities.embeddings, "embedding")
            .into_iter()
            .map(|(provider, display_name, models)| EmbeddingProviderInfo {
                provider,
                display_name,
                models,
            })
            .collect(),
    )
}

/// v4 `collectMCPServers` — names only, never URLs or auth fields. Reads the
/// `qtap-plugin-mcp` config row, which is a plain database read and so ports
/// exactly even though v5 cannot load the plugin itself.
pub fn collect_mcp_servers(db: &Db, user_id: &str) -> Result<McpServersInfo, DbError> {
    let defaults = McpServersInfo {
        configured: 0.0,
        enabled: 0.0,
        server_names: Vec::new(),
        auto_reconnect: true,
        max_reconnect_attempts: 5.0,
    };

    let configs = db
        .read_main(|c| plugin_config::find_by_user_id(c, user_id))
        .unwrap_or_default();
    let Some((_, config)) = configs.iter().find(|(name, _)| name == "qtap-plugin-mcp") else {
        return Ok(defaults);
    };

    // v4 accepts the `servers` field as either a JSON string or an array, and
    // treats anything else (or a parse failure) as no servers at all.
    let servers: Vec<Value> = match config.get("servers") {
        Some(Value::String(s)) => serde_json::from_str::<Value>(s)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default(),
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };

    let mut server_names = Vec::new();
    let mut enabled_count = 0.0;
    for server in &servers {
        // v4 `server.displayName || server.name || 'Unnamed Server'` — a falsy
        // (absent OR empty) displayName falls through.
        let name = server
            .get("displayName")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                server
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("Unnamed Server");
        server_names.push(name.to_string());
        if server.get("enabled") == Some(&Value::Bool(true)) {
            enabled_count += 1.0;
        }
    }

    Ok(McpServersInfo {
        configured: servers.len() as f64,
        enabled: enabled_count,
        server_names,
        auto_reconnect: config
            .get("autoReconnect")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        max_reconnect_attempts: config
            .get("maxReconnectAttempts")
            .and_then(Value::as_f64)
            .unwrap_or(5.0),
    })
}

/// v4 `collectThemeInfo` — see the module header. The active theme and colour
/// mode come off `chat_settings` and are ported exactly; the theme inventory
/// and its icon-override statistics are client-side in v5.
pub fn collect_theme_info(db: &Db, user_id: &str) -> Result<ThemeReportInfo, DbError> {
    let settings = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))
        .unwrap_or(None)
        .unwrap_or(Value::Null);
    let pref = settings.get("themePreference");
    Ok(ThemeReportInfo {
        active_theme_id: pref
            .and_then(|v| v.get("activeThemeId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        color_mode: pref
            .and_then(|v| v.get("colorMode"))
            .and_then(Value::as_str)
            .unwrap_or("system")
            .to_string(),
        stats: ThemeStats {
            total: 0.0,
            with_dark_mode: 0.0,
            with_css_overrides: 0.0,
            errors: 0.0,
        },
        themes: Vec::new(),
        themes_with_icons: 0.0,
        total_icon_overrides: 0.0,
        total_unknown_icon_overrides: 0.0,
    })
}
