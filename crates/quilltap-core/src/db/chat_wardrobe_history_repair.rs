//! The chat wardrobe history's boot ensures (v4 `83387d0e5` + `4ae20977f` —
//! migrations `add-chat-wardrobe-history-table-v1` (`dependsOn:
//! ['sqlite-initial-schema-v1']`), `add-chat-wardrobe-history-tool-name-v1`
//! (`dependsOn: ['add-chat-wardrobe-history-table-v1']`) and
//! `seed-chat-wardrobe-history-v1` (`dependsOn: [the table, the tool-name
//! column]` — "the tool-name column first: the shared INSERT spells it"), all
//! `introducedInVersion '4.11.0'`, registered in that order after
//! `add-composer-slash-commands-field-v1` (`migrations/scripts/index.ts:
//! 916-919` at the `e8e17b2d9` pin); P4.D315).
//!
//! v4's `generateDDL` never renders this table (its repository is a plain
//! class, no `ensureCollection`), so — the human's 2026-10-19 ruling, standing
//! for every migration-only table — v5 makes it at BOOT through these three
//! ensures, never through a v4-dumped artifact. v5's migration runner is a
//! locked deferral, so each migration is re-homed here, run from the host's
//! `seed_built_ins` after the P4.D304 `chat_settings` pair and BEFORE the
//! P4.160 index-family backfill (which does not carry these two indexes — the
//! table step makes both itself). The same chain runs in
//! `restore_vintage_state`'s mirror-boot and `zero_migrations_rerun`'s
//! `run_chain` — three callers that must not drift.
//!
//! ## The stamps (R-C / R-1 — what RAN, as v4's runner would)
//!
//! v4's runner checks `migrations_state` BEFORE `shouldRun`
//! (`migrations/index.ts`) and records a row only for a migration that RAN
//! (`recordCompletedMigration`, `migrations/state.ts:100-130`). Each step here
//! does the same: completed by either app → skipped, nothing written; else
//! v4's `shouldRun`; else the work + the stamp in ONE savepoint.
//!
//! - **table** — `shouldRun` = the table absent; v4's three DDL statements
//!   (`CHAT_WARDROBE_HISTORY_DDL` at its FINAL, 11-column shape); stamped
//!   `itemsAffected 1`, `Created chat_wardrobe_history table`. A table already
//!   present without a ledger row is skipped and NOT stamped.
//! - **column** — `shouldRun` = the table present AND `actorToolName` absent;
//!   v4's `addColumnIfMissing` (`ALTER TABLE "chat_wardrobe_history" ADD
//!   COLUMN "actorToolName" TEXT`); stamped `itemsAffected 1`, `Added
//!   actorToolName column to chat_wardrobe_history`. Reachable ONLY on the
//!   pre-column vintage (a v4 instance that booted `83387d0e5 … f46cf40a2`):
//!   on a table step 1 just made it is "not needed" and UNRECORDED — exactly
//!   v4. No backfill: such rows keep NULL (v4's header: "the readers label such
//!   an entry plain 'Pascal'").
//! - **seed** — `shouldRun` = the table and `chats` exist and `chats` has BOTH
//!   `equippedOutfit` and `updatedAt`. It has NO once-gate of its own — the
//!   ledger row is the only thing stopping v4's next boot on a shared instance
//!   from re-seeding every pair that gained an outfit since, so the stamp is
//!   mandatory (`itemsAffected` = entries seeded, `Seeded N outfit record(s)
//!   across M chat(s)` with M every chat whose `equippedOutfit` is NON-NULL —
//!   `'nope'`, `'[]'` and `'{}'` cells included).
//!
//! **Failure (R-2).** The table and column steps are FATAL (`Err`, like every
//! table ensure in the chain). The seed is NON-fatal — the P4.D255 R-D shape:
//! a failure rolls back its savepoint, logs ONE v5 ERROR with the bare SQLite
//! text, stamps NOTHING and lets the boot continue
//! ([`HistorySeedOutcome::Failed`]); v4's seed is not `resumable` (a failure
//! stops v4's boot) — the recorded divergence class shared with the wear seed.
//! **R-3:** v4 dates a seeded row by `chats.updatedAt`, so a NULL `updatedAt`
//! violates `"createdAt" TEXT NOT NULL` and fails the WHOLE seed; v5 ports
//! that faithfully (unreachable on a `chats` table generateDDL made — its
//! `updatedAt` is `NOT NULL`).
//!
//! ## NO-PORT (recorded)
//!
//! v4's migration INFO / ERROR lines (`Created chat_wardrobe_history table`,
//! `Added actorToolName column to chat_wardrobe_history`, `Seeded the chat
//! wardrobe history from current outfits`, the three `Failed to …` lines under
//! `migration.<id>` contexts), the runner's `reportProgress` per chat and
//! `addColumnIfMissing`'s DEBUG `Adding column` belong to the deferred runner;
//! v5 logs its own boot lines (the host). `lib/startup/prettify.ts:159-161`'s
//! loading-screen labels, verbatim:
//!
//! - `'add-chat-wardrobe-history-table-v1': 'Binding a ledger of every change of clothes'`
//! - `'seed-chat-wardrobe-history-v1': 'Noting down what everyone has on at the moment'`
//! - `'add-chat-wardrobe-history-tool-name-v1': 'Ruling a column for the contraption that did the dressing'`

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection};
use serde_json::Value;

