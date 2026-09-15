//! The `files.generationKey` boot ensure — column **and** index (v4
//! `7fbf8a55b`, migration `add-file-generation-key-column-v1` — "cache
//! character avatars per configuration").
//!
//! v4 adds both through its migration runner. v5's migration runner is a
//! locked deferral, so — following the P4.d7 / P4.D41 / P4.D63 / P4.D73 /
//! P4.D77 / P4.D79 / P4.D135 / P4.D171 precedents — the add is re-homed as a
//! boot repair pass over the main partition.
//!
//! ## Why this pass creates TWO things, not one
//!
//! Unlike every column ensure beside it, v4's migration here issues a second
//! statement: `CREATE INDEX IF NOT EXISTS "idx_files_generationKey" ON
//! "files" ("generationKey")`. `generateDDL` walks the Zod schema, and a Zod
//! field cannot express a plain index — measured at the `31436bae4` re-dump,
//! the dump carries the COLUMN (v4's `FileEntrySchema` declares
//! `generationKey: z.string().nullable().optional()`) and carries no index for
//! it. So a fresh v5 instance provisions the column from
//! `provisioning/fresh_schema.json` and would have no index at all; an
//! existing instance has neither. This ensure closes both gaps, and the two
//! arms are independent:
//!
//! - column absent → ALTER, then the index;
//! - column present, index absent (a fresh-provisioned instance, or a v4
//!   instance whose migration half-ran) → the index alone;
//! - both present → two cheap reads plus the idempotent `CREATE INDEX IF NOT
//!   EXISTS`, which writes nothing.
//!
//! ## What is load-bearing
//!
//! On an existing instance the COLUMN is: this port's `files` INSERT binds
//! `generationKey` on every row it writes (P4.D182), so without the column the
//! very first file write would 500. The INDEX is what makes the avatar cache's
//! pre-generation lookup cheap enough to run on every avatar job (P4.D184's
//! `find_by_generation_key`) — correctness does not depend on it, the cost of
//! every avatar generation does.
//!
//! On the shared Friday instance this is an exact no-op in both arms: v4's own
//! migration runner has already added the column and created the index there.
//!
//! ## One recorded divergence: a FRESH instance
//!
//! v4's migration gates its WHOLE run on the column being absent
//! (`shouldRun: !sqliteColumnExists('files', 'generationKey')`), and a fresh
//! v4 instance provisions the column from `generateDDL` — so on a fresh v4
//! instance the migration never runs and `idx_files_generationKey` is never
//! created: v4's avatar-cache lookup is a table scan there. v5 creates the
//! index on every entrance. Performance-only, in v5's favour, invisible to
//! `provisioning_equivalence` (which compares the provisioner's output, not
//! the post-boot instance), and a candidate upstream nicety. Recorded at the
//! `31436bae4` round's unification rather than "closed" — the two arms below
//! are v5's design, not a v4 gap this module fills.
//!
//! v4's migration pretty label (`lib/startup/prettify.ts`) has no v5 analog —
//! recorded NO-PORT, the P4.D63 / P4.D73 / P4.D79 / P4.D171 precedent.
//!
//! Idempotent; one PRAGMA, one `sqlite_master` probe and one no-op
//! `CREATE INDEX IF NOT EXISTS` on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The avatar configuration cache key column. v4's migration DDL verbatim —
/// and `generateDDL`'s, which emits the same `"generationKey" TEXT` (the two
/// agree here, like `cycleOrderParticipantIds` and unlike `routeTrail`).
const GENERATION_KEY_COLUMN: &str = "generationKey";
const GENERATION_KEY_DECL: &str = "TEXT";

/// v4 `migrations/scripts/add-file-generation-key-column-v1.ts`, verbatim.
const GENERATION_KEY_INDEX_DDL: &str =
    r#"CREATE INDEX IF NOT EXISTS "idx_files_generationKey" ON "files" ("generationKey")"#;

