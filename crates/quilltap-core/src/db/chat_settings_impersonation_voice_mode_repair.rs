//! The `chat_settings."impersonationVoiceMode"` boot ensure (v4 `07b8f0209`
//! — migration `impersonation-voice-mode-v1`, `introducedInVersion: '4.10.0'`,
//! `dependsOn: ['add-impersonation-voice-rewrite-field-v1']`), P4.D251.
//!
//! v4 REPLACED the 4.10-dev on/off column `impersonationVoiceRewrite`
//! (INTEGER 0/1, P4.D179) with `impersonationVoiceMode` (TEXT: `'off'` /
//! `'ask'` / `'always'`) at the SAME schema position, through a migration that
//! (i) adds the new column, (ii) translates every row still at the column
//! default from the old column — `1` → `'ask'`, anything else → `'off'`
//! ([`crate::services::impersonation_voice_legacy`]) — and (iii) DROPS the old
//! column. v5's migration runner is a locked deferral, so — following the
//! P4.d7 / P4.D41 / P4.D63 / P4.D73 / P4.D77 / P4.D79 / P4.D135 / P4.D171 /
//! P4.D179 / P4.D249 precedents — the pass is re-homed as a **boot repair over
//! the main partition**; a fresh instance already has the new column from the
//! D23 re-dump #5 at `07b8f0209`.
//!
//! ## This ensure REPLACES the P4.D179 one — the cross-app hazard (§R.13)
//!
//! `db/chat_settings_impersonation_voice_repair.rs` (deleted with this
//! module) re-ADDED the old column whenever it was absent. Live Friday has
//! carried `impersonationVoiceRewrite = 1` since before 2026-09-15, and v4's
//! next boot on it runs `impersonation-voice-mode-v1`: the row becomes `'ask'`
//! and the old column is DROPPED. A v5 still running the P4.D179 ensure would
//! re-add the dropped column (default 0) on that file, read the setting as OFF
//! where v4 says `'ask'`, and v4 would then re-run its migration on the next
//! boot (translating nothing — the mode is already `'ask'` — and dropping the
//! column again): a ping-pong. So the old ensure is gone, and THIS ensure's
//! "both columns present" arm — v4's own re-runnable `shouldRun` (`new column
//! missing OR old column present`) — is what heals a file v5 damaged in the
//! window between v4's migration landing and this port unifying.
//!
//! ## ONE shape, measured
//!
//! generateDDL spells `ImpersonationVoiceModeEnum.default('off')` as
//! `"impersonationVoiceMode" TEXT DEFAULT 'off'` (in schema order, after
//! `composerUnicode`) and the migration's `addColumnIfMissing(…, "TEXT DEFAULT
//! 'off'")` renders the SAME clause (appended) — measured at the pin, unlike
//! the P4.D249 `NOT NULL` pair. Only the POSITION differs, which the tolerant
//! positional read already absorbs (every v5 read and write names its
//! columns). [`MODE_DECL`] is the one clause both carry.
//!
//! ## v4's predicate, verbatim
//!
//! The backfill touches only rows `WHERE "impersonationVoiceMode" IS NULL OR
//! "impersonationVoiceMode" = 'off'` — a row already `'always'` or `'ask'`
//! (written by v4 after its migration, or by the SPA) is left alone even when
//! the stale old column beside it says `1`. Then the old column is dropped
//! (`ALTER TABLE … DROP COLUMN`, SQLite ≥ 3.35; v5 links 3.53.2 — pinned by a
//! test below; no index or trigger names the column, so a plain DROP is
//! enough, as v4's migration header says).
//!
//! ## One transaction — a recorded divergence
//!
//! v4's migration runs statement by statement with no enclosing transaction;
//! a failure mid-pass (after the ADD, before the DROP) leaves a half-migrated
//! table its `shouldRun` would simply resume on the next boot. v5 wraps the
//! whole pass in ONE transaction so a failure leaves the table EXACTLY as it
//! was: on a shared instance a half-applied pass is the worse state (both
//! columns present with a partial backfill), and the atomic pass is the
//! strictly safer superset — every end state v5 can leave is one v4's
//! re-runnable migration also reaches. Recorded here and in the lane record.
//!
//! ## NO-PORT (the fourth recording)
//!
//! v4's migration INFO (`Replaced the impersonated-line voice toggle with a
//! three-state mode` with `columnsAdded / rowsBackfilled / columnsDropped /
//! durationMs`), its two DEBUGs (`Translating the impersonated-line voice
//! toggle` with `candidates`; `Translated one impersonated-line voice setting`
//! per row), its ERROR (`Failed to replace the impersonated-line voice
//! toggle`), its `migrations_state` ledger row, and its pretty label ("Teaching
//! the prompter to wait until called upon, rather than whispering every line
//! unbidden…", `lib/startup/prettify.ts:149`) belong to the deferred migration
//! runner and have no v5 analog — the P4.D63 / P4.D73 / P4.D79 / P4.D249
//! precedent `chats_cycle_order_repair.rs` records.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::{params, Connection};

