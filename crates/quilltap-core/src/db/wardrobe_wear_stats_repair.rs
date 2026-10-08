//! The wardrobe wear ledger's boot ensure (v4 `3ee3b1342`, "Wardrobe wear
//! ledger (#81)" — migrations `add-wardrobe-wear-stats-table-v1`
//! (`dependsOn: ['sqlite-initial-schema-v1']`) and `seed-wardrobe-wear-stats-v1`
//! (`dependsOn: ['add-wardrobe-wear-stats-table-v1']`), both `introducedInVersion
//! '4.10.0'`, registered after `impersonation-voice-mode-v1`; P4.D255).
//!
//! v5's migration runner is a locked deferral, so — the P4.d7 / P4.D63 /
//! P4.D249 / P4.D251 precedents — both migrations are re-homed as ONE boot
//! ensure over the main partition, run from the host's `seed_built_ins`
//! directly before the P4.160 index-family backfill.
//!
//! ## Step 1 — the table (`add-wardrobe-wear-stats-table-v1`)
//!
//! v4's `shouldRun` is `!sqliteTableExists('wardrobe_wear_stats')`; `run`
//! executes [`WARDROBE_WEAR_STATS_DDL`]'s three statements verbatim (the table
//! with `"wearCount" INTEGER NOT NULL DEFAULT 0` + the UNIQUE `COALESCE` index
//! every upsert's `ON CONFLICT` targets + the wearer index) and the runner
//! records `itemsAffected 1`, `Created wardrobe_wear_stats table`. This step
//! does the same in one savepoint and STAMPS that row. A table already present
//! (a fresh v5 instance provisions it in generateDDL's shape — the human's
//! 2026-10-08 ruling over the order's R-A; or v4 made it) is left alone and
//! NOTHING is stamped — v4 records only a migration that RAN.
//!
//! v4's REPOSITORY then adds generateDDL's `idx_wardrobe_wear_stats_createdAt`
//! on its first touch (`ensureCollection`, `CREATE INDEX IF NOT EXISTS`). v5
//! has no lazy collection init, so this step re-homes that touch too, with the
//! statement read from `fresh_schema.json` (D23 — never spelled here), whenever
//! the table exists.
//!
//! ## Step 2 — the seed (`seed-wardrobe-wear-stats-v1`)
//!
//! **R-B (the cross-app handshake).** v4's runner checks `migrations_state`
//! BEFORE `shouldRun` (`migrations/index.ts:144-147, :164`), and the seed's
//! `shouldRun` is TRUE whenever the table and the two `chats` columns exist —
//! it has no once-only gate of its own ("Idempotent in effect only if run once
//! — it is, by the migration ledger", its header). A v5 that seeded WITHOUT
//! stamping would make v4's next boot on a shared instance credit every
//! current outfit TWICE. So this step skips when EITHER app wrote
//! `seed-wardrobe-wear-stats-v1` (the `thinking_prefill_retire_heal` shape),
//! and stamps it in `recordCompletedMigration`'s row shape when it runs —
//! the first stamping ensure since P4.D63, justified by that hazard. A fresh
//! instance seeds too: v4's real first boot (measured at the `f5e953a3f` pin)
//! runs the seed over zero chats and records `Credited 0 wear(s) across 0
//! chat(s)`.
//!
//! **R-D (the algorithm, exactly).** `SELECT "id", "updatedAt",
//! "equippedOutfit" FROM "chats" WHERE "equippedOutfit" IS NOT NULL` with NO
//! `ORDER BY` (rowid scan order), ONE `now`, ONE savepoint; per chat,
//! [`wears_from_equipped_outfit`] → one [`WARDROBE_WEAR_INCREMENT_SQL`] per
//! (character × distinct item), dated by the chat's `updatedAt` for both first
//! and last worn, last worn in that chat — the increment's `>=` tie-break
//! gives a tie to the LATER-scanned chat. Whole composite ids left in legacy
//! rows are credited as themselves; ids that no longer resolve make orphan
//! rows nothing prunes (v4's header). A failed seed (a chat whose `updatedAt`
//! is NULL violates `firstWornAt NOT NULL`; a hand-damaged table without its
//! UNIQUE index refuses the `ON CONFLICT`) ROLLS BACK, logs ONE v5-side ERROR
//! with the bare SQLite text, stamps NOTHING and lets the boot continue — v4's
//! failed-migration shape (unrecorded, retried next boot).
//!
//! ## NO-PORT (the fifth recording of the deferred runner)
//!
//! v4's migration INFO lines (`Created wardrobe_wear_stats table`, `Seeded the
//! wardrobe wear ledger from current outfits`), their ERROR lines (`Failed to
//! create …` / `Failed to seed …` under `migration.<id>` contexts), the
//! runner's `reportProgress` per chat, and `lib/startup/prettify.ts:154-155`'s
//! labels ("Ruling a ledger for who has worn what", "Taking stock of what the
//! cast is wearing this minute") belong to the runner — P4.D251 item 16 is the
//! precedent. The ledger ROW is ported (R-B); the lines are not.

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection};
use serde_json::Value;

