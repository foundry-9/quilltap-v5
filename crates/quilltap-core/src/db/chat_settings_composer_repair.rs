//! The `chat_settings` composer/typography column boot ensure (v4 4.8.2 —
//! migrations `add-composer-emoji-field-v1`, `add-composer-unicode-field-v1`,
//! and `add-smart-typography-settings-field-v1`).
//!
//! v4 adds `composerEmoji` / `composerUnicode` / `smartTypographySettings` to
//! `chat_settings` through its migration runner. v5's migration runner is a
//! locked deferral, so — following the P4.d7 (NOCASE namespace), P4.D41
//! (`linkGroupId`) and P4.D63 (`characters` archive columns) precedents — the
//! column adds are re-homed as a **boot repair pass over the main partition**:
//! any instance v5 boots gains the columns, and a fresh instance already has
//! them from the D23 `generateDDL` re-dump at `48396682`.
//!
//! This is the WRITE-side half of the tolerance story, and here it is
//! load-bearing rather than cosmetic. The read side
//! ([`super::chat_settings::find_by_user_id`]) tolerates each column's absence
//! by substituting the Zod default, so an un-repaired instance still OPENS and
//! still renders the settings screen — but the SAVE does not survive.
//! MEASURED on an un-ensured instance (the ensure disabled, the web wire test's
//! probe): `update_for_user`'s update branch is a plain `UPDATE … SET
//! composerEmoji = ?`, so the PUT answers
//! `500 sqlite error: no such column: composerEmoji` — the same class as the
//! third Friday dogfood sighting, which bricked every page load. (The
//! create-a-default-row branch goes through [`crate::db::tolerant_insert`]
//! instead and would drop the value silently.) The ensure is what makes the
//! toggles reachable at all.
//!
//! v4's three migrations are three independent `ALTER TABLE … ADD COLUMN`
//! statements, each guarded by its own column-presence check; this reproduces
//! that shape (a half-migrated table gains only what it lacks) **with v4's
//! exact column types and DEFAULT clauses**, which matters because a v4
//! instance later opened by v4 must see the same defaults for rows written
//! before the add.
//!
//! v4's migration pretty labels ("Cataloguing the little faces", "Teaching the
//! machine its Greek", "Teaching the quotation marks to curtsey") have no v5
//! analog — v5 surfaces no migration labels anywhere. Recorded NO-PORT, the
//! P4.D63 / `231be14c` precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// v4's `SmartTypographySettingsSchema` defaults, serialized exactly as v4's
/// migration serializes them (`JSON.stringify` of the three keys in schema
/// declaration order). Also the DDL default the D23 re-dump captured.
pub const SMART_TYPOGRAPHY_DEFAULT_JSON: &str =
    r#"{"displayQuotes":false,"dashes":true,"ellipsis":true}"#;

/// The three columns in v4's migration order, each with the column type +
/// DEFAULT clause its migration writes verbatim. The third entry embeds
/// [`SMART_TYPOGRAPHY_DEFAULT_JSON`]; `the_ddl_default_matches_the_json_const`
/// pins the two to each other.
const COMPOSER_COLUMNS: [(&str, &str); 3] = [
    // `add-composer-emoji-field-v1`
    ("composerEmoji", "INTEGER DEFAULT 1"),
    // `add-composer-unicode-field-v1`
    ("composerUnicode", "INTEGER DEFAULT 1"),
    // `add-smart-typography-settings-field-v1`
    (
        "smartTypographySettings",
        r#"TEXT DEFAULT '{"displayQuotes":false,"dashes":true,"ellipsis":true}'"#,
    ),
];

/// Add whichever of the three composer/typography columns the `chat_settings`
/// table lacks.
///
/// A no-op when the table is absent (a partition that has never held settings)
/// or already carries all three.
pub fn ensure_chat_settings_composer_columns(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chat_settings")? {
        return Ok(());
    }

    let existing = column_names(main, "chat_settings")?;
    for (col, decl) in COMPOSER_COLUMNS {
        if existing.iter().any(|c| c == col) {
            continue;
        }
        main.execute_batch(&format!(
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"{col}\" {decl}"
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

    /// The pre-4.8.2 shape: a `chat_settings` table with none of the three.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"chat_settings\" (\"id\" TEXT PRIMARY KEY, \"userId\" TEXT NOT NULL, \
             \"composerSpellcheck\" INTEGER DEFAULT 1, \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db
    }

    fn has(db: &Connection, col: &str) -> bool {
        column_names(db, "chat_settings")
            .expect("pragma")
            .iter()
            .any(|c| c == col)
    }

    #[test]
    fn adds_all_three_to_an_old_table() {
        let db = old_schema_db();
        for (col, _) in COMPOSER_COLUMNS {
            assert!(!has(&db, col), "{col} should be absent before the ensure");
        }

        ensure_chat_settings_composer_columns(&db).expect("ensure");

        for (col, _) in COMPOSER_COLUMNS {
            assert!(has(&db, col), "{col} should exist after the ensure");
        }
    }

    /// The DEFAULT clauses are the point of the ensure: a row that predates the
    /// add must read back with v4's Zod defaults, not NULL.
    #[test]
    fn a_preexisting_row_reads_v4_defaults() {
        let db = old_schema_db();
        db.execute(
            "INSERT INTO chat_settings (id, userId, createdAt, updatedAt) \
             VALUES ('s1', 'u1', '2026-08-13T00:00:00.000Z', '2026-08-13T00:00:00.000Z')",
            [],
        )
        .expect("seed row");

        ensure_chat_settings_composer_columns(&db).expect("ensure");

        let (emoji, unicode, typo): (i64, i64, String) = db
            .query_row(
                "SELECT composerEmoji, composerUnicode, smartTypographySettings \
                 FROM chat_settings WHERE id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("read back");
        assert_eq!(emoji, 1, "composerEmoji defaults to v4's 1");
        assert_eq!(unicode, 1, "composerUnicode defaults to v4's 1");
        assert_eq!(
            typo, SMART_TYPOGRAPHY_DEFAULT_JSON,
            "smartTypographySettings defaults to v4's serialized bag"
        );
    }

    #[test]
    fn adds_only_what_is_missing_and_is_idempotent() {
        let db = old_schema_db();
        db.execute_batch(
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"composerEmoji\" INTEGER DEFAULT 1",
        )
        .expect("half-migrate");

        // A second ADD of composerEmoji would be a hard error, so surviving this
        // call at all proves the per-column guard.
        ensure_chat_settings_composer_columns(&db).expect("ensure over a half-migrated table");
        for (col, _) in COMPOSER_COLUMNS {
            assert!(has(&db, col));
        }

        ensure_chat_settings_composer_columns(&db).expect("re-run is a no-op");
        for (col, _) in COMPOSER_COLUMNS {
            assert!(has(&db, col));
        }
    }

    /// One source of truth for v4's serialized default bag: the DDL clause the
    /// ALTER writes must carry exactly the JSON the repository seed and the
    /// route's per-key defaults produce.
    #[test]
    fn the_ddl_default_matches_the_json_const() {
        let (_, decl) = COMPOSER_COLUMNS[2];
        assert_eq!(
            decl,
            format!("TEXT DEFAULT '{SMART_TYPOGRAPHY_DEFAULT_JSON}'")
        );
    }

    #[test]
    fn no_chat_settings_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        ensure_chat_settings_composer_columns(&db).expect("no-op on a table-less partition");
    }
}
