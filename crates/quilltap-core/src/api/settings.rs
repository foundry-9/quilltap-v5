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
use crate::provider_manifest::search::SearchManifest;
use crate::provider_manifest::{Capability, Registry};
use crate::services::profile_names::normalize_profile_name;

use super::chat_outfits::is_zod_uuid;
use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

// ===========================================================================
// The shared profile-patch field readers (P4.56)
//
// The three profile-update handlers — `connection_profile_update` here,
// `image_profiles::image_profile_update`, and
// `embedding_profiles::embedding_profile_update` — read `apiKeyId` and
// `baseUrl` with the same JS semantics and diverge only in what they DO with
// the answer (which patch struct, which lookup, which fixed 500 sentence). The
// DECISION lives here once; the three sites keep their own consequences.
// ===========================================================================

/// What a present `apiKeyId` on a profile PUT means.
///
/// v4 has no Zod schema on any of the three routes: the value falls straight
/// into `repos.connections.findApiKeyById(apiKeyId)`, whose `safeQuery` carries
/// a `null` fallback. So a present non-string answers `notFound('API key')` —
/// a number can only match an id literally spelled that way, and every Quilltap
/// id is a UUID; an object / array / boolean makes better-sqlite3's binder
/// throw, which the `null` fallback swallows. Measured on v4 for both `5` and
/// `{}` (P4.55 D1/D2/D3, the missing-`else` sub-family: v5 used to DROP a
/// present non-string silently and answer 200).
pub(crate) enum ApiKeyIdPatch<'a> {
    /// An explicit `null` — clear the column.
    Clear,
    /// A string — look it up, then set it.
    Set(&'a str),
    /// Anything else — v4's lookup misses, so `notFound('API key')`.
    Refuse,
}

pub(crate) fn classify_api_key_id(v: &Value) -> ApiKeyIdPatch<'_> {
    match v {
        Value::Null => ApiKeyIdPatch::Clear,
        Value::String(s) => ApiKeyIdPatch::Set(s),
        _ => ApiKeyIdPatch::Refuse,
    }
}

/// What a present `baseUrl` on a profile PUT means.
///
/// v4 writes `updateData.baseUrl = baseUrl || null` — JS FALSINESS, not a
/// string check (P4.55: v5's old `as_str()` filter collapsed every non-string
/// to null, so a truthy non-string silently CLEARED the column instead of
/// reaching v4's failure).
pub(crate) enum BaseUrlPatch<'a> {
    /// A non-empty string — assign it.
    Set(&'a str),
    /// The falsy arms `||` turns into null: `""`, `null`, `false`, `0`/`-0`.
    Clear,
    /// A TRUTHY non-string: v4 assigns it VERBATIM, the repository's in-memory
    /// merge validation then rejects the row, and the route's outer catch
    /// answers its own fixed 500. Measured (`{"baseUrl": 5}` → 500, the table
    /// untouched).
    ///
    /// RECORDED EDGE DIVERGENCE (§3 unification review, P4.55): v4's failure is
    /// TERMINAL but comes AFTER side effects — an earlier `isDefault` sweep's
    /// writes land first, so `{"baseUrl": 5, "isDefault": true}` clears the
    /// other profiles' defaults in v4 before the 500, where v5 refuses before
    /// any write. In v5's favor; unpinned, the input being hand-crafted-API-only.
    Refuse,
}

pub(crate) fn classify_base_url(v: &Value) -> BaseUrlPatch<'_> {
    match v {
        Value::String(s) if !s.is_empty() => BaseUrlPatch::Set(s),
        Value::String(_) | Value::Null | Value::Bool(false) => BaseUrlPatch::Clear,
        Value::Number(n) if n.as_f64() == Some(0.0) => BaseUrlPatch::Clear,
        _ => BaseUrlPatch::Refuse,
    }
}

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
///
/// The implementation moved to [`crate::services::api_key_service`] when v4's
/// bug-81 resolver landed beside it (this helper and the Brahma console's were
/// two copies of one v4 function); this stays as the settings-module spelling so
/// the call sites read the same as they always did.
pub(super) fn requires_api_key(provider: &str) -> bool {
    crate::services::api_key_service::provider_requires_api_key(provider)
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

/// v4 `enrichProfile`'s tag arm (`connection-profiles/[id]/route.ts:59-66`, the
/// same shape as `enrichWithTags`) → `[{tagId, tag}]`, preserving the profile's
/// own tag order and dropping unresolved ids. The nested `tag` is the **full**
/// marshaled v4 `Tag`.
///
/// P4.D85 closed the documented `{id,name}`-only seam this carried: the corpus
/// had no tagged profile (nothing could write one — there was no add-tag verb),
/// so both sides produced `[]` and the narrow shape measured nothing. The
/// settings fixture now bakes a tagged profile, and the nested object is the
/// full entity v4 sends. Distinct from the FLAT `EditorTag` that `get-tags`
/// answers — never conflate the two (v4 Bug 74's third layer).
fn enrich_with_tags(conn: &rusqlite::Connection, tag_ids: &[String]) -> Value {
    if tag_ids.is_empty() {
        return json!([]);
    }
    let tags = tags::find_full_by_ids(conn, tag_ids).unwrap_or_default();
    Value::Array(
        tags.into_iter()
            .map(
                |tag| json!({ "tagId": tag.get("id").cloned().unwrap_or(Value::Null), "tag": tag }),
            )
            .collect(),
    )
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
        // v4's catch splits the status on the MESSAGE (`route.ts:391`:
        // `errorMessage.includes('Invalid') ? 400 : 500`) — so a validation
        // failure whose ZodError text carries no "Invalid" anywhere (a
        // threshold-only `too_big`/`too_small`: "Too big: expected number to
        // be <=1") answers 500, not 400, with the same `{error}` body. Found
        // at the help-drift unification (P4.47 §3); pinned by the harness's
        // per-row `status` assert.
        Err(msg) => {
            return if msg.contains("Invalid") {
                bad_request(msg)
            } else {
                Response::error(ErrorKind::Internal, msg)
            };
        }
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

/// v4 `CheapLLMSettingsSchema` over a PUT sub-bag: `strategy` defaults
/// `PROVIDER_CHEAPEST`, `fallbackToLocal` `true`, `embeddingProvider` `'OPENAI'`;
/// the three `.nullable().optional()` ids keep a present `null` and are OMITTED
/// when absent; unknown keys are stripped; output in schema field order.
///
/// **The odd one out, and P4.47 (A) is where that stopped being invisible.**
/// v4's ROUTE does not parse this bag at all — it runs two manual enum guards
/// (`Invalid cheap LLM strategy` / `Invalid embedding provider`, reproduced at
/// the call site) and then stores the bag RAW. The Zod check that actually
/// governs it is the base repo's merge-then-`validate` over the WHOLE
/// ChatSettings object, which has two consequences the corpus now pins:
///
/// 1. Every issue `path` is PREFIXED with `cheapLLMSettings` (hence `PREFIX`).
/// 2. It throws AFTER every route-level arm, so a request carrying both a bad
///    cheap-LLM value and a bad `dangerousContentSettings` answers the
///    dangerous-content error. The caller defers this call to the end of the
///    assignment walk for exactly that reason.
///
/// The two enum arms ARE modelled, and the reason is a trap: v4's manual guards
/// are written `if (settings.strategy && !valid.includes(...))`, so they only
/// catch a TRUTHY non-member. A FALSY one — `null`, `""`, `0`, `false` — slips
/// past the guard, rides into `updateData`, and reaches the repo's Zod as an
/// `invalid_value` issue like any other miss.
fn zod_cheap_llm_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    const PREFIX: &[&str] = &["cheapLLMSettings"];
    const STRATEGIES: &[&str] = &["USER_DEFINED", "PROVIDER_CHEAPEST", "LOCAL_FIRST"];
    const EMBEDDING_PROVIDERS: &[&str] = &["SAME_PROVIDER", "OPENAI", "LOCAL"];
    let o = zod_object_or_issue(v, PREFIX)?;
    let mut out = Map::new();
    let mut issues: Vec<ZodIssue> = Vec::new();

    // Schema declaration order (which is also the issue order).
    let strategy = zod_enum(
        o,
        "strategy",
        "PROVIDER_CHEAPEST",
        STRATEGIES,
        PREFIX,
        &mut issues,
    );
    out.insert("strategy".into(), json!(strategy));
    zod_opt_uuid(o, "userDefinedProfileId", PREFIX, &mut out, &mut issues);
    zod_opt_uuid(o, "defaultCheapProfileId", PREFIX, &mut out, &mut issues);
    let fallback = zod_bool(o, "fallbackToLocal", true, PREFIX, &mut issues);
    out.insert("fallbackToLocal".into(), json!(fallback));
    let embedding = zod_enum(
        o,
        "embeddingProvider",
        "OPENAI",
        EMBEDDING_PROVIDERS,
        PREFIX,
        &mut issues,
    );
    out.insert("embeddingProvider".into(), json!(embedding));
    zod_opt_uuid(o, "imagePromptProfileId", PREFIX, &mut out, &mut issues);
    // v4 `65f5021c8` appended `allowCheapFallback` at the END of the schema,
    // so it lands last in the parsed key order too — which is what a fresh
    // instance's `cheapLLMSettings` DEFAULT and its seed row both carry.
    let allow_cheap_fallback = zod_bool(o, "allowCheapFallback", false, PREFIX, &mut issues);
    out.insert("allowCheapFallback".into(), json!(allow_cheap_fallback));

    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    serde_json::to_string(&Value::Object(out))
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| "Invalid cheap LLM settings".to_string())
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

// ---------------------------------------------------------------------------
// P4.D73 — `SmartTypographySettingsSchema.parse` at the route (v4 4.8.2
// `2d31810f`, `settings/chat/route.ts` L273).
// ---------------------------------------------------------------------------

/// One Zod issue, serialized in Zod's own key order — this is what
/// `JSON.stringify(err.issues, null, 2)` emits, and `ZodError.message` IS that
/// string. v4's route lets the throw escape to `getErrorMessage`, and the
/// `.includes('Invalid')` status test then turns it into a 400 whose body
/// carries the whole issue array verbatim. So the bytes here are contractual,
/// not an implementation detail.
///
/// Each code carries a DIFFERENT key set in a DIFFERENT order (measured against
/// v4's zod 4.4.3 and pinned by the `settings_zod` corpus family), so the
/// variants are untagged structs rather than one struct with optional keys —
/// `Option` skipping cannot reorder, and `invalid_value` puts `code` first
/// while `invalid_type` puts `expected` first.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum ZodIssue {
    InvalidType {
        expected: &'static str,
        code: &'static str,
        path: Vec<String>,
        message: String,
    },
    InvalidValue {
        code: &'static str,
        values: &'static [&'static str],
        path: Vec<String>,
        message: String,
    },
    TooBig {
        origin: &'static str,
        code: &'static str,
        maximum: Value,
        inclusive: bool,
        path: Vec<String>,
        message: String,
    },
    TooSmall {
        origin: &'static str,
        code: &'static str,
        minimum: Value,
        inclusive: bool,
        path: Vec<String>,
        message: String,
    },
    InvalidFormat {
        origin: &'static str,
        code: &'static str,
        format: &'static str,
        pattern: &'static str,
        path: Vec<String>,
        message: String,
    },
}

