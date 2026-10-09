//! The wardrobe wear ledger's DATA layer — v4 `lib/database/repositories/
//! wardrobe-wear.repository.ts` + `lib/database/backends/sqlite/
//! wardrobe-wear-stats-ddl.ts` + `lib/schemas/wardrobe-wear.types.ts`
//! (`3ee3b1342`, "Wardrobe wear ledger (#81)", unchanged through `f5e953a3f`;
//! P4.D255).
//!
//! Backs `wardrobe_wear_stats`: per (wardrobe item × wearer) a wear count, the
//! first and last time it was worn, and the chat it was last worn in. Totals
//! are sums over wearers; there is no per-event log. v4's why, carried: the
//! tally lives here rather than in the item's frontmatter because items are
//! vault files and every item write re-projects the whole `Wardrobe/` folder
//! and bumps `updatedAt` — a counter there would rewrite thirty files to count
//! one wear and make "last edited" mean "last worn". Here an increment is one
//! atomic upsert.
//!
//! **What is NOT here:** v4's chokepoint `commitEquippedOutfit` (the one place
//! a character's equipped slots are written on a "put something on" path) is
//! P4.D262's `services::wardrobe_wear_commit`, built on [`diff_equipped_outfit`]
//! and [`WardrobeWearStatsRepository::increment_wears`]. v4's warning stands
//! for it: never call `increment_wears` from a handler to credit a wear by hand
//! — the handler's idea of "already worn" is stale.
//!
//! **Two table shapes reach these reads** (P4.D255, the human's 2026-10-08
//! ruling over the order's R-A): an instance the boot ensure upgraded carries
//! v4's MIGRATION text (`"wearCount" INTEGER NOT NULL DEFAULT 0`); a fresh v5
//! instance carries generateDDL's (`"wearCount" REAL NOT NULL`, the standing
//! D23 table surface — 40 of v4's 45 tables differ the same way). Both carry
//! the UNIQUE `COALESCE` index (`migration_indexes.json`), which is what every
//! `ON CONFLICT` below needs. A REAL cell reads back `1.0`, so every read maps
//! `wearCount` through [`wear_count_cell`] (v4 normalizes with
//! `Number(row.wearCount)`, `wardrobe-wear.repository.ts:165-167`).
//!
//! **Atomicity.** v4's `runAtomically` is better-sqlite3's `db.transaction`,
//! which nests as a SAVEPOINT when the job applier's `BEGIN IMMEDIATE` is open.
//! [`run_atomically`] is the same: a named SAVEPOINT (which opens a
//! transaction when none is active and nests when one is), released on `Ok`,
//! rolled back on `Err`. `delete_by_item_ids` and `upsert_rows` are NOT
//! wrapped — v4's are not (per chunk / per row through `rawQuery`).
//!
//! **Fallback reads.** `find_summaries` and `find_history` are v4 fallback
//! `safeQuery`s: a failed read (a missing table included) logs v4's ERROR
//! under `quilltap::db` (`collection`, the context key, the BARE SQLite text
//! through [`super::fallback::error_text`]) and answers the never-worn shape.
//! `find_rows_for_items`, `find_rows_for_wearer`, `increment_wears`,
//! `upsert_rows` and `delete_by_item_ids` do NOT fall back in v4 — they
//! propagate (`Result`). `find_all` (v4's inherited `findAll`, the backup's
//! read) answers `[]` on an ABSENT table, as `services::backup::marshal::
//! query_all` does, and otherwise logs v4's `Error finding all entities`
//! through [`super::fallback::find_all_or_empty`].

use std::collections::{HashMap, HashSet};

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use super::DbError;
use crate::chunk::SQLITE_VARIABLE_CHUNK_SIZE;
use crate::wardrobe::Slots;

/// v4 `WARDROBE_WEAR_STATS_TABLE`.
pub const WARDROBE_WEAR_STATS_TABLE: &str = "wardrobe_wear_stats";

/// v4 `WARDROBE_WEAR_STATS_DDL` (`wardrobe-wear-stats-ddl.ts:18-34`) — the
/// three statements, BYTE-exact (whitespace included: SQLite stores the text
/// as written minus `IF NOT EXISTS`, and `sqlite_master.sql` is a comparand).
/// The table-creation ensure ([`super::wardrobe_wear_stats_repair`]) runs them;
/// nothing else spells them. Why the unique index is on
/// `COALESCE("wearerCharacterId", '')` (v4's comment): SQLite treats NULLs as
/// distinct in a plain unique index, so two "unattributed" rows for one item
/// would both be admitted; folding NULL to `''` makes the unattributed row
/// unique per item, which is what lets `ON CONFLICT ("itemId",
/// COALESCE("wearerCharacterId", ''))` upsert it.
pub const WARDROBE_WEAR_STATS_DDL: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS \"wardrobe_wear_stats\" (\n    \"id\" TEXT PRIMARY KEY,\n    \"itemId\" TEXT NOT NULL,\n    \"wearerCharacterId\" TEXT,\n    \"wearCount\" INTEGER NOT NULL DEFAULT 0,\n    \"firstWornAt\" TEXT NOT NULL,\n    \"lastWornAt\" TEXT NOT NULL,\n    \"lastWornChatId\" TEXT,\n    \"createdAt\" TEXT NOT NULL,\n    \"updatedAt\" TEXT NOT NULL\n  )",
    "CREATE UNIQUE INDEX IF NOT EXISTS \"idx_wardrobe_wear_stats_item_wearer\"\n    ON \"wardrobe_wear_stats\" (\"itemId\", COALESCE(\"wearerCharacterId\", ''))",
    "CREATE INDEX IF NOT EXISTS \"idx_wardrobe_wear_stats_wearer\"\n    ON \"wardrobe_wear_stats\" (\"wearerCharacterId\")",
];

