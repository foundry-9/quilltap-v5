//! The embedding-profiles repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`,
//! `roleplay_templates`, `image_profiles`, and `connection_profiles`. Ports v4's
//! `lib/database/repositories/embedding-profiles.repository.ts` (+ the
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
//! Deferred (not in the corpus, mirroring `image_profiles`/`provider_models`):
//! setting a nullable column (`apiKeyId`, `baseUrl`, `dimensions`,
//! `truncateToDimensions`) **to NULL** via `update` — the patch models a provided
//! field as "set to this value", so a nullable setter lands when an op needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

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
    pub api_key_id: Option<String>,
    pub base_url: Option<String>,
    pub model_name: Option<String>,
    pub dimensions: Option<f64>,
    pub truncate_to_dimensions: Option<f64>,
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
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;

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
        if let Some(dimensions) = patch.dimensions {
            assignments.push(format!("dimensions = ?{}", values.len() + 1));
            values.push(Box::new(dimensions));
        }
        if let Some(truncate_to_dimensions) = patch.truncate_to_dimensions {
            assignments.push(format!("truncateToDimensions = ?{}", values.len() + 1));
            values.push(Box::new(truncate_to_dimensions));
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
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
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
