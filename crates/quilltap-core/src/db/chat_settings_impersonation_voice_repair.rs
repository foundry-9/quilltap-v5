//! The `chat_settings."impersonationVoiceRewrite"` boot ensure (v4 `686954937`
//! — migration `add-impersonation-voice-rewrite-field-v1`, `introducedInVersion:
//! '4.10.0'`).
//!
//! v4 adds the column through its migration runner: ONE `ALTER TABLE`, guarded
//! by its own column-presence check, no collection loop. v5's migration runner
//! is a locked deferral, so — following the P4.d7 (NOCASE namespace), P4.D41
//! (`linkGroupId`), P4.D63 (`characters` archive columns) and P4.D73 (the three
//! 4.8.2 composer columns) precedents — the add is re-homed as a **boot repair
//! pass over the main partition**: any instance v5 boots gains the column, and
//! a fresh instance already has it from the D23 `generateDDL` re-dump at
//! `f4ad2c8d1`.
//!
//! **Load-bearing, not cosmetic** — the P4.D73 module's paragraph applies
//! verbatim, and this column is the same shape. The read side
//! ([`super::chat_settings::find_by_user_id`]) tolerates the column's absence by
//! substituting v4's Zod default (`false`), so an un-repaired instance still
//! OPENS and still renders the settings screen. The SAVE does not survive:
//! `update_for_user`'s update branch is a plain `UPDATE … SET
//! impersonationVoiceRewrite = ?`, which answers `500 … no such column` without
//! the ensure. That claim is pinned at the web venue rather than asserted here
//! (`crates/quilltap-web/tests/chat_settings_composer_web_routes.rs`).
//!
//! **The cross-app hazard is the reason the DEFAULT clause is copied verbatim
//! rather than reasoned about.** The Friday instance is shared with v4's dev
//! build, which ran `add-impersonation-voice-rewrite-field-v1` on it the day the
//! feature shipped. So the common case here is a table that ALREADY carries the
//! column — the ensure must be an exact no-op on it — and the uncommon case (v5
//! adding it first, v4 opening later) must leave v4 reading the same
//! `INTEGER DEFAULT 0` it would have written itself for rows that predate the
//! add.
//!
//! v4's migration pretty label ("Fitting the prompter's box beneath the stage,
//! so a borrowed voice may be rehearsed before it carries…",
//! `lib/startup/prettify.ts:144`) has no v5 analog — v5 surfaces no migration
//! labels anywhere. Recorded NO-PORT, the P4.D63 / P4.D73 / `231be14c`
//! precedent; this is the third recording of that ruling.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The column name and the type + DEFAULT clause v4's migration writes
/// verbatim (`ALTER TABLE "chat_settings" ADD COLUMN
/// "impersonationVoiceRewrite" INTEGER DEFAULT 0`).
const IMPERSONATION_VOICE_COLUMN: (&str, &str) = ("impersonationVoiceRewrite", "INTEGER DEFAULT 0");

/// Add `impersonationVoiceRewrite` to `chat_settings` if it is absent.
///
/// A no-op when the table is absent (a partition that has never held settings)
/// or already carries the column.
pub fn ensure_chat_settings_impersonation_voice_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chat_settings")? {
        return Ok(());
    }

    let (col, decl) = IMPERSONATION_VOICE_COLUMN;
    if column_names(main, "chat_settings")?
        .iter()
        .any(|c| c == col)
    {
        return Ok(());
    }
    main.execute_batch(&format!(
        "ALTER TABLE \"chat_settings\" ADD COLUMN \"{col}\" {decl}"
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

    /// The pre-4.10 shape: a `chat_settings` table without the column.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"chat_settings\" (\"id\" TEXT PRIMARY KEY, \"userId\" TEXT NOT NULL, \
             \"composerUnicode\" INTEGER DEFAULT 1, \"createdAt\" TEXT, \"updatedAt\" TEXT);",
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
    fn adds_the_column_to_an_old_table() {
        let db = old_schema_db();
        assert!(!has(&db, "impersonationVoiceRewrite"));
        ensure_chat_settings_impersonation_voice_column(&db).expect("ensure");
        assert!(has(&db, "impersonationVoiceRewrite"));
    }

    /// The DEFAULT clause is the point of the ensure: a row that predates the
    /// add must read back with v4's Zod default (`false` → 0), not NULL — and
    /// a v4 build later opening the same file must agree.
    #[test]
    fn a_preexisting_row_reads_v4_default() {
        let db = old_schema_db();
        db.execute(
            "INSERT INTO chat_settings (id, userId, createdAt, updatedAt) \
             VALUES ('s1', 'u1', '2026-09-10T00:00:00.000Z', '2026-09-10T00:00:00.000Z')",
            [],
        )
        .expect("seed row");

        ensure_chat_settings_impersonation_voice_column(&db).expect("ensure");

        let v: i64 = db
            .query_row(
                "SELECT impersonationVoiceRewrite FROM chat_settings WHERE id = 's1'",
                [],
                |r| r.get(0),
            )
            .expect("read back");
        assert_eq!(v, 0, "impersonationVoiceRewrite defaults to v4's 0");
    }

    /// Running twice is a no-op — a second ADD of the same column is a hard
    /// SQLite error, so surviving the call at all proves the guard.
    #[test]
    fn a_second_run_is_a_no_op() {
        let db = old_schema_db();
        ensure_chat_settings_impersonation_voice_column(&db).expect("first run");
        ensure_chat_settings_impersonation_voice_column(&db).expect("second run is a no-op");
        assert!(has(&db, "impersonationVoiceRewrite"));
    }

    /// §R.12a, the cross-app hazard: the SHARED Friday instance already carries
    /// the column, because v4's own migration ran on it the day the feature
    /// shipped. The ensure must not touch such a table — and must not disturb a
    /// value v4 already wrote there.
    #[test]
    fn a_table_that_already_carries_the_column_is_untouched() {
        let db = old_schema_db();
        db.execute_batch(
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"impersonationVoiceRewrite\" INTEGER DEFAULT 0",
        )
        .expect("v4's own migration");
        db.execute(
            "INSERT INTO chat_settings (id, userId, impersonationVoiceRewrite, createdAt, updatedAt) \
             VALUES ('s1', 'u1', 1, '2026-09-10T00:00:00.000Z', '2026-09-10T00:00:00.000Z')",
            [],
        )
        .expect("a v4-written row with the toggle ON");

        ensure_chat_settings_impersonation_voice_column(&db).expect("no-op on a migrated table");

        let v: i64 = db
            .query_row(
                "SELECT impersonationVoiceRewrite FROM chat_settings WHERE id = 's1'",
                [],
                |r| r.get(0),
            )
            .expect("read back");
        assert_eq!(v, 1, "the v4-written value survives the ensure untouched");
    }

    #[test]
    fn no_chat_settings_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        ensure_chat_settings_impersonation_voice_column(&db)
            .expect("no-op on a table-less partition");
    }

    /// The ALTER's DDL must be v4's migration bytes — the cross-app default
    /// agreement depends on it.
    #[test]
    fn the_ddl_matches_v4s_migration() {
        let (col, decl) = IMPERSONATION_VOICE_COLUMN;
        assert_eq!(
            format!("ALTER TABLE \"chat_settings\" ADD COLUMN \"{col}\" {decl}"),
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"impersonationVoiceRewrite\" INTEGER DEFAULT 0"
        );
    }
}
