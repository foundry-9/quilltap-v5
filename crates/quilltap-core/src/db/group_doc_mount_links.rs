//! The group-doc-mount-links repository — a **mount-index sibling-DB repo** of
//! Phase 2 (a near-clone of the `group_character_members` pilot that established
//! the sibling-DB machinery). Ports v4's
//! `lib/database/repositories/group-doc-mount-links.repository.ts` (+ the
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
//! base repo). The custom helpers — `findByGroupId`, `findByMountPointId`,
//! `link`, `unlink`, `deleteByGroupId` — are out of scope here.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! Nothing new on the marshaling axis — it is the **plainest possible join
//! table**, structurally identical to the `group_character_members` pilot: `id` +
//! two required UUID-as-TEXT columns (`groupId`, `mountPointId`) + the two
//! timestamps. `groupId` is a cross-DB ref to `groups.id` in the MAIN db, stored
//! as plain TEXT since `generateCreateTable` emits no FK constraints; the link
//! itself lives in the mount-index DB for co-location with mount data. What it
//! banks is a second proof that the sibling-DB round-trip holds.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! plain-base repos use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a link (the `Omit<GroupDocMountLink,'id'|timestamps>`
/// shape). Both columns are required TEXT (UUID strings).
pub struct GdmlCreate {
    /// `groupId` — cross-db ref to `groups.id` in the MAIN db (plain TEXT here).
    pub group_id: String,
    /// `mountPointId` — ref to a mount point (plain TEXT, no FK constraint).
    pub mount_point_id: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A link update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved, `updatedAt` is set explicitly.
#[derive(Default)]
pub struct GdmlUpdate {
    pub group_id: Option<String>,
    pub mount_point_id: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct GroupDocMountLinksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> GroupDocMountLinksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a link with the given pinned id + timestamps.
    pub fn create(&self, data: &GdmlCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO group_doc_mount_links \
               (id, groupId, mountPointId, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                opts.id,
                data.group_id,
                data.mount_point_id,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the link `id`. Returns `Ok(false)` when no
    /// row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &GdmlUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM group_doc_mount_links WHERE id = ?1",
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

        if let Some(group_id) = &patch.group_id {
            assignments.push(format!("groupId = ?{}", values.len() + 1));
            values.push(Box::new(group_id.clone()));
        }
        if let Some(mount_point_id) = &patch.mount_point_id {
            assignments.push(format!("mountPointId = ?{}", values.len() + 1));
            values.push(Box::new(mount_point_id.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE group_doc_mount_links SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the link `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM group_doc_mount_links WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// v4 `findByGroupId` — every mount-point id linked to this group. Used by the
    /// provisioning flow's adopt branch ([`super::ensure_official_store`]).
    pub fn find_by_group_id(&self, group_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT mountPointId FROM group_doc_mount_links WHERE groupId = ?1")?;
        let ids = stmt
            .query_map(params![group_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// v4 `link` (`group-doc-mount-links.repository.ts:154`): find-or-create the
    /// `(groupId, mountPointId)` link, idempotently. Returns without touching the
    /// DB when the pair already exists; otherwise mints id + timestamps and
    /// inserts (the `_create` path). Used by provisioning to attach a freshly
    /// created store to the group.
    pub fn link(&self, group_id: &str, mount_point_id: &str) -> Result<(), DbError> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM group_doc_mount_links WHERE groupId = ?1 AND mountPointId = ?2",
                params![group_id, mount_point_id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if existing.is_some() {
            return Ok(());
        }
        let now = crate::clock::now_iso();
        self.create(
            &GdmlCreate {
                group_id: group_id.to_string(),
                mount_point_id: mount_point_id.to_string(),
            },
            &CreateOptions {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
                updated_at: now,
            },
        )
    }
}
