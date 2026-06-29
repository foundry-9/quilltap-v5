//! The terminal-sessions repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `image_profiles`, and the rest. Ports v4's
//! `lib/database/repositories/terminal-sessions.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). The custom query/maintenance helpers — `findByChatId`,
//! `findActiveByChatId`, `cleanupClosedSessions`, `deleteByChatId` — are out of
//! scope here. Single source: `TerminalSessionSchema` from `terminal.types`,
//! used by the in-chat terminal (PTY) session metadata.
//!
//! v4's `create`/`update`/`delete` are thin `safeQuery` wrappers that delegate
//! straight to `_create`/`_update`/`_delete` with NO default injection — the
//! create-input schema (`TerminalSessionCreateSchema`) marks `shell`/`cwd`/
//! `startedAt` optional, but the spawn handler supplies them, and the *stored*
//! `TerminalSessionSchema` requires them. So this port takes them as required
//! `String`s and the tier-2 corpus pins them explicitly (no nondeterministic
//! `now`/default is hit).
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! A clean strings-plus-nullables shape — no boolean, no JSON column:
//!
//!   - three **nullable string columns** (`label`, `exitedAt`, `transcriptPath`,
//!     each `z.string()`/`TimestampSchema` `.nullable().optional()`). `None` →
//!     SQL NULL, `Some` → the text. The seed/create corpus exercises both a fully
//!     populated row and an all-nulls row.
//!   - a **nullable REAL-affinity unbounded-int column** (`exitCode`,
//!     `z.number().int().nullable().optional()`). The Zod field is an integer
//!     with NO `.max()`, and v4's schema-translator (`mapToSQLiteType`) only
//!     assigns INTEGER affinity when a numeric field has BOTH an integer min AND
//!     an integer max — so `exitCode` maps to **REAL**. This port binds it as
//!     `Option<f64>`, not `Option<i64>`. The canonical dump's `js_number_to_json`
//!     collapses an integer-valued REAL cell (`137.0`) back to `137` — matching
//!     how the v4 oracle (better-sqlite3 → JS `Number` → `JSON.stringify`)
//!     renders it — so an integer exit code round-trips byte-for-byte. `None` →
//!     SQL NULL.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`image_profiles`/… use.
//!
//! Deferred (not in the corpus, mirroring the other repos): clearing a nullable
//! column (`label`, `exitedAt`, `exitCode`, `transcriptPath`) back to NULL via
//! `update` — the patch models a provided field as "set to this value", so to
//! avoid an `Option<Option<_>>` setter each field is "set when `Some`"; a
//! nullable setter lands when an op needs it. (The corpus only ever sets these
//! to non-null values — e.g. a session exit fills `exitedAt`/`exitCode`.)

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a terminal session (the `Omit<TerminalSession,'id'|
/// timestamps>` shape). `exit_code` is the nullable REAL-affinity column (bound
/// as `Option<f64>`); `label`/`exited_at`/`transcript_path` are the nullable
/// string columns (`None` → SQL NULL). `shell`/`cwd`/`started_at` are required
/// (v4 injects no defaults — the corpus pins them).
pub struct TsCreate {
    pub chat_id: String,
    /// `None` => SQL NULL.
    pub label: Option<String>,
    pub shell: String,
    pub cwd: String,
    pub started_at: String,
    /// `None` => SQL NULL.
    pub exited_at: Option<String>,
    /// REAL-affinity (int, no `.max()`); `None` => SQL NULL.
    pub exit_code: Option<f64>,
    /// `None` => SQL NULL.
    pub transcript_path: Option<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A terminal-session update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved (v4 deletes `id` off the
/// patch; we never touch id or createdAt), `updatedAt` is set explicitly. Each
/// `Some` field sets that column; clearing a nullable column to NULL is deferred
/// (see the module header).
#[derive(Default)]
pub struct TsUpdate {
    pub label: Option<String>,
    pub shell: Option<String>,
    pub cwd: Option<String>,
    pub started_at: Option<String>,
    pub exited_at: Option<String>,
    /// Re-bound as `f64` (REAL affinity) when provided.
    pub exit_code: Option<f64>,
    pub transcript_path: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct TerminalSessionsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TerminalSessionsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a terminal session with the given pinned id + timestamps.
    /// `exitCode` is bound as `Option<f64>` (REAL affinity);
    /// `label`/`exitedAt`/`transcriptPath` as `Option<String>` (`None` → SQL
    /// NULL).
    pub fn create(&self, data: &TsCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO terminal_sessions \
               (id, chatId, label, shell, cwd, startedAt, exitedAt, exitCode, \
                transcriptPath, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                opts.id,
                data.chat_id,
                data.label,
                data.shell,
                data.cwd,
                data.started_at,
                data.exited_at,
                data.exit_code,
                data.transcript_path,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the terminal session `id`. Returns `Ok(false)`
    /// when no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &TsUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM terminal_sessions WHERE id = ?1",
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

        if let Some(label) = &patch.label {
            assignments.push(format!("label = ?{}", values.len() + 1));
            values.push(Box::new(label.clone()));
        }
        if let Some(shell) = &patch.shell {
            assignments.push(format!("shell = ?{}", values.len() + 1));
            values.push(Box::new(shell.clone()));
        }
        if let Some(cwd) = &patch.cwd {
            assignments.push(format!("cwd = ?{}", values.len() + 1));
            values.push(Box::new(cwd.clone()));
        }
        if let Some(started_at) = &patch.started_at {
            assignments.push(format!("startedAt = ?{}", values.len() + 1));
            values.push(Box::new(started_at.clone()));
        }
        if let Some(exited_at) = &patch.exited_at {
            assignments.push(format!("exitedAt = ?{}", values.len() + 1));
            values.push(Box::new(exited_at.clone()));
        }
        if let Some(exit_code) = patch.exit_code {
            assignments.push(format!("exitCode = ?{}", values.len() + 1));
            values.push(Box::new(exit_code));
        }
        if let Some(transcript_path) = &patch.transcript_path {
            assignments.push(format!("transcriptPath = ?{}", values.len() + 1));
            values.push(Box::new(transcript_path.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE terminal_sessions SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the terminal session `id`. Returns `false` when no row matched
    /// (v4's `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM terminal_sessions WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }
}
