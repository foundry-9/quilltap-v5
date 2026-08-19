//! The embedding-profiles repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`,
//! `roleplay_templates`, `image_profiles`, and `connection_profiles`. Ports v4's
//! `lib/database/repositories/embedding-profiles.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo), plus the custom query helpers `findDefault`, `findByName`, and
//! `unsetAllDefaults` (the last two added by P4.9H2A for the management route
//! family — the create/PUT duplicate-name probe and the default flip). v4's
//! `update` strips `id` and `createdAt` before `_update`, which is a no-op for
//! this port since we preserve both anyway. There is **no built-in guard**
//! (unlike `prompt_templates`).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `embedding_profiles` extends v4's `TaggableBaseRepository`, so it carries the
//! **Taggable lineage** (`image_profiles` introduced): a user-scoped `userId`
//! plus a JSON **`tags` array** column — the same `Vec<String>` → compact JSON
//! text (`["id"]`, `[]`) shape. It widens the surface with:
//!
//!   - **two nullable REAL number columns** (`dimensions`,
//!     `truncateToDimensions`) — `dimensions` is a bare
//!     `z.number().nullable().optional()`, and `truncateToDimensions` is
//!     `z.number().int().positive().nullable().optional()` (min only, no max).
//!     v4's schema translator (`mapToSQLiteType`) maps both to **REAL** (INTEGER
//!     affinity requires BOTH an integer min AND max). They are bound as
//!     `Option<f64>`: `None` → SQL NULL, `Some(n)` → an 8-byte float. An
//!     integer-valued REAL (e.g. `1536.0`) renders back as `1536` in the
//!     canonical dump via [`super::js_number_to_json`], matching v4's
//!     better-sqlite3 → `JSON.stringify` path byte-for-byte (the
//!     `provider_models` `contextWindow`/`maxOutputTokens` precedent).
//!   - **two boolean columns** (`normalizeL2` default `true`, `isDefault` default
//!     `false`) → INTEGER 0/1 (`i64::from(bool)`, the `tags.quickHide` mapping).
//!     Both have Zod defaults but are modeled as `bool` and set explicitly in
//!     every corpus row (no reliance on defaults).
//!   - two more **nullable string columns** (`apiKeyId`, `baseUrl`) →
//!     `Option<String>` (`None` → SQL NULL).
//!   - an **enum TEXT column** (`provider`,
//!     `z.enum(['OPENAI','OLLAMA','OPENROUTER','BUILTIN'])`) — stored as plain
//!     text.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations`/`provider_models`/`image_profiles` use.
//!
//! The four nullable columns (`apiKeyId`, `baseUrl`, `dimensions`,
//! `truncateToDimensions`) ride a **tri-state** [`EpUpdate`] field
//! (`Option<Option<T>>`, the `image_profiles` `IpUpdate` precedent): `None` skips
//! the column, `Some(None)` sets it to SQL NULL (the PUT's explicit
//! `truncateToDimensions: null` / `apiKeyId: null`), `Some(Some(v))` sets the
//! value. This is what the P4.9H2A PUT trigger matrix's clear-to-null arm needs.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection, OptionalExtension};

use super::DbError;

/// The subset of an `embedding_profiles` row the BUILTIN embedding + refit paths
/// and the API embedding path consume (v4 `repos.embeddingProfiles.findById` /
/// `findDefault`). Scoped read, not the full net marshaling: `provider` gates the
/// refit / dispatch; `api_key_id` / `base_url` / `model_name` / `dimensions` feed
/// the API wire (`generateApiEmbedding`); `truncate_to_dimensions` /
/// `normalize_l2` feed `applyEmbeddingProfile`.
#[derive(Debug, Clone)]
pub struct EmbeddingProfileRow {
    pub id: String,
    pub user_id: String,
    /// `OPENAI` / `OLLAMA` / `OPENROUTER` / `BUILTIN`.
    pub provider: String,
    /// The connection api-key id (`None` when the column is NULL).
    pub api_key_id: Option<String>,
    /// The profile's base URL override (`None` when the column is NULL).
    pub base_url: Option<String>,
    /// The embedding model (e.g. `text-embedding-3-small`).
    pub model_name: String,
    /// The provider-requested output dimensions (`None` when NULL; a falsy `0`
    /// is dropped by the API path, matching v4's `profile.dimensions ||
    /// undefined`).
    pub dimensions: Option<f64>,
    /// The Matryoshka slice target (`None` when the column is NULL).
    pub truncate_to_dimensions: Option<f64>,
    /// v4 `applyEmbeddingProfile` uses `normalizeL2 !== false` — so a NULL column
    /// resolves to `true` here (matching `!== false`).
    pub normalize_l2: bool,
}

