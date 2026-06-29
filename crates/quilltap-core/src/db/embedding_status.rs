//! The embedding-status repository — a Phase-2 repo port. Ports v4's
//! `lib/database/repositories/embedding-status.repository.ts`.
//!
//! Scope: `create`, `update`, and `delete`. The custom helpers —
//! `upsertByEntity`/`markAsEmbedded`/`markAsFailed`/`markAllPendingByProfileId`/
//! the by-entity / by-profile finders and deletes / `getStatsByProfileId` — are
//! out of scope. `embedding_status` tracks the embedding status of entities
//! (memories, files, etc.) per embedding profile.
//!
//! ## ⚠️ This repo OVERRIDES the base `create`/`update` — `updatedAt` is minted,
//! not pinnable
//!
//! Like `tfidf_vocabulary` (and unlike `folders`/`tags`, which delegate to the
//! base repo's `_create`/`_update`), v4's `EmbeddingStatusRepository` **does not**
//! delegate to `_create`/`_update`. Its bespoke `create` and `update` set
//! `updatedAt = this.getCurrentTimestamp()` UNCONDITIONALLY:
//!
//!   - `create`: `id = options?.id || generateId()`, `createdAt =
//!     options?.createdAt || now`, but `updatedAt = now` — `options.updatedAt` is
//!     **ignored**.
//!   - `update`: strips `id`/`createdAt` off the patch, then `$set: { ...patch,
//!     updatedAt: now }` — any `updatedAt` in the patch is **overwritten** with
//!     `now`.
//!
//! So this port mints `updatedAt` itself via [`crate::clock::now_iso`] (v4's
//! `new Date().toISOString()`), exactly as v4 does. `id` + `createdAt` are still
//! pinnable (create honors both), so the tier-2 differential pins those and
//! **placeholder-normalizes only `updatedAt`** — the minted-timestamp form
//! established by `folders_remap` / `tfidf_vocabulary`, applied to a single column
//! with ids + createdAt left exact.
//!
//! ## Marshaling surface this repo banks
//!
//! All columns are TEXT or NULL — no booleans, numbers, JSON, or BLOB:
//!   - **UUID → TEXT** strings: `userId`, `entityId`, `profileId` (and `id`).
//!   - **enum → TEXT**: `entityType`
//!     (`MEMORY`/`FILE`/`HELP_DOC`/`CONVERSATION_CHUNK`/`MOUNT_CHUNK`) and `status`
//!     (`PENDING`/`EMBEDDED`/`FAILED`, default `PENDING` — set explicitly in every
//!     corpus row). Both bind their plain `String`.
//!   - **nullable TEXT**: `embeddedAt` (`TimestampSchema.nullable().optional()`)
//!     and `error` (`z.string().nullable().optional()`) — `Option<String>`, NULL
//!     when absent.
//!   - `createdAt` TEXT (PINNED via options); `updatedAt` TEXT (MINTED —
//!     placeholdered in the differential).

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock;

/// Fields for creating an embedding-status record (the `Omit<EmbeddingStatus,
/// 'id'|'createdAt'|'updatedAt'>` shape). All TEXT; `entityType`/`status` are
/// enum strings; `embedded_at`/`error` are nullable TEXT (`Option<String>`, NULL
/// when `None`).
pub struct EsCreate {
    pub user_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub profile_id: String,
    pub status: String,
    /// Nullable timestamp TEXT — `None` → NULL.
    pub embedded_at: Option<String>,
    /// Nullable error message TEXT — `None` → NULL.
    pub error: Option<String>,
}

/// Pinned id + createdAt (v4's `CreateOptions`). NOTE: this repo's `create`
/// IGNORES `options.updatedAt` (it always mints `updatedAt = now`), so there is
/// no `updated_at` field here — mirroring the override.
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
}

/// An embedding-status update patch. Mirrors v4 `update`: provided fields
/// overwrite, id and createdAt are preserved, and `updatedAt` is set to `now`
/// (minted internally — NOT a field here, because v4 overwrites any caller value).
#[derive(Default)]
pub struct EsUpdate {
    pub user_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub profile_id: Option<String>,
    pub status: Option<String>,
    pub embedded_at: Option<String>,
    pub error: Option<String>,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct EmbeddingStatusRepository<'c> {
    conn: &'c Connection,
}

impl<'c> EmbeddingStatusRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert an embedding-status record. `id` + `createdAt` come from `opts`;
    /// `updatedAt` is MINTED here (`clock::now_iso`), exactly as v4's override
    /// (`updatedAt = now`, ignoring `options.updatedAt`). All values bind their
    /// plain `String`; `embedded_at`/`error` bind `Option<String>` (NULL when
    /// `None`).
    pub fn create(&self, data: &EsCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let now = clock::now_iso();
        self.conn.execute(
            "INSERT INTO embedding_status \
               (id, userId, entityType, entityId, profileId, status, embeddedAt, \
                error, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                opts.id,
                data.user_id,
                data.entity_type,
                data.entity_id,
                data.profile_id,
                data.status,
                data.embedded_at,
                data.error,
                opts.created_at,
                now,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the status `id`. Returns `Ok(false)` when no row
    /// matched (v4's "modifiedCount === 0 -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set to a
    /// freshly minted `now` (v4 overwrites any caller-supplied value).
    ///
    /// `embedded_at` / `error` are nullable columns: a `Some(None)`-style "set to
    /// NULL" is not expressible here (the patch field is a single `Option`), which
    /// matches the corpus — updates that clear them are out of scope. A `Some(v)`
    /// binds the value; `None` leaves the column untouched.
    pub fn update(&self, id: &str, patch: &EsUpdate) -> Result<bool, DbError> {
        // v4's bespoke update issues an `updateOne({ id }, ...)`; a missing row
        // yields `modifiedCount === 0 -> null` (a no-op).
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let now = clock::now_iso();
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(user_id) = &patch.user_id {
            assignments.push(format!("userId = ?{}", values.len() + 1));
            values.push(Box::new(user_id.clone()));
        }
        if let Some(entity_type) = &patch.entity_type {
            assignments.push(format!("entityType = ?{}", values.len() + 1));
            values.push(Box::new(entity_type.clone()));
        }
        if let Some(entity_id) = &patch.entity_id {
            assignments.push(format!("entityId = ?{}", values.len() + 1));
            values.push(Box::new(entity_id.clone()));
        }
        if let Some(profile_id) = &patch.profile_id {
            assignments.push(format!("profileId = ?{}", values.len() + 1));
            values.push(Box::new(profile_id.clone()));
        }
        if let Some(status) = &patch.status {
            assignments.push(format!("status = ?{}", values.len() + 1));
            values.push(Box::new(status.clone()));
        }
        if let Some(embedded_at) = &patch.embedded_at {
            assignments.push(format!("embeddedAt = ?{}", values.len() + 1));
            values.push(Box::new(embedded_at.clone()));
        }
        if let Some(error) = &patch.error {
            assignments.push(format!("error = ?{}", values.len() + 1));
            values.push(Box::new(error.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(now));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE embedding_status SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the status `id`. Returns `Ok(false)` when no row matched (v4's
    /// `deletedCount > 0 -> true` else `false`).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM embedding_status WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — the `update` precondition (a missing
    /// target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM embedding_status WHERE id = ?1",
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
