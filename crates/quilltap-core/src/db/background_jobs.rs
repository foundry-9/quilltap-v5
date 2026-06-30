//! The background-jobs repository — a Phase-2 repo port (main DB). Ports v4's
//! `lib/database/repositories/background-jobs.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`). The schema
//! is `lib/schemas/job.types.ts` (`BackgroundJobSchema`).
//!
//! `background_jobs` is the durable work queue: every async unit (memory
//! extraction, context summaries, embedding generation, autonomous room turns,
//! …) is a row here. It extends v4's `UserOwnedBaseRepository`, so it carries a
//! `userId` TEXT column, and it has **no base-method override** — `create` /
//! `update` / `delete` honor pinned id / createdAt / updatedAt (the
//! `text_replacement_rules` shape). On top of the three abstract methods it adds
//! a queue-specific API: atomic claim, completion/failure marking with
//! exponential backoff, pause/resume, and a family of bulk reset / cleanup /
//! query helpers.
//!
//! ## Columns & affinities (verified against v4's `mapToSQLiteType`)
//!
//! `mapToSQLiteType` maps a bare `z.number()` (no integer `.min()` AND `.max()`)
//! to **REAL**. The job schema's three numeric columns are all bare
//! `z.number().default(N)` — `priority`, `attempts`, `maxAttempts` — so all
//! three are **REAL-affinity**, NOT INTEGER (the scoping note that called them
//! INTEGER was wrong; the DDL is what counts). This port therefore binds them as
//! `f64`, exactly like `conversation_annotations.messageIndex`; the canonical
//! dump's `js_number_to_json` collapses the integer-valued REAL cell (`3.0`) back
//! to `3` so it matches the v4 oracle (better-sqlite3 → JS `Number` →
//! `JSON.stringify`). The 14 columns, in schema order:
//!
//!   - `id`          TEXT PK
//!   - `userId`      TEXT
//!   - `type`        TEXT  (enum, 24 variants — stored as the raw string)
//!   - `status`      TEXT  (enum PENDING/PROCESSING/COMPLETED/FAILED/DEAD/PAUSED; schema default PENDING — the create path always sets it explicitly)
//!   - `payload`     TEXT  (open-JSON object `z.record(string,unknown)` → compact JSON via `serde_json::to_string`)
//!   - `priority`    REAL  (bare `z.number().default(0)`)
//!   - `attempts`    REAL  (bare `z.number().default(0)`)
//!   - `maxAttempts` REAL  (bare `z.number().default(3)`)
//!   - `lastError`   TEXT  nullable
//!   - `scheduledAt` TEXT  (ISO timestamp)
//!   - `startedAt`   TEXT  nullable (ISO timestamp)
//!   - `completedAt` TEXT  nullable (ISO timestamp)
//!   - `createdAt`   TEXT  (ISO timestamp)
//!   - `updatedAt`   TEXT  (ISO timestamp)
//!
//! ## Harness form (minted-timestamp placeholder, like `embedding_status`)
//!
//! Several queue ops mint `now` (and `markFailed` mints `now + backoff`)
//! UNCONDITIONALLY from `getCurrentTimestamp()` — they take no pinned timestamp:
//! `claimNextJob`, `markCompleted`, `markFailed`, `pause`, `resume`, `cancel`,
//! `cancelByType`, `resetAllProcessingJobs`, `resetStuckJobs`. So a pure
//! zero-normalization form is impossible. The differential pins ids + createdAt
//! and diffs every DETERMINISTIC column EXACTLY (status, attempts, lastError,
//! payload, priority, maxAttempts — the queue logic), and placeholders only the
//! four mintable timestamp columns (`scheduledAt`, `startedAt`, `completedAt`,
//! `updatedAt`) to `<ts>` on both sides. `createdAt` is honored on create and
//! preserved on update, so it stays exact. This proves, e.g., that `markFailed`
//! picks DEAD vs FAILED correctly (the status is exact) even though its computed
//! `scheduledAt` instant is nondeterministic.
//!
//! ## Open-JSON `payload` seam (TRACKED, seam #5 in phase-2-onramp.md)
//!
//! `payload` is the open/arbitrary-JSON object column, modeled as
//! [`serde_json::Value`] and stored via `serde_json::to_string` (like
//! `plugin_config.config`). A payload with **two or more keys** would expose the
//! key-order divergence (`serde_json::Value`'s `BTreeMap` SORTS keys; v4's
//! `JSON.stringify` preserves INSERTION order), so the corpus keeps every stored
//! payload `{}` or single-key. `markCompleted`'s optional `payload.result` merge
//! is exercised only where the result keeps the payload `{}`/single-key. Close
//! the seam (preserve-insertion-order serializer) before a multi-key payload op
//! lands.
//!
//! ## Nested-JSON path queries (`findPendingForChat` / `findPendingForEntity`)
//!
//! v4 filters on `payload.chatId` / `payload.entityId`; its query translator
//! compiles a dotted key whose head is a JSON column into
//! `json_extract("payload", '$.chatId') = ?` (see
//! `lib/database/backends/sqlite/{query-translator,json-columns}.ts`). This port
//! reproduces that with the same `json_extract` expression.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;

