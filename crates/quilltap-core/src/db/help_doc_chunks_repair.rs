//! The `help_doc_chunks` table boot ensure (v4 `24633026`, migration
//! `create-help-doc-chunks-table-v1`).
//!
//! v4 creates this table through its migration runner. v5's migration runner is
//! a locked deferral, so — following the P4.d7 (NOCASE namespace), P4.D41
//! (`linkGroupId`), P4.D63 (character archive columns) and P4.D73 (composer
//! columns) precedents — the create is re-homed as a **boot repair pass over the
//! main partition**: any instance v5 boots gains the table, and a fresh instance
//! already has it from the D23 `generateDDL` re-dump.
//!
//! **The DDL below is v4's migration script's, byte for byte** — so an existing
//! instance ends up with exactly the table v4's own migration would have given
//! it, `UNIQUE(docId, chunkIndex)` and the `ON DELETE CASCADE` foreign key
//! included. That is deliberately NOT the same shape as
//! `provisioning/fresh_schema.json`'s (which is the `generateDDL` surface: no
//! UNIQUE, no FK, `chunkIndex REAL`, `embedding TEXT`, and a `createdAt` index
//! instead of a `docId` one). Both shapes are v4's own; which one an instance
//! has depends only on how it was born, in v4 exactly as in v5.
//!
//! ⚠ The consequence, stated once: **the cascade is not available everywhere**,
//! so the chunk write paths must never depend on it — see
//! [`super::help_doc_chunks::HelpDocChunksRepository::delete_by_doc_id`] and
//! `delete_orphaned`, which v4 ships belt-and-braces for the same reason.
//!
//! v4's migration pretty label ("Slipping bookmarks between the chapters of the
//! help library", `lib/startup/prettify.ts`) has no v5 analog — v5 surfaces no
//! migration labels anywhere. Recorded NO-PORT, the `231be14c` precedent.
//!
//! Idempotent; two `CREATE … IF NOT EXISTS` statements on every boot.

use rusqlite::Connection;

use super::DbError;

/// v4 `migrations/scripts/create-help-doc-chunks-table.ts:51-64`, verbatim.
pub const HELP_DOC_CHUNKS_TABLE_DDL: &str = r#"CREATE TABLE IF NOT EXISTS "help_doc_chunks" (
            "id" TEXT PRIMARY KEY,
            "docId" TEXT NOT NULL,
            "chunkIndex" INTEGER NOT NULL,
            "heading" TEXT,
            "content" TEXT NOT NULL,
            "embedding" BLOB,
            "createdAt" TEXT NOT NULL,
            "updatedAt" TEXT NOT NULL,
            UNIQUE("docId", "chunkIndex"),
            FOREIGN KEY ("docId") REFERENCES "help_docs"("id") ON DELETE CASCADE
          )"#;

/// v4 `migrations/scripts/create-help-doc-chunks-table.ts:66`, verbatim.
pub const HELP_DOC_CHUNKS_INDEX_DDL: &str =
    r#"CREATE INDEX IF NOT EXISTS "idx_help_doc_chunks_docId" ON "help_doc_chunks" ("docId")"#;

/// Create `help_doc_chunks` (+ its `docId` index) if the main partition lacks
/// it.
///
/// v4's migration wraps the pair in one `db.transaction`; the caller here is
/// already inside the writer's transaction scope, and both statements are
/// independently idempotent, so no nested transaction is opened.
///
/// Unlike the column repairs, there is no "table absent → no-op" escape: the
/// point of this pass is precisely that the table may be absent. It is a no-op
/// on the second and every later boot.
pub fn ensure_help_doc_chunks_table(main: &Connection) -> Result<(), DbError> {
    main.execute_batch(HELP_DOC_CHUNKS_TABLE_DDL)?;
    main.execute_batch(HELP_DOC_CHUNKS_INDEX_DDL)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_sql(conn: &Connection, name: &str) -> Option<String> {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    #[test]
    fn creates_the_migration_shape_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(table_sql(&conn, "help_doc_chunks").is_none());

        ensure_help_doc_chunks_table(&conn).unwrap();
        let sql = table_sql(&conn, "help_doc_chunks").expect("table created");
        // The constraints the generateDDL shape does NOT have — this is how we
        // know the boot ensure used the migration DDL and not the fresh one.
        assert!(sql.contains(r#"UNIQUE("docId", "chunkIndex")"#), "{sql}");
        assert!(sql.contains("ON DELETE CASCADE"), "{sql}");
        assert!(sql.contains(r#""chunkIndex" INTEGER NOT NULL"#), "{sql}");
        assert!(sql.contains(r#""embedding" BLOB"#), "{sql}");
        assert!(table_sql(&conn, "idx_help_doc_chunks_docId").is_some());

        // Second boot: no error, no change.
        ensure_help_doc_chunks_table(&conn).unwrap();
        assert_eq!(
            table_sql(&conn, "help_doc_chunks").as_deref(),
            Some(&sql[..])
        );
    }

    #[test]
    fn leaves_an_existing_fresh_shaped_table_alone() {
        // A v5-provisioned instance already carries the generateDDL shape; the
        // ensure must not attempt to replace or widen it.
        let conn = Connection::open_in_memory().unwrap();
        let fresh = "CREATE TABLE \"help_doc_chunks\" (\n  \"id\" TEXT PRIMARY KEY NOT NULL,\n  \"docId\" TEXT NOT NULL,\n  \"chunkIndex\" REAL NOT NULL,\n  \"heading\" TEXT,\n  \"content\" TEXT NOT NULL,\n  \"embedding\" TEXT,\n  \"createdAt\" TEXT NOT NULL,\n  \"updatedAt\" TEXT NOT NULL\n)";
        conn.execute_batch(fresh).unwrap();
        ensure_help_doc_chunks_table(&conn).unwrap();
        assert_eq!(table_sql(&conn, "help_doc_chunks").as_deref(), Some(fresh));
    }
}
