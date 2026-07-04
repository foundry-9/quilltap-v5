//! Port of three help / agent-mode tool handlers (W4.1d batch 1):
//!
//!   - `help_settings`  — v4 `lib/tools/handlers/help-settings-handler.ts`
//!     (a DB read: the settings reader for help characters like Lorian/Riya).
//!   - `help_navigate`  — v4 `lib/tools/handlers/help-navigate-handler.ts`
//!     (pure URL validation for client-side navigation).
//!   - `submit_final_response` — v4 `lib/tools/handlers/submit-final-response-handler.ts`
//!     (agent mode: pure validate-and-return; the loop/completion semantics are
//!     W4.1e, out of scope — only the handler's validate+return+format shape).
//!
//! Each ports the `execute*Tool` (→ the native Output) + `format*Results`
//! (→ the string that lands in the tool-result message). The Output structs use
//! `#[serde(rename)]` to v4's camelCase and drop absent optionals exactly as
//! `JSON.stringify` drops `undefined`.
//!
//! ## `help_settings` — what it reads
//!
//! v4's `fetchCategorySettings` reads up to FIVE user-scoped repos, all in the
//! **main** database, all with the standard `findByUserId` net-read:
//!   - `chatSettings.findByUserId` (a single row or `null`);
//!   - `connections.findByUserId` / `embeddingProfiles.findByUserId` /
//!     `imageProfiles.findByUserId` / `roleplayTemplates.findByUserId` (lists).
//!
//! The connection/embedding/image profile lists are passed through an explicit
//! **allowlist** sanitizer (`sanitize*Profile`) that emits only a small, safe
//! field set (API keys/secrets are NEVER exposed) in a fixed literal key order.
//! The roleplay-template list is mapped to `{ id, name, description, isDefault }`
//! — but `RoleplayTemplateSchema` has **no `isDefault` field**, so that key is
//! always `undefined` → dropped by `JSON.stringify` (banked by the differential).
//!
//! The full `chatSettings.findByUserId` net-read is ported in
//! [`crate::db::chat_settings::find_by_user_id`] (a deferred read sub-unit of the
//! already-ported chat_settings repo). The four profile lists are read here as
//! scoped SELECTs producing exactly the sanitizer/mapper field set (no full
//! net-read marshaling is needed — only the allowlisted columns land in the
//! output), matching v4's `findByFilter({ userId })` (no ORDER BY → rowid order,
//! identical on both sides reading the same fixture).
//!
//! `formatHelpSettingsResults` renders `Settings: <Titlecased category>\n\n` +
//! `JSON.stringify(data, null, 2)`. Under the workspace-wide `serde_json`
//! `preserve_order` feature, a `Value::Object` is an insertion-ordered `IndexMap`
//! and `to_string_pretty` emits 2-space-indented JSON with insertion order — the
//! byte-for-byte analogue of `JSON.stringify(x, null, 2)`.

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::DbError;

// ============================================================================
// help_settings
// ============================================================================

/// The eight valid categories (v4 `HelpSettingsCategory`).
const HELP_SETTINGS_CATEGORIES: [&str; 8] = [
    "overview",
    "chat",
    "connections",
    "embeddings",
    "images",
    "appearance",
    "templates",
    "system",
];

