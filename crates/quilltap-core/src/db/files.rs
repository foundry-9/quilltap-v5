//! The files repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`,
//! `provider_models`, `roleplay_templates`, `image_profiles`, and
//! `connection_profiles`. Ports v4's
//! `lib/database/repositories/files.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo), plus the two file-linking primitives the message-persistence path
//! needs — `addLink` (attach an entity to a file) and `addTag` (the inherited
//! `TaggableBaseRepository.addTag`, tagging a generated image with its character
//! so it surfaces in the gallery). The remaining custom query helpers —
//! `findByIds`, `findBySha256`, `findByCategory`, `findByFolder*`, `removeLink`,
//! `removeTag`, etc. — are out of scope here. v4's `update` strips `id` and
//! `createdAt` before `_update`, which is a no-op for this port since we preserve
//! both anyway.
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
    /// `FileCategoryEnum` → TEXT
    /// (IMAGE/DOCUMENT/AVATAR/ATTACHMENT/EXPORT/BACKUP/ARCHIVE — `ARCHIVE`
    /// joined the enum at v4 `d553f72a`; v5 stores the category as plain TEXT
    /// either way, so nothing but this list moved).
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

/// The `files`-row columns the stale-chat maintenance sweep reads (`FileEntry`
/// reduced). `sha256` is always present on disk (`z.string().length(64)`), but a
/// legacy `avatarUrl`-fallback row could carry an empty string — normalized to
/// `None` by [`crate::services::maintenance`]'s keep-set. `size` is the REAL column
/// (bare `z.number()`), read as `f64`.
#[derive(Debug, Clone)]
pub struct FileSweepRow {
    pub id: String,
    pub sha256: String,
    pub size: f64,
    /// `FileSourceEnum`: `'UPLOADED' | 'GENERATED' | 'IMPORTED' | 'SYSTEM'`.
    pub source: String,
    /// `FileCategoryEnum`: `'IMAGE' | 'DOCUMENT' | 'AVATAR' | 'ATTACHMENT' |
    /// 'EXPORT' | 'BACKUP' | 'ARCHIVE'` (`ARCHIVE` new at v4 `d553f72a`).
    pub category: String,
}

impl FileSweepRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(FileSweepRow {
            id: row.get(0)?,
            sha256: row.get(1)?,
            size: row.get(2)?,
            source: row.get(3)?,
            category: row.get(4)?,
        })
    }
}

