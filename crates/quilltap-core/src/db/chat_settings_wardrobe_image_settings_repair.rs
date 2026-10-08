//! The `chat_settings.wardrobeImageSettings` boot ensure (v4 `7c8572869`,
//! "Wardrobe item images (#82)" — migration `add-wardrobe-image-settings-
//! field-v1`, `dependsOn: ['sqlite-initial-schema-v1']`, `introducedInVersion
//! '4.10.0'`, registered after `seed-wardrobe-wear-stats-v1`; P4.D255).
//!
//! v5's migration runner is a locked deferral, so the column add is re-homed
//! as a boot ensure over the main partition — the
//! `chat_settings_composer_repair` / `chat_informs_permanent_repair`
//! precedent — run from `seed_built_ins` after the wear-ledger ensure (v4's
//! registration order).
//!
//! ## Three v4 shapes reach the read path (§R.4(c))
//!
//!   - **the migration** (`add-wardrobe-image-settings-field-v1.ts:27, 50-55`):
//!     `addColumnIfMissing('chat_settings', 'wardrobeImageSettings', "TEXT
//!     DEFAULT '{\"imageProfileId\":null}'")` — appended LAST, its DEFAULT the
//!     ONE-key object (`DEFAULT_WARDROBE_IMAGE_SETTINGS`, never bumped when
//!     `b3f937076` added `generateFromTools`);
//!   - **generateDDL** (`WardrobeImageSettingsSchema.optional()` — no
//!     `.default`): a nullable `"wardrobeImageSettings" TEXT` with NO default,
//!     in schema order between `storyBackgroundsSettings` and
//!     `conciergeSettings` (the D23 re-dump #6, `fresh_schema.json`);
//!   - **the repository seed** (`chat-settings.repository.ts:222-225`):
//!     `{imageProfileId: null, generateFromTools: false}` on `updateForUser`'s
//!     create branch (`chat_settings_seed.json`).
//!
//! This ensure spells the MIGRATION's clause verbatim; the read
//! (`chat_settings::find_by_user_id`) parses every stored object with both
//! fields defaulted, so the one-key DEFAULT reads as the two-key object.
//!
//! ## No stamp (R-B)
//!
//! Unlike the wear-ledger seed, a re-ADD is a no-op (`addColumnIfMissing`'s own
//! gate and v4's `shouldRun` column gate), so there is no double-apply hazard
//! and — as every sibling column ensure — nothing is written to
//! `migrations_state`.
//!
//! ## NO-PORT
//!
//! v4's migration INFO (`Added wardrobeImageSettings column to chat_settings
//! table`), its ERROR line, and `lib/startup/prettify.ts:158`'s label
//! ("Engaging a portraitist for the wardrobe") belong to the deferred runner.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The column, with v4's migration clause verbatim.
const COLUMN: &str = "wardrobeImageSettings";
const DECL: &str = "TEXT DEFAULT '{\"imageProfileId\":null}'";

/// Add `chat_settings.wardrobeImageSettings` when absent — a no-op when the
/// table is absent (v4's `shouldRun` table gate) or the column exists in
/// either shape (its column gate).
pub fn ensure_chat_settings_wardrobe_image_settings(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "chat_settings")? {
        return Ok(());
    }
    if column_names(main, "chat_settings")?
        .iter()
        .any(|c| c == COLUMN)
    {
        return Ok(());
    }
    main.execute_batch(&format!(
        "ALTER TABLE \"chat_settings\" ADD COLUMN \"{COLUMN}\" {DECL}"
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
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(conn: &Connection) -> Vec<(String, String, Option<String>)> {
        let mut stmt = conn
            .prepare("PRAGMA table_info(\"chat_settings\")")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(4)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// Absent → appended with v4's one-key DEFAULT, and an existing row's cell
    /// reads as that default; a second run is a no-op.
    #[test]
    fn an_absent_column_is_appended_with_the_one_key_default() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_settings (id TEXT PRIMARY KEY, conciergeSettings TEXT);\
             INSERT INTO chat_settings (id) VALUES ('s1');",
        )
        .unwrap();
        ensure_chat_settings_wardrobe_image_settings(&conn).unwrap();
        let cols = info(&conn);
        assert_eq!(
            cols.last().unwrap(),
            &(
                "wardrobeImageSettings".to_string(),
                "TEXT".to_string(),
                Some("'{\"imageProfileId\":null}'".to_string())
            )
        );
        let cell: String = conn
            .query_row("SELECT wardrobeImageSettings FROM chat_settings", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(cell, "{\"imageProfileId\":null}");
        let sql_before: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        ensure_chat_settings_wardrobe_image_settings(&conn).unwrap();
        let sql_after: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sql_before, sql_after);
    }

    /// The fresh generateDDL shape (nullable TEXT, no default, schema order)
    /// is left exactly as it is.
    #[test]
    fn the_fresh_shape_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_settings (id TEXT PRIMARY KEY, wardrobeImageSettings TEXT, conciergeSettings TEXT);",
        )
        .unwrap();
        let before = info(&conn);
        ensure_chat_settings_wardrobe_image_settings(&conn).unwrap();
        assert_eq!(info(&conn), before);
    }

    #[test]
    fn no_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_chat_settings_wardrobe_image_settings(&conn).unwrap();
        assert!(!table_exists(&conn, "chat_settings").unwrap());
    }
}
