//! The Settings-server dispatch handlers (P4.6d) — the route-logic backfill the
//! first Settings vertical (the Providers tab + the setup wizard + basic
//! Appearance) consumes, composed over already-ported repos/services.
//!
//! Each handler is a differential port of a v4 route handler (the oracle): the
//! chat-settings PUT + default-injection (`settings/chat/route.ts`), connection
//! profiles (`connection-profiles/route.ts` + `[id]/`), API keys
//! (`api-keys/route.ts` + `[id]/`), the providers listing (`providers/route.ts`),
//! and the models read/fetch (`models/route.ts`). Returns a [`Response`] directly
//! (the engine arm is a one-line delegate). Reads go through the read pool
//! ([`Db::read_main`]); writes through [`Db::write`].
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! harness can drive with the fixture's own user id on both sides; the engine
//! passes `SINGLE_USER_ID`.
//!
//! ## The wire-action seams
//!
//! Three families of actions need a live provider wire — connection test
//! (`test-connection`), test message (`test-message`), API-key test (`?action=
//! test`), and models fetch (`POST /models`). v4 drives these through
//! `providerRegistry.validateApiKey` / `plugin.sendMessage` / `getAvailableModels`
//! (host-side network). This port keeps the wire an **injected seam** so the
//! differential cans it on both sides (the W4.7 recipe):
//!   - [`ConnectionValidator`] — the boolean/error outcome of v4's
//!     `validateApiKey` (the per-provider validate WIRE is v4 plugin internals,
//!     not ported; the differential pins the boolean);
//!   - [`crate::model::completion::CompletionProvider`] — the tier-3 completion
//!     seam for `test-message` (canned by recorded key, the tier-3 precedent);
//!   - [`ModelsFetcher`] — the model-id list outcome of `getAvailableModels`
//!     (the differential records what v4 returned; the CACHE it produces through
//!     the ported `provider_models::upsert_model_for_provider` is the load-bearing
//!     verified effect).
//!
//! The engine dispatch wires the PURE actions (CRUD, listing, cached models GET,
//! chat settings) live; the wire actions route to these functions and are engine-
//! gated behind a `not assembled` refusal until a host provider-actions driver is
//! wired (the swipe-generate deferral precedent).

use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{
    api_keys, chat_settings, connection_profiles, instance_settings, provider_models,
    roleplay_templates, tags, DbError,
};
use crate::provider_manifest::{Capability, Registry};
use crate::services::profile_names::normalize_profile_name;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// v4 `maskApiKey` (`lib/encryption.ts`): `<12` chars → the 12-bullet literal;
/// else `first8 + '••••' + last4`. Callers pre-truncate the key to its first 32
/// chars (the list/GET path) or pass it whole (import-preview, not ported).
fn mask_api_key(api_key: &str) -> String {
    // Operate on Unicode chars (v4 `String.length`/`substring` are UTF-16, but
    // synthetic keys are ASCII, so chars == code units).
    let chars: Vec<char> = api_key.chars().collect();
    if chars.is_empty() || chars.len() < 12 {
        return "••••••••••••".to_string();
    }
    let prefix: String = chars[..8].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}••••{suffix}")
}

/// v4 `maskApiKey(key.key_value.substring(0, 32))` — the list/GET masking: the raw
/// key is first truncated to its first 32 chars, then masked.
fn mask_api_key_preview(key_value: &str) -> String {
    let truncated: String = key_value.chars().take(32).collect();
    mask_api_key(&truncated)
}

/// v4 `isValidModelClassName` (`lib/llm/model-classes.ts`) — exact, case-sensitive
/// membership over the four class names.
fn is_valid_model_class_name(name: &str) -> bool {
    matches!(name, "Compact" | "Standard" | "Extended" | "Deep")
}

/// v4 `supportsImageGeneration` (`lib/llm/image-capable.ts`): upper-case the
/// provider, then the registry `imageGeneration` capability (uninitialized → false;
/// here the registry is always built).
fn supports_image_generation(provider: &str) -> bool {
    Registry::built_in().supports_capability(&provider.to_uppercase(), Capability::ImageGeneration)
}

/// v4 `requiresBaseUrl` (`lib/plugins/provider-validation.ts`):
/// `getConfigRequirements(provider)?.requiresBaseUrl ?? false` — exact-case, unknown
/// → false.
fn requires_base_url(provider: &str) -> bool {
    Registry::built_in()
        .get_provider(provider)
        .map(|m| m.config_requirements.requires_base_url)
        .unwrap_or(false)
}

/// v4 `requiresApiKey`: `getConfigRequirements(provider)?.requiresApiKey ?? true` —
/// exact-case, unknown → **true** (fail-safe, asymmetric with `requiresBaseUrl`).
/// `pub(super)` — the live provider-actions validator reuses the guard.
pub(super) fn requires_api_key(provider: &str) -> bool {
    Registry::built_in()
        .get_provider(provider)
        .map(|m| m.config_requirements.requires_api_key)
        .unwrap_or(true)
}

/// v4 `validateProviderConfig` — `{valid, errors}`. Provider-not-found (exact-case)
/// → single-error `Provider '<p>' not found`; else base-URL error (pushed FIRST)
/// then API-key error, per requirement + JS-falsy value.
fn validate_provider_config(provider: &str, api_key: &str, base_url: Option<&str>) -> Vec<String> {
    let registry = Registry::built_in();
    let Some(manifest) = registry.get_provider(provider) else {
        return vec![format!("Provider '{provider}' not found")];
    };
    let req = &manifest.config_requirements;
    let mut errors = Vec::new();
    // `!config.baseUrl` — JS-falsy (empty / absent).
    let has_base = base_url.map(|b| !b.is_empty()).unwrap_or(false);
    if req.requires_base_url && !has_base {
        let label = req.base_url_label.as_deref().unwrap_or("Base URL");
        errors.push(format!("{label} is required for {provider}"));
    }
    if req.requires_api_key && api_key.is_empty() {
        let label = req.api_key_label.as_deref().unwrap_or("API key");
        errors.push(format!("{label} is required for {provider}"));
    }
    errors
}

/// v4 `enrichWithApiKey(apiKeyId, repos)` → `{id,label,provider,isActive}` | null.
/// `apiKeyId` falsy → null; UNSCOPED lookup (`findApiKeyById`); not found → null.
fn enrich_with_api_key(conn: &rusqlite::Connection, api_key_id: Option<&str>) -> Value {
    let Some(id) = api_key_id.filter(|s| !s.is_empty()) else {
        return Value::Null;
    };
    match api_keys::find_by_id(conn, id) {
        Ok(Some(k)) => json!({
            "id": k.id,
            "label": k.label,
            "provider": k.provider,
            "isActive": k.is_active,
        }),
        _ => Value::Null,
    }
}

/// v4 `enrichWithTags(tagIds, repos)` → `[{tagId, tag}]`, preserving input order,
/// dropping unresolved ids. The nested `tag` object is the full v4 `Tag`; the
/// ported [`tags::find_by_ids`] returns only `{id,name}` — a **documented seam**
/// (full-tag marshaling on profiles is deferred; the corpus keeps profiles
/// tag-less so both sides produce `[]`).
fn enrich_with_tags(conn: &rusqlite::Connection, tag_ids: &[String]) -> Value {
    if tag_ids.is_empty() {
        return json!([]);
    }
    let rows = tags::find_by_ids(conn, tag_ids).unwrap_or_default();
    let by_id: std::collections::HashMap<&str, &tags::TagRow> =
        rows.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut out = Vec::new();
    for id in tag_ids {
        if let Some(t) = by_id.get(id.as_str()) {
            out.push(json!({ "tagId": id, "tag": { "id": t.id, "name": t.name } }));
        }
    }
    Value::Array(out)
}

/// Build the enriched-profile object: `{...profile, apiKey [, tags]}` (v4
/// `enrichProfile` / the list mapper / the create response).
fn enrich_profile(
    conn: &rusqlite::Connection,
    profile: &Value,
    with_tags: bool,
) -> Result<Value, DbError> {
    let mut obj = profile.as_object().cloned().unwrap_or_default();
    let api_key_id = obj.get("apiKeyId").and_then(Value::as_str);
    obj.insert("apiKey".into(), enrich_with_api_key(conn, api_key_id));
    if with_tags {
        let tag_ids: Vec<String> = obj
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        obj.insert("tags".into(), enrich_with_tags(conn, &tag_ids));
    }
    Ok(Value::Object(obj))
}

// ===========================================================================
// Chat settings (v4 settings/chat/route.ts) — GET default-injection + PUT
// ===========================================================================

/// v4 `GET /api/v1/settings/chat` (`handler`): return the user's settings, or —
/// closing the P4.6a deferral — default-inject via the `updateForUser`-equivalent
/// (the captured seed = v4's `updateForUser(userId, {defaults})`) when no row
/// exists, then return it.
pub async fn chat_settings_get(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    match db.read_main(move |conn| chat_settings::find_by_user_id(conn, &uid)) {
        Ok(Some(settings)) => return Response::ChatSettings(settings),
        Ok(None) => {}
        Err(e) => return internal(e),
    }
    // No row → create the default row (v4 `updateForUser(userId, defaults)`).
    let now = crate::clock::now_iso();
    let uid = user_id.to_string();
    let created = db
        .write(move |w| chat_settings::update_for_user(w.main().connection(), &uid, &[], &now))
        .await;
    if let Err(e) = created {
        return internal(e);
    }
    read_settings_response(db, user_id, true)
}

