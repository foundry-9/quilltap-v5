//! The `characters` archive-column boot ensure (v4 `d553f72a`, migration
//! `add-character-archive-fields-v1`).
//!
//! v4 adds `archivedAt` / `archiveFileId` / `archivedAvatarFileId` to
//! `characters` through its migration runner. v5's migration runner is a locked
//! deferral, so — following the P4.d7 (NOCASE namespace) and P4.D41
//! (`linkGroupId`) precedents — the column adds are re-homed as a **boot repair
//! pass over the main partition**: any instance v5 boots gains the columns, and
//! a fresh instance already has them from the D23 `generateDDL` re-dump.
//!
//! This is the WRITE-side half of the tolerance story. The READ side
//! ([`super::characters_read`]) selects literal `NULL`s when the columns are
//! absent, which is what keeps a pre-drift differential fixture — opened
//! directly, never through a boot — readable at all.
//!
//! v4's migration is three independent `ALTER TABLE … ADD COLUMN … TEXT`
//! statements, each guarded by its own column-presence check; this reproduces
//! that shape (a half-migrated table gains only what it lacks). No index is
//! created: v4's migration adds none, and the D23 re-dump at `d553f72a`
//! confirms `generateDDL` adds none either.
//!
//! v4's migration pretty label ("Preparing archive tombstones for your
//! characters") has no v5 analog — v5 surfaces no migration labels anywhere.
//! Recorded NO-PORT, the `231be14c` precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The three columns, in v4's migration order, each `TEXT` and nullable.
const ARCHIVE_COLUMNS: [&str; 3] = ["archivedAt", "archiveFileId", "archivedAvatarFileId"];

/// Add whichever of the three archive columns the `characters` table lacks.
///
/// A no-op when the table is absent (a partition that has never held
/// characters) or already carries all three.
pub fn ensure_character_archive_columns(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "characters")? {
        return Ok(());
    }

    let existing = column_names(main, "characters")?;
    for col in ARCHIVE_COLUMNS {
        if !existing.iter().any(|c| c == col) {
            main.execute_batch(&format!(
                "ALTER TABLE \"characters\" ADD COLUMN \"{col}\" TEXT"
            ))?;
        }
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

    /// The pre-drift shape: a `characters` table with none of the three.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"characters\" (\"id\" TEXT PRIMARY KEY, \"userId\" TEXT NOT NULL, \
             \"name\" TEXT NOT NULL, \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db
    }

    fn has(db: &Connection, col: &str) -> bool {
        column_names(db, "characters")
            .expect("pragma")
            .iter()
            .any(|c| c == col)
    }

    #[test]
    fn adds_all_three_to_an_old_table() {
        let db = old_schema_db();
        for col in ARCHIVE_COLUMNS {
            assert!(!has(&db, col), "{col} should be absent before the ensure");
        }

        ensure_character_archive_columns(&db).expect("ensure");

        for col in ARCHIVE_COLUMNS {
            assert!(has(&db, col), "{col} should exist after the ensure");
        }
    }

    #[test]
    fn adds_only_what_is_missing_and_is_idempotent() {
        let db = old_schema_db();
        db.execute_batch("ALTER TABLE \"characters\" ADD COLUMN \"archivedAt\" TEXT")
            .expect("half-migrate");

        // A second ADD of archivedAt would be a hard error, so surviving this
        // call at all proves the per-column guard.
        ensure_character_archive_columns(&db).expect("ensure over a half-migrated table");
        for col in ARCHIVE_COLUMNS {
            assert!(has(&db, col));
        }

        ensure_character_archive_columns(&db).expect("re-run is a no-op");
        for col in ARCHIVE_COLUMNS {
            assert!(has(&db, col));
        }
    }

    #[test]
    fn no_characters_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        ensure_character_archive_columns(&db).expect("no-op on a table-less partition");
    }
}
