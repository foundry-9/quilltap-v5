//! The doc-mount-chunks repository — a **mount-index sibling-DB BLOB repo** of
//! Phase 2. Ports v4's
//! `lib/database/repositories/doc-mount-chunks.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! ## The sibling DB (mirrors `doc_mount_points` / `group_character_members`)
//!
//! Like every mount-index repo, v4 overrides `getCollection()` to route all
//! reads/writes to the dedicated mount-index database (`quilltap-mount-index.db`)
//! via `getRawMountIndexDatabase()`, isolating mount-tracking data from the main
//! DB. In the Rust port that routing is **not** a property of the repo — it is
//! the file the [`super::Writer`] was opened against. `Writer::open_writable`
//! opens any ChaCha20 file by path, so a writer opened on the mount-index DB
//! exposes these repos exactly as a main-DB writer exposes `users`/`folders`. The
//! repo code is therefore identical in shape to a plain main-DB repo; only the
//! harness points it at the mount-index fixture (the tier-2 case + builder target
//! `SQLITE_MOUNT_INDEX_PATH` and read back through `getRawMountIndexDatabase()`).
//!
//! ## The runtime `getCollection()` extra DDL is a NO-OP on a fresh fixture
//!
//! v4's overridden `getCollection()` runs the schema DDL then creates two
//! indexes (`idx_doc_mount_chunks_linkId`, `idx_doc_mount_chunks_mp`) with
//! `CREATE INDEX IF NOT EXISTS`. Indexes do not change column layout or stored
//! cell bytes, so they are irrelevant to the tier-2 row dump; on a fresh fixture
//! the table has EXACTLY the schema columns, in schema/Zod field order. We bind
//! the INSERT in that exact order.
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each a
//! straight delegate to `_create`/`_update`/`_delete`). The custom query helpers
//! — `findByLinkId`, `findByMountPointId`, `countEmbeddedByMountPointIds`,
//! `findAllWithEmbeddingsByMountPointIds`, `clearEmbeddingsByLinkId`,
//! `deleteByLinkId`, `deleteByMountPointId`, `updateEmbedding`, `bulkInsert`,
//! and the legacy aliases — are out of scope here.
//!
//! ## A BLOB column (after `help_docs` / `conversation_chunks`)
//!
//! `doc_mount_chunks.embedding` is a tier-2 **BLOB column**, modeled exactly like
//! `conversation_chunks.embedding`: a raw little-endian Float32 byte buffer via
//! [`crate::embedding_blob::float32_to_blob`]. v4's `documentToRow` blob path
//! (`embeddingToBlob`, wired by the repo registering `embedding` as a blob column
//! in `getCollection()`) stores an **empty array or null as SQL NULL**, never a
//! zero-length blob; here `None` *or* an empty `Vec<f32>` binds SQL NULL, only a
//! non-empty vector is serialized. The canonical dump emits BLOBs as lowercase
//! hex on both sides, so a deterministic Float32 buffer compares byte-for-byte
//! (`[0.5,-0.25,0.75,0.125]` → `0000003f000080be0000403f0000003e`).
//!
//! Following `conversation_chunks`/`help_docs` exactly, the BLOB is **not
//! touchable through `update`**: v4's `_update` whole-row rewrite re-persists the
//! existing embedding unchanged, so a text-only patch leaves it intact. This port
//! models the patch as a partial `UPDATE SET` over only the provided columns +
//! `updatedAt`, never naming the `embedding` column, so the stored BLOB survives
//! untouched. The corpus exercises this directly (a content/heading/tokenCount
//! update on the embedded seed row, asserted to still show the original embedding
//! hex). v4's `updateEmbedding` IS the path that mutates the BLOB, and it is out
//! of scope here.
//!
//! ## The rest of the marshaling surface
//!
//!   - `chunkIndex` and `tokenCount` are both `z.number().int().min(0)` — a min
//!     but NO max — so v4's `mapToSQLiteType` lowers them to **REAL** (INTEGER
//!     affinity needs an integer min AND max). They bind `f64`; an integer-valued
//!     REAL (e.g. `0.0`) renders back as `0` in the canonical dump via
//!     [`super::js_number_to_json`], matching v4 byte-for-byte. (Same idiom as
//!     `conversation_chunks.interchangeIndex`.)
//!   - `linkId` and `mountPointId` are UUIDs → TEXT.
//!   - `content` is required TEXT.
//!   - `headingContext` is `z.string().nullable().optional()` → NULLABLE TEXT
//!     (`None` → SQL NULL). Both null and non-null banked.
//!   - timestamps are TEXT.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`conversation_chunks`/`doc_mount_points` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::embedding_blob::float32_to_blob;

