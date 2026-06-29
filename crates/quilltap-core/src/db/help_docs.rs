//! The help-docs repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`,
//! and `provider_models`. Ports v4's
//! `lib/database/repositories/help-docs.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete`, and `upsert_by_path` (v4's
//! `upsertByPath` — find-by-path then text-only update / minted create, verified
//! in the minted-values remap form). The embedding-only updates and the
//! `clearAll*` / `findAll*` reads remain out of scope.
//!
//! ## The first tier-2 BLOB column (the headline)
//!
//! `help_docs.embedding` is the first **BLOB column** to land in the tier-2
//! differential. v4 stores an embedding as a raw **Float32 byte buffer** —
//! little-endian `f32` bytes — via `documentToRow`'s blob path
//! (`embeddingToBlob`), which the help-docs repo wires up by auto-registering
//! the `embedding` blob column on first `getCollection()`. Two subtleties the
//! port reproduces exactly:
//!
//!   - **empty / null → SQL NULL.** v4 stores an empty embedding array (and a
//!     null) as NULL, never as a zero-length blob. Here `None` *or* an empty
//!     `Vec<f32>` binds SQL NULL; only a non-empty vector is serialized.
//!   - **bit-exact comparison.** The canonical dump emits BLOBs as lowercase
//!     hex on BOTH sides (Rust `cell_to_json` → `hex::encode`, the TS
//!     `canonValue` → `Buffer.toString('hex')`), so a deterministic Float32
//!     buffer compares byte-for-byte. The fixture uses only
//!     exactly-float32-representable values (`0.5`, `-0.25`, …) so the f64→f32
//!     cast is lossless and identical on both sides.
//!
//! The Float32→bytes conversion is [`crate::embedding_blob::float32_to_blob`]
//! (the same little-endian encoder the embedding layer uses).
//!
//! ## Text-only update preserves the embedding (banked behavior)
//!
//! v4's `_update` rewrites the *whole* row from the hydrated existing entity
//! merged with the patch, so an update that touches only text fields (e.g.
//! `content` + `contentHash`) re-persists the existing embedding unchanged (the
//! BLOB round-trips losslessly through hydrate→re-store). This port models the
//! patch as a partial `UPDATE SET` over only the provided text columns and
//! `updatedAt`, so it simply never names the `embedding` column — leaving the
//! stored BLOB untouched. The corpus exercises this directly: a content +
//! contentHash update on a row that HAS an embedding, asserted via the dump to
//! still show the original embedding hex.
//!
//! Determinism: the tier-2 case pins the id and timestamps, so the persisted
//! rows match v4's byte-for-byte with no normalization — the form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;
use crate::embedding_blob::float32_to_blob;

/// Fields for creating a help doc (the `Omit<HelpDoc,'id'|timestamps>` shape).
/// `embedding` is the BLOB column: `None` or an empty vector → SQL NULL, a
/// non-empty vector → little-endian Float32 bytes.
pub struct HdCreate {
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub content_hash: String,
    pub embedding: Option<Vec<f32>>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A help-doc update patch. Mirrors v4 `update` over `_update` for the
/// text-only path: provided fields overwrite, id and createdAt are preserved,
/// `updatedAt` is set explicitly. It deliberately has **no embedding field** —
/// a text-only update never touches the BLOB column, matching v4's whole-row
/// rewrite that re-persists the existing embedding unchanged (see the module
/// header).
#[derive(Default)]
pub struct HdUpdate {
    pub title: Option<String>,
    pub url: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub updated_at: String,
}

/// Input to [`HelpDocsRepository::upsert_by_path`] — v4's
/// `Omit<HelpDoc,'id'|'createdAt'|'updatedAt'|'embedding'>`. There is
/// deliberately **no `embedding` field**: an upsert that hits the create branch
/// stores a NULL embedding, and one that hits the update branch patches only the
/// four text columns, leaving any existing embedding BLOB untouched.
pub struct HdUpsert {
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub content_hash: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct HelpDocsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> HelpDocsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a help doc with the given pinned id + timestamps. The embedding is
    /// serialized to a little-endian Float32 BLOB; `None` or an empty vector
    /// binds SQL NULL (v4's empty→NULL rule in `documentToRow`).
    pub fn create(&self, data: &HdCreate, opts: &CreateOptions) -> Result<(), DbError> {
        // empty / null embedding -> SQL NULL; non-empty -> Float32 LE bytes.
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };

