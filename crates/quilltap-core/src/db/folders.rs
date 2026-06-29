//! The folders repository — the Phase-2 pilot port of v4's
//! `lib/database/repositories/folders.repository.ts` (+ the `_create`/`_update`
//! internals of `base.repository.ts`).
//!
//! Scope (the on-ramp's "repos directly first"): `create` and `update`. v4's
//! folders repo is pure single-table CRUD — `create`/`update` just wrap
//! `_create`/`_update` with logging — so the resulting row state is what these
//! reproduce. Tier-2 verified against the v4 oracle (`folders-tier2`).
//!
//! Determinism: the pilot pins the id and timestamps (v4 honors
//! `CreateOptions.{id,createdAt,updatedAt}` on create and an explicit
//! `updatedAt` on update), so the persisted rows match v4's byte-for-byte with
//! no normalization.
//!
//! The **unpinned** create path (the normal, non-sync app path) is also ported:
//! when an id / timestamp is not supplied, `_create` mints them
//! (`options?.id || generateId()`, `createdAt/updatedAt || now`). `create`
//! returns the id actually used so a caller can wire it into a dependent op
//! (e.g. a child folder's `parentFolderId`). That path is verified by the
//! tier-2 *remap* case, which normalizes the legitimately-nondeterministic
//! generated ids (first-seen remap) and timestamps (placeholder) on both sides.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use crate::clock::now_iso;

use super::DbError;

/// Fields for creating a folder (the `Omit<FolderInput,'id'|timestamps>` shape).
pub struct FolderCreate {
    pub user_id: String,
    pub path: String,
    pub name: String,
    /// `None` => root level (stored as SQL NULL).
    pub parent_folder_id: Option<String>,
    /// `None` => general files, not in a project (stored as SQL NULL).
    pub project_id: Option<String>,
}

/// Id + timestamps (v4's `CreateOptions`). Each field is optional and mirrors
/// `_create`'s defaults: `id = options?.id || generateId()`,
/// `createdAt = options?.createdAt || now`, `updatedAt = options?.updatedAt ||
/// now`. The tier-2 pilot supplies all three (fully deterministic); the remap
/// case supplies none (the minted-values path).
#[derive(Default)]
pub struct CreateOptions {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A folder update patch. Mirrors v4 `_update`: provided fields overwrite, id
/// and createdAt are preserved, updatedAt is set explicitly. The pilot patches
/// `name` + `path` + `updatedAt`; the remaining columns and v4's "updatedAt =
/// now when absent" fallback land when an op needs them.
pub struct FolderUpdate {
    pub name: Option<String>,
    pub path: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct FoldersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> FoldersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a folder, minting id / timestamps that `opts` leaves unset
    /// (v4 `_create`: `id = options?.id || generateId()`, timestamps `|| now`).
    /// Returns the id actually persisted so a caller can reference it.
    pub fn create(&self, data: &FolderCreate, opts: &CreateOptions) -> Result<String, DbError> {
        let id = opts
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = now_iso();
        let created_at = opts.created_at.clone().unwrap_or_else(|| now.clone());
        let updated_at = opts.updated_at.clone().unwrap_or(now);

        self.conn.execute(
            "INSERT INTO folders \
               (id, userId, path, name, parentFolderId, projectId, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                data.user_id,
                data.path,
                data.name,
                data.parent_folder_id,
                data.project_id,
                created_at,
                updated_at,
            ],
        )?;
        Ok(id)
    }

    /// Apply an update patch to the folder `id`. Returns `false` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    pub fn update(&self, id: &str, patch: &FolderUpdate) -> Result<bool, DbError> {
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

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
            "UPDATE folders SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }
}