/// v4 `HelpSettingsToolOutput` — `{ success, category, data?, error? }`.
#[derive(Debug, Clone, Serialize)]
pub struct HelpSettingsOutput {
    pub success: bool,
    pub category: String,
    /// The category payload (only on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// The failure message (only on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `validateHelpSettingsInput` — the Zod object requires `category` to be one
/// of the eight enum values. Returns the validated category, or `None` when the
/// input is not an object with a valid `category` string.
fn validate_help_settings_input(args: &Value) -> Option<String> {
    let category = args.as_object()?.get("category")?.as_str()?;
    if HELP_SETTINGS_CATEGORIES.contains(&category) {
        Some(category.to_string())
    } else {
        None
    }
}

/// The exact v4 validation-failure message.
const HELP_SETTINGS_INVALID: &str = "Invalid input: category is required and must be one of: overview, chat, connections, embeddings, images, appearance, templates, system";

/// Execute the `help_settings` tool. Reads the requested category's settings for
/// `user_id` and returns v4's `HelpSettingsToolOutput`. A validation failure
/// yields `{ success: false, category: 'overview', error }`; a DB error yields
/// `{ success: false, category, error }` (v4's catch path). Never panics — a DB
/// error surfaces as the failure output, mirroring v4's try/catch.
pub async fn execute_help_settings(db: &Db, user_id: &str, args: &Value) -> HelpSettingsOutput {
    let Some(category) = validate_help_settings_input(args) else {
        return HelpSettingsOutput {
            success: false,
            category: "overview".to_string(),
            data: None,
            error: Some(HELP_SETTINGS_INVALID.to_string()),
        };
    };

    match fetch_category_settings(db, &category, user_id) {
        Ok(data) => HelpSettingsOutput {
            success: true,
            category,
            data: Some(data),
            error: None,
        },
        Err(e) => HelpSettingsOutput {
            success: false,
            category,
            data: None,
            error: Some(e.to_string()),
        },
    }
}

/// Format the `help_settings` result — v4 `formatHelpSettingsResults`.
///
/// `Settings: <Titlecased category>\n\n` + `JSON.stringify(data, null, 2)`. On
/// failure (or missing `data`), returns the error, else the static fallback.
pub fn format_help_settings(out: &HelpSettingsOutput) -> String {
    let Some(data) = out.data.as_ref() else {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Failed to read settings.".to_string());
    };
    if !out.success {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Failed to read settings.".to_string());
    }

    let header = format!("Settings: {}", title_case_first(&out.category));
    let body = serde_json::to_string_pretty(data).unwrap_or_else(|_| String::from("{}"));
    format!("{header}\n\n{body}")
}

/// `category.charAt(0).toUpperCase() + category.slice(1)`. The categories are
/// all ASCII, so ASCII case mapping suffices.
fn title_case_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Read the category payload from the main DB. Errors propagate as v4's catch
/// path.
fn fetch_category_settings(db: &Db, category: &str, user_id: &str) -> Result<Value, DbError> {
    match category {
        "chat" => fetch_chat(db, user_id),
        "connections" => fetch_connections(db, user_id),
        "embeddings" => fetch_embeddings(db, user_id),
        "images" => fetch_images(db, user_id),
        "appearance" => fetch_appearance(db, user_id),
        "templates" => fetch_templates(db, user_id),
        "system" => fetch_system(db, user_id),
        "overview" => fetch_overview(db, user_id),
        // Unreachable — validation constrained the category — but mirror v4's
        // UNKNOWN_CATEGORY throw as a DB-layer error (never exercised).
        other => Err(DbError::Key(format!("Unknown category: {other}"))),
    }
}

// --- chat ---------------------------------------------------------------------

/// v4 `case 'chat'`: the settings row's chat-relevant fields, or a placeholder
/// when the user has no settings row.
fn fetch_chat(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let settings = read_chat_settings(db, user_id)?;
    let Some(s) = settings else {
        return Ok(json!({ "message": "No chat settings configured yet" }));
    };
    // Pull the exact fields v4 lists, in v4's literal object key order. Each is a
    // field of the hydrated settings object; a `.nullable().optional()` field
    // (`timezone`) is `undefined` (omitted) when NULL, which `field_or_undefined`
    // reproduces (the key is dropped from the output object).
    let mut obj = Map::new();
    obj.insert(
        "tokenDisplaySettings".into(),
        field(&s, "tokenDisplaySettings"),
    );
    obj.insert(
        "contextCompressionSettings".into(),
        field(&s, "contextCompressionSettings"),
    );
    obj.insert(
        "memoryCascadePreferences".into(),
        field(&s, "memoryCascadePreferences"),
    );
    obj.insert(
        "defaultTimestampConfig".into(),
        field(&s, "defaultTimestampConfig"),
    );
    obj.insert("agentModeSettings".into(), field(&s, "agentModeSettings"));
    obj.insert(
        "dangerousContentSettings".into(),
        field(&s, "dangerousContentSettings"),
    );
    obj.insert("autoDetectRng".into(), field(&s, "autoDetectRng"));
    obj.insert("llmLoggingSettings".into(), field(&s, "llmLoggingSettings"));
    obj.insert("avatarDisplayMode".into(), field(&s, "avatarDisplayMode"));
    obj.insert("avatarDisplayStyle".into(), field(&s, "avatarDisplayStyle"));
    // `timezone` is nullable-optional: v4 emits `settings.timezone` directly, so
    // when the hydrated object omits it (NULL), the key is `undefined` → dropped.
    put_field_if_present(&mut obj, &s, "timezone");
    Ok(Value::Object(obj))
}

// --- connections --------------------------------------------------------------

/// v4 `case 'connections'`: `{ count, profiles: sanitizeConnectionProfile[] }`.
fn fetch_connections(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let profiles = db.read_main(|conn| read_connection_profiles(conn, user_id))?;
    Ok(json!({
        "count": profiles.len(),
        "profiles": Value::Array(profiles),
    }))
}

/// v4 `sanitizeConnectionProfile` — the allowlisted safe fields, in literal key
/// order. The `?? default` fallbacks apply v4's post-hydration defaults; but
/// every referenced field is a `.default(...)` schema field (always
/// materialized) except `baseUrl` (`.nullable().optional()` → `|| null`). So:
/// materialized-default fields carry their stored value; `baseUrl` is the stored
/// string or `null`.
fn read_connection_profiles(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, provider, modelName, baseUrl, allowToolUse, pseudoToolMode, \
                allowWebSearch, useNativeWebSearch, isDefault, sortIndex \
         FROM connection_profiles WHERE userId = ?1",
    )?;
    let rows = stmt.query_map([user_id], |r| {
        let mut obj = Map::new();
        obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        obj.insert("name".into(), Value::String(r.get::<_, String>(1)?));
        obj.insert("provider".into(), Value::String(r.get::<_, String>(2)?));
        obj.insert("modelName".into(), Value::String(r.get::<_, String>(3)?));
        // `baseUrl || null` — nullable → the stored string or JSON null.
        obj.insert(
            "baseUrl".into(),
            match r.get::<_, Option<String>>(4)? {
                Some(s) => Value::String(s),
                None => Value::Null,
            },
        );
        obj.insert("allowToolUse".into(), Value::Bool(r.get::<_, i64>(5)? == 1));
        obj.insert(
            "pseudoToolMode".into(),
            Value::String(r.get::<_, String>(6)?),
        );
        obj.insert(
            "allowWebSearch".into(),
            Value::Bool(r.get::<_, i64>(7)? == 1),
        );
        obj.insert(
            "useNativeWebSearch".into(),
            Value::Bool(r.get::<_, i64>(8)? == 1),
        );
        obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(9)? == 1));
        obj.insert(
            "sortIndex".into(),
            crate::db::js_number_to_json(r.get::<_, f64>(10)?),
        );
        Ok(Value::Object(obj))
    })?;
    collect(rows)
}