use super::wardrobe_wear_stats::{
    run_atomically, WARDROBE_WEAR_INCREMENT_SQL, WARDROBE_WEAR_STATS_DDL, WARDROBE_WEAR_STATS_TABLE,
};
use super::DbError;
use crate::wardrobe::Slots;

/// v4's table-creation migration id.
pub const TABLE_MIGRATION_ID: &str = "add-wardrobe-wear-stats-table-v1";
/// v4's seed migration id.
pub const SEED_MIGRATION_ID: &str = "seed-wardrobe-wear-stats-v1";
/// The `idx_<table>_createdAt` index v4's repository adds on first touch.
const CREATED_AT_INDEX: &str = "idx_wardrobe_wear_stats_createdAt";

/// What [`ensure_wardrobe_wear_stats`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WearStatsEnsureOutcome {
    /// Step 1 created the table (and stamped `add-wardrobe-wear-stats-table-v1`).
    pub table_created: bool,
    /// Step 2.
    pub seed: SeedOutcome,
}

/// The seed step's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedOutcome {
    /// Either app already recorded `seed-wardrobe-wear-stats-v1`.
    AlreadyCompleted,
    /// v4's `shouldRun` was false (no table, no `chats`, or `chats` lacks
    /// `equippedOutfit` / `updatedAt`) — nothing written, nothing stamped,
    /// retried next boot.
    NotApplicable,
    /// The seed ran and was stamped: `wears` credited across
    /// `chats_with_outfits` chats.
    Seeded {
        wears: usize,
        chats_with_outfits: usize,
    },
    /// The seed failed and rolled back — logged, NOT stamped, retried next boot.
    Failed,
}

/// Create (when absent) and seed (once per instance) the wear ledger.
///
/// `Err` only for a failure outside the seed's own savepoint (the ledger
/// tables, the table step, a stamp); a failed SEED is
/// [`SeedOutcome::Failed`], never an `Err`.
pub fn ensure_wardrobe_wear_stats(main: &Connection) -> Result<WearStatsEnsureOutcome, DbError> {
    super::migrations_ledger::ensure_migrations_tables(main)?;
    let now = crate::clock::now_iso();

    // Step 1 — v4's runner: completed? then `shouldRun` (table absent).
    let mut table_created = false;
    if !ledger_has(main, TABLE_MIGRATION_ID)? && !table_exists(main, WARDROBE_WEAR_STATS_TABLE)? {
        run_atomically(main, |conn| {
            for sql in WARDROBE_WEAR_STATS_DDL {
                conn.execute_batch(sql)?;
            }
            stamp(
                conn,
                TABLE_MIGRATION_ID,
                &now,
                1,
                "Created wardrobe_wear_stats table",
            )
        })?;
        table_created = true;
    }
    // v4's repository first touch: generateDDL's createdAt index.
    if table_exists(main, WARDROBE_WEAR_STATS_TABLE)? {
        let sql = crate::services::provisioning::fresh_main_index(CREATED_AT_INDEX)
            .map_err(|e| DbError::Internal(e.to_string()))?
            .ok_or_else(|| {
                DbError::Internal(format!("fresh_schema.json carries no {CREATED_AT_INDEX}"))
            })?;
        main.execute_batch(&sql.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1))?;
    }

    // Step 2 — completed by either app? then `shouldRun`.
    let seed = if ledger_has(main, SEED_MIGRATION_ID)? {
        SeedOutcome::AlreadyCompleted
    } else if !seed_should_run(main)? {
        SeedOutcome::NotApplicable
    } else {
        let seeded = run_atomically(main, |conn| {
            let (wears, chats_with_outfits) = seed(conn, &now)?;
            stamp(
                conn,
                SEED_MIGRATION_ID,
                &now,
                wears as i64,
                &format!("Credited {wears} wear(s) across {chats_with_outfits} chat(s)"),
            )?;
            Ok((wears, chats_with_outfits))
        });
        match seeded {
            Ok((wears, chats_with_outfits)) => SeedOutcome::Seeded {
                wears,
                chats_with_outfits,
            },
            Err(error) => {
                tracing::error!(
                    target: "quilltap::boot",
                    migrationId = SEED_MIGRATION_ID,
                    error = %super::fallback::error_text(&error),
                    "Failed to seed the wardrobe wear ledger; it will be retried next boot"
                );
                SeedOutcome::Failed
            }
        }
    };

    Ok(WearStatsEnsureOutcome {
        table_created,
        seed,
    })
}

