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

/// A chunk row as the reindex / embedding-scheduler services read it — the
/// subset of v4 `DocMountChunk` those paths touch, plus an embedding-presence
/// flag (the schedulers only test `!chunk.embedding || length === 0`, never the
/// vector itself).
#[derive(Clone, Debug)]
pub struct ChunkRow {
    pub id: String,
    pub link_id: String,
    pub mount_point_id: String,
    pub chunk_index: i64,
    pub content: String,
    /// v4 `!chunk.embedding || chunk.embedding.length === 0` inverted: `true`
    /// when a non-empty embedding BLOB is stored.
    pub has_embedding: bool,
    /// The stored vector's component count — all `embeddingMatchesDim` reads
    /// (EMBEDDING_REINDEX_ALL's new phase 4, P4.d27). `0` for a NULL/empty BLOB,
    /// which can never equal a positive target dim, so such a chunk re-embeds.
    ///
    /// ⚠ The DECODED length, never `length(embedding) / 4` — current writes are
    /// int8-quantized (`11 + dim` bytes), so byte arithmetic gives a plausible,
    /// wrong answer. (Same convention and same warning as
    /// [`super::conversation_chunks::CcRow::embedding_dim`].)
    pub embedding_dim: usize,
}

/// The projection every [`ChunkRow`] read shares. The embedding BLOB rides along
/// (rather than a `length(embedding) > 0` probe alone) because `embedding_dim` is
/// the DECODED component count; `has_embedding` stays the SQL byte test so a blob
/// too short to decode into a single float keeps reading as "present", exactly as
/// it did before that field existed.
const CHUNK_ROW_SELECT: &str = "SELECT id, linkId, mountPointId, chunkIndex, content, \
     (embedding IS NOT NULL AND length(embedding) > 0), embedding \
     FROM doc_mount_chunks";

fn marshal_chunk_row(row: &rusqlite::Row) -> rusqlite::Result<ChunkRow> {
    let blob: Option<Vec<u8>> = row.get(6)?;
    Ok(ChunkRow {
        id: row.get(0)?,
        link_id: row.get(1)?,
        mount_point_id: row.get(2)?,
        chunk_index: match row.get_ref(3)? {
            rusqlite::types::ValueRef::Integer(i) => i,
            rusqlite::types::ValueRef::Real(f) => f as i64,
            _ => 0,
        },
        content: row.get(4)?,
        has_embedding: row.get::<_, i64>(5)? != 0,
        embedding_dim: blob
            .as_deref()
            .map(|b| crate::embedding_blob::blob_to_float32(b).len())
            .unwrap_or(0),
    })
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
    /// v4 `findByMountPointId`, narrowed to the reindex/scheduler-read fields
    /// (see [`ChunkRow`]). Insertion order (rowid) like v4's unordered filter
    /// scan on a fresh table.
    pub fn find_rows_by_mount_point_id(
        &self,
        mount_point_id: &str,
    ) -> Result<Vec<ChunkRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare(&format!("{CHUNK_ROW_SELECT} WHERE mountPointId = ?1"))?;
        let rows = stmt
            .query_map(params![mount_point_id], marshal_chunk_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findById` (P4.6BL), narrowed to the reindex/handler-read fields (the
    /// same [`ChunkRow`] shape `find_rows_by_mount_point_id` returns). `None`
    /// when no row matches.
    pub fn find_row_by_id(&self, id: &str) -> Result<Option<ChunkRow>, DbError> {
        self.conn
            .query_row(
                &format!("{CHUNK_ROW_SELECT} WHERE id = ?1"),
                params![id],
                marshal_chunk_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `updateEmbedding` (P4.6BL) — set just the `embedding` BLOB on a chunk
    /// (+ the minted `updatedAt`, injected here as `now_iso` per this repo's
    /// convention). The vector serializes exactly as `create` does (`empty →
    /// SQL NULL`, else Float32 LE bytes). Returns `Ok(false)` when no row
    /// matched — v4 THROWS `` `Doc mount chunk not found for embedding update:
    /// ${id}` `` there; the EMBEDDING_GENERATE handler (its only caller) formats
    /// that exact string. v4's method also invalidates the in-memory mount-chunk
    /// cache — v5 has no such cache (the search path reads the DB directly), so
    /// that side-effect is a documented no-op here.
    pub fn update_embedding(
        &self,
        id: &str,
        embedding: &[f32],
        now_iso: &str,
    ) -> Result<bool, DbError> {
        let embedding_blob: Option<Vec<u8>> = if embedding.is_empty() {
            None
        } else {
            Some(float32_to_blob(embedding))
        };
        let affected = self.conn.execute(
            "UPDATE doc_mount_chunks SET embedding = ?1, updatedAt = ?2 WHERE id = ?3",
            params![embedding_blob, now_iso, id],
        )?;
        Ok(affected > 0)
    }

    /// v4 `findByLinkId(linkId)` narrowed to the ids — the one field its only
    /// v5 caller needs. The character-archive prune reads them BEFORE
    /// `deleteWithGC` cascades the chunks away, so the matching
    /// `embedding_status` rows can be deleted too (they would otherwise linger
    /// as permanent orphans; v4 `archive-service.ts:838`). Insertion order
    /// (rowid), like v4's unordered filter scan.
    pub fn find_ids_by_link_id(&self, link_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM doc_mount_chunks WHERE linkId = ?1")?;
        let rows = stmt
            .query_map(params![link_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `clearEmbeddingsByLinkId`: NULL (don't delete) every stored embedding
    /// under a link. Returns the number of cleared rows.
    pub fn clear_embeddings_by_link_id(&self, link_id: &str) -> Result<usize, DbError> {
        let n = self.conn.execute(
            "UPDATE doc_mount_chunks SET embedding = NULL \
             WHERE linkId = ?1 AND embedding IS NOT NULL",
            params![link_id],
        )?;
        Ok(n)
    }

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

/// v4 `docMountChunks.countEmbeddedByMountPointIds(ids)` — the CHEAP GROUP-BY count
/// (`embedding IS NOT NULL`, no BLOB decode) used by the mount-points LIST route.
/// Returns `(mountPointId, count)` rows for the ids that have ≥1 embedded chunk;
/// mounts with none are absent (the route's `|| 0` fallback fills them).
pub fn count_embedded_by_mount_point_ids(
    conn: &Connection,
    ids: &[String],
) -> Result<std::collections::HashMap<String, i64>, DbError> {
    let mut out = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT mountPointId, COUNT(*) FROM doc_mount_chunks \
         WHERE mountPointId IN ({placeholders}) AND embedding IS NOT NULL \
         GROUP BY mountPointId"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    for r in rows {
        let (mp, c) = r?;
        out.insert(mp, c);
    }
    Ok(out)
}

/// v4 GET-\[id\]'s EXPENSIVE embedded count: `findByMountPointId(id)` hydrates all
/// chunks, then filters `c.embedding != null && c.embedding.length > 0`. The SQL
/// `embedding IS NOT NULL AND length(embedding) > 0` is byte-length > 0, which
/// matches the JS float-length > 0 (both are false only for a 0-byte BLOB). This
/// differs from the cheap LIST count, which checks only `IS NOT NULL`.
pub fn count_nonempty_embeddings_by_mount_point_id(
    conn: &Connection,
    mount_point_id: &str,
) -> Result<i64, DbError> {
    let n = conn.query_row(
        "SELECT COUNT(*) FROM doc_mount_chunks \
         WHERE mountPointId = ?1 AND embedding IS NOT NULL AND length(embedding) > 0",
        params![mount_point_id],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(n)
}
