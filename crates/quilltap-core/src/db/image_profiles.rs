//! The image-profiles repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, and `conversation_annotations`.
//! Ports v4's `lib/database/repositories/image-profiles.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom query helpers — `findByName`, `findDefault`,
//! `unsetAllDefaults` — are out of scope here. v4's `update` strips `id` and
//! `createdAt` before `_update`, which is a no-op for this port since we preserve
//! both anyway. There is **no built-in guard** (unlike `prompt_templates`).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `image_profiles` extends v4's `TaggableBaseRepository`, so it banks the
//! **Taggable lineage**: a user-scoped `userId` plus a JSON **`tags` array**
//! column — the same `Vec<String>` → compact JSON text (`["id"]`, `[]`) shape
//! `prompt_templates` introduced. It widens the surface with:
//!
//!   - the first **open / arbitrary-JSON object column** (`parameters`,
//!     `JsonSchema.default({})` = `z.record(z.string(), z.unknown())`). Because
//!     it has no fixed shape, it can't be a typed struct (the precedent
//!     `tags.visualStyle` set). It is modeled as a [`serde_json::Value`] and
//!     stored via `serde_json::to_string`, mirroring v4's `JSON.stringify`. The
//!     empty default `{}` and a single-key object (e.g. `{"steps":30}`,
//!     `{"guidanceScale":7.5}`) serialize identically on both sides; the
//!     fractional value exercises `js_number_to_json`'s float pass-through inside
//!     the JSON text.
//!   - two **boolean columns** (`isDefault`, `isDangerousCompatible`) → INTEGER
//!     0/1 (the `tags.quickHide` mapping), and two more **nullable string
//!     columns** (`apiKeyId`, `baseUrl`) → `None` → SQL NULL.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations` use.
//!
//! The `parameters` open-JSON column is key-order-faithful: this crate enables
//! serde_json's `preserve_order`, so a `Value::Object` is an `IndexMap` emitting
//! INSERTION order, exactly as v4's `JSON.stringify` does. (This was once tracked
//! as the open-JSON multi-key key-order seam, on the assumption that `Value`
//! sorts its keys; `preserve_order` closed it.) The corpus stays constrained to
//! `{}` or single-key objects because nothing needs more.
//!
//! Deferred (not in the corpus, mirroring `prompt_templates`): setting a nullable
//! column (`apiKeyId`, `baseUrl`) **to NULL** via `update` — the patch models a
//! provided field as "set to this value", so a nullable setter lands when an op
//! needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating an image profile (the `Omit<ImageProfile,'id'|
/// timestamps>` shape). `parameters` is the open-JSON object column (bound as
/// compact JSON text); `tags` is the JSON array column; the two `Option` string
/// fields are the nullable columns; the two bools land as INTEGER 0/1.
pub struct IpCreate {
    pub user_id: String,
    pub name: String,
    pub provider: String,
    /// `None` => SQL NULL (the `.nullable().optional()` column absent).
    pub api_key_id: Option<String>,
    /// `None` => SQL NULL.
    pub base_url: Option<String>,
    pub model_name: String,
    /// Open/arbitrary JSON object (`{}` or a single-key object — see the module
    /// header's multi-key key-order deferral). Stored via `serde_json::to_string`.
    pub parameters: serde_json::Value,
    pub is_default: bool,
    pub is_dangerous_compatible: bool,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An image-profile update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved (v4 deletes them off the
/// patch; we never touch them), `updatedAt` is set explicitly. Each `Some` field
/// sets that column; clearing a nullable column to NULL is deferred (see header).
#[derive(Default)]
pub struct IpUpdate {
    pub name: Option<String>,
    pub provider: Option<String>,
    /// The nullable-clearing tri-state (P4.6p): `None` skips the column;
    /// `Some(None)` sets it to SQL NULL (v4 PUT's explicit `apiKeyId: null`);
    /// `Some(Some(v))` sets the value.
    pub api_key_id: Option<Option<String>>,
    /// The nullable-clearing tri-state — v4 PUT's `baseUrl: baseUrl || null`.
    pub base_url: Option<Option<String>>,
    pub model_name: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub parameters: Option<serde_json::Value>,
    pub is_default: Option<bool>,
    pub is_dangerous_compatible: Option<bool>,
    /// Re-serialized to compact JSON text when provided.
    pub tags: Option<Vec<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ImageProfilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ImageProfilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert an image profile with the given pinned id + timestamps. `parameters`
    /// → compact JSON text; `tags` → compact JSON array text; bools → INTEGER 0/1;
    /// `apiKeyId`/`baseUrl` as `Option<String>` (`None` → SQL NULL).
    pub fn create(&self, data: &IpCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let parameters_json = serde_json::to_string(&data.parameters)
            .map_err(|e| DbError::Key(format!("parameters serialize: {e}")))?;
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO image_profiles \
               (id, userId, name, provider, apiKeyId, baseUrl, modelName, parameters, \
                isDefault, isDangerousCompatible, tags, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.provider,
                data.api_key_id,
                data.base_url,
                data.model_name,
                parameters_json,
                i64::from(data.is_default),
                i64::from(data.is_dangerous_compatible),
                tags_json,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the image profile `id`. Returns `Ok(false)` when
    /// no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &IpUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM image_profiles WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .map(|_| true)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })?;
        if !exists {
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
        if let Some(api_key_id) = &patch.api_key_id {
            assignments.push(format!("apiKeyId = ?{}", values.len() + 1));
            values.push(Box::new(api_key_id.clone()));
        }
        if let Some(base_url) = &patch.base_url {
            assignments.push(format!("baseUrl = ?{}", values.len() + 1));
            values.push(Box::new(base_url.clone()));
        }
        if let Some(model_name) = &patch.model_name {
            assignments.push(format!("modelName = ?{}", values.len() + 1));
            values.push(Box::new(model_name.clone()));
        }
        if let Some(parameters) = &patch.parameters {
            let parameters_json = serde_json::to_string(parameters)
                .map_err(|e| DbError::Key(format!("parameters serialize: {e}")))?;
            assignments.push(format!("parameters = ?{}", values.len() + 1));
            values.push(Box::new(parameters_json));
        }
        if let Some(is_default) = patch.is_default {
            assignments.push(format!("isDefault = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_default)));
        }
        if let Some(is_dangerous_compatible) = patch.is_dangerous_compatible {
            assignments.push(format!("isDangerousCompatible = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_dangerous_compatible)));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE image_profiles SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the image profile `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM image_profiles WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `unsetAllDefaults(userId)` — `updateMany({userId, isDefault:true},
    /// {isDefault:false})`. The base `updateMany` injects a fresh `updatedAt` into
    /// every matched row's `$set`, so this mints `updated_at` on all flipped rows
    /// (one wall-clock stamp for the whole `UPDATE`, matching v4's single
    /// `getCurrentTimestamp()`). Returns the modified-row count.
    pub fn unset_all_defaults(&self, user_id: &str, updated_at: &str) -> Result<usize, DbError> {
        let n = self.conn.execute(
            "UPDATE image_profiles SET isDefault = 0, updatedAt = ?1 \
             WHERE userId = ?2 AND isDefault = 1",
            params![updated_at, user_id],
        )?;
        Ok(n)
    }
}

/// v4 `repos.imageProfiles.findByUserId(userId)` — every profile for the user,
/// UNSORTED (the route applies the default-first + createdAt-DESC sort, then the
/// optional `sortByCharacter` re-sort).
pub fn find_by_user_id(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {IP_COLUMNS} FROM image_profiles WHERE userId = ?1"
    ))?;
    let rows = stmt.query_map(params![user_id], marshal_ip_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// v4 `repos.imageProfiles.findByName(userId, name)` — the user-scoped duplicate
/// probe. Returns the matching profile's id (the routes need existence /
/// self-comparison), or `None`.
pub fn find_id_by_user_and_name(
    conn: &Connection,
    user_id: &str,
    name: &str,
) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT id FROM image_profiles WHERE userId = ?1 AND name = ?2 LIMIT 1",
        params![user_id, name],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// The shared column list every image-profile net read selects.
const IP_COLUMNS: &str = "id, userId, name, provider, apiKeyId, baseUrl, modelName, parameters, \
     isDefault, isDangerousCompatible, tags, createdAt, updatedAt";

/// Marshal one `image_profiles` row (selected in [`IP_COLUMNS`] order) into the
/// net-read [`serde_json::Value`] — nullable `apiKeyId`/`baseUrl` omitted when
/// SQL NULL, bools coerced, `parameters`/`tags` parsed.
fn marshal_ip_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    use serde_json::{Map, Value};
    let mut obj = Map::new();
    obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
    obj.insert("userId".into(), Value::String(r.get::<_, String>(1)?));
    obj.insert("name".into(), Value::String(r.get::<_, String>(2)?));
    obj.insert("provider".into(), Value::String(r.get::<_, String>(3)?));
    if let Some(s) = r.get::<_, Option<String>>(4)? {
        obj.insert("apiKeyId".into(), Value::String(s));
    }
    if let Some(s) = r.get::<_, Option<String>>(5)? {
        obj.insert("baseUrl".into(), Value::String(s));
    }
    obj.insert("modelName".into(), Value::String(r.get::<_, String>(6)?));
    obj.insert(
        "parameters".into(),
        match r.get::<_, Option<String>>(7)? {
            Some(raw) if !raw.is_empty() => {
                serde_json::from_str(&raw).unwrap_or_else(|_| Value::Object(Map::new()))
            }
            _ => Value::Object(Map::new()),
        },
    );
    obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(8)? == 1));
    obj.insert(
        "isDangerousCompatible".into(),
        Value::Bool(r.get::<_, i64>(9)? == 1),
    );
    obj.insert(
        "tags".into(),
        match r.get::<_, Option<String>>(10)? {
            Some(raw) if !raw.is_empty() => {
                serde_json::from_str(&raw).unwrap_or_else(|_| Value::Array(Vec::new()))
            }
            _ => Value::Array(Vec::new()),
        },
    );
    obj.insert("createdAt".into(), Value::String(r.get::<_, String>(11)?));
    obj.insert("updatedAt".into(), Value::String(r.get::<_, String>(12)?));
    Ok(Value::Object(obj))
}

/// v4 `repos.imageProfiles.findById(id)` — one image profile by id (net read),
/// or `None`.
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<serde_json::Value>, DbError> {
    conn.query_row(
        &format!("SELECT {IP_COLUMNS} FROM image_profiles WHERE id = ?1"),
        params![id],
        marshal_ip_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `repos.imageProfiles.findAll()` — every image profile, insertion (rowid)
/// order (v4's `collection.find({})` with no sort). Used by the dangerous-content
/// image-provider-routing scan.
pub fn find_all(conn: &Connection) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!("SELECT {IP_COLUMNS} FROM image_profiles"))?;
    let rows = stmt.query_map([], marshal_ip_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