use super::DbError;
use crate::services::impersonation_voice_legacy::{
    impersonation_voice_mode_from_legacy, LEGACY_KEY, MODE_KEY,
};

/// The type + DEFAULT clause v4 spells for the column — identical from
/// generateDDL and from the migration's `addColumnIfMissing` (measured).
pub const MODE_DECL: &str = "TEXT DEFAULT 'off'";

/// v4's three statements, verbatim (`impersonation-voice-mode.ts:72-100`).
const ADD_MODE: &str =
    "ALTER TABLE \"chat_settings\" ADD COLUMN \"impersonationVoiceMode\" TEXT DEFAULT 'off'";
const SELECT_CANDIDATES: &str =
    "SELECT \"id\", \"impersonationVoiceRewrite\" FROM \"chat_settings\" \
     WHERE \"impersonationVoiceMode\" IS NULL OR \"impersonationVoiceMode\" = 'off'";
const UPDATE_ROW: &str =
    "UPDATE \"chat_settings\" SET \"impersonationVoiceMode\" = ? WHERE \"id\" = ?";
const DROP_OLD: &str = "ALTER TABLE \"chat_settings\" DROP COLUMN \"impersonationVoiceRewrite\"";

/// What one pass did — asserted by tests, never logged (NO-PORT above).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct VoiceModeEnsureOutcome {
    pub column_added: bool,
    pub rows_backfilled: usize,
    pub column_dropped: bool,
}

/// v4 `impersonation-voice-mode-v1` over `chat_settings`: add the mode column
/// when absent, translate every row still at the default from the retired
/// boolean when that column is present, then drop the retired column. One
/// transaction. A no-op (and `Ok(Default::default())`) when the table is
/// absent (v4's `shouldRun` table gate) or already migrated.
pub fn ensure_chat_settings_impersonation_voice_mode(
    main: &Connection,
) -> Result<VoiceModeEnsureOutcome, DbError> {
    if !table_exists(main, "chat_settings")? {
        return Ok(VoiceModeEnsureOutcome::default());
    }
    let columns = column_names(main, "chat_settings")?;
    let has_mode = columns.iter().any(|c| c == MODE_KEY);
    let has_old = columns.iter().any(|c| c == LEGACY_KEY);
    if has_mode && !has_old {
        return Ok(VoiceModeEnsureOutcome::default());
    }

    let tx = main.unchecked_transaction()?;
    let mut outcome = VoiceModeEnsureOutcome::default();
    if !has_mode {
        tx.execute_batch(ADD_MODE)?;
        outcome.column_added = true;
    }
    if has_old {
        let candidates: Vec<(String, rusqlite::types::Value)> = {
            let mut stmt = tx.prepare(SELECT_CANDIDATES)?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            out
        };
        for (id, legacy) in candidates {
            let mode = impersonation_voice_mode_from_legacy(&cell_to_json(legacy));
            tx.execute(UPDATE_ROW, params![mode.as_str(), id])?;
            outcome.rows_backfilled += 1;
        }
        tx.execute_batch(DROP_OLD)?;
        outcome.column_dropped = true;
    }
    tx.commit()?;
    Ok(outcome)
}

