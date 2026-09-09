//! The `chat_messages.routeTrail` boot ensure (v4 `5841a8c62`, migration
//! `add-route-trail-message-column-v1` — "Message route trail: every model
//! tried, in order, under avatar").
//!
//! v4 adds the column through its migration runner. v5's migration runner is
//! a locked deferral, so — following the P4.d7 / P4.D41 / P4.D63 / P4.D73 /
//! P4.D77 / P4.D79 / P4.D135 precedents — the column add is re-homed as a
//! boot repair pass over the main partition.
//!
//! ## The two v4 shapes DISAGREE here (the P4.D78 / bug-68 class)
//!
//! Measured at the `78b381a96` re-dump:
//!
//!   - **generateDDL** (`lib/database/repositories/chats-messages.ops.ts`,
//!     via `schema-translator.ts`) emits bare `"routeTrail" TEXT` — no
//!     explicit default;
//!   - **the migration** (`add-route-trail-message-column-v1.ts`) emits
//!     `ADD COLUMN "routeTrail" TEXT DEFAULT NULL`.
//!
//! Both are carried, exactly as v4 does: `fresh_schema.json` holds the bare
//! `TEXT` for a fresh instance (`SELECT` returns `NULL` either way — SQLite
//! makes no observable difference between an implicit and an explicit
//! `DEFAULT NULL`), and this ensure emits the migration's DDL verbatim for an
//! existing one.
//!
//! ## No backfill
//!
//! Every existing message's route trail is unknown, and "unknown" already
//! reads as `NULL` — the same value nothing-failed messages carry going
//! forward. v4's migration runs no UPDATE after the ALTER, and neither does
//! this ensure.
//!
//! v4's migration pretty label ("Recording who really answered", by analogy
//! with `lib/startup/prettify.ts`'s existing labels) has no v5 analog —
//! recorded NO-PORT, the P4.D63 / P4.D73 / P4.D79 precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The route-trail column, with v4's migration DDL verbatim.
const ROUTE_TRAIL_COLUMN: &str = "routeTrail";
const ROUTE_TRAIL_DECL: &str = "TEXT DEFAULT NULL";

/// Add `chat_messages.routeTrail` when absent.
///
/// A no-op when the table is absent (a partition that has never held
/// messages) or the column already exists.
pub fn ensure_chat_messages_route_trail_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chat_messages")? {
        return Ok(());
    }
    let columns = column_names(main, "chat_messages")?;
    if columns.iter().any(|c| c == ROUTE_TRAIL_COLUMN) {
        return Ok(());
    }

    main.execute_batch(&format!(
        "ALTER TABLE \"chat_messages\" ADD COLUMN \"{ROUTE_TRAIL_COLUMN}\" {ROUTE_TRAIL_DECL}"
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

    /// A pre-4.10 `chat_messages` table: no `routeTrail` column.
    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"chat_messages\" (\
               \"id\" TEXT PRIMARY KEY, \"chatId\" TEXT, \"pascalMeta\" TEXT, \
               \"pendingExternalPrompt\" TEXT, \"createdAt\" TEXT);",
        )
        .unwrap();
    }

    fn insert(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, createdAt) VALUES (?1, 'c1', 't')",
            [id],
        )
        .unwrap();
    }

    fn columns(conn: &Connection) -> Vec<String> {
        column_names(conn, "chat_messages").unwrap()
    }

    #[test]
    fn a_legacy_table_gains_the_column_defaulting_null() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "m1");

        ensure_chat_messages_route_trail_column(&conn).unwrap();

        let cols = columns(&conn);
        assert!(cols.iter().any(|c| c == ROUTE_TRAIL_COLUMN));
        // v4's migration APPENDS — not the generateDDL mid-table slot.
        assert_eq!(cols.last().unwrap(), ROUTE_TRAIL_COLUMN);

        let trail: Option<String> = conn
            .query_row(
                "SELECT routeTrail FROM chat_messages WHERE id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(trail, None, "no backfill — unknown reads as NULL");
    }

    #[test]
    fn a_second_boot_never_reclobbers_a_planted_trail() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "m1");
        ensure_chat_messages_route_trail_column(&conn).unwrap();

        conn.execute(
            "UPDATE chat_messages SET routeTrail = '[{\"outcome\":\"answered\"}]' \
             WHERE id = 'm1'",
            [],
        )
        .unwrap();

        ensure_chat_messages_route_trail_column(&conn).unwrap();

        let trail: Option<String> = conn
            .query_row(
                "SELECT routeTrail FROM chat_messages WHERE id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(trail.as_deref(), Some("[{\"outcome\":\"answered\"}]"));
    }

    #[test]
    fn the_generatedll_shape_is_recognised_and_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        // A FRESH instance: the column sits mid-table, in the Zod slot.
        conn.execute_batch(
            "CREATE TABLE \"chat_messages\" (\
               \"id\" TEXT PRIMARY KEY, \"pascalMeta\" TEXT, \"routeTrail\" TEXT, \
               \"pendingExternalPrompt\" TEXT);",
        )
        .unwrap();

        ensure_chat_messages_route_trail_column(&conn).unwrap();

        assert_eq!(
            columns(&conn),
            vec!["id", "pascalMeta", "routeTrail", "pendingExternalPrompt"],
            "a fresh instance is untouched — no appended duplicate"
        );
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chat_messages_route_trail_column(&conn).unwrap();
    }
}
