//! The Concierge refusal ledger's two columns — a boot ensure (v4 `49059fb14`,
//! #74, migration `add-chat-refusal-ledger-v1`).
//!
//! v4 adds `chats.moderationRefusalCount INTEGER NOT NULL DEFAULT 0` and
//! `chats.lastModerationRefusalAt TEXT DEFAULT NULL` through its migration
//! runner (`dependsOn: ['add-chat-concierge-override-v1']`, appended LAST).
//! v5's migration runner is a locked deferral, so — the P4.D182
//! `chats_transcript_version_repair` precedent exactly — the add is re-homed as
//! a boot repair pass over the main partition.
//!
//! ## These can ONLY arrive here — the D23 re-dump cannot carry them
//!
//! v4 keeps both columns out of `ChatMetadataSchema` AND
//! `ChatMetadataBaseSchema` deliberately (the phase-2 SPEC says the opposite —
//! it declares them in both schemas and exports them; the as-built code is the
//! authority, §R.4(a)). `generateDDL` (`lib/database/schema-translator.ts:433`)
//! WALKS the Zod schema, so it never emits them. **Measured at planning, live
//! from v4's own `generateDDL` at the pin: the `chats` CREATE (104 lines, for
//! both schemas) carries neither `moderationRefusalCount` nor
//! `lastModerationRefusalAt`** (nor `transcriptVersion`) — so
//! `provisioning/fresh_schema.json` does not move, and this pass is the
//! columns' ONLY source on every v5 instance, fresh or not. On a Friday copy
//! that v4's runner has already migrated, it is an exact no-op.
//!
//! ## What is load-bearing, and what must stay away
//!
//! Load-bearing: the ledger's `UPDATE … "moderationRefusalCount" =
//! "moderationRefusalCount" + ?` and its reads would fail on every chat
//! without them.
//!
//! Must stay away — v4's own rules for fields outside the schema: the columns
//! are NOT in `chats_read::ALL_COLUMNS` (the index census pins that list to the
//! D23 dump, which does not carry them), NOT in `ChatUpdate` (a whole-row
//! rewrite must not be able to rewind the tally), and NOT exported in a `.qtap`
//! bundle (v4's export schema is untouched by `49059fb14`). The ledger's three
//! repository operations in `db::chats` are their ONLY readers and writers;
//! `moderation_refusal_ledger_isolation_guard` pins the three negatives.
//!
//! ## No backfill
//!
//! v4's migration runs no UPDATE after the ALTERs; `DEFAULT 0` / `DEFAULT
//! NULL` give every existing chat an empty ledger. v4's pretty label
//! (`prettify.ts`: "Opening the Concierge's ledger of refusals") is NO-PORT —
//! the standing class.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The ledger's two columns, with v4's migration DDL verbatim, in v4's order.
const LEDGER_COLUMNS: [(&str, &str); 2] = [
    ("moderationRefusalCount", "INTEGER NOT NULL DEFAULT 0"),
    ("lastModerationRefusalAt", "TEXT DEFAULT NULL"),
];

/// Add whichever of the two ledger columns `chats` lacks (v4's
/// `addColumnIfMissing` twice). A no-op when the table is absent or both
/// columns already exist.
pub fn ensure_chats_moderation_refusal_ledger_columns(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chats")? {
        return Ok(());
    }
    let columns = column_names(main, "chats")?;
    for (name, decl) in LEDGER_COLUMNS {
        if columns.iter().any(|c| c == name) {
            continue;
        }
        main.execute_batch(&format!(
            "ALTER TABLE \"chats\" ADD COLUMN \"{name}\" {decl}"
        ))?;
    }
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

    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \
               \"conciergeOverride\" TEXT, \"createdAt\" TEXT);\
             INSERT INTO chats (id, userId, createdAt) VALUES ('c1', 'u1', 't');",
        )
        .unwrap();
    }

    fn ledger(conn: &Connection) -> (i64, Option<String>) {
        conn.query_row(
            "SELECT moderationRefusalCount, lastModerationRefusalAt FROM chats WHERE id = 'c1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn a_legacy_table_gains_both_columns_appended_with_an_empty_ledger() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        ensure_chats_moderation_refusal_ledger_columns(&conn).unwrap();
        let cols = column_names(&conn, "chats").unwrap();
        assert_eq!(
            &cols[cols.len() - 2..],
            ["moderationRefusalCount", "lastModerationRefusalAt"],
            "v4's migration APPENDS, count first"
        );
        assert_eq!(ledger(&conn), (0, None), "no backfill: DEFAULT 0 / NULL");
        // NOT NULL DEFAULT 0 is the declared type v4's migration writes.
        let notnull: i64 = conn
            .query_row(
                "SELECT \"notnull\" FROM pragma_table_info('chats') WHERE name = 'moderationRefusalCount'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(notnull, 1);
    }

    #[test]
    fn a_second_boot_never_resets_a_recorded_tally() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        ensure_chats_moderation_refusal_ledger_columns(&conn).unwrap();
        conn.execute_batch(
            "UPDATE chats SET lastModerationRefusalAt = 'x', \
               moderationRefusalCount = moderationRefusalCount + 1 WHERE id = 'c1'",
        )
        .unwrap();
        ensure_chats_moderation_refusal_ledger_columns(&conn).unwrap();
        assert_eq!(ledger(&conn), (1, Some("x".to_string())));
    }

    #[test]
    fn a_half_migrated_table_gains_only_the_missing_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \
               \"moderationRefusalCount\" INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();
        ensure_chats_moderation_refusal_ledger_columns(&conn).unwrap();
        assert_eq!(
            column_names(&conn, "chats").unwrap(),
            ["id", "moderationRefusalCount", "lastModerationRefusalAt"]
        );
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chats_moderation_refusal_ledger_columns(&conn).unwrap();
    }
}
