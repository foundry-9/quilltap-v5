//! The `chat_informs.permanent` boot ensure (v4 `52d6e7ecd`, migration
//! `add-chat-informs-permanent-v1` — "Inform: standing (per-chat) informs").
//!
//! v4 adds the column through its migration runner (`dependsOn:
//! ['add-chat-informs-table-v1']`). v5's migration runner is a locked
//! deferral, so — following the P4.d7 / P4.D41 / P4.D63 / P4.D73 / P4.D77 /
//! P4.D79 / P4.D135 / P4.D171 precedents — the column add is re-homed as a
//! boot repair pass over the main partition, run AFTER
//! [`super::chat_informs::ensure_chat_informs_table`] (v4's `dependsOn`).
//!
//! ## The two v4 shapes DISAGREE here (the P4.D78 / P4.D171 class)
//!
//! Measured at the D23 re-dump #4 from the `52d6e7ecd` pin:
//!
//!   - **generateDDL** (`ChatInformSchema.permanent: z.boolean().default(false)`
//!     through `schema-translator.ts`) emits `"permanent" INTEGER DEFAULT 0`
//!     in SCHEMA order — after `recordMessageId`, before `createdAt` — with
//!     **no `NOT NULL`**;
//!   - **the migration** (`add-chat-informs-permanent.ts`) emits
//!     `ADD COLUMN "permanent" INTEGER NOT NULL DEFAULT 0`, appended LAST.
//!
//! Both are carried, exactly as v4 does: `fresh_schema.json` and
//! `CHAT_INFORMS_TABLE_DDL` hold the generateDDL shape for a fresh (or
//! boot-created) table, and this ensure emits the migration's DDL verbatim for
//! an existing one. Every writer in v5 binds a boolean, so the NULL the fresh
//! shape permits is never written by v5; the reader still treats a NULL cell
//! as `false`, which is what v4's Zod `.default(false)` makes of the
//! `undefined` its SQLite deserializer turns NULL into. Column ORDER is
//! harmless: v5 names its columns explicitly on every read and write.
//!
//! ## No backfill
//!
//! The default makes every existing row a one-shot inform, which is what they
//! all were. v4's migration runs no UPDATE after the ALTER, and neither does
//! this ensure.
//!
//! ## NO-PORT
//!
//! v4's migration INFO (`Added the permanent column to chat_informs`) and ERROR
//! (`Failed to add the permanent column to chat_informs`) lines, its
//! `migrations_state` ledger row, and its pretty label ("Pinning a place on the
//! tray for the notes meant to stay put", `lib/startup/prettify.ts`) belong to
//! the deferred migration runner and have no v5 analog — the P4.D63 / P4.D73 /
//! P4.D79 precedent `chats_cycle_order_repair.rs` records.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The standing-inform flag, with v4's migration DDL verbatim.
const PERMANENT_COLUMN: &str = "permanent";
const PERMANENT_DECL: &str = "INTEGER NOT NULL DEFAULT 0";

/// Add `chat_informs.permanent` when absent.
///
/// A no-op when the table is absent (v4's `shouldRun` table gate — on a v5
/// boot the table ensure ahead of this one has always created it) or the
/// column already exists (`shouldRun`'s column gate).
pub fn ensure_chat_informs_permanent_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chat_informs")? {
        return Ok(());
    }
    let columns = column_names(main, "chat_informs")?;
    if columns.iter().any(|c| c == PERMANENT_COLUMN) {
        return Ok(());
    }

    main.execute_batch(&format!(
        "ALTER TABLE \"chat_informs\" ADD COLUMN \"{PERMANENT_COLUMN}\" {PERMANENT_DECL}"
    ))?;

    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pre-`52d6e7ecd` `chat_informs` — the generateDDL shape the D23
    /// re-dump #3 carried, byte for byte (no `permanent`).
    const BASELINE_DDL: &str = r#"CREATE TABLE "chat_informs" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "chatId" TEXT NOT NULL,
  "batchId" TEXT NOT NULL,
  "participantId" TEXT NOT NULL,
  "contentMarkdown" TEXT NOT NULL,
  "recordMessageId" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  "consumedAt" TEXT,
  "consumedByMessageId" TEXT
)"#;

    fn baseline_table(conn: &Connection) {
        conn.execute_batch(BASELINE_DDL).unwrap();
        conn.execute(
            "INSERT INTO chat_informs (id, chatId, batchId, participantId, contentMarkdown, \
               createdAt, updatedAt) VALUES ('i1', 'c1', 'b1', 'p1', 'hi', 't', 't')",
            [],
        )
        .unwrap();
    }

    /// `PRAGMA table_info` row for one column: (type, notnull, dflt_value).
    fn column_info(conn: &Connection, col: &str) -> Option<(String, i64, Option<String>)> {
        let mut stmt = conn.prepare("PRAGMA table_info(\"chat_informs\")").unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap();
        for r in rows {
            let (name, ty, notnull, dflt) = r.unwrap();
            if name == col {
                return Some((ty, notnull, dflt));
            }
        }
        None
    }

    #[test]
    fn an_absent_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chat_informs_permanent_column(&conn).unwrap();
        assert!(!table_exists(&conn, "chat_informs").unwrap());
    }

    #[test]
    fn a_baseline_table_gains_the_migration_shape_appended_last() {
        let conn = Connection::open_in_memory().unwrap();
        baseline_table(&conn);

        ensure_chat_informs_permanent_column(&conn).unwrap();

        let cols = column_names(&conn, "chat_informs").unwrap();
        // v4's migration APPENDS — not the generateDDL mid-table slot.
        assert_eq!(cols.last().unwrap(), PERMANENT_COLUMN);
        assert_eq!(
            column_info(&conn, PERMANENT_COLUMN),
            Some(("INTEGER".to_string(), 1, Some("0".to_string()))),
            "v4's migration DDL: INTEGER NOT NULL DEFAULT 0"
        );
    }

    #[test]
    fn existing_rows_read_as_one_shots() {
        let conn = Connection::open_in_memory().unwrap();
        baseline_table(&conn);
        ensure_chat_informs_permanent_column(&conn).unwrap();
        let permanent: i64 = conn
            .query_row(
                "SELECT permanent FROM chat_informs WHERE id = 'i1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(permanent, 0, "no backfill — the default is the one-shot");
    }

    #[test]
    fn a_second_run_is_a_no_op_and_keeps_a_standing_row() {
        let conn = Connection::open_in_memory().unwrap();
        baseline_table(&conn);
        ensure_chat_informs_permanent_column(&conn).unwrap();
        conn.execute("UPDATE chat_informs SET permanent = 1 WHERE id = 'i1'", [])
            .unwrap();
        let before: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        ensure_chat_informs_permanent_column(&conn).unwrap();

        let after: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
        let permanent: i64 = conn
            .query_row(
                "SELECT permanent FROM chat_informs WHERE id = 'i1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(permanent, 1);
    }

    #[test]
    fn a_fresh_generate_ddl_table_is_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::chat_informs::ensure_chat_informs_table(&conn).unwrap();
        let before = column_info(&conn, PERMANENT_COLUMN);
        assert_eq!(
            before,
            Some(("INTEGER".to_string(), 0, Some("0".to_string()))),
            "generateDDL's shape: INTEGER DEFAULT 0, nullable"
        );
        ensure_chat_informs_permanent_column(&conn).unwrap();
        assert_eq!(column_info(&conn, PERMANENT_COLUMN), before);
    }
}