        self.conn.execute(
            "INSERT INTO help_docs \
               (id, title, path, url, content, contentHash, embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.title,
                data.path,
                data.url,
                data.content,
                data.content_hash,
                embedding_blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply a text-only update patch to the help doc `id`. Returns `Ok(false)`
    /// when no row matched (v4's "not found -> null"). id, createdAt, and the
    /// `embedding` BLOB are never touched.
    pub fn update(&self, id: &str, patch: &HdUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`: the row must exist or the update is a
        // no-op (-> null). Mirror that so a missing target yields Ok(false)
        // rather than relying on the UPDATE affecting zero rows (which would be
        // ambiguous when the patch is text-only).
        if !self.exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(title) = &patch.title {
            assignments.push(format!("title = ?{}", values.len() + 1));
            values.push(Box::new(title.clone()));
        }
        if let Some(url) = &patch.url {
            assignments.push(format!("url = ?{}", values.len() + 1));
            values.push(Box::new(url.clone()));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(content_hash) = &patch.content_hash {
            assignments.push(format!("contentHash = ?{}", values.len() + 1));
            values.push(Box::new(content_hash.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE help_docs SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Insert or update a help doc keyed by its `path` (v4's `upsertByPath`).
    ///
    /// If a row with `path` already exists, patches ONLY the four text columns
    /// (`title`, `url`, `content`, `contentHash`) plus `updatedAt` — the
    /// `embedding` BLOB is never named, so it survives untouched, matching v4's
    /// whole-row rewrite that re-persists the existing embedding. Otherwise it
    /// creates a fresh row with a minted id + timestamps and a NULL embedding
    /// (v4's `_create` over the embedding-less `data`).
    ///
    /// Mints `id` (`uuid::Uuid::new_v4`) and `now` ([`crate::clock::now_iso`])
    /// just like the create/remap path, so the resulting row carries
    /// nondeterministic id + timestamps (verified by the harness via remap +
    /// timestamp-placeholder normalization). Returns the id of the affected row.
    pub fn upsert_by_path(&self, data: &HdUpsert) -> Result<String, DbError> {
        let now = now_iso();

        if let Some(existing_id) = self.find_id_by_path(&data.path)? {
            // Existing row -> text-only update. The embedding column is NOT in
            // the patch, so the stored BLOB is left intact.
            self.update(
                &existing_id,
                &HdUpdate {
                    title: Some(data.title.clone()),
                    url: Some(data.url.clone()),
                    content: Some(data.content.clone()),
                    content_hash: Some(data.content_hash.clone()),
                    updated_at: now,
                },
            )?;
            return Ok(existing_id);
        }

        // No existing row -> create with a minted id + timestamps and (since
        // `HdUpsert` carries no embedding) a NULL embedding.
        let id = uuid::Uuid::new_v4().to_string();
        self.create(
            &HdCreate {
                title: data.title.clone(),
                path: data.path.clone(),
                url: data.url.clone(),
                content: data.content.clone(),
                content_hash: data.content_hash.clone(),
                embedding: None,
            },
            &CreateOptions {
                id: id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        Ok(id)
    }

    /// The id of the row whose `path` matches, or `None` (v4's `findByPath`
    /// non-null check; reads only the key column).
    fn find_id_by_path(&self, path: &str) -> Result<Option<String>, DbError> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM help_docs WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(id)
    }

    /// Delete the help doc `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM help_docs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with `id` exists — v4's `findById` non-null check, reading
    /// nothing but the key.
    fn exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM help_docs WHERE id = ?1",
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
