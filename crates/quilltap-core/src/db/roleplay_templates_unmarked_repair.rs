//! The roleplay-template unmarked-text boot ensure — P4.D341, the TWENTY-FIRST
//! D23 move (v4 `778b97b2b` — migration
//! `add-roleplay-template-unmarked-fields-v1`, `dependsOn:
//! ['add-narration-delimiters-field-v1']`, `introducedInVersion '4.11.0'`).
//!
//! v4's why, carried (the migration's docblock): two template-wide rendering
//! columns on `roleplay_templates` —
//!
//! - `unmarkedStyle` (TEXT, JSON `{ style, addOns? }` or NULL): how text that
//!   no delimiter claims is displayed — what lets narration be plain prose;
//! - `markdownAsterisks` (INTEGER, default 1): whether Markdown reads asterisks
//!   as bold / italic; 0 hands single `*` to the template's own delimiters.
//!
//! Existing rows keep today's behaviour (no unmarked style, Markdown asterisks
//! on). The built-in Novel template is seeded with both set by the repository
//! (the seeder, not this ensure).
//!
//! v4 registers the migration LAST at the `236c04e35` pin
//! (`migrations/scripts/index.ts:946`); the host runs it after the llm-logs
//! round-id ensure. TWO-shape columns: generateDDL's `"unmarkedStyle" TEXT,
//! "markdownAsterisks" INTEGER DEFAULT 1` BEFORE `createdAt` on a FRESH
//! instance; the migration's appended `TEXT` / `INTEGER DEFAULT 1` on an
//! existing one (the clauses are byte-equal; only the position differs).
//!
//! UNSTAMPED (`shouldRun` is column-gated), FATAL on failure.
//!
//! ## NO-PORT
//!
//! The migration's ONE INFO `Added unmarked-text columns to roleplay_templates
//! table` (`context: 'migration.add-roleplay-template-unmarked-fields'`,
//! `columnsAdded`, `durationMs`), its result message `Added ${columnsAdded}
//! column(s) to roleplay_templates`, its ERROR `Failed to add unmarked-text
//! columns to roleplay_templates`, the DEBUG `Adding column`, the
//! `migrations_state` row and the prettify label `Setting aside a shelf for the
//! unadorned novel`. The host logs v5's ONE boot INFO (`Set aside a shelf for
//! the unadorned novel`) when the ensure added anything; silent otherwise.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-roleplay-template-unmarked-fields-v1";
/// The migration's column clauses, verbatim.
pub const UNMARKED_STYLE_DECL: &str = "TEXT";
pub const MARKDOWN_ASTERISKS_DECL: &str = "INTEGER DEFAULT 1";

/// `add-roleplay-template-unmarked-fields-v1`: append `unmarkedStyle` then
/// `markdownAsterisks` when absent. Answers how many it added (0–2).
pub fn ensure_roleplay_templates_unmarked_fields(main: &Connection) -> Result<u8, DbError> {
    if !table_exists(main, "roleplay_templates")? {
        return Ok(0);
    }
    let mut added = 0;
    for (column, decl) in [
        ("unmarkedStyle", UNMARKED_STYLE_DECL),
        ("markdownAsterisks", MARKDOWN_ASTERISKS_DECL),
    ] {
        if add_column_if_missing(main, "roleplay_templates", column, decl)? {
            added += 1;
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    fn pre_round() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            r#"CREATE TABLE "roleplay_templates" ("id" TEXT PRIMARY KEY NOT NULL, "name" TEXT, "createdAt" TEXT, "updatedAt" TEXT);
               INSERT INTO roleplay_templates (id, name) VALUES ('t1', 'x');"#,
        )
        .unwrap();
        db
    }

    #[test]
    fn appends_both_in_v4s_order() {
        let db = pre_round();
        assert_eq!(ensure_roleplay_templates_unmarked_fields(&db).unwrap(), 2);
        let sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'roleplay_templates'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.ends_with(", \"unmarkedStyle\" TEXT, \"markdownAsterisks\" INTEGER DEFAULT 1)"),
            "{sql}"
        );
        let (u, m): (Option<String>, i64) = db
            .query_row(
                "SELECT unmarkedStyle, markdownAsterisks FROM roleplay_templates",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((u, m), (None, 1));
        assert_eq!(ensure_roleplay_templates_unmarked_fields(&db).unwrap(), 0);
    }

    #[test]
    fn half_migrated_gets_markdown_asterisks_alone() {
        let db = pre_round();
        db.execute_batch(r#"ALTER TABLE "roleplay_templates" ADD COLUMN "unmarkedStyle" TEXT"#)
            .unwrap();
        assert_eq!(ensure_roleplay_templates_unmarked_fields(&db).unwrap(), 1);
        let names = column_names(&db, "roleplay_templates").unwrap();
        assert_eq!(names.last().unwrap(), "markdownAsterisks");
    }

    #[test]
    fn no_table_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(ensure_roleplay_templates_unmarked_fields(&db).unwrap(), 0);
    }

    /// The committed dump's fresh lines are byte-equal to the clauses.
    #[test]
    fn the_decls_match_the_committed_dump_lines() {
        let fresh: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let ddl = fresh["main"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|s| {
                s.as_str()
                    .filter(|s| s.starts_with("CREATE TABLE \"roleplay_templates\""))
            })
            .unwrap();
        for (col, decl) in [
            ("unmarkedStyle", UNMARKED_STYLE_DECL),
            ("markdownAsterisks", MARKDOWN_ASTERISKS_DECL),
        ] {
            let line = format!("  \"{col}\" {decl},\n");
            assert_eq!(ddl.matches(&line).count(), 1, "{line:?} in {ddl}");
        }
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddl).unwrap();
        assert_eq!(ensure_roleplay_templates_unmarked_fields(&db).unwrap(), 0);
    }
}