/// Re-read the settings row and (for a create) reproduce v4's create-RETURN shape.
fn read_settings_response(db: &Db, user_id: &str, created: bool) -> Response {
    let uid = user_id.to_string();
    match db.read_main(move |conn| chat_settings::find_by_user_id(conn, &uid)) {
        Ok(Some(mut settings)) => {
            if created {
                chat_settings::patch_create_return_shape(&mut settings);
            }
            Response::ChatSettings(settings)
        }
        Ok(None) => internal("chat settings write failed to persist"),
        Err(e) => internal(e),
    }
}

/// v4 `PUT /api/v1/settings/chat` (`updateChatSettings`): the ~27-field
/// field-guarded validation layer folding into one `updateForUser`. Returns the
/// updated settings row (v4's PUT echo). An invalid field → `bad-request` with
/// v4's message.
pub async fn chat_settings_update(db: &Db, user_id: &str, bag: &Value) -> Response {
    // Validate + collect the assignments (v4's `updateData`). Each JSON-object
    // field is deserialized into the ported typed struct (schema-ordered
    // serialize == v4's final `ChatSettingsSchema.parse` output). The template
    // existence check needs a read, so it runs before the write.
    let assignments = match build_settings_assignments(db, bag) {
        Ok(a) => a,
        Err(msg) => return bad_request(msg),
    };

    let now = crate::clock::now_iso();
    let uid = user_id.to_string();
    let created = db
        .write(move |w| {
            chat_settings::update_for_user(w.main().connection(), &uid, &assignments, &now)
        })
        .await;
    let created = match created {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    read_settings_response(db, user_id, created)
}

/// Serialize a JSON-object settings field into schema-ordered compact JSON by
/// round-tripping it through the ported typed struct `T` (the struct's field
/// order == v4's schema order == v4's `ChatSettingsSchema.parse` output). A
/// deserialize failure yields `Invalid <label>` (→ bad-request; the exact Zod
/// throw message is a documented seam — the corpus sends fully-specified objects).
fn json_field<T>(label: &str, v: &Value) -> Result<chat_settings::SettingsColVal, String>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let parsed: T = serde_json::from_value(v.clone()).map_err(|_| format!("Invalid {label}"))?;
    let text = serde_json::to_string(&parsed).map_err(|_| format!("Invalid {label}"))?;
    Ok(chat_settings::SettingsColVal::Text(text))
}

/// v4 `CheapLLMSettingsSchema.parse` over a PUT sub-bag (the base repo's
/// merge-then-`validate` runs the FULL nested schema, so Zod defaults
/// materialize on a partial input): `strategy` defaults `PROVIDER_CHEAPEST`,
/// `fallbackToLocal` `true`, `embeddingProvider` `'OPENAI'`; the three
/// `.nullable().optional()` ids keep a present `null` and are OMITTED when
/// absent; unknown keys are stripped; output in schema field order. The UUID
/// format check on the ids is a documented seam (the corpus sends valid ids);
/// a type mismatch is `Invalid cheap LLM settings` (the Zod-throw-message seam).
fn zod_cheap_llm_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    let err = || "Invalid cheap LLM settings".to_string();
    let o = v.as_object().ok_or_else(err)?;
    let mut out = Map::new();
    let strategy = match o.get("strategy") {
        None => "PROVIDER_CHEAPEST",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return Err(err()),
    };
    out.insert("strategy".into(), json!(strategy));
    let opt_id = |key: &str, out: &mut Map<String, Value>| -> Result<(), String> {
        match o.get(key) {
            None => Ok(()),
            Some(Value::Null) => {
                out.insert(key.to_string(), Value::Null);
                Ok(())
            }
            Some(Value::String(s)) => {
                out.insert(key.to_string(), json!(s));
                Ok(())
            }
            Some(_) => Err(err()),
        }
    };
    opt_id("userDefinedProfileId", &mut out)?;
    opt_id("defaultCheapProfileId", &mut out)?;
    let fallback = match o.get("fallbackToLocal") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(err()),
    };
    out.insert("fallbackToLocal".into(), json!(fallback));
    let embedding = match o.get("embeddingProvider") {
        None => "OPENAI",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return Err(err()),
    };
    out.insert("embeddingProvider".into(), json!(embedding));
    opt_id("imagePromptProfileId", &mut out)?;
    serde_json::to_string(&Value::Object(out))
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| err())
}

/// v4 `ThemePreferenceSchema.parse` over a PUT sub-bag (route-level parse):
/// `activeThemeId` defaults `null` (ALWAYS present), `colorMode` `'system'`,
/// `showNavThemeSelector` `false`; `customOverrides` is `.optional()` with no
/// default (omitted when absent); unknown keys stripped; schema field order.
fn zod_theme_preference(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    let err = || "Invalid theme preference".to_string();
    let o = v.as_object().ok_or_else(err)?;
    let mut out = Map::new();
    let active = match o.get("activeThemeId") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) => json!(s),
        Some(_) => return Err(err()),
    };
    out.insert("activeThemeId".into(), active);
    let color_mode = match o.get("colorMode") {
        None => "system",
        Some(Value::String(s)) if matches!(s.as_str(), "light" | "dark" | "system") => s.as_str(),
        Some(_) => return Err(err()),
    };
    out.insert("colorMode".into(), json!(color_mode));
    match o.get("customOverrides") {
        None => {}
        Some(Value::Object(m)) => {
            if !m.values().all(Value::is_string) {
                return Err(err());
            }
            out.insert("customOverrides".into(), Value::Object(m.clone()));
        }
        Some(_) => return Err(err()),
    }
    let show = match o.get("showNavThemeSelector") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(err()),
    };
    out.insert("showNavThemeSelector".into(), json!(show));
    serde_json::to_string(&Value::Object(out))
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| err())
}

/// v4 `DangerousContentSettingsSchema.parse` over a PUT sub-bag (a route-level
/// parse, `settings/chat/route.ts` L165 — NOT the repo's merge-then-validate, so
/// the input bag stands alone and the Zod defaults materialize over whatever is
/// absent): `mode` defaults `'OFF'`, `threshold` `0.7`, `scanTextChat` `true`,
/// `scanImagePrompts` `true`, `scanImageGeneration` `false`, `displayMode`
/// `'SHOW'`, `showWarningBadges` `true`; the three `.nullable().optional()`
/// fields — `uncensoredTextProfileId`, `uncensoredImageProfileId`,
/// `customClassificationPrompt` — KEEP a present `null` and are OMITTED when
/// absent; unknown keys are stripped; output in schema field order.
///
/// `threshold` is stored as the INCOMING `Value` rather than round-tripped
/// through `f64`: v4 stringifies the parsed JS number, so an integral `1`
/// re-emits as `1` — a `f64` round-trip would write `1.0`. The `.min(0).max(1)`
/// bound is enforced here; the UUID format check on the two ids is the same
/// documented seam as {@link zod_cheap_llm_settings} (the corpus sends valid
/// ids), and a type mismatch collapses to `Invalid dangerous content settings`
/// (the Zod-throw-message seam — v4 surfaces the raw `ZodError` text).
fn zod_dangerous_content_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    let err = || "Invalid dangerous content settings".to_string();
    let o = v.as_object().ok_or_else(err)?;
    let mut out = Map::new();

    let enum_field =
        |key: &str, default: &'static str, allowed: &[&str]| -> Result<Value, String> {
            match o.get(key) {
                None => Ok(json!(default)),
                Some(Value::String(s)) if allowed.contains(&s.as_str()) => Ok(json!(s)),
                Some(_) => Err(err()),
            }
        };
    let bool_default = |key: &str, default: bool| -> Result<Value, String> {
        match o.get(key) {
            None => Ok(json!(default)),
            Some(Value::Bool(b)) => Ok(json!(b)),
            Some(_) => Err(err()),
        }
    };
    let opt_nullable = |key: &str, out: &mut Map<String, Value>| -> Result<(), String> {
        match o.get(key) {
            None => Ok(()),
            Some(Value::Null) => {
                out.insert(key.to_string(), Value::Null);
                Ok(())
            }
            Some(Value::String(s)) => {
                out.insert(key.to_string(), json!(s));
                Ok(())
            }
            Some(_) => Err(err()),
        }
    };

    out.insert(
        "mode".into(),
        enum_field("mode", "OFF", &["OFF", "DETECT_ONLY", "AUTO_ROUTE"])?,
    );
    let threshold = match o.get("threshold") {
        None => json!(0.7),
        Some(n @ Value::Number(num)) => {
            let f = num.as_f64().ok_or_else(err)?;
            if !(0.0..=1.0).contains(&f) {
                return Err(err());
            }
            n.clone()
        }
        Some(_) => return Err(err()),
    };
    out.insert("threshold".into(), threshold);
    out.insert("scanTextChat".into(), bool_default("scanTextChat", true)?);
    out.insert(
        "scanImagePrompts".into(),
        bool_default("scanImagePrompts", true)?,
    );
    out.insert(
        "scanImageGeneration".into(),
        bool_default("scanImageGeneration", false)?,
    );
    opt_nullable("uncensoredTextProfileId", &mut out)?;
    opt_nullable("uncensoredImageProfileId", &mut out)?;
    out.insert(
        "displayMode".into(),
        enum_field("displayMode", "SHOW", &["SHOW", "BLUR", "COLLAPSE"])?,
    );
    out.insert(
        "showWarningBadges".into(),
        bool_default("showWarningBadges", true)?,
    );
    opt_nullable("customClassificationPrompt", &mut out)?;

    serde_json::to_string(&Value::Object(out))
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| err())
}

