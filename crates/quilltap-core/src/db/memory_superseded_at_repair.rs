//! The `memories.supersededAt` boot ensure — P4.D288 (C1-2), v4 `c2795733f`'s
//! migration `add-memory-superseded-at-v1` (bug 225 — "the cold-retention
//! clock"; `dependsOn: ['add-memory-tiers-v1']`, registered right after it).
//!
//! `coldRetentionDays` ages a superseded cold row by `supersededAt`, never by
//! `updatedAt`, which every link rewrite resets. v4 adds the column (`TEXT
//! DEFAULT NULL`, the MIGRATION's text — generateDDL writes bare `TEXT` in its
//! schema slot) and backfills it from `updatedAt` for rows a digest already
//! superseded. [`ensure_memory_superseded_at`] follows v4's `assessWork` /
//! `shouldRun` / `run` (`migrations/scripts/add-memory-superseded-at-v1.ts`):
//!
//!   - work is needed when `memories` exists and the column is absent, OR a
//!     superseded row (`supersededById IS NOT NULL`) is unstamped;
//!   - the column, when absent, through `addColumnIfMissing`;
//!   - whenever `memories.supersededById` exists, v4's ONE backfill UPDATE,
//!     byte for byte.
//!
//! Structure-gated, UNSTAMPED: v4's migration converges by itself (a second
//! run finds the column present and no unstamped superseded row), so a v4 boot
//! over a v5-ensured instance meets `shouldRun` false. FATAL on failure — the
//! caller `?`s it (v4's migration is not `resumable`). No transaction wraps the
//! two steps, as v4's has none. v4's migration lines are NO-PORT (the runner is
//! deferred); the host logs v5's ONE boot INFO when the outcome is not a no-op.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, column_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-memory-superseded-at-v1";

/// The column and the MIGRATION's DDL text.
pub const COLUMN: &str = "supersededAt";
const DECL: &str = "TEXT DEFAULT NULL";

/// v4's backfill, the bytes copied (line breaks and indentation included).
const BACKFILL_SQL: &str = "UPDATE memories
                SET supersededAt = updatedAt
              WHERE supersededById IS NOT NULL AND supersededAt IS NULL";

/// What one [`ensure_memory_superseded_at`] run did; the default is a no-op.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupersededAtEnsureOutcome {
    /// `memories.supersededAt` was added.
    pub column_added: bool,
    /// Superseded rows stamped from `updatedAt` (v4's `stamped.changes`).
    pub backfilled_rows: usize,
}

impl SupersededAtEnsureOutcome {
    /// True when the run changed nothing — the boot logs no line then.
    pub fn is_noop(&self) -> bool {
        self == &Self::default()
    }
}

/// v4's `assessWork()`: `(needsColumn, unstampedRows)`.
fn assess_work(main: &Connection) -> Result<(bool, i64), DbError> {
    if !table_exists(main, "memories")? {
        return Ok((false, 0));
    }
    let needs_column = !column_exists(main, "memories", COLUMN)?;
    if !column_exists(main, "memories", "supersededById")? {
        return Ok((needs_column, 0));
    }
    let sql = if needs_column {
        "SELECT COUNT(*) AS n FROM memories WHERE supersededById IS NOT NULL"
    } else {
        "SELECT COUNT(*) AS n FROM memories WHERE supersededById IS NOT NULL AND supersededAt IS NULL"
    };
    let unstamped: i64 = main.query_row(sql, [], |r| r.get(0))?;
    Ok((needs_column, unstamped))
}

/// Run v4's `add-memory-superseded-at-v1` when its `shouldRun` would.
pub fn ensure_memory_superseded_at(
    main: &Connection,
) -> Result<SupersededAtEnsureOutcome, DbError> {
    let mut outcome = SupersededAtEnsureOutcome::default();
    let (needs_column, unstamped) = assess_work(main)?;
    if !needs_column && unstamped == 0 {
        return Ok(outcome);
    }
    if needs_column && add_column_if_missing(main, "memories", COLUMN, DECL)? {
        outcome.column_added = true;
    }
    if table_exists(main, "memories")? && column_exists(main, "memories", "supersededById")? {
        outcome.backfilled_rows = main.execute(BACKFILL_SQL, [])?;
    }
    Ok(outcome)
}