/// v4 zod's own `uuid()` source pattern, verbatim — it is a VALUE in the issue
/// body (stringified with the surrounding slashes), so it is transcribed, not
/// re-derived. Note the RFC nibbles: version `1-8`, variant `89abAB`, with the
/// nil and max UUIDs allowed as literal alternatives.
const ZOD_UUID_PATTERN: &str = "/^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$/";

/// `true` when `s` satisfies [`ZOD_UUID_PATTERN`]. Hand-matched rather than
/// regex-compiled: the shape is fixed and the crate has no regex dependency.
fn zod_uuid_ok(s: &str) -> bool {
    if s.eq_ignore_ascii_case("00000000-0000-0000-0000-000000000000")
        || s.eq_ignore_ascii_case("ffffffff-ffff-ffff-ffff-ffffffffffff")
    {
        // The literal alternatives are case-SENSITIVE in the pattern; the nil
        // form has no letters, and the max form is spelled lowercase.
        return s == "00000000-0000-0000-0000-000000000000"
            || s == "ffffffff-ffff-ffff-ffff-ffffffffffff";
    }
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    let hex = |i: usize| b[i].is_ascii_hexdigit();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !hex(i) {
                    return false;
                }
            }
        }
    }
    matches!(b[14], b'1'..=b'8') && matches!(b[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
}

impl ZodIssue {
    fn invalid_type(expected: &'static str, path: Vec<String>, got: Option<&Value>) -> Self {
        Self::InvalidType {
            expected,
            code: "invalid_type",
            path,
            message: format!(
                "Invalid input: expected {expected}, received {}",
                zod_parsed_type(got)
            ),
        }
    }

    /// A `z.enum([...])` miss. Zod 4 reports it as `invalid_value` and prints
    /// the options double-quoted and pipe-joined.
    fn invalid_value(values: &'static [&'static str], path: Vec<String>) -> Self {
        let rendered = values
            .iter()
            .map(|v| format!("\"{v}\""))
            .collect::<Vec<_>>()
            .join("|");
        Self::InvalidValue {
            code: "invalid_value",
            values,
            path,
            message: format!("Invalid option: expected one of {rendered}"),
        }
    }

    /// `.max(n)` on a number. `maximum` rides as the JSON number v4 declares
    /// (an integral bound prints as `1`, never `1.0`).
    fn too_big(maximum: Value, path: Vec<String>) -> Self {
        let message = format!("Too big: expected number to be <={maximum}");
        Self::TooBig {
            origin: "number",
            code: "too_big",
            maximum,
            inclusive: true,
            path,
            message,
        }
    }

    /// `.min(n)` on a number.
    fn too_small(minimum: Value, path: Vec<String>) -> Self {
        let message = format!("Too small: expected number to be >={minimum}");
        Self::TooSmall {
            origin: "number",
            code: "too_small",
            minimum,
            inclusive: true,
            path,
            message,
        }
    }

    /// A `z.string().uuid()` miss (the string type check has already passed).
    fn invalid_uuid(path: Vec<String>) -> Self {
        Self::InvalidFormat {
            origin: "string",
            code: "invalid_format",
            format: "uuid",
            pattern: ZOD_UUID_PATTERN,
            path,
            message: "Invalid UUID".to_string(),
        }
    }
}

/// v4 `util.parsedType` (the same table the Pascal Zod port pins). `None` is
/// JS `undefined` — a missing key.
fn zod_parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