/// v4 `WARDROBE_WEAR_INCREMENT_SQL` (`wardrobe-wear-stats-ddl.ts:42-52`),
/// byte-exact (the template literal's leading newline and indentation
/// included). One wear for one (item × wearer): inserts the row at count 1, or
/// bumps an existing row's count and moves its "last worn" forward — never
/// backward, so a replayed or backfilled earlier wear cannot rewind it. Ties
/// (`>=`) move `lastWornChatId` to the LATER-applied wear. Bind with
/// [`increment_params`].
pub const WARDROBE_WEAR_INCREMENT_SQL: &str = "\n  INSERT INTO \"wardrobe_wear_stats\"\n    (\"id\", \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\", \"createdAt\", \"updatedAt\")\n  VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?)\n  ON CONFLICT (\"itemId\", COALESCE(\"wearerCharacterId\", '')) DO UPDATE SET\n    \"wearCount\" = \"wearCount\" + 1,\n    \"firstWornAt\" = MIN(\"firstWornAt\", excluded.\"firstWornAt\"),\n    \"lastWornChatId\" = CASE WHEN excluded.\"lastWornAt\" >= \"lastWornAt\" THEN excluded.\"lastWornChatId\" ELSE \"lastWornChatId\" END,\n    \"lastWornAt\" = MAX(\"lastWornAt\", excluded.\"lastWornAt\"),\n    \"updatedAt\" = excluded.\"updatedAt\"\n";

/// v4 `foldWearerIntoUnattributed`'s upsert (`wardrobe-wear.repository.ts:
/// 338-348`), byte-exact: the wearer's count is ADDED to the item's
/// unattributed row.
const FOLD_UPSERT_SQL: &str = "INSERT INTO \"wardrobe_wear_stats\"\n           (\"id\", \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\", \"createdAt\", \"updatedAt\")\n         VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)\n         ON CONFLICT (\"itemId\", COALESCE(\"wearerCharacterId\", '')) DO UPDATE SET\n           \"wearCount\" = \"wearCount\" + excluded.\"wearCount\",\n           \"firstWornAt\" = MIN(\"firstWornAt\", excluded.\"firstWornAt\"),\n           \"lastWornChatId\" = CASE WHEN excluded.\"lastWornAt\" >= \"lastWornAt\" THEN excluded.\"lastWornChatId\" ELSE \"lastWornChatId\" END,\n           \"lastWornAt\" = MAX(\"lastWornAt\", excluded.\"lastWornAt\"),\n           \"updatedAt\" = excluded.\"updatedAt\"";

/// v4 `upsertRows`'s statement (`wardrobe-wear.repository.ts:374-386`),
/// byte-exact: REPLACE semantics — a colliding (item × wearer) row takes the
/// given tally; its `id` and `createdAt` stay.
const UPSERT_ROW_SQL: &str = "INSERT INTO \"wardrobe_wear_stats\"\n           (\"id\", \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\", \"createdAt\", \"updatedAt\")\n         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)\n         ON CONFLICT (\"itemId\", COALESCE(\"wearerCharacterId\", '')) DO UPDATE SET\n           \"wearCount\" = excluded.\"wearCount\",\n           \"firstWornAt\" = excluded.\"firstWornAt\",\n           \"lastWornAt\" = excluded.\"lastWornAt\",\n           \"lastWornChatId\" = excluded.\"lastWornChatId\",\n           \"updatedAt\" = excluded.\"updatedAt\"";

/// v4 `wardrobeWearIncrementParams` (`wardrobe-wear-stats-ddl.ts:55-73`) —
/// `[id, itemId, wearerCharacterId, at, at, chatId, now, now]` (first and last
/// worn are both `at`; created and updated are both `now`).
pub fn increment_params<'a>(
    id: &'a str,
    item_id: &'a str,
    wearer_character_id: Option<&'a str>,
    at: &'a str,
    chat_id: Option<&'a str>,
    now: &'a str,
) -> [Option<&'a str>; 8] {
    [
        Some(id),
        Some(item_id),
        wearer_character_id,
        Some(at),
        Some(at),
        chat_id,
        Some(now),
        Some(now),
    ]
}

// ============================================================================
// Types (v4 `wardrobe-wear.types.ts` + the repository's own)
// ============================================================================

/// One `wardrobe_wear_stats` row: one wearer's tally for one item (v4
/// `WardrobeWearStatsRowSchema`). Serialized camelCase in DDL key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeWearStatsRow {
    pub id: String,
    /// Wardrobe item id (a vault file's frontmatter id). No FK: items are not rows.
    pub item_id: String,
    /// `None` = unattributed (the wearer was deleted, or an import could not
    /// resolve them).
    pub wearer_character_id: Option<String>,
    pub wear_count: i64,
    pub first_worn_at: String,
    pub last_worn_at: String,
    /// No FK: a deleted chat leaves a dangling id the readers treat as "a chat
    /// since deleted".
    pub last_worn_chat_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// An item's totals across every wearer (v4 `WardrobeWearSummary`).
/// `wear_count: 0` with nulls is "never worn".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeWearSummary {
    pub wear_count: i64,
    pub first_worn_at: Option<String>,
    pub last_worn_at: Option<String>,
    pub last_worn_chat_id: Option<String>,
}

/// v4 `neverWornSummary()` — the canonical "never worn" summary. Readers
/// return this, never an absent value.
pub fn never_worn_summary() -> WardrobeWearSummary {
    WardrobeWearSummary {
        wear_count: 0,
        first_worn_at: None,
        last_worn_at: None,
        last_worn_chat_id: None,
    }
}

/// One wearer's share of an item's tally (v4 `WardrobeWearer`). `None` =
/// unattributed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeWearer {
    pub character_id: Option<String>,
    pub wear_count: i64,
    pub first_worn_at: String,
    pub last_worn_at: String,
    pub last_worn_chat_id: Option<String>,
}

/// Totals plus the per-wearer breakdown, most recent wearer first (v4
/// `WardrobeWearHistory` — the summary's four keys, then `wearers`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeWearHistory {
    pub wear_count: i64,
    pub first_worn_at: Option<String>,
    pub last_worn_at: Option<String>,
    pub last_worn_chat_id: Option<String>,
    pub wearers: Vec<WardrobeWearer>,
}

impl WardrobeWearHistory {
    fn from_summary(summary: WardrobeWearSummary, wearers: Vec<WardrobeWearer>) -> Self {
        WardrobeWearHistory {
            wear_count: summary.wear_count,
            first_worn_at: summary.first_worn_at,
            last_worn_at: summary.last_worn_at,
            last_worn_chat_id: summary.last_worn_chat_id,
            wearers,
        }
    }
}

/// One wear to credit (v4 `WardrobeWearIncrement`). `at` is an ISO timestamp.
/// v4 types `wearerCharacterId` / `chatId` as strings; the seed and the fold
/// write NULLs through the same statement, so both are `Option` here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeWearIncrement {
    pub item_id: String,
    pub wearer_character_id: Option<String>,
    pub chat_id: Option<String>,
    pub at: String,
}

