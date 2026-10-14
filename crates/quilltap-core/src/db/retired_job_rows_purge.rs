//! The retired job-type purges — P4.D288 (C1-2), v4's two DELETE migrations:
//! `drop-inter-character-memory-jobs-v1` (`ac6348c5f`, Phase D — the
//! INTER_CHARACTER_MEMORY job type is gone) and
//! `drop-housekeeping-consolidation-jobs-v1` (`104e986c7`, Phase F — the
//! MEMORY_HOUSEKEEPING / MEMORY_CONSOLIDATION types gave way to
//! MEMORY_MAINTENANCE). Both `dependsOn: ['sqlite-initial-schema-v1']`.
//!
//! A retired type's queued row would otherwise sit in `background_jobs` with no
//! handler to claim it. Each purge follows v4's `shouldRun` (the table exists
//! and at least one row of the type, ANY status) and `run` (ONE DELETE, the
//! statement copied). Structure-gated, UNSTAMPED (a second run counts zero),
//! FATAL on failure (the caller `?`s it). v4's migration lines are NO-PORT; the
//! host logs v5's ONE boot INFO when a purge deleted rows.

use rusqlite::Connection;

use super::schema_probe::table_exists;
use super::DbError;

/// v4's migration id for the INTER_CHARACTER_MEMORY purge.
pub const INTER_CHARACTER_MIGRATION_ID: &str = "drop-inter-character-memory-jobs-v1";

/// v4's migration id for the housekeeping / consolidation purge.
pub const HOUSEKEEPING_CONSOLIDATION_MIGRATION_ID: &str = "drop-housekeeping-consolidation-jobs-v1";

const INTER_COUNT_SQL: &str =
    r#"SELECT COUNT(*) AS n FROM "background_jobs" WHERE "type" = 'INTER_CHARACTER_MEMORY'"#;
const INTER_DELETE_SQL: &str =
    r#"DELETE FROM "background_jobs" WHERE "type" = 'INTER_CHARACTER_MEMORY'"#;
const HK_COUNT_SQL: &str = r#"SELECT COUNT(*) AS n FROM "background_jobs" WHERE "type" IN ('MEMORY_HOUSEKEEPING', 'MEMORY_CONSOLIDATION')"#;
const HK_DELETE_SQL: &str = r#"DELETE FROM "background_jobs" WHERE "type" IN ('MEMORY_HOUSEKEEPING', 'MEMORY_CONSOLIDATION')"#;

fn purge(main: &Connection, count_sql: &str, delete_sql: &str) -> Result<usize, DbError> {
    if !table_exists(main, "background_jobs")? {
        return Ok(0);
    }
    let n: i64 = main.query_row(count_sql, [], |r| r.get(0))?;
    if n == 0 {
        return Ok(0);
    }
    Ok(main.execute(delete_sql, [])?)
}

/// v4 `drop-inter-character-memory-jobs-v1`: the rows deleted (0 — a no-op).
pub fn purge_inter_character_memory_jobs(main: &Connection) -> Result<usize, DbError> {
    purge(main, INTER_COUNT_SQL, INTER_DELETE_SQL)
}

/// v4 `drop-housekeeping-consolidation-jobs-v1`: the rows deleted (0 — a no-op).
pub fn purge_housekeeping_consolidation_jobs(main: &Connection) -> Result<usize, DbError> {
    purge(main, HK_COUNT_SQL, HK_DELETE_SQL)
}
