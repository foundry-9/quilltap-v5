//! The doc-mount-files repository — a **mount-index sibling-DB repo** of Phase 2.
//! Ports v4's `lib/database/repositories/doc-mount-files.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! A `doc_mount_files` row is the **content identity** for a set of bytes — its
//! identity is the bytes (`sha256` is indexed, not UNIQUE). Location and per-link
//! metadata live on `doc_mount_file_links`, not here.
//!
//! ## The sibling DB (mirrors `doc_mount_points`)
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
//! ## The runtime `getCollection()` extras are NO-OPS / harmless on a fresh fixture
//!
//! v4's overridden `getCollection()` does two things on first access beyond
//! creating the table: it runs the `generateDDL` CREATE (which on a fresh fixture
//! produces EXACTLY the current schema columns, in schema/Zod field order — we
//! bind the INSERT in that same order) and it adds a non-UNIQUE sha256 lookup
//! index (`idx_doc_mount_files_sha256`). The index is purely a read-path
//! accelerator — it changes nothing about the persisted row bytes, so it is
//! harmless for the tier-2 state diff. Unlike `doc_mount_points`, this repo has
//! NO runtime ALTER-TABLE migrations.
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each
//! delegating straight to `_create`/`_update`/`_delete`). The content-addressable
//! helpers (`findBySha256`, `findOrCreateByContent`) and the joined-view facades
//! (`findByMountPointId`, `findByMountPointAndPath`, `deleteByMountPointId`) are
//! out of scope here.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! The **narrowest mount-index repo** — no JSON, no boolean, no nullable columns:
//!
//!   - **a required content-fingerprint TEXT** (`sha256`, `z.string().length(64)`)
//!     → plain TEXT.
//!   - **a REAL-affinity int column** (`fileSizeBytes`, `z.number().int().min(0)`).
//!     Per v4's `mapToSQLiteType`, a number maps to INTEGER only with BOTH an
//!     integer min AND an integer max; `.min(0)` alone (no max) → REAL. So this is
//!     REAL — bound as `f64`. The canonical dump's [`super::js_number_to_json`]
//!     collapses an integer-valued REAL (e.g. `5242880.0`) back to a JSON integer
//!     (`5242880`), so it matches v4 byte-for-byte. (Same idiom as
//!     `doc_mount_points.fileCount` / `conversation_annotations.messageIndex` /
//!     `files.size`.) Both 0 and >0 banked.
//!   - **two enum TEXT columns** (`fileType` ∈ {pdf,docx,markdown,txt,json,jsonl,
//!     blob}, `source` ∈ {filesystem,database}) — both stored as plain TEXT.
//!
//! To sidestep Zod-default-replication subtlety the corpus provides ALL fields
//! explicitly on every create, so v4's `source` `.default('filesystem')` never
//! fires and both sides insert identical bytes. [`DmfCreate`] therefore carries
//! all four non-id/timestamp fields.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! Phase-2 repos use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a doc mount file (the
/// `Omit<DocMountFile,'id'|timestamps>` shape). All four non-id/timestamp fields
/// are present so the corpus can set them explicitly (no Zod default fires).
pub struct DmfCreate {
    /// `z.string().length(64)` → required TEXT (the content fingerprint; indexed).
    pub sha256: String,
    /// `z.number().int().min(0)` → REAL → `f64` (integer-valued collapses in dump).
    pub file_size_bytes: f64,
    /// `enum['pdf','docx','markdown','txt','json','jsonl','blob']` → TEXT.
    pub file_type: String,
    /// `enum['filesystem','database'].default('filesystem')` → TEXT (corpus always
    /// sets it, so the default never fires).
    pub source: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A doc-mount-file update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Each `Some` field sets that column. Covers every settable column.
#[derive(Default)]
pub struct DmfUpdate {
    pub sha256: Option<String>,
    pub file_size_bytes: Option<f64>,
    pub file_type: Option<String>,
    pub source: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountFilesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountFilesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a content row with the given pinned id + timestamps. `fileSizeBytes`
    /// binds `f64` (REAL); `sha256`/`fileType`/`source` pass through as TEXT.
    /// Columns are bound in schema/Zod field order (= on-disk order on a fresh
    /// fixture).
    pub fn create(&self, data: &DmfCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO doc_mount_files \
               (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                opts.id,
                data.sha256,
                data.file_size_bytes,
                data.file_type,
                data.source,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the file `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &DmfUpdate) -> Result<bool, DbError> {
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
        if let Some(file_size_bytes) = patch.file_size_bytes {
            assignments.push(format!("fileSizeBytes = ?{}", values.len() + 1));
            values.push(Box::new(file_size_bytes));
        }
        if let Some(file_type) = &patch.file_type {
            assignments.push(format!("fileType = ?{}", values.len() + 1));
            values.push(Box::new(file_type.clone()));
        }
        if let Some(source) = &patch.source {
            assignments.push(format!("source = ?{}", values.len() + 1));
            values.push(Box::new(source.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_files SET {} WHERE id = ?{}",
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
            .execute("DELETE FROM doc_mount_files WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_files WHERE id = ?1",
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