/// Nullable-string column: `null` → SQL NULL, a string → TEXT, anything else →
/// error.
fn nullable_string(label: &str, v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    match v {
        Value::Null => Ok(chat_settings::SettingsColVal::Null),
        Value::String(s) => Ok(chat_settings::SettingsColVal::Text(s.clone())),
        _ => Err(format!("Invalid {label}")),
    }
}

/// Boolean column with v4's `Invalid <name> value (must be boolean)` message.
fn bool_field(v: &Value, err_msg: &str) -> Result<chat_settings::SettingsColVal, String> {
    match v.as_bool() {
        Some(b) => Ok(chat_settings::SettingsColVal::Int(i64::from(b))),
        None => Err(err_msg.to_string()),
    }
}

/// Build the validated `updateData` assignment list (v4 `updateChatSettings`).
/// Reproduces each field's manual guard + error message; the JSON-object fields
/// route through [`json_field`] (schema-ordered).
fn build_settings_assignments(
    db: &Db,
    bag: &Value,
) -> Result<Vec<(&'static str, chat_settings::SettingsColVal)>, String> {
    use chat_settings::SettingsColVal as Col;
    let obj = bag.as_object().cloned().unwrap_or_default();
    let mut out: Vec<(&'static str, Col)> = Vec::new();

    // avatarDisplayMode — truthy-gated enum (v4 `if (avatarDisplayMode)`).
    if let Some(v) = obj.get("avatarDisplayMode") {
        if let Some(m) = v.as_str().filter(|m| !m.is_empty()) {
            if !matches!(m, "ALWAYS" | "GROUP_ONLY" | "NEVER") {
                return Err("Invalid avatar display mode".to_string());
            }
            out.push(("avatarDisplayMode", Col::Text(m.to_string())));
        }
    }
    // avatarDisplayStyle — truthy-gated enum.
    if let Some(v) = obj.get("avatarDisplayStyle") {
        if let Some(st) = v.as_str().filter(|s| !s.is_empty()) {
            if !matches!(st, "CIRCULAR" | "RECTANGULAR") {
                return Err("Invalid avatar display style".to_string());
            }
            out.push(("avatarDisplayStyle", Col::Text(st.to_string())));
        }
    }
    if let Some(v) = obj.get("tagStyles") {
        // TagStyleMapSchema = record<string, TagVisualStyle>. The corpus keeps it
        // `{}` (the multi-key map + per-value key-order seam is deferred, the
        // `connection_profiles.parameters` precedent); an empty/single-key map
        // serializes byte-identically to v4's `JSON.stringify`.
        let text = serde_json::to_string(v).map_err(|_| "Invalid tag styles".to_string())?;
        out.push(("tagStyles", chat_settings::SettingsColVal::Text(text)));
    }
    if let Some(v) = obj.get("cheapLLMSettings") {
        // v4 manual enum guards, then schema-ordered store.
        if let Some(o) = v.as_object() {
            if let Some(st) = o.get("strategy").and_then(Value::as_str) {
                if !matches!(st, "USER_DEFINED" | "PROVIDER_CHEAPEST" | "LOCAL_FIRST") {
                    return Err("Invalid cheap LLM strategy".to_string());
                }
            }
            if let Some(ep) = o.get("embeddingProvider").and_then(Value::as_str) {
                if !matches!(ep, "SAME_PROVIDER" | "OPENAI" | "LOCAL") {
                    return Err("Invalid embedding provider".to_string());
                }
            }
        }
        out.push(("cheapLLMSettings", zod_cheap_llm_settings(v)?));
    }
    if let Some(v) = obj.get("imageDescriptionProfileId") {
        out.push((
            "imageDescriptionProfileId",
            nullable_string("imageDescriptionProfileId", v)?,
        ));
    }
    if let Some(v) = obj.get("uncensoredImageDescriptionProfileId") {
        out.push((
            "uncensoredImageDescriptionProfileId",
            nullable_string("uncensoredImageDescriptionProfileId", v)?,
        ));
    }
    if let Some(v) = obj.get("themePreference") {
        out.push(("themePreference", zod_theme_preference(v)?));
    }
    if let Some(v) = obj.get("defaultRoleplayTemplateId") {
        // Validate the template exists when setting a non-null value.
        match v {
            Value::Null => out.push(("defaultRoleplayTemplateId", Col::Null)),
            Value::String(id) => {
                let id_owned = id.clone();
                let exists = db
                    .read_main(move |conn| {
                        roleplay_templates::find_system_prompt_by_id(conn, &id_owned)
                    })
                    .map_err(|e| e.to_string())?
                    .is_some();
                if !exists {
                    return Err("Invalid roleplay template ID".to_string());
                }
                out.push(("defaultRoleplayTemplateId", Col::Text(id.clone())));
            }
            _ => return Err("Invalid roleplay template ID".to_string()),
        }
    }
    if let Some(v) = obj.get("sidebarWidth") {
        let n = v.as_f64().filter(|n| (256.0..=512.0).contains(n));
        match n {
            Some(n) => out.push(("sidebarWidth", Col::Int(n as i64))),
            None => return Err("Invalid sidebar width (must be 256-512)".to_string()),
        }
    }
    if let Some(v) = obj.get("tokenDisplaySettings") {
        out.push((
            "tokenDisplaySettings",
            json_field::<chat_settings::TokenDisplaySettings>("token display settings", v)?,
        ));
    }
    if let Some(v) = obj.get("memoryCascadePreferences") {
        if let Some(o) = v.as_object() {
            for key in ["onMessageDelete", "onSwipeRegenerate"] {
                if let Some(a) = o.get(key).and_then(Value::as_str) {
                    if !matches!(
                        a,
                        "DELETE_MEMORIES"
                            | "KEEP_MEMORIES"
                            | "ASK_EVERY_TIME"
                            | "REGENERATE_MEMORIES"
                    ) {
                        return Err(format!("Invalid memory cascade action for {key}"));
                    }
                }
            }
        }
        out.push((
            "memoryCascadePreferences",
            json_field::<chat_settings::MemoryCascadePreferences>("memory cascade preferences", v)?,
        ));
    }
    if let Some(v) = obj.get("llmLoggingSettings") {
        out.push((
            "llmLoggingSettings",
            json_field::<chat_settings::LlmLoggingSettings>("llm logging settings", v)?,
        ));
    }
    if let Some(v) = obj.get("autoDetectRng") {
        out.push((
            "autoDetectRng",
            bool_field(v, "Invalid autoDetectRng value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("customTools") {
        out.push((
            "customTools",
            bool_field(v, "Invalid customTools value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("agentModeSettings") {
        out.push((
            "agentModeSettings",
            json_field::<chat_settings::AgentModeSettings>("agent mode settings", v)?,
        ));
    }
    if let Some(v) = obj.get("storyBackgroundsSettings") {
        out.push((
            "storyBackgroundsSettings",
            json_field::<chat_settings::StoryBackgroundsSettings>("story backgrounds settings", v)?,
        ));
    }
    if let Some(v) = obj.get("contextCompressionSettings") {
        if let Some(o) = v.as_object() {
            if o.get("enabled").and_then(Value::as_bool).is_none() {
                return Err(
                    "Invalid contextCompressionSettings.enabled (must be boolean)".to_string(),
                );
            }
            let ws = o.get("windowSize").and_then(Value::as_f64);
            if ws.map(|w| w < 1.0).unwrap_or(true) {
                return Err(
                    "Invalid contextCompressionSettings.windowSize (must be positive number)"
                        .to_string(),
                );
            }
        }
        out.push((
            "contextCompressionSettings",
            json_field::<chat_settings::ContextCompressionSettings>(
                "context compression settings",
                v,
            )?,
        ));
    }
    if let Some(v) = obj.get("dangerousContentSettings") {
        out.push((
            "dangerousContentSettings",
            zod_dangerous_content_settings(v)?,
        ));
    }
    if let Some(v) = obj.get("autoLockSettings") {
        out.push((
            "autoLockSettings",
            json_field::<chat_settings::AutoLockSettings>("auto lock settings", v)?,
        ));
    }
    if let Some(v) = obj.get("compositionModeDefault") {
        out.push((
            "compositionModeDefault",
            bool_field(v, "Invalid compositionModeDefault value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("composerSpellcheck") {
        out.push((
            "composerSpellcheck",
            bool_field(v, "Invalid composerSpellcheck value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("textReplacementsEnabled") {
        out.push((
            "textReplacementsEnabled",
            bool_field(v, "Invalid textReplacementsEnabled value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("autoScrollOnResponseComplete") {
        out.push((
            "autoScrollOnResponseComplete",
            bool_field(
                v,
                "Invalid autoScrollOnResponseComplete value (must be boolean)",
            )?,
        ));
    }
    if let Some(v) = obj.get("autonomousRoomSettings") {
        out.push((
            "autonomousRoomSettings",
            json_field::<chat_settings::AutonomousRoomSettings>("autonomous room settings", v)?,
        ));
    }
    if let Some(v) = obj.get("thinkingDisplay") {
        out.push((
            "thinkingDisplay",
            json_field::<chat_settings::ThinkingDisplaySettings>("thinking display", v)?,
        ));
    }
    if let Some(v) = obj.get("answerConfirmationSettings") {
        out.push((
            "answerConfirmationSettings",
            json_field::<chat_settings::AnswerConfirmationSettings>(
                "answer confirmation settings",
                v,
            )?,
        ));
    }
    Ok(out)
}

// ===========================================================================
// Connection profiles (v4 connection-profiles/route.ts + [id]/)
// ===========================================================================

/// v4 `GET /api/v1/connection-profiles` — the enriched list (`{profiles, count}`).
/// Each profile is enriched with `apiKey` + `tags`; the `imageCapable` filter and
/// the `sortIndex` asc → name `localeCompare` sort are applied. The
/// `sortByCharacter` branch is a tracked deferral (the Providers-tab MVP never
/// sends it).
pub fn connection_profile_list(db: &Db, user_id: &str, image_capable: bool) -> Response {
    let uid = user_id.to_string();
    let result = db.read_main(move |conn| {
        let profiles = connection_profiles::find_by_user_id(conn, &uid)?;
        let mut enriched = Vec::with_capacity(profiles.len());
        for p in &profiles {
            enriched.push(enrich_profile(conn, p, true)?);
        }
        Ok(enriched)
    });
    let mut enriched: Vec<Value> = match result {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if image_capable {
        enriched.retain(|p| {
            p.get("provider")
                .and_then(Value::as_str)
                .map(supports_image_generation)
                .unwrap_or(false)
        });
    }
    // sortIndex asc, then name localeCompare.
    enriched.sort_by(|a, b| {
        let ai = a.get("sortIndex").and_then(Value::as_f64).unwrap_or(0.0);
        let bi = b.get("sortIndex").and_then(Value::as_f64).unwrap_or(0.0);
        ai.partial_cmp(&bi)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                crate::collation::locale_compare(
                    a.get("name").and_then(Value::as_str).unwrap_or(""),
                    b.get("name").and_then(Value::as_str).unwrap_or(""),
                )
            })
    });
    let count = enriched.len();
    Response::ConnectionProfiles(json!({ "profiles": enriched, "count": count }))
}

/// v4 `POST /api/v1/connection-profiles` (`handleCreate`).
pub async fn connection_profile_create(db: &Db, user_id: &str, bag: &Value) -> Response {
    let get_str = |k: &str| bag.get(k).and_then(Value::as_str);
    let get_bool = |k: &str, d: bool| bag.get(k).and_then(Value::as_bool).unwrap_or(d);

    let transport = get_str("transport").unwrap_or("api").to_string();
    if transport != "api" && transport != "courier" {
        return bad_request("Transport must be \"api\" or \"courier\"");
    }
    let is_courier = transport == "courier";

    let provider = get_str("provider").unwrap_or("").to_string();
    let base_url = get_str("baseUrl").map(str::to_string);
    // resolvedSupportsImageUpload — the client field, else the static capability.
    let resolved_supports_image_upload =
        match bag.get("supportsImageUpload").and_then(Value::as_bool) {
            Some(b) => b,
            None => crate::files::attachment_support::supports_mime_type(&provider, "image/jpeg"),
        };

    // Required-field validation (v4 order).
    let name = get_str("name").unwrap_or("");
    if name.trim().is_empty() {
        return bad_request("Name is required");
    }
    if provider.trim().is_empty() {
        return bad_request("Provider is required");
    }
    let model_name = get_str("modelName").unwrap_or("");
    if model_name.trim().is_empty() {
        return bad_request("Model name is required");
    }

    // Name uniqueness per user.
    let uid = user_id.to_string();
    let existing = match db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let normalized = normalize_profile_name(name);
    if existing
        .iter()
        .any(|p| s(p, "name").map(|n| normalize_profile_name(&n)) == Some(normalized.clone()))
    {
        return bad_request(format!(
            "A connection profile named \"{}\" already exists",
            name.trim()
        ));
    }

    // apiKeyId provider-match (non-courier).
    let api_key_id = get_str("apiKeyId")
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let (Some(akid), false) = (&api_key_id, is_courier) {
        let akid_owned = akid.clone();
        let key = match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
            Ok(v) => v,
            Err(e) => return internal(e),
        };
        let Some(key) = key else {
            return not_found("API key");
        };
        if key.provider != provider {
            return bad_request("API key provider does not match profile provider");
        }
    }

    // modelClass validity.
    let model_class = get_str("modelClass")
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(mc) = &model_class {
        if !is_valid_model_class_name(mc) {
            return bad_request(format!("Invalid model class: {mc}"));
        }
    }

    // maxContext positive integer (number or numeric string).
    let max_context = match parse_max_context(bag.get("maxContext")) {
        Ok(v) => v,
        Err(msg) => return bad_request(msg),
    };

    // baseUrl requirement (non-courier).
    if !is_courier && requires_base_url(&provider) && base_url.as_deref().unwrap_or("").is_empty() {
        return bad_request(format!("Base URL is required for {provider}"));
    }

    let is_default = get_bool("isDefault", false);
    let now = crate::clock::now_iso();

    // Default-unset sweep + max sortIndex.
    let max_sort_index = existing
        .iter()
        .map(|p| p.get("sortIndex").and_then(Value::as_f64).unwrap_or(0.0))
        .fold(-1.0_f64, f64::max);
    if is_default {
        for p in &existing {
            if p.get("isDefault").and_then(Value::as_bool) == Some(true) {
                if let Some(pid) = s(p, "id") {
                    let (pid, now2) = (pid, now.clone());
                    if let Err(e) = db
                        .write(move |w| {
                            connection_profiles::ConnectionProfilesRepository::new(
                                w.main().connection(),
                            )
                            .update(
                                &pid,
                                &connection_profiles::CpUpdate {
                                    is_default: Some(false),
                                    updated_at: now2,
                                    ..Default::default()
                                },
                            )
                        })
                        .await
                    {
                        return internal(e);
                    }
                }
            }
        }
    }

    let pseudo_tool_mode = if is_courier {
        "auto".to_string()
    } else {
        match get_str("pseudoToolMode") {
            Some(m @ ("native" | "simple-json" | "text-block")) => m.to_string(),
            _ => "auto".to_string(),
        }
    };
    let parameters = bag.get("parameters").cloned().unwrap_or_else(|| json!({}));

    let data = connection_profiles::CpCreate {
        user_id: user_id.to_string(),
        name: name.trim().to_string(),
        provider: provider.clone(),
        transport: transport.clone(),
        courier_delta_mode: if is_courier {
            get_bool("courierDeltaMode", true)
        } else {
            true
        },
        api_key_id: if is_courier { None } else { api_key_id },
        base_url: if is_courier {
            None
        } else {
            base_url.filter(|s| !s.is_empty())
        },
        model_name: model_name.trim().to_string(),
        parameters,
        is_default,
        is_cheap: get_bool("isCheap", false),
        allow_web_search: if is_courier {
            false
        } else {
            get_bool("allowWebSearch", false)
        },
        use_native_web_search: if is_courier {
            false
        } else {
            get_bool("useNativeWebSearch", false)
        },
        allow_tool_use: if is_courier {
            false
        } else {
            get_bool("allowToolUse", true)
        },
        pseudo_tool_mode,
        model_class,
        max_context,
        max_tokens: None,
        is_dangerous_compatible: if is_courier {
            false
        } else {
            get_bool("isDangerousCompatible", false)
        },
        supports_image_upload: if is_courier {
            false
        } else {
            resolved_supports_image_upload
        },
        tags: vec![],
        sort_index: max_sort_index + 1.0,
        total_tokens: 0.0,
        total_prompt_tokens: 0.0,
        total_completion_tokens: 0.0,
        message_count: 0.0,
    };
    let id = uuid::Uuid::new_v4().to_string();
    let opts = connection_profiles::CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    let (data2, opts2) = (data, opts);
    if let Err(e) = db
        .write(move |w| {
            connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                .create(&data2, &opts2)
        })
        .await
    {
        return internal(e);
    }
    // Re-read + reproduce v4's create-RETURN shape + enrich with apiKey only
    // (v4 create response: `{...profile, apiKey}`).
    let id_owned = id.clone();
    let out = db.read_main(move |conn| {
        let profile = connection_profiles::find_by_id(conn, &id_owned)?
            .ok_or_else(|| DbError::Key("created profile vanished".into()))?;
        let shaped = connection_profiles::create_return_shape(&profile);
        enrich_profile(conn, &shaped, false)
    });
    match out {
        Ok(v) => Response::ConnectionProfile(json!({ "profile": v })),
        Err(e) => internal(e),
    }
}

/// v4 `maxContext` parse: number or numeric string → positive integer, else null.
/// Returns `Ok(None)` for absent/null/falsy-`0`; `Ok(Some(f64))` for a valid
/// positive integer; `Err(message)` for a non-positive / non-integer.
fn parse_max_context(v: Option<&Value>) -> Result<Option<f64>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f == 0.0 {
                return Ok(None);
            }
            if f.fract() != 0.0 || f <= 0.0 {
                return Err("maxContext must be a positive integer".to_string());
            }
            Ok(Some(f))
        }
        Some(Value::String(st)) => {
            if st.is_empty() {
                return Ok(None);
            }
            match st.parse::<i64>() {
                Ok(n) if n > 0 => Ok(Some(n as f64)),
                _ => Err("maxContext must be a positive integer".to_string()),
            }
        }
        _ => Err("maxContext must be a positive integer".to_string()),
    }
}

/// v4 `PUT /api/v1/connection-profiles/[id]`.
pub async fn connection_profile_update(db: &Db, user_id: &str, id: &str, bag: &Value) -> Response {
    use connection_profiles::CpUpdate;
    let id_owned = id.to_string();
    let existing = match db.read_main(move |conn| connection_profiles::find_by_id(conn, &id_owned))
    {
        Ok(Some(p)) => p,
        Ok(None) => return not_found("Connection profile"),
        Err(e) => return internal(e),
    };

    // v4's `updateData` is applied unconditionally (an empty patch still bumps
    // `updatedAt` via `_update`), so no "has a change" gate is needed.
    let mut patch = CpUpdate::default();

    // transport.
    if let Some(v) = bag.get("transport") {
        let t = v.as_str().unwrap_or("");
        if t != "api" && t != "courier" {
            return bad_request("Transport must be \"api\" or \"courier\"");
        }
        patch.transport = Some(t.to_string());
    }
    if let Some(v) = bag.get("courierDeltaMode") {
        let Some(b) = v.as_bool() else {
            return bad_request("courierDeltaMode must be a boolean");
        };
        patch.courier_delta_mode = Some(b);
    }

    // Effective transport → courier gating.
    let effective_transport = patch
        .transport
        .clone()
        .or_else(|| s(&existing, "transport"))
        .unwrap_or_else(|| "api".to_string());
    let is_courier = effective_transport == "courier";
    if is_courier {
        patch.api_key_id = Some(None);
        patch.clear_base_url = true;
        patch.allow_tool_use = Some(false);
        patch.allow_web_search = Some(false);
        patch.use_native_web_search = Some(false);
        patch.supports_image_upload = Some(false);
        patch.is_dangerous_compatible = Some(false);
    }

    if let Some(v) = bag.get("name") {
        let name = v.as_str().unwrap_or("");
        if name.trim().is_empty() {
            return bad_request("Name must be a non-empty string");
        }
        let normalized = normalize_profile_name(name);
        let uid = user_id.to_string();
        let profiles =
            match db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid)) {
                Ok(v) => v,
                Err(e) => return internal(e),
            };
        if profiles.iter().any(|p| {
            s(p, "id").as_deref() != Some(id)
                && s(p, "name").map(|n| normalize_profile_name(&n)) == Some(normalized.clone())
        }) {
            return bad_request(format!(
                "A connection profile named \"{}\" already exists",
                name.trim()
            ));
        }
        patch.name = Some(name.trim().to_string());
    }

    if let Some(v) = bag.get("provider") {
        let p = v.as_str().unwrap_or("");
        if p.trim().is_empty() {
            return bad_request("Provider must be a non-empty string");
        }
        patch.provider = Some(p.to_string());
    }

    // apiKeyId (non-courier).
    if !is_courier {
        if let Some(v) = bag.get("apiKeyId") {
            if v.is_null() {
                patch.api_key_id = Some(None);
            } else if let Some(akid) = v.as_str() {
                let akid_owned = akid.to_string();
                let key = match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
                    Ok(v) => v,
                    Err(e) => return internal(e),
                };
                let Some(key) = key else {
                    return not_found("API key");
                };
                let provider_to_check = patch
                    .provider
                    .clone()
                    .or_else(|| s(&existing, "provider"))
                    .unwrap_or_default();
                if key.provider != provider_to_check {
                    return bad_request("API key provider does not match profile provider");
                }
                patch.api_key_id = Some(Some(akid.to_string()));
            }
        }
    }

    // baseUrl (non-courier): `baseUrl || null`.
    if !is_courier {
        if let Some(v) = bag.get("baseUrl") {
            match v.as_str().filter(|s| !s.is_empty()) {
                Some(b) => patch.base_url = Some(b.to_string()),
                None => patch.clear_base_url = true,
            }
        }
    }

    if let Some(v) = bag.get("modelName") {
        let m = v.as_str().unwrap_or("");
        if m.trim().is_empty() {
            return bad_request("Model name must be a non-empty string");
        }
        patch.model_name = Some(m.trim().to_string());
    }

    if let Some(v) = bag.get("parameters") {
        if !v.is_object() {
            return bad_request("Parameters must be an object");
        }
        patch.parameters = Some(v.clone());
    }

    if let Some(v) = bag.get("isDefault") {
        let Some(b) = v.as_bool() else {
            return bad_request("isDefault must be a boolean");
        };
        if b {
            // Unset other defaults.
            let uid = user_id.to_string();
            let all =
                match db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid)) {
                    Ok(v) => v,
                    Err(e) => return internal(e),
                };
            for p in &all {
                if p.get("isDefault").and_then(Value::as_bool) == Some(true)
                    && s(p, "id").as_deref() != Some(id)
                {
                    if let Some(pid) = s(p, "id") {
                        let (pid, now2) = (pid, crate::clock::now_iso());
                        if let Err(e) = db
                            .write(move |w| {
                                connection_profiles::ConnectionProfilesRepository::new(
                                    w.main().connection(),
                                )
                                .update(
                                    &pid,
                                    &CpUpdate {
                                        is_default: Some(false),
                                        updated_at: now2,
                                        ..Default::default()
                                    },
                                )
                            })
                            .await
                        {
                            return internal(e);
                        }
                    }
                }
            }
        }
        patch.is_default = Some(b);
    }

    if let Some(v) = bag.get("isCheap") {
        let Some(b) = v.as_bool() else {
            return bad_request("isCheap must be a boolean");
        };
        patch.is_cheap = Some(b);
    }
    if !is_courier {
        for (key, err) in [
            (
                "isDangerousCompatible",
                "isDangerousCompatible must be a boolean",
            ),
            ("allowWebSearch", "allowWebSearch must be a boolean"),
            ("useNativeWebSearch", "useNativeWebSearch must be a boolean"),
            ("allowToolUse", "allowToolUse must be a boolean"),
        ] {
            if let Some(v) = bag.get(key) {
                let Some(b) = v.as_bool() else {
                    return bad_request(err);
                };
                match key {
                    "isDangerousCompatible" => patch.is_dangerous_compatible = Some(b),
                    "allowWebSearch" => patch.allow_web_search = Some(b),
                    "useNativeWebSearch" => patch.use_native_web_search = Some(b),
                    "allowToolUse" => patch.allow_tool_use = Some(b),
                    _ => unreachable!(),
                }
            }
        }
        if let Some(v) = bag.get("pseudoToolMode") {
            let m = v.as_str().unwrap_or("");
            if !matches!(m, "auto" | "native" | "simple-json" | "text-block") {
                return bad_request(
                    "pseudoToolMode must be one of auto, native, simple-json, text-block",
                );
            }
            patch.pseudo_tool_mode = Some(m.to_string());
        }
    }

    if let Some(v) = bag.get("modelClass") {
        if v.is_null() || v.as_str() == Some("") {
            patch.clear_model_class = true;
        } else {
            let mc = v.as_str().unwrap_or("");
            if !is_valid_model_class_name(mc) {
                return bad_request(format!("Invalid model class: {mc}"));
            }
            patch.model_class = Some(mc.to_string());
        }
    }

    if let Some(v) = bag.get("maxContext") {
        if v.is_null() || v.as_str() == Some("") || v.as_f64() == Some(0.0) {
            patch.clear_max_context = true;
        } else {
            match parse_max_context(Some(v)) {
                Ok(Some(n)) => patch.max_context = Some(n),
                Ok(None) => patch.clear_max_context = true,
                Err(msg) => return bad_request(msg),
            }
        }
    }

    if let Some(v) = bag.get("sortIndex") {
        let ok = v.as_f64().filter(|n| n.fract() == 0.0 && *n >= 0.0);
        match ok {
            Some(n) => patch.sort_index = Some(n),
            None => return bad_request("sortIndex must be a non-negative integer"),
        }
    }

    if !is_courier {
        if let Some(v) = bag.get("supportsImageUpload") {
            let Some(b) = v.as_bool() else {
                return bad_request("supportsImageUpload must be a boolean");
            };
            patch.supports_image_upload = Some(b);
        }
    }

    patch.updated_at = crate::clock::now_iso();
    let (cid, patch2) = (id.to_string(), patch);
    let updated = db
        .write(move |w| {
            connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                .update(&cid, &patch2)
        })
        .await;
    match updated {
        Ok(true) => {}
        Ok(false) => return internal("Failed to update connection profile"),
        Err(e) => return internal(e),
    }
    let id_owned = id.to_string();
    let out = db.read_main(move |conn| {
        let profile = connection_profiles::find_by_id(conn, &id_owned)?
            .ok_or_else(|| DbError::Key("updated profile vanished".into()))?;
        enrich_profile(conn, &profile, true)
    });
    match out {
        Ok(v) => Response::ConnectionProfile(json!({ "profile": v })),
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /api/v1/connection-profiles/[id]`. → ack.
pub async fn connection_profile_delete(db: &Db, id: &str) -> Response {
    let id_owned = id.to_string();
    match db.read_main(move |conn| connection_profiles::find_by_id(conn, &id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Connection profile"),
        Err(e) => return internal(e),
    }
    let cid = id.to_string();
    match db
        .write(move |w| {
            connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                .delete(&cid)
        })
        .await
    {
        Ok(_) => Response::Ack(super::types::AckDto::default()),
        Err(e) => internal(e),
    }
}

/// v4 `?action=reorder` — the contract sends `orderedIds`; each id's `sortIndex`
/// becomes its position. Verifies ownership, then bulk-updates. → ack.
pub async fn connection_profile_reorder(
    db: &Db,
    user_id: &str,
    ordered_ids: &[String],
) -> Response {
    let uid = user_id.to_string();
    let profiles = match db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let owned: std::collections::HashSet<String> =
        profiles.iter().filter_map(|p| s(p, "id")).collect();
    for id in ordered_ids {
        if !owned.contains(id) {
            return not_found("Connection profile");
        }
    }
    for (i, id) in ordered_ids.iter().enumerate() {
        let (pid, idx, now) = (id.clone(), i as f64, crate::clock::now_iso());
        if let Err(e) = db
            .write(move |w| {
                connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                    .update(
                        &pid,
                        &connection_profiles::CpUpdate {
                            sort_index: Some(idx),
                            updated_at: now,
                            ..Default::default()
                        },
                    )
            })
            .await
        {
            return internal(e);
        }
    }
    Response::Ack(super::types::AckDto::default())
}

/// v4 `?action=reset-sort` — default first, then non-cheap alpha, then cheap
/// alpha. Bulk-updates `sortIndex`. → ack.
pub async fn connection_profile_reset_sort(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let profiles = match db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let is_default = |p: &Value| p.get("isDefault").and_then(Value::as_bool) == Some(true);
    let is_cheap = |p: &Value| p.get("isCheap").and_then(Value::as_bool) == Some(true);
    let name = |p: &Value| s(p, "name").unwrap_or_default();

    let mut ordered: Vec<String> = Vec::new();
    if let Some(d) = profiles.iter().find(|p| is_default(p)) {
        if let Some(id) = s(d, "id") {
            ordered.push(id);
        }
    }
    let mut regular: Vec<&Value> = profiles
        .iter()
        .filter(|p| !is_default(p) && !is_cheap(p))
        .collect();
    regular.sort_by(|a, b| crate::collation::locale_compare(&name(a), &name(b)));
    for p in regular {
        if let Some(id) = s(p, "id") {
            ordered.push(id);
        }
    }
    let mut cheap: Vec<&Value> = profiles
        .iter()
        .filter(|p| !is_default(p) && is_cheap(p))
        .collect();
    cheap.sort_by(|a, b| crate::collation::locale_compare(&name(a), &name(b)));
    for p in cheap {
        if let Some(id) = s(p, "id") {
            ordered.push(id);
        }
    }

    for (i, id) in ordered.iter().enumerate() {
        let (pid, idx, now) = (id.clone(), i as f64, crate::clock::now_iso());
        if let Err(e) = db
            .write(move |w| {
                connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                    .update(
                        &pid,
                        &connection_profiles::CpUpdate {
                            sort_index: Some(idx),
                            updated_at: now,
                            ..Default::default()
                        },
                    )
            })
            .await
        {
            return internal(e);
        }
    }
    Response::Ack(super::types::AckDto::default())
}

// ===========================================================================
// Providers listing (v4 providers/route.ts)
// ===========================================================================

/// v4 `GET /api/v1/providers` — the plugin-registry listing becomes a read over
/// the v5 manifest [`Registry`]. Emits the LLM providers only (no search-provider
/// manifest is ported — a documented absence); `icon`/`optionsSchema` are always
/// `null` (the manifest deliberately lacks them, per the W4.7a survey), and
/// `type` is always `"llm"`.
pub fn provider_list() -> Response {
    let registry = Registry::built_in();
    let providers: Vec<Value> = registry
        .all_providers()
        .iter()
        .map(|m| {
            let req = &m.config_requirements;
            let mut config = Map::new();
            config.insert("requiresApiKey".into(), json!(req.requires_api_key));
            config.insert("requiresBaseUrl".into(), json!(req.requires_base_url));
            if let Some(l) = &req.api_key_label {
                config.insert("apiKeyLabel".into(), json!(l));
            }
            if let Some(l) = &req.base_url_label {
                config.insert("baseUrlLabel".into(), json!(l));
            }
            if let Some(l) = &req.base_url_placeholder {
                config.insert("baseUrlPlaceholder".into(), json!(l));
            }
            if let Some(l) = &req.base_url_default {
                config.insert("baseUrlDefault".into(), json!(l));
            }
            json!({
                "id": m.id,
                "name": m.id,
                "displayName": m.display_name,
                "description": m.description,
                "abbreviation": m.abbreviation,
                "colors": { "bg": m.colors.bg, "text": m.colors.text, "icon": m.colors.icon },
                "icon": Value::Null,
                "type": "llm",
                "capabilities": {
                    "chat": m.capabilities.chat,
                    "imageGeneration": m.capabilities.image_generation,
                    "embeddings": m.capabilities.embeddings,
                    "webSearch": m.capabilities.web_search,
                    "toolUse": m.capabilities.tool_use,
                },
                "configRequirements": Value::Object(config),
                "optionsSchema": Value::Null,
            })
        })
        .collect();
    let count = providers.len();
    Response::Providers(json!({ "providers": providers, "count": count }))
}

// ===========================================================================
// Models read / fetch (v4 models/route.ts)
// ===========================================================================

/// v4 `GET /api/v1/models` (+ `?provider=`) — the cached models read.
pub fn model_list(db: &Db, provider: Option<&str>) -> Response {
    let provider_owned = provider.map(str::to_string);
    let result = db.read_main(move |conn| match &provider_owned {
        Some(p) => provider_models::find_by_provider(conn, p),
        None => provider_models::find_all(conn),
    });
    let models = match result {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let count = models.len();
    Response::Models(json!({
        "models": models,
        "count": count,
        "filters": { "provider": provider, "hasVision": false, "hasStreaming": false },
        "cached": true,
    }))
}

/// The models-fetch seam: v4's `getAvailableModels` + the plugin metadata/static
/// merge (host-side plugin internals). Returns `(models, modelsWithInfo)` — the
/// id list and the enriched rows the route caches + echoes. The differential cans
/// this on both sides (records v4's output); the CACHE it produces through the
/// ported [`provider_models::ProviderModelsRepository::upsert_model`] is the
/// load-bearing verified effect.
pub trait ModelsFetcher {
    fn fetch(
        &self,
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<(Vec<String>, Vec<Value>), String>;
}

/// v4 `POST /api/v1/models` — live fetch + cache. Validates baseUrl/apiKey
/// requirements, fetches via the injected [`ModelsFetcher`], caches each enriched
/// row through `upsertModel` (per-row error-swallowed, matching v4), and echoes
/// `{provider, models, modelsWithInfo, count}`.
pub async fn model_fetch<F: ModelsFetcher>(
    db: &Db,
    user_id: &str,
    provider: &str,
    api_key_id: Option<&str>,
    base_url: Option<&str>,
    fetcher: &F,
) -> Response {
    if provider.trim().is_empty() {
        return bad_request("Provider is required");
    }
    let mut decrypted_key = String::new();
    if let Some(akid) = api_key_id.filter(|s| !s.is_empty()) {
        let (akid, uid) = (akid.to_string(), user_id.to_string());
        let key =
            match db.read_main(move |conn| api_keys::find_by_id_and_user_id(conn, &akid, &uid)) {
                Ok(v) => v,
                Err(e) => return internal(e),
            };
        let Some(key) = key else {
            return not_found("API key not found");
        };
        decrypted_key = key.key_value;
    }
    if requires_base_url(provider) && base_url.map(|b| b.is_empty()).unwrap_or(true) {
        return bad_request(format!("Base URL is required for {provider} provider"));
    }
    if requires_api_key(provider) && decrypted_key.is_empty() {
        return bad_request(format!("API key is required for {provider} provider"));
    }

    let (models, models_with_info) = match fetcher.fetch(provider, &decrypted_key, base_url) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };

    let cache_rows: Vec<provider_models::PmCreate> = models_with_info
        .iter()
        .filter_map(|m| {
            let model_id = m.get("id").and_then(Value::as_str)?.to_string();
            let display_name = m.get("displayName").and_then(Value::as_str)?.to_string();
            Some(provider_models::PmCreate {
                provider: provider.to_string(),
                model_id,
                model_type: "chat".to_string(),
                display_name,
                base_url: base_url.map(str::to_string),
                context_window: m.get("contextWindow").and_then(Value::as_f64),
                max_output_tokens: m.get("maxOutputTokens").and_then(Value::as_f64),
                deprecated: m
                    .get("deprecated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                experimental: m
                    .get("experimental")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    if !cache_rows.is_empty() {
        let _ = db
            .write(move |w| {
                let repo = provider_models::ProviderModelsRepository::new(w.main().connection());
                for row in &cache_rows {
                    let _ = repo.upsert_model(row);
                }
                Ok::<(), DbError>(())
            })
            .await;
    }

    let count = models.len();
    Response::Models(json!({
        "provider": provider,
        "models": models,
        "modelsWithInfo": models_with_info,
        "count": count,
    }))
}

// ===========================================================================
// The connection-validation wire seam (v4 validateApiKey / testProviderConnection)
// ===========================================================================

/// The boolean/error outcome of v4's `providerRegistry.validateApiKey` (the
/// per-provider validate WIRE is v4 plugin internals, NOT ported); the differential
/// pins the boolean. `Ok(true)`/`Ok(false)` = validated / not; `Err` = a thrown
/// error (its message surfaces).
pub trait ConnectionValidator {
    fn validate(
        &self,
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<bool, String>;
}

/// v4 `?action=test-connection` (`handleTestConnection`): `validateProviderConfig`
/// (pure) → the injected validator. `apiKeyId` (unscoped) resolves the key; absent
/// → empty.
pub fn connection_test<V: ConnectionValidator>(
    db: &Db,
    provider: &str,
    api_key_id: Option<&str>,
    base_url: Option<&str>,
    validator: &V,
) -> Response {
    let mut decrypted_key = String::new();
    if let Some(akid) = api_key_id.filter(|s| !s.is_empty()) {
        let akid_owned = akid.to_string();
        let key = match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
            Ok(v) => v,
            Err(e) => return internal(e),
        };
        let Some(key) = key else {
            return not_found("API key");
        };
        decrypted_key = key.key_value;
    }
    let errors = validate_provider_config(provider, &decrypted_key, base_url);
    if let Some(first) = errors.first() {
        return Response::ConnectionTest(json!({
            "valid": false, "provider": provider, "error": first,
        }));
    }
    let registry = Registry::built_in();
    if !registry.has_provider(provider) {
        return Response::ConnectionTest(json!({
            "valid": false, "provider": provider,
            "error": format!("Provider '{provider}' not found"),
        }));
    }
    match validator.validate(provider, &decrypted_key, base_url) {
        Ok(true) => Response::ConnectionTest(json!({
            "valid": true, "provider": provider,
            "message": format!("Successfully connected to {provider}"),
        })),
        Ok(false) => {
            let display = registry
                .get_provider(provider)
                .map(|m| m.display_name.clone())
                .unwrap_or_else(|| provider.to_string());
            Response::ConnectionTest(json!({
                "valid": false, "provider": provider,
                "error": format!("Failed to validate connection to {display}"),
            }))
        }
        Err(msg) => Response::ConnectionTest(json!({
            "valid": false, "provider": provider, "error": msg,
        })),
    }
}

/// v4 `?action=test-message` (`handleTestMessage`): `validateProviderConfig` (pure)
/// → a single completion over the injected [`CompletionProvider`] seam. The
/// `top_p` param is not threaded (the completion seam's canned key omits it — a
/// documented seam).
pub async fn connection_test_message<C>(
    db: &Db,
    provider: &str,
    api_key_id: Option<&str>,
    base_url: Option<&str>,
    model_name: &str,
    parameters: &Value,
    completion: &C,
) -> Response
where
    C: crate::model::completion::CompletionProvider,
{
    use crate::model::completion::{CompletionMessage, CompletionParams};
    let mut decrypted_key = String::new();
    if let Some(akid) = api_key_id.filter(|s| !s.is_empty()) {
        let akid_owned = akid.to_string();
        let key = match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
            Ok(v) => v,
            Err(e) => return internal(e),
        };
        let Some(key) = key else {
            return not_found("API key");
        };
        decrypted_key = key.key_value;
    }
    let errors = validate_provider_config(provider, &decrypted_key, base_url);
    if let Some(first) = errors.first() {
        return bad_request(first.clone());
    }
    let temperature = parameters.get("temperature").and_then(Value::as_f64);
    let max_tokens = parameters
        .get("max_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(50);
    let params = CompletionParams {
        messages: vec![CompletionMessage::user(
            "Hello! Please respond with a brief greeting to confirm the connection is working.",
        )],
        model: model_name.to_string(),
        temperature,
        max_tokens,
        strict_max_tokens: false,
        cache_key: None,
        profile_parameters: None,
        attachments: vec![],
    };
    match completion.send_message(provider, base_url, &params).await {
        Err(e) => Response::ConnectionTest(json!({
            "success": false, "provider": provider, "error": e.message,
        })),
        Ok(resp) => {
            let content = resp.content;
            let preview: String = content.chars().take(100).collect();
            let truncated = content.chars().count() > 100;
            let suffix = if truncated { "..." } else { "" };
            let message = if preview.is_empty() {
                "Test message successful! Model responded but returned empty content.".to_string()
            } else {
                format!("Test message successful! Model responded: \"{preview}{suffix}\"")
            };
            let response_preview: String = content.chars().take(200).collect();
            Response::ConnectionTest(json!({
                "success": true, "provider": provider, "modelName": model_name,
                "message": message, "responsePreview": response_preview,
            }))
        }
    }
}

// ===========================================================================
// API keys (v4 api-keys/route.ts + [id]/)
// ===========================================================================

/// The masked API-key projection: `{id, provider, label, isActive, lastUsed?,
/// createdAt, updatedAt, keyPreview?}`. `lastUsed` is OMITTED when NULL (v4's
/// `undefined` dropped). `keyPreview` = `maskApiKey(key.substring(0,32))`.
fn masked_api_key(k: &api_keys::ApiKey, with_preview: bool) -> Value {
    let mut obj = Map::new();
    obj.insert("id".into(), json!(k.id));
    obj.insert("provider".into(), json!(k.provider));
    obj.insert("label".into(), json!(k.label));
    obj.insert("isActive".into(), json!(k.is_active));
    if let Some(lu) = &k.last_used {
        obj.insert("lastUsed".into(), json!(lu));
    }
    obj.insert("createdAt".into(), json!(k.created_at));
    obj.insert("updatedAt".into(), json!(k.updated_at));
    if with_preview {
        obj.insert(
            "keyPreview".into(),
            json!(mask_api_key_preview(&k.key_value)),
        );
    }
    Value::Object(obj)
}

/// v4 `GET /api/v1/api-keys` — the masked list, newest-first (`{apiKeys, count}`).
pub fn api_key_list(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let mut keys = match db.read_main(move |conn| api_keys::get_api_keys_by_user_id(conn, &uid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    keys.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let masked: Vec<Value> = keys.iter().map(|k| masked_api_key(k, true)).collect();
    let count = masked.len();
    Response::ApiKeys(json!({ "apiKeys": masked, "count": count }))
}

/// v4 `POST /api/v1/api-keys` (`handleCreate`). The create-time `autoAssociateApiKeys`
/// is a **tracked deferral** (it composes over connection profiles — the wizard
/// MVP does not need auto-association); `associations` is returned faithfully-
/// shaped as `[]`.
pub async fn api_key_create(
    db: &Db,
    user_id: &str,
    provider: &str,
    label: &str,
    api_key: &str,
) -> Response {
    if provider.trim().is_empty() {
        return bad_request("Invalid provider");
    }
    if label.trim().is_empty() {
        return bad_request("Label is required");
    }
    if api_key.trim().is_empty() {
        return bad_request("API key is required");
    }
    let data = api_keys::AkCreate {
        user_id: user_id.to_string(),
        label: label.trim().to_string(),
        provider: provider.to_string(),
        key_value: api_key.to_string(),
        is_active: Some(true),
        last_used: None,
    };
    let created = db
        .write(move |w| api_keys::ApiKeysRepository::new(w.main().connection()).create(&data))
        .await;
    let key = match created {
        Ok(k) => k,
        Err(e) => return internal(e),
    };
    Response::ApiKey(json!({
        "apiKey": {
            "id": key.id,
            "provider": key.provider,
            "label": key.label,
            "isActive": key.is_active,
            "createdAt": key.created_at,
            "updatedAt": key.updated_at,
            "associations": [],
        }
    }))
}

/// v4 `PUT /api/v1/api-keys/[id]`.
pub async fn api_key_update(
    db: &Db,
    id: &str,
    label: Option<&str>,
    is_active: Option<bool>,
    api_key: Option<&str>,
) -> Response {
    let id_owned = id.to_string();
    match db.read_main(move |conn| api_keys::find_by_id(conn, &id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("API key"),
        Err(e) => return internal(e),
    }
    let mut patch = api_keys::AkUpdate::default();
    if let Some(l) = label {
        if l.trim().is_empty() {
            return bad_request("Label must be a non-empty string");
        }
        patch.label = Some(l.trim().to_string());
    }
    if let Some(a) = is_active {
        patch.is_active = Some(a);
    }
    if let Some(k) = api_key {
        if k.trim().is_empty() {
            return bad_request("API key must be a non-empty string");
        }
        patch.key_value = Some(k.to_string());
    }
    let (cid, patch2) = (id.to_string(), patch);
    let updated = db
        .write(move |w| {
            api_keys::ApiKeysRepository::new(w.main().connection()).update(&cid, &patch2)
        })
        .await;
    match updated {
        Ok(Some(k)) => Response::ApiKey(json!({ "apiKey": masked_api_key(&k, false) })),
        Ok(None) => internal("Failed to update API key"),
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /api/v1/api-keys/[id]`. → ack.
pub async fn api_key_delete(db: &Db, id: &str) -> Response {
    let id_owned = id.to_string();
    match db.read_main(move |conn| api_keys::find_by_id(conn, &id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("API key"),
        Err(e) => return internal(e),
    }
    let cid = id.to_string();
    match db
        .write(move |w| api_keys::ApiKeysRepository::new(w.main().connection()).delete(&cid))
        .await
    {
        Ok(_) => Response::Ack(super::types::AckDto::default()),
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/api-keys/[id]?action=test` (`testProviderApiKey` + record-usage).
pub async fn api_key_test<V: ConnectionValidator>(
    db: &Db,
    user_id: &str,
    id: &str,
    base_url: Option<&str>,
    validator: &V,
) -> Response {
    let (id_owned, uid) = (id.to_string(), user_id.to_string());
    let key =
        match db.read_main(move |conn| api_keys::find_by_id_and_user_id(conn, &id_owned, &uid)) {
            Ok(Some(k)) => k,
            Ok(None) => return not_found("API key"),
            Err(e) => return internal(e),
        };
    match validator.validate(&key.provider, &key.key_value, base_url) {
        Ok(true) => {
            let cid = id.to_string();
            if let Err(e) = db
                .write(move |w| {
                    api_keys::ApiKeysRepository::new(w.main().connection()).record_usage(&cid)
                })
                .await
            {
                return internal(e);
            }
            Response::ApiKeyTest(json!({
                "valid": true, "provider": key.provider, "message": "API key is valid",
            }))
        }
        // v4 `testProviderApiKey`: a known provider whose `validateApiKey`
        // returns false yields `{valid:false}` (no error); the route drops the
        // undefined `error`. Unknown-provider / thrown → `Err` carries the
        // message.
        Ok(false) => Response::ApiKeyTest(json!({
            "valid": false, "provider": key.provider,
        })),
        Err(msg) => Response::ApiKeyTest(json!({
            "valid": false, "provider": key.provider, "error": msg,
        })),
    }
}

// === P4.d3 === (data-retention setting — the db-size-reduction drift)

/// v4 `GET /api/v1/settings/data-retention` — the instance-wide stale-chat
/// retention window `{staleChatDays}` (default 30 when unset). A read failure is
/// v4's `serverError('Failed to fetch…')`.
pub fn data_retention_settings_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_data_retention_settings) {
        Ok(days) => Response::DataRetention(json!({ "staleChatDays": days })),
        Err(e) => internal(e),
    }
}

/// v4 `PUT /api/v1/settings/data-retention` — merge `{...current, ...body}`,
/// `safeParse` (validate `staleChatDays` as an int in `[1, 3650]`), persist, and
/// echo the parsed settings. On a schema violation v4 returns `validationError`
/// (a 400 `{error: 'Validation error', details}`); the port surfaces the
/// `{error}` envelope (the Zod issue array is v4-implementation-specific).
pub async fn data_retention_settings_update(db: &Db, bag: Value) -> Response {
    let current = match db.read_main(instance_settings::get_data_retention_settings) {
        Ok(d) => d,
        Err(e) => return internal(e),
    };
    // The only schema field is `staleChatDays`; the merge overlays it when the
    // body carries it, else the current value survives.
    let days = match bag.get("staleChatDays") {
        None => current,
        Some(v) => match instance_settings::validate_stale_chat_days(v) {
            Some(d) => d,
            None => return Response::error(ErrorKind::BadRequest, "Validation error"),
        },
    };
    match db
        .write(move |w| instance_settings::set_data_retention_settings(w.main().connection(), days))
        .await
    {
        Ok(_) => Response::DataRetention(json!({ "staleChatDays": days })),
        Err(e) => internal(e),
    }
}

// === end P4.d3 ===

// ===========================================================================
// General state (P4.d10 §A — v4 `app/api/v1/settings/general-state/route.ts`
// at `f48f34dc`): the bottom cascade tier, a `state.json` document at the
// "Quilltap General" mount root — no entity row, hence bespoke.
// ===========================================================================

/// v4 general-state GET: `{ success, state }` (`readGeneralState` — always
/// fail-soft `{}`).
pub async fn general_state_get(db: &Db) -> Response {
    use crate::services::mount_index::general_state::read_general_state;
    let out = db
        .write(move |writers| {
            let mount = writers.mount_index().map(|w| w.connection());
            let main = writers.main().connection();
            Ok(read_general_state(main, mount))
        })
        .await;
    match out {
        Ok(state) => Response::State(json!({ "success": true, "state": state })),
        Err(e) => {
            eprintln!("[Settings v1] Error reading general state: {e}");
            Response::error(ErrorKind::Internal, "Failed to read general state")
        }
    }
}

/// v4 general-state PUT: `stateBodySchema` parse (failure → the 400
/// `Validation error`), then `writeGeneralState` (wholesale). Body
/// `{ success, state: parsed.state }` — the ECHOED body, not a read-back.
pub async fn general_state_set(db: &Db, state: Value) -> Response {
    use crate::services::mount_index::general_state::write_general_state;
    if !state.is_object() {
        return Response::error(ErrorKind::BadRequest, "Validation error");
    }
    let state_clone = state.clone();
    let out = db
        .write(move |writers| {
            let write = match writers.mount_index() {
                Some(mount_w) => {
                    let mount_c = mount_w.connection();
                    let main_c = writers.main().connection();
                    write_general_state(main_c, mount_c, &state_clone).map_err(|e| e.to_string())
                }
                // Degraded open == unprovisioned (v4's write throws).
                None => Err("Quilltap General mount has not been provisioned yet".to_string()),
            };
            Ok(write)
        })
        .await;
    match out {
        Ok(Ok(())) => Response::State(json!({ "success": true, "state": state })),
        // v4's catch → the fixed `serverError('Failed to update general state')`.
        Ok(Err(e)) => {
            eprintln!("[Settings v1] Error updating general state: {e}");
            Response::error(ErrorKind::Internal, "Failed to update general state")
        }
        Err(e) => {
            eprintln!("[Settings v1] Error updating general state: {e}");
            Response::error(ErrorKind::Internal, "Failed to update general state")
        }
    }
}

/// v4 general-state DELETE: read the previous, write `{}`. Body
/// `{ success, previousState }`.
pub async fn general_state_reset(db: &Db) -> Response {
    use crate::services::mount_index::general_state::{read_general_state, write_general_state};
    let out = db
        .write(move |writers| {
            let previous = {
                let mount = writers.mount_index().map(|w| w.connection());
                let main = writers.main().connection();
                read_general_state(main, mount)
            };
            let write = match writers.mount_index() {
                Some(mount_w) => {
                    let mount_c = mount_w.connection();
                    let main_c = writers.main().connection();
                    write_general_state(main_c, mount_c, &json!({})).map_err(|e| e.to_string())
                }
                None => Err("Quilltap General mount has not been provisioned yet".to_string()),
            };
            Ok((previous, write))
        })
        .await;
    match out {
        Ok((previous, Ok(()))) => {
            Response::State(json!({ "success": true, "previousState": previous }))
        }
        // v4's catch → the fixed `serverError('Failed to reset general state')`.
        Ok((_, Err(e))) => {
            eprintln!("[Settings v1] Error resetting general state: {e}");
            Response::error(ErrorKind::Internal, "Failed to reset general state")
        }
        Err(e) => {
            eprintln!("[Settings v1] Error resetting general state: {e}");
            Response::error(ErrorKind::Internal, "Failed to reset general state")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_shapes() {
        // < 12 chars → the 12-bullet literal.
        assert_eq!(mask_api_key("short"), "••••••••••••");
        assert_eq!(mask_api_key(""), "••••••••••••");
        // >= 12 → first8 + •••• + last4.
        assert_eq!(mask_api_key("sk-abcdefghijklmnop"), "sk-abcde••••mnop");
        // Preview truncates to 32 first: first 32 = "sk-0123456789ABCDEFGHIJKLMNOPQRS",
        // masked → first8 + •••• + last4 of that.
        let key = "sk-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 39 chars
        assert_eq!(mask_api_key_preview(key), "sk-01234••••PQRS");
    }

    #[test]
    fn normalize_and_model_class() {
        assert_eq!(normalize_profile_name("  My Profile "), "my profile");
        assert!(is_valid_model_class_name("Compact"));
        assert!(is_valid_model_class_name("Deep"));
        assert!(!is_valid_model_class_name("compact"));
        assert!(!is_valid_model_class_name("Nope"));
    }

    #[test]
    fn provider_config_validation() {
        // Unknown provider → single not-found error.
        assert_eq!(
            validate_provider_config("NOPE", "k", None),
            vec!["Provider 'NOPE' not found".to_string()]
        );
        // ANTHROPIC requires an API key (label from the manifest).
        let errs = validate_provider_config("ANTHROPIC", "", None);
        assert_eq!(
            errs,
            vec!["Anthropic API Key is required for ANTHROPIC".to_string()]
        );
        // A present key → no errors.
        assert!(validate_provider_config("ANTHROPIC", "sk-x", None).is_empty());
        // OLLAMA requires a base URL, not a key (manifest label, base-URL first).
        let errs = validate_provider_config("OLLAMA", "", None);
        assert_eq!(
            errs.first().map(String::as_str),
            Some("Ollama Base URL is required for OLLAMA")
        );
    }

    #[test]
    fn requires_flags_defaults() {
        // Unknown provider: requiresApiKey defaults true, requiresBaseUrl false.
        assert!(requires_api_key("NOPE"));
        assert!(!requires_base_url("NOPE"));
    }

    #[test]
    fn max_context_parsing() {
        assert_eq!(parse_max_context(None).unwrap(), None);
        assert_eq!(parse_max_context(Some(&json!(null))).unwrap(), None);
        assert_eq!(parse_max_context(Some(&json!(0))).unwrap(), None);
        assert_eq!(
            parse_max_context(Some(&json!(128000))).unwrap(),
            Some(128000.0)
        );
        assert_eq!(
            parse_max_context(Some(&json!("64000"))).unwrap(),
            Some(64000.0)
        );
        assert!(parse_max_context(Some(&json!(-5))).is_err());
        assert!(parse_max_context(Some(&json!(1.5))).is_err());
    }

    #[test]
    fn providers_listing_shape() {
        // The listing surfaces every built-in with the manifest-covered fields.
        match provider_list() {
            Response::Providers(v) => {
                let count = v.get("count").and_then(Value::as_u64).unwrap();
                let providers = v.get("providers").and_then(Value::as_array).unwrap();
                assert_eq!(count as usize, providers.len());
                assert!(providers.len() >= 9);
                let anthropic = providers
                    .iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some("ANTHROPIC"))
                    .expect("ANTHROPIC present");
                assert_eq!(anthropic.get("type").and_then(Value::as_str), Some("llm"));
                assert_eq!(anthropic.get("icon"), Some(&Value::Null));
                assert!(anthropic
                    .get("capabilities")
                    .and_then(|c| c.get("chat"))
                    .and_then(Value::as_bool)
                    .unwrap());
                assert!(anthropic
                    .get("configRequirements")
                    .and_then(|c| c.get("requiresApiKey"))
                    .is_some());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