/// The retry-eligible statuses a claim/scan considers, and the cancellable set,
/// kept as `&str` consts so the enum strings match v4 byte-for-byte.
pub const STATUS_PENDING: &str = "PENDING";
pub const STATUS_PROCESSING: &str = "PROCESSING";
pub const STATUS_COMPLETED: &str = "COMPLETED";
pub const STATUS_FAILED: &str = "FAILED";
pub const STATUS_DEAD: &str = "DEAD";
pub const STATUS_PAUSED: &str = "PAUSED";

/// A hydrated `background_jobs` row (the column subset the queue API returns).
/// Numbers are the REAL-affinity columns read back as `f64`.
#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub id: String,
    pub user_id: String,
    pub job_type: String,
    pub status: String,
    /// The raw payload JSON text (as stored). Parse with `serde_json` if needed.
    pub payload: String,
    pub priority: f64,
    pub attempts: f64,
    pub max_attempts: f64,
    pub last_error: Option<String>,
    pub scheduled_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl BackgroundJob {
    /// Read a full row from a `SELECT` whose columns are in the canonical order
    /// (id, userId, type, status, payload, priority, attempts, maxAttempts,
    /// lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt).
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(BackgroundJob {
            id: row.get(0)?,
            user_id: row.get(1)?,
            job_type: row.get(2)?,
            status: row.get(3)?,
            payload: row.get(4)?,
            priority: row.get(5)?,
            attempts: row.get(6)?,
            max_attempts: row.get(7)?,
            last_error: row.get(8)?,
            scheduled_at: row.get(9)?,
            started_at: row.get(10)?,
            completed_at: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }
}

const SELECT_ALL_COLS: &str = "id, userId, type, status, payload, priority, attempts, \
     maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt";

/// Fields for creating a job (the `Omit<BackgroundJob,'id'|timestamps>` shape).
/// `payload` is the open-JSON object column (bound as compact JSON text — kept
/// `{}`/single-key, see the module header). The numeric fields are bound as
/// `f64` (REAL affinity). `status` defaults to PENDING when `None`, mirroring the
/// Zod `.default('PENDING')` the validator applies before insert.
pub struct BjCreate {
    pub user_id: String,
    pub job_type: String,
    /// `None` => "PENDING" (the schema default the validator materializes).
    pub status: Option<String>,
    pub payload: serde_json::Value,
    pub priority: f64,
    pub attempts: f64,
    pub max_attempts: f64,
    pub last_error: Option<String>,
    pub scheduled_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`). create honors all three (no
/// base-method override).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A job update patch (v4 `update` over `_update`: provided fields overwrite, id
/// and createdAt preserved, `updatedAt` set explicitly). Each `Some` sets that
/// column. Nullable columns use `Option<Option<_>>` so a patch can set them to
/// SQL NULL (`Some(None)`) vs leave them (`None`).
#[derive(Default)]
pub struct BjUpdate {
    pub user_id: Option<String>,
    pub job_type: Option<String>,
    pub status: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub priority: Option<f64>,
    pub attempts: Option<f64>,
    pub max_attempts: Option<f64>,
    pub last_error: Option<Option<String>>,
    pub scheduled_at: Option<String>,
    pub started_at: Option<Option<String>>,
    pub completed_at: Option<Option<String>>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct BackgroundJobsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> BackgroundJobsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    // -- CRUD (the three abstract base methods) -----------------------------

    /// Insert a job with the given pinned id + timestamps (v4 `_create`).
    /// `payload` → compact JSON text; numeric fields as `f64`; nullable columns
    /// as `Option<_>` (`None` → SQL NULL). `status` defaults to PENDING.
    pub fn create(&self, data: &BjCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let payload_json = serde_json::to_string(&data.payload)
            .map_err(|e| DbError::Key(format!("payload serialize: {e}")))?;
        let status = data
            .status
            .clone()
            .unwrap_or_else(|| STATUS_PENDING.to_string());

        self.conn.execute(
            "INSERT INTO background_jobs \
               (id, userId, type, status, payload, priority, attempts, maxAttempts, \
                lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                opts.id,
                data.user_id,
                data.job_type,
                status,
                payload_json,
                data.priority,
                data.attempts,
                data.max_attempts,
                data.last_error,
                data.scheduled_at,
                data.started_at,
                data.completed_at,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to job `id` (v4 `_update`). Returns `Ok(false)` when
    /// no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched; `updatedAt` is always set from the patch.
    pub fn update(&self, id: &str, patch: &BjUpdate) -> Result<bool, DbError> {
        if !self.exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(user_id) = &patch.user_id {
            push_set(
                &mut assignments,
                &mut values,
                "userId",
                Box::new(user_id.clone()),
            );
        }
        if let Some(job_type) = &patch.job_type {
            push_set(
                &mut assignments,
                &mut values,
                "type",
                Box::new(job_type.clone()),
            );
        }
        if let Some(status) = &patch.status {
            push_set(
                &mut assignments,
                &mut values,
                "status",
                Box::new(status.clone()),
            );
        }
        if let Some(payload) = &patch.payload {
            let payload_json = serde_json::to_string(payload)
                .map_err(|e| DbError::Key(format!("payload serialize: {e}")))?;
            push_set(
                &mut assignments,
                &mut values,
                "payload",
                Box::new(payload_json),
            );
        }
        if let Some(priority) = patch.priority {
            push_set(
                &mut assignments,
                &mut values,
                "priority",
                Box::new(priority),
            );
        }
        if let Some(attempts) = patch.attempts {
            push_set(
                &mut assignments,
                &mut values,
                "attempts",
                Box::new(attempts),
            );
        }
        if let Some(max_attempts) = patch.max_attempts {
            push_set(
                &mut assignments,
                &mut values,
                "maxAttempts",
                Box::new(max_attempts),
            );
        }
        if let Some(last_error) = &patch.last_error {
            push_set(
                &mut assignments,
                &mut values,
                "lastError",
                Box::new(last_error.clone()),
            );
        }
        if let Some(scheduled_at) = &patch.scheduled_at {
            push_set(
                &mut assignments,
                &mut values,
                "scheduledAt",
                Box::new(scheduled_at.clone()),
            );
        }
        if let Some(started_at) = &patch.started_at {
            push_set(
                &mut assignments,
                &mut values,
                "startedAt",
                Box::new(started_at.clone()),
            );
        }
        if let Some(completed_at) = &patch.completed_at {
            push_set(
                &mut assignments,
                &mut values,
                "completedAt",
                Box::new(completed_at.clone()),
            );
        }
        push_set(
            &mut assignments,
            &mut values,
            "updatedAt",
            Box::new(patch.updated_at.clone()),
        );

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE background_jobs SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );
        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete job `id` (v4 `_delete`). `false` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM background_jobs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // -- Queue API ----------------------------------------------------------

    /// Claim the next available job atomically (v4 `claimNextJob`). Picks one row
    /// with `status IN (PENDING, FAILED)`, `scheduledAt <= now`, and
    /// `attempts < maxAttempts`, ordered by `priority DESC, createdAt ASC`; sets
    /// it to PROCESSING with `startedAt = updatedAt = now` and `attempts += 1`;
    /// returns the post-update row. `None` when nothing is claimable.
    ///
    /// v4 does this with a single `findOneAndUpdate`; here it is a transaction
    /// (SELECT the winning id, then UPDATE that id) so the select-then-update is
    /// atomic against concurrent writers, matching v4's atomicity guarantee. The
    /// minted `now` is nondeterministic — the harness placeholders the timestamp
    /// columns (see the module header).
    pub fn claim_next_job(&self) -> Result<Option<BackgroundJob>, DbError> {
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;

        // Pick the winner. attempts/maxAttempts are REAL → compare as numbers.
        let id: Option<String> = tx
            .query_row(
                "SELECT id FROM background_jobs \
                 WHERE status IN ('PENDING', 'FAILED') \
                   AND scheduledAt <= ?1 \
                   AND attempts < maxAttempts \
                 ORDER BY priority DESC, createdAt ASC \
                 LIMIT 1",
                params![now],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let Some(id) = id else {
            tx.commit()?;
            return Ok(None);
        };

        tx.execute(
            "UPDATE background_jobs \
             SET status = 'PROCESSING', startedAt = ?1, updatedAt = ?1, attempts = attempts + 1 \
             WHERE id = ?2",
            params![now, id],
        )?;

        let job = tx.query_row(
            &format!("SELECT {SELECT_ALL_COLS} FROM background_jobs WHERE id = ?1"),
            params![id],
            BackgroundJob::from_row,
        )?;
        tx.commit()?;
        Ok(Some(job))
    }

    /// Mark job `id` COMPLETED (v4 `markCompleted`): sets status, `completedAt`,
    /// `updatedAt` to now; if `result` is given, merges it into `payload.result`.
    /// Returns the post-update row, or `None` if the row is gone.
    ///
    /// PAYLOAD MERGE — a v5-only path: v4 sets the dotted Mongo key
    /// `payload.result`, but v4's **SQLite backend has no translator for a dotted
    /// JSON sub-key** — `findOneAndUpdate({...,'payload.result':result})` throws
    /// `no such column: payload.result`. So on v4-on-SQLite a `markCompleted` WITH
    /// a result is unreachable (it errors), and only the no-result path actually
    /// runs. The tier-2 differential therefore exercises only the no-result path
    /// (verified against the v4 oracle); the merge here is a forward v5 capability
    /// (v4's evident intent) reproduced via read-modify-write through the pure
    /// [`merge_result_into_payload`] helper, which a unit test covers byte-for-byte.
    /// Kept `{}`/single-key per the open-JSON seam.
    pub fn mark_completed(
        &self,
        id: &str,
        result: Option<&serde_json::Value>,
    ) -> Result<Option<BackgroundJob>, DbError> {
        let now = now_iso();

        // Read the existing payload so we can merge `result` (if any).
        let existing_payload: Option<String> = self
            .conn
            .query_row(
                "SELECT payload FROM background_jobs WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some(existing_payload) = existing_payload else {
            return Ok(None);
        };

        if let Some(result) = result {
            let payload_json = merge_result_into_payload(&existing_payload, result)?;
            self.conn.execute(
                "UPDATE background_jobs \
                 SET status = 'COMPLETED', completedAt = ?1, updatedAt = ?1, payload = ?2 \
                 WHERE id = ?3",
                params![now, payload_json, id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE background_jobs \
                 SET status = 'COMPLETED', completedAt = ?1, updatedAt = ?1 \
                 WHERE id = ?2",
                params![now, id],
            )?;
        }

        self.find_by_id(id)
    }

    /// Mark job `id` FAILED (or DEAD) with retry scheduling (v4 `markFailed`).
    /// Reads the current `attempts` / `maxAttempts`, computes an exponential
    /// backoff `min(30 * 2^attempts, 300)` seconds, sets `scheduledAt = now +
    /// backoff`, picks DEAD when `attempts >= maxAttempts` else FAILED, and sets
    /// `lastError`. Returns the post-update row, or `None` if the row is gone.
    ///
    /// The status decision is DETERMINISTIC (read attempts vs maxAttempts) and is
    /// diffed exactly; only the computed `scheduledAt` instant is
    /// nondeterministic and gets placeholdered. v4 reads `attempts` as the JS
    /// number; here it is the REAL `f64`, so the `>=` and the `2^attempts` use
    /// the same integer value the column holds.
    pub fn mark_failed(
        &self,
        id: &str,
        error_message: &str,
    ) -> Result<Option<BackgroundJob>, DbError> {
        let now_ms = current_unix_ms();

        let current: Option<(f64, f64)> = self
            .conn
            .query_row(
                "SELECT attempts, maxAttempts FROM background_jobs WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((attempts, max_attempts)) = current else {
            return Ok(None);
        };

        // Backoff in seconds: min(30 * 2^attempts, 300). v4 uses Math.pow on the
        // JS number; reproduce with integer arithmetic on the (integer-valued)
        // attempts. 2^attempts can be large, so cap via the min, computing the
        // power in f64 like v4 (Math.pow returns a double) then flooring into the
        // milliseconds add the same way `Date.now() + backoffSeconds*1000` does.
        let backoff_seconds = (30.0_f64 * 2.0_f64.powf(attempts)).min(300.0);
        let scheduled_ms = now_ms + (backoff_seconds * 1000.0) as i64;
        let scheduled_at = crate::clock::iso_from_unix_ms(scheduled_ms);
        let now = now_iso();

        let new_status = if attempts >= max_attempts {
            STATUS_DEAD
        } else {
            STATUS_FAILED
        };

        let affected = self.conn.execute(
            "UPDATE background_jobs \
             SET status = ?1, lastError = ?2, scheduledAt = ?3, updatedAt = ?4 \
             WHERE id = ?5",
            params![new_status, error_message, scheduled_at, now, id],
        )?;
        if affected == 0 {
            return Ok(None);
        }
        self.find_by_id(id)
    }

    /// Pause a PENDING|FAILED job (v4 `pause`): → PAUSED, `updatedAt = now`.
    /// Returns the row, or `None` if not found / not pausable.
    pub fn pause(&self, id: &str) -> Result<Option<BackgroundJob>, DbError> {
        let now = now_iso();
        let affected = self.conn.execute(
            "UPDATE background_jobs SET status = 'PAUSED', updatedAt = ?1 \
             WHERE id = ?2 AND status IN ('PENDING', 'FAILED')",
            params![now, id],
        )?;
        if affected == 0 {
            return Ok(None);
        }
        self.find_by_id(id)
    }

    /// Resume a PAUSED job (v4 `resume`): → PENDING, `scheduledAt = updatedAt =
    /// now`. Returns the row, or `None` if not found / not resumable.
    pub fn resume(&self, id: &str) -> Result<Option<BackgroundJob>, DbError> {
        let now = now_iso();
        let affected = self.conn.execute(
            "UPDATE background_jobs SET status = 'PENDING', scheduledAt = ?1, updatedAt = ?1 \
             WHERE id = ?2 AND status = 'PAUSED'",
            params![now, id],
        )?;
        if affected == 0 {
            return Ok(None);
        }
        self.find_by_id(id)
    }

    /// Cancel a PENDING|FAILED job (v4 `cancel`): → DEAD, lastError "Cancelled by
    /// user", `updatedAt = now`. Returns `true` if a row was modified.
    pub fn cancel(&self, id: &str) -> Result<bool, DbError> {
        let now = now_iso();
        let affected = self.conn.execute(
            "UPDATE background_jobs \
             SET status = 'DEAD', lastError = 'Cancelled by user', updatedAt = ?1 \
             WHERE id = ?2 AND status IN ('PENDING', 'FAILED')",
            params![now, id],
        )?;
        Ok(affected > 0)
    }

    /// Cancel all non-completed jobs of a type (v4 `cancelByType`):
    /// PENDING|FAILED|PROCESSING of `job_type` → DEAD, lastError "Superseded by
    /// new reindex", `updatedAt = now`. Returns the count modified.
    pub fn cancel_by_type(&self, job_type: &str) -> Result<usize, DbError> {
        let now = now_iso();
        let affected = self.conn.execute(
            "UPDATE background_jobs \
             SET status = 'DEAD', lastError = 'Superseded by new reindex', updatedAt = ?1 \
             WHERE type = ?2 AND status IN ('PENDING', 'FAILED', 'PROCESSING')",
            params![now, job_type],
        )?;
        Ok(affected)
    }

    /// Kill ALL PROCESSING jobs on startup (v4 `resetAllProcessingJobs`):
    /// PROCESSING → DEAD, lastError "Orphaned on startup — killed", `updatedAt =
    /// now`. Returns the count modified. (Note the em-dash in the message —
    /// matched byte-for-byte with v4.)
    pub fn reset_all_processing_jobs(&self) -> Result<usize, DbError> {
        let now = now_iso();
        let affected = self.conn.execute(
            "UPDATE background_jobs \
             SET status = 'DEAD', lastError = 'Orphaned on startup — killed', updatedAt = ?1 \
             WHERE status = 'PROCESSING'",
            params![now],
        )?;
        Ok(affected)
    }

    /// Reset stuck PROCESSING jobs (v4 `resetStuckJobs`): PROCESSING with
    /// `startedAt < cutoff` (cutoff = now − `timeout_minutes`) → FAILED, lastError
    /// "Timed out after N minutes", `updatedAt = now`. Returns the count modified.
    /// The cutoff is computed from the system clock, so the SET of rows reset
    /// depends on `now` — the corpus pins `startedAt` far in the past so the
    /// selection is deterministic regardless of when the test runs.
    pub fn reset_stuck_jobs(&self, timeout_minutes: i64) -> Result<usize, DbError> {
        let cutoff_ms = current_unix_ms() - timeout_minutes * 60 * 1000;
        let cutoff = crate::clock::iso_from_unix_ms(cutoff_ms);
        let now = now_iso();
        let message = format!("Timed out after {timeout_minutes} minutes");
        let affected = self.conn.execute(
            "UPDATE background_jobs \
             SET status = 'FAILED', lastError = ?1, updatedAt = ?2 \
             WHERE status = 'PROCESSING' AND startedAt < ?3",
            params![message, now, cutoff],
        )?;
        Ok(affected)
    }

    /// Hard-delete every job whose type AND status match (v4
    /// `deleteByTypesAndStatuses`). Returns the count deleted. Empty input set →
    /// 0 (no-op), matching v4.
    pub fn delete_by_types_and_statuses(
        &self,
        types: &[String],
        statuses: &[String],
    ) -> Result<usize, DbError> {
        if types.is_empty() || statuses.is_empty() {
            return Ok(0);
        }
        let type_ph = placeholders(types.len(), 1);
        let status_ph = placeholders(statuses.len(), 1 + types.len());
        let sql = format!(
            "DELETE FROM background_jobs WHERE type IN ({type_ph}) AND status IN ({status_ph})"
        );
        let mut values: Vec<&dyn ToSql> = Vec::with_capacity(types.len() + statuses.len());
        for t in types {
            values.push(t);
        }
        for s in statuses {
            values.push(s);
        }
        let affected = self.conn.execute(&sql, values.as_slice())?;
        Ok(affected)
    }

    /// Reap COMPLETED|DEAD jobs older than `older_than` (v4 `cleanupOldJobs`,
    /// deprecated single-window reaper keyed on `completedAt`). Returns the count
    /// deleted.
    pub fn cleanup_old_jobs(&self, older_than_iso: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM background_jobs \
             WHERE status IN ('COMPLETED', 'DEAD') AND completedAt < ?1",
            params![older_than_iso],
        )?;
        Ok(affected)
    }

    /// Reap finished jobs on per-status retention windows keyed on `completedAt`
    /// (v4 `cleanupOldJobsByStatus`): COMPLETED before `completed_older_than`,
    /// DEAD before `dead_older_than`. Returns `(completed, dead)` counts.
    pub fn cleanup_old_jobs_by_status(
        &self,
        completed_older_than_iso: &str,
        dead_older_than_iso: &str,
    ) -> Result<(usize, usize), DbError> {
        let completed = self.conn.execute(
            "DELETE FROM background_jobs WHERE status = 'COMPLETED' AND completedAt < ?1",
            params![completed_older_than_iso],
        )?;
        let dead = self.conn.execute(
            "DELETE FROM background_jobs WHERE status = 'DEAD' AND completedAt < ?1",
            params![dead_older_than_iso],
        )?;
        Ok((completed, dead))
    }

    // -- Read helpers -------------------------------------------------------

    /// Find a job by id (v4 `findById`). `None` if absent.
    pub fn find_by_id(&self, id: &str) -> Result<Option<BackgroundJob>, DbError> {
        self.conn
            .query_row(
                &format!("SELECT {SELECT_ALL_COLS} FROM background_jobs WHERE id = ?1"),
                params![id],
                BackgroundJob::from_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(DbError::Sqlite(other)),
            })
    }

    /// All jobs, capped at 1000 (v4 `findAll`, admin/debug). Insertion order is
    /// not guaranteed; callers that need ordering use a dedicated finder.
    pub fn find_all(&self) -> Result<Vec<BackgroundJob>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_ALL_COLS} FROM background_jobs LIMIT 1000"
        ))?;
        let rows = stmt
            .query_map([], BackgroundJob::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Jobs for a user, optional status filter, newest-first, capped at 100 (v4
    /// `findByUserId`).
    pub fn find_by_user_id(
        &self,
        user_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<BackgroundJob>, DbError> {
        match status {
            Some(status) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {SELECT_ALL_COLS} FROM background_jobs \
                     WHERE userId = ?1 AND status = ?2 ORDER BY createdAt DESC LIMIT 100"
                ))?;
                let rows = stmt
                    .query_map(params![user_id, status], BackgroundJob::from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            None => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {SELECT_ALL_COLS} FROM background_jobs \
                     WHERE userId = ?1 ORDER BY createdAt DESC LIMIT 100"
                ))?;
                let rows = stmt
                    .query_map(params![user_id], BackgroundJob::from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }

    /// The N most-recently-updated jobs of a type (v4 `findRecentByType`).
    pub fn find_recent_by_type(
        &self,
        job_type: &str,
        limit: i64,
    ) -> Result<Vec<BackgroundJob>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_ALL_COLS} FROM background_jobs \
             WHERE type = ?1 ORDER BY updatedAt DESC LIMIT ?2"
        ))?;
        let rows = stmt
            .query_map(params![job_type, limit], BackgroundJob::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// PENDING|PROCESSING jobs for a chat (v4 `findPendingForChat`), via the
    /// `json_extract("payload", '$.chatId')` nested-path filter v4's translator
    /// emits, ordered by `priority DESC, createdAt ASC`.
    pub fn find_pending_for_chat(&self, chat_id: &str) -> Result<Vec<BackgroundJob>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_ALL_COLS} FROM background_jobs \
             WHERE json_extract(payload, '$.chatId') = ?1 \
               AND status IN ('PENDING', 'PROCESSING') \
             ORDER BY priority DESC, createdAt ASC"
        ))?;
        let rows = stmt
            .query_map(params![chat_id], BackgroundJob::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// PENDING|PROCESSING jobs for an entity (v4 `findPendingForEntity`), via
    /// `json_extract("payload", '$.entityId')`.
    pub fn find_pending_for_entity(&self, entity_id: &str) -> Result<Vec<BackgroundJob>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SELECT_ALL_COLS} FROM background_jobs \
             WHERE json_extract(payload, '$.entityId') = ?1 \
               AND status IN ('PENDING', 'PROCESSING') \
             ORDER BY priority DESC, createdAt ASC"
        ))?;
        let rows = stmt
            .query_map(params![entity_id], BackgroundJob::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The earliest `scheduledAt` among retry-eligible jobs (PENDING|FAILED with
    /// `attempts < maxAttempts`) (v4 `findNextScheduledAt`). `None` if none.
    pub fn find_next_scheduled_at(&self) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT scheduledAt FROM background_jobs \
                 WHERE status IN ('PENDING', 'FAILED') AND attempts < maxAttempts \
                 ORDER BY scheduledAt ASC LIMIT 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(DbError::Sqlite(other)),
            })
    }

    /// Queue statistics (v4 `getStats`): per-status counts, optionally scoped to
    /// a user. Computed in one aggregating SQL pass (v4 fetches + counts in JS;
    /// the totals are identical).
    pub fn get_stats(&self, user_id: Option<&str>) -> Result<QueueStats, DbError> {
        let mut stats = QueueStats::default();
        let (sql, bind_user): (String, bool) = match user_id {
            Some(_) => (
                "SELECT status, COUNT(*) FROM background_jobs WHERE userId = ?1 GROUP BY status"
                    .to_string(),
                true,
            ),
            None => (
                "SELECT status, COUNT(*) FROM background_jobs GROUP BY status".to_string(),
                false,
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let collect = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, i64)> {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        };
        let rows: Vec<(String, i64)> = if bind_user {
            stmt.query_map(params![user_id.unwrap()], collect)?
                .collect::<Result<_, _>>()?
        } else {
            stmt.query_map([], collect)?.collect::<Result<_, _>>()?
        };
        for (status, count) in rows {
            match status.as_str() {
                STATUS_PENDING => stats.pending = count,
                STATUS_PROCESSING => stats.processing = count,
                STATUS_COMPLETED => stats.completed = count,
                STATUS_FAILED => stats.failed = count,
                STATUS_DEAD => stats.dead = count,
                STATUS_PAUSED => stats.paused = count,
                _ => {}
            }
        }
        Ok(stats)
    }

    /// Active (PENDING|PROCESSING) job counts grouped by type (v4
    /// `getActiveCountsByType`), optionally scoped to a user.
    pub fn get_active_counts_by_type(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<(String, i64)>, DbError> {
        let (sql, bind_user): (String, bool) = match user_id {
            Some(_) => (
                "SELECT type, COUNT(*) FROM background_jobs \
                 WHERE status IN ('PENDING', 'PROCESSING') AND userId = ?1 GROUP BY type"
                    .to_string(),
                true,
            ),
            None => (
                "SELECT type, COUNT(*) FROM background_jobs \
                 WHERE status IN ('PENDING', 'PROCESSING') GROUP BY type"
                    .to_string(),
                false,
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let collect = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, i64)> {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        };
        let rows: Vec<(String, i64)> = if bind_user {
            stmt.query_map(params![user_id.unwrap()], collect)?
                .collect::<Result<_, _>>()?
        } else {
            stmt.query_map([], collect)?.collect::<Result<_, _>>()?
        };
        Ok(rows)
    }

    // -- private ------------------------------------------------------------

    fn exists(&self, id: &str) -> Result<bool, DbError> {
        self.conn
            .query_row(
                "SELECT 1 FROM background_jobs WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .map(|_| true)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(DbError::Sqlite(other)),
            })
    }
}

/// Per-status queue counts (v4's `QueueStats`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueueStats {
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead: i64,
    pub paused: i64,
}

