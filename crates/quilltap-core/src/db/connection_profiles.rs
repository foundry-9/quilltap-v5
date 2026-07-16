//! The connection-profiles repository — the workhorse profile repo, a Phase-2
//! repo port after `folders`, `tags`, `text_replacement_rules`,
//! `prompt_templates`, `conversation_annotations`, and `provider_models`. Ports
//! v4's `lib/database/repositories/connection-profiles.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` for the main `connection_profiles`
//! entity (the three abstract methods over the base repo). The `*ApiKey` helpers
//! operate on a *different* table (`api_keys`) and are out of scope; the token-
//! usage counters' `$inc` helpers are not CRUD ops and are out of scope too.
//! There is no built-in guard here — the three ops delegate straight to the base
//! repo's `_create`/`_update`/`_delete`.
//!
//! This is by far the **widest marshaling surface** the tier-2 ports have hit:
//! ~29 columns spanning every cell shape the port has met so far —
//!
//!   - **three enum TEXT columns** (`provider`, `transport`, `pseudoToolMode`) —
//!     v4's `ProviderEnum` is `z.string().min(1)` and the latter two are
//!     `z.enum([...])`, all lowering to plain TEXT. Bound as `String`, no
//!     validation here (the corpus supplies valid enum members).
//!   - **MANY boolean columns** → INTEGER 0/1 (`i64::from(bool)`):
//!     `courierDeltaMode`, `isDefault`, `isCheap`, `allowWebSearch`,
//!     `useNativeWebSearch`, `allowToolUse`, `isDangerousCompatible`,
//!     `supportsImageUpload`. Every one is bound explicitly.
//!   - **two nullable REAL int-override columns** (`maxContext`, `maxTokens`) —
//!     both `z.number().int().positive().nullable().optional()`. `.positive()`
//!     is a *min only* (no max), and v4's schema translator (`mapToSQLiteType`)
//!     maps a number to INTEGER only when it has BOTH an integer min AND max, so
//!     these are **REAL**. Bound as `Option<f64>`: `None` → SQL NULL, `Some(n)` →
//!     an 8-byte float. An integer-valued REAL (e.g. `128000.0`) renders back as
//!     `128000` in the canonical dump via [`super::js_number_to_json`], matching
//!     v4's better-sqlite3 → `JSON.stringify` path byte-for-byte.
//!   - **five REAL token-counter columns** — `sortIndex`, `totalTokens`,
//!     `totalPromptTokens`, `totalCompletionTokens`, `messageCount` are all bare
//!     `z.number().default(0)` (no min/max) → **REAL**. Bound as `f64`; the same
//!     integer-collapse applies in the dump.
//!   - **three nullable string columns** (`apiKeyId`, `baseUrl`, `modelClass`) →
//!     `Option<String>`; `None` → SQL NULL.
//!   - the **`tags` JSON array column** (`z.array(UUIDSchema)`) — `Vec<String>` →
//!     compact JSON text via `serde_json::to_string`, as `prompt_templates` did
//!     (order-preserving, no key-order subtlety).
//!   - the **`parameters` open-JSON object column** (`JsonSchema.default({})`,
//!     i.e. `z.record(z.string(), z.unknown())`). Modeled as a
//!     `serde_json::Value` → `serde_json::to_string`, which is key-order-faithful:
//!     this crate enables serde_json's `preserve_order`, so a `Value::Object` is
//!     an `IndexMap` emitting *insertion* order exactly as v4's `JSON.stringify`
//!     does. (This was once a tracked deferred seam on the assumption that
//!     `Value` sorts its keys — `preserve_order` closed it.) The corpus still
//!     constrains `parameters` to `{}` or a single-key object, but only because
//!     nothing in it needs more; a multi-key object would marshal correctly.
//!
//! To avoid relying on Zod create-time defaults (`transport`/`pseudoToolMode`/
//! `courierDeltaMode`/`allowToolUse` and the many `false`/`0` defaults), the
//! tier-2 corpus supplies every column explicitly on create.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/`provider_models`
//! use.
//!
//! Deferred (not in the corpus): setting a nullable column **to NULL** via
//! `update` (the patch models a provided field as "set to this value"; clearing a
//! column to NULL lands when an op needs it), Zod's create-time defaults, and the
//! open-JSON multi-key `parameters` key-order seam above.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a connection profile (the `Omit<ConnectionProfile,'id'|
/// timestamps>` shape) — every persisted column in schema (on-disk) order.
pub struct CpCreate {
    pub user_id: String,
    pub name: String,
    /// Enum TEXT (`ProviderEnum`).
    pub provider: String,
    /// Enum TEXT (`'api' | 'courier'`).
    pub transport: String,
    pub courier_delta_mode: bool,
    /// `None` => SQL NULL.
    pub api_key_id: Option<String>,
    /// `None` => SQL NULL.
    pub base_url: Option<String>,
    pub model_name: String,
    /// Open-JSON object → compact JSON text. CONSTRAINED to `{}` / single-key
    /// (see the module header's key-order deferral).
    pub parameters: serde_json::Value,
    pub is_default: bool,
    pub is_cheap: bool,
    pub allow_web_search: bool,
    pub use_native_web_search: bool,
    pub allow_tool_use: bool,
    /// Enum TEXT (`'auto' | 'native' | 'simple-json' | 'text-block'`).
    pub pseudo_tool_mode: String,
    /// `None` => SQL NULL.
    pub model_class: Option<String>,
    /// Nullable REAL int-override; `None` => SQL NULL.
    pub max_context: Option<f64>,
    /// Nullable REAL int-override; `None` => SQL NULL.
    pub max_tokens: Option<f64>,
    pub is_dangerous_compatible: bool,
    pub supports_image_upload: bool,
    /// JSON array column → compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
    pub sort_index: f64,
    pub total_tokens: f64,
    pub total_prompt_tokens: f64,
    pub total_completion_tokens: f64,
    pub message_count: f64,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A connection-profile update patch. Mirrors v4 `update` over `_update`:
/// provided fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. A sensible subset of the updatable columns is exposed — the
/// corpus exercises name, the nullable REAL overrides, a few booleans, the token
/// counters, the `tags` array, and `sortIndex`. Each `Some` field sets that
/// column; clearing a nullable column to NULL is deferred (not in the corpus).
#[derive(Default)]
pub struct CpUpdate {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub transport: Option<String>,
    pub courier_delta_mode: Option<bool>,
    pub base_url: Option<String>,
    /// Set `baseUrl` to SQL NULL (v4's courier gate / an explicit `baseUrl: null`).
    /// Wins over [`Self::base_url`] when true (the route never sets both).
    pub clear_base_url: bool,
    /// Set `apiKeyId` — the route PUT can set a value (`Some(Some(id))`), clear it
    /// (`Some(None)` → SQL NULL, the courier gate / explicit null), or leave it
    /// (`None`). A double-`Option` so NULL is expressible (the base field is not
    /// exercised NULL by the Phase-2 corpus).
    pub api_key_id: Option<Option<String>>,
    /// Open-JSON `parameters` object (the route PUT can replace it). CONSTRAINED to
    /// `{}` / single-key by the same open-JSON key-order seam as `create`.
    pub parameters: Option<serde_json::Value>,
    pub model_name: Option<String>,
    pub is_default: Option<bool>,
    pub is_cheap: Option<bool>,
    pub allow_web_search: Option<bool>,
    pub use_native_web_search: Option<bool>,
    pub allow_tool_use: Option<bool>,
    pub pseudo_tool_mode: Option<String>,
    pub model_class: Option<String>,
    /// Set `modelClass` to SQL NULL (the route PUT's `modelClass: null|''`). Wins
    /// over [`Self::model_class`] when true.
    pub clear_model_class: bool,
    pub max_context: Option<f64>,
    /// Set `maxContext` to SQL NULL (the route PUT's `maxContext: null|''|0`). Wins
    /// over [`Self::max_context`] when true.
    pub clear_max_context: bool,
    pub max_tokens: Option<f64>,
    pub is_dangerous_compatible: Option<bool>,
    pub supports_image_upload: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub sort_index: Option<f64>,
    pub total_tokens: Option<f64>,
    pub total_prompt_tokens: Option<f64>,
    pub total_completion_tokens: Option<f64>,
    pub message_count: Option<f64>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ConnectionProfilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ConnectionProfilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a connection profile with the given pinned id + timestamps. All ~29
    /// columns are written explicitly in schema order; the REAL columns bind
    /// `f64`/`Option<f64>`, the boolean columns bind `i64::from(bool)`, the two
    /// JSON columns bind compact JSON text, the enums bind plain `String`.
    pub fn create(&self, data: &CpCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let parameters_json = serde_json::to_string(&data.parameters)
            .map_err(|e| DbError::Key(format!("parameters serialize: {e}")))?;
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO connection_profiles \
               (id, userId, name, provider, transport, courierDeltaMode, apiKeyId, baseUrl, \
                modelName, parameters, isDefault, isCheap, allowWebSearch, useNativeWebSearch, \
                allowToolUse, pseudoToolMode, modelClass, maxContext, maxTokens, \
                isDangerousCompatible, supportsImageUpload, tags, sortIndex, totalTokens, \
                totalPromptTokens, totalCompletionTokens, messageCount, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.provider,
                data.transport,
                i64::from(data.courier_delta_mode),
                data.api_key_id,
                data.base_url,
                data.model_name,
                parameters_json,
                i64::from(data.is_default),
                i64::from(data.is_cheap),
                i64::from(data.allow_web_search),
                i64::from(data.use_native_web_search),
                i64::from(data.allow_tool_use),
                data.pseudo_tool_mode,
                data.model_class,
                data.max_context,
                data.max_tokens,
                i64::from(data.is_dangerous_compatible),
                i64::from(data.supports_image_upload),
                tags_json,
                data.sort_index,
                data.total_tokens,
                data.total_prompt_tokens,
                data.total_completion_tokens,
                data.message_count,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the profile `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    pub fn update(&self, id: &str, patch: &CpUpdate) -> Result<bool, DbError> {
        // v4 `_findById`: the row must exist or the update is a no-op (-> null).
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(provider) = &patch.provider {
            assignments.push(format!("provider = ?{}", values.len() + 1));
            values.push(Box::new(provider.clone()));
        }
        if let Some(transport) = &patch.transport {
            assignments.push(format!("transport = ?{}", values.len() + 1));
            values.push(Box::new(transport.clone()));
        }
        if let Some(courier_delta_mode) = patch.courier_delta_mode {
            assignments.push(format!("courierDeltaMode = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(courier_delta_mode)));
        }
        if patch.clear_base_url {
            assignments.push("baseUrl = NULL".to_string());
        } else if let Some(base_url) = &patch.base_url {
            assignments.push(format!("baseUrl = ?{}", values.len() + 1));
            values.push(Box::new(base_url.clone()));
        }
        if let Some(api_key_id) = &patch.api_key_id {
            match api_key_id {
                Some(id) => {
                    assignments.push(format!("apiKeyId = ?{}", values.len() + 1));
                    values.push(Box::new(id.clone()));
                }
                None => assignments.push("apiKeyId = NULL".to_string()),
            }
        }
        if let Some(parameters) = &patch.parameters {
            let json = serde_json::to_string(parameters)
                .map_err(|e| DbError::Key(format!("parameters serialize: {e}")))?;
            assignments.push(format!("parameters = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(model_name) = &patch.model_name {
            assignments.push(format!("modelName = ?{}", values.len() + 1));
            values.push(Box::new(model_name.clone()));
        }
        if let Some(is_default) = patch.is_default {
            assignments.push(format!("isDefault = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_default)));
        }
        if let Some(is_cheap) = patch.is_cheap {
            assignments.push(format!("isCheap = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_cheap)));
        }
        if let Some(allow_web_search) = patch.allow_web_search {
            assignments.push(format!("allowWebSearch = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(allow_web_search)));
        }
        if let Some(use_native_web_search) = patch.use_native_web_search {
            assignments.push(format!("useNativeWebSearch = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(use_native_web_search)));
        }
        if let Some(allow_tool_use) = patch.allow_tool_use {
            assignments.push(format!("allowToolUse = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(allow_tool_use)));
        }
        if let Some(pseudo_tool_mode) = &patch.pseudo_tool_mode {
            assignments.push(format!("pseudoToolMode = ?{}", values.len() + 1));
            values.push(Box::new(pseudo_tool_mode.clone()));
        }
        if patch.clear_model_class {
            assignments.push("modelClass = NULL".to_string());
        } else if let Some(model_class) = &patch.model_class {
            assignments.push(format!("modelClass = ?{}", values.len() + 1));
            values.push(Box::new(model_class.clone()));
        }
        if patch.clear_max_context {
            assignments.push("maxContext = NULL".to_string());
        } else if let Some(max_context) = patch.max_context {
            assignments.push(format!("maxContext = ?{}", values.len() + 1));
            values.push(Box::new(max_context));
        }
        if let Some(max_tokens) = patch.max_tokens {
            assignments.push(format!("maxTokens = ?{}", values.len() + 1));
            values.push(Box::new(max_tokens));
        }
        if let Some(is_dangerous_compatible) = patch.is_dangerous_compatible {
            assignments.push(format!("isDangerousCompatible = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_dangerous_compatible)));
        }
        if let Some(supports_image_upload) = patch.supports_image_upload {
            assignments.push(format!("supportsImageUpload = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(supports_image_upload)));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        if let Some(sort_index) = patch.sort_index {
            assignments.push(format!("sortIndex = ?{}", values.len() + 1));
            values.push(Box::new(sort_index));
        }
        if let Some(total_tokens) = patch.total_tokens {
            assignments.push(format!("totalTokens = ?{}", values.len() + 1));
            values.push(Box::new(total_tokens));
        }
        if let Some(total_prompt_tokens) = patch.total_prompt_tokens {
            assignments.push(format!("totalPromptTokens = ?{}", values.len() + 1));
            values.push(Box::new(total_prompt_tokens));
        }
        if let Some(total_completion_tokens) = patch.total_completion_tokens {
            assignments.push(format!("totalCompletionTokens = ?{}", values.len() + 1));
            values.push(Box::new(total_completion_tokens));
        }
        if let Some(message_count) = patch.message_count {
            assignments.push(format!("messageCount = ?{}", values.len() + 1));
            values.push(Box::new(message_count));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE connection_profiles SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the profile `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM connection_profiles WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM connection_profiles WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}

/// **Scoped read** for the participant resolver
/// ([`crate::services::participant_resolver`]): a connection profile by id
/// (v4 `repos.connections.findById(id)`), marshaled as a `serde_json::Value` in
/// v4's net read shape. `None` when no row exists (the resolver then throws
/// `"Connection profile not found"`). This is the READ half of a Phase-2 repo
/// that already ships the `create`/`update`/`delete` write half; the participant
/// resolver is the first consumer of a full profile row, so the read marshaling
/// lands here alongside it (scoped read precedent:
/// [`super::chat_settings::find_auto_housekeeping_settings_by_user_id`]).
///
/// Net read shape (matching [`super::chats_read`]/[`super::characters_read`]):
///   * nullable-optional columns (`apiKeyId`, `baseUrl`, `modelClass`,
///     `maxContext`, `maxTokens`) are **OMITTED** when SQL NULL (v4's
///     `undefined` dropped by `JSON.stringify`), and the two REAL int-overrides
///     JS-render (integer-valued → bare) when present;
///   * `.default(...)` columns (the booleans, the numbers, the enums,
///     `transport`/`pseudoToolMode`, `parameters`→`{}`, `tags`→`[]`) are
///     materialized — but since every v4-written row already carries the
///     Zod-parsed value on disk, the stored cell is simply rendered;
///   * booleans coerce INTEGER 0/1 → bool; numbers JS-render; `parameters` /
///     `tags` parse straight to JSON. `parameters` multi-key key order is the
///     same tracked open-JSON seam as the write half (corpus keeps it `{}` /
///     single-key).
///
/// The decrypted API key stays a **host-side deferred seam** (mirroring
/// [`crate::services::cheap_llm_exec`]): the resolver returns this profile row
/// (which carries `apiKeyId` when set); actually fetching + decrypting the key
/// from the `api_keys` table is the host's job.
/// The shared column list every connection-profile net read selects (net-read
/// order = [`marshal_cp_row`]'s field access order).
const CP_COLUMNS: &str = "id, userId, name, provider, transport, courierDeltaMode, apiKeyId, \
     baseUrl, modelName, parameters, isDefault, isCheap, allowWebSearch, useNativeWebSearch, \
     allowToolUse, pseudoToolMode, modelClass, maxContext, maxTokens, isDangerousCompatible, \
     supportsImageUpload, tags, sortIndex, totalTokens, totalPromptTokens, totalCompletionTokens, \
     messageCount, createdAt, updatedAt";

// Insert a nullable-optional TEXT value: `Some` → string, `None` → omit.
fn put_opt_string(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    v: Option<String>,
) {
    if let Some(s) = v {
        obj.insert(key.to_string(), serde_json::Value::String(s));
    }
}
// Insert a nullable-optional number column (`NULL` → omit, else JS render).
fn put_opt_number(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str, v: Option<f64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), super::js_number_to_json(n));
    }
}
// A `.default(default)` array column: parsed array, or `[]` on NULL/empty/bad.
fn array_or_empty(v: Option<String>) -> serde_json::Value {
    match v {
        Some(raw) if !raw.is_empty() => {
            serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()))
        }
        _ => serde_json::Value::Array(Vec::new()),
    }
}
// A `.default({})` open-JSON object column: parsed, or `{}` on NULL/empty/bad.
fn object_or_empty(v: Option<String>) -> serde_json::Value {
    match v {
        Some(raw) if !raw.is_empty() => serde_json::from_str(&raw)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new())),
        _ => serde_json::Value::Object(serde_json::Map::new()),
    }
}