/// `ZodError.message` for a set of issues: `JSON.stringify(issues, null, 2)`.
/// `serde_json::to_string_pretty` uses the same two-space indent and the same
/// empty-array / nested-array shapes, so this is byte-identical.
fn zod_error_message(issues: &[ZodIssue]) -> String {
    serde_json::to_string_pretty(issues).unwrap_or_else(|_| "Invalid input".to_string())
}

/// The path for a key inside a bag reached under `prefix` — `[]` for a
/// route-level `Schema.parse` (the bag IS the parse root), `["cheapLLMSettings"]`
/// for the one bag whose Zod check happens inside the repo's whole-ChatSettings
/// validate.
fn zod_path(prefix: &[&str], key: &str) -> Vec<String> {
    let mut p: Vec<String> = prefix.iter().map(|s| (*s).to_string()).collect();
    p.push(key.to_string());
    p
}

/// Zod's object gate: a non-object input yields exactly one top-level
/// `invalid_type` and the per-key checks never run (Zod's own short-circuit).
fn zod_object_or_issue<'a>(
    v: &'a Value,
    prefix: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    v.as_object().ok_or_else(|| {
        zod_error_message(&[ZodIssue::invalid_type(
            "object",
            prefix.iter().map(|s| (*s).to_string()).collect(),
            Some(v),
        )])
    })
}

/// A `z.boolean().default(d)` key: absent → `d`, a bool → itself, anything else
/// → one `invalid_type` issue (and the default, so the walk continues and every
/// offending key is reported, as Zod does).
fn zod_bool(
    o: &Map<String, Value>,
    key: &'static str,
    default: bool,
    prefix: &[&str],
    issues: &mut Vec<ZodIssue>,
) -> bool {
    match o.get(key) {
        None => default,
        Some(Value::Bool(b)) => *b,
        other => {
            issues.push(ZodIssue::invalid_type(
                "boolean",
                zod_path(prefix, key),
                other,
            ));
            default
        }
    }
}

/// A `z.enum([...]).default(d)` key. A miss — of ANY input type, since Zod's
/// enum compares values, not types — is `invalid_value`.
fn zod_enum(
    o: &Map<String, Value>,
    key: &'static str,
    default: &'static str,
    values: &'static [&'static str],
    prefix: &[&str],
    issues: &mut Vec<ZodIssue>,
) -> &'static str {
    match o.get(key) {
        None => default,
        Some(Value::String(s)) => match values.iter().find(|v| *v == &s.as_str()) {
            Some(v) => v,
            None => {
                issues.push(ZodIssue::invalid_value(values, zod_path(prefix, key)));
                default
            }
        },
        Some(_) => {
            issues.push(ZodIssue::invalid_value(values, zod_path(prefix, key)));
            default
        }
    }
}

/// A `UUIDSchema.nullable().optional()` key: absent → omitted; a present `null`
/// → kept; a string → format-checked; anything else → `invalid_type`. The type
/// check runs BEFORE the format check, so a number is `invalid_type`, never
/// `invalid_format`.
fn zod_opt_uuid(
    o: &Map<String, Value>,
    key: &'static str,
    prefix: &[&str],
    out: &mut Map<String, Value>,
    issues: &mut Vec<ZodIssue>,
) {
    match o.get(key) {
        None => {}
        Some(Value::Null) => {
            out.insert(key.to_string(), Value::Null);
        }
        Some(Value::String(s)) => {
            if zod_uuid_ok(s) {
                out.insert(key.to_string(), json!(s));
            } else {
                issues.push(ZodIssue::invalid_uuid(zod_path(prefix, key)));
            }
        }
        other => issues.push(ZodIssue::invalid_type(
            "string",
            zod_path(prefix, key),
            other,
        )),
    }
}

/// v4 `SmartTypographySettingsSchema.parse` over a PUT bag. Three
/// `z.boolean()` keys with defaults (`displayQuotes` false, `dashes` true,
/// `ellipsis` true), so a PARTIAL bag materializes the absent keys; unknown
/// keys are stripped; the output is in schema declaration order.
///
/// A non-object input yields the single top-level `invalid_type` issue and the
/// key checks never run (Zod's own short-circuit); a non-boolean value yields
/// one issue per offending key, in declaration order. The `Err` string is the
/// whole `ZodError.message`, which is what v4's 400 body carries.
fn zod_smart_typography_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    let o = zod_object_or_issue(v, &[])?;

    let mut issues: Vec<ZodIssue> = Vec::new();
    // Declaration order — both for the issue order and the stored key order.
    let display_quotes = zod_bool(o, "displayQuotes", false, &[], &mut issues);
    let dashes = zod_bool(o, "dashes", true, &[], &mut issues);
    let ellipsis = zod_bool(o, "ellipsis", true, &[], &mut issues);

    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }

    let parsed = chat_settings::SmartTypographySettings {
        display_quotes,
        dashes,
        ellipsis,
    };
    let text = serde_json::to_string(&parsed)
        .map_err(|_| "Invalid smart typography settings".to_string())?;
    Ok(chat_settings::SettingsColVal::Text(text))
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
/// re-emits as `1` — a `f64` round-trip would write `1.0`.
///
/// P4.47 (A) closes the D73-banked Zod-collapse seam here: the failure legs no
/// longer answer the invented `Invalid dangerous content settings` but v4's
/// whole `ZodError.message`. This schema reaches four issue codes — enum misses
/// (`invalid_value`), the `.min(0).max(1)` bound (`too_small` / `too_big`), the
/// `.uuid()` format (`invalid_format`) and plain `invalid_type` — and Zod
/// collects EVERY offending key before throwing, in declaration order, so the
/// walk below never returns early.
fn zod_dangerous_content_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    const MODES: &[&str] = &["OFF", "DETECT_ONLY", "AUTO_ROUTE"];
    const DISPLAY_MODES: &[&str] = &["SHOW", "BLUR", "COLLAPSE"];
    let o = zod_object_or_issue(v, &[])?;
    let mut out = Map::new();
    let mut issues: Vec<ZodIssue> = Vec::new();

    out.insert(
        "mode".into(),
        json!(zod_enum(o, "mode", "OFF", MODES, &[], &mut issues)),
    );
    let threshold = match o.get("threshold") {
        None => json!(0.7),
        Some(n @ Value::Number(num)) => {
            // `as_f64` is infallible for any JSON number serde parsed.
            let f = num.as_f64().unwrap_or_default();
            if f < 0.0 {
                issues.push(ZodIssue::too_small(json!(0), vec!["threshold".into()]));
            } else if f > 1.0 {
                issues.push(ZodIssue::too_big(json!(1), vec!["threshold".into()]));
            }
            n.clone()
        }
        other => {
            issues.push(ZodIssue::invalid_type(
                "number",
                vec!["threshold".into()],
                other,
            ));
            json!(0.7)
        }
    };
    out.insert("threshold".into(), threshold);
    for (key, default) in [
        ("scanTextChat", true),
        ("scanImagePrompts", true),
        ("scanImageGeneration", false),
    ] {
        let got = zod_bool(o, key, default, &[], &mut issues);
        out.insert(key.into(), json!(got));
    }
    zod_opt_uuid(o, "uncensoredTextProfileId", &[], &mut out, &mut issues);
    zod_opt_uuid(o, "uncensoredImageProfileId", &[], &mut out, &mut issues);
    out.insert(
        "displayMode".into(),
        json!(zod_enum(
            o,
            "displayMode",
            "SHOW",
            DISPLAY_MODES,
            &[],
            &mut issues
        )),
    );
    let badges = zod_bool(o, "showWarningBadges", true, &[], &mut issues);
    out.insert("showWarningBadges".into(), json!(badges));
    // `.nullable().optional()` plain string — no format check.
    match o.get("customClassificationPrompt") {
        None => {}
        Some(Value::Null) => {
            out.insert("customClassificationPrompt".into(), Value::Null);
        }
        Some(Value::String(s)) => {
            out.insert("customClassificationPrompt".into(), json!(s));
        }
        other => issues.push(ZodIssue::invalid_type(
            "string",
            vec!["customClassificationPrompt".into()],
            other,
        )),
    }

    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    serde_json::to_string(&Value::Object(out))
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| "Invalid dangerous content settings".to_string())
}