/// Fields for creating a doc mount chunk (the `Omit<DocMountChunk,'id'|
/// timestamps>` shape). `embedding` is the BLOB column (`None`/empty → SQL NULL,
/// non-empty → little-endian Float32 bytes); `chunk_index`/`token_count` are the
/// REAL number columns; `heading_context` is the nullable TEXT column.
pub struct DmcCreate {
    /// `UUIDSchema` (FK → doc_mount_file_links.id) → TEXT.
    pub link_id: String,
    /// `UUIDSchema` (denormalized) → TEXT.
    pub mount_point_id: String,
    /// `z.number().int().min(0)` (min only, no max) → REAL → bound `f64`.
    pub chunk_index: f64,
    pub content: String,
    /// `z.number().int().min(0)` (min only, no max) → REAL → bound `f64`.
    pub token_count: f64,
    /// `z.string().nullable().optional()` → NULLABLE TEXT (`None` → SQL NULL).
    pub heading_context: Option<String>,
    /// `None` or empty → SQL NULL; non-empty → little-endian Float32 bytes.
    pub embedding: Option<Vec<f32>>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A doc-mount-chunk update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Following `conversation_chunks`/`help_docs`, it deliberately has
/// **no embedding field** — the BLOB is never touched through `update` (v4's
/// whole-row rewrite re-persists the existing embedding unchanged; here the
/// partial `UPDATE SET` simply never names the `embedding` column). Each `Some`
/// field sets that column. Clearing `heading_context` to NULL is deferred (the
/// patch models a provided field as "set to this value").
#[derive(Default)]
pub struct DmcUpdate {
    pub link_id: Option<String>,
    pub mount_point_id: Option<String>,
    /// REAL number column; `Some(n)` sets it (bound `f64`).
    pub chunk_index: Option<f64>,
    pub content: Option<String>,
    /// REAL number column; `Some(n)` sets it (bound `f64`).
    pub token_count: Option<f64>,
    pub heading_context: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountChunksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountChunksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a doc mount chunk with the given pinned id + timestamps. The
    /// embedding serializes to a little-endian Float32 BLOB (`None`/empty → SQL
    /// NULL); `chunkIndex`/`tokenCount` bind `f64` (REAL); `headingContext` passes
    /// through (`None` → SQL NULL). Columns are bound in schema/Zod field order
    /// (= on-disk order on a fresh fixture).
    pub fn create(&self, data: &DmcCreate, opts: &CreateOptions) -> Result<(), DbError> {
        // empty / null embedding -> SQL NULL; non-empty -> Float32 LE bytes.
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };

        self.conn.execute(
            "INSERT INTO doc_mount_chunks \
               (id, linkId, mountPointId, chunkIndex, content, tokenCount, \
                headingContext, embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                opts.id,
                data.link_id,
                data.mount_point_id,
                data.chunk_index,
                data.content,
                data.token_count,
                data.heading_context,
                embedding_blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the chunk `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id, createdAt, and the `embedding`
    /// BLOB are never touched. Each `Some` field sets that column; `updatedAt` is
    /// always set.
    pub fn update(&self, id: &str, patch: &DmcUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`: the row must exist or the update is a
        // no-op (-> null). Mirror that so a missing target yields Ok(false)
        // rather than relying on the UPDATE affecting zero rows.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(link_id) = &patch.link_id {
            assignments.push(format!("linkId = ?{}", values.len() + 1));
            values.push(Box::new(link_id.clone()));
        }
        if let Some(mount_point_id) = &patch.mount_point_id {
            assignments.push(format!("mountPointId = ?{}", values.len() + 1));
            values.push(Box::new(mount_point_id.clone()));
        }
        if let Some(chunk_index) = patch.chunk_index {
            assignments.push(format!("chunkIndex = ?{}", values.len() + 1));
            values.push(Box::new(chunk_index));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(token_count) = patch.token_count {
            assignments.push(format!("tokenCount = ?{}", values.len() + 1));
            values.push(Box::new(token_count));
        }
        if let Some(heading_context) = &patch.heading_context {
            assignments.push(format!("headingContext = ?{}", values.len() + 1));
            values.push(Box::new(heading_context.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_chunks SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the chunk `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_chunks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_chunks WHERE id = ?1",
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