// --- embeddings ---------------------------------------------------------------

/// v4 `case 'embeddings'`: `{ count, profiles: sanitizeEmbeddingProfile[] }`.
fn fetch_embeddings(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let profiles = db.read_main(|conn| read_embedding_profiles(conn, user_id))?;
    Ok(json!({
        "count": profiles.len(),
        "profiles": Value::Array(profiles),
    }))
}

/// v4 `sanitizeEmbeddingProfile`: `{ id, name, provider, modelName, dimensions,
/// isDefault ?? false }`. `dimensions` is `.nullable().optional()` and read
/// directly (no `|| null`): when NULL the hydrated object omits it, so the key is
/// `undefined` → dropped by `JSON.stringify`.
fn read_embedding_profiles(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, provider, modelName, dimensions, isDefault \
         FROM embedding_profiles WHERE userId = ?1",
    )?;
    let rows = stmt.query_map([user_id], |r| {
        let mut obj = Map::new();
        obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        obj.insert("name".into(), Value::String(r.get::<_, String>(1)?));
        obj.insert("provider".into(), Value::String(r.get::<_, String>(2)?));
        obj.insert("modelName".into(), Value::String(r.get::<_, String>(3)?));
        // `dimensions`: present (number) or OMITTED when NULL (undefined dropped).
        if let Some(d) = r.get::<_, Option<f64>>(4)? {
            obj.insert("dimensions".into(), crate::db::js_number_to_json(d));
        }
        obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(5)? == 1));
        Ok(Value::Object(obj))
    })?;
    collect(rows)
}

// --- images -------------------------------------------------------------------

