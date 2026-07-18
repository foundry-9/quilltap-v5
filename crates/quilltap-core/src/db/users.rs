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
//! nullable column back **to NULL** via `update`. **P4.9c closed this for
//! `image`** — the avatar-clear arm of v4's `PATCH ?action=set-avatar` writes a
//! literal null, so `UserUpdate.image` is now the `Option<Option<_>>` nullable
//! setter the paragraph anticipated. `email`, `name`, `emailVerified`, and
//! `passwordHash` keep the single-`Option` shape until an op needs otherwise.
//!
//! P4.9c also added the two scoped reads the profile surface consumes:
//! [`find_profile_by_id`] and [`find_id_by_email`].

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
/// column.
///
/// `image` is the ONE nullable setter (`Option<Option<String>>`) the header's
/// deferral anticipated: v4's `PATCH ?action=set-avatar` writes
/// `{ image: imageId ? '/api/v1/files/<id>' : null }`, so clearing an avatar is
/// a genuine SET-TO-NULL, not an absent field. The other five nullables keep the
/// single-`Option` "absent or set to this value" shape until an op needs more.
#[derive(Default)]
pub struct UserUpdate {
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    /// Outer `None` — leave the column alone. `Some(None)` — SET NULL.
    pub image: Option<Option<String>>,
    pub email_verified: Option<String>,
    pub password_hash: Option<String>,
    pub updated_at: String,
}

/// The profile projection v4's `GET /api/v1/user/profile` returns
/// (`route.ts:92-102`) — the seven columns the response object names, in its
/// field order. A SCOPED read, following [`find_name_by_id`]'s precedent rather
/// than marshaling the whole row: `passwordHash` and `emailVerified` are not
/// part of any profile response, and a struct that carries a password hash
/// around is a liability the consumer never asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct UserProfileRow {
    pub id: String,
    pub email: Option<String>,
    pub username: String,
    pub name: Option<String>,
    pub image: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// v4 `repos.users.findById(userId)`, projected to the profile columns.
/// `None` = no row (v4's `findById` → `null`, which the route turns into its
/// `serverError('User not found')`).
pub fn find_profile_by_id(conn: &Connection, id: &str) -> Result<Option<UserProfileRow>, DbError> {
    conn.query_row(
        "SELECT id, email, username, name, image, createdAt, updatedAt \
         FROM users WHERE id = ?1",
        params![id],
        |row| {
            Ok(UserProfileRow {
                id: row.get(0)?,
                email: row.get(1)?,
                username: row.get(2)?,
                name: row.get(3)?,
                image: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}

/// v4 `repos.users.findByEmail(email)` — `findOneByFilter({ email })` — reduced
/// to the one column its only caller reads (the PUT's uniqueness check compares
/// `existingUser.id !== user.id`). SQLite's TEXT `=` is case-SENSITIVE and the
/// generated `users.email` column carries no `NOCASE` collation, so two
/// addresses differing only in case do NOT collide here — exactly as in v4.
pub fn find_id_by_email(conn: &Connection, email: &str) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT id FROM users WHERE email = ?1 LIMIT 1",
        params![email],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
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
        // The nullable setter: `Some(None)` binds SQL NULL (the avatar-clear
        // arm), `Some(Some(v))` binds the value, `None` leaves the column alone.
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

/// **Scoped read** for the user-identity resolver
/// ([`crate::services::user_identity_resolver`]): the profile `name` of a user,
/// v4 `users.findById(userId)?.name`. Follows the scoped-read precedent
/// [`super::chat_settings::find_auto_housekeeping_settings_by_user_id`] — it
/// marshals ONLY the one column the consumer reads, not the full `User` row (the
/// complete `findById` read marshaling is a later users read sub-unit).
///
/// The two levels of `Option` are load-bearing and mirror v4's `?.name` guard:
///   * outer `None` — no `users` row for this id (v4 `findById` → `null`).
///   * inner `None` — the row exists but `name` is SQL NULL (`z.string()
///     .nullable().optional()`); v4's `userProfile?.name` is then falsy, so the
///     resolver falls through to the `'User'` default. The two absent shapes
///     collapse identically downstream, but are kept distinct for fidelity.
pub fn find_name_by_id(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<Option<String>>, DbError> {
    conn.query_row(
        "SELECT name FROM users WHERE id = ?1",
        params![user_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
}