use super::chat_wardrobe_history::{
    CHAT_WARDROBE_HISTORY_DDL, CHAT_WARDROBE_HISTORY_INSERT_SQL, CHAT_WARDROBE_HISTORY_TABLE,
    CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN,
};
use super::schema_probe::{add_column_if_missing, column_exists, column_names, in_savepoint};
use super::wardrobe_wear_stats_repair::{js_entries, ledger_has, stamp, table_exists};
use super::DbError;
use crate::wardrobe::{hash_equipped_slots, normalize_equipped_slots, Slots};

/// v4's table-creation migration id.
pub const TABLE_MIGRATION_ID: &str = "add-chat-wardrobe-history-table-v1";
/// v4's `actorToolName` column migration id.
pub const TOOL_NAME_MIGRATION_ID: &str = "add-chat-wardrobe-history-tool-name-v1";
/// v4's seed migration id.
pub const SEED_MIGRATION_ID: &str = "seed-chat-wardrobe-history-v1";

/// The savepoint each step's work + stamp runs under (R-4).
const SAVEPOINT: &str = "wardrobe_history";

/// What [`run_chat_wardrobe_history_ensures`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatWardrobeHistoryEnsureOutcome {
    /// The table step created the table (and stamped it).
    pub table_created: bool,
    /// The column step ALTERed a pre-column table (and stamped it).
    pub column_added: bool,
    /// The seed step.
    pub seed: HistorySeedOutcome,
}

/// The seed step's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistorySeedOutcome {
    /// Either app already recorded `seed-chat-wardrobe-history-v1`.
    AlreadyCompleted,
    /// v4's `shouldRun` was false — nothing written, nothing stamped.
    NotApplicable,
    /// The seed ran and was stamped.
    Seeded {
        entries: usize,
        chats_scanned: usize,
        pairs_skipped: usize,
    },
    /// The seed failed and rolled back — logged, NOT stamped, retried next boot.
    Failed,
}