/// v4 `AnswerConfirmationSettingsSchema.parse` over a PUT sub-bag (a route-level
/// parse, `settings/chat/route.ts` L270) — one `z.boolean().default(false)` key,
/// so a partial or empty bag materializes `enabled: false`, unknown keys are
/// stripped, and any failure is the whole `ZodError.message` (P4.47 (A): this
/// used to route through `json_field`, which collapsed every throw to
/// `Invalid answer confirmation settings`).
fn zod_answer_confirmation_settings(v: &Value) -> Result<chat_settings::SettingsColVal, String> {
    let o = zod_object_or_issue(v, &[])?;
    let mut issues: Vec<ZodIssue> = Vec::new();
    let enabled = zod_bool(o, "enabled", false, &[], &mut issues);
    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    let parsed = chat_settings::AnswerConfirmationSettings { enabled };
    serde_json::to_string(&parsed)
        .map(chat_settings::SettingsColVal::Text)
        .map_err(|_| "Invalid answer confirmation settings".to_string())
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
    // P4.47 (A): v4's route runs ONLY the two manual enum guards here — the bag
    // then rides raw into `updateData` and its Zod check happens later, inside
    // the repo's whole-object validate. So the guards fire in place (they must:
    // they precede every arm below) and the parse is DEFERRED to the end of the
    // walk, where v4's really happens. `cheap_llm_slot` remembers where the
    // assignment belongs so the stored key order is untouched.
    let mut cheap_llm_slot: Option<usize> = None;
    if let Some(v) = obj.get("cheapLLMSettings") {
        if let Some(o) = v.as_object() {
            // v4 guards a TRUTHY non-member of any type (`!includes` on the raw
            // value), so a non-string strategy lands here too.
            if let Some(st) = o.get("strategy").filter(|s| is_truthy(s)) {
                if !matches!(
                    st.as_str(),
                    Some("USER_DEFINED" | "PROVIDER_CHEAPEST" | "LOCAL_FIRST")
                ) {
                    return Err("Invalid cheap LLM strategy".to_string());
                }
            }
            if let Some(ep) = o.get("embeddingProvider").filter(|s| is_truthy(s)) {
                if !matches!(ep.as_str(), Some("SAME_PROVIDER" | "OPENAI" | "LOCAL")) {
                    return Err("Invalid embedding provider".to_string());
                }
            }
            // v4 `65f5021c8`, `settings/chat/route.ts:89-94`. Unlike the two
            // enum guards above this one is NOT truthiness-gated: it is
            // `typeof !== 'undefined' && typeof !== 'boolean'`, so an explicit
            // `null` — falsy, and waved through by both guards above — is
            // refused here.
            match o.get("allowCheapFallback") {
                None | Some(Value::Bool(_)) => {}
                Some(_) => return Err("allowCheapFallback must be a boolean".to_string()),
            }
        }
        cheap_llm_slot = Some(out.len());
        out.push(("cheapLLMSettings", Col::Null));
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
    // P4.D73 (v4 4.8.2) — the two composer typeahead gates, at v4's own
    // schema-ordered positions right after `composerSpellcheck`. Same
    // `typeof x !== 'boolean'` guard, same sentence bytes.
    if let Some(v) = obj.get("composerEmoji") {
        out.push((
            "composerEmoji",
            bool_field(v, "Invalid composerEmoji value (must be boolean)")?,
        ));
    }
    if let Some(v) = obj.get("composerUnicode") {
        out.push((
            "composerUnicode",
            bool_field(v, "Invalid composerUnicode value (must be boolean)")?,
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
            zod_answer_confirmation_settings(v)?,
        ));
    }
    // P4.D73 (v4 4.8.2 `2d31810f`) — `SmartTypographySettingsSchema.parse` at
    // the route (NOT the repo's merge-then-validate), so a PARTIAL bag takes
    // each absent key's Zod default. A non-object, an explicit `null`, or a
    // non-boolean value throws, and the 400 body carries the whole
    // `ZodError.message` — reproduced byte-for-byte by
    // [`zod_smart_typography_settings`]. Because the dispatch carries the RAW
    // settings bag (`Request::ChatSettingsUpdate`), an explicit `null` arrives
    // as `Some(Value::Null)` here rather than vanishing into an absent key
    // (the Taboo §3 lesson; pinned end-to-end by the web-edge wire test).
    if let Some(v) = obj.get("smartTypographySettings") {
        out.push(("smartTypographySettings", zod_smart_typography_settings(v)?));
    }

    // P4.47 (A) — the deferred cheap-LLM parse. v4 reaches
    // `CheapLLMSettingsSchema` only inside the repo's whole-object validate,
    // which runs after EVERY arm above; validating it in place would answer the
    // cheap-LLM error for a request whose dangerous-content bag is also bad,
    // where v4 answers the dangerous-content one.
    //
    // NOTE (still open, and named rather than hidden): the same repo-level
    // validate also governs the fields v4's route stores raw with no check of
    // its own — `imageDescriptionProfileId`,
    // `uncensoredImageDescriptionProfileId` and the two manually-guarded bags
    // (`contextCompressionSettings`, `thinkingDisplay`) past their guards. Those
    // arms still answer v5's own sentences; no corpus case exercises them, and
    // closing them is a separate order (this one's mandate is the three D73
    // siblings).
    if let Some(slot) = cheap_llm_slot {
        let v = obj
            .get("cheapLLMSettings")
            .expect("slot is only set when the key is present");
        out[slot].1 = zod_cheap_llm_settings(v)?;
    }
    Ok(out)
}

/// JS truthiness for a JSON value — v4's guards are written `if (x)`, so `0`,
/// `""`, `false` and `null` fall through them.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
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
    // v4 `route.ts:176-188`, checked right after the transport gate. Present but
    // not a boolean — INCLUDING an explicit `null` — is a 400; absent takes the
    // RESOLVED default — off for Anthropic (4.6+ rejects an assistant tail) and
    // off for a profile that will run a thinking turn (v4 bug 85), on
    // everywhere else — which is then STORED (create never writes the tri-state
    // NULL). Not gated on `isCourier`: v4's comment is that this is not a tool
    // flag, and the Courier renders the same assembled context. v4 passes
    // `profileRunsThinkingTurn(provider, modelName, parameters)`; an absent
    // `parameters` bag reads as v4's Zod-defaulted `{}` (no key either way).
    let multi_character_prefill = match bag.get("multiCharacterPrefill") {
        None => crate::services::multi_character_prefill::default_multi_character_prefill(
            Some(&provider),
            crate::services::thinking_turn::profile_runs_thinking_turn(
                Registry::built_in(),
                Some(&provider),
                bag.get("modelName").and_then(Value::as_str),
                bag.get("parameters"),
            ),
        ),
        Some(Value::Bool(b)) => *b,
        Some(_) => return bad_request("multiCharacterPrefill must be a boolean"),
    };

    // The fallback chain (v4 `65f5021c8`, `route.ts:170-206`). Both gates sit
    // HERE — after the prefill check, before the required-field checks — so a
    // create with a bad `allowTierFallback` AND no name answers the fallback
    // error, exactly as v4's route order does.
    //
    // v4 destructures with defaults (`fallbackProfileId = null`,
    // `allowTierFallback = false`), so absent and explicit-null differ only for
    // `allowTierFallback`: its `typeof !== 'boolean'` gate 400s on an explicit
    // null where absence takes the default.
    let allow_tier_fallback = match bag.get("allowTierFallback") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return bad_request("allowTierFallback must be a boolean"),
    };
    // A brand-new profile has no id yet, so the self-reference rule cannot bite
    // here; what can is a target that is not the user's, or one whose transport
    // is Courier — a request a human carries by hand is no kind of automatic
    // failover.
    let fallback_profile_id: Option<String> = match bag.get("fallbackProfileId") {
        None | Some(Value::Null) => None,
        Some(Value::String(target)) if target.is_empty() => None,
        Some(Value::String(target)) => {
            let target = target.clone();
            let lookup = target.clone();
            let found =
                match db.read_main(move |conn| connection_profiles::find_by_id(conn, &lookup)) {
                    Ok(v) => v,
                    Err(e) => return internal(e),
                };
            let owned_by_user = found
                .as_ref()
                .and_then(|p| s(p, "userId"))
                .is_some_and(|owner| owner == user_id);
            let Some(found) = found.filter(|_| owned_by_user) else {
                return bad_request("Fallback profile not found");
            };
            if s(&found, "transport").as_deref() == Some("courier") {
                return bad_request(
                    "A Courier profile cannot be used as a fallback — its requests are carried by hand, so it cannot stand in automatically",
                );
            }
            Some(target)
        }
        Some(_) => return bad_request("fallbackProfileId must be a profile id or null"),
    };

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
        // v4 answers `conflict(...)` → 409 here (`connection-profiles/
        // route.ts:206`, `[id]/route.ts:176`) — was bad_request/400 until the
        // help-drift unification's status-assert pass caught it.
        return Response::error(
            ErrorKind::Conflict,
            format!(
                "A connection profile named \"{}\" already exists",
                name.trim()
            ),
        );
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
        // The create route always resolves a boolean (`:1331`), so the column
        // is always named and never NULL — the outer `Some` is "the document
        // carries the key", the inner one its value.
        multi_character_prefill: Some(Some(multi_character_prefill)),
        model_class,
        fallback_profile_id,
        allow_tier_fallback,
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
            .ok_or_else(|| DbError::Internal("created profile vanished".into()))?;
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
            // v4 `[id]/route.ts:176` — conflict → 409 (see the create arm).
            return Response::error(
                ErrorKind::Conflict,
                format!(
                    "A connection profile named \"{}\" already exists",
                    name.trim()
                ),
            );
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

    // apiKeyId (non-courier) — the shared reader decides the SHAPE; this site
    // adds the provider match, which the other two profile kinds do not have.
    if !is_courier {
        if let Some(v) = bag.get("apiKeyId") {
            match classify_api_key_id(v) {
                ApiKeyIdPatch::Clear => patch.api_key_id = Some(None),
                ApiKeyIdPatch::Set(akid) => {
                    let akid_owned = akid.to_string();
                    let key =
                        match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
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
                ApiKeyIdPatch::Refuse => return not_found("API key"),
            }
        }
    }

    // baseUrl (non-courier) — the shared reader; this site's patch spells the
    // clear as a separate flag rather than an `Option<Option<_>>`.
    if !is_courier {
        if let Some(v) = bag.get("baseUrl") {
            match classify_base_url(v) {
                BaseUrlPatch::Set(b) => patch.base_url = Some(b.to_string()),
                BaseUrlPatch::Clear => patch.clear_base_url = true,
                BaseUrlPatch::Refuse => return internal("Failed to update connection profile"),
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

    // v4 `[id]/route.ts:286-292`. NOT gated on `isCourier` — v4's comment: "not
    // a tool flag; the Courier renders the same assembled context for the user
    // to carry by hand". Present-but-not-boolean (an explicit `null` included)
    // is a 400; the bag is an untyped `Value` all the way from the dispatch
    // verb, so `Some(Value::Null)` reaches here intact and no double-`Option`
    // carry is needed to keep it apart from absent (the Taboo §3 hazard exists
    // only where a typed DTO sits at the web edge — this verb has none).
    if let Some(v) = bag.get("multiCharacterPrefill") {
        let Some(b) = v.as_bool() else {
            return bad_request("multiCharacterPrefill must be a boolean");
        };
        patch.multi_character_prefill = Some(b);
    }

    // The fallback chain (v4 `65f5021c8`, `[id]/route.ts:324-355`). Two rules,
    // both structural: a profile cannot understudy itself (the chain would be
    // one attempt wearing two names), and a Courier profile cannot stand in for
    // anyone (its "transport" is a human carrying the request by hand).
    // Everything else — a target with no API key yet, a cycle A->B/B->A — is
    // legal; chains never recurse, so a cycle simply stops.
    //
    // Note the guard ORDER: the self-reference check comes BEFORE the lookup,
    // so naming yourself answers "cannot be its own fallback" even when the row
    // would have been found. Not gated on `isCourier` in v4 either.
    if let Some(v) = bag.get("fallbackProfileId") {
        match v {
            Value::Null => patch.fallback_profile_id = Some(None),
            Value::String(target) if target.is_empty() => patch.fallback_profile_id = Some(None),
            Value::String(target) if target == id => {
                return bad_request("A connection profile cannot be its own fallback")
            }
            Value::String(target) => {
                let target = target.clone();
                let lookup = target.clone();
                let found = match db
                    .read_main(move |conn| connection_profiles::find_by_id(conn, &lookup))
                {
                    Ok(v) => v,
                    Err(e) => return internal(e),
                };
                let owned_by_user = found
                    .as_ref()
                    .and_then(|p| s(p, "userId"))
                    .is_some_and(|owner| owner == user_id);
                let Some(found) = found.filter(|_| owned_by_user) else {
                    return bad_request("Fallback profile not found");
                };
                if s(&found, "transport").as_deref() == Some("courier") {
                    return bad_request(
                        "A Courier profile cannot be used as a fallback — its requests are carried by hand, so it cannot stand in automatically",
                    );
                }
                patch.fallback_profile_id = Some(Some(target));
            }
            _ => return bad_request("fallbackProfileId must be a profile id or null"),
        }
    }

    if let Some(v) = bag.get("allowTierFallback") {
        let Some(b) = v.as_bool() else {
            return bad_request("allowTierFallback must be a boolean");
        };
        patch.allow_tier_fallback = Some(b);
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
    let cleared = cleared_null_keys(&patch);
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
            .ok_or_else(|| DbError::Internal("updated profile vanished".into()))?;
        enrich_profile(conn, &profile, true)
    });
    match out {
        Ok(mut v) => {
            restore_cleared_nulls(&mut v, &cleared);
            Response::ConnectionProfile(json!({ "profile": v }))
        }
        Err(e) => internal(e),
    }
}

/// The keys a PUT can clear to an explicit `null` (`TaggableBaseRepository`
/// aside, these are the four `updateData.<k> = null` sites in v4's route, plus
/// the courier gate's two).
fn cleared_null_keys(patch: &connection_profiles::CpUpdate) -> Vec<&'static str> {
    let mut out = Vec::new();
    if matches!(patch.api_key_id, Some(None)) {
        out.push("apiKeyId");
    }
    if patch.clear_base_url {
        out.push("baseUrl");
    }
    if patch.clear_model_class {
        out.push("modelClass");
    }
    if patch.clear_max_context {
        out.push("maxContext");
    }
    if matches!(patch.fallback_profile_id, Some(None)) {
        out.push("fallbackProfileId");
    }
    out
}

/// v4's `_update` answers `validate({...existing, ...updateData})` — the
/// IN-MEMORY merge, never a re-read (`base.repository.ts:342-370`). So a key the
/// PUT set to `null` is PRESENT as an explicit `null` in the response, while the
/// same row read back from SQL omits it (a NULL cell is `undefined` after Zod,
/// which `JSON.stringify` drops). v5 answers from a re-read, so the cleared keys
/// are put back here.
///
/// Their POSITION is the schema's, not the merge's: Zod's object parse rebuilds
/// in SHAPE order, so a key absent from `existing` does not land at the end.
/// Measured against v4 at `d123658d`, both ways (a row whose column already held
/// a value, and one whose column was already NULL) — the
/// `cp_update_base_url_empty*` / `cp_update_clear_optionals` /
/// `cp_update_courier_gate` corpus arms.
///
/// This was invisible until P4.D85 gave the fixture a profile with a stored
/// `baseUrl`: with every column already NULL, `existing` had no key for the
/// merge to overwrite and both sides agreed by accident.
fn restore_cleared_nulls(profile: &mut Value, cleared: &[&str]) {
    if cleared.is_empty() {
        return;
    }
    let Some(obj) = profile.as_object() else {
        return;
    };
    let mut rebuilt = Map::new();
    for key in connection_profiles::cp_schema_key_order() {
        if let Some(v) = obj.get(key) {
            rebuilt.insert(key.to_string(), v.clone());
        } else if cleared.contains(&key) {
            rebuilt.insert(key.to_string(), Value::Null);
        }
    }
    // Anything the enrichment appended past the document's own keys (`apiKey`).
    for (k, v) in obj {
        if !rebuilt.contains_key(k) {
            rebuilt.insert(k.clone(), v.clone());
        }
    }
    *profile = Value::Object(rebuilt);
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

// ---------------------------------------------------------------------------
// Connection-profile tags (v4 Bug 74, commit `d123658d`)
//
// v4 reaches these three through `?action=` on the item route; v5 has no
// `?action=` surface for connection profiles (no REST edge exists — the SPA and
// every other consumer ride `/api/dispatch`), so the ACTIONS are the verbs and
// v4's two action-gate 400 sentences have no v5 counterpart. The differential
// RECORDS both v4 gate arms and asserts their bytes so upstream copy drift is
// caught — the `search_replace` middleware-arm precedent. `auto-configure` is
// the third v4 POST action and is unported (no service, no consumer); it is
// recorded the same way rather than given a phantom verb to refuse from.
// ---------------------------------------------------------------------------

/// v4 `GET /api/v1/connection-profiles/[id]?action=get-tags` — ownership 404
/// first (the GET's own `findById` gate), then the FLAT `EditorTag` list.
pub fn connection_profile_get_tags(db: &Db, id: &str) -> Response {
    let id_owned = id.to_string();
    let out = db.read_main(move |conn| {
        let Some(profile) = connection_profiles::find_by_id(conn, &id_owned)? else {
            return Ok(None);
        };
        Ok(Some(tags::resolve_editor_tags(
            conn,
            &tag_ids_of(&profile),
        )?))
    });
    match out {
        Ok(Some(t)) => Response::ConnectionProfile(json!({ "tags": t })),
        Ok(None) => not_found("Connection profile"),
        Err(e) => internal(e),
    }
}

/// A profile's own `tags` id array (the slim column), string entries only.
fn tag_ids_of(profile: &Value) -> Vec<String> {
    profile
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `?action=add-tag` — ownership 404, `z.uuid()` on `tagId` (a ZodError is
/// caught by `handleRouteError` → 400 `Validation error`), tag existence 404,
/// then `TaggableBaseRepository.addTag`: push + persist ONLY when the id is not
/// already held. Answers `{success: true, tag}` with the full tag row (201 on
/// v4's wire; the dispatch envelope carries no per-verb status).
pub async fn connection_profile_add_tag(db: &Db, id: &str, tag_id: &str) -> Response {
    let (pid, tid) = (id.to_string(), tag_id.to_string());
    let pre = db.read_main(move |conn| {
        let Some(profile) = connection_profiles::find_by_id(conn, &pid)? else {
            return Ok(None);
        };
        Ok(Some((
            tag_ids_of(&profile),
            tags::find_full_by_id(conn, &tid)?,
        )))
    });
    let (current, tag) = match pre {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Connection profile"),
        Err(e) => return internal(e),
    };
    // v4 parses the body BEFORE the tag lookup, so a malformed `tagId` is a 400
    // even when it would also have missed.
    if !is_zod_uuid(tag_id) {
        return validation_error();
    }
    let Some(tag) = tag else {
        return not_found("Tag");
    };
    if !current.iter().any(|t| t == tag_id) {
        let mut next = current;
        next.push(tag_id.to_string());
        let now = crate::clock::now_iso();
        let pid2 = id.to_string();
        if let Err(e) = db
            .write(move |w| {
                connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                    .update(
                        &pid2,
                        &connection_profiles::CpUpdate {
                            tags: Some(next),
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
    Response::ConnectionProfile(json!({ "success": true, "tag": tag }))
}

/// v4 `?action=remove-tag` — ownership 404, `z.uuid()` on `tagId`, then
/// `TaggableBaseRepository.removeTag`: filter the id out and persist ONLY when
/// the array actually shrank. NO tag-existence check (removing an id no tag
/// backs is a silent success). Answers `{success: true}`.
pub async fn connection_profile_remove_tag(db: &Db, id: &str, tag_id: &str) -> Response {
    let (pid, tid) = (id.to_string(), tag_id.to_string());
    let current = match db.read_main(move |conn| {
        Ok(connection_profiles::find_by_id(conn, &pid)?.map(|p| tag_ids_of(&p)))
    }) {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Connection profile"),
        Err(e) => return internal(e),
    };
    if !is_zod_uuid(tag_id) {
        return validation_error();
    }
    let filtered: Vec<String> = current.iter().filter(|t| *t != &tid).cloned().collect();
    if filtered.len() != current.len() {
        let now = crate::clock::now_iso();
        let pid2 = id.to_string();
        if let Err(e) = db
            .write(move |w| {
                connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                    .update(
                        &pid2,
                        &connection_profiles::CpUpdate {
                            tags: Some(filtered),
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
    Response::ConnectionProfile(json!({ "success": true }))
}

/// v4 `handleRouteError`'s ZodError arm (`lib/api/middleware/context.ts:166`) →
/// `validationError(error)` → 400 `{error: 'Validation error', details: [...]}`.
/// The `details` array is v4-implementation-specific (the Zod issue objects) and
/// the differential drops it on both sides, as the other settings families do.
fn validation_error() -> Response {
    bad_request("Validation error")
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
/// the v5 manifest [`Registry`]. `icon` is always `null` (the manifest
/// deliberately lacks it).
///
/// `search_providers` are the SEARCH-provider manifests the host registered this
/// boot (P4.59) — the SAME registration answer that decides whether the
/// `search_web` runner has a provider, so what is advertised here and what
/// executes can never disagree. v4's route spreads
/// `[...providerList, ...searchProviderList]`, so they are appended AFTER the
/// LLM rows, and their row shape is materially different: no `capabilities`
/// (which is how v4's own profile editor keeps them out of the LLM picker —
/// `p.capabilities?.chat` on an absent bag), no `optionsSchema`, no
/// `thinkingTurnRule`, and a hand-built THREE-key `configRequirements` (no
/// `acceptsApiKey`, no base-URL labels). Key order is wire-visible under
/// `preserve_order`; mirror the omissions and the positions exactly, per the
/// bug-81 precedent below.
///
/// `optionsSchema` is served from the manifest since P4.D83 (shared contract B):
/// per provider, v4's `ProviderOptionsSchema` verbatim, or `null` exactly when
/// the plugin declares none (google only, at the `93ed8abf` pin). Field keys are
/// storage keys inside the profile's `parameters` blob and must round-trip
/// untouched — the renderer reads and writes the flat bag, and the provider
/// reads the SAME keys at call time — so the value is carried opaquely and never
/// reshaped here.
pub fn provider_list(search_providers: &[&SearchManifest]) -> Response {
    let registry = Registry::built_in();
    let providers: Vec<Value> = registry
        .all_providers()
        .iter()
        .map(|m| {
            let req = &m.config_requirements;
            let mut config = Map::new();
            config.insert("requiresApiKey".into(), json!(req.requires_api_key));
            // v4 bug 81: the route passes `plugin.config` through whole, so the
            // key is present exactly where the plugin declares it — OAC only.
            // v5 hand-builds this map, so mirror both the omission and the
            // position (insertion order is wire-visible under `preserve_order`).
            if let Some(accepts) = req.accepts_api_key {
                config.insert("acceptsApiKey".into(), json!(accepts));
            }
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
                "optionsSchema": m.options_schema.clone().unwrap_or(Value::Null),
                // v4 bug 85 (`97d2fcb5`): how the profile editor tells whether
                // the profile in front of the user will run a thinking turn,
                // which decides the multi-character prefill default.
                // Declarative precisely so it can cross the wire — the browser
                // cannot call a server-side plugin. `?? null`, positioned
                // after `optionsSchema`, exactly as v4's route emits it; the
                // typed struct's field order carries the rule's key order.
                "thinkingTurnRule": m.thinking_turn_rule.as_ref()
                    .map(|r| serde_json::to_value(r).expect("rule serializes"))
                    .unwrap_or(Value::Null),
            })
        })
        .collect();
    let mut providers = providers;
    for m in search_providers {
        let req = &m.config_requirements;
        // The route hand-builds these THREE keys, in this order — NOT the
        // plugin's whole `config` (that is the LLM arm above).
        let mut config = Map::new();
        config.insert("requiresApiKey".into(), json!(req.requires_api_key));
        config.insert("requiresBaseUrl".into(), json!(req.requires_base_url));
        // v4 reads `plugin.config.apiKeyLabel` unguarded, so the key is ALWAYS
        // present — `undefined` would drop it, but every search plugin declares
        // one and `JSON.stringify` of an absent label would be `null` in the
        // manifest either way.
        config.insert("apiKeyLabel".into(), json!(req.api_key_label));
        providers.push(json!({
            "id": m.id,
            "name": m.id,
            "displayName": m.display_name,
            "description": m.description,
            "abbreviation": m.abbreviation,
            "colors": { "bg": m.colors.bg, "text": m.colors.text, "icon": m.colors.icon },
            "icon": Value::Null,
            "type": "search",
            "configRequirements": Value::Object(config),
        }));
    }
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

    // v4 bug 85 (`97d2fcb5`): the thinking facts travel with the model so the
    // connection-profile editor can seed the multi-character prefill box the
    // way the server would. `thinksByDefault` is the load-bearing one: a model
    // that reasons unasked is one the user never opted into. v4's route
    // spreads `staticInfo?.supportsThinking` / `staticInfo?.thinksByDefault`
    // per exact-id match onto each `modelsWithInfo` row and `undefined` drops
    // the key — mirrored here from the manifest's model catalogue (the merge
    // lives at the ROUTE in v4, not the fetcher, so it covers every fetcher).
    // The GET leg is untouched: v4's cache write carries no thinking facts,
    // so the cached-models read never serves them.
    let registry = Registry::built_in();
    let models_with_info: Vec<Value> = models_with_info
        .into_iter()
        .map(|mut m| {
            let facts = m
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| registry.model_thinking_facts(provider, id));
            if let (Some(facts), Some(obj)) = (facts, m.as_object_mut()) {
                if let Some(st) = facts.supports_thinking {
                    obj.insert("supportsThinking".into(), json!(st));
                }
                if let Some(tbd) = facts.thinks_by_default {
                    obj.insert("thinksByDefault".into(), json!(tbd));
                }
            }
            m
        })
        .collect();

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
        max_tokens: Some(max_tokens),
        strict_max_tokens: false,
        top_p: None,
        cache_key: None,
        profile_parameters: None,
        attachments: vec![],
        // v4 sets `requestTimeoutMs` ONLY on the cheap-LLM path
        // (`core-execution.ts` `baseParams`); every other `sendMessage` caller
        // leaves the provider's own default in charge.
        request_timeout_ms: None,
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
        // P4.56: v4's catch answers its own FIXED sentence; this used to leak the
        // `DbError` text (harmless while nothing served the route, and the first
        // thing an operator would see now that `quilltap-web` does).
        Err(_) => Response::error(
            ErrorKind::Internal,
            "Failed to fetch data-retention settings",
        ),
    }
}

/// v4 `PUT /api/v1/settings/data-retention` — merge `{...current, ...body}`,
/// `safeParse` (validate `staleChatDays` as an int in `[1, 3650]`), persist, and
/// echo the parsed settings. On a schema violation v4 returns `validationError`
/// (a 400 `{error: 'Validation error', details}`); the port surfaces the
/// `{error}` envelope (the Zod issue array is v4-implementation-specific).
pub async fn data_retention_settings_update(
    db: &Db,
    stale_chat_days: Option<Option<Value>>,
) -> Response {
    // P4.56: v4's catch answers its own FIXED sentence on every failure inside
    // the try (the read, the write); the `DbError` text used to leak instead.
    let failed = || {
        Response::error(
            ErrorKind::Internal,
            "Failed to update data-retention settings",
        )
    };
    let current = match db.read_main(instance_settings::get_data_retention_settings) {
        Ok(d) => d,
        Err(_) => return failed(),
    };
    // The only schema field is `staleChatDays`; the merge overlays it when the
    // body carries it, else the current value survives.
    //
    // P4.57: the tri-state arrives DECODED (`types::data_retention_update_request`
    // is the one decoder) rather than being re-derived here from a rebuilt bag —
    // an explicit `null` is `Some(None)`, which validates as the `null` Zod's
    // `.default(30)` never sees.
    let days = match stale_chat_days {
        None => current,
        Some(v) => match instance_settings::validate_stale_chat_days(&v.unwrap_or(Value::Null)) {
            Some(d) => d,
            None => return Response::error(ErrorKind::BadRequest, "Validation error"),
        },
    };
    match db
        .write(move |w| instance_settings::set_data_retention_settings(w.main().connection(), days))
        .await
    {
        Ok(_) => Response::DataRetention(json!({ "staleChatDays": days })),
        Err(_) => failed(),
    }
}

// === end P4.d3 ===

// === P4.D50 === (the instance-wide Taboo list — the `7df7de8e` drift)

/// v4 `GET /api/v1/settings/taboo` — the instance-wide forbidden-phrase list
/// `{phrases}` (empty when never written). A read failure is v4's
/// `serverError('Failed to fetch taboo settings')`.
pub fn taboo_settings_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_taboo_settings) {
        Ok(phrases) => Response::Taboo(json!({ "phrases": phrases })),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to fetch taboo settings"),
    }
}

/// v4 `PUT /api/v1/settings/taboo` — read the current settings, merge the body
/// OVER them (`{...current, ...body}`, so a partial body — `{}` in particular —
/// can never wipe the list), `safeParse` the merge, persist, and echo what was
/// stored.
///
/// The echo is `setTabooSettings`'s return value, i.e. the NORMALIZED list
/// (trimmed, blanks dropped, case-insensitive duplicates dropped keeping the
/// first), not the submission — that is what keeps the client's cache in step
/// with the database. A schema violation is v4's `validationError` (400
/// `{error: 'Validation error', details}`); the port surfaces the `{error}`
/// envelope, the Zod issue array being v4-implementation-specific.
pub async fn taboo_settings_update(db: &Db, phrases: Option<Option<Value>>) -> Response {
    let current = match db.read_main(instance_settings::get_taboo_settings) {
        Ok(p) => p,
        Err(_) => return Response::error(ErrorKind::Internal, "Failed to update taboo settings"),
    };
    // The schema's only field is `phrases`; the merge overlays it when the body
    // carries it (even as an explicit empty array — that is the clear gesture —
    // or an explicit `null`, which Zod rejects), else the current value survives
    // and is re-validated as v4 re-validates it.
    //
    // P4.57: the tri-state arrives DECODED (`types::taboo_update_request` is the
    // one decoder) instead of being re-derived here from a rebuilt bag. `Some(v)`
    // is the key present, `v` being `None` for an explicit `null` — which
    // `json!` renders back as `null`, the value Zod refuses.
    let merged = match phrases {
        Some(v) => json!({ "phrases": v }),
        None => json!({ "phrases": current }),
    };
    let Some(parsed) = instance_settings::parse_taboo_settings(&merged) else {
        return Response::error(ErrorKind::BadRequest, "Validation error");
    };
    match db
        .write(move |w| instance_settings::set_taboo_settings(w.main().connection(), &parsed))
        .await
    {
        // `Ok(None)` is v4's `setTabooSettings` THROW (the schema refused the
        // normalized list), which lands in the route's catch as the 500.
        Ok(Some(saved)) => Response::Taboo(json!({ "phrases": saved })),
        Ok(None) | Err(_) => {
            Response::error(ErrorKind::Internal, "Failed to update taboo settings")
        }
    }
}

// === end P4.D50 ===

// === P4.D57 === (the instance-wide Brahma Console turn budget — v4 `6452e2c3`)

/// v4 `GET /api/v1/settings/brahma-console` — the instance-wide agent-turn budget
/// `{maxAgentTurns}` (the default 50 when never written). A read failure is v4's
/// `serverError('Failed to fetch brahma-console settings')`.
pub fn brahma_console_settings_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_brahma_console_settings) {
        Ok(max_agent_turns) => Response::BrahmaConsole(json!({ "maxAgentTurns": max_agent_turns })),
        Err(_) => Response::error(
            ErrorKind::Internal,
            "Failed to fetch brahma-console settings",
        ),
    }
}

/// v4 `PUT /api/v1/settings/brahma-console` — read the current settings, merge the
/// body OVER them (`{...current, ...body}`, so a partial body — `{}` in
/// particular — can never wipe the value), `safeParse` the merge, persist, and
/// echo what was stored.
///
/// A schema violation (out of range, non-integer, a string, or an explicit
/// `null`) is v4's `validationError` (400 `{error: 'Validation error', details}`);
/// the port surfaces the `{error}` envelope, the Zod issue array being
/// v4-implementation-specific. The taboo `taboo_settings_update` precedent.
pub async fn brahma_console_settings_update(
    db: &Db,
    max_agent_turns: Option<Option<Value>>,
) -> Response {
    let current = match db.read_main(instance_settings::get_brahma_console_settings) {
        Ok(v) => v,
        Err(_) => {
            return Response::error(
                ErrorKind::Internal,
                "Failed to update brahma-console settings",
            )
        }
    };
    // The schema's only field is `maxAgentTurns`; the merge overlays it when the
    // body carries it (even an explicit `null` — which Zod rejects), else the
    // current value survives and is re-validated as v4 re-validates it.
    //
    // P4.57: the tri-state arrives DECODED (`types::brahma_console_update_request`
    // is the one decoder) rather than being re-derived here from a rebuilt bag.
    let merged = match max_agent_turns {
        Some(v) => json!({ "maxAgentTurns": v }),
        None => json!({ "maxAgentTurns": current }),
    };
    let Some(parsed) = instance_settings::parse_brahma_console_settings(&merged) else {
        return Response::error(ErrorKind::BadRequest, "Validation error");
    };
    match db
        .write(move |w| {
            instance_settings::set_brahma_console_settings(w.main().connection(), parsed)
        })
        .await
    {
        // `Ok(None)` is v4's `setBrahmaConsoleSettings` THROW (the schema refused
        // the value), which lands in the route's catch as the 500 — unreachable
        // here since the handler pre-parses, but faithful.
        Ok(Some(saved)) => Response::BrahmaConsole(json!({ "maxAgentTurns": saved })),
        Ok(None) | Err(_) => Response::error(
            ErrorKind::Internal,
            "Failed to update brahma-console settings",
        ),
    }
}

// === end P4.D57 ===

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
        match provider_list(&[]) {
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

    /// P4.59: with the Serper provider registered the search row is APPENDED
    /// (v4's spread order) and carries no capability bag — the absence v4's own
    /// profile editor filters the LLM picker on.
    #[test]
    fn providers_listing_appends_the_registered_search_row() {
        let search =
            crate::provider_manifest::search::SearchRegistry::built_in().registered(None, None);
        let (with, without) = (provider_list(&search), provider_list(&[]));
        let (Response::Providers(with), Response::Providers(without)) = (with, without) else {
            panic!("expected two listings");
        };
        let a = with["providers"].as_array().unwrap();
        let b = without["providers"].as_array().unwrap();
        assert_eq!(a.len(), b.len() + 1, "the search row is additive");
        assert_eq!(&a[..b.len()], &b[..], "the LLM rows are untouched");
        let row = a.last().unwrap();
        assert_eq!(row["id"], json!("SERPER"));
        assert_eq!(row["type"], json!("search"));
        assert!(row.get("capabilities").is_none());
        assert!(row.get("optionsSchema").is_none());
        assert!(row.get("thinkingTurnRule").is_none());
        assert_eq!(
            serde_json::to_string(&row["configRequirements"]).unwrap(),
            r#"{"requiresApiKey":true,"requiresBaseUrl":false,"apiKeyLabel":"Serper API Key"}"#
        );
        assert_eq!(with["count"], json!(a.len()));
    }
}