/// Where a put-on gesture came from (v4 `EquipSource`). Recorded in the log;
/// `Merge` never credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquipSource {
    /// Wardrobe dialog set_all / wear / replace / add_to_slot.
    #[serde(rename = "ui")]
    Ui,
    /// `wardrobe_wear`, `wardrobe_create` `equip_now`.
    #[serde(rename = "tool")]
    Tool,
    /// `applyOutfitSelections` for a new chat.
    #[serde(rename = "chat-start")]
    ChatStart,
    /// `applyOutfitSelections` for an added / reactivated seat.
    #[serde(rename = "participant-added")]
    ParticipantAdded,
    /// `applyOutfitSelections` from a merge (never counts).
    #[serde(rename = "merge")]
    Merge,
    /// `removeFromSlot` — nothing can be newly worn; kept for the log line.
    #[serde(rename = "take-off")]
    TakeOff,
}

impl EquipSource {
    /// v4's wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            EquipSource::Ui => "ui",
            EquipSource::Tool => "tool",
            EquipSource::ChatStart => "chat-start",
            EquipSource::ParticipantAdded => "participant-added",
            EquipSource::Merge => "merge",
            EquipSource::TakeOff => "take-off",
        }
    }
}

/// A bundle the caller dissolved into the next slots, with the leaves it
/// contributed (v4's `wornBundles` element `{ id, leafIds }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WornBundle {
    pub id: String,
    pub leaf_ids: Vec<String>,
}

/// [`diff_equipped_outfit`]'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquippedOutfitDiff {
    /// Leaves that went from not-worn to worn — slot order, first seen, deduped.
    pub newly_worn_leaf_ids: Vec<String>,
    /// Bundles that earn a wear (in `worn_bundles` order).
    pub credited_bundle_ids: Vec<String>,
    /// Whether the slots changed at all.
    pub changed: bool,
}

// ============================================================================
// Pure helpers
// ============================================================================

/// v4 `allEquippedItemIds` (`lib/schemas/wardrobe.types.ts:291-293`) —
/// `Array.from(new Set(WARDROBE_SLOT_TYPES.flatMap(s => slots[s] ?? [])))`:
/// every id across the slots, slot order top → bottom → footwear →
/// accessories → hair, first seen, deduped.
pub fn all_equipped_item_ids(slots: &Slots) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        for id in slots.slot(slot) {
            if seen.insert(id.as_str()) {
                out.push(id.clone());
            }
        }
    }
    out
}

/// v4 `diffEquippedOutfit(prior, next, wornBundles = [])`
/// (`wardrobe-wear.repository.ts:104-130`) — the pure half of the chokepoint:
/// which leaves went on, which bundles earn a wear, and whether anything
/// changed at all.
///
/// Bundles: one already credited is skipped; one whose OWN id is newly worn is
/// skipped (a bundle stored whole in the slots — a legacy row — is already
/// counted by the leaf diff); otherwise one is credited when any of its leaves
/// is newly worn.
pub fn diff_equipped_outfit(
    prior: Option<&Slots>,
    next: &Slots,
    worn_bundles: &[WornBundle],
) -> EquippedOutfitDiff {
    let before: HashSet<String> = prior
        .map(|p| all_equipped_item_ids(p).into_iter().collect())
        .unwrap_or_default();
    let newly_worn: Vec<String> = all_equipped_item_ids(next)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    let newly_worn_set: HashSet<&str> = newly_worn.iter().map(String::as_str).collect();

    let mut credited_bundle_ids: Vec<String> = Vec::new();
    for bundle in worn_bundles {
        if credited_bundle_ids.contains(&bundle.id) {
            continue;
        }
        if newly_worn_set.contains(bundle.id.as_str()) {
            continue;
        }
        if bundle
            .leaf_ids
            .iter()
            .any(|leaf| newly_worn_set.contains(leaf.as_str()))
        {
            credited_bundle_ids.push(bundle.id.clone());
        }
    }

    EquippedOutfitDiff {
        newly_worn_leaf_ids: newly_worn,
        credited_bundle_ids,
        changed: !same_slots(prior, next),
    }
}

/// v4 `sameSlots` (`:132-144`): no prior ⇒ the next slots hold no id at all;
/// else every slot positionally equal (v4 unions both objects' keys with
/// `?? []`; [`Slots`] always carries all five).
fn same_slots(a: Option<&Slots>, b: &Slots) -> bool {
    match a {
        None => all_equipped_item_ids(b).is_empty(),
        Some(a) => crate::wardrobe::WARDROBE_SLOT_TYPES
            .iter()
            .all(|slot| a.slot(slot) == b.slot(slot)),
    }
}

/// v4 `summarize(rows)` (`:147-161`) — fold per-wearer rows into an item's
/// totals. Rows with `wearCount <= 0` are skipped; `firstWornAt` is the min by
/// string `<`; `lastWornAt` the max by STRICT `>` — ties keep the FIRST row
/// seen (SELECT order) — and carries that row's `lastWornChatId`.
pub fn summarize(rows: &[WardrobeWearStatsRow]) -> WardrobeWearSummary {
    let mut summary = never_worn_summary();
    for row in rows {
        if row.wear_count <= 0 {
            continue;
        }
        summary.wear_count += row.wear_count;
        if summary
            .first_worn_at
            .as_deref()
            .is_none_or(|first| row.first_worn_at.as_str() < first)
        {
            summary.first_worn_at = Some(row.first_worn_at.clone());
        }
        if summary
            .last_worn_at
            .as_deref()
            .is_none_or(|last| row.last_worn_at.as_str() > last)
        {
            summary.last_worn_at = Some(row.last_worn_at.clone());
            summary.last_worn_chat_id = row.last_worn_chat_id.clone();
        }
    }
    summary
}

/// R-C: `wearCount` read type-tolerantly — v4's `Number(row.wearCount)`. An
/// INTEGER cell (the migration's shape) reads as itself; a REAL cell (a fresh
/// v5 instance's generateDDL shape stores `1` as `1.0`) truncates to its
/// integer; a numeric TEXT parses; NULL is `Number(null)` = 0.
fn wear_count_cell(value: ValueRef<'_>) -> i64 {
    match value {
        ValueRef::Integer(i) => i,
        ValueRef::Real(f) => f as i64,
        ValueRef::Text(t) => std::str::from_utf8(t)
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|f| f as i64)
            .unwrap_or(0),
        ValueRef::Null | ValueRef::Blob(_) => 0,
    }
}