/// v4 `seed-wardrobe-wear-stats-v1`'s `shouldRun`: the table and `chats`
/// exist, and `chats` has BOTH `equippedOutfit` and `updatedAt`.
fn seed_should_run(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, WARDROBE_WEAR_STATS_TABLE)? || !table_exists(main, "chats")? {
        return Ok(false);
    }
    let mut stmt = main.prepare("PRAGMA table_info(\"chats\")")?;
    let columns: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    Ok(columns.iter().any(|c| c == "equippedOutfit") && columns.iter().any(|c| c == "updatedAt"))
}

/// The seed's body (inside the caller's savepoint): answers `(wears,
/// chats_with_outfits)`.
fn seed(conn: &Connection, now: &str) -> Result<(usize, usize), DbError> {
    // Read up front, as v4 does (better-sqlite3 cannot write mid-iterate).
    let chats: Vec<(String, Option<String>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT \"id\", \"updatedAt\", \"equippedOutfit\" FROM \"chats\" WHERE \"equippedOutfit\" IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            let outfit = match r.get_ref(2)? {
                ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
                // A non-text cell parses to a non-object (or not at all) in v4.
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
    let mut increment = conn.prepare(WARDROBE_WEAR_INCREMENT_SQL)?;
    let mut wears = 0usize;
    let mut chats_with_outfits = 0usize;
    for (chat_id, updated_at, outfit) in &chats {
        let chat_wears = wears_from_equipped_outfit(outfit.as_deref());
        if !chat_wears.is_empty() {
            chats_with_outfits += 1;
        }
        for (character_id, item_id) in &chat_wears {
            let id = uuid::Uuid::new_v4().to_string();
            // v4 `wardrobeWearIncrementParams` order; `at` is the chat's
            // `updatedAt` — possibly NULL, which the table's NOT NULL refuses
            // (v4's failure, the whole seed).
            let at = updated_at.as_deref();
            increment.execute(params![
                id,
                item_id,
                character_id,
                at,
                at,
                chat_id,
                now,
                now
            ])?;
            wears += 1;
        }
    }
    Ok((wears, chats_with_outfits))
}

/// v4 `wearsFromEquippedOutfit(rawOutfit)` (`seed-wardrobe-wear-stats-v1.ts:
/// 49-69`) — the wears one chat contributes: one per (character × distinct
/// item id), as `(characterId, itemId)`.
///
/// `None` / empty → none; text `JSON.parse` refuses → none; a parse that is
/// not a plain object (null, a primitive, an array) → none. Characters in
/// `Object.entries` order (array-index keys first, ascending, then insertion
/// order); an empty character key skipped; each character's ids through
/// `allEquippedItemIds(normalizeEquippedSlots(slots))` — slot order top →
/// bottom → footwear → accessories → hair, first seen, deduped (the same id
/// in two slots is ONE wear) — and an empty id skipped.
pub fn wears_from_equipped_outfit(raw: Option<&str>) -> Vec<(String, String)> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Vec::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let mut wears = Vec::new();
    for (character_id, slots) in js_entries(&map) {
        if character_id.is_empty() {
            continue;
        }
        let slots = Slots::from_value(Some(slots));
        for item_id in super::wardrobe_wear_stats::all_equipped_item_ids(&slots) {
            if !item_id.is_empty() {
                wears.push((character_id.to_string(), item_id));
            }
        }
    }
    wears
}

/// `Object.entries` order over a parsed JSON object: the array-index keys
/// (canonical decimal integers below 2³² − 1) first, ascending, then every
/// other key in insertion order (`serde_json`'s `preserve_order`).
fn js_entries(map: &serde_json::Map<String, Value>) -> Vec<(&str, &Value)> {
    let is_index = |k: &str| {
        !k.is_empty()
            && (k == "0" || !k.starts_with('0'))
            && k.bytes().all(|b| b.is_ascii_digit())
            && k.parse::<u64>().is_ok_and(|n| n < u32::MAX as u64)
    };
    let mut indexed: Vec<(&str, &Value)> = map
        .iter()
        .filter(|(k, _)| is_index(k))
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    indexed.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(0));
    indexed.extend(
        map.iter()
            .filter(|(k, _)| !is_index(k))
            .map(|(k, v)| (k.as_str(), v)),
    );
    indexed
}