/// v4 `case 'images'`: `{ count, profiles: sanitizeImageProfile[],
/// storyBackgroundsSettings: settings?.storyBackgroundsSettings || null }`.
fn fetch_images(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let profiles = db.read_main(|conn| read_image_profiles(conn, user_id))?;
    let settings = read_chat_settings(db, user_id)?;
    // `settings?.storyBackgroundsSettings || null` — a materialized-default object
    // (truthy) when a settings row exists, else `null`.
    let story = settings
        .as_ref()
        .and_then(|s| s.get("storyBackgroundsSettings").cloned())
        .unwrap_or(Value::Null);
    Ok(json!({
        "count": profiles.len(),
        "profiles": Value::Array(profiles),
        "storyBackgroundsSettings": story,
    }))
}

/// v4 `sanitizeImageProfile`: `{ id, name, provider, modelName, isDefault ??
/// false }`.
fn read_image_profiles(conn: &rusqlite::Connection, user_id: &str) -> Result<Vec<Value>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, provider, modelName, isDefault \
         FROM image_profiles WHERE userId = ?1",
    )?;
    let rows = stmt.query_map([user_id], |r| {
        let mut obj = Map::new();
        obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        obj.insert("name".into(), Value::String(r.get::<_, String>(1)?));
        obj.insert("provider".into(), Value::String(r.get::<_, String>(2)?));
        obj.insert("modelName".into(), Value::String(r.get::<_, String>(3)?));
        obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(4)? == 1));
        Ok(Value::Object(obj))
    })?;
    collect(rows)
}

// --- appearance ---------------------------------------------------------------

/// v4 `case 'appearance'`: theme + avatar + tag styles + sidebar width, each
/// `settings?.X || null`. When a settings row exists, the `.default(...)` fields
/// are truthy (so `|| null` is a no-op); `tagStyles` defaults to `{}` — an EMPTY
/// object is truthy in JS, so it stays `{}` (not `null`). When there is no
/// settings row every field is `null`.
fn fetch_appearance(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let settings = read_chat_settings(db, user_id)?;
    let get = |key: &str| -> Value {
        settings
            .as_ref()
            .and_then(|s| s.get(key).cloned())
            .unwrap_or(Value::Null)
    };
    let mut obj = Map::new();
    obj.insert("themePreference".into(), or_null(get("themePreference")));
    obj.insert(
        "avatarDisplayMode".into(),
        or_null(get("avatarDisplayMode")),
    );
    obj.insert(
        "avatarDisplayStyle".into(),
        or_null(get("avatarDisplayStyle")),
    );
    obj.insert("tagStyles".into(), or_null(get("tagStyles")));
    obj.insert("sidebarWidth".into(), or_null(get("sidebarWidth")));
    Ok(Value::Object(obj))
}

// --- templates ----------------------------------------------------------------

/// v4 `case 'templates'`: `{ count, templates: {id,name,description,isDefault}[],
/// defaultRoleplayTemplateId: settings?.defaultRoleplayTemplateId || null }`.
fn fetch_templates(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let templates = db.read_main(|conn| read_roleplay_templates(conn, user_id))?;
    let settings = read_chat_settings(db, user_id)?;
    // `settings?.defaultRoleplayTemplateId || null` — nullable-optional; present
    // as a string only when set on the settings row.
    let default_id = settings
        .as_ref()
        .and_then(|s| s.get("defaultRoleplayTemplateId").cloned())
        .map(or_null)
        .unwrap_or(Value::Null);
    Ok(json!({
        "count": templates.len(),
        "templates": Value::Array(templates),
        "defaultRoleplayTemplateId": default_id,
    }))
}