/// Push a `col = ?N` assignment + its bound value, numbering positionally.
fn push_set(
    assignments: &mut Vec<String>,
    values: &mut Vec<Box<dyn ToSql>>,
    col: &str,
    value: Box<dyn ToSql>,
) {
    assignments.push(format!("{col} = ?{}", values.len() + 1));
    values.push(value);
}

/// `?start, ?start+1, …` for `count` positional placeholders.
fn placeholders(count: usize, start: usize) -> String {
    (0..count)
        .map(|i| format!("?{}", start + i))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Current wall-clock in Unix milliseconds (the pre-format value v4 reaches via
/// `Date.now()`), used by the backoff / cutoff math.
fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

/// Merge `result` into the `result` key of an existing payload JSON object (the
/// v5 forward-capability behind `markCompleted`'s optional result — see that
/// method's note on why v4-on-SQLite can't run this path). A non-object existing
/// payload is treated as empty. Pure, so the byte output is unit-tested directly.
fn merge_result_into_payload(
    existing_payload: &str,
    result: &serde_json::Value,
) -> Result<String, DbError> {
    let mut payload: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str::<serde_json::Value>(existing_payload)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
    payload.insert("result".to_string(), result.clone());
    serde_json::to_string(&serde_json::Value::Object(payload))
        .map_err(|e| DbError::Key(format!("payload serialize: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_result_into_empty_payload() {
        let out = merge_result_into_payload("{}", &json!({ "summary": "done" })).unwrap();
        assert_eq!(out, r#"{"result":{"summary":"done"}}"#);
    }

    #[test]
    fn merge_result_overwrites_existing_result_key() {
        let out = merge_result_into_payload(r#"{"result":"old"}"#, &json!("new")).unwrap();
        assert_eq!(out, r#"{"result":"new"}"#);
    }

    #[test]
    fn merge_result_into_non_object_payload_treated_as_empty() {
        // A non-object stored payload (shouldn't happen for jobs, but be defensive)
        // is treated as an empty object before the merge.
        let out = merge_result_into_payload("null", &json!(42)).unwrap();
        assert_eq!(out, r#"{"result":42}"#);
    }
}