/// v4's runner's `recordCompletedMigration` (`migrations/state.ts:100-130`):
/// the row `(id, completedAt, quilltapVersion, itemsAffected, message)` and
/// the two metadata keys — the heals' shape (`avatar_rolls_collapse_heal::
/// stamp`). `quilltapVersion` is THIS app's version, as every heal writes.
fn stamp(
    conn: &Connection,
    id: &str,
    now: &str,
    items_affected: i64,
    message: &str,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, now, env!("CARGO_PKG_VERSION"), items_affected, message],
    )?;
    for (k, v) in [
        ("lastChecked", now),
        ("quilltapVersion", env!("CARGO_PKG_VERSION")),
    ] {
        conn.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![k, v],
        )?;
    }
    Ok(())
}

fn ledger_has(conn: &Connection, id: &str) -> Result<bool, DbError> {
    if !table_exists(conn, "migrations_state")? {
        return Ok(false);
    }
    let mut stmt = conn.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
    Ok(stmt.exists([id])?)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

/// The TABLE step alone — never the seed, never a stamp — for a harness
/// family reading a pre-round fixture as a booted instance would (C1 §6;
/// wrapped by `test_support::ensure_wear_ledger_on`).
pub fn ensure_wardrobe_wear_stats_table_only(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, WARDROBE_WEAR_STATS_TABLE)? {
        for sql in WARDROBE_WEAR_STATS_DDL {
            main.execute_batch(sql)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::wardrobe_wear_stats::WardrobeWearStatsRepository;
    use crate::test_support::captured_with;

    const CHATS: &str = "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \"updatedAt\" TEXT, \"equippedOutfit\" TEXT);";

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

    fn master(conn: &Connection) -> Vec<(String, Option<String>)> {
        let mut stmt = conn
            .prepare(
                "SELECT name, sql FROM sqlite_master WHERE tbl_name = 'wardrobe_wear_stats' ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// v4's own spec (`wardrobe-wear-stats.test.ts:93-135`): four chats → 5
    /// wears, 4 rows; coat×A exact.
    #[test]
    fn v4s_four_chat_corpus() {
        let conn = instance(&[
            (
                "chat-1",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat","shirt"],"bottom":["slacks"]},"B":{"top":["coat"]}}"#),
            ),
            (
                "chat-2",
                Some("2026-03-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"],"accessories":["coat"]}}"#),
            ),
            ("chat-3", Some("2026-02-01T00:00:00.000Z"), None),
            ("chat-4", Some("2026-02-01T00:00:00.000Z"), Some("not json")),
        ]);
        let outcome = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert!(outcome.table_created);
        assert_eq!(
            outcome.seed,
            SeedOutcome::Seeded {
                wears: 5,
                chats_with_outfits: 2
            }
        );
        let repo = WardrobeWearStatsRepository::new(&conn);
        let rows = repo.find_all();
        assert_eq!(rows.len(), 4);
        let coat_a = rows
            .iter()
            .find(|r| r.item_id == "coat" && r.wearer_character_id.as_deref() == Some("A"))
            .unwrap();
        assert_eq!(coat_a.wear_count, 2);
        assert_eq!(coat_a.first_worn_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(coat_a.last_worn_at, "2026-03-01T00:00:00.000Z");
        assert_eq!(coat_a.last_worn_chat_id.as_deref(), Some("chat-2"));
        assert_eq!(
            ledger_rows(&conn),
            vec![
                (
                    TABLE_MIGRATION_ID.into(),
                    1,
                    "Created wardrobe_wear_stats table".into()
                ),
                (
                    SEED_MIGRATION_ID.into(),
                    5,
                    "Credited 5 wear(s) across 2 chat(s)".into()
                ),
            ]
        );
        // The table in the migration's text, the two hand indexes + the
        // repository's createdAt index.
        let m = master(&conn);
        assert_eq!(
            m.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec![
                "idx_wardrobe_wear_stats_createdAt",
                "idx_wardrobe_wear_stats_item_wearer",
                "idx_wardrobe_wear_stats_wearer",
                "sqlite_autoindex_wardrobe_wear_stats_1",
                "wardrobe_wear_stats",
            ]
        );
        assert_eq!(
            m[4].1.as_deref(),
            Some(&WARDROBE_WEAR_STATS_DDL[0].replacen(" IF NOT EXISTS", "", 1)[..])
        );

        // A second run is an exact no-op.
        let before = master(&conn);
        let again = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert!(!again.table_created);
        assert_eq!(again.seed, SeedOutcome::AlreadyCompleted);
        assert_eq!(master(&conn), before);
        assert_eq!(repo.find_all().len(), 4);
        assert_eq!(ledger_rows(&conn).len(), 2);
    }

    /// Two chats sharing an item with EQUAL `updatedAt`: the later-scanned
    /// chat (rowid) wins `lastWornChatId`.
    #[test]
    fn an_equal_updated_at_goes_to_the_later_chat() {
        let conn = instance(&[
            (
                "b-first",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"]}}"#),
            ),
            (
                "a-second",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"]}}"#),
            ),
        ]);
        ensure_wardrobe_wear_stats(&conn).unwrap();
        let rows = WardrobeWearStatsRepository::new(&conn).find_all();
        assert_eq!(rows[0].wear_count, 2);
        assert_eq!(rows[0].last_worn_chat_id.as_deref(), Some("a-second"));
    }

    /// v4 already wrote the seed row (it booted the copy first): zero rows
    /// seeded; the table step still runs only if the table is absent.
    #[test]
    fn a_v4_written_seed_row_skips_the_seed() {
        let conn = instance(&[(
            "chat-1",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        crate::db::migrations_ledger::ensure_migrations_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
             VALUES (?1, 'x', '4.10.0', 1, 'Credited 1 wear(s) across 1 chat(s)')",
            [SEED_MIGRATION_ID],
        )
        .unwrap();
        let outcome = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert!(outcome.table_created);
        assert_eq!(outcome.seed, SeedOutcome::AlreadyCompleted);
        assert!(WardrobeWearStatsRepository::new(&conn)
            .find_all()
            .is_empty());
    }

    /// A v4-written TABLE row with no table (unreachable from either app; the
    /// recorded behaviour mirrors v4's runner): no table is created, so the
    /// seed's `shouldRun` is false and nothing is stamped.
    #[test]
    fn a_v4_written_table_row_with_no_table() {
        let conn = instance(&[]);
        crate::db::migrations_ledger::ensure_migrations_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
             VALUES (?1, 'x', '4.10.0', 1, 'Created wardrobe_wear_stats table')",
            [TABLE_MIGRATION_ID],
        )
        .unwrap();
        let outcome = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert_eq!(
            outcome,
            WearStatsEnsureOutcome {
                table_created: false,
                seed: SeedOutcome::NotApplicable
            }
        );
        assert!(!table_exists(&conn, WARDROBE_WEAR_STATS_TABLE).unwrap());
        assert_eq!(ledger_rows(&conn).len(), 1);
    }

    /// A table already present (a fresh v5 instance): no DDL, no table stamp;
    /// the seed runs over zero chats and stamps `Credited 0 …` (v4's measured
    /// fresh boot).
    #[test]
    fn a_present_table_is_left_alone_and_an_empty_seed_stamps() {
        let conn = instance(&[]);
        for sql in WARDROBE_WEAR_STATS_DDL {
            conn.execute_batch(sql).unwrap();
        }
        let outcome = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert_eq!(
            outcome,
            WearStatsEnsureOutcome {
                table_created: false,
                seed: SeedOutcome::Seeded {
                    wears: 0,
                    chats_with_outfits: 0
                }
            }
        );
        assert_eq!(
            ledger_rows(&conn),
            vec![(
                SEED_MIGRATION_ID.into(),
                0,
                "Credited 0 wear(s) across 0 chat(s)".into()
            )]
        );
    }

    /// No `chats.equippedOutfit`: the seed's `shouldRun` is false — nothing
    /// stamped, retried next boot.
    #[test]
    fn chats_without_the_columns_is_not_applicable() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE chats (id TEXT PRIMARY KEY, updatedAt TEXT);")
            .unwrap();
        let outcome = ensure_wardrobe_wear_stats(&conn).unwrap();
        assert_eq!(outcome.seed, SeedOutcome::NotApplicable);
        assert_eq!(ledger_rows(&conn).len(), 1);
    }

    /// A failed seed (a NULL `updatedAt` violates `firstWornAt NOT NULL`)
    /// rolls back whole, logs ONE ERROR, stamps nothing; the next run retries.
    #[test]
    fn a_failed_seed_rolls_back_and_is_not_stamped() {
        let conn = instance(&[
            (
                "ok",
                Some("2026-01-01T00:00:00.000Z"),
                Some(r#"{"A":{"top":["coat"]}}"#),
            ),
            ("bad", None, Some(r#"{"A":{"top":["hat"]}}"#)),
        ]);
        let (outcome, lines) = captured_with(|| ensure_wardrobe_wear_stats(&conn).unwrap());
        assert_eq!(outcome.seed, SeedOutcome::Failed);
        assert_eq!(
            lines,
            vec![format!(
                "ERROR quilltap::boot Failed to seed the wardrobe wear ledger; it will be retried next boot migrationId={SEED_MIGRATION_ID} error=NOT NULL constraint failed: wardrobe_wear_stats.firstWornAt"
            )]
        );
        assert!(WardrobeWearStatsRepository::new(&conn)
            .find_all()
            .is_empty());
        assert_eq!(ledger_rows(&conn).len(), 1);
        // Silence leg: a clean run logs nothing.
        conn.execute(
            "UPDATE chats SET updatedAt = '2026-01-02T00:00:00.000Z' WHERE id = 'bad'",
            [],
        )
        .unwrap();
        let (outcome, lines) = captured_with(|| ensure_wardrobe_wear_stats(&conn).unwrap());
        assert_eq!(
            outcome.seed,
            SeedOutcome::Seeded {
                wears: 2,
                chats_with_outfits: 2
            }
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// A hand-damaged table without its UNIQUE index refuses the `ON
    /// CONFLICT` — the seed fails and is not stamped.
    #[test]
    fn a_table_without_the_unique_index_fails_the_seed() {
        let conn = instance(&[(
            "c",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        conn.execute_batch(WARDROBE_WEAR_STATS_DDL[0]).unwrap();
        let (outcome, lines) = captured_with(|| ensure_wardrobe_wear_stats(&conn).unwrap());
        assert_eq!(outcome.seed, SeedOutcome::Failed);
        assert!(
            lines[0].contains(
                "error=ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint"
            ),
            "{lines:?}"
        );
    }

    /// v4's `wearsFromEquippedOutfit` spec rows (`:137-143`) plus the
    /// corpus edges.
    #[test]
    fn wears_from_equipped_outfit_rules() {
        let w = |raw: Option<&str>| wears_from_equipped_outfit(raw);
        let pair = |c: &str, i: &str| (c.to_string(), i.to_string());
        assert_eq!(w(Some(r#"{"A":{"top":["x"]}}"#)), vec![pair("A", "x")]);
        assert!(w(None).is_empty());
        assert!(w(Some("")).is_empty());
        assert!(w(Some("[]")).is_empty());
        assert!(w(Some("null")).is_empty());
        assert!(w(Some("not json")).is_empty());
        assert!(w(Some("5")).is_empty());
        // An empty character key; an empty id; the same id in two slots;
        // a non-array slot; a non-string element; hair last.
        assert_eq!(
            w(Some(
                r#"{"":{"top":["z"]},"B":{"hair":["h"],"top":["coat",""],"accessories":["coat",3],"bottom":"x"}}"#
            )),
            vec![pair("B", "coat"), pair("B", "h")]
        );
        // `Object.entries` order: array-index keys first, ascending.
        assert_eq!(
            w(Some(
                r#"{"b":{"top":["1"]},"10":{"top":["2"]},"2":{"top":["3"]},"01":{"top":["4"]}}"#
            )),
            vec![
                pair("2", "3"),
                pair("10", "2"),
                pair("b", "1"),
                pair("01", "4")
            ]
        );
    }

    #[test]
    fn table_only_creates_without_seeding_or_stamping() {
        let conn = instance(&[(
            "c",
            Some("2026-01-01T00:00:00.000Z"),
            Some(r#"{"A":{"top":["coat"]}}"#),
        )]);
        ensure_wardrobe_wear_stats_table_only(&conn).unwrap();
        ensure_wardrobe_wear_stats_table_only(&conn).unwrap();
        assert!(table_exists(&conn, WARDROBE_WEAR_STATS_TABLE).unwrap());
        assert!(WardrobeWearStatsRepository::new(&conn)
            .find_all()
            .is_empty());
        assert!(!table_exists(&conn, "migrations_state").unwrap());
    }
}
