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
//!     `serde_json::Value` → `serde_json::to_string`. **TRACKED DEFERRED SEAM
//!     (open-JSON multi-key key order):** `serde_json::Value` (an object backed by
//!     `BTreeMap`) SORTS its keys, whereas v4's `JSON.stringify` preserves
//!     *insertion* order. For a `{}` or single-key object the two agree
//!     trivially, so the corpus CONSTRAINS every `parameters` value to `{}` or a
//!     single-key object. A multi-key open-JSON column needs an insertion-order-
//!     preserving serialization (a `Vec<(String, Value)>` or an
//!     `IndexMap`-backed value) before this column can carry arbitrary objects —
//!     close this before real data with multi-key `parameters`.
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
    pub model_name: Option<String>,
    pub is_default: Option<bool>,
    pub is_cheap: Option<bool>,
    pub allow_web_search: Option<bool>,
    pub use_native_web_search: Option<bool>,
    pub allow_tool_use: Option<bool>,
    pub pseudo_tool_mode: Option<String>,
    pub model_class: Option<String>,
    pub max_context: Option<f64>,
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
        if let Some(base_url) = &patch.base_url {
            assignments.push(format!("baseUrl = ?{}", values.len() + 1));
            values.push(Box::new(base_url.clone()));
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
        if let Some(model_class) = &patch.model_class {
            assignments.push(format!("modelClass = ?{}", values.len() + 1));
            values.push(Box::new(model_class.clone()));
        }
        if let Some(max_context) = patch.max_context {
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
