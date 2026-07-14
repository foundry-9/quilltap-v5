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

/// A `folders` row hydrated in full (v4 `Folder`) — the shape the files-family
/// folders routes read (`findByUserId` GET map, `findByPath`, the rename/delete
/// arms).
#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: String,
    pub user_id: String,
    pub path: String,
    pub name: String,
    pub parent_folder_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const FOLDER_SELECT_ALL: &str = "SELECT id, userId, path, name, parentFolderId, projectId, \
     createdAt, updatedAt FROM folders";

fn map_folder_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FolderRow> {
    Ok(FolderRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        parent_folder_id: row.get(4)?,
        project_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
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

    /// v4 base `findByUserId(userId)` — every folder owned by the user, in
    /// natural/rowid order (the folders GET route sorts by `path.localeCompare`
    /// afterward).
    pub fn find_by_user_id(&self, user_id: &str) -> Result<Vec<FolderRow>, DbError> {
        let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id], map_folder_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByPath(userId, path, projectId)` — the single folder at a path in a
    /// scope (`findOneByFilter`). Nullable `projectId` via v4's `createNullableFilter`
    /// (null → `IS NULL`, else `= ?`). `None` when absent.
    pub fn find_by_path(
        &self,
        user_id: &str,
        path: &str,
        project_id: Option<&str>,
    ) -> Result<Option<FolderRow>, DbError> {
        let result = match project_id {
            Some(pid) => self.conn.query_row(
                &format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND path = ?2 AND projectId = ?3 LIMIT 1"),
                params![user_id, path, pid],
                map_folder_row,
            ),
            None => self.conn.query_row(
                &format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND path = ?2 AND projectId IS NULL LIMIT 1"),
                params![user_id, path],
                map_folder_row,
            ),
        };
        result.map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.into()),
        })
    }

    /// v4 `updatePathPrefix(userId, oldPrefix, newPrefix, projectId)` — rewrite
    /// every folder in-scope whose `path` starts with `old_prefix`, replacing the
    /// FIRST occurrence (v4 `String.replace` semantics; the prefix is at index 0)
    /// and minting a fresh `updatedAt` via [`Self::update`]. Returns the count
    /// updated. Called AFTER the renamed folder itself is updated, so it counts only
    /// descendants (the route adds `+1`).
    pub fn update_path_prefix(
        &self,
        user_id: &str,
        old_prefix: &str,
        new_prefix: &str,
        project_id: Option<&str>,
    ) -> Result<usize, DbError> {
        let all = self.find_all_in_scope(user_id, project_id)?;
        let mut updated = 0usize;
        for folder in all.into_iter().filter(|f| f.path.starts_with(old_prefix)) {
            let new_path = folder.path.replacen(old_prefix, new_prefix, 1);
            self.update(
                &folder.id,
                &FolderUpdate {
                    name: None,
                    path: Some(new_path),
                    updated_at: now_iso(),
                },
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    /// v4 `hasChildren(folderId)` — `count({parentFolderId: folderId}) > 0`.
    pub fn has_children(&self, folder_id: &str) -> Result<bool, DbError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM folders WHERE parentFolderId = ?1",
            params![folder_id],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// v4 `delete(folderId)` — `_delete`. Returns `false` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `findAllInProject(userId, projectId)` reduced to the rows the rename
    /// prefix-rewrite needs (nullable `projectId`). Rowid order (the caller filters
    /// + counts, so order is not observable).
    fn find_all_in_scope(
        &self,
        user_id: &str,
        project_id: Option<&str>,
    ) -> Result<Vec<FolderRow>, DbError> {
        match project_id {
            Some(pid) => {
                let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND projectId = ?2");
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id, pid], map_folder_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND projectId IS NULL");
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id], map_folder_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }
}
