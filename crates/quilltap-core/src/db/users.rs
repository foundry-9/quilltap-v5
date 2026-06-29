//! The users repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`, and
//! `image_profiles`. Ports v4's `lib/database/repositories/users.repository.ts`
//! (+ the `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom helpers — `getCurrentUser`, `findByEmail`,
//! `findByUsername`, `migrateUserId`, and the GeneralSettings compound ops
//! (`getGeneralSettings` / `updateGeneralSettings`) — are out of scope here. v4's
//! `update` strips `id` and `createdAt` before `_update`, which is a no-op for
//! this port since we preserve both anyway. There is **no built-in guard**.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `users` is an **all-strings + nullable-strings** shape — no booleans, numbers,
//! JSON, or BLOB columns. It exercises the plainest marshaling path: a single
//! required TEXT column (`username`, `z.string().min(3).max(50)`) and five
//! nullable TEXT columns (`email`, `name`, `image`, `emailVerified`,
//! `passwordHash`) where `None` → SQL NULL and `Some` → the string. `email`,
//! `name`, `image`, `passwordHash` are `z.string().nullable().optional()` and
//! `emailVerified` is `TimestampSchema.nullable().optional()` — all TEXT
//! nullable. The seed/create corpus exercises both a fully-populated row (every
//! nullable set, a plausible `passwordHash`, an `emailVerified` timestamp) and a
//! minimal row (username only, all nullables null).
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates`/
//! `conversation_annotations`/`image_profiles` use.
//!
//! Deferred (not in the corpus, mirroring the precedent repos): clearing a
//! nullable column (`email`, `name`, `image`, `emailVerified`, `passwordHash`)
//! back **to NULL** via `update` — the patch models a provided field as "set to
//! this value", so a nullable setter (an `Option<Option<_>>` shape) lands when an
//! op needs it. The corpus only sets nullable columns to non-null values.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a user (the `Omit<User,'id'|timestamps>` shape).
/// `username` is the required TEXT column; the five `Option` fields are the
/// nullable TEXT columns (`None` → SQL NULL).
pub struct UserCreate {
    pub username: String,
    /// `None` => SQL NULL (`email`, `z.email().nullable().optional()`).
    pub email: Option<String>,
    /// `None` => SQL NULL (`name`, `z.string().nullable().optional()`).
    pub name: Option<String>,
    /// `None` => SQL NULL (`image`, `z.string().nullable().optional()`).
    pub image: Option<String>,
    /// `None` => SQL NULL (`emailVerified`, `TimestampSchema.nullable().optional()`).
    pub email_verified: Option<String>,
    /// `None` => SQL NULL (`passwordHash`, `z.string().nullable().optional()`).
    pub password_hash: Option<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A user update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved (v4 deletes them off the patch; we
/// never touch them), `updatedAt` is set explicitly. Each `Some` field sets that
/// column; clearing a nullable column to NULL is deferred (see header).
#[derive(Default)]
pub struct UserUpdate {
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub image: Option<String>,
    pub email_verified: Option<String>,
    pub password_hash: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct UsersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> UsersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a user with the given pinned id + timestamps. `username` is the
    /// required TEXT; the five nullable columns bind as `Option<String>`
    /// (`None` → SQL NULL).
    pub fn create(&self, data: &UserCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO users \
               (id, username, email, name, image, emailVerified, passwordHash, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.username,
                data.email,
                data.name,
                data.image,
                data.email_verified,
                data.password_hash,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the user `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &UserUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row("SELECT 1 FROM users WHERE id = ?1", params![id], |_| Ok(()))
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

        if let Some(username) = &patch.username {
            assignments.push(format!("username = ?{}", values.len() + 1));
            values.push(Box::new(username.clone()));
        }
        if let Some(email) = &patch.email {
            assignments.push(format!("email = ?{}", values.len() + 1));
            values.push(Box::new(email.clone()));
        }
        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(image) = &patch.image {
            assignments.push(format!("image = ?{}", values.len() + 1));
            values.push(Box::new(image.clone()));
        }
        if let Some(email_verified) = &patch.email_verified {
            assignments.push(format!("emailVerified = ?{}", values.len() + 1));
            values.push(Box::new(email_verified.clone()));
        }
        if let Some(password_hash) = &patch.password_hash {
            assignments.push(format!("passwordHash = ?{}", values.len() + 1));
            values.push(Box::new(password_hash.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE users SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the user `id`. Returns `false` when no row matched (v4's `_delete`
    /// "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM users WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