/// Run the three ensures in v4's registration order (table → `actorToolName`
/// column → seed), ONE `now` for the call. `Err` for a failure in the table or
/// column step (or the ledger tables); a failed SEED is
/// [`HistorySeedOutcome::Failed`], never an `Err`. Logs only the seed's ERROR —
/// the boot INFOs are the host's.
pub fn run_chat_wardrobe_history_ensures(
    main: &Connection,
) -> Result<ChatWardrobeHistoryEnsureOutcome, DbError> {
    super::migrations_ledger::ensure_migrations_tables(main)?;
    let now = crate::clock::now_iso();

    // Step 1 — the table.
    let mut table_created = false;
    if !ledger_has(main, TABLE_MIGRATION_ID)? && !table_exists(main, CHAT_WARDROBE_HISTORY_TABLE)? {
        in_savepoint(main, SAVEPOINT, |conn| {
            for sql in CHAT_WARDROBE_HISTORY_DDL {
                conn.execute_batch(sql)?;
            }
            stamp(
                conn,
                TABLE_MIGRATION_ID,
                &now,
                1,
                "Created chat_wardrobe_history table",
            )
        })?;
        table_created = true;
    }

    // Step 2 — the `actorToolName` column (the pre-column vintage only).
    let mut column_added = false;
    if !ledger_has(main, TOOL_NAME_MIGRATION_ID)?
        && table_exists(main, CHAT_WARDROBE_HISTORY_TABLE)?
        && !column_exists(
            main,
            CHAT_WARDROBE_HISTORY_TABLE,
            CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN,
        )?
    {
        column_added = in_savepoint(main, SAVEPOINT, |conn| {
            let added = add_column_if_missing(
                conn,
                CHAT_WARDROBE_HISTORY_TABLE,
                CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN,
                "TEXT",
            )?;
            stamp(
                conn,
                TOOL_NAME_MIGRATION_ID,
                &now,
                i64::from(added),
                "Added actorToolName column to chat_wardrobe_history",
            )?;
            Ok(added)
        })?;
    }

    // Step 3 — the seed.
    let seed = if ledger_has(main, SEED_MIGRATION_ID)? {
        HistorySeedOutcome::AlreadyCompleted
    } else if !seed_should_run(main)? {
        HistorySeedOutcome::NotApplicable
    } else {
        let seeded = in_savepoint(main, SAVEPOINT, |conn| {
            let (entries, chats_scanned, pairs_skipped) = seed(conn)?;
            stamp(
                conn,
                SEED_MIGRATION_ID,
                &now,
                entries as i64,
                &format!("Seeded {entries} outfit record(s) across {chats_scanned} chat(s)"),
            )?;
            Ok((entries, chats_scanned, pairs_skipped))
        });
        match seeded {
            Ok((entries, chats_scanned, pairs_skipped)) => HistorySeedOutcome::Seeded {
                entries,
                chats_scanned,
                pairs_skipped,
            },
            Err(error) => {
                tracing::error!(
                    target: "quilltap::boot",
                    migrationId = SEED_MIGRATION_ID,
                    error = %super::fallback::error_text(&error),
                    "Failed to seed the chat wardrobe history; it will be retried next boot"
                );
                HistorySeedOutcome::Failed
            }
        }
    };

    Ok(ChatWardrobeHistoryEnsureOutcome {
        table_created,
        column_added,
        seed,
    })
}

/// v4 `seed-chat-wardrobe-history-v1`'s `shouldRun` (`:75-80`): the history
/// table and `chats` exist, and `chats` has BOTH `equippedOutfit` and
/// `updatedAt`.
fn seed_should_run(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, CHAT_WARDROBE_HISTORY_TABLE)? || !table_exists(main, "chats")? {
        return Ok(false);
    }
    let columns = column_names(main, "chats")?;
    Ok(columns.iter().any(|c| c == "equippedOutfit") && columns.iter().any(|c| c == "updatedAt"))
}

/// The seed's body (`:82-126`, inside the caller's savepoint): answers
/// `(entries, chats_scanned, pairs_skipped)`.
fn seed(conn: &Connection) -> Result<(usize, usize, usize), DbError> {
    // Read up front, as v4 does (better-sqlite3 cannot write mid-iterate). NO
    // `ORDER BY` — rowid scan order.
    let chats: Vec<(String, Option<String>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT \"id\", \"updatedAt\", \"equippedOutfit\" FROM \"chats\" WHERE \"equippedOutfit\" IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            let outfit = match r.get_ref(2)? {
                ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
                // A number parses to a non-object in v4 → nothing; a BLOB is
                // a recorded, unplanted class (v4's `JSON.parse(Buffer)`).
                _ => None,
            };
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                outfit,
            ))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let mut has_rows = conn.prepare(&format!(
        "SELECT 1 FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 LIMIT 1"
    ))?;
    let mut insert = conn.prepare(CHAT_WARDROBE_HISTORY_INSERT_SQL)?;
    let mut seeded = 0usize;
    let mut skipped = 0usize;
    for (chat_id, updated_at, outfit) in &chats {
        for (character_id, slots) in outfits_from_equipped_outfit(outfit.as_deref()) {
            if has_rows.exists(params![chat_id, character_id])? {
                skipped += 1;
                continue;
            }
            insert.execute(params![
                uuid::Uuid::new_v4().to_string(),
                chat_id,
                character_id,
                0i64,
                slots.to_value().to_string(),
                hash_equipped_slots(Some(&slots)),
                "seeded",
                Option::<String>::None,
                Option::<String>::None,
                1i64,
                // The chat's `updatedAt` — possibly NULL, which `"createdAt"
                // TEXT NOT NULL` refuses (v4's failure, the whole seed; R-3).
                updated_at,
            ])?;
            seeded += 1;
        }
    }
    Ok((seeded, chats.len(), skipped))
}

