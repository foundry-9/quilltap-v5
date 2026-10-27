//! The terminal-geometry boot ensure — P4.D337, the FIFTEENTH D23 move (v4
//! `a6292f169`, bug 255 — migration `add-terminal-session-geometry-v1`,
//! `dependsOn: ['add-terminal-sessions-table-v1']`, `introducedInVersion
//! '4.11.0'`).
//!
//! v4's why, carried: `terminal_sessions.cols` / `rows` hold the terminal's
//! size at spawn, updated to its final value on exit. `terminal_read` replays
//! an exited session's transcript through a terminal emulator at this size
//! (bug 255). No backfill: older sessions read back at 80×24.
//!
//! v4 registers the migration LAST (`migrations/scripts/index.ts:928`, after
//! the Inventory's two column migrations). v5's migration runner is a locked
//! deferral, so the column add is re-homed as a boot ensure over the main
//! partition, run by the host AFTER P4.D324's Inventory ensures and BEFORE the
//! P4.160 index-family backfill (which must stay last; this ensure makes no
//! index). `dependsOn` is satisfied by construction: v5 has no twin of
//! `add-terminal-sessions-table-v1` — the table comes from `fresh_schema.json`
//! at provisioning.
//!
//! ## TWO shapes (the P4.D249 class, NOT the P4.D304 byte-equal one)
//!
//! A FRESH instance renders generateDDL's `"cols" REAL,` / `"rows" REAL,` in
//! schema order (between `transcriptPath` and `createdAt`) — a
//! `z.number().int()` with no integer `.min()` AND `.max()` maps to REAL
//! (`schema-translator.ts`), exactly `exitCode`'s case; those are the bytes of
//! P4.D324's `fresh_schema.json` re-dump, read here, never written. An
//! EXISTING instance gets the migration's appended `INTEGER DEFAULT NULL`
//! clauses, `cols` then `rows`. Both are legal: `DEFAULT NULL` is semantically
//! no default. The same `f64` bind lands as a REAL `80.0` on the fresh shape
//! and an INTEGER `80` on the migrated one; every reader collapses either to
//! `Some(80)`.
//!
//! ## No stamp, fatal
//!
//! v4's `shouldRun` (`isSQLiteBackend() ∧ sqliteTableExists('terminal_sessions')
//! ∧ (¬cols ∨ ¬rows)`) is column-gated: once this ensure appended both, v4's
//! runner answers `not needed` and records nothing, every boot. Nothing later
//! drops the columns, so this is the `chat_informs.permanent` class, NOT the
//! `add-memory-tiers-v1` ping-pong one — no `migrations_state` row is written
//! (the ruled `UNSTAMPED_COLUMN_ENSURES` asymmetry: v4's runner stamps the id
//! when IT adds the columns). FATAL on failure: the migration is not
//! `resumable`, so a failure stops v4's boot; the host `?`s this ensure.
//!
//! ## NO-PORT (R-1)
//!
//! Belonging to the deferred migration runner, with no v5 analog: the
//! migration's INFO `Added geometry columns to terminal_sessions` `{ context:
//! 'migration.add-terminal-session-geometry-v1', added }`, its ERROR `Failed to
//! add geometry columns to terminal_sessions` `{ context, error }`, its result
//! message `Added cols and rows columns to terminal_sessions`, its
//! `migrations_state` row, `database-utils`' DEBUG `Adding column`, and
//! `lib/startup/prettify.ts`'s label `Measuring each of Ariel's terminal
//! windows, the better to read them back`. The host logs v5's ONE boot INFO
//! (`Measured each terminal session's window for transcript replay`) when this
//! ensure added anything, and is silent on a no-op boot.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, table_exists};
use super::DbError;

/// v4's migration id.
pub const GEOMETRY_MIGRATION_ID: &str = "add-terminal-session-geometry-v1";
/// The migration's column clause, verbatim (`addColumnIfMissing(…, 'INTEGER
/// DEFAULT NULL')`).
pub const GEOMETRY_DECL: &str = "INTEGER DEFAULT NULL";

/// `add-terminal-session-geometry-v1`: append `terminal_sessions.cols` then
/// `rows` when absent. Answers how many columns it added — 0, 1 (a
/// half-migrated table) or 2. A no-op when the table is absent (v4's
/// `sqliteTableExists` gate — on a v5 boot provisioning always created it).
pub fn ensure_terminal_sessions_geometry_columns(main: &Connection) -> Result<usize, DbError> {
    if !table_exists(main, "terminal_sessions")? {
        return Ok(0);
    }
    let mut added = 0;
    for column in ["cols", "rows"] {
        if add_column_if_missing(main, "terminal_sessions", column, GEOMETRY_DECL)? {
            added += 1;
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    /// The pre-`a6292f169` `terminal_sessions`, as generateDDL rendered it.
    const BASELINE_DDL: &str = r#"CREATE TABLE "terminal_sessions" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "chatId" TEXT NOT NULL,
  "label" TEXT,
  "shell" TEXT NOT NULL,
  "cwd" TEXT NOT NULL,
  "startedAt" TEXT NOT NULL,
  "exitedAt" TEXT,
  "exitCode" REAL,
  "transcriptPath" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)"#;

    fn sql_of(db: &Connection) -> String {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'terminal_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn table(ddl: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddl).unwrap();
        db.execute(
            "INSERT INTO terminal_sessions (id, chatId, shell, cwd, startedAt, createdAt, \
             updatedAt) VALUES ('s1', 'c1', '/bin/sh', '/', 't', 't', 't')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn appends_both_in_v4s_order_with_the_migration_clause() {
        let db = table(BASELINE_DDL);
        assert_eq!(ensure_terminal_sessions_geometry_columns(&db).unwrap(), 2);
        assert!(
            sql_of(&db)
                .ends_with(", \"cols\" INTEGER DEFAULT NULL, \"rows\" INTEGER DEFAULT NULL)"),
            "{}",
            sql_of(&db)
        );
        let (cols, rows): (Option<i64>, Option<i64>) = db
            .query_row("SELECT cols, rows FROM terminal_sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((cols, rows), (None, None), "no backfill");
        // Idempotent.
        assert_eq!(ensure_terminal_sessions_geometry_columns(&db).unwrap(), 0);
    }

    #[test]
    fn a_half_migrated_table_gets_rows_alone() {
        let db = table(BASELINE_DDL);
        db.execute_batch(
            "ALTER TABLE \"terminal_sessions\" ADD COLUMN \"cols\" INTEGER DEFAULT NULL",
        )
        .unwrap();
        assert_eq!(ensure_terminal_sessions_geometry_columns(&db).unwrap(), 1);
        let names = column_names(&db, "terminal_sessions").unwrap();
        assert_eq!(&names[names.len() - 2..], ["cols", "rows"]);
    }

    #[test]
    fn the_fresh_real_shape_is_left_alone() {
        let fresh = BASELINE_DDL.replace(
            "  \"transcriptPath\" TEXT,\n",
            "  \"transcriptPath\" TEXT,\n  \"cols\" REAL,\n  \"rows\" REAL,\n",
        );
        let db = table(&fresh);
        let before = sql_of(&db);
        assert_eq!(ensure_terminal_sessions_geometry_columns(&db).unwrap(), 0);
        assert_eq!(sql_of(&db), before);
    }

    #[test]
    fn no_table_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(ensure_terminal_sessions_geometry_columns(&db).unwrap(), 0);
        assert!(!table_exists(&db, "terminal_sessions").unwrap());
    }
}