/// The retired cell as better-sqlite3 hands it to v4's translation: an
/// INTEGER as a JS number, a REAL as a number, TEXT as a string, NULL as
/// `null` (a BLOB has no faithful JSON spelling and translates to `'off'` on
/// both sides — v4's `=== 1` never matches a Buffer).
fn cell_to_json(v: rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value as Sql;
    match v {
        Sql::Null | Sql::Blob(_) => serde_json::Value::Null,
        Sql::Integer(i) => serde_json::Value::from(i),
        Sql::Real(f) => serde_json::Value::from(f),
        Sql::Text(s) => serde_json::Value::String(s),
    }
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

    /// A `chat_settings` table in the pre-`686954937` shape: neither voice
    /// column (the add-field migration's target).
    fn base_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"chat_settings\" (\"id\" TEXT PRIMARY KEY NOT NULL, \"userId\" TEXT NOT NULL, \
             \"composerUnicode\" INTEGER DEFAULT 1, \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);",
        )
        .expect("ddl");
        db
    }

    /// v4 `add-impersonation-voice-rewrite-field.ts:64`, verbatim — how a
    /// real instance got the old column.
    fn add_old_column(db: &Connection) {
        db.execute_batch(
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"impersonationVoiceRewrite\" INTEGER DEFAULT 0",
        )
        .expect("v4's add-field migration");
    }

    fn insert(db: &Connection, id: &str, cols: &str, vals: &str) {
        db.execute_batch(&format!(
            "INSERT INTO chat_settings (id, userId, createdAt, updatedAt{cols}) \
             VALUES ('{id}', 'u1', 't', 't'{vals})"
        ))
        .expect("seed row");
    }

    fn cols(db: &Connection) -> Vec<String> {
        column_names(db, "chat_settings").expect("pragma")
    }

    fn modes(db: &Connection) -> Vec<(String, Option<String>)> {
        let mut stmt = db
            .prepare("SELECT id, impersonationVoiceMode FROM chat_settings ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn table_sql(db: &Connection) -> String {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn no_chat_settings_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        assert_eq!(
            ensure_chat_settings_impersonation_voice_mode(&db).unwrap(),
            VoiceModeEnsureOutcome::default()
        );
    }

    /// Mode (B): neither column — the mode is added alone and every row reads
    /// `'off'` through the DEFAULT (v4 runs add-field THEN mode and lands on
    /// the same shape; the differential proves that, this pins the outcome).
    #[test]
    fn neither_column_adds_the_mode_and_every_row_reads_off() {
        let db = base_db();
        insert(&db, "s1", "", "");
        insert(&db, "s2", "", "");
        let outcome = ensure_chat_settings_impersonation_voice_mode(&db).unwrap();
        assert_eq!(
            outcome,
            VoiceModeEnsureOutcome {
                column_added: true,
                rows_backfilled: 0,
                column_dropped: false
            }
        );
        assert_eq!(cols(&db).last().unwrap(), MODE_KEY, "appended");
        assert!(!cols(&db).iter().any(|c| c == LEGACY_KEY));
        assert_eq!(
            modes(&db),
            vec![
                ("s1".into(), Some("off".into())),
                ("s2".into(), Some("off".into()))
            ]
        );
    }

    /// Mode (A): the old column present with rows 1 / 0 / NULL → `'ask'` /
    /// `'off'` / `'off'`, and the old column GONE.
    #[test]
    fn old_column_rows_translate_and_the_old_column_is_dropped() {
        let db = base_db();
        add_old_column(&db);
        insert(&db, "s1", ", impersonationVoiceRewrite", ", 1");
        insert(&db, "s2", ", impersonationVoiceRewrite", ", 0");
        insert(&db, "s3", ", impersonationVoiceRewrite", ", NULL");
        let outcome = ensure_chat_settings_impersonation_voice_mode(&db).unwrap();
        assert_eq!(
            outcome,
            VoiceModeEnsureOutcome {
                column_added: true,
                rows_backfilled: 3,
                column_dropped: true
            }
        );
        assert!(!cols(&db).iter().any(|c| c == LEGACY_KEY), "dropped");
        assert_eq!(cols(&db).last().unwrap(), MODE_KEY);
        assert_eq!(
            modes(&db),
            vec![
                ("s1".into(), Some("ask".into())),
                ("s2".into(), Some("off".into())),
                ("s3".into(), Some("off".into()))
            ]
        );
    }

    /// Mode (C), §R.13: both columns present. A row already `'always'` beside
    /// a stale old `1` is LEFT ALONE (v4's `IS NULL OR = 'off'` predicate);
    /// `'off'` + 1 → `'ask'`; NULL + 0 → `'off'`; the old column is dropped.
    #[test]
    fn both_columns_present_backfills_only_default_rows_and_drops_the_old_column() {
        let db = base_db();
        db.execute_batch(ADD_MODE).unwrap();
        add_old_column(&db);
        insert(
            &db,
            "s1",
            ", impersonationVoiceRewrite, impersonationVoiceMode",
            ", 1, 'always'",
        );
        insert(
            &db,
            "s2",
            ", impersonationVoiceRewrite, impersonationVoiceMode",
            ", 1, 'off'",
        );
        insert(
            &db,
            "s3",
            ", impersonationVoiceRewrite, impersonationVoiceMode",
            ", 0, NULL",
        );
        insert(
            &db,
            "s4",
            ", impersonationVoiceRewrite, impersonationVoiceMode",
            ", 1, 'ask'",
        );
        let outcome = ensure_chat_settings_impersonation_voice_mode(&db).unwrap();
        assert_eq!(
            outcome,
            VoiceModeEnsureOutcome {
                column_added: false,
                rows_backfilled: 2,
                column_dropped: true
            }
        );
        assert!(!cols(&db).iter().any(|c| c == LEGACY_KEY));
        assert_eq!(
            modes(&db),
            vec![
                ("s1".into(), Some("always".into())),
                ("s2".into(), Some("ask".into())),
                ("s3".into(), Some("off".into())),
                ("s4".into(), Some("ask".into()))
            ]
        );
    }

    /// A migrated table is an EXACT no-op: `sqlite_master.sql` byte-equal
    /// before and after, and a second run of the ensure the same.
    #[test]
    fn a_migrated_table_and_a_second_run_are_exact_no_ops() {
        let db = base_db();
        add_old_column(&db);
        insert(&db, "s1", ", impersonationVoiceRewrite", ", 1");
        ensure_chat_settings_impersonation_voice_mode(&db).unwrap();
        let before = table_sql(&db);
        let again = ensure_chat_settings_impersonation_voice_mode(&db).unwrap();
        assert_eq!(again, VoiceModeEnsureOutcome::default());
        assert_eq!(table_sql(&db), before);
        assert_eq!(modes(&db), vec![("s1".into(), Some("ask".into()))]);
    }

    /// The whole pass is one transaction: a failure after the ADD leaves the
    /// table untouched. Posed by making the DROP impossible — a VIEW naming
    /// the old column makes SQLite refuse `DROP COLUMN` — and asserting the
    /// mode column is NOT left behind.
    #[test]
    fn a_failing_pass_leaves_the_table_exactly_as_it_was() {
        let db = base_db();
        add_old_column(&db);
        insert(&db, "s1", ", impersonationVoiceRewrite", ", 1");
        db.execute_batch(
            "CREATE VIEW legacy_view AS SELECT impersonationVoiceRewrite FROM chat_settings",
        )
        .unwrap();
        let before = table_sql(&db);
        let err = ensure_chat_settings_impersonation_voice_mode(&db).unwrap_err();
        assert!(
            err.to_string().contains("impersonationVoiceRewrite"),
            "{err}"
        );
        assert_eq!(
            table_sql(&db),
            before,
            "the ADD was rolled back with the failure"
        );
        assert!(!cols(&db).iter().any(|c| c == MODE_KEY));
    }

    /// The ALTER's DDL must be v4's migration bytes — and generateDDL's
    /// clause is the same one (the ONE-shape measurement in the header).
    #[test]
    fn the_ddl_matches_v4s_migration_and_generate_ddl() {
        assert_eq!(
            ADD_MODE,
            format!("ALTER TABLE \"chat_settings\" ADD COLUMN \"{MODE_KEY}\" {MODE_DECL}")
        );
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
            .expect("the fresh chat_settings DDL");
        assert!(
            ddl.contains(&format!("  \"{MODE_KEY}\" {MODE_DECL},\n")),
            "generateDDL spells the same clause"
        );
        assert!(
            !ddl.contains(LEGACY_KEY),
            "the fresh DDL no longer names the retired column"
        );
    }

    /// `ALTER TABLE … DROP COLUMN` needs SQLite ≥ 3.35.0; v5 links 3.53.2.
    #[test]
    fn the_linked_sqlite_supports_drop_column() {
        assert!(
            rusqlite::version_number() >= 3_035_000,
            "{}",
            rusqlite::version()
        );
    }
}
