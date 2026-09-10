//! The `chats.cycleOrderParticipantIds` boot ensure (v4 `2aca73ad6`,
//! migration `add-cycle-order-column-v1` — "Draw a multi-character chat's
//! speaking order once per cycle").
//!
//! v4 adds the column through its migration runner. v5's migration runner is
//! a locked deferral, so — following the P4.d7 / P4.D41 / P4.D63 / P4.D73 /
//! P4.D77 / P4.D79 / P4.D135 precedents — the column add is re-homed as a
//! boot repair pass over the main partition.
//!
//! ## The two v4 shapes AGREE here (unlike the route trail beside it)
//!
//! Measured at the `78b381a96` re-dump: both the migration
//! (`addColumnIfMissing('chats', 'cycleOrderParticipantIds', "TEXT DEFAULT
//! '[]'")`) and `generateDDL` (`lib/schemas/chat.types.ts`'s
//! `cycleOrderParticipantIds: z.string().default('[]')`) declare the same
//! type and the same default. So — unlike `routeTrail` beside it, and unlike
//! most of this ensure's siblings — there is only ONE shape here, and this
//! module emits it. Do not carry two shapes on reflex; this pair is the
//! `2aca73ad6`-round exception (P4.D171's Tier-2 doc note names it for the
//! next lane).
//!
//! ## No backfill
//!
//! `'[]'` reads as "no rotation on file" — exactly what an existing chat has,
//! since the feature only draws a rotation going forward. v4's migration runs
//! no UPDATE after the ALTER (the column default already gives every existing
//! row `'[]'`), and neither does this ensure.
//!
//! v4's migration pretty label (`lib/startup/prettify.ts:186`) has no v5
//! analog — recorded NO-PORT, the P4.D63 / P4.D73 / P4.D79 precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The drawn-rotation column, with v4's migration DDL verbatim (and
/// `generateDDL`'s — the two agree here).
const CYCLE_ORDER_COLUMN: &str = "cycleOrderParticipantIds";
const CYCLE_ORDER_DECL: &str = "TEXT DEFAULT '[]'";

/// Add `chats.cycleOrderParticipantIds` when absent.
///
/// A no-op when the table is absent (a partition that has never held chats)
/// or the column already exists.
pub fn ensure_chats_cycle_order_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chats")? {
        return Ok(());
    }
    let columns = column_names(main, "chats")?;
    if columns.iter().any(|c| c == CYCLE_ORDER_COLUMN) {
        return Ok(());
    }

    main.execute_batch(&format!(
        "ALTER TABLE \"chats\" ADD COLUMN \"{CYCLE_ORDER_COLUMN}\" {CYCLE_ORDER_DECL}"
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

    /// A pre-4.10 `chats` table: no `cycleOrderParticipantIds` column.
    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \
               \"spokenThisCycleParticipantIds\" TEXT DEFAULT '[]', \
               \"documentEditingMode\" INTEGER DEFAULT 0, \"createdAt\" TEXT);",
        )
        .unwrap();
    }

    fn insert(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO chats (id, userId, createdAt) VALUES (?1, 'u1', 't')",
            [id],
        )
        .unwrap();
    }

    fn columns(conn: &Connection) -> Vec<String> {
        column_names(conn, "chats").unwrap()
    }

    #[test]
    fn a_legacy_table_gains_the_column_defaulting_empty_array() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "c1");

        ensure_chats_cycle_order_column(&conn).unwrap();

        let cols = columns(&conn);
        assert!(cols.iter().any(|c| c == CYCLE_ORDER_COLUMN));
        // v4's migration APPENDS — not the generateDDL mid-table slot.
        assert_eq!(cols.last().unwrap(), CYCLE_ORDER_COLUMN);

        let order: String = conn
            .query_row(
                "SELECT cycleOrderParticipantIds FROM chats WHERE id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(order, "[]", "no rotation on file for an existing chat");
    }

    #[test]
    fn a_second_boot_never_reclobbers_a_planted_rotation() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "c1");
        ensure_chats_cycle_order_column(&conn).unwrap();

        conn.execute(
            "UPDATE chats SET cycleOrderParticipantIds = '[\"p1\",\"p2\"]' WHERE id = 'c1'",
            [],
        )
        .unwrap();

        ensure_chats_cycle_order_column(&conn).unwrap();

        let order: String = conn
            .query_row(
                "SELECT cycleOrderParticipantIds FROM chats WHERE id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(order, "[\"p1\",\"p2\"]");
    }

    #[test]
    fn the_generateddl_shape_is_recognised_and_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        // A FRESH instance: the column sits mid-table, in the Zod slot.
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \
               \"spokenThisCycleParticipantIds\" TEXT DEFAULT '[]', \
               \"cycleOrderParticipantIds\" TEXT DEFAULT '[]', \
               \"documentEditingMode\" INTEGER DEFAULT 0);",
        )
        .unwrap();

        ensure_chats_cycle_order_column(&conn).unwrap();

        assert_eq!(
            columns(&conn),
            vec![
                "id",
                "spokenThisCycleParticipantIds",
                "cycleOrderParticipantIds",
                "documentEditingMode"
            ],
            "a fresh instance is untouched — no appended duplicate"
        );
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chats_cycle_order_column(&conn).unwrap();
    }
}
