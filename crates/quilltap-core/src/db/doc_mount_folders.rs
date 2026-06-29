//! The doc-mount-folders repository — a **mount-index sibling-DB repo** of
//! Phase 2 (after the pilot `group_character_members`). Ports v4's
//! `lib/database/repositories/doc-mount-folders.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! ## What makes this repo special: the sibling DB
//!
//! In v4 this repo overrides `getCollection()` to route all reads/writes to the
//! **dedicated mount-index database** (`quilltap-mount-index.db`) via
//! `getRawMountIndexDatabase()`, isolating mount-tracking data from the main DB
//! so corruption there can never threaten characters/chats/memories. In the Rust
//! port that routing is **not** a property of the repo at all — it is the file
//! the [`super::Writer`] was opened against. `Writer::open_writable` opens any
//! ChaCha20 file by path, so a writer opened on the mount-index DB exposes these
//! repos exactly as a main-DB writer exposes `users`/`folders`. The repo code is
//! therefore identical in shape to a plain main-DB repo; only the harness points
//! it at the mount-index fixture (see the tier-2 case + builder, which target
//! `SQLITE_MOUNT_INDEX_PATH` and read back through `getRawMountIndexDatabase()`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom helpers — `findByMountPointAndPath`, `findChildren`,
//! `findByMountPointId`, `deleteByMountPointId`, the lazy DDL/backfill in
//! `getCollection` — are out of scope here.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! It banks a **nullable-UUID column in a sibling-DB repo**: `parentId`
//! (`UUIDSchema.nullable().optional()` → NULLABLE TEXT, `null` = the mount-point
//! root). `None` → SQL NULL, `Some` → the string — exercising both the null
//! (root folder) and non-null (child folder) paths in one corpus. The remaining
//! columns are plain: `mountPointId` (required UUID-as-TEXT, a cross-DB-ish ref
//! to a mount point, stored as plain TEXT since `generateCreateTable` emits no FK
//! constraints), `name` (required TEXT), and `path` (required TEXT, `''` for
//! root) + the two timestamps.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! plain-base repos use.
//!
//! Deferred (not in the corpus, mirroring the `users.rs` precedent): clearing
//! `parentId` back **to NULL** via `update` — the patch models a provided field
//! as "set to this value", so a nullable setter (an `Option<Option<_>>` shape)
//! lands when an op needs it. The corpus only sets `parentId` to a non-null
//! value or leaves it untouched.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a folder (the `Omit<DocMountFolder,'id'|timestamps>`
/// shape). `mount_point_id`, `name`, and `path` are required TEXT; `parent_id`
/// is the nullable UUID-as-TEXT column (`None` → SQL NULL, the mount-point root).
pub struct DmfCreate {
    /// `mountPointId` — required UUID-as-TEXT, a ref to a mount point (plain TEXT).
    pub mount_point_id: String,
    /// `parentId` — nullable UUID-as-TEXT (`None` → SQL NULL = mount-point root).
    pub parent_id: Option<String>,
    /// `name` — required TEXT (`z.string().min(1)`, folder segment only).
    pub name: String,
    /// `path` — required TEXT (`z.string()`, full relative path; `''` for root).
    pub path: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A folder update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved (v4 deletes them off the patch; we
/// never touch them), `updatedAt` is set explicitly. Each `Some` field sets that
/// column; clearing the nullable `parent_id` to NULL is deferred (see header).
#[derive(Default)]
pub struct DmfUpdate {
    pub mount_point_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountFoldersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountFoldersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a folder with the given pinned id + timestamps. `parent_id` binds
    /// as `Option<String>` (`None` → SQL NULL). Column order matches v4's Zod
    /// field order: id, mountPointId, parentId, name, path, createdAt, updatedAt.
    pub fn create(&self, data: &DmfCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO doc_mount_folders \
               (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                opts.id,
                data.mount_point_id,
                data.parent_id,
                data.name,
                data.path,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the folder `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &DmfUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_folders WHERE id = ?1",
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

        if let Some(mount_point_id) = &patch.mount_point_id {
            assignments.push(format!("mountPointId = ?{}", values.len() + 1));
            values.push(Box::new(mount_point_id.clone()));
        }
        if let Some(parent_id) = &patch.parent_id {
            assignments.push(format!("parentId = ?{}", values.len() + 1));
            values.push(Box::new(parent_id.clone()));
        }
        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(path) = &patch.path {
            assignments.push(format!("path = ?{}", values.len() + 1));
            values.push(Box::new(path.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_folders SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the folder `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_folders WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