/// Marshal one `connection_profiles` row selected in [`CP_COLUMNS`] order into
/// the net-read [`serde_json::Value`].
fn marshal_cp_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    use serde_json::{Map, Value};
    let mut obj = Map::new();
    obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
    obj.insert("userId".into(), Value::String(r.get::<_, String>(1)?));
    obj.insert("name".into(), Value::String(r.get::<_, String>(2)?));
    obj.insert("provider".into(), Value::String(r.get::<_, String>(3)?));
    obj.insert("transport".into(), Value::String(r.get::<_, String>(4)?));
    obj.insert(
        "courierDeltaMode".into(),
        Value::Bool(r.get::<_, i64>(5)? == 1),
    );
    put_opt_string(&mut obj, "apiKeyId", r.get::<_, Option<String>>(6)?);
    put_opt_string(&mut obj, "baseUrl", r.get::<_, Option<String>>(7)?);
    obj.insert("modelName".into(), Value::String(r.get::<_, String>(8)?));
    obj.insert(
        "parameters".into(),
        object_or_empty(r.get::<_, Option<String>>(9)?),
    );
    obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(10)? == 1));
    obj.insert("isCheap".into(), Value::Bool(r.get::<_, i64>(11)? == 1));
    obj.insert(
        "allowWebSearch".into(),
        Value::Bool(r.get::<_, i64>(12)? == 1),
    );
    obj.insert(
        "useNativeWebSearch".into(),
        Value::Bool(r.get::<_, i64>(13)? == 1),
    );
    obj.insert(
        "allowToolUse".into(),
        Value::Bool(r.get::<_, i64>(14)? == 1),
    );
    obj.insert(
        "pseudoToolMode".into(),
        Value::String(r.get::<_, String>(15)?),
    );
    put_opt_string(&mut obj, "modelClass", r.get::<_, Option<String>>(16)?);
    put_opt_number(&mut obj, "maxContext", r.get::<_, Option<f64>>(17)?);
    put_opt_number(&mut obj, "maxTokens", r.get::<_, Option<f64>>(18)?);
    obj.insert(
        "isDangerousCompatible".into(),
        Value::Bool(r.get::<_, i64>(19)? == 1),
    );
    obj.insert(
        "supportsImageUpload".into(),
        Value::Bool(r.get::<_, i64>(20)? == 1),
    );
    obj.insert(
        "tags".into(),
        array_or_empty(r.get::<_, Option<String>>(21)?),
    );
    obj.insert(
        "sortIndex".into(),
        super::js_number_to_json(r.get::<_, f64>(22)?),
    );
    obj.insert(
        "totalTokens".into(),
        super::js_number_to_json(r.get::<_, f64>(23)?),
    );
    obj.insert(
        "totalPromptTokens".into(),
        super::js_number_to_json(r.get::<_, f64>(24)?),
    );
    obj.insert(
        "totalCompletionTokens".into(),
        super::js_number_to_json(r.get::<_, f64>(25)?),
    );
    obj.insert(
        "messageCount".into(),
        super::js_number_to_json(r.get::<_, f64>(26)?),
    );
    obj.insert("createdAt".into(), Value::String(r.get::<_, String>(27)?));
    obj.insert("updatedAt".into(), Value::String(r.get::<_, String>(28)?));
    Ok(Value::Object(obj))
}