const EP_ROW_COLUMNS: &str = "id, userId, provider, apiKeyId, baseUrl, modelName, \
     dimensions, truncateToDimensions, normalizeL2";

fn marshal_profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmbeddingProfileRow> {
    Ok(EmbeddingProfileRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        provider: row.get(2)?,
        api_key_id: row.get::<_, Option<String>>(3)?,
        base_url: row.get::<_, Option<String>>(4)?,
        model_name: row.get(5)?,
        dimensions: row.get::<_, Option<f64>>(6)?,
        truncate_to_dimensions: row.get::<_, Option<f64>>(7)?,
        normalize_l2: row
            .get::<_, Option<i64>>(8)?
            .map(|v| v != 0)
            .unwrap_or(true),
    })
}

/// Read an embedding profile by id (v4 `findById`), or `None` when absent.
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<EmbeddingProfileRow>, DbError> {
    conn.query_row(
        &format!("SELECT {EP_ROW_COLUMNS} FROM embedding_profiles WHERE id = ?1"),
        params![id],
        marshal_profile_row,
    )
    .optional()
    .map_err(Into::into)
}

/// Read the user's default embedding profile (v4 `findDefault` =
/// `findOneByFilter({ userId, isDefault: true })`), or `None` when the user has
/// no default. v4's `findOne` carries no ORDER BY; a well-formed instance holds
/// at most one default per user.
pub fn find_default(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<EmbeddingProfileRow>, DbError> {
    conn.query_row(
        &format!(
            "SELECT {EP_ROW_COLUMNS} FROM embedding_profiles \
             WHERE userId = ?1 AND isDefault = 1 LIMIT 1"
        ),
        params![user_id],
        marshal_profile_row,
    )
    .optional()
    .map_err(Into::into)
}

/// The profile id the cold-chunk re-embed and the conversation-render
/// reconcile pick: the **marked default, and only that** (v4 `d553f72a`
/// dropped the `|| embeddingProfiles[0]` fallback at all five of its sites).
///
/// One embedding standard per instance: a fallback to an arbitrary profile
/// mixes vector spaces, and in the reconcile's case it would exclude FAILED
/// rows under a profile nothing embeds with any more. With no default marked,
/// the caller waits — that is the intended state, not an error.
///
/// `findAll` is UNSCOPED (all users) and carries no `ORDER BY`; the `rowid ASC`
/// here is now only about picking deterministically among (malformed)
/// multiple defaults. Returns `None` when no profile is marked default.
pub fn pick_reembed_profile_id(conn: &Connection) -> Result<Option<String>, DbError> {
    let mut stmt =
        conn.prepare("SELECT id, isDefault FROM embedding_profiles ORDER BY rowid ASC")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows
        .iter()
        .find(|(_, is_default)| *is_default != 0)
        .map(|(id, _)| id.clone()))
}