/// One `SELECT *` row → [`WardrobeWearStatsRow`], columns read BY NAME (the two
/// table shapes share names, not affinities).
fn map_row(row: &Row<'_>) -> rusqlite::Result<WardrobeWearStatsRow> {
    Ok(WardrobeWearStatsRow {
        id: row.get("id")?,
        item_id: row.get("itemId")?,
        wearer_character_id: row.get("wearerCharacterId")?,
        wear_count: wear_count_cell(row.get_ref("wearCount")?),
        first_worn_at: row.get("firstWornAt")?,
        last_worn_at: row.get("lastWornAt")?,
        last_worn_chat_id: row.get("lastWornChatId")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

/// `Array.from(new Set(ids.filter(Boolean)))` — first seen, empties dropped.
fn distinct_ids(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect()
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// v4 `runAtomically` — `work` inside ONE savepoint: it opens a transaction
/// when none is active and nests inside one that is (the job applier's), as
/// better-sqlite3's `db.transaction` does. Released on `Ok`; rolled back (and
/// released) on `Err`, so a batch lands whole or not at all.
pub(crate) fn run_atomically<R>(
    conn: &Connection,
    work: impl FnOnce(&Connection) -> Result<R, DbError>,
) -> Result<R, DbError> {
    conn.execute_batch("SAVEPOINT \"wardrobe_wear\"")?;
    match work(conn) {
        Ok(value) => {
            conn.execute_batch("RELEASE \"wardrobe_wear\"")?;
            Ok(value)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO \"wardrobe_wear\"; RELEASE \"wardrobe_wear\"");
            Err(error)
        }
    }
}

/// v4's fallback `safeQuery` line for the two ledger reads (`Error reading
/// wear summaries` / `Error reading wear history`): `{collection, <key>,
/// error}` with the bare SQLite text. The repository is v4's `safeQuery`
/// collection-enriched wrap, so `collection` leads.
fn log_read_failure(
    message: &'static str,
    error: &DbError,
    item_count: Option<usize>,
    item_id: Option<&str>,
) {
    let error = super::fallback::error_text(error);
    match (item_count, item_id) {
        (Some(item_count), _) => tracing::error!(
            target: "quilltap::db",
            collection = WARDROBE_WEAR_STATS_TABLE,
            itemCount = item_count,
            error = %error,
            "{message}"
        ),
        (None, item_id) => tracing::error!(
            target: "quilltap::db",
            collection = WARDROBE_WEAR_STATS_TABLE,
            itemId = item_id.unwrap_or_default(),
            error = %error,
            "{message}"
        ),
    }
}

// ============================================================================
// The repository
// ============================================================================

/// v4 `WardrobeWearRepository` minus the chokepoint, over the MAIN connection.
pub struct WardrobeWearStatsRepository<'a> {
    conn: &'a Connection,
}

impl<'a> WardrobeWearStatsRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        WardrobeWearStatsRepository { conn }
    }

    /// v4 `incrementWears(entries)` (`:272-290`): every entry in ONE savepoint
    /// (a batch lands whole or not at all), ONE `now` for the batch, a minted
    /// UUID per entry, v4's exact statement and parameter order. Empty ⇒ a
    /// no-op.
    pub fn increment_wears(&self, entries: &[WardrobeWearIncrement]) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let now = crate::clock::now_iso();
        run_atomically(self.conn, |conn| {
            let mut statement = conn.prepare(WARDROBE_WEAR_INCREMENT_SQL)?;
            for entry in entries {
                let id = uuid::Uuid::new_v4().to_string();
                statement.execute(increment_params(
                    &id,
                    &entry.item_id,
                    entry.wearer_character_id.as_deref(),
                    &entry.at,
                    entry.chat_id.as_deref(),
                    &now,
                ))?;
            }
            Ok(())
        })
    }

    /// v4 `deleteByItemIds(itemIds)` (`:308-318`) — item deleted: drop its
    /// rows (a composite's deletion leaves its components' rows alone).
    /// Distinct non-empty ids, chunked by [`SQLITE_VARIABLE_CHUNK_SIZE`], one
    /// DELETE per chunk (NOT atomic across chunks, as v4's are not), then v4's
    /// DEBUG `Dropped wear ledger rows for deleted items` (`itemCount`).
    pub fn delete_by_item_ids(&self, item_ids: &[String]) -> Result<(), DbError> {
        let ids = distinct_ids(item_ids);
        if ids.is_empty() {
            return Ok(());
        }
        for chunk in ids.chunks(SQLITE_VARIABLE_CHUNK_SIZE) {
            self.conn.execute(
                &format!(
                    "DELETE FROM \"{WARDROBE_WEAR_STATS_TABLE}\" WHERE \"itemId\" IN ({})",
                    placeholders(chunk.len())
                ),
                rusqlite::params_from_iter(chunk.iter()),
            )?;
        }
        tracing::debug!(
            target: "quilltap::wardrobe_wear",
            module = "wardrobe-wear",
            itemCount = ids.len(),
            "Dropped wear ledger rows for deleted items"
        );
        Ok(())
    }

    /// v4 `foldWearerIntoUnattributed(characterId)` (`:327-363`) — character
    /// deleted: fold each of their rows into the item's unattributed row
    /// (counts summed, earliest first wear, latest last wear and its chat),
    /// then delete theirs. Totals survive; attribution does not. A fold rather
    /// than `SET NULL` because the unique index admits one unattributed row
    /// per item. Read, fold and delete in ONE savepoint (v4's why: a failure
    /// part-way must not leave the wearer's rows AND their unattributed copies,
    /// which would double-count every wear). ONE `now`. Answers the folded row
    /// count; v4's INFO `Folded a departed wearer into the unattributed wear
    /// ledger` (`characterId`, `rowCount`) when it is non-zero, logged on the
    /// CALLER's thread after the savepoint commits.
    pub fn fold_wearer_into_unattributed(&self, character_id: &str) -> Result<usize, DbError> {
        let now = crate::clock::now_iso();
        let folded = run_atomically(self.conn, |conn| {
            let rows = rows_where(
                conn,
                &format!(
                    "SELECT * FROM \"{WARDROBE_WEAR_STATS_TABLE}\" WHERE \"wearerCharacterId\" = ?"
                ),
                params![character_id],
            )?;
            if rows.is_empty() {
                return Ok(0);
            }
            let mut upsert = conn.prepare(FOLD_UPSERT_SQL)?;
            for row in &rows {
                let id = uuid::Uuid::new_v4().to_string();
                upsert.execute(params![
                    id,
                    row.item_id,
                    row.wear_count,
                    row.first_worn_at,
                    row.last_worn_at,
                    row.last_worn_chat_id,
                    now,
                    now
                ])?;
            }
            conn.execute(
                &format!(
                    "DELETE FROM \"{WARDROBE_WEAR_STATS_TABLE}\" WHERE \"wearerCharacterId\" = ?"
                ),
                params![character_id],
            )?;
            Ok(rows.len())
        })?;
        if folded > 0 {
            tracing::info!(
                target: "quilltap::wardrobe_wear",
                module = "wardrobe-wear",
                characterId = character_id,
                rowCount = folded,
                "Folded a departed wearer into the unattributed wear ledger"
            );
        }
        Ok(folded)
    }

    /// v4 `upsertRows(rows)` (`:371-398`) — import / restore: write rows as
    /// given, no increment. A row colliding with an existing (item × wearer)
    /// row REPLACES its tally (the live row's `id` and `createdAt` stay).
    /// Callers that may hand over two rows for one key must merge them first.
    /// One statement per row, NOT wrapped (v4's `rawQuery` per row), then v4's
    /// DEBUG `Upserted wear ledger rows` (`rowCount`). Empty ⇒ a no-op.
    pub fn upsert_rows(&self, rows: &[WardrobeWearStatsRow]) -> Result<(), DbError> {
        if rows.is_empty() {
            return Ok(());
        }
        for row in rows {
            self.conn.execute(
                UPSERT_ROW_SQL,
                params![
                    row.id,
                    row.item_id,
                    row.wearer_character_id,
                    row.wear_count,
                    row.first_worn_at,
                    row.last_worn_at,
                    row.last_worn_chat_id,
                    row.created_at,
                    row.updated_at
                ],
            )?;
        }
        tracing::debug!(
            target: "quilltap::wardrobe_wear",
            module = "wardrobe-wear",
            rowCount = rows.len(),
            "Upserted wear ledger rows"
        );
        Ok(())
    }

    /// v4 `findSummaries(itemIds)` (`:408-430`) — totals for list views. Every
    /// requested (distinct, non-empty) id is in the map; one never worn maps
    /// to [`never_worn_summary`]. A failed read — a missing table included —
    /// logs v4's `Error reading wear summaries` (`itemCount`) and answers the
    /// pre-filled map (v4's fallback IS the pre-filled map, so a read that
    /// failed part-way still answers every id never-worn).
    pub fn find_summaries(&self, item_ids: &[String]) -> HashMap<String, WardrobeWearSummary> {
        let ids = distinct_ids(item_ids);
        let mut result: HashMap<String, WardrobeWearSummary> = ids
            .iter()
            .map(|id| (id.clone(), never_worn_summary()))
            .collect();
        if ids.is_empty() {
            return result;
        }
        match self.find_rows_for_items(&ids) {
            Ok(rows) => {
                // v4 groups into a Map (insertion order) then summarizes each
                // group in SELECT order — the order `summarize`'s tie rule sees.
                let mut by_item: Vec<(String, Vec<WardrobeWearStatsRow>)> = Vec::new();
                for row in rows {
                    match by_item.iter_mut().find(|(id, _)| *id == row.item_id) {
                        Some((_, list)) => list.push(row),
                        None => by_item.push((row.item_id.clone(), vec![row])),
                    }
                }
                for (item_id, rows) in by_item {
                    result.insert(item_id, summarize(&rows));
                }
                result
            }
            Err(error) => {
                log_read_failure(
                    "Error reading wear summaries",
                    &error,
                    Some(ids.len()),
                    None,
                );
                result
            }
        }
    }

    /// v4 `findHistory(itemId)` (`:433-454`) — totals plus the per-wearer rows,
    /// most recent first (string compare on `lastWornAt`, a STABLE sort: ties
    /// keep SELECT order, as V8's sort does). Rows with `wearCount <= 0` are
    /// dropped first. A failed read logs v4's `Error reading wear history`
    /// (`itemId`) and answers never-worn with no wearers.
    pub fn find_history(&self, item_id: &str) -> WardrobeWearHistory {
        let empty = || WardrobeWearHistory::from_summary(never_worn_summary(), Vec::new());
        match self.find_rows_for_items(&[item_id.to_string()]) {
            Ok(rows) => {
                let rows: Vec<WardrobeWearStatsRow> =
                    rows.into_iter().filter(|r| r.wear_count > 0).collect();
                if rows.is_empty() {
                    return empty();
                }
                let mut wearers: Vec<WardrobeWearer> = rows
                    .iter()
                    .map(|row| WardrobeWearer {
                        character_id: row.wearer_character_id.clone(),
                        wear_count: row.wear_count,
                        first_worn_at: row.first_worn_at.clone(),
                        last_worn_at: row.last_worn_at.clone(),
                        last_worn_chat_id: row.last_worn_chat_id.clone(),
                    })
                    .collect();
                // `sort_by` is stable — never `sort_unstable_by` here.
                wearers.sort_by(|a, b| b.last_worn_at.cmp(&a.last_worn_at));
                WardrobeWearHistory::from_summary(summarize(&rows), wearers)
            }
            Err(error) => {
                log_read_failure("Error reading wear history", &error, None, Some(item_id));
                empty()
            }
        }
    }

    /// v4 `findRowsForItems(itemIds)` (`:457-469`) — every ledger row for these
    /// items (export / import). Distinct non-empty ids in first-seen order,
    /// chunked; v4's statement per chunk. No fallback: a failure propagates.
    pub fn find_rows_for_items(
        &self,
        item_ids: &[String],
    ) -> Result<Vec<WardrobeWearStatsRow>, DbError> {
        let ids = distinct_ids(item_ids);
        let mut out = Vec::new();
        for chunk in ids.chunks(SQLITE_VARIABLE_CHUNK_SIZE) {
            out.extend(rows_where(
                self.conn,
                &format!(
                    "SELECT * FROM \"{WARDROBE_WEAR_STATS_TABLE}\" WHERE \"itemId\" IN ({})",
                    placeholders(chunk.len())
                ),
                rusqlite::params_from_iter(chunk.iter()),
            )?);
        }
        Ok(out)
    }

    /// v4 `findRowsForWearer(characterId)` (`:472-478`) — every ledger row one
    /// character holds. No fallback.
    pub fn find_rows_for_wearer(
        &self,
        character_id: &str,
    ) -> Result<Vec<WardrobeWearStatsRow>, DbError> {
        rows_where(
            self.conn,
            &format!(
                "SELECT * FROM \"{WARDROBE_WEAR_STATS_TABLE}\" WHERE \"wearerCharacterId\" = ?"
            ),
            params![character_id],
        )
    }

    /// v4's inherited `findAll()` — the backup's read. An ABSENT table answers
    /// `[]` silently (a pre-round instance the boot has not yet upgraded, as
    /// `services::backup::marshal::query_all` treats every table); any other
    /// failure logs v4's `Error finding all entities` and answers `[]`.
    ///
    /// Only a table the schema really LACKS is the silent arm: a failed
    /// existence check (a lock, a damaged schema) is a read failure like any
    /// other and takes the logged fallback, never a silently ledger-less
    /// backup (the `f5e953a3f` unification's §3 finding).
    pub fn find_all(&self) -> Vec<WardrobeWearStatsRow> {
        use rusqlite::OptionalExtension as _;
        super::fallback::find_all_or_empty(WARDROBE_WEAR_STATS_TABLE, || {
            let exists = self
                .conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [WARDROBE_WEAR_STATS_TABLE],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Ok(Vec::new());
            }
            rows_where(
                self.conn,
                &format!("SELECT * FROM \"{WARDROBE_WEAR_STATS_TABLE}\""),
                [],
            )
        })
    }
}

