//! The two 4.11 `chat_settings` boot ensures — P4.D304 (C1-4), the TENTH +
//! ELEVENTH D23 moves:
//!
//!   - v4 `362426c8f` ("trigger phrases run custom tools when their words are
//!     said") — migration `add-custom-tool-triggers-field-v1`, `dependsOn:
//!     ['add-custom-tools-field-v1']`, `introducedInVersion '4.11.0'`,
//!     registered at `migrations/scripts/index.ts:909`;
//!   - v4 `4877fc2da` ("slash commands in the composer") — migration
//!     `add-composer-slash-commands-field-v1`, `dependsOn:
//!     ['sqlite-initial-schema-v1']`, `introducedInVersion '4.11.0'`,
//!     registered at `index.ts:911`.
//!
//! Both are appended after the twelve P4.D288 span migrations (slot 12 =
//! `drop-housekeeping-consolidation-jobs-v1`, `index.ts:907`), triggers FIRST;
//! each `dependsOn` is earlier in the array, so registration order IS run
//! order. [`run_chat_settings_4_11_ensures`] is the ONE home for that order —
//! the host's `seed_built_ins` (after `p4d288_span::run_span_ensures`, before
//! the P4.160 index-family backfill), the vintage mirror-boot and the
//! zero-rerun differential all call it, so the three cannot drift (the
//! `p4d288_span` lesson).
//!
//! ## ONE shape per column, measured
//!
//! generateDDL's `"customToolTriggers" INTEGER DEFAULT 0` (cid 18, between
//! `customTools` and `compositionModeDefault`) and `"composerSlashCommands"
//! INTEGER DEFAULT 1` (cid 23, between `composerUnicode` and
//! `impersonationVoiceMode`) are BYTE-EQUAL to the migrations'
//! `addColumnIfMissing(…, 'INTEGER DEFAULT 0')` / `'INTEGER DEFAULT 1'`
//! clauses; only the POSITION differs (a migration appends). Both defaults
//! equal the Zod defaults (`z.boolean().default(false)` / `.default(true)`) and
//! the repository seed values, so a row written by an app that does not bind
//! the column reads the same either way. No backfill on either side.
//!
//! v4's `shouldRun` (`isSQLiteBackend() ∧ sqliteTableExists('chat_settings') ∧
//! !sqliteColumnExists('chat_settings', <col>)`) is the table gate here plus
//! `add_column_if_missing`'s own column gate — one PRAGMA on the happy path.
//!
//! ## No stamp (R-B)
//!
//! A re-ADD is a no-op on either app (`addColumnIfMissing`'s gate and v4's
//! `shouldRun` column gate — a `shouldRun` false is "skipped and NOT
//! recorded" by v4's runner, re-asked every boot), so there is no
//! double-apply hazard and nothing is written to `migrations_state` — as every
//! sibling column ensure. FATAL on failure: neither migration is `resumable`,
//! so a failure stops v4's boot (the host `?`s the chain).
//!
//! ## NO-PORT
//!
//! v4's migration INFO lines — `Added customToolTriggers column to
//! chat_settings table` TWICE on a real add (inside the `if`,
//! `add-custom-tool-triggers-field.ts:54-56`, and again as the summary with
//! `columnsAdded` / `durationMs`, `:61-65`) and `Added composerSlashCommands
//! column to chat_settings table` ONCE (inside the `if` only,
//! `add-composer-slash-commands-field.ts:49-51` — the asymmetry is v4's; do
//! not "fix" one to match the other) — the two ERRORs (`Failed to add
//! customToolTriggers column` / `Failed to add composerSlashCommands column`),
//! `database-utils`' DEBUG `Adding column`, the `migrations_state` rows, and
//! `lib/startup/prettify.ts`'s two labels ("Teaching Pascal which words to
//! listen for" `:188`; "Fitting the composer with a speaking-tube to the tool
//! room" `:150`) belong to the deferred migration runner. The chain logs v5's
//! ONE INFO per column it added (`target: "quilltap::boot"`, `migrationId`,
//! `columnAdded`) and is silent on a no-op boot.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, table_exists};
use super::DbError;

/// v4's migration id for the trigger-phrase toggle.
pub const MIGRATION_ID_TRIGGERS: &str = "add-custom-tool-triggers-field-v1";
/// v4's migration id for the composer slash-command toggle.
pub const MIGRATION_ID_SLASH: &str = "add-composer-slash-commands-field-v1";

/// The trigger-phrase column, with v4's migration clause verbatim.
pub const TRIGGERS_COLUMN: &str = "customToolTriggers";
pub const TRIGGERS_DECL: &str = "INTEGER DEFAULT 0";
/// The composer slash-command column, with v4's migration clause verbatim.
pub const SLASH_COLUMN: &str = "composerSlashCommands";
pub const SLASH_DECL: &str = "INTEGER DEFAULT 1";

fn ensure_column(main: &Connection, column: &str, decl: &str) -> Result<bool, DbError> {
    if !table_exists(main, "chat_settings")? {
        return Ok(false);
    }
    add_column_if_missing(main, "chat_settings", column, decl)
}

/// `add-custom-tool-triggers-field-v1`: add `chat_settings.customToolTriggers`
/// when the table exists and the column does not. Answers whether it added.
pub fn ensure_chat_settings_custom_tool_triggers(main: &Connection) -> Result<bool, DbError> {
    ensure_column(main, TRIGGERS_COLUMN, TRIGGERS_DECL)
}

