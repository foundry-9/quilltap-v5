//! The files repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`,
//! `provider_models`, `roleplay_templates`, `image_profiles`, and
//! `connection_profiles`. Ports v4's
//! `lib/database/repositories/files.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The many custom query helpers — `findByIds`, `findBySha256`,
//! `findByCategory`, `findByFolder*`, `addLink`/`removeLink`, etc. — are out of
//! scope here. v4's `update` strips `id` and `createdAt` before `_update`, which
//! is a no-op for this port since we preserve both anyway.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `files` extends v4's `TaggableBaseRepository`, so it carries the **Taggable
//! lineage**: a user-scoped `userId` plus a JSON **`tags` array** column (the
//! `Vec<String>` → compact JSON text shape). It is the **widest** repo ported so
//! far (~23 columns) and widens the surface with:
//!
//!   - **two JSON array columns** — `linkedTo` AND `tags`, both
//!     `z.array(UUIDSchema).default([])` → compact JSON text via
//!     `serde_json::to_string` of a `Vec<String>` (order-preserving, no key-order
//!     subtlety).
//!   - a **REAL number column** (`size`, bare `z.number()` → REAL → `f64`) plus
//!     **two nullable REAL number columns** (`width`, `height`, both
//!     `z.number().nullable().optional()` → `Option<f64>`). Integer-valued REAL
//!     cells render back as JSON integers via [`super::js_number_to_json`].
//!   - an **optional boolean column with NO default** (`isPlainText`,
//!     `z.boolean().optional()`): present → INTEGER 0/1, absent → SQL NULL. It is
//!     modeled as `Option<bool>` (bound as `Option<i64>`) so an omitted value
//!     persists as NULL, distinct from an explicit `false`/0.
//!   - several **enum TEXT columns** (`source` = `FileSourceEnum`, `category` =
//!     `FileCategoryEnum`, `fileStatus` = `FileStatusEnum`) — all stored as plain
//!     TEXT.
//!   - **fixed-length string** `sha256` (`z.string().length(64)`) — TEXT, no
//!     special marshaling.
//!   - a batch of **nullable string columns** (`generationPrompt`,
//!     `generationModel`, `generationRevisedPrompt`, `description`, `projectId`,
//!     `folderPath`, `storageKey`) → `Option<String>` (`None` → SQL NULL).
//!
//! `fileStatus` (`FileStatusEnum.default('ok').optional()`) is modeled as a
//! required `String` in `FileCreate`: the corpus always sets it explicitly
//! (`'ok'`/`'orphaned'`), sidestepping the `.default().optional()` absent
//! behavior subtlety. `isPlainText` is modeled as `Option<bool>` precisely
//! because its absent case (NULL) is semantically distinct from `false` — the
//! corpus exercises true, false, and one absent (→ NULL) row.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! Phase-2 repos use.
//!
//! Deferred (not in the corpus, mirroring the other Taggable repos): setting a
//! nullable column **to NULL** via `update` — the patch models a provided field
//! as "set to this value", so a nullable setter lands when an op needs it.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a file entry (the `Omit<FileEntry,'id'|timestamps>`
/// shape). `linkedTo`/`tags` are the two JSON array columns; `size` is the REAL
/// column; `width`/`height` the nullable REAL columns; `is_plain_text` the
/// optional-no-default boolean (absent → NULL); `source`/`category`/`file_status`
/// the enum TEXT columns; the rest are nullable string columns.
pub struct FileCreate {
    pub user_id: String,
    /// 64-hex-char content hash (`z.string().length(64)`) → TEXT.
    pub sha256: String,
    pub original_filename: String,
    pub mime_type: String,
    /// Bare `z.number()` → REAL → `f64` (integer-valued collapses in the dump).
    pub size: f64,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL.
    pub width: Option<f64>,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL.
    pub height: Option<f64>,
    /// `z.boolean().optional()` with NO default: `Some(b)` → INTEGER 0/1,
    /// `None` → SQL NULL (the absent case, distinct from an explicit `false`).
    pub is_plain_text: Option<bool>,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub linked_to: Vec<String>,
    /// `FileSourceEnum` → TEXT (UPLOADED/GENERATED/IMPORTED/SYSTEM).
    pub source: String,
    /// `FileCategoryEnum` → TEXT (IMAGE/DOCUMENT/AVATAR/ATTACHMENT/EXPORT/BACKUP).
    pub category: String,
    /// `None` => SQL NULL.
    pub generation_prompt: Option<String>,
    /// `None` => SQL NULL.
    pub generation_model: Option<String>,
    /// `None` => SQL NULL.
    pub generation_revised_prompt: Option<String>,
    /// `None` => SQL NULL.
    pub description: Option<String>,
    /// Stored as compact JSON text (`["id1","id2"]`, `[]` when empty).
    pub tags: Vec<String>,
    /// `None` => SQL NULL.
    pub project_id: Option<String>,
    /// `None` => SQL NULL.
    pub folder_path: Option<String>,
    /// `None` => SQL NULL.
    pub storage_key: Option<String>,
    /// `FileStatusEnum` → TEXT (`'ok'`/`'orphaned'`). The corpus always sets it,
    /// so it is required here (sidesteps the `.default().optional()` subtlety).
    pub file_status: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A file update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved (v4 deletes them off the patch; we
/// never touch them), `updatedAt` is set explicitly. Each `Some` field sets that
/// column; clearing a nullable column to NULL is deferred (see header). Covers
/// every settable column.
#[derive(Default)]
pub struct FileUpdate {
    pub sha256: Option<String>,
    pub original_filename: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub is_plain_text: Option<bool>,
    /// Re-serialized to compact JSON text when provided.
    pub linked_to: Option<Vec<String>>,
    pub source: Option<String>,
    pub category: Option<String>,
    pub generation_prompt: Option<String>,
    pub generation_model: Option<String>,
    pub generation_revised_prompt: Option<String>,
    pub description: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub tags: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub folder_path: Option<String>,
    pub storage_key: Option<String>,
    pub file_status: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct FilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> FilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a file entry with the given pinned id + timestamps. `linkedTo`/`tags`
    /// → compact JSON array text; `size`/`width`/`height` bind `f64`/`Option<f64>`
    /// (REAL); `isPlainText` binds `Option<i64>` (0/1 or NULL); the enum + nullable
    /// strings pass through (`None` → SQL NULL).
    pub fn create(&self, data: &FileCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let linked_to_json = serde_json::to_string(&data.linked_to)
            .map_err(|e| DbError::Key(format!("linkedTo serialize: {e}")))?;
        let tags_json = serde_json::to_string(&data.tags)
            .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
        let is_plain_text = data.is_plain_text.map(i64::from);

        self.conn.execute(
            "INSERT INTO files \
               (id, userId, sha256, originalFilename, mimeType, size, width, height, \
                isPlainText, linkedTo, source, category, generationPrompt, generationModel, \
                generationRevisedPrompt, description, tags, projectId, folderPath, storageKey, \
                fileStatus, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                opts.id,
                data.user_id,
                data.sha256,
                data.original_filename,
                data.mime_type,
                data.size,
                data.width,
                data.height,
                is_plain_text,
                linked_to_json,
                data.source,
                data.category,
                data.generation_prompt,
                data.generation_model,
                data.generation_revised_prompt,
                data.description,
                tags_json,
                data.project_id,
                data.folder_path,
                data.storage_key,
                data.file_status,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the file `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &FileUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(sha256) = &patch.sha256 {
            assignments.push(format!("sha256 = ?{}", values.len() + 1));
            values.push(Box::new(sha256.clone()));
        }
        if let Some(original_filename) = &patch.original_filename {
            assignments.push(format!("originalFilename = ?{}", values.len() + 1));
            values.push(Box::new(original_filename.clone()));
        }
        if let Some(mime_type) = &patch.mime_type {
            assignments.push(format!("mimeType = ?{}", values.len() + 1));
            values.push(Box::new(mime_type.clone()));
        }
        if let Some(size) = patch.size {
            assignments.push(format!("size = ?{}", values.len() + 1));
            values.push(Box::new(size));
        }
        if let Some(width) = patch.width {
            assignments.push(format!("width = ?{}", values.len() + 1));
            values.push(Box::new(width));
        }
        if let Some(height) = patch.height {
            assignments.push(format!("height = ?{}", values.len() + 1));
            values.push(Box::new(height));
        }
        if let Some(is_plain_text) = patch.is_plain_text {
            assignments.push(format!("isPlainText = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(is_plain_text)));
        }
        if let Some(linked_to) = &patch.linked_to {
            let linked_to_json = serde_json::to_string(linked_to)
                .map_err(|e| DbError::Key(format!("linkedTo serialize: {e}")))?;
            assignments.push(format!("linkedTo = ?{}", values.len() + 1));
            values.push(Box::new(linked_to_json));
        }
        if let Some(source) = &patch.source {
            assignments.push(format!("source = ?{}", values.len() + 1));
            values.push(Box::new(source.clone()));
        }
        if let Some(category) = &patch.category {
            assignments.push(format!("category = ?{}", values.len() + 1));
            values.push(Box::new(category.clone()));
        }
        if let Some(generation_prompt) = &patch.generation_prompt {
            assignments.push(format!("generationPrompt = ?{}", values.len() + 1));
            values.push(Box::new(generation_prompt.clone()));
        }
        if let Some(generation_model) = &patch.generation_model {
            assignments.push(format!("generationModel = ?{}", values.len() + 1));
            values.push(Box::new(generation_model.clone()));
        }
        if let Some(generation_revised_prompt) = &patch.generation_revised_prompt {
            assignments.push(format!("generationRevisedPrompt = ?{}", values.len() + 1));
            values.push(Box::new(generation_revised_prompt.clone()));
        }
        if let Some(description) = &patch.description {
            assignments.push(format!("description = ?{}", values.len() + 1));
            values.push(Box::new(description.clone()));
        }
        if let Some(tags) = &patch.tags {
            let tags_json = serde_json::to_string(tags)
                .map_err(|e| DbError::Key(format!("tags serialize: {e}")))?;
            assignments.push(format!("tags = ?{}", values.len() + 1));
            values.push(Box::new(tags_json));
        }
        if let Some(project_id) = &patch.project_id {
            assignments.push(format!("projectId = ?{}", values.len() + 1));
            values.push(Box::new(project_id.clone()));
        }
        if let Some(folder_path) = &patch.folder_path {
            assignments.push(format!("folderPath = ?{}", values.len() + 1));
            values.push(Box::new(folder_path.clone()));
        }
        if let Some(storage_key) = &patch.storage_key {
            assignments.push(format!("storageKey = ?{}", values.len() + 1));
            values.push(Box::new(storage_key.clone()));
        }
        if let Some(file_status) = &patch.file_status {
            assignments.push(format!("fileStatus = ?{}", values.len() + 1));
            values.push(Box::new(file_status.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE files SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the file `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM files WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM files WHERE id = ?1", params![id], |row| {
                row.get::<_, i64>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}
