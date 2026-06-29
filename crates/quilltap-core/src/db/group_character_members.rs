//! The group-character-members repository — the **first mount-index sibling-DB
//! repo** of Phase 2. Ports v4's
//! `lib/database/repositories/group-character-members.repository.ts` (+ the
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
//! base repo). The custom helpers — `findByGroupId`, `findByCharacterId`,
//! `deleteByGroupId`, `deleteByCharacterId`, the membership add/remove
//! convenience ops — are out of scope here.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! Nothing new on the marshaling axis — it is the **plainest possible join
//! table**: `id` + two required UUID-as-TEXT columns (`groupId`, `characterId`,
//! both cross-DB refs to `groups.id` / `characters.id` in the MAIN db, stored as
//! plain TEXT since `generateCreateTable` emits no FK constraints) + the two
//! timestamps. What it banks is the **machinery**: the tier-2 fixture/oracle now
//! prove a sibling-DB repo round-trips, unlocking the rest of the mount-index
//! family.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! plain-base repos use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a membership (the `Omit<GroupCharacterMember,'id'|timestamps>`
/// shape). Both columns are required TEXT (UUID strings).
pub struct GcmCreate {
    /// `groupId` — cross-db ref to `groups.id` in the MAIN db (plain TEXT here).
    pub group_id: String,
    /// `characterId` — cross-db ref to `characters.id` in the MAIN db (plain TEXT).
    pub character_id: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A membership update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved, `updatedAt` is set explicitly.
#[derive(Default)]
pub struct GcmUpdate {
    pub group_id: Option<String>,
    pub character_id: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct GroupCharacterMembersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> GroupCharacterMembersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a membership with the given pinned id + timestamps.
    pub fn create(&self, data: &GcmCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO group_character_members \
               (id, groupId, characterId, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                opts.id,
                data.group_id,
                data.character_id,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the membership `id`. Returns `Ok(false)` when no
    /// row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &GcmUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM group_character_members WHERE id = ?1",
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
        if let Some(character_id) = &patch.character_id {
            assignments.push(format!("characterId = ?{}", values.len() + 1));
            values.push(Box::new(character_id.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE group_character_members SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the membership `id`. Returns `false` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM group_character_members WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }
}