/// Add `files.generationKey` when absent, and create its index when absent.
///
/// A no-op when the table is absent (a partition that has never held files).
/// The two arms are independent: a table that already has the column but not
/// the index gains the index alone.
pub fn ensure_files_generation_key_column_and_index(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "files")? {
        return Ok(());
    }

    let columns = column_names(main, "files")?;
    if !columns.iter().any(|c| c == GENERATION_KEY_COLUMN) {
        main.execute_batch(&format!(
            "ALTER TABLE \"files\" ADD COLUMN \"{GENERATION_KEY_COLUMN}\" {GENERATION_KEY_DECL}"
        ))?;
    }

    // `IF NOT EXISTS` makes this its own guard; it runs on every boot and
    // writes nothing once the index is there (v4 issues it unconditionally
    // inside its migration too).
    main.execute_batch(GENERATION_KEY_INDEX_DDL)?;

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

    /// A pre-4.10 `files` table: no `generationKey`, no index.
    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"files\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \
               \"generationPrompt\" TEXT, \"generationModel\" TEXT, \
               \"generationRevisedPrompt\" TEXT, \"description\" TEXT, \
               \"createdAt\" TEXT);",
        )
        .unwrap();
    }

    fn insert(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO files (id, userId, createdAt) VALUES (?1, 'u1', 't')",
            [id],
        )
        .unwrap();
    }

    fn columns(conn: &Connection) -> Vec<String> {
        column_names(conn, "files").unwrap()
    }

    fn index_exists(conn: &Connection) -> bool {
        let mut stmt = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1")
            .unwrap();
        stmt.exists(["idx_files_generationKey"]).unwrap()
    }

    #[test]
    fn a_legacy_table_gains_the_column_and_the_index() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "f1");

        ensure_files_generation_key_column_and_index(&conn).unwrap();

        let cols = columns(&conn);
        assert!(cols.iter().any(|c| c == GENERATION_KEY_COLUMN));
        // v4's migration APPENDS — not the generateDDL mid-table slot.
        assert_eq!(cols.last().unwrap(), GENERATION_KEY_COLUMN);
        assert!(index_exists(&conn));

        // Nullable, no default: an existing row reads NULL.
        let key: Option<String> = conn
            .query_row("SELECT generationKey FROM files WHERE id = 'f1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(key, None, "every pre-existing file carries no cache key");
    }

    #[test]
    fn a_half_migrated_table_gains_the_index_alone() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        conn.execute_batch("ALTER TABLE \"files\" ADD COLUMN \"generationKey\" TEXT")
            .unwrap();
        insert(&conn, "f1");
        conn.execute("UPDATE files SET generationKey = 'k-1' WHERE id = 'f1'", [])
            .unwrap();
        assert!(!index_exists(&conn));

        ensure_files_generation_key_column_and_index(&conn).unwrap();

        assert!(index_exists(&conn), "the index arm runs on its own");
        let key: Option<String> = conn
            .query_row("SELECT generationKey FROM files WHERE id = 'f1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            key.as_deref(),
            Some("k-1"),
            "the column arm did not re-add and wipe"
        );
        assert_eq!(
            columns(&conn)
                .iter()
                .filter(|c| *c == "generationKey")
                .count(),
            1,
            "no appended duplicate"
        );
    }

    #[test]
    fn a_second_boot_never_reclobbers_a_planted_key() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "f1");
        ensure_files_generation_key_column_and_index(&conn).unwrap();

        conn.execute("UPDATE files SET generationKey = 'k-2' WHERE id = 'f1'", [])
            .unwrap();

        ensure_files_generation_key_column_and_index(&conn).unwrap();

        let key: Option<String> = conn
            .query_row("SELECT generationKey FROM files WHERE id = 'f1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(key.as_deref(), Some("k-2"));
        assert!(index_exists(&conn));
    }

    #[test]
    fn the_generateddl_shape_is_recognised_and_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        // A FRESH instance: the column sits mid-table, in the Zod slot — and
        // there is no index, because `generateDDL` cannot emit one.
        conn.execute_batch(
            "CREATE TABLE \"files\" (\
               \"id\" TEXT PRIMARY KEY, \
               \"generationRevisedPrompt\" TEXT, \
               \"generationKey\" TEXT, \
               \"description\" TEXT);",
        )
        .unwrap();

        ensure_files_generation_key_column_and_index(&conn).unwrap();

        assert_eq!(
            columns(&conn),
            vec![
                "id",
                "generationRevisedPrompt",
                "generationKey",
                "description"
            ],
            "a fresh instance's columns are untouched — no appended duplicate"
        );
        assert!(
            index_exists(&conn),
            "…and it gains the index the D23 dump cannot carry"
        );
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_files_generation_key_column_and_index(&conn).unwrap();
        assert!(!index_exists(&conn));
    }
}