/// The `files`-row metadata the W4.4b `loadAndProcessFiles` reads (v4
/// `findByLinkedTo` subset: `id`/`originalFilename`/`mimeType`/`size`).
#[derive(Debug, Clone)]
pub struct FileLinkMeta {
    pub id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub size: i64,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct FilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> FilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `findAll().filter(f => f.projectId === projectId).length` — the number of
    /// files bound to a project. `findAll` is unscoped, so a `COUNT WHERE
    /// projectId = ?` yields the identical count. Used by the `project_info` tool.
    /// Read-only.
    pub fn count_by_project_id(&self, project_id: &str) -> Result<i64, DbError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE projectId = ?1",
            rusqlite::params![project_id],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    /// v4 the legacy `list-files` branch: `files.findAll().filter(projectId === ?)`
    /// (natural/rowid order — the `WHERE` filter yields the identical set + order)
    /// hydrated into the fields the Files-card DTO reads. `description`/`width`/
    /// `height` are Option — the file read marshaling omits them when NULL, so the
    /// DTO drops those keys.
    pub fn find_by_project_id_for_listing(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectFileListRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, userId, originalFilename, mimeType, size, category, description, \
                    projectId, folderPath, storageKey, width, height, createdAt, updatedAt \
             FROM files WHERE projectId = ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(ProjectFileListRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    original_filename: row.get(2)?,
                    mime_type: row.get(3)?,
                    size: real_affinity_opt_i64(row.get_ref(4)?).unwrap_or(0),
                    category: row.get(5)?,
                    description: row.get(6)?,
                    project_id: row.get(7)?,
                    folder_path: row.get(8)?,
                    storage_key: row.get(9)?,
                    width: real_affinity_opt_i64(row.get_ref(10)?),
                    height: real_affinity_opt_i64(row.get_ref(11)?),
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 handleRemoveFile: `files.update(fileId, { projectId: null })` — clear the
    /// project link. `FileUpdate.project_id` can't express NULL, so this is a
    /// direct guarded update (mints `updatedAt`). Result ignored by the route
    /// (always `{success:true}`).
    pub fn clear_project_id(&self, file_id: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE files SET projectId = NULL, updatedAt = ?1 WHERE id = ?2",
            rusqlite::params![crate::clock::now_iso(), file_id],
        )?;
        Ok(())
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

    /// v4 `findById(fileId)` reduced to the columns the stale-chat maintenance
    /// sweep consults (`id`, `sha256`, `size`, `source`, `category`). `None` when
    /// absent. The full `FileEntry` net-read marshaling is out of scope (no consumer
    /// yet); this scoped read serves the sweep's keep-set / delete path.
    pub fn find_sweep_row_by_id(&self, id: &str) -> Result<Option<FileSweepRow>, DbError> {
        self.conn
            .query_row(
                "SELECT id, sha256, size, source, category FROM files WHERE id = ?1",
                params![id],
                FileSweepRow::from_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `findByLinkedTo(entityId)` (`files.repository.ts:87`) reduced to the
    /// sweep columns: every file whose `linkedTo` JSON array contains `entity_id`.
    /// v4 filters `{ linkedTo: { $in: [entityId] } }`, which the translator turns
    /// into a `json_each` membership test — reproduced here. Rowid order (v4's
    /// unordered scan). Drives the stale-chat sweep's candidate enumeration.
    pub fn find_sweep_rows_by_linked_to(
        &self,
        entity_id: &str,
    ) -> Result<Vec<FileSweepRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, size, source, category FROM files \
             WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1)",
        )?;
        let rows = stmt
            .query_map(params![entity_id], FileSweepRow::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByLinkedTo(entityId)` reduced to the columns the W4.4b
    /// `loadAndProcessFiles` consumes (`id`, `originalFilename`, `mimeType`,
    /// `size`). Same `json_each` membership test as [`Self::find_sweep_rows_by_linked_to`],
    /// rowid order (v4's unordered scan → `matched` preserves this order, NOT
    /// `fileIds` order).
    pub fn find_link_meta_by_linked_to(
        &self,
        entity_id: &str,
    ) -> Result<Vec<FileLinkMeta>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, originalFilename, mimeType, size FROM files \
             WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1)",
        )?;
        let rows = stmt
            .query_map(params![entity_id], |row| {
                Ok(FileLinkMeta {
                    id: row.get(0)?,
                    original_filename: row.get(1)?,
                    mime_type: row.get(2)?,
                    size: real_affinity_opt_i64(row.get_ref(3)?).unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `addLink(fileId, entityId)` — append `entity_id` to the file's
    /// `linkedTo` array (idempotent: absent-only), persisting through `update`
    /// (which mints a fresh `updatedAt`). A missing file is a no-op (v4 warns +
    /// returns `null`); an already-present link leaves the row untouched (v4
    /// returns the file without an `update`, so no `updatedAt` bump). Returns
    /// whether a row was written.
    ///
    /// The port reads the stored `linkedTo` JSON, checks membership, and — only on
    /// a change — issues the same partial `UPDATE` v4's `update({ linkedTo })`
    /// does (minting `updatedAt` via [`crate::clock::now_iso`], matching v4's
    /// `new Date().toISOString()`).
    pub fn add_link(&self, file_id: &str, entity_id: &str) -> Result<bool, DbError> {
        let Some(mut linked_to) = self.read_linked_to(file_id)? else {
            // File not found — v4 logs a warn and returns null (no write).
            return Ok(false);
        };
        if linked_to.iter().any(|e| e == entity_id) {
            // Already linked — v4 returns the file without an update (no
            // `updatedAt` bump). No-op.
            return Ok(false);
        }
        linked_to.push(entity_id.to_string());
        let patch = FileUpdate {
            linked_to: Some(linked_to),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        };
        self.update(file_id, &patch)
    }

    /// Read the stored `linkedTo` JSON array for a file. `None` when the row is
    /// missing (v4's `findById` → null); `Some(vec)` otherwise (`[]` when the
    /// column is empty/unparseable, matching the Zod `.default([])`).
    fn read_linked_to(&self, file_id: &str) -> Result<Option<Vec<String>>, DbError> {
        self.read_json_array_column("linkedTo", file_id)
    }

    /// v4 `addTag(entityId, tagId)` (inherited from `TaggableBaseRepository`) —
    /// append `tag_id` to the file's `tags` array (idempotent: absent-only),
    /// persisting through `update` (which mints a fresh `updatedAt`). Semantics
    /// mirror [`Self::add_link`] exactly, only over the `tags` column: a missing
    /// file is a no-op (v4 warns + returns `null`); an already-present tag leaves
    /// the row untouched (v4 returns the entity without an `update`, so no
    /// `updatedAt` bump). Returns whether a row was written.
    pub fn add_tag(&self, file_id: &str, tag_id: &str) -> Result<bool, DbError> {
        let Some(mut tags) = self.read_json_array_column("tags", file_id)? else {
            // File not found — v4 logs a warn and returns null (no write).
            return Ok(false);
        };
        if tags.iter().any(|t| t == tag_id) {
            // Already tagged — v4 returns the entity without an update (no
            // `updatedAt` bump). No-op.
            return Ok(false);
        }
        tags.push(tag_id.to_string());
        let patch = FileUpdate {
            tags: Some(tags),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        };
        self.update(file_id, &patch)
    }

    /// v4 `findById` — the file row hydrated into the [`FileEntry`] subset the
    /// photo tools consume (metadata only; the bytes live behind the
    /// [`crate::photos::save_image_to_album::FileBytesStore`] seam). `None` when
    /// the row is absent.
    pub fn find_by_id(&self, id: &str) -> Result<Option<FileEntry>, DbError> {
        let row = self
            .conn
            .query_row(FILE_ENTRY_SELECT, params![id], map_file_entry)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(row)
    }

    /// v4 `findByStorageKey(storageKey)` — the file row at a storage key (the
    /// files-proxy route's lookup, `files/proxy/[...key]/route.ts:60`). `None`
    /// when absent.
    pub fn find_by_storage_key(&self, storage_key: &str) -> Result<Option<FileEntry>, DbError> {
        let sql = format!("{FILE_ENTRY_SELECT_ALL} WHERE storageKey = ?1 LIMIT 1");
        let row = self
            .conn
            .query_row(&sql, params![storage_key], map_file_entry)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(row)
    }

    /// v4 `findBySha256(sha256)` — every file row with this content hash, hydrated
    /// into the [`FileEntry`] subset the photo tools consume. Ordered by
    /// `createdAt` ASC (insertion order) so `sisters[0]` is deterministic.
    pub fn find_by_sha256(&self, sha256: &str) -> Result<Vec<FileEntry>, DbError> {
        let sql = format!("{FILE_ENTRY_SELECT_ALL} WHERE sha256 = ?1 ORDER BY createdAt ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![sha256], map_file_entry)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Read a JSON-array column (`linkedTo` / `tags`) for a file. `None` when the
    /// row is missing (v4's `findById` → null); `Some(vec)` otherwise (`[]` when
    /// the column is empty/unparseable, matching the Zod `.default([])`). The
    /// column name is a fixed literal from the caller, never user input.
    fn read_json_array_column(
        &self,
        column: &str,
        file_id: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let sql = format!("SELECT {column} FROM files WHERE id = ?1");
        let row: Option<String> = self
            .conn
            .query_row(&sql, params![file_id], |row| {
                row.get::<_, Option<String>>(0)
            })
            .map(|opt| opt.unwrap_or_default())
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(row.map(|s| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default()))
    }

    /// v4 `findById(fileId)` hydrated into the [`FileFull`] projection the
    /// files-family routes (`serializeFileEntry` / `buildManagedFileResponse` /
    /// delete + dissociate) read. `None` when absent.
    pub fn find_full_by_id(&self, id: &str) -> Result<Option<FileFull>, DbError> {
        self.conn
            .query_row(FILE_FULL_SELECT, params![id], map_file_full)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `findAll()` (the un-overridden base-repo `_findAll`) — EVERY file row,
    /// no user filter, natural/rowid order. First consumer: the homepage
    /// project-activity map (`services::home`), which reads only
    /// `projectId`/`updatedAt` and folds through `max`, so the order is not
    /// observable there.
    pub fn find_all(&self) -> Result<Vec<FileFull>, DbError> {
        let mut stmt = self.conn.prepare(FILE_FULL_SELECT_ALL)?;
        let rows = stmt
            .query_map([], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByUserId(userId)` (base repo `findByFilter({userId})`) — every file
    /// row owned by the user, in natural/rowid order (the files list route sorts by
    /// `createdAt` DESC afterward, so this order is not observable there).
    pub fn find_by_user_id(&self, user_id: &str) -> Result<Vec<FileFull>, DbError> {
        let sql = format!("{FILE_FULL_SELECT_ALL} WHERE userId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findGeneralFiles(userId)` — `{userId, $or:[{projectId:null},
    /// {projectId:{$exists:false}}]}`. In this single-table store a missing column
    /// is impossible, so `projectId IS NULL` is the exact set. Rowid order.
    pub fn find_general_files(&self, user_id: &str) -> Result<Vec<FileFull>, DbError> {
        let sql = format!("{FILE_FULL_SELECT_ALL} WHERE userId = ?1 AND projectId IS NULL");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByProjectId(userId, projectId)` — `findByFilter({userId,
    /// projectId})`. Rowid order.
    pub fn find_by_project_id(
        &self,
        user_id: &str,
        project_id: &str,
    ) -> Result<Vec<FileFull>, DbError> {
        let sql = format!("{FILE_FULL_SELECT_ALL} WHERE userId = ?1 AND projectId = ?2");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id, project_id], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByFilenameInScope(userId, projectId, folderPath, filename)` —
    /// duplicate detection before create/overwrite. The nullable `projectId` uses
    /// v4's `createNullableFilter` (null → `IS NULL`, else `= ?`). Rowid order (the
    /// overwrite path takes `existing[0]`).
    pub fn find_by_filename_in_scope(
        &self,
        user_id: &str,
        project_id: Option<&str>,
        folder_path: &str,
        filename: &str,
    ) -> Result<Vec<FileFull>, DbError> {
        match project_id {
            Some(pid) => {
                let sql = format!(
                    "{FILE_FULL_SELECT_ALL} WHERE userId = ?1 AND originalFilename = ?2 \
                     AND folderPath = ?3 AND projectId = ?4"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id, filename, folder_path, pid], map_file_full)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!(
                    "{FILE_FULL_SELECT_ALL} WHERE userId = ?1 AND originalFilename = ?2 \
                     AND folderPath = ?3 AND projectId IS NULL"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id, filename, folder_path], map_file_full)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }

    /// v4 `findBySha256(sha256)` hydrated into [`FileFull`] (adds `projectId` +
    /// `createdAt` the chat-upload conflict path reads). Ordered `createdAt` ASC
    /// (v4's insertion order — `existing[0]` / `.find(projectId===…)`).
    pub fn find_by_sha256_full(&self, sha256: &str) -> Result<Vec<FileFull>, DbError> {
        let sql = format!("{FILE_FULL_SELECT_ALL} WHERE sha256 = ?1 ORDER BY createdAt ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![sha256], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByFilenameInProject(userId, projectId, filename)` — the chat-upload
    /// filename-conflict probe (`findByFilter({userId, projectId,
    /// originalFilename})`). Rowid order.
    pub fn find_by_filename_in_project(
        &self,
        user_id: &str,
        project_id: &str,
        filename: &str,
    ) -> Result<Vec<FileFull>, DbError> {
        let sql = format!(
            "{FILE_FULL_SELECT_ALL} WHERE userId = ?1 AND projectId = ?2 AND originalFilename = ?3"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id, project_id, filename], map_file_full)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The move/promote metadata update (v4 `files.update` over `{originalFilename?,
    /// folderPath, projectId}`) — the one path that must express SQL `projectId =
    /// NULL` (promote-to-general / move-to-general), which [`FileUpdate`] can't. Sets
    /// `originalFilename` (when `Some`), `folderPath`, mints `updatedAt`, and applies
    /// the tri-state `projectId` via [`MoveProjectId`]. Returns `false` when the row
    /// is absent (v4's `_update` no-op → `null`).
    pub fn apply_move_update(
        &self,
        id: &str,
        original_filename: Option<&str>,
        folder_path: &str,
        project_id: MoveProjectId<'_>,
    ) -> Result<bool, DbError> {
        if !self.row_exists(id)? {
            return Ok(false);
        }
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();
        if let Some(name) = original_filename {
            assignments.push(format!("originalFilename = ?{}", values.len() + 1));
            values.push(Box::new(name.to_string()));
        }
        assignments.push(format!("folderPath = ?{}", values.len() + 1));
        values.push(Box::new(folder_path.to_string()));
        match project_id {
            MoveProjectId::Set(pid) => {
                assignments.push(format!("projectId = ?{}", values.len() + 1));
                values.push(Box::new(pid.to_string()));
            }
            MoveProjectId::Clear => {
                // SQL NULL literal — no bound value (mirrors v4 `projectId: null`).
                assignments.push("projectId = NULL".to_string());
            }
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(crate::clock::now_iso()));
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

/// The `files`-row subset the photo tools consume (v4 `FileEntry`, restricted to
/// the columns `saveImageToAlbum` / `attach_image` read). The image bytes live
/// behind the [`crate::photos::save_image_to_album::FileBytesStore`] seam, keyed
/// by [`FileEntry::id`] + [`FileEntry::storage_key`].
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub id: String,
    pub sha256: String,
    pub original_filename: String,
    pub mime_type: String,
    /// The byte size (v4 `size`, a REAL-affinity integer). Read as `i64` (the
    /// W4.4b legacy attachment descriptor's `size` is `fileEntry.size`).
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub category: String,
    pub generation_prompt: Option<String>,
    pub generation_model: Option<String>,
    pub generation_revised_prompt: Option<String>,
    /// The stored image description (W4.4b `generateImageDescription`'s persisted
    /// reuse tier falls back to this after the two generation-prompt columns).
    pub description: Option<String>,
    pub storage_key: Option<String>,
}

/// The tri-state `projectId` a move/promote update carries (v4's `projectId ===
/// undefined ? keep : value`, where the value may itself be `null`). Only `Set`
/// and `Clear` reach [`FilesRepository::apply_move_update`] — the "keep" case is
/// resolved by the caller into the file's current `projectId`.
pub enum MoveProjectId<'a> {
    /// `projectId = <value>`.
    Set(&'a str),
    /// `projectId = NULL` (promote-to-general / move-to-general).
    Clear,
}

/// A `files` row hydrated into the projection the files-family routes read
/// (`serializeFileEntry` / `buildManagedFileResponse` + the delete/dissociate +
/// associations arms). Superset of the photo-tool [`FileEntry`]: adds `userId`,
/// `folderPath`, `fileStatus`, `linkedTo`, `projectId`, and the timestamps.
#[derive(Debug, Clone)]
pub struct FileFull {
    pub id: String,
    pub user_id: String,
    pub original_filename: String,
    pub mime_type: String,
    /// 64-hex content hash (the chat-upload conflict-detection + orphan-cleanup
    /// paths read it; the general files routes ignore it).
    pub sha256: String,
    /// The byte size (v4 `size`, REAL affinity, always integer-valued on disk).
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub category: String,
    pub description: Option<String>,
    /// The `linkedTo` JSON array (entity ids referencing this file).
    pub linked_to: Vec<String>,
    pub project_id: Option<String>,
    pub folder_path: Option<String>,
    pub storage_key: Option<String>,
    /// `FileStatusEnum` (`'ok'`/`'orphaned'`); NULL on legacy rows (→ `'ok'` in the
    /// serializer's `|| 'ok'`).
    pub file_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The legacy `list-files` branch row — the file columns the Files-card DTO
/// reads. `description`/`width`/`height` are Option so the DTO can omit them when
/// NULL (the file read marshaling drops null optionals).
#[derive(Debug, Clone)]
pub struct ProjectFileListRow {
    pub id: String,
    pub user_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub size: i64,
    pub category: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub folder_path: Option<String>,
    pub storage_key: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// The canonical column list, referenced by the lockstep test that guards the two
/// hardcoded SELECTs against drift.
#[cfg(test)]
const FILE_ENTRY_COLUMNS: &str = "id, sha256, originalFilename, mimeType, size, width, height, \
     category, generationPrompt, generationModel, generationRevisedPrompt, description, storageKey";

const FILE_ENTRY_SELECT: &str =
    "SELECT id, sha256, originalFilename, mimeType, size, width, height, \
     category, generationPrompt, generationModel, generationRevisedPrompt, description, storageKey \
     FROM files WHERE id = ?1";

/// The same column list for a filtered/ordered multi-row read (the WHERE + ORDER
/// BY are appended by the caller). Kept in lockstep with [`FILE_ENTRY_SELECT`].
const FILE_ENTRY_SELECT_ALL: &str =
    "SELECT id, sha256, originalFilename, mimeType, size, width, height, \
     category, generationPrompt, generationModel, generationRevisedPrompt, description, storageKey \
     FROM files";

/// The [`FileFull`] column projection (by-id form). Kept in lockstep with
/// [`FILE_FULL_SELECT_ALL`] and [`map_file_full`].
const FILE_FULL_SELECT: &str =
    "SELECT id, userId, originalFilename, mimeType, size, width, height, category, description, \
     linkedTo, projectId, folderPath, storageKey, fileStatus, createdAt, updatedAt, sha256 \
     FROM files WHERE id = ?1";

/// The same projection for a filtered multi-row read (the caller appends WHERE).
const FILE_FULL_SELECT_ALL: &str =
    "SELECT id, userId, originalFilename, mimeType, size, width, height, category, description, \
     linkedTo, projectId, folderPath, storageKey, fileStatus, createdAt, updatedAt, sha256 \
     FROM files";

/// Map a `files` row (the [`FILE_FULL_SELECT`] projection) into a [`FileFull`].
/// `size`/`width`/`height` are REAL-affinity integers; `linkedTo` is a JSON array
/// text column (`[]`/absent → empty vec, matching the Zod `.default([])`).
fn map_file_full(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileFull> {
    let linked_to_json: Option<String> = row.get(9)?;
    let linked_to = linked_to_json
        .map(|s| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default())
        .unwrap_or_default();
    Ok(FileFull {
        id: row.get(0)?,
        user_id: row.get(1)?,
        original_filename: row.get(2)?,
        mime_type: row.get(3)?,
        size: real_affinity_opt_i64(row.get_ref(4)?).unwrap_or(0),
        width: real_affinity_opt_i64(row.get_ref(5)?),
        height: real_affinity_opt_i64(row.get_ref(6)?),
        category: row.get(7)?,
        description: row.get(8)?,
        linked_to,
        project_id: row.get(10)?,
        folder_path: row.get(11)?,
        storage_key: row.get(12)?,
        file_status: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        sha256: row.get(16)?,
    })
}

/// Map a `files` row (the [`FILE_ENTRY_COLUMNS`] projection) into a [`FileEntry`].
/// `width`/`height` have REAL affinity but store integers; read as `Option<i64>`
/// (SQLite coerces a Real `123.0` cell on `get::<i64>`).
fn map_file_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileEntry> {
    Ok(FileEntry {
        id: row.get(0)?,
        sha256: row.get(1)?,
        original_filename: row.get(2)?,
        mime_type: row.get(3)?,
        size: real_affinity_opt_i64(row.get_ref(4)?).unwrap_or(0),
        width: real_affinity_opt_i64(row.get_ref(5)?),
        height: real_affinity_opt_i64(row.get_ref(6)?),
        category: row.get(7)?,
        generation_prompt: row.get(8)?,
        generation_model: row.get(9)?,
        generation_revised_prompt: row.get(10)?,
        description: row.get(11)?,
        storage_key: row.get(12)?,
    })
}

/// Read a nullable REAL-affinity integer cell (`width`/`height`): Integer/Real →
/// `Some(i64)`, NULL/other → `None`.
fn real_affinity_opt_i64(v: rusqlite::types::ValueRef<'_>) -> Option<i64> {
    match v {
        rusqlite::types::ValueRef::Integer(i) => Some(i),
        rusqlite::types::ValueRef::Real(f) => Some(f as i64),
        _ => None,
    }
}

#[cfg(test)]
mod file_entry_tests {
    use super::*;

    #[test]
    fn select_columns_stay_in_lockstep() {
        assert!(FILE_ENTRY_SELECT.contains(FILE_ENTRY_COLUMNS));
        assert!(FILE_ENTRY_SELECT_ALL.contains(FILE_ENTRY_COLUMNS));
    }
}
