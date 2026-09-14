//! The `chats.transcriptVersion` boot ensure (v4 `5029075bb`, migration
//! `add-transcript-version-column-v1` — "Salon transcript as subscribed
//! read").
//!
//! v4 adds the column through its migration runner. v5's migration runner is
//! a locked deferral, so — following the P4.d7 / P4.D41 / P4.D63 / P4.D73 /
//! P4.D77 / P4.D79 / P4.D135 / P4.D171 precedents — the column add is
//! re-homed as a boot repair pass over the main partition.
//!
//! ## This one can ONLY arrive here — the D23 re-dump cannot carry it
//!
//! Every sibling of this ensure exists because v5 has no migration runner,
//! while a FRESH v5 instance still gets the column from
//! `provisioning/fresh_schema.json`. Not this one. v4 declares
//! `transcriptVersion` in neither `ChatMetadataSchema` nor
//! `ChatMetadataBaseSchema` — two identical comment blocks in
//! `lib/schemas/chat.types.ts` say why, in v4's own words: every repository
//! update rewrites the whole validated row from a snapshot it read moments
//! earlier, so a counter carried as a schema field could be **rewound** by any
//! concurrent chat-row write, which is precisely the "answered unchanged while
//! a message is missing" failure the counter exists to prevent. Keeping it out
//! of the schema means Zod strips it from every write, so the column is
//! touched only by the atomic `SET v = v + 1`.
//!
//! `generateDDL` walks the schema. So it never emits this column, the
//! `31436bae4` re-dump provably does not carry it (measured: zero occurrences
//! in the whole dump), and **a fresh v5 instance would not have it either**.
//! This pass is therefore the column's ONLY source on every instance, fresh or
//! not — the shape P4.D145's bug-114 unique index took for the mirror-image
//! reason (`generateDDL` could not express THAT one either).
//!
//! ## What is load-bearing, and what must stay away
//!
//! Load-bearing: without the column, P4.D183's `announceTranscriptChange`
//! bump — an `UPDATE chats SET transcriptVersion = transcriptVersion + 1` on
//! the one message-write funnel — would 500 on every message this port
//! writes. On the shared Friday instance this ensure is an exact no-op: v4's
//! own migration runner has already added the column there.
//!
//! Must stay away: **nothing** in v5 may read, project, remap or export this
//! column outside P4.D183's single writer and single reader. Its absence from
//! every column list IS the export mechanism — v4 has no export-side filter
//! for it and v5 must invent none. The negative pins live beside the surfaces
//! they guard: `db::chats_read`'s index census (the column is not in the D23
//! dump, so `ALL_COLUMNS` must not name it, and `marshal_row` must not
//! marshal it even when a real table carries it), and `db::chats`'s
//! `ChatUpdate` guard.
//!
//! ## No backfill
//!
//! v4's migration runs no UPDATE after the ALTER, and neither does this
//! ensure: the `DEFAULT 0` already gives every existing row `0`, which v4's
//! own migration header calls correct — "the first read from any tab carries
//! no known version and gets the whole transcript, and the first write after
//! this migration moves the counter off zero."
//!
//! v4's migration pretty label (`lib/startup/prettify.ts`) has no v5 analog —
//! recorded NO-PORT, the P4.D63 / P4.D73 / P4.D79 / P4.D171 precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The transcript counter column, with v4's migration DDL verbatim. There is
/// no `generateDDL` shape to reconcile against — see the header.
const TRANSCRIPT_VERSION_COLUMN: &str = "transcriptVersion";
const TRANSCRIPT_VERSION_DECL: &str = "INTEGER DEFAULT 0";

/// Add `chats.transcriptVersion` when absent.
///
/// A no-op when the table is absent (a partition that has never held chats)
/// or the column already exists.
pub fn ensure_chats_transcript_version_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chats")? {
        return Ok(());
    }
    let columns = column_names(main, "chats")?;
    if columns.iter().any(|c| c == TRANSCRIPT_VERSION_COLUMN) {
        return Ok(());
    }

    main.execute_batch(&format!(
        "ALTER TABLE \"chats\" ADD COLUMN \"{TRANSCRIPT_VERSION_COLUMN}\" {TRANSCRIPT_VERSION_DECL}"
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

    /// A pre-4.10 `chats` table: no `transcriptVersion` column.
    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \
               \"messageCount\" REAL DEFAULT 0, \
               \"lastMessageAt\" TEXT, \"createdAt\" TEXT);",
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

    fn version(conn: &Connection, id: &str) -> Option<i64> {
        conn.query_row(
            "SELECT transcriptVersion FROM chats WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn a_legacy_table_gains_the_column_and_existing_chats_read_zero() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "c1");

        ensure_chats_transcript_version_column(&conn).unwrap();

        let cols = columns(&conn);
        assert!(cols.iter().any(|c| c == TRANSCRIPT_VERSION_COLUMN));
        // v4's migration APPENDS; there is no generateDDL slot to land in.
        assert_eq!(cols.last().unwrap(), TRANSCRIPT_VERSION_COLUMN);

        assert_eq!(
            version(&conn, "c1"),
            Some(0),
            "v4: existing chats start at 0, and no backfill is possible or wanted"
        );
    }

    #[test]
    fn a_second_boot_never_rewinds_a_bumped_counter() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "c1");
        ensure_chats_transcript_version_column(&conn).unwrap();

        conn.execute(
            "UPDATE chats SET transcriptVersion = transcriptVersion + 1 WHERE id = 'c1'",
            [],
        )
        .unwrap();
        assert_eq!(version(&conn, "c1"), Some(1));

        ensure_chats_transcript_version_column(&conn).unwrap();

        assert_eq!(
            version(&conn, "c1"),
            Some(1),
            "rewinding the counter is the one failure this column exists to prevent"
        );
    }

    #[test]
    fn a_v4_migrated_table_is_recognised_and_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        // What v4's own migration runner leaves behind: the column appended.
        conn.execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \
               \"transcriptVersion\" INTEGER DEFAULT 0);",
        )
        .unwrap();
        insert_minimal(&conn, "c1");
        conn.execute("UPDATE chats SET transcriptVersion = 9 WHERE id = 'c1'", [])
            .unwrap();

        ensure_chats_transcript_version_column(&conn).unwrap();

        assert_eq!(
            columns(&conn),
            vec!["id", "userId", "transcriptVersion"],
            "no appended duplicate"
        );
        assert_eq!(version(&conn, "c1"), Some(9));
    }

    fn insert_minimal(conn: &Connection, id: &str) {
        conn.execute("INSERT INTO chats (id, userId) VALUES (?1, 'u1')", [id])
            .unwrap();
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chats_transcript_version_column(&conn).unwrap();
    }
}