/// v4 `outfitsFromEquippedOutfit(rawOutfit)` (`seed-chat-wardrobe-history-v1.
/// ts:48-66`) — the (character, slots) pairs one chat's `equippedOutfit` map
/// holds. `None` / empty → none; text `JSON.parse` refuses → none; a parse
/// that is falsy, not an object, or an array → none. Characters in
/// `Object.entries` order (array-index keys first); an empty character key
/// skipped; EVERY value — `null`, a number, a malformed bag included —
/// through `normalizeEquippedSlots`, so it seeds an EMPTY entry, never a skip.
pub fn outfits_from_equipped_outfit(raw: Option<&str>) -> Vec<(String, Slots)> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Vec::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    js_entries(&map)
        .into_iter()
        .filter(|(character_id, _)| !character_id.is_empty())
        .map(|(character_id, slots)| {
            (
                character_id.to_string(),
                Slots::from_value(Some(&normalize_equipped_slots(Some(slots)))),
            )
        })
        .collect()
}

/// The TABLE at its final shape when absent — never the column step, never the
/// seed, never a `migrations_state` stamp. For a harness family reading a
/// pre-round fixture as a booted instance would (wrapped by
/// `test_support::ensure_chat_wardrobe_history_on`).
pub fn ensure_chat_wardrobe_history_table_only(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, CHAT_WARDROBE_HISTORY_TABLE)? {
        for sql in CHAT_WARDROBE_HISTORY_DDL {
            main.execute_batch(sql)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::chat_wardrobe_history::ChatWardrobeHistoryRepository;
    use crate::test_support::captured_with;

    const CHATS: &str = "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \"updatedAt\" TEXT, \"equippedOutfit\" TEXT);";

    /// The `83387d0e5` pre-column table text (`:18-29`), byte-exact.
    const PRE_COLUMN_TABLE: &str = "CREATE TABLE IF NOT EXISTS \"chat_wardrobe_history\" (\n    \"id\" TEXT PRIMARY KEY,\n    \"chatId\" TEXT NOT NULL REFERENCES \"chats\"(\"id\") ON DELETE CASCADE,\n    \"characterId\" TEXT NOT NULL,\n    \"entryIndex\" INTEGER NOT NULL,\n    \"slots\" TEXT NOT NULL,\n    \"slotsHash\" TEXT NOT NULL,\n    \"source\" TEXT NOT NULL,\n    \"actorCharacterId\" TEXT,\n    \"isCurrent\" INTEGER NOT NULL DEFAULT 0,\n    \"createdAt\" TEXT NOT NULL\n  )";

    fn instance(chats: &[(&str, Option<&str>, Option<&str>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CHATS).unwrap();
        for (id, updated, outfit) in chats {
            conn.execute(
                "INSERT INTO chats (id, updatedAt, equippedOutfit) VALUES (?1, ?2, ?3)",
                params![id, updated, outfit],
            )
            .unwrap();
        }
        conn
    }

    fn ledger_rows(conn: &Connection) -> Vec<(String, i64, String)> {
        let mut stmt = conn
            .prepare("SELECT id, itemsAffected, message FROM migrations_state ORDER BY rowid")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn table_sql(conn: &Connection) -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_wardrobe_history'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn plant_ledger(conn: &Connection, id: &str) {
        crate::db::migrations_ledger::ensure_migrations_tables(conn).unwrap();
        conn.execute(
            "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
             VALUES (?1, 'x', '4.11.0', 1, 'planted')",
            [id],
        )
        .unwrap();
    }

    /// v4's `seeds one current entry 0 per (chat, character), dated by the
    /// chat` (`chat-wardrobe-history.test.ts:108`) through the whole chain on
    /// a mode-A instance: the table + the seed stamped, the column step "not
    /// needed" and unrecorded.
    #[test]
    fn mode_a_creates_seeds_and_stamps_table_and_seed_only() {
        let conn = instance(&[
            (
                "chat-1",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"]},"B":{}}"#),
            ),
            ("chat-2", Some("2026-02-01T00:00:00.000Z"), Some("nope")),
            ("chat-3", Some("2026-03-01T00:00:00.000Z"), None),
        ]);
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(
            outcome,
            ChatWardrobeHistoryEnsureOutcome {
                table_created: true,
                column_added: false,
                seed: HistorySeedOutcome::Seeded {
                    entries: 2,
                    chats_scanned: 2,
                    pairs_skipped: 0
                }
            }
        );
        assert_eq!(
            ledger_rows(&conn),
            vec![
                (
                    TABLE_MIGRATION_ID.into(),
                    1,
                    "Created chat_wardrobe_history table".into()
                ),
                (
                    SEED_MIGRATION_ID.into(),
                    2,
                    "Seeded 2 outfit record(s) across 2 chat(s)".into()
                ),
            ]
        );
        assert_eq!(
            table_sql(&conn),
            CHAT_WARDROBE_HISTORY_DDL[0].replacen(" IF NOT EXISTS", "", 1)
        );
        let rows = ChatWardrobeHistoryRepository::new(&conn)
            .find_by_chat_id("chat-1")
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].character_id, "A");
        assert_eq!(rows[0].slots.top, vec!["coat".to_string()]);
        assert_eq!(rows[0].created_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(rows[0].source, "seeded");
        assert!(rows[0].is_current);
        assert_eq!(rows[0].actor_character_id, None);
        assert_eq!(rows[0].actor_tool_name, None);
        assert_eq!(rows[1].character_id, "B");
        assert_eq!(rows[1].slots, Slots::fresh());
        let stored: String = conn
            .query_row(
                "SELECT slots FROM chat_wardrobe_history WHERE characterId = 'A'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored,
            r#"{"top":["coat"],"bottom":[],"footwear":[],"accessories":[],"hair":[]}"#
        );

        // The second run is an exact no-op.
        let again = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(
            again,
            ChatWardrobeHistoryEnsureOutcome {
                table_created: false,
                column_added: false,
                seed: HistorySeedOutcome::AlreadyCompleted
            }
        );
        assert_eq!(ledger_rows(&conn).len(), 2);
    }

    /// v4's `skips a pair that already has rows` (`:148`).
    #[test]
    fn the_seed_skips_a_pair_that_already_has_rows() {
        let conn = instance(&[(
            "chat-1",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]},"B":{"top":["hat"]}}"#),
        )]);
        ensure_chat_wardrobe_history_table_only(&conn).unwrap();
        let repo = ChatWardrobeHistoryRepository::new(&conn);
        let prior = repo.upsert_raw_rows(&[serde_json::json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "chatId": "chat-1",
            "characterId": "A",
            "entryIndex": 0,
            "slots": {"top": ["old"]},
            "source": "operator",
            "actorCharacterId": null,
            "isCurrent": true,
            "createdAt": "2025-01-01T00:00:00.000Z"
        })]);
        assert_eq!(prior, 1);
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(
            outcome.seed,
            HistorySeedOutcome::Seeded {
                entries: 1,
                chats_scanned: 1,
                pairs_skipped: 1
            }
        );
        assert!(!outcome.table_created);
        assert_eq!(
            ledger_rows(&conn),
            vec![(
                SEED_MIGRATION_ID.into(),
                1,
                "Seeded 1 outfit record(s) across 1 chat(s)".into()
            )]
        );
        let a = repo.find_entries("chat-1", "A").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].slots.top, vec!["old".to_string()]);
    }

    /// v4's `adds actorToolName to a table created before it, keeping its
    /// rows` (`:157`) — mode B: the column step stamped `1`, the ALTERed text.
    #[test]
    fn mode_b_alters_the_pre_column_table_and_stamps_it() {
        let conn = instance(&[(
            "chat-1",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        conn.execute_batch(PRE_COLUMN_TABLE).unwrap();
        for sql in &CHAT_WARDROBE_HISTORY_DDL[1..] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute(
            "INSERT INTO chat_wardrobe_history (id, chatId, characterId, entryIndex, slots, slotsHash, source, actorCharacterId, isCurrent, createdAt) \
             VALUES ('r1', 'chat-1', 'A', 0, '{}', 'x', 'pascal', 'A', 1, 't')",
            [],
        )
        .unwrap();
        // Before the ALTER the read sees no column: `actorToolName` reads null.
        let before = ChatWardrobeHistoryRepository::new(&conn)
            .find_all()
            .unwrap();
        assert_eq!(before[0].actor_tool_name, None);
        plant_ledger(&conn, TABLE_MIGRATION_ID);
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(
            outcome,
            ChatWardrobeHistoryEnsureOutcome {
                table_created: false,
                column_added: true,
                seed: HistorySeedOutcome::Seeded {
                    entries: 0,
                    chats_scanned: 1,
                    pairs_skipped: 1
                }
            }
        );
        assert_eq!(
            table_sql(&conn),
            "CREATE TABLE \"chat_wardrobe_history\" (\n    \"id\" TEXT PRIMARY KEY,\n    \"chatId\" TEXT NOT NULL REFERENCES \"chats\"(\"id\") ON DELETE CASCADE,\n    \"characterId\" TEXT NOT NULL,\n    \"entryIndex\" INTEGER NOT NULL,\n    \"slots\" TEXT NOT NULL,\n    \"slotsHash\" TEXT NOT NULL,\n    \"source\" TEXT NOT NULL,\n    \"actorCharacterId\" TEXT,\n    \"isCurrent\" INTEGER NOT NULL DEFAULT 0,\n    \"createdAt\" TEXT NOT NULL\n  , \"actorToolName\" TEXT)"
        );
        let ledger = ledger_rows(&conn);
        assert_eq!(
            ledger[1],
            (
                TOOL_NAME_MIGRATION_ID.into(),
                1,
                "Added actorToolName column to chat_wardrobe_history".into()
            )
        );
        let rows = ChatWardrobeHistoryRepository::new(&conn)
            .find_all()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].actor_tool_name, None);
    }

    /// v4's `has nothing to add to a fresh table` (`:173`) — mode D: the
    /// table present, unledgered — neither the table nor the column step
    /// runs or stamps; the seed does.
    #[test]
    fn mode_d_a_present_final_table_seeds_only() {
        let conn = instance(&[(
            "chat-1",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        ensure_chat_wardrobe_history_table_only(&conn).unwrap();
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert!(!outcome.table_created && !outcome.column_added);
        assert_eq!(
            ledger_rows(&conn),
            vec![(
                SEED_MIGRATION_ID.into(),
                1,
                "Seeded 1 outfit record(s) across 1 chat(s)".into()
            )]
        );
    }

    /// Each step is skipped when either app recorded it: v4 ran all three
    /// (its ledger rows planted) — nothing written.
    #[test]
    fn every_step_skips_its_own_ledger_row() {
        let conn = instance(&[(
            "chat-1",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        for id in [
            TABLE_MIGRATION_ID,
            TOOL_NAME_MIGRATION_ID,
            SEED_MIGRATION_ID,
        ] {
            plant_ledger(&conn, id);
        }
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(
            outcome,
            ChatWardrobeHistoryEnsureOutcome {
                table_created: false,
                column_added: false,
                seed: HistorySeedOutcome::AlreadyCompleted
            }
        );
        assert!(!table_exists(&conn, CHAT_WARDROBE_HISTORY_TABLE).unwrap());
        assert_eq!(ledger_rows(&conn).len(), 3);
    }

    /// `chats` without the two columns: the seed's `shouldRun` is false.
    #[test]
    fn chats_without_the_columns_is_not_applicable() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE chats (id TEXT PRIMARY KEY, updatedAt TEXT);")
            .unwrap();
        let outcome = run_chat_wardrobe_history_ensures(&conn).unwrap();
        assert_eq!(outcome.seed, HistorySeedOutcome::NotApplicable);
        assert_eq!(ledger_rows(&conn).len(), 1);
    }

    /// R-2 / R-3: a NULL `updatedAt` fails the WHOLE seed — rolled back, ONE
    /// ERROR, unstamped, the table intact; the next run retries (and fails
    /// again with the same line). Silence leg: a clean run logs nothing.
    #[test]
    fn a_null_updated_at_fails_the_whole_seed_unstamped() {
        let conn = instance(&[
            (
                "ok",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"]}}"#),
            ),
            ("bad", None, Some(r#"{"A":{"top":["hat"]}}"#)),
        ]);
        let line = format!(
            "ERROR quilltap::boot Failed to seed the chat wardrobe history; it will be retried next boot migrationId={SEED_MIGRATION_ID} error=NOT NULL constraint failed: chat_wardrobe_history.createdAt"
        );
        for _ in 0..2 {
            let (outcome, lines) =
                captured_with(|| run_chat_wardrobe_history_ensures(&conn).unwrap());
            assert_eq!(outcome.seed, HistorySeedOutcome::Failed);
            assert_eq!(lines, vec![line.clone()]);
            assert!(table_exists(&conn, CHAT_WARDROBE_HISTORY_TABLE).unwrap());
            assert!(ChatWardrobeHistoryRepository::new(&conn)
                .find_all()
                .unwrap()
                .is_empty());
            assert_eq!(
                ledger_rows(&conn)
                    .iter()
                    .map(|r| r.0.as_str())
                    .collect::<Vec<_>>(),
                vec![TABLE_MIGRATION_ID]
            );
        }
        conn.execute(
            "UPDATE chats SET updatedAt = '2026-01-02T00:00:00.000Z' WHERE id = 'bad'",
            [],
        )
        .unwrap();
        let (outcome, lines) = captured_with(|| run_chat_wardrobe_history_ensures(&conn).unwrap());
        assert_eq!(
            outcome.seed,
            HistorySeedOutcome::Seeded {
                entries: 2,
                chats_scanned: 2,
                pairs_skipped: 0
            }
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// v4's `reads legacy and malformed outfit maps` (`:178-185`) plus the
    /// corpus edges: a null / numeric / malformed VALUE seeds an EMPTY entry;
    /// an empty key is skipped; array-index keys come first.
    #[test]
    fn outfits_from_equipped_outfit_rules() {
        let o = |raw: Option<&str>| {
            outfits_from_equipped_outfit(raw)
                .into_iter()
                .map(|(c, s)| (c, s.to_value().to_string()))
                .collect::<Vec<_>>()
        };
        let empty = Slots::fresh().to_value().to_string();
        let pair = |c: &str, s: &str| (c.to_string(), s.to_string());
        assert_eq!(
            o(Some(r#"{"A":{"top":["x"]}}"#)),
            vec![pair(
                "A",
                r#"{"top":["x"],"bottom":[],"footwear":[],"accessories":[],"hair":[]}"#
            )]
        );
        assert!(o(None).is_empty());
        assert!(o(Some("")).is_empty());
        assert!(o(Some("[]")).is_empty());
        assert!(o(Some("nope")).is_empty());
        assert!(o(Some("null")).is_empty());
        assert!(o(Some("5")).is_empty());
        assert_eq!(o(Some(r#"{"c": null}"#)), vec![pair("c", &empty)]);
        assert_eq!(o(Some(r#"{"c": 5}"#)), vec![pair("c", &empty)]);
        assert_eq!(o(Some(r#"{"c": {"top": "x"}}"#)), vec![pair("c", &empty)]);
        assert!(o(Some(r#"{"": {"top": ["x"]}}"#)).is_empty());
        assert_eq!(
            o(Some(r#"{"1": {}, "b": {}, "0": {}}"#))
                .into_iter()
                .map(|(c, _)| c)
                .collect::<Vec<_>>(),
            vec!["0", "1", "b"]
        );
    }

    #[test]
    fn table_only_creates_without_seeding_or_stamping() {
        let conn = instance(&[(
            "c",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        ensure_chat_wardrobe_history_table_only(&conn).unwrap();
        ensure_chat_wardrobe_history_table_only(&conn).unwrap();
        assert!(table_exists(&conn, CHAT_WARDROBE_HISTORY_TABLE).unwrap());
        assert!(ChatWardrobeHistoryRepository::new(&conn)
            .find_all()
            .unwrap()
            .is_empty());
        assert!(!table_exists(&conn, "migrations_state").unwrap());
    }
}