/// Reproduce v4's `repos.connections.create` RETURN shape from a [`find_by_id`]
/// re-read. v4's `handleCreate` create-input explicitly sets `apiKeyId`/`baseUrl`/
/// `modelClass`/`maxContext` (as `X || null`), so the parsed create-return keeps
/// them present (null when null) — whereas the re-read OMITS a NULL
/// nullable-optional column. `maxTokens` is NOT in the create input, so it stays
/// omitted (the re-read agrees). Rebuilds the object in CP schema order, filling
/// the four create-input-nullable keys with the re-read value or `null`.
pub fn create_return_shape(reread: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    let empty = Map::new();
    let src = reread.as_object().unwrap_or(&empty);
    let take = |k: &str| src.get(k).cloned();
    // The four create-input nullable keys: present in the create-return (null when
    // absent from the re-read).
    let nullable = |k: &str| take(k).unwrap_or(Value::Null);

    let mut out = Map::new();
    for key in [
        "id",
        "userId",
        "name",
        "provider",
        "transport",
        "courierDeltaMode",
    ] {
        if let Some(v) = take(key) {
            out.insert(key.to_string(), v);
        }
    }
    out.insert("apiKeyId".into(), nullable("apiKeyId"));
    out.insert("baseUrl".into(), nullable("baseUrl"));
    for key in [
        "modelName",
        "parameters",
        "isDefault",
        "isCheap",
        "allowWebSearch",
        "useNativeWebSearch",
        "allowToolUse",
        "pseudoToolMode",
    ] {
        if let Some(v) = take(key) {
            out.insert(key.to_string(), v);
        }
    }
    out.insert("modelClass".into(), nullable("modelClass"));
    out.insert("maxContext".into(), nullable("maxContext"));
    for key in [
        "isDangerousCompatible",
        "supportsImageUpload",
        "tags",
        "sortIndex",
        "totalTokens",
        "totalPromptTokens",
        "totalCompletionTokens",
        "messageCount",
        "createdAt",
        "updatedAt",
    ] {
        if let Some(v) = take(key) {
            out.insert(key.to_string(), v);
        }
    }
    Value::Object(out)
}