fn rows_where<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<WardrobeWearStatsRow>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    fn ledger() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        for sql in WARDROBE_WEAR_STATS_DDL {
            conn.execute_batch(sql).unwrap();
        }
        conn
    }

    fn inc(
        item: &str,
        wearer: Option<&str>,
        chat: Option<&str>,
        at: &str,
    ) -> WardrobeWearIncrement {
        WardrobeWearIncrement {
            item_id: item.into(),
            wearer_character_id: wearer.map(str::to_string),
            chat_id: chat.map(str::to_string),
            at: at.into(),
        }
    }

    fn slots(top: &[&str], accessories: &[&str]) -> Slots {
        Slots {
            top: top.iter().map(|s| s.to_string()).collect(),
            accessories: accessories.iter().map(|s| s.to_string()).collect(),
            ..Slots::default()
        }
    }

    /// v4's spec `:90-99`: insert at 1, then a later increment moves last
    /// forward.
    #[test]
    fn a_later_wear_moves_last_worn_forward() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[inc(
            "coat",
            Some("A"),
            Some("chat-1"),
            "2026-01-01T00:00:00.000Z",
        )])
        .unwrap();
        repo.increment_wears(&[inc(
            "coat",
            Some("A"),
            Some("chat-2"),
            "2026-02-01T00:00:00.000Z",
        )])
        .unwrap();
        let rows = repo.find_rows_for_items(&["coat".into()]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].wear_count, 2);
        assert_eq!(rows[0].first_worn_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(rows[0].last_worn_at, "2026-02-01T00:00:00.000Z");
        assert_eq!(rows[0].last_worn_chat_id.as_deref(), Some("chat-2"));
    }

    /// `:101-110`: an EARLIER wear landing late never rewinds.
    #[test]
    fn an_earlier_wear_landing_late_never_rewinds() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[inc(
            "coat",
            Some("A"),
            Some("chat-2"),
            "2026-02-01T00:00:00.000Z",
        )])
        .unwrap();
        repo.increment_wears(&[inc(
            "coat",
            Some("A"),
            Some("chat-1"),
            "2026-01-01T00:00:00.000Z",
        )])
        .unwrap();
        let row = &repo.find_rows_for_items(&["coat".into()]).unwrap()[0];
        assert_eq!(row.first_worn_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(row.last_worn_at, "2026-02-01T00:00:00.000Z");
        assert_eq!(row.last_worn_chat_id.as_deref(), Some("chat-2"));
    }

    /// `:112-120`: a batch with one bad row lands whole or not at all (a NULL
    /// `itemId` violates NOT NULL — v4's `itemId: null`; Rust's type cannot
    /// carry a null id, so the bad row here is a NULL `at`).
    #[test]
    fn a_batch_with_one_bad_row_writes_nothing() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        let bad = WardrobeWearIncrement {
            item_id: "hat".into(),
            wearer_character_id: Some("A".into()),
            chat_id: None,
            at: String::new(),
        };
        // Force the NOT NULL failure through the statement itself.
        conn.execute_batch(
            "CREATE TRIGGER refuse_hat BEFORE INSERT ON wardrobe_wear_stats \
             WHEN NEW.itemId = 'hat' BEGIN SELECT RAISE(ABORT, 'NOT NULL constraint failed'); END;",
        )
        .unwrap();
        let err = repo.increment_wears(&[
            inc("coat", Some("A"), None, "2026-01-01T00:00:00.000Z"),
            bad,
        ]);
        assert!(err.is_err());
        assert!(repo.find_all().is_empty());
    }

    /// `:122-127`: nests inside an open transaction as a savepoint; the outer
    /// ROLLBACK erases it.
    #[test]
    fn nests_inside_an_open_transaction() {
        let conn = ledger();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        WardrobeWearStatsRepository::new(&conn)
            .increment_wears(&[inc("coat", Some("A"), None, "2026-01-01T00:00:00.000Z")])
            .unwrap();
        conn.execute_batch("ROLLBACK").unwrap();
        assert!(WardrobeWearStatsRepository::new(&conn)
            .find_all()
            .is_empty());
    }

    /// `:129-149`: one row per wearer; two NULL-wearer increments are ONE row.
    #[test]
    fn one_row_per_wearer_and_one_unattributed_row() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[
            inc("coat", Some("A"), None, "2026-01-01T00:00:00.000Z"),
            inc("coat", Some("B"), None, "2026-01-01T00:00:00.000Z"),
            inc("coat", None, None, "2026-01-01T00:00:00.000Z"),
            inc("coat", None, None, "2026-01-02T00:00:00.000Z"),
        ])
        .unwrap();
        let rows = repo.find_rows_for_items(&["coat".into()]).unwrap();
        assert_eq!(rows.len(), 3);
        let unattributed: Vec<_> = rows
            .iter()
            .filter(|r| r.wearer_character_id.is_none())
            .collect();
        assert_eq!(unattributed.len(), 1);
        assert_eq!(unattributed[0].wear_count, 2);
    }

    /// `:151-200`: the fold sums, keeps min first / max last + its chat, and
    /// logs v4's INFO; a failing final DELETE rolls the whole fold back.
    #[test]
    fn the_fold_sums_and_rolls_back_whole() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[
            inc("coat", None, Some("chat-0"), "2026-01-05T00:00:00.000Z"),
            inc(
                "coat",
                Some("A"),
                Some("chat-1"),
                "2026-01-01T00:00:00.000Z",
            ),
            inc(
                "coat",
                Some("A"),
                Some("chat-2"),
                "2026-03-01T00:00:00.000Z",
            ),
        ])
        .unwrap();
        let (folded, lines) = captured_with(|| repo.fold_wearer_into_unattributed("A").unwrap());
        assert_eq!(folded, 1);
        assert_eq!(
            lines,
            vec![
                "INFO quilltap::wardrobe_wear Folded a departed wearer into the unattributed wear ledger module=wardrobe-wear characterId=A rowCount=1"
                    .to_string()
            ]
        );
        let rows = repo.find_rows_for_items(&["coat".into()]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].wear_count, 3);
        assert_eq!(rows[0].first_worn_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(rows[0].last_worn_at, "2026-03-01T00:00:00.000Z");
        assert_eq!(rows[0].last_worn_chat_id.as_deref(), Some("chat-2"));

        // Silence leg: nothing to fold, nothing logged.
        let (none, lines) = captured_with(|| repo.fold_wearer_into_unattributed("A").unwrap());
        assert_eq!(none, 0);
        assert!(lines.is_empty(), "{lines:?}");

        // Rollback leg: the final DELETE fails → the wearer's rows survive.
        repo.increment_wears(&[inc("hat", Some("B"), None, "2026-01-01T00:00:00.000Z")])
            .unwrap();
        conn.execute_batch(
            "CREATE TRIGGER refuse_delete BEFORE DELETE ON wardrobe_wear_stats \
             BEGIN SELECT RAISE(ABORT, 'no'); END;",
        )
        .unwrap();
        assert!(repo.fold_wearer_into_unattributed("B").is_err());
        let hat = repo.find_rows_for_items(&["hat".into()]).unwrap();
        assert_eq!(hat.len(), 1);
        assert_eq!(hat[0].wearer_character_id.as_deref(), Some("B"));
        assert_eq!(hat[0].wear_count, 1);
    }

    /// `:202-226`: summaries total across wearers; an unknown id is the zero
    /// summary; history is most recent first.
    #[test]
    fn summaries_and_history() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[
            inc("coat", Some("A"), Some("c1"), "2026-01-01T00:00:00.000Z"),
            inc("coat", Some("B"), Some("c2"), "2026-02-01T00:00:00.000Z"),
        ])
        .unwrap();
        let sums = repo.find_summaries(&["coat".into(), "".into(), "nope".into(), "coat".into()]);
        assert_eq!(sums.len(), 2);
        assert_eq!(sums["coat"].wear_count, 2);
        assert_eq!(sums["coat"].last_worn_chat_id.as_deref(), Some("c2"));
        assert_eq!(sums["nope"], never_worn_summary());
        let history = repo.find_history("coat");
        assert_eq!(history.wear_count, 2);
        assert_eq!(
            history
                .wearers
                .iter()
                .map(|w| w.character_id.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["B", "A"]
        );
        assert!(repo.find_history("nope").wearers.is_empty());
    }

    /// The strict-`>` tie: the FIRST row (SELECT order) keeps `lastWornChatId`;
    /// the history sort is stable on the same tie.
    #[test]
    fn ties_keep_the_first_row() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[
            inc("coat", Some("A"), Some("first"), "2026-01-01T00:00:00.000Z"),
            inc(
                "coat",
                Some("B"),
                Some("second"),
                "2026-01-01T00:00:00.000Z",
            ),
        ])
        .unwrap();
        let rows = repo.find_rows_for_items(&["coat".into()]).unwrap();
        let summary = summarize(&rows);
        assert_eq!(summary.last_worn_chat_id, rows[0].last_worn_chat_id);
        let history = repo.find_history("coat");
        assert_eq!(
            history
                .wearers
                .iter()
                .map(|w| w.character_id.clone())
                .collect::<Vec<_>>(),
            rows.iter()
                .map(|r| r.wearer_character_id.clone())
                .collect::<Vec<_>>()
        );
    }

    /// `:228-235` + the DEBUG line and its silence leg.
    #[test]
    fn delete_by_item_ids_drops_only_those() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[
            inc("coat", Some("A"), None, "2026-01-01T00:00:00.000Z"),
            inc("hat", Some("A"), None, "2026-01-01T00:00:00.000Z"),
        ])
        .unwrap();
        let (_, lines) = captured_with(|| {
            repo.delete_by_item_ids(&["coat".into(), "coat".into(), "".into()])
                .unwrap()
        });
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::wardrobe_wear Dropped wear ledger rows for deleted items module=wardrobe-wear itemCount=1".to_string()]
        );
        let left: Vec<String> = repo.find_all().into_iter().map(|r| r.item_id).collect();
        assert_eq!(left, vec!["hat"]);
        let (_, lines) = captured_with(|| repo.delete_by_item_ids(&["".into()]).unwrap());
        assert!(lines.is_empty());
    }

    /// REPLACE semantics: the colliding row takes the tally, keeps its id.
    #[test]
    fn upsert_rows_replaces_the_tally() {
        let conn = ledger();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[inc(
            "coat",
            Some("A"),
            Some("c1"),
            "2026-01-01T00:00:00.000Z",
        )])
        .unwrap();
        let live = repo.find_all().remove(0);
        let incoming = WardrobeWearStatsRow {
            id: "other-id".into(),
            item_id: "coat".into(),
            wearer_character_id: Some("A".into()),
            wear_count: 7,
            first_worn_at: "2025-01-01T00:00:00.000Z".into(),
            last_worn_at: "2025-06-01T00:00:00.000Z".into(),
            last_worn_chat_id: None,
            created_at: "2025-01-01T00:00:00.000Z".into(),
            updated_at: "2025-06-01T00:00:00.000Z".into(),
        };
        let (_, lines) =
            captured_with(|| repo.upsert_rows(std::slice::from_ref(&incoming)).unwrap());
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::wardrobe_wear Upserted wear ledger rows module=wardrobe-wear rowCount=1".to_string()]
        );
        let row = repo.find_all().remove(0);
        assert_eq!(row.id, live.id);
        assert_eq!(row.created_at, live.created_at);
        assert_eq!(row.wear_count, 7);
        assert_eq!(row.first_worn_at, incoming.first_worn_at);
        assert_eq!(row.last_worn_chat_id, None);
        let (_, lines) = captured_with(|| repo.upsert_rows(&[]).unwrap());
        assert!(lines.is_empty());
    }

    /// R-C: a REAL `wearCount` cell (a fresh v5 instance's generateDDL shape)
    /// reads as an integer.
    #[test]
    fn a_real_wear_count_reads_as_an_integer() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE \"wardrobe_wear_stats\" (\"id\" TEXT PRIMARY KEY NOT NULL, \"itemId\" TEXT NOT NULL, \
             \"wearerCharacterId\" TEXT, \"wearCount\" REAL NOT NULL, \"firstWornAt\" TEXT NOT NULL, \
             \"lastWornAt\" TEXT NOT NULL, \"lastWornChatId\" TEXT, \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute_batch(WARDROBE_WEAR_STATS_DDL[1]).unwrap();
        let repo = WardrobeWearStatsRepository::new(&conn);
        repo.increment_wears(&[inc("coat", Some("A"), None, "2026-01-01T00:00:00.000Z")])
            .unwrap();
        repo.increment_wears(&[inc("coat", Some("A"), None, "2026-01-02T00:00:00.000Z")])
            .unwrap();
        let stored: f64 = conn
            .query_row("SELECT wearCount FROM wardrobe_wear_stats", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored, 2.0);
        let typeof_: String = conn
            .query_row(
                "SELECT typeof(wearCount) FROM wardrobe_wear_stats",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(typeof_, "real");
        assert_eq!(repo.find_all()[0].wear_count, 2);
        assert_eq!(wear_count_cell(ValueRef::Real(2.0)), 2);
        assert_eq!(wear_count_cell(ValueRef::Text(b"3")), 3);
        assert_eq!(wear_count_cell(ValueRef::Null), 0);
    }

    /// An absent table: `find_all` is silent `[]`; `find_summaries` /
    /// `find_history` answer never-worn with v4's fallback line; the
    /// row reads propagate.
    #[test]
    fn an_absent_table() {
        let conn = Connection::open_in_memory().unwrap();
        let repo = WardrobeWearStatsRepository::new(&conn);
        let (all, lines) = captured_with(|| repo.find_all());
        assert!(all.is_empty());
        assert!(lines.is_empty());
        let (sums, lines) = captured_with(|| repo.find_summaries(&["coat".into()]));
        assert_eq!(sums["coat"], never_worn_summary());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error reading wear summaries collection=wardrobe_wear_stats itemCount=1 error=no such table: wardrobe_wear_stats".to_string()]
        );
        let (history, lines) = captured_with(|| repo.find_history("coat"));
        assert_eq!(history.wear_count, 0);
        assert!(history.wearers.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error reading wear history collection=wardrobe_wear_stats itemId=coat error=no such table: wardrobe_wear_stats".to_string()]
        );
        assert!(repo.find_rows_for_items(&["coat".into()]).is_err());
        assert!(repo.find_rows_for_wearer("A").is_err());
        // No ids: no read, no line.
        let (sums, lines) = captured_with(|| repo.find_summaries(&[]));
        assert!(sums.is_empty());
        assert!(lines.is_empty());
    }

    /// v4's `diffEquippedOutfit` spec rows (`:241-342`).
    #[test]
    fn diff_equipped_outfit_rules() {
        // A newly worn leaf only.
        let d = diff_equipped_outfit(
            Some(&slots(&["shirt"], &[])),
            &slots(&["shirt", "coat"], &[]),
            &[],
        );
        assert_eq!(d.newly_worn_leaf_ids, vec!["coat"]);
        assert!(d.changed);
        // Same id in two slots: one leaf, slot order.
        let d = diff_equipped_outfit(None, &slots(&["coat"], &["coat", "pin"]), &[]);
        assert_eq!(d.newly_worn_leaf_ids, vec!["coat", "pin"]);
        // A bundle credited once when ≥1 leaf transitioned.
        let suit = WornBundle {
            id: "suit".into(),
            leaf_ids: vec!["jacket".into(), "shirt".into()],
        };
        let d = diff_equipped_outfit(
            Some(&slots(&["shirt"], &[])),
            &slots(&["shirt", "jacket"], &[]),
            &[suit.clone(), suit.clone()],
        );
        assert_eq!(d.credited_bundle_ids, vec!["suit"]);
        // A bundle whose leaves were all on earns nothing; unchanged slots.
        let d = diff_equipped_outfit(
            Some(&slots(&["shirt", "jacket"], &[])),
            &slots(&["shirt", "jacket"], &[]),
            std::slice::from_ref(&suit),
        );
        assert!(d.credited_bundle_ids.is_empty());
        assert!(!d.changed);
        // A bundle stored whole (legacy) is counted by the leaf diff alone.
        let d = diff_equipped_outfit(None, &slots(&["suit"], &[]), &[suit]);
        assert_eq!(d.newly_worn_leaf_ids, vec!["suit"]);
        assert!(d.credited_bundle_ids.is_empty());
        // An empty write over nothing is unchanged.
        assert!(!diff_equipped_outfit(None, &Slots::default(), &[]).changed);
    }

    #[test]
    fn equip_source_wire_strings() {
        for (s, wire) in [
            (EquipSource::Ui, "ui"),
            (EquipSource::Tool, "tool"),
            (EquipSource::ChatStart, "chat-start"),
            (EquipSource::ParticipantAdded, "participant-added"),
            (EquipSource::Merge, "merge"),
            (EquipSource::TakeOff, "take-off"),
        ] {
            assert_eq!(s.as_str(), wire);
            assert_eq!(serde_json::to_value(s).unwrap(), serde_json::json!(wire));
        }
    }

    #[test]
    fn row_serializes_camel_case_in_ddl_order() {
        let row = WardrobeWearStatsRow {
            id: "i".into(),
            item_id: "it".into(),
            wearer_character_id: None,
            wear_count: 1,
            first_worn_at: "f".into(),
            last_worn_at: "l".into(),
            last_worn_chat_id: None,
            created_at: "c".into(),
            updated_at: "u".into(),
        };
        assert_eq!(
            serde_json::to_string(&row).unwrap(),
            r#"{"id":"i","itemId":"it","wearerCharacterId":null,"wearCount":1,"firstWornAt":"f","lastWornAt":"l","lastWornChatId":null,"createdAt":"c","updatedAt":"u"}"#
        );
    }
}