/// The default profile's `(id, name)` — the memories embedding-status route reads
/// `defaultProfile?.name` (the scoped [`EmbeddingProfileRow`] omits `name`).
pub fn find_default_id_name(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<(String, String)>, DbError> {
    conn.query_row(
        "SELECT id, name FROM embedding_profiles WHERE userId = ?1 AND isDefault = 1 LIMIT 1",
        params![user_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

/// v4 `repos.embeddingProfiles.findByName(userId, name)` — the user-scoped
/// duplicate probe (`findOneByFilter({ userId, name })`). Returns the matching
/// profile as the full net-read shape (v4 returns the whole `EmbeddingProfile`),
/// or `None`. The create/PUT routes read `.id` off the result (existence for
/// create's 409; `dup.id !== id` for the PUT 409).
pub fn find_by_name(
    conn: &Connection,
    user_id: &str,
    name: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    conn.query_row(
        &format!(
            "SELECT {EP_FULL_COLUMNS} FROM embedding_profiles \
             WHERE userId = ?1 AND name = ?2 LIMIT 1"
        ),
        params![user_id, name],
        marshal_ep_full_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// Fields for creating an embedding profile (the `Omit<EmbeddingProfile,'id'|
/// timestamps>` shape). `api_key_id`/`base_url` are the nullable string columns;
/// `dimensions`/`truncate_to_dimensions` are the nullable REAL columns;
/// `normalize_l2`/`is_default` are the bool→INTEGER 0/1 columns; `tags` is the
/// JSON array column.
pub struct EpCreate {
    pub user_id: String,
    pub name: String,
    /// One of `OPENAI` / `OLLAMA` / `OPENROUTER` / `BUILTIN` (enum TEXT).
    pub provider: String,
    /// `None` => SQL NULL (the `.nullable().optional()` column absent).
    pub api_key_id: Option<String>,
    /// `None` => SQL NULL.
    pub base_url: Option<String>,
    pub model_name: String,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL (integer-valued collapses in
    /// the dump).
    pub dimensions: Option<f64>,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL.
    pub truncate_to_dimensions: Option<f64>,
    pub normalize_l2: bool,
    pub is_default: bool,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An embedding-profile update patch. Mirrors v4 `update` over `_update`:
/// provided fields overwrite, id and createdAt are preserved (v4 deletes them off
/// the patch; we never touch them), `updatedAt` is set explicitly. Each `Some`
/// field sets that column; clearing a nullable column to NULL is deferred (see
/// header).
#[derive(Default)]
pub struct EpUpdate {
    pub name: Option<String>,
    pub provider: Option<String>,
    /// The nullable-clearing tri-state (P4.9H2A): `None` skips the column,
    /// `Some(None)` sets it to SQL NULL (v4 PUT's explicit `apiKeyId: null`),
    /// `Some(Some(v))` sets the value.
    pub api_key_id: Option<Option<String>>,
    /// The nullable-clearing tri-state — v4 PUT's `baseUrl: baseUrl || null`.
    pub base_url: Option<Option<String>>,
    pub model_name: Option<String>,
    /// The nullable-clearing tri-state — v4 PUT's explicit `dimensions: null`.
    pub dimensions: Option<Option<f64>>,
    /// The nullable-clearing tri-state — v4 PUT's explicit
    /// `truncateToDimensions: null` (the matrix's clear-to-null arm).
    pub truncate_to_dimensions: Option<Option<f64>>,
    pub normalize_l2: Option<bool>,
    pub is_default: Option<bool>,
    /// Re-serialized to compact JSON text when provided.
    pub tags: Option<Vec<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct EmbeddingProfilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> EmbeddingProfilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert an embedding profile with the given pinned id + timestamps. The
    /// REAL columns bind `Option<f64>`; the bool columns bind `i64::from(bool)`;
    /// `tags` → compact JSON array text; `apiKeyId`/`baseUrl` as `Option<String>`
    /// (`None` → SQL NULL).
    pub fn create(&self, data: &EpCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO embedding_profiles \
               (id, userId, name, provider, apiKeyId, baseUrl, modelName, dimensions, \
                truncateToDimensions, normalizeL2, isDefault, tags, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                opts.id,
                data.user_id,
                data.name,
                data.provider,
                data.api_key_id,
                data.base_url,
                data.model_name,
                data.dimensions,
                data.truncate_to_dimensions,
                i64::from(data.normalize_l2),
                i64::from(data.is_default),
                tags_json,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the embedding profile `id`. Returns `Ok(false)`
    /// when no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &EpUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
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
        if let Some(api_key_id) = &patch.api_key_id {
            // `Option<String>` binds NULL for `None`, the value for `Some`.
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
        if let Some(dimensions) = &patch.dimensions {
            assignments.push(format!("dimensions = ?{}", values.len() + 1));
            values.push(Box::new(*dimensions));
        }
        if let Some(truncate_to_dimensions) = &patch.truncate_to_dimensions {
            assignments.push(format!("truncateToDimensions = ?{}", values.len() + 1));
            values.push(Box::new(*truncate_to_dimensions));
        }
        if let Some(normalize_l2) = patch.normalize_l2 {
            assignments.push(format!("normalizeL2 = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(normalize_l2)));
        }
        if let Some(is_default) = patch.is_default {
            assignments.push(format!("isDefault = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_default)));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Internal(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE embedding_profiles SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the embedding profile `id`. Returns `false` when no row matched
    /// (v4's `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM embedding_profiles WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `unsetAllDefaults(userId)` — `updateMany({userId, isDefault:true},
    /// {isDefault:false})`. The base `updateMany` injects a fresh `updatedAt` into
    /// every matched row's `$set`, so this mints `updated_at` on all flipped rows
    /// (one wall-clock stamp for the whole `UPDATE`, matching v4's single
    /// `getCurrentTimestamp()`). Returns the modified-row count (v4's
    /// `modifiedCount`). Mirrors the `image_profiles` `unset_all_defaults`.
    pub fn unset_all_defaults(&self, user_id: &str, updated_at: &str) -> Result<usize, DbError> {
        let n = self.conn.execute(
            "UPDATE embedding_profiles SET isDefault = 0, updatedAt = ?1 \
             WHERE userId = ?2 AND isDefault = 1",
            params![updated_at, user_id],
        )?;
        Ok(n)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM embedding_profiles WHERE id = ?1",
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

// ============================================================================
// The net-read (full v4 shape) view — P4.9G4
// ============================================================================

/// Every column an `EmbeddingProfile` carries, in `EmbeddingProfileSchema`
/// declaration order (v4 `lib/schemas/profile.types.ts:172`). The scoped
/// [`EmbeddingProfileRow`] deliberately omits `name`/`isDefault`/`tags`/the
/// timestamps; the `.qtap` export needs the whole row.
const EP_FULL_COLUMNS: &str = "id, userId, name, provider, apiKeyId, baseUrl, modelName, \
     dimensions, truncateToDimensions, normalizeL2, isDefault, tags, createdAt, updatedAt";

/// Marshal one `embedding_profiles` row (selected in [`EP_FULL_COLUMNS`] order)
/// into the v4 net-read shape: `.nullable().optional()` columns are OMITTED when
/// SQL NULL (v4's Zod parse drops `undefined`), bools coerced from INTEGER, and
/// `tags` parsed (`NULL`/empty → `[]`, the Zod default).
fn marshal_ep_full_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
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
    if let Some(d) = r.get::<_, Option<f64>>(7)? {
        obj.insert("dimensions".into(), super::js_number_to_json(d));
    }
    if let Some(d) = r.get::<_, Option<f64>>(8)? {
        obj.insert("truncateToDimensions".into(), super::js_number_to_json(d));
    }
    obj.insert("normalizeL2".into(), Value::Bool(r.get::<_, i64>(9)? == 1));
    obj.insert("isDefault".into(), Value::Bool(r.get::<_, i64>(10)? == 1));
    obj.insert(
        "tags".into(),
        match r.get::<_, Option<String>>(11)? {
            Some(raw) if !raw.is_empty() => {
                serde_json::from_str(&raw).unwrap_or_else(|_| Value::Array(Vec::new()))
            }
            _ => Value::Array(Vec::new()),
        },
    );
    obj.insert("createdAt".into(), Value::String(r.get::<_, String>(12)?));
    obj.insert("updatedAt".into(), Value::String(r.get::<_, String>(13)?));
    Ok(Value::Object(obj))
}

/// v4 `repos.embeddingProfiles.findById(id)` as the full net-read shape.
pub fn find_full_json_by_id(
    conn: &Connection,
    id: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    conn.query_row(
        &format!("SELECT {EP_FULL_COLUMNS} FROM embedding_profiles WHERE id = ?1"),
        params![id],
        marshal_ep_full_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `repos.embeddingProfiles.findByUserId(userId)` as the full net-read shape —
/// UNSORTED (v4's `findByFilter({userId})` carries no ORDER BY; the management
/// list route applies the default-first + createdAt-DESC sort itself).
pub fn find_by_user_id(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EP_FULL_COLUMNS} FROM embedding_profiles WHERE userId = ?1"
    ))?;
    let rows = stmt.query_map(params![user_id], marshal_ep_full_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// v4 `repos.embeddingProfiles.findAll()` as the full net-read shape — insertion
/// (rowid) order, matching v4's unsorted `collection.find({})`.
pub fn find_all_full_json(conn: &Connection) -> Result<Vec<serde_json::Value>, DbError> {
    let mut stmt = conn.prepare(&format!("SELECT {EP_FULL_COLUMNS} FROM embedding_profiles"))?;
    let rows = stmt.query_map([], marshal_ep_full_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `embedding_profiles` table for the SQL-shape unit tests (the
    /// differential fixtures build the real DDL via v4's `ensureCollection`).
    fn open_scratch() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE embedding_profiles (
                id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT,
                apiKeyId TEXT, baseUrl TEXT, modelName TEXT, dimensions REAL,
                truncateToDimensions REAL, normalizeL2 INTEGER, isDefault INTEGER,
                tags TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
        conn
    }

    fn seed(conn: &Connection, id: &str, user: &str, name: &str, is_default: bool) {
        EmbeddingProfilesRepository::new(conn)
            .create(
                &EpCreate {
                    user_id: user.into(),
                    name: name.into(),
                    provider: "OPENAI".into(),
                    api_key_id: Some("key-1".into()),
                    base_url: Some("https://x".into()),
                    model_name: "text-embedding-3-small".into(),
                    dimensions: Some(1536.0),
                    truncate_to_dimensions: Some(512.0),
                    normalize_l2: true,
                    is_default,
                    tags: vec![],
                },
                &CreateOptions {
                    id: id.into(),
                    created_at: "2026-01-01T00:00:00.000Z".into(),
                    updated_at: "2026-01-01T00:00:00.000Z".into(),
                },
            )
            .unwrap();
    }

    #[test]
    fn unset_all_defaults_scopes_to_user_and_stamps_updated_at() {
        let conn = open_scratch();
        seed(&conn, "p1", "userA", "A default", true);
        seed(&conn, "p2", "userA", "A other", false);
        seed(&conn, "p3", "userB", "B default", true);

        let n = EmbeddingProfilesRepository::new(&conn)
            .unset_all_defaults("userA", "2026-05-05T00:00:00.000Z")
            .unwrap();
        // Only userA's one default row is modified.
        assert_eq!(n, 1);

        let row = |id: &str| -> (i64, String) {
            conn.query_row(
                "SELECT isDefault, updatedAt FROM embedding_profiles WHERE id = ?1",
                params![id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .unwrap()
        };
        // p1 flipped to 0 with the new stamp; p2 untouched; p3 (userB) untouched.
        assert_eq!(row("p1"), (0, "2026-05-05T00:00:00.000Z".into()));
        assert_eq!(row("p2"), (0, "2026-01-01T00:00:00.000Z".into()));
        assert_eq!(row("p3"), (1, "2026-01-01T00:00:00.000Z".into()));
    }

    #[test]
    fn update_tri_state_clears_nullable_columns_to_null() {
        let conn = open_scratch();
        seed(&conn, "p1", "userA", "A", false);

        let patch = EpUpdate {
            api_key_id: Some(None),
            base_url: Some(None),
            dimensions: Some(None),
            truncate_to_dimensions: Some(None),
            updated_at: "2026-06-06T00:00:00.000Z".into(),
            ..Default::default()
        };
        assert!(EmbeddingProfilesRepository::new(&conn)
            .update("p1", &patch)
            .unwrap());

        let (aki, bu, dim, trunc): (Option<String>, Option<String>, Option<f64>, Option<f64>) =
            conn.query_row(
                "SELECT apiKeyId, baseUrl, dimensions, truncateToDimensions \
                 FROM embedding_profiles WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((aki, bu, dim, trunc), (None, None, None, None));
    }

    #[test]
    fn find_by_name_is_user_scoped() {
        let conn = open_scratch();
        seed(&conn, "p1", "userA", "Shared", false);
        seed(&conn, "p2", "userB", "Shared", false);

        let a = find_by_name(&conn, "userA", "Shared").unwrap().unwrap();
        assert_eq!(a["id"], serde_json::json!("p1"));
        assert!(find_by_name(&conn, "userC", "Shared").unwrap().is_none());
        assert!(find_by_name(&conn, "userA", "Nope").unwrap().is_none());
    }
}