/// Net read for one profile by id (see the module net-read shape doc above).
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<serde_json::Value>, DbError> {
    let row = conn
        .query_row(
            &format!("SELECT {CP_COLUMNS} FROM connection_profiles WHERE id = ?1"),
            params![id],
            marshal_cp_row,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// v4 `repos.connections.findAll()` — every profile, in insertion (rowid) order,
/// matching v4's `collection.find({})` with no sort. Used by the dangerous-content
/// provider-routing scan for `isDangerousCompatible` profiles.
pub fn find_all(conn: &Connection) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!("SELECT {CP_COLUMNS} FROM connection_profiles"))?;
    let rows = stmt.query_map([], marshal_cp_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// v4 `repos.connections.findDefault(userId)` — the user's default profile
/// (`findOneByFilter({ userId, isDefault: true })`, first match in rowid order).
/// `None` when the user has no default. Used by the Carina profile-resolution
/// chain (`services::carina_query`).
pub fn find_default(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    let row = conn
        .query_row(
            &format!(
                "SELECT {CP_COLUMNS} FROM connection_profiles \
                 WHERE userId = ?1 AND isDefault = 1 LIMIT 1"
            ),
            params![user_id],
            marshal_cp_row,
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// v4 `repos.connections.findByUserId(userId)` — profiles for one user, in
/// insertion (rowid) order (v4 `findByFilter({ userId })` with no sort).
pub fn find_by_user_id(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CP_COLUMNS} FROM connection_profiles WHERE userId = ?1"
    ))?;
    let rows = stmt.query_map(params![user_id], marshal_cp_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