/// `add-composer-slash-commands-field-v1`: add
/// `chat_settings.composerSlashCommands` when the table exists and the column
/// does not. Answers whether it added.
pub fn ensure_chat_settings_composer_slash_commands(main: &Connection) -> Result<bool, DbError> {
    ensure_column(main, SLASH_COLUMN, SLASH_DECL)
}

/// Run both ensures in v4's registration order (triggers, then slash),
/// logging v5's ONE boot INFO per column added. The first `Err` returns.
///
/// The lines are emitted here, on whatever thread runs the chain — the host
/// runs it inside `Db::write`, so a capture that must see them uses the
/// process-wide rig (memory note `capture-rig-writer-thread`).
pub fn run_chat_settings_4_11_ensures(main: &Connection) -> Result<(), DbError> {
    if ensure_chat_settings_custom_tool_triggers(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = MIGRATION_ID_TRIGGERS,
            columnAdded = true,
            "Fitted the custom-tool trigger toggle"
        );
    }
    if ensure_chat_settings_composer_slash_commands(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = MIGRATION_ID_SLASH,
            columnAdded = true,
            "Fitted the composer slash-command toggle"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    /// The pre-4.11 shape: neither column, around the two neighbours.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"chat_settings\" (\"id\" TEXT PRIMARY KEY, \"userId\" TEXT NOT NULL, \
             \"customTools\" INTEGER DEFAULT 1, \"composerUnicode\" INTEGER DEFAULT 1, \
             \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db
    }

    fn info(db: &Connection) -> Vec<(String, String, Option<String>)> {
        let mut stmt = db.prepare("PRAGMA table_info(\"chat_settings\")").unwrap();
        stmt.query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(4)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// Both appended, in v4's order, with v4's clauses.
    #[test]
    fn adds_both_to_an_old_table_in_registration_order() {
        let db = old_schema_db();
        run_chat_settings_4_11_ensures(&db).expect("chain");
        let cols = info(&db);
        let tail: Vec<_> = cols[cols.len() - 2..].to_vec();
        assert_eq!(
            tail,
            vec![
                (
                    "customToolTriggers".to_string(),
                    "INTEGER".to_string(),
                    Some("0".to_string())
                ),
                (
                    "composerSlashCommands".to_string(),
                    "INTEGER".to_string(),
                    Some("1".to_string())
                ),
            ]
        );
        let sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.ends_with(
                ", \"customToolTriggers\" INTEGER DEFAULT 0, \"composerSlashCommands\" INTEGER DEFAULT 1)"
            ),
            "{sql}"
        );
    }

    /// The DEFAULT clauses are the point: a row that predates the add reads
    /// v4's Zod defaults (`0` / `1`), never NULL.
    #[test]
    fn a_preexisting_row_reads_the_ddl_defaults() {
        let db = old_schema_db();
        db.execute(
            "INSERT INTO chat_settings (id, userId) VALUES ('s1', 'u1')",
            [],
        )
        .unwrap();
        run_chat_settings_4_11_ensures(&db).expect("chain");
        let (t, s): (Option<i64>, Option<i64>) = db
            .query_row(
                "SELECT customToolTriggers, composerSlashCommands FROM chat_settings",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((t, s), (Some(0), Some(1)));
    }

    /// Half-migrated (triggers present, slash absent): only slash is added,
    /// and the answers say so; a second run adds nothing.
    #[test]
    fn half_migrated_adds_one_and_is_idempotent() {
        let db = old_schema_db();
        assert!(ensure_chat_settings_custom_tool_triggers(&db).unwrap());
        assert!(!ensure_chat_settings_custom_tool_triggers(&db).unwrap());
        assert!(ensure_chat_settings_composer_slash_commands(&db).unwrap());
        assert!(!ensure_chat_settings_composer_slash_commands(&db).unwrap());
        let before = info(&db);
        run_chat_settings_4_11_ensures(&db).expect("re-run is a no-op");
        assert_eq!(info(&db), before);
    }

    #[test]
    fn no_chat_settings_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        assert!(!ensure_chat_settings_custom_tool_triggers(&db).unwrap());
        assert!(!ensure_chat_settings_composer_slash_commands(&db).unwrap());
        run_chat_settings_4_11_ensures(&db).expect("no-op");
        assert!(column_names(&db, "chat_settings").unwrap().is_empty());
    }

    /// ONE shape: each `COLUMN` + `DECL` is byte-equal to its line in the
    /// committed generateDDL dump (D23 re-dump #10/#11), position apart — and
    /// the dump carries each exactly once, at cid 18 / 23.
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
                    .filter(|s| s.starts_with("CREATE TABLE \"chat_settings\""))
            })
            .unwrap();
        for (col, decl) in [(TRIGGERS_COLUMN, TRIGGERS_DECL), (SLASH_COLUMN, SLASH_DECL)] {
            let line = format!("  \"{col}\" {decl},\n");
            assert_eq!(ddl.matches(&line).count(), 1, "{line:?}");
        }
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddl).unwrap();
        let names = column_names(&db, "chat_settings").unwrap();
        assert_eq!(names[18], TRIGGERS_COLUMN);
        assert_eq!(names[23], SLASH_COLUMN);
        assert_eq!(names.len(), 39);
        // A fresh table is a no-op for both.
        assert!(!ensure_chat_settings_custom_tool_triggers(&db).unwrap());
        assert!(!ensure_chat_settings_composer_slash_commands(&db).unwrap());
    }
}
