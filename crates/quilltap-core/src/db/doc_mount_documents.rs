//! The doc-mount-documents repository — a **mount-index sibling-DB repo** of
//! Phase 2. Ports v4's
//! `lib/database/repositories/doc-mount-documents.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! ## The sibling DB (mirrors `doc_mount_folders` / `doc_mount_points`)
//!
//! Like every mount-index repo, v4 overrides `getCollection()` to route all
//! reads/writes to the dedicated mount-index database (`quilltap-mount-index.db`)
//! via `getRawMountIndexDatabase()`, isolating mount-tracking content from the
//! main DB so corruption there can never threaten characters/chats/memories. In
//! the Rust port that routing is **not** a property of the repo — it is the file
//! the [`super::Writer`] was opened against. `Writer::open_writable` opens any
//! ChaCha20 file by path, so a writer opened on the mount-index DB exposes these
//! repos exactly as a main-DB writer exposes `users`/`folders`. The repo code is
//! therefore identical in shape to a plain main-DB repo; only the harness points
//! it at the mount-index fixture (the tier-2 case + builder target
//! `SQLITE_MOUNT_INDEX_PATH` and read back through `getRawMountIndexDatabase()`).
//!
//! ## What this table is (content-addressable document text)
//!
//! `doc_mount_documents` stores the **text content** of database-backed files
//! inside the mount index. It is content-addressable: keyed by `fileId` (UNIQUE),
//! mirroring the file row in `doc_mount_files`; multiple hard links may reference
//! the same document via `doc_mount_file_links`. v4 creates a UNIQUE index on
//! `fileId` in `getCollection()` on first access — `generateDDL` from the current
//! schema already emits the columns, and the unique index is created on the fresh
//! fixture by v4's seed pass (`CREATE UNIQUE INDEX IF NOT EXISTS …`), so the Rust
//! port opens a table whose schema is fixed and inserts in schema/Zod field order.
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each
//! delegating straight to `_create`/`_update`/`_delete`). The content-addressable
//! / joined-view query helpers — `findByFileId`, `findManyByFileIds`,
//! `findByMountPointAndPath`, `findManyByMountPointsAndPath`,
//! `findManyByMountPointsInFolder`, `findByMountPointId`, `deleteByMountPointId`
//! (all reads or joins through `doc_mount_file_links`/`doc_mount_files`) — are out
//! of scope here; this ports only the CRUD mutations on the
//! `doc_mount_documents` table itself.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! A **content-text mount-index repo** — plain string columns plus one
//! REAL-affinity int counter:
//!
//!   - **a `plainTextLength` REAL-int column** (`z.number().int().min(0)`). Per
//!     v4's `mapToSQLiteType`, a number maps to INTEGER only with BOTH an integer
//!     `min` AND an integer `max`; `.int().min(0)` is **min-only** → REAL. So it
//!     is bound as `f64`, and the canonical dump's [`super::js_number_to_json`]
//!     collapses an integer-valued REAL (e.g. `0.0`) back to a JSON integer (`0`)
//!     to match v4 byte-for-byte. (Same idiom as `conversation_chunks.interchangeIndex`
//!     and the `doc_mount_points` counters.) Both 0 and >0 banked.
//!   - **a UUID-as-TEXT natural key** (`fileId`, `UUIDSchema`, UNIQUE) — stored as
//!     plain TEXT (`generateCreateTable` emits no FK constraints, so the cross-DB
//!     ref to `doc_mount_files.id` needs no seeded parent).
//!   - **plain required TEXT** (`content`, `contentSha256` — `z.string().length(64)`,
//!     a content-hash mirror, stored as plain TEXT with no length constraint on disk).
//!
//! Every column is required (no nullable columns), so there is no nullable-setter
//! seam here.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned zero-normalization
//! form the prior Phase-2 repos use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a doc mount document (the
/// `Omit<DocMountDocument,'id'|timestamps>` shape). All four non-id/timestamp
/// fields are present and required.
pub struct DmdCreate {
    /// `fileId` — required UUID-as-TEXT, the UNIQUE natural key (a ref to
    /// `doc_mount_files.id`, stored as plain TEXT — no FK constraint emitted).
    pub file_id: String,
    /// `content` — required TEXT (the document's text content).
    pub content: String,
    /// `contentSha256` — required TEXT (`z.string().length(64)`, the sha256 mirror
    /// of `doc_mount_files.sha256`; no on-disk length constraint).
    pub content_sha256: String,
    /// `plainTextLength` — `z.number().int().min(0)` → REAL → `f64`
    /// (integer-valued collapses to a JSON integer in the dump).
    pub plain_text_length: f64,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A document update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved (v4 deletes them off the patch; we
/// never touch them), `updatedAt` is set explicitly. Each `Some` field sets that
/// column; every settable column is covered. All columns are required (no
/// nullable setter needed).
#[derive(Default)]
pub struct DmdUpdate {
    pub file_id: Option<String>,
    pub content: Option<String>,
    pub content_sha256: Option<String>,
    pub plain_text_length: Option<f64>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountDocumentsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountDocumentsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a document with the given pinned id + timestamps. `plainTextLength`
    /// binds `f64` (REAL); the TEXT columns pass through. Columns are bound in
    /// schema/Zod field order (= on-disk order on a fresh fixture).
    pub fn create(&self, data: &DmdCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO doc_mount_documents \
               (id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                opts.id,
                data.file_id,
                data.content,
                data.content_sha256,
                data.plain_text_length,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the document `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &DmdUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_documents WHERE id = ?1",
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

        if let Some(file_id) = &patch.file_id {
            assignments.push(format!("fileId = ?{}", values.len() + 1));
            values.push(Box::new(file_id.clone()));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(content_sha256) = &patch.content_sha256 {
            assignments.push(format!("contentSha256 = ?{}", values.len() + 1));
            values.push(Box::new(content_sha256.clone()));
        }
        if let Some(plain_text_length) = patch.plain_text_length {
            assignments.push(format!("plainTextLength = ?{}", values.len() + 1));
            values.push(Box::new(plain_text_length));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_documents SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the document `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_documents WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