/// v4's template mapper: `{ id, name, description, isDefault }`. But
/// `RoleplayTemplateSchema` has NO `isDefault` field, so `t.isDefault` is always
/// `undefined` → dropped. `description` is `.nullable().optional()`: present when
/// stored, omitted when NULL (undefined dropped).
fn read_roleplay_templates(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<Vec<Value>, DbError> {
    let mut stmt =
        conn.prepare("SELECT id, name, description FROM roleplay_templates WHERE userId = ?1")?;
    let rows = stmt.query_map([user_id], |r| {
        let mut obj = Map::new();
        obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        obj.insert("name".into(), Value::String(r.get::<_, String>(1)?));
        // `description`: present or omitted (undefined dropped). `isDefault` is
        // never a schema field → always omitted.
        if let Some(d) = r.get::<_, Option<String>>(2)? {
            obj.insert("description".into(), Value::String(d));
        }
        Ok(Value::Object(obj))
    })?;
    collect(rows)
}

// --- system -------------------------------------------------------------------

/// v4 `case 'system'`: `{ llmLoggingSettings: settings?.llmLoggingSettings ||
/// null, timezone: settings?.timezone || null }`.
fn fetch_system(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let settings = read_chat_settings(db, user_id)?;
    let logging = settings
        .as_ref()
        .and_then(|s| s.get("llmLoggingSettings").cloned())
        .map(or_null)
        .unwrap_or(Value::Null);
    let tz = settings
        .as_ref()
        .and_then(|s| s.get("timezone").cloned())
        .map(or_null)
        .unwrap_or(Value::Null);
    Ok(json!({
        "llmLoggingSettings": logging,
        "timezone": tz,
    }))
}

// --- overview -----------------------------------------------------------------

/// v4 `case 'overview'`: counts + distinct-provider sets + a few settings fields.
fn fetch_overview(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let (conn_providers, conn_count) =
        db.read_main(|c| provider_set(c, "connection_profiles", user_id))?;
    let (emb_providers, emb_count) =
        db.read_main(|c| provider_set(c, "embedding_profiles", user_id))?;
    let (img_providers, img_count) =
        db.read_main(|c| provider_set(c, "image_profiles", user_id))?;
    let tmpl_count = db.read_main(|c| count_rows(c, "roleplay_templates", user_id))?;
    let settings = read_chat_settings(db, user_id)?;

    // `settings?.themePreference || 'default'` — the object when a row exists,
    // else the string `'default'`.
    let theme = settings
        .as_ref()
        .and_then(|s| s.get("themePreference").cloned())
        .map(|v| or_default_string(v, "default"))
        .unwrap_or_else(|| Value::String("default".to_string()));
    let agent = settings
        .as_ref()
        .and_then(|s| s.get("agentModeSettings").cloned())
        .map(or_null)
        .unwrap_or(Value::Null);
    let compression = settings
        .as_ref()
        .and_then(|s| s.get("contextCompressionSettings").cloned())
        .map(or_null)
        .unwrap_or(Value::Null);

    Ok(json!({
        "connectionProfiles": { "count": conn_count, "providers": Value::Array(conn_providers) },
        "embeddingProfiles": { "count": emb_count, "providers": Value::Array(emb_providers) },
        "imageProfiles": { "count": img_count, "providers": Value::Array(img_providers) },
        "roleplayTemplates": { "count": tmpl_count },
        "theme": theme,
        "agentMode": agent,
        "contextCompression": compression,
    }))
}

/// `[...new Set(rows.map(r => r.provider))]` — distinct providers in
/// first-occurrence order (JS `Set` preserves insertion order), plus the row
/// count. Rows come back in the same (no-ORDER-BY) order on both sides.
fn provider_set(
    conn: &rusqlite::Connection,
    table: &str,
    user_id: &str,
) -> Result<(Vec<Value>, i64), DbError> {
    let sql = format!("SELECT provider FROM {table} WHERE userId = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let providers: Vec<String> = stmt
        .query_map([user_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let count = providers.len() as i64;
    let mut seen: Vec<String> = Vec::new();
    for p in providers {
        if !seen.contains(&p) {
            seen.push(p);
        }
    }
    Ok((seen.into_iter().map(Value::String).collect(), count))
}

/// `rows.length` for a user-scoped table.
fn count_rows(conn: &rusqlite::Connection, table: &str, user_id: &str) -> Result<i64, DbError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE userId = ?1");
    let n: i64 = conn.query_row(&sql, [user_id], |r| r.get(0))?;
    Ok(n)
}

// --- shared helpers -----------------------------------------------------------

/// Read the user's full `chat_settings` row (or `None`).
fn read_chat_settings(db: &Db, user_id: &str) -> Result<Option<Value>, DbError> {
    db.read_main(|conn| crate::db::chat_settings::find_by_user_id(conn, user_id))
}

/// A materialized field of the hydrated settings object (always present here).
fn field(settings: &Value, key: &str) -> Value {
    settings.get(key).cloned().unwrap_or(Value::Null)
}

/// Insert `settings[key]` into `obj` only if the key is present (v4 emits
/// `settings.X` directly, so an absent/`undefined` field is dropped by
/// `JSON.stringify`). Used for the nullable-optional `timezone` in the `chat`
/// category, which v4 references without a `|| null` fallback.
fn put_field_if_present(obj: &mut Map<String, Value>, settings: &Value, key: &str) {
    if let Some(v) = settings.get(key) {
        obj.insert(key.to_string(), v.clone());
    }
}

/// Reproduce JS `x || null`: a JS-falsy value (`null`, `false`, `0`, `""`) →
/// `null`; anything else (incl. an empty object/array, which are truthy) → `x`.
fn or_null(v: Value) -> Value {
    if is_js_falsy(&v) {
        Value::Null
    } else {
        v
    }
}

/// Reproduce JS `x || 'default'` (used by overview's `theme`).
fn or_default_string(v: Value, default: &str) -> Value {
    if is_js_falsy(&v) {
        Value::String(default.to_string())
    } else {
        v
    }
}

/// JS falsiness for the value kinds these fields can hold: `null`, `false`, the
/// number `0`, and the empty string. Objects and arrays (incl. empty ones) are
/// truthy in JS.
fn is_js_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// Collect a rusqlite mapped-row iterator into a `Vec<Value>`.
fn collect(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Value>>,
) -> Result<Vec<Value>, DbError> {
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ============================================================================
// help_navigate
// ============================================================================

/// Allowed URL path prefixes (v4 `ALLOWED_PATH_PREFIXES`).
const ALLOWED_PATH_PREFIXES: [&str; 7] = [
    "/settings",
    "/aurora",
    "/salon",
    "/prospero",
    "/profile",
    "/files",
    "/setup",
];

/// v4 `HelpNavigateToolOutput` — `{ success, url, error? }`.
#[derive(Debug, Clone, Serialize)]
pub struct HelpNavigateOutput {
    pub success: bool,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The exact v4 validation-failure message.
const HELP_NAVIGATE_INVALID: &str =
    "Invalid input: url is required and must be a valid internal Quilltap path starting with /";

/// v4 `validateHelpNavigateInput` — the Zod refinements on `url`:
///   `.min(1)` → non-empty; `.refine(trim().length > 0)` → not whitespace-only;
///   `.refine(startsWith('/'))`; `.refine(prefix allowlist on the pre-`?` path)`.
/// Returns the URL when valid.
fn validate_help_navigate_input(args: &Value) -> Option<String> {
    let url = args.as_object()?.get("url")?.as_str()?;
    // `.min(1)`: a zero-length string fails.
    if url.is_empty() {
        return None;
    }
    // `.refine(val.trim().length > 0)`: JS `String.prototype.trim`.
    if crate::jsstr::js_trim(url).is_empty() {
        return None;
    }
    // `.refine(val.startsWith('/'))`.
    if !url.starts_with('/') {
        return None;
    }
    // `.refine(...)` on the pre-`?` pathname against the prefix allowlist.
    let pathname = url.split('?').next().unwrap_or(url);
    if ALLOWED_PATH_PREFIXES
        .iter()
        .any(|p| pathname.starts_with(p))
    {
        Some(url.to_string())
    } else {
        None
    }
}

/// Execute the `help_navigate` tool (pure — validates the URL and echoes it back
/// for the client to navigate). v4 `executeHelpNavigateTool`.
pub fn execute_help_navigate(_user_id: &str, args: &Value) -> HelpNavigateOutput {
    match validate_help_navigate_input(args) {
        Some(url) => HelpNavigateOutput {
            success: true,
            url,
            error: None,
        },
        None => HelpNavigateOutput {
            success: false,
            url: String::new(),
            error: Some(HELP_NAVIGATE_INVALID.to_string()),
        },
    }
}

/// Format the `help_navigate` result — v4 `formatHelpNavigateResults`.
pub fn format_help_navigate(out: &HelpNavigateOutput) -> String {
    if !out.success {
        return out
            .error
            .clone()
            .unwrap_or_else(|| "Failed to navigate.".to_string());
    }
    format!("Navigation initiated to: {}", out.url)
}

// ============================================================================
// submit_final_response
// ============================================================================

/// v4 `SubmitFinalResponseToolOutput` — `{ success, message, finalResponse?,
/// summary?, confidence? }`.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitFinalResponseOutput {
    pub success: bool,
    pub message: String,
    #[serde(rename = "finalResponse", skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// `confidence` renders the JS way: an integer-valued double (e.g. `1.0`,
    /// `0.0`) serializes bare (`1`, `0`), matching `JSON.stringify(confidence)`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    pub confidence: Option<f64>,
}

/// Serialize an `Option<f64>` the JS way (an integer-valued double renders bare).
/// Only invoked when the value is `Some` (guarded by `skip_serializing_if`).
fn ser_opt_js_number<S: serde::Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(v.expect("skip_serializing_if guards None")).serialize(s)
}

/// The exact v4 validation-failure message.
const SUBMIT_INVALID: &str =
    "Invalid input: response parameter is required and must be a non-empty string";

/// The valid parsed input of `submit_final_response`.
struct SubmitFinalResponseInput {
    response: String,
    summary: Option<String>,
    confidence: Option<f64>,
}

/// v4 `validateSubmitFinalResponseInput` — the Zod object:
///   `response: z.string()` (required — but the handler's failure message says
///     "non-empty string"; the schema itself is `z.string()` with no `.min(1)`,
///     so an EMPTY string PASSES validation. The message wording is aspirational;
///     validation only requires a string. Reproduced faithfully.);
///   `summary: z.string().optional()`;
///   `confidence: z.number().min(0).max(1).optional()`.
/// Returns the parsed input when valid.
fn validate_submit_final_response_input(args: &Value) -> Option<SubmitFinalResponseInput> {
    let obj = args.as_object()?;
    // `response` must be present and a string (empty allowed — no `.min(1)`).
    let response = obj.get("response")?.as_str()?.to_string();
    // `summary` optional: absent → None; present must be a string, else fail.
    let summary = match obj.get("summary") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    // `confidence` optional: absent → None; present must be a number in [0,1].
    let confidence = match obj.get("confidence") {
        None => None,
        Some(v) => {
            let n = v.as_f64()?;
            if !(0.0..=1.0).contains(&n) {
                return None;
            }
            Some(n)
        }
    };
    Some(SubmitFinalResponseInput {
        response,
        summary,
        confidence,
    })
}

/// Execute the `submit_final_response` tool (pure — validates and returns the
/// final response for the orchestrator; no side effects). v4
/// `executeSubmitFinalResponseTool`. The `chat_id` is v4's logging context.
pub fn execute_submit_final_response(_chat_id: &str, args: &Value) -> SubmitFinalResponseOutput {
    match validate_submit_final_response_input(args) {
        Some(input) => SubmitFinalResponseOutput {
            success: true,
            message: "Final response submitted successfully".to_string(),
            final_response: Some(input.response),
            summary: input.summary,
            confidence: input.confidence,
        },
        None => SubmitFinalResponseOutput {
            success: false,
            message: SUBMIT_INVALID.to_string(),
            final_response: None,
            summary: None,
            confidence: None,
        },
    }
}

/// Format the `submit_final_response` result — v4 `formatSubmitFinalResponseResults`.
///
/// On success: `Final response submitted.` optionally + ` Summary: <summary>` +
/// ` Confidence: <Math.round(confidence*100)>%`. On failure: `Error: <message>`.
pub fn format_submit_final_response(out: &SubmitFinalResponseOutput) -> String {
    if !out.success {
        return format!("Error: {}", out.message);
    }
    let mut output = String::from("Final response submitted.");
    if let Some(summary) = &out.summary {
        // v4: `if (result.summary)` — a JS-falsy summary (empty string) is skipped.
        if !summary.is_empty() {
            output.push_str(&format!(" Summary: {summary}"));
        }
    }
    if let Some(confidence) = out.confidence {
        // v4: `if (result.confidence !== undefined)` — present (incl. 0) appends.
        let pct = js_math_round(confidence * 100.0);
        output.push_str(&format!(" Confidence: {pct}%"));
    }
    output
}

/// JS `Math.round`: round half toward +∞. For the `[0,100]` domain of
/// `confidence * 100` (confidence ∈ [0,1]) every value is non-negative, so this
/// equals Rust's `f64::round` (half away from zero); implemented as `(x +
/// 0.5).floor()` to match `Math.round`'s exact tie rule regardless of sign of
/// the input in this domain.
fn js_math_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}
