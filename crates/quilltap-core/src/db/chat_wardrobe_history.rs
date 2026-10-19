//! The chat wardrobe history's DATA layer — v4 `lib/database/repositories/
//! chat-wardrobe-history.repository.ts` + `lib/database/backends/sqlite/
//! chat-wardrobe-history-ddl.ts` + `lib/schemas/chat-wardrobe-history.types.ts`
//! (`83387d0e5`, "outfit history storage, phase 1", with `4ae20977f`'s
//! `actorToolName`; the `e8e17b2d9` pin; P4.D315).
//!
//! Backs `chat_wardrobe_history`: per (chat × character) a numbered timeline of
//! the outfits worn there, `0` being what they arrived in, and a cursor
//! (`isCurrent`) on the entry they are wearing now.
//!
//! ## The chokepoint (v4's why, carried)
//!
//! [`ChatWardrobeHistoryRepository::settle`] is the one runtime writer. It is
//! reached at the moment Aurora's announcement is scheduled (one entry per
//! announcement, never per slot gesture) and from the outfit selections for
//! entry `0` (P4.D316's callers). It always reads the chat's *live* slots
//! itself — never a snapshot the caller passes — so the entry records what the
//! chat really holds. [`ChatWardrobeHistoryRepository::upsert_rows`] /
//! [`ChatWardrobeHistoryRepository::upsert_raw_rows`] are for restore and
//! import only. Nothing renumbers or prunes.
//!
//! **The DDL (v4's why).** Two unique indexes carry the invariants:
//! `(chatId, characterId, entryIndex)` keeps each timeline's numbers unique,
//! and the partial index on `(chatId, characterId) WHERE isCurrent = 1` admits
//! at most one cursor per timeline. `slots` is plain JSON TEXT like
//! `chats.equippedOutfit` — not a compressed text column; nothing searches it
//! and the rows are tiny. `actorToolName` was added after the table shipped;
//! the column migration appends it to an existing table, which is why it sits
//! last in the DDL too — a fresh table and an ALTERed one agree on column
//! ORDER (two `sqlite_master` texts are reachable, one column order).
//!
//! ## On the single writer (R-10)
//!
//! v4's `settle` reads the live slots after the caller's outfit commit and
//! then runs ONE `db.transaction` (which nests as a savepoint inside the job
//! applier's). v5's `settle` is a method on the borrowing repository, called
//! INSIDE a `Db::write` / `write_blocking` closure on the writer's connection:
//! the live read runs on that SAME connection (so a caller's uncommitted
//! commit in the same closure IS the live outfit — the observable equal of
//! v4's after-commit read), and the read-compare-write runs under ONE named
//! savepoint ([`super::schema_probe::in_savepoint`], `"wardrobe_history"`).
//! Every `Err` rolls back that savepoint ONLY — the caller's earlier writes in
//! the same closure survive — logs v4's WARN and answers
//! [`SettleOutcome::Skipped`]. The live read is v4's FALLBACK reader
//! ([`super::chats_outfits::ChatOutfitsRepository::get_equipped_outfit_for_character`]
//! answers `None` on a failed chat read, never an `Err` — measured), so a
//! corrupt `equippedOutfit` cell records an EMPTY outfit, never a skip.
//! [`settle_db`] is the after-commit form (one `db.write` around `settle`).
//!
//! **The realtime publish (R-12)** fires inside `settle`, on `Appended` /
//! `CursorMoved` only, after the savepoint is released. v4 publishes after its
//! transaction COMMITTED; v5's call sits before the caller's closure commits.
//! Measured: `realtime::bus::publish_realtime` is a 250 ms trailing-edge
//! debounce whose delivery runs on a task the host spawns — never synchronous
//! with the publish — so no subscriber can act on the hint before the writer
//! closure returns and commits.
//!
//! ## Logging
//!
//! v4's `logger.child({ module: 'wardrobe-history' })` → `target:
//! "quilltap::wardrobe_history", module = "wardrobe-history"` (the wear
//! ledger's convention). A v4 `null` field renders `null`; an object field
//! renders through the `…Json` file-layer convention (`intentJson`,
//! `outcomeJson`). Every line here may run on the writer thread — invisible to
//! the thread-scoped `captured_with` there (pin through `global_capture`, or
//! on a bare `Connection` on the test thread).
//!
//! ## NO-PORT (recorded)
//!
//! - The job-child buffering and the child proxy's `'wardrobeHistory.settle':
//!   'write'` override (`METHOD_OVERRIDES`) — v5 has no job child (P4.D255
//!   (5)).
//! - `runAtomically`'s `Wardrobe history: the main database is not
//!   initialized` and `settleAppend`'s `ChatWardrobeHistoryRepository.settle
//!   needs a chats store` throws — structurally unreachable: the repository
//!   always holds a connection and reads the live slots through it.
//! - v4's backend ERROR `Raw query failed` `{query, error}` that precedes the
//!   upsert WARN (and every failing `rawQuery`) — the standing NO-PORT class
//!   (`db/fallback.rs`'s `Raw query failed` record).

use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params, params_from_iter, Connection, Row};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use super::schema_probe::in_savepoint;
use super::DbError;
use crate::wardrobe::{hash_equipped_slots, normalize_equipped_slots, Slots};

/// v4 `CHAT_WARDROBE_HISTORY_TABLE`.
pub const CHAT_WARDROBE_HISTORY_TABLE: &str = "chat_wardrobe_history";

/// v4 `CHAT_WARDROBE_HISTORY_DDL` (`chat-wardrobe-history-ddl.ts:18-36` at the
/// pin) — the table + the two UNIQUE indexes, BYTE-exact (column lines FOUR
/// leading spaces, the closing `)` TWO, each `ON` line FOUR): SQLite stores the
/// text as written minus `IF NOT EXISTS`, and `sqlite_master.sql` is a
/// comparand. The table-creation ensure ([`super::chat_wardrobe_history_repair`])
/// runs them; nothing else spells them.
pub const CHAT_WARDROBE_HISTORY_DDL: [&str; 3] = [
    r#"CREATE TABLE IF NOT EXISTS "chat_wardrobe_history" (
    "id" TEXT PRIMARY KEY,
    "chatId" TEXT NOT NULL REFERENCES "chats"("id") ON DELETE CASCADE,
    "characterId" TEXT NOT NULL,
    "entryIndex" INTEGER NOT NULL,
    "slots" TEXT NOT NULL,
    "slotsHash" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "actorCharacterId" TEXT,
    "isCurrent" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TEXT NOT NULL,
    "actorToolName" TEXT
  )"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_chat_wardrobe_history_entry"
    ON "chat_wardrobe_history" ("chatId", "characterId", "entryIndex")"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_chat_wardrobe_history_current"
    ON "chat_wardrobe_history" ("chatId", "characterId") WHERE "isCurrent" = 1"#,
];

/// v4 `CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN`.
pub const CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN: &str = "actorToolName";

/// v4 `CHAT_WARDROBE_HISTORY_INSERT_SQL` (`:46-50`), byte-exact (the template
/// literal's leading newline and indentation included). Positional: id,
/// chatId, characterId, entryIndex, slots, slotsHash, source,
/// actorCharacterId, actorToolName, isCurrent, createdAt.
pub const CHAT_WARDROBE_HISTORY_INSERT_SQL: &str = "\n  INSERT INTO \"chat_wardrobe_history\"\n    (\"id\", \"chatId\", \"characterId\", \"entryIndex\", \"slots\", \"slotsHash\", \"source\", \"actorCharacterId\", \"actorToolName\", \"isCurrent\", \"createdAt\")\n  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)\n";

/// v4 `upsertRows`' conflict tail (`chat-wardrobe-history.repository.ts:
/// 336-345`), appended to the INSERT exactly as v4's template does. `chatId` /
/// `characterId` are NOT in the update set.
const UPSERT_CONFLICT_SQL: &str = "\n           ON CONFLICT (\"id\") DO UPDATE SET\n             \"entryIndex\" = excluded.\"entryIndex\",\n             \"slots\" = excluded.\"slots\",\n             \"slotsHash\" = excluded.\"slotsHash\",\n             \"source\" = excluded.\"source\",\n             \"actorCharacterId\" = excluded.\"actorCharacterId\",\n             \"actorToolName\" = excluded.\"actorToolName\",\n             \"isCurrent\" = excluded.\"isCurrent\",\n             \"createdAt\" = excluded.\"createdAt\"";

/// The savepoint `settle` runs under (R-4 — distinct from the wear ledger's
/// `"wardrobe_wear"`, so a nested trace reads which ledger it was).
const SAVEPOINT: &str = "wardrobe_history";

// ============================================================================
// The types (v4 `chat-wardrobe-history.types.ts`)
// ============================================================================

/// v4 `WardrobeHistorySourceSchema` — who (or what) produced an entry, in v4's
/// enum order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WardrobeHistorySource {
    /// `applyOutfitSelections` for a new chat.
    ChatStart,
    /// `applyOutfitSelections` for an added / reactivated seat.
    ParticipantAdded,
    /// `applyOutfitSelections` from a merge.
    Merge,
    /// The Wardrobe dialog (`?action=equip`), or Wear again
    /// (`actorCharacterId` = the user's live character).
    Operator,
    /// A `wardrobe_*` tool call in a model turn (`actorCharacterId` = caller).
    Character,
    /// A custom tool's wardrobe effects (`actorCharacterId` = rolling
    /// character, `actorToolName` = the tool).
    Pascal,
    /// The backfill migration, or a first-touch seed.
    Seeded,
}

impl WardrobeHistorySource {
    /// The wire / stored string.
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeHistorySource::ChatStart => "chat-start",
            WardrobeHistorySource::ParticipantAdded => "participant-added",
            WardrobeHistorySource::Merge => "merge",
            WardrobeHistorySource::Operator => "operator",
            WardrobeHistorySource::Character => "character",
            WardrobeHistorySource::Pascal => "pascal",
            WardrobeHistorySource::Seeded => "seeded",
        }
    }

    /// v4 `normalizeRow`'s `WardrobeHistorySourceSchema.safeParse(row.source)`
    /// fallback: a stored value outside the seven READS as `Seeded`.
    pub fn from_stored(raw: &str) -> Self {
        match raw {
            "chat-start" => WardrobeHistorySource::ChatStart,
            "participant-added" => WardrobeHistorySource::ParticipantAdded,
            "merge" => WardrobeHistorySource::Merge,
            "operator" => WardrobeHistorySource::Operator,
            "character" => WardrobeHistorySource::Character,
            "pascal" => WardrobeHistorySource::Pascal,
            _ => WardrobeHistorySource::Seeded,
        }
    }

    /// v4 `ARRIVAL_SOURCES` (`:64`): the paths that dress a character for the
    /// first time in a chat — their first entry is the outfit itself.
    pub fn is_arrival(self) -> bool {
        matches!(
            self,
            WardrobeHistorySource::ChatStart
                | WardrobeHistorySource::ParticipantAdded
                | WardrobeHistorySource::Merge
        )
    }
}

/// v4 `WardrobeHistoryProvenance` — who is making an outfit change. `source`
/// is never `Seeded` (v4's `Exclude<…, 'seeded'>`; [`Self::new`] asserts it).
/// The two options are v4's `?:` keys: an absent one is OMITTED from the
/// object bytes, never `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeHistoryProvenance {
    pub source: WardrobeHistorySource,
    /// See [`ChatWardrobeHistoryRow::actor_character_id`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_character_id: Option<String>,
    /// `pascal` only: the custom tool's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl WardrobeHistoryProvenance {
    /// A provenance with no actor and no tool.
    pub fn new(source: WardrobeHistorySource) -> Self {
        debug_assert!(
            source != WardrobeHistorySource::Seeded,
            "a provenance's source is never `seeded` (v4 Exclude<…, 'seeded'>)"
        );
        WardrobeHistoryProvenance {
            source,
            actor_character_id: None,
            tool_name: None,
        }
    }

    /// The same provenance naming the actor.
    pub fn with_actor(mut self, actor_character_id: impl Into<String>) -> Self {
        self.actor_character_id = Some(actor_character_id.into());
        self
    }

    /// The same provenance naming the custom tool (`pascal` only).
    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }
}

/// v4 `WardrobeHistoryIntent` — what a turn owes the ledger for one
/// character; the last intent in a turn wins. Serialises to v4's object
/// bytes: `{"kind":"append","provenance":{…}}` / `{"kind":"cursor","index":3}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WardrobeHistoryIntent {
    Append {
        provenance: WardrobeHistoryProvenance,
    },
    Cursor {
        index: i64,
    },
}

/// One `chat_wardrobe_history` row — v4 `ChatWardrobeHistoryRowSchema` as the
/// repository's `normalizeRow` produces it.
///
/// **The FIELD DECLARATION ORDER IS A CONTRACT** (C1 §2): v4's `normalizeRow`
/// order — `actorToolName` BEFORE `isCurrent`, NOT the DDL's column order —
/// which the backup file, the `.qtap` record and the import comparand all
/// emit. `slots` serialises through [`Slots::to_value`] (`top→hair`); the two
/// actor options serialise `null`, never skipped.
///
/// `Deserialize` reads the SHAPE only (a missing `actorToolName` is `null` —
/// v4's `.default(null)`); it is NOT v4's Zod parse (no UUID / length /
/// enum checks) — a carrier that must refuse what v4 refuses validates first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatWardrobeHistoryRow {
    pub id: String,
    pub chat_id: String,
    /// No FK: characters may be archived tombstones.
    pub character_id: String,
    /// 0-based, contiguous per (chatId, characterId); never renumbered.
    pub entry_index: i64,
    /// Dissolved leaves, the shape `chats.equippedOutfit` stores.
    #[serde(
        serialize_with = "serialize_slots",
        deserialize_with = "deserialize_slots"
    )]
    pub slots: Slots,
    /// `hashEquippedSlots(slots)` — passed through as STORED on read.
    pub slots_hash: String,
    /// The CANONICAL source (an unknown stored value reads `seeded`).
    pub source: String,
    /// Who made the change, by source: `character` — the character whose tool
    /// call it was; `pascal` — the rolling character; `operator` — the
    /// character the user was speaking as when they dressed them (the
    /// "Speaking As" seat, impersonation included), or null when the user had
    /// none. Null for arrivals and seeds.
    pub actor_character_id: Option<String>,
    /// `pascal` only: the name of the custom tool whose wardrobe effects made
    /// the change. Null for every other source.
    #[serde(default)]
    pub actor_tool_name: Option<String>,
    /// The cursor: exactly one row per (chatId, characterId) is current.
    pub is_current: bool,
    pub created_at: String,
}

fn serialize_slots<S: Serializer>(slots: &Slots, s: S) -> Result<S::Ok, S::Error> {
    slots.to_value().serialize(s)
}

fn deserialize_slots<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Slots, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(Slots::from_value(Some(&v)))
}

/// v4 `SettleOutcome` — what a `settle` call did. Serialises to v4's object
/// bytes (the tier-2 comparand): `{"kind":"appended","entryIndex":1,
/// "seeded":true}` / `{"kind":"unchanged","entryIndex":2}` /
/// `{"kind":"cursor-moved","entryIndex":0}` / `{"kind":"skipped","reason":
/// "error"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    Appended {
        index: i64,
        seeded: bool,
    },
    CursorMoved {
        index: i64,
    },
    Unchanged {
        index: i64,
    },
    /// `"error"` | `"missing-index"`.
    Skipped {
        reason: &'static str,
    },
}

impl SettleOutcome {
    /// v4's `kind` string.
    pub fn kind(&self) -> &'static str {
        match self {
            SettleOutcome::Appended { .. } => "appended",
            SettleOutcome::CursorMoved { .. } => "cursor-moved",
            SettleOutcome::Unchanged { .. } => "unchanged",
            SettleOutcome::Skipped { .. } => "skipped",
        }
    }
}

impl Serialize for SettleOutcome {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("kind", self.kind())?;
        match self {
            SettleOutcome::Appended { index, seeded } => {
                m.serialize_entry("entryIndex", index)?;
                m.serialize_entry("seeded", seeded)?;
            }
            SettleOutcome::CursorMoved { index } | SettleOutcome::Unchanged { index } => {
                m.serialize_entry("entryIndex", index)?;
            }
            SettleOutcome::Skipped { reason } => m.serialize_entry("reason", reason)?,
        }
        m.end()
    }
}

// ============================================================================
// Row reading (v4 `parseSlots` / `normalizeRow`)
// ============================================================================

/// v4 `parseSlots(raw)` — `normalizeEquippedSlots(JSON.parse(raw))`, a throw →
/// empty.
fn parse_slots(raw: &str) -> Slots {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => Slots::from_value(Some(&normalize_equipped_slots(Some(&v)))),
        Err(_) => Slots::fresh(),
    }
}

/// A cell as JS would see it through better-sqlite3, for the string fields.
fn text_cell(v: ValueRef<'_>) -> Option<String> {
    match v {
        ValueRef::Null => None,
        ValueRef::Text(t) | ValueRef::Blob(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Integer(i) => Some(i.to_string()),
        ValueRef::Real(f) => Some(crate::pascal::js_value::number_to_string(f)),
    }
}

/// JS `Number(cell)` for the numeric fields (`entryIndex`, `isCurrent`).
fn number_cell(v: ValueRef<'_>) -> f64 {
    match v {
        ValueRef::Null => 0.0,
        ValueRef::Integer(i) => i as f64,
        ValueRef::Real(f) => f,
        ValueRef::Text(t) | ValueRef::Blob(t) => {
            crate::jsnum::number_from_str(&String::from_utf8_lossy(t))
        }
    }
}

/// v4 `normalizeRow` (`:90-105`) — every column read BY NAME (a table without
/// `actorToolName` — the pre-column vintage before its ALTER, a hand plant —
/// reads `null`); `entryIndex` `Number(…)`; `slots` through [`parse_slots`];
/// the STORED `slotsHash` passed through; `source` through
/// [`WardrobeHistorySource::from_stored`] (the wire carries the CANONICAL
/// value); `isCurrent` = `Number(…) === 1` (a `2` reads `false`).
///
/// A NULL `id` (v4 writes one for a raw restore row with no `id` — the
/// legacy non-INTEGER PRIMARY KEY admits NULL; measured) reads `""` where v4
/// reads `null` — the recorded `NULL_ID_READS_EMPTY` divergence, reachable only
/// through a hand-edited archive.
fn normalize_row(r: &Row<'_>) -> rusqlite::Result<ChatWardrobeHistoryRow> {
    let cell = |name: &str| -> rusqlite::Result<Option<ValueRef<'_>>> {
        match r.as_ref().column_index(name) {
            Ok(i) => r.get_ref(i).map(Some),
            Err(_) => Ok(None),
        }
    };
    let text =
        |name: &str| -> rusqlite::Result<Option<String>> { Ok(cell(name)?.and_then(text_cell)) };
    let entry_index = cell("entryIndex")?.map(number_cell).unwrap_or(0.0);
    Ok(ChatWardrobeHistoryRow {
        id: text("id")?.unwrap_or_default(),
        chat_id: text("chatId")?.unwrap_or_default(),
        character_id: text("characterId")?.unwrap_or_default(),
        entry_index: entry_index as i64,
        slots: parse_slots(&text("slots")?.unwrap_or_default()),
        slots_hash: text("slotsHash")?.unwrap_or_default(),
        source: WardrobeHistorySource::from_stored(&text("source")?.unwrap_or_default())
            .as_str()
            .to_string(),
        actor_character_id: text("actorCharacterId")?,
        actor_tool_name: text("actorToolName")?,
        is_current: cell("isCurrent")?.map(number_cell) == Some(1.0),
        created_at: text("createdAt")?.unwrap_or_default(),
    })
}

/// v4 `buildRow(input)` (`:124-148`) — a fresh id, the slots cloned, the hash
/// recomputed, current.
#[allow(clippy::too_many_arguments)]
fn build_row(
    chat_id: &str,
    character_id: &str,
    entry_index: i64,
    slots: &Slots,
    source: WardrobeHistorySource,
    actor_character_id: Option<String>,
    actor_tool_name: Option<String>,
    at: &str,
) -> ChatWardrobeHistoryRow {
    ChatWardrobeHistoryRow {
        id: uuid::Uuid::new_v4().to_string(),
        chat_id: chat_id.to_string(),
        character_id: character_id.to_string(),
        entry_index,
        slots: slots.clone(),
        slots_hash: hash_equipped_slots(Some(slots)),
        source: source.as_str().to_string(),
        actor_character_id,
        actor_tool_name,
        is_current: true,
        created_at: at.to_string(),
    }
}

/// v4 `insertParams(row)` (`:108-122`) for a TYPED row — the 11 positional
/// values in INSERT order, `JSON.stringify(row.slots)`, `isCurrent ? 1 : 0`.
fn insert_params(row: &ChatWardrobeHistoryRow) -> [SqlValue; 11] {
    let opt = |v: &Option<String>| match v {
        Some(s) => SqlValue::Text(s.clone()),
        None => SqlValue::Null,
    };
    [
        SqlValue::Text(row.id.clone()),
        SqlValue::Text(row.chat_id.clone()),
        SqlValue::Text(row.character_id.clone()),
        SqlValue::Integer(row.entry_index),
        SqlValue::Text(row.slots.to_value().to_string()),
        SqlValue::Text(row.slots_hash.clone()),
        SqlValue::Text(row.source.clone()),
        opt(&row.actor_character_id),
        opt(&row.actor_tool_name),
        SqlValue::Integer(i64::from(row.is_current)),
        SqlValue::Text(row.created_at.clone()),
    ]
}

/// better-sqlite3's bind of ONE JS value (measured at the pin): a string binds
/// TEXT; EVERY number binds as a double (so a `5` lands in a TEXT column as
/// `"5.0"`, and in an INTEGER column as `5` through SQLite's affinity);
/// `null` / `undefined` bind NULL; a boolean throws `SQLite3 can only bind
/// numbers, strings, bigints, buffers, and null`. An object or array argument
/// is not a bindable scalar either: better-sqlite3 reads an object as a
/// named-parameter bag and flattens an array into the argument list — v5
/// refuses both with the boolean's sentence (a recorded divergence class,
/// `NON_SCALAR_RAW_CELL`; reachable only through a hand-edited archive).
fn js_bind(v: Option<&Value>) -> Result<SqlValue, String> {
    const REFUSAL: &str = "SQLite3 can only bind numbers, strings, bigints, buffers, and null";
    match v {
        None | Some(Value::Null) => Ok(SqlValue::Null),
        Some(Value::String(s)) => Ok(SqlValue::Text(s.clone())),
        Some(Value::Number(n)) => Ok(SqlValue::Real(n.as_f64().unwrap_or(f64::NAN))),
        Some(_) => Err(REFUSAL.to_string()),
    }
}

/// JS truthiness of a JSON value (v4 `row.isCurrent ? 1 : 0`).
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

// ============================================================================
// The repository
// ============================================================================

/// v4 `ChatWardrobeHistoryRepository`, over the MAIN connection (the writer's
/// inside a `Db::write` closure for `settle` / the upserts / the delete; any
/// connection for the finds).
pub struct ChatWardrobeHistoryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ChatWardrobeHistoryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        ChatWardrobeHistoryRepository { conn }
    }

    // ------------------------------------------------------------------
    // The chokepoint
    // ------------------------------------------------------------------

    /// v4 `settle(chatId, characterId, intent)` (`:177-195`) — record what an
    /// outfit change leaves behind, or move the cursor. NEVER fails in its
    /// observable effect: any error rolls back this call's savepoint only, is
    /// logged at WARN (`Wardrobe history settle failed; the outfit change
    /// stands unrecorded`) and answers `Skipped { reason: "error" }`. Publishes
    /// `chats` / `chat_id` on `Appended` / `CursorMoved` only (R-12).
    pub fn settle(
        &self,
        chat_id: &str,
        character_id: &str,
        intent: &WardrobeHistoryIntent,
    ) -> SettleOutcome {
        self.settle_with_live(chat_id, character_id, intent, || {
            // v4's `getEquippedOutfitForCharacter(…) ?? makeEmptyEquippedSlots()`
            // — the FALLBACK reader (a failed chat read answers `None`).
            let live = super::chats_outfits::ChatOutfitsRepository::new(self.conn)
                .get_equipped_outfit_for_character(chat_id, character_id);
            Slots::from_value(live.as_ref())
        })
    }

    /// [`Self::settle`] with the live-slot read injected (the unit tests' fake
    /// `LiveOutfitReader`, v4's own test seam).
    fn settle_with_live(
        &self,
        chat_id: &str,
        character_id: &str,
        intent: &WardrobeHistoryIntent,
        live: impl FnOnce() -> Slots,
    ) -> SettleOutcome {
        let result = match intent {
            WardrobeHistoryIntent::Append { provenance } => {
                self.settle_append(chat_id, character_id, provenance, live())
            }
            WardrobeHistoryIntent::Cursor { index } => {
                self.settle_cursor(chat_id, character_id, *index)
            }
        };
        match result {
            Ok(outcome) => {
                if matches!(
                    outcome,
                    SettleOutcome::Appended { .. } | SettleOutcome::CursorMoved { .. }
                ) {
                    crate::realtime::bus::publish_realtime(
                        crate::realtime::types::RealtimeTopic::Chats,
                        Some(chat_id),
                    );
                }
                outcome
            }
            Err(error) => {
                tracing::warn!(
                    target: "quilltap::wardrobe_history",
                    module = "wardrobe-history",
                    chatId = chat_id,
                    characterId = character_id,
                    intentJson = %serde_json::to_string(intent).unwrap_or_default(),
                    error = %super::fallback::error_text(&error),
                    "Wardrobe history settle failed; the outfit change stands unrecorded"
                );
                SettleOutcome::Skipped { reason: "error" }
            }
        }
    }

    /// v4 `settleAppend` (`:197-287`).
    fn settle_append(
        &self,
        chat_id: &str,
        character_id: &str,
        provenance: &WardrobeHistoryProvenance,
        live: Slots,
    ) -> Result<SettleOutcome, DbError> {
        let live_hash = hash_equipped_slots(Some(&live));
        let empty_hash = hash_equipped_slots(None);
        let source = provenance.source;
        let actor_tool_name = if source == WardrobeHistorySource::Pascal {
            provenance.tool_name.clone()
        } else {
            None
        };
        let at = crate::clock::now_iso();

        // Read, compare and write under one savepoint, so two settles for the
        // same character cannot both claim max + 1.
        let outcome = in_savepoint(self.conn, SAVEPOINT, |conn| {
            let rows = self.query_rows(
                &format!(
                    "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 ORDER BY \"entryIndex\" ASC"
                ),
                &[&chat_id, &character_id],
            )?;
            let insert = |row: &ChatWardrobeHistoryRow| -> Result<(), DbError> {
                conn.execute(
                    CHAT_WARDROBE_HISTORY_INSERT_SQL,
                    params_from_iter(insert_params(row)),
                )?;
                Ok(())
            };

            if rows.is_empty() {
                let arrival = source.is_arrival();
                if arrival || live_hash == empty_hash {
                    // The arrival outfit (or, for a seat that predates the
                    // history, an empty outfit that is already the truth) is
                    // entry 0. v4's `buildRow` here carries NO actor and NO
                    // tool name — even for a `pascal` source (R-7, recorded).
                    let row = build_row(
                        chat_id,
                        character_id,
                        0,
                        &live,
                        if arrival {
                            source
                        } else {
                            WardrobeHistorySource::Seeded
                        },
                        None,
                        None,
                        &at,
                    );
                    insert(&row)?;
                    return Ok(SettleOutcome::Appended {
                        index: 0,
                        seeded: !arrival,
                    });
                }
                // A seat that predates the history and was never dressed: its
                // prior state was, by construction, nothing recorded. Seed an
                // empty 0 and record the change as 1.
                let mut seed = build_row(
                    chat_id,
                    character_id,
                    0,
                    &Slots::fresh(),
                    WardrobeHistorySource::Seeded,
                    None,
                    None,
                    &at,
                );
                seed.is_current = false;
                insert(&seed)?;
                let next = build_row(
                    chat_id,
                    character_id,
                    1,
                    &live,
                    source,
                    provenance.actor_character_id.clone(),
                    actor_tool_name.clone(),
                    &at,
                );
                insert(&next)?;
                return Ok(SettleOutcome::Appended {
                    index: 1,
                    seeded: true,
                });
            }

            let last = rows.last().map(|r| r.entry_index).unwrap_or(0);
            let current = rows
                .iter()
                .find(|r| r.is_current)
                .or(rows.last())
                .expect("rows is non-empty");
            // The STORED hash, never a recompute.
            if current.slots_hash == live_hash {
                return Ok(SettleOutcome::Unchanged {
                    index: current.entry_index,
                });
            }

            // max + 1, NOT current + 1: after a rollback the next change
            // lands past every recorded entry.
            let next_index = last + 1;
            conn.execute(
                &format!(
                    "UPDATE \"{CHAT_WARDROBE_HISTORY_TABLE}\" SET \"isCurrent\" = 0 WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 AND \"isCurrent\" = 1"
                ),
                params![chat_id, character_id],
            )?;
            let next = build_row(
                chat_id,
                character_id,
                next_index,
                &live,
                source,
                provenance.actor_character_id.clone(),
                actor_tool_name.clone(),
                &at,
            );
            insert(&next)?;
            Ok(SettleOutcome::Appended {
                index: next_index,
                seeded: false,
            })
        })?;

        match outcome {
            SettleOutcome::Unchanged { index } => tracing::debug!(
                target: "quilltap::wardrobe_history",
                module = "wardrobe-history",
                chatId = chat_id,
                characterId = character_id,
                source = source.as_str(),
                entryIndex = index,
                "Outfit unchanged since the current entry; nothing recorded"
            ),
            _ => tracing::debug!(
                target: "quilltap::wardrobe_history",
                module = "wardrobe-history",
                chatId = chat_id,
                characterId = character_id,
                source = source.as_str(),
                actorCharacterId = provenance.actor_character_id.as_deref().unwrap_or("null"),
                actorToolName = actor_tool_name.as_deref().unwrap_or("null"),
                outcomeJson = %serde_json::to_string(&outcome).unwrap_or_default(),
                slotsHash = live_hash.as_str(),
                "Recorded wardrobe history entry"
            ),
        }
        Ok(outcome)
    }

    /// v4 `settleCursor` (`:289-307`) — both UPDATEs even onto the
    /// already-current index.
    fn settle_cursor(
        &self,
        chat_id: &str,
        character_id: &str,
        index: i64,
    ) -> Result<SettleOutcome, DbError> {
        let outcome = in_savepoint(self.conn, SAVEPOINT, |conn| {
            let target: Option<Option<String>> = {
                let mut stmt = conn.prepare(&format!(
                    "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 AND \"entryIndex\" = ?3"
                ))?;
                let mut rows = stmt.query(params![chat_id, character_id, index])?;
                match rows.next()? {
                    Some(r) => Some(r.get::<_, Option<String>>("id")?),
                    None => None,
                }
            };
            let Some(target_id) = target else {
                return Ok(SettleOutcome::Skipped {
                    reason: "missing-index",
                });
            };
            conn.execute(
                &format!(
                    "UPDATE \"{CHAT_WARDROBE_HISTORY_TABLE}\" SET \"isCurrent\" = 0 WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 AND \"isCurrent\" = 1"
                ),
                params![chat_id, character_id],
            )?;
            conn.execute(
                &format!(
                    "UPDATE \"{CHAT_WARDROBE_HISTORY_TABLE}\" SET \"isCurrent\" = 1 WHERE \"id\" = ?1"
                ),
                params![target_id],
            )?;
            Ok(SettleOutcome::CursorMoved { index })
        })?;

        if matches!(outcome, SettleOutcome::Skipped { .. }) {
            tracing::warn!(
                target: "quilltap::wardrobe_history",
                module = "wardrobe-history",
                chatId = chat_id,
                characterId = character_id,
                index,
                "Wardrobe history cursor target missing; cursor left where it was"
            );
        } else {
            tracing::debug!(
                target: "quilltap::wardrobe_history",
                module = "wardrobe-history",
                chatId = chat_id,
                characterId = character_id,
                index,
                "Moved wardrobe history cursor"
            );
        }
        Ok(outcome)
    }

    // ------------------------------------------------------------------
    // Writes (restore / import / cleanup)
    // ------------------------------------------------------------------

    /// v4 `upsertRows(rows)` (`:330-361`) for TYPED rows (the import) — write
    /// rows as given, ids and indices preserved, `slots` normalized and the
    /// hash RECOMPUTED. Per row, NOT one transaction: a row that would break
    /// an invariant (a second current row, a duplicate index under another
    /// id, an FK miss) is logged at WARN (`Skipped a wardrobe history row that
    /// would break the timeline`) and skipped; DEBUG `Upserted wardrobe history
    /// rows` `{requested, written}` after the loop. Answers `written`; NEVER
    /// fails. (v4's backend ERROR `Raw query failed` before each WARN is the
    /// recorded NO-PORT class.)
    pub fn upsert_rows(&self, rows: &[ChatWardrobeHistoryRow]) -> usize {
        let raw: Vec<Value> = rows
            .iter()
            .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
            .collect();
        self.upsert_raw_rows(&raw)
    }

    /// The restore's UNVALIDATED form of [`Self::upsert_rows`] — v4's
    /// `upsertRows` fed an archive's rows as parsed JSON (v4's restore does not
    /// validate them either), with better-sqlite3's bind semantics PER ROW
    /// ([`js_bind`], measured): `slots` salvaged through
    /// `normalizeEquippedSlots` (a missing / non-object bag → empty),
    /// `slotsHash` RECOMPUTED, `isCurrent` by JS truthiness, `source` /
    /// `entryIndex` / the ids written AS GIVEN (a numeric `entryIndex` string
    /// lands as an integer through the column's affinity; an absent `id` binds
    /// NULL and v4's non-INTEGER primary key ADMITS it), `actorToolName`
    /// absent → NULL. A row that cannot bind or violates NOT NULL / UNIQUE /
    /// the FK → the same WARN (with whichever of `id`, `chatId`,
    /// `characterId`, `entryIndex` the row carries — v4 omits an `undefined`
    /// field) and skipped. `upsert_rows` writes through here — ONE per-row
    /// writer.
    pub fn upsert_raw_rows(&self, rows: &[Value]) -> usize {
        let sql = format!("{CHAT_WARDROBE_HISTORY_INSERT_SQL}{UPSERT_CONFLICT_SQL}");
        let mut written = 0usize;
        for row in rows {
            match self.upsert_one(&sql, row) {
                Ok(()) => written += 1,
                Err(error) => {
                    let field = |k: &str| match row.get(k) {
                        None => None,
                        Some(Value::String(s)) => Some(s.clone()),
                        Some(other) => Some(other.to_string()),
                    };
                    let [id, chat_id, character_id, entry_index] =
                        ["id", "chatId", "characterId", "entryIndex"].map(field);
                    tracing::warn!(
                        target: "quilltap::wardrobe_history",
                        module = "wardrobe-history",
                        id = id.as_deref(),
                        chatId = chat_id.as_deref(),
                        characterId = character_id.as_deref(),
                        entryIndex = entry_index.as_deref(),
                        error = %error,
                        "Skipped a wardrobe history row that would break the timeline"
                    );
                }
            }
        }
        tracing::debug!(
            target: "quilltap::wardrobe_history",
            module = "wardrobe-history",
            requested = rows.len(),
            written,
            "Upserted wardrobe history rows"
        );
        written
    }

    /// One row of [`Self::upsert_raw_rows`]: v4's `insertParams({ ...row,
    /// slots, slotsHash: hashEquippedSlots(slots) })` bound as better-sqlite3
    /// would. `Err` carries v4's `error.message`.
    fn upsert_one(&self, sql: &str, row: &Value) -> Result<(), String> {
        let get = |k: &str| row.get(k);
        let slots = Slots::from_value(Some(&normalize_equipped_slots(get("slots"))));
        fn or_null(v: Option<&Value>) -> Option<&Value> {
            v.filter(|v| !v.is_null())
        }
        let params: Vec<SqlValue> = vec![
            js_bind(get("id"))?,
            js_bind(get("chatId"))?,
            js_bind(get("characterId"))?,
            js_bind(get("entryIndex"))?,
            SqlValue::Text(slots.to_value().to_string()),
            SqlValue::Text(hash_equipped_slots(Some(&slots))),
            js_bind(get("source"))?,
            js_bind(or_null(get("actorCharacterId")))?,
            js_bind(or_null(get("actorToolName")))?,
            SqlValue::Integer(i64::from(js_truthy(get("isCurrent")))),
            js_bind(get("createdAt"))?,
        ];
        self.conn
            .execute(sql, params_from_iter(params))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// v4 `deleteByCharacter(characterId)` (`:364-367`) — character deleted:
    /// their timelines have no reader. DEBUG `Dropped wardrobe history for a
    /// deleted character`; a failure is `Err` (the cascade's catch logs it).
    pub fn delete_by_character(&self, character_id: &str) -> Result<usize, DbError> {
        let n = self.conn.execute(
            &format!("DELETE FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"characterId\" = ?1"),
            params![character_id],
        )?;
        tracing::debug!(
            target: "quilltap::wardrobe_history",
            module = "wardrobe-history",
            characterId = character_id,
            "Dropped wardrobe history for a deleted character"
        );
        Ok(n)
    }

    // ------------------------------------------------------------------
    // Reads (v4 `rawQuery` — a failure THROWS, no fallback)
    // ------------------------------------------------------------------

    fn query_rows(
        &self,
        sql: &str,
        args: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<ChatWardrobeHistoryRow>, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(args, normalize_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// v4 `findEntries(chatId, characterId)` — one character's timeline in a
    /// chat, ascending by index.
    pub fn find_entries(
        &self,
        chat_id: &str,
        character_id: &str,
    ) -> Result<Vec<ChatWardrobeHistoryRow>, DbError> {
        self.query_rows(
            &format!(
                "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 ORDER BY \"entryIndex\" ASC"
            ),
            &[&chat_id, &character_id],
        )
    }

    /// v4 `findCurrent(chatId, characterId)` — the entry the character is
    /// wearing now, or `None` when there is no history.
    pub fn find_current(
        &self,
        chat_id: &str,
        character_id: &str,
    ) -> Result<Option<ChatWardrobeHistoryRow>, DbError> {
        Ok(self
            .query_rows(
                &format!(
                    "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 AND \"characterId\" = ?2 AND \"isCurrent\" = 1 LIMIT 1"
                ),
                &[&chat_id, &character_id],
            )?
            .into_iter()
            .next())
    }

    /// v4 `findByChatId(chatId)` — every row for a chat (export, backup), by
    /// character then index.
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<Vec<ChatWardrobeHistoryRow>, DbError> {
        self.query_rows(
            &format!(
                "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" WHERE \"chatId\" = ?1 ORDER BY \"characterId\", \"entryIndex\" ASC"
            ),
            &[&chat_id],
        )
    }

    /// v4 `findAll()` — every row (backup).
    pub fn find_all(&self) -> Result<Vec<ChatWardrobeHistoryRow>, DbError> {
        self.query_rows(
            &format!(
                "SELECT * FROM \"{CHAT_WARDROBE_HISTORY_TABLE}\" ORDER BY \"chatId\", \"characterId\", \"entryIndex\" ASC"
            ),
            &[],
        )
    }
}

/// The after-commit form of [`ChatWardrobeHistoryRepository::settle`] (R-10):
/// ONE `db.write` around it on the writer's main connection. `Err` only when
/// the writer is gone — the caller's WARN (P4.D316's).
pub async fn settle_db(
    db: &super::runtime::Db,
    chat_id: &str,
    character_id: &str,
    intent: &WardrobeHistoryIntent,
) -> Result<SettleOutcome, DbError> {
    let (chat_id, character_id, intent) = (
        chat_id.to_string(),
        character_id.to_string(),
        intent.clone(),
    );
    db.write(move |writers| {
        Ok(
            ChatWardrobeHistoryRepository::new(writers.main().connection()).settle(
                &chat_id,
                &character_id,
                &intent,
            ),
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    //! v4's `chat-wardrobe-history.repository.test.ts` (`e8e17b2d9`), every
    //! `it` by name, over a bare in-memory `chats(id)` + the shared DDL, the
    //! live slots injected through [`ChatWardrobeHistoryRepository::
    //! settle_with_live`] (v4's fake `LiveOutfitReader`). Lines are pinned on
    //! the TEST thread (no writer involved).
    use super::*;
    use crate::test_support::global_capture::capture as captured_with;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const CHAT: &str = "c0c0c0c0-0000-4000-8000-000000000001";
    const CHAR_A: &str = "a0a0a0a0-0000-4000-8000-00000000000a";
    const CHAR_B: &str = "b0b0b0b0-0000-4000-8000-00000000000b";
    const DRESS: &str = "dress";
    const BOOTS: &str = "boots";
    const SCARF: &str = "scarf";

    struct Fixture {
        conn: Connection,
        live: RefCell<HashMap<String, Slots>>,
    }

    impl Fixture {
        fn new() -> Self {
            // The process-global rig (memory note `capture-rig-writer-thread`):
            // sibling tests reach the same callsites with no subscriber.
            crate::test_support::global_capture::install();
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY)")
                .unwrap();
            conn.execute("INSERT INTO \"chats\" VALUES (?1)", [CHAT])
                .unwrap();
            for sql in CHAT_WARDROBE_HISTORY_DDL {
                conn.execute_batch(sql).unwrap();
            }
            Fixture {
                conn,
                live: RefCell::new(HashMap::new()),
            }
        }
        fn set(&self, character_id: &str, slots: Slots) {
            self.live.borrow_mut().insert(character_id.into(), slots);
        }
        fn repo(&self) -> ChatWardrobeHistoryRepository<'_> {
            ChatWardrobeHistoryRepository::new(&self.conn)
        }
        fn settle(&self, character_id: &str, intent: &WardrobeHistoryIntent) -> SettleOutcome {
            let live = self.live.borrow().get(character_id).cloned();
            self.repo()
                .settle_with_live(CHAT, character_id, intent, || live.unwrap_or_default())
        }
        /// `(entryIndex, isCurrent, source, actorCharacterId, actorToolName,
        /// slots, slotsHash)` straight off the table.
        #[allow(clippy::type_complexity)]
        fn rows(
            &self,
            character_id: &str,
        ) -> Vec<(
            i64,
            i64,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
        )> {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT entryIndex, isCurrent, source, actorCharacterId, actorToolName, slots, slotsHash \
                     FROM chat_wardrobe_history WHERE characterId = ?1 ORDER BY entryIndex",
                )
                .unwrap();
            stmt.query_map([character_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
        }
        /// v4's `assertInvariants`: contiguous indices from 0, exactly one
        /// current, every `slotsHash` the hash of its `slots`.
        fn assert_invariants(&self, character_id: &str) {
            let all = self.rows(character_id);
            if all.is_empty() {
                return;
            }
            let indices: Vec<i64> = all.iter().map(|r| r.0).collect();
            assert_eq!(indices, (0..all.len() as i64).collect::<Vec<_>>());
            assert_eq!(all.iter().filter(|r| r.1 == 1).count(), 1);
            for r in &all {
                let slots = parse_slots(&r.5);
                assert_eq!(r.6, hash_equipped_slots(Some(&slots)));
            }
        }
    }

    fn slots(top: &[&str], footwear: &[&str], accessories: &[&str]) -> Slots {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        Slots {
            top: v(top),
            footwear: v(footwear),
            accessories: v(accessories),
            ..Slots::fresh()
        }
    }

    fn append(source: WardrobeHistorySource) -> WardrobeHistoryIntent {
        WardrobeHistoryIntent::Append {
            provenance: WardrobeHistoryProvenance::new(source),
        }
    }

    use WardrobeHistorySource::{ChatStart, Operator};

    #[test]
    fn records_an_arrival_outfit_as_entry_0_with_its_own_source() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        let outcome = f.settle(CHAR_A, &append(ChatStart));
        assert_eq!(
            outcome,
            SettleOutcome::Appended {
                index: 0,
                seeded: false
            }
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(rows[0].2, "chat-start");
        assert_eq!(rows[0].1, 1);
        assert_eq!(parse_slots(&rows[0].5).top, vec![DRESS.to_string()]);
    }

    #[test]
    fn records_a_naked_arrival_as_entry_0() {
        let f = Fixture::new();
        assert_eq!(
            f.settle(CHAR_A, &append(ChatStart)),
            SettleOutcome::Appended {
                index: 0,
                seeded: false
            }
        );
        assert_eq!(f.rows(CHAR_A).len(), 1);
    }

    #[test]
    fn seeds_an_empty_0_alone_when_a_pre_history_seat_is_still_undressed() {
        let f = Fixture::new();
        assert_eq!(
            f.settle(CHAR_A, &append(Operator)),
            SettleOutcome::Appended {
                index: 0,
                seeded: true
            }
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(rows[0].2, "seeded");
        assert_eq!(rows[0].1, 1);
    }

    #[test]
    fn seeds_an_empty_0_then_records_the_change_as_1_when_a_pre_history_seat_gets_dressed() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        let intent = WardrobeHistoryIntent::Append {
            provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Character)
                .with_actor(CHAR_A),
        };
        assert_eq!(
            f.settle(CHAR_A, &intent),
            SettleOutcome::Appended {
                index: 1,
                seeded: true
            }
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(rows[0].2, "seeded");
        assert!(parse_slots(&rows[0].5).top.is_empty());
        assert_eq!(rows[0].1, 0);
        assert_eq!(rows[1].2, "character");
        assert_eq!(rows[1].3.as_deref(), Some(CHAR_A));
        assert_eq!(rows[1].1, 1);
        f.assert_invariants(CHAR_A);
    }

    #[test]
    fn records_the_custom_tool_behind_a_pascal_change_and_no_tool_for_anyone_else() {
        let f = Fixture::new();
        f.settle(CHAR_A, &append(ChatStart));
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(
            CHAR_A,
            &WardrobeHistoryIntent::Append {
                provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Pascal)
                    .with_actor(CHAR_A)
                    .with_tool_name("dress_for_dinner"),
            },
        );
        f.set(CHAR_A, Slots::fresh());
        f.settle(
            CHAR_A,
            &WardrobeHistoryIntent::Append {
                provenance: WardrobeHistoryProvenance::new(Operator)
                    .with_actor(CHAR_B)
                    .with_tool_name("ignored"),
            },
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(rows[0].4, None);
        assert_eq!(rows[1].2, "pascal");
        assert_eq!(rows[1].4.as_deref(), Some("dress_for_dinner"));
        assert_eq!(rows[2].2, "operator");
        assert_eq!(rows[2].3.as_deref(), Some(CHAR_B));
        assert_eq!(rows[2].4, None);
        let read_back = f.repo().find_entries(CHAT, CHAR_A).unwrap();
        assert_eq!(
            read_back[1].actor_tool_name.as_deref(),
            Some("dress_for_dinner")
        );
        f.assert_invariants(CHAR_A);
    }

    /// R-7 (recorded, NOT fixed): the empty-timeline ONE-row arm drops the
    /// actor AND the tool name, even for a `pascal` provenance.
    #[test]
    fn the_one_row_arm_drops_the_actor_and_the_tool_name() {
        let f = Fixture::new();
        let outcome = f.settle(
            CHAR_A,
            &WardrobeHistoryIntent::Append {
                provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Pascal)
                    .with_actor(CHAR_B)
                    .with_tool_name("dress_for_dinner"),
            },
        );
        assert_eq!(
            outcome,
            SettleOutcome::Appended {
                index: 0,
                seeded: true
            }
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(
            (rows[0].2.as_str(), &rows[0].3, &rows[0].4),
            ("seeded", &None, &None)
        );
        // An arrival keeps its own source but writes no actor either.
        let f = Fixture::new();
        f.settle(
            CHAR_A,
            &WardrobeHistoryIntent::Append {
                provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::ParticipantAdded)
                    .with_actor(CHAR_B),
            },
        );
        let rows = f.rows(CHAR_A);
        assert_eq!(
            (rows[0].2.as_str(), &rows[0].3),
            ("participant-added", &None)
        );
    }

    #[test]
    fn is_a_no_op_when_the_live_outfit_equals_the_current_entry() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        let (outcome, lines) = captured_with(|| f.settle(CHAR_A, &append(Operator)));
        assert_eq!(outcome, SettleOutcome::Unchanged { index: 0 });
        assert_eq!(f.rows(CHAR_A).len(), 1);
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::wardrobe_history Outfit unchanged since the current entry; nothing recorded module=wardrobe-history chatId={CHAT} characterId={CHAR_A} source=operator entryIndex=0"
            )]
        );
    }

    #[test]
    fn appends_max_plus_1_after_a_rollback_moving_the_cursor_there() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.set(CHAR_A, slots(&[DRESS], &[BOOTS], &[]));
        f.settle(CHAR_A, &append(Operator));
        f.set(CHAR_A, slots(&[], &[BOOTS], &[]));
        f.settle(CHAR_A, &append(Operator));
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        assert_eq!(
            f.settle(CHAR_A, &WardrobeHistoryIntent::Cursor { index: 0 }),
            SettleOutcome::CursorMoved { index: 0 }
        );
        assert_eq!(
            f.rows(CHAR_A).iter().map(|r| r.1).collect::<Vec<_>>(),
            vec![1, 0, 0]
        );
        f.set(CHAR_A, slots(&[DRESS], &[], &[SCARF]));
        assert_eq!(
            f.settle(CHAR_A, &append(Operator)),
            SettleOutcome::Appended {
                index: 3,
                seeded: false
            }
        );
        assert_eq!(
            f.rows(CHAR_A).iter().map(|r| r.1).collect::<Vec<_>>(),
            vec![0, 0, 0, 1]
        );
        f.assert_invariants(CHAR_A);
    }

    #[test]
    fn appends_even_when_the_live_outfit_matches_an_older_non_current_entry() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.set(CHAR_A, slots(&[], &[BOOTS], &[]));
        f.settle(CHAR_A, &append(Operator));
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        assert_eq!(
            f.settle(CHAR_A, &append(Operator)),
            SettleOutcome::Appended {
                index: 2,
                seeded: false
            }
        );
        f.assert_invariants(CHAR_A);
    }

    #[test]
    fn moves_the_cursor_without_appending_forwards_as_well_as_back() {
        let f = Fixture::new();
        for s in [
            slots(&[DRESS], &[], &[]),
            slots(&[], &[BOOTS], &[]),
            slots(&[], &[], &[SCARF]),
        ] {
            f.set(CHAR_A, s);
            f.settle(CHAR_A, &append(Operator));
        }
        let (_, lines) =
            captured_with(|| f.settle(CHAR_A, &WardrobeHistoryIntent::Cursor { index: 1 }));
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::wardrobe_history Moved wardrobe history cursor module=wardrobe-history chatId={CHAT} characterId={CHAR_A} index=1"
            )]
        );
        assert_eq!(
            f.repo()
                .find_current(CHAT, CHAR_A)
                .unwrap()
                .unwrap()
                .entry_index,
            1
        );
        f.settle(CHAR_A, &WardrobeHistoryIntent::Cursor { index: 2 });
        assert_eq!(
            f.repo()
                .find_current(CHAT, CHAR_A)
                .unwrap()
                .unwrap()
                .entry_index,
            2
        );
        // Onto the already-current index: both UPDATEs, `cursor-moved`.
        assert_eq!(
            f.settle(CHAR_A, &WardrobeHistoryIntent::Cursor { index: 2 }),
            SettleOutcome::CursorMoved { index: 2 }
        );
        assert_eq!(f.rows(CHAR_A).len(), 4);
        f.assert_invariants(CHAR_A);
    }

    #[test]
    fn leaves_the_cursor_alone_for_a_missing_index_without_throwing() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        let (outcome, lines) =
            captured_with(|| f.settle(CHAR_A, &WardrobeHistoryIntent::Cursor { index: 7 }));
        assert_eq!(
            outcome,
            SettleOutcome::Skipped {
                reason: "missing-index"
            }
        );
        assert_eq!(
            lines,
            vec![format!(
                "WARN quilltap::wardrobe_history Wardrobe history cursor target missing; cursor left where it was module=wardrobe-history chatId={CHAT} characterId={CHAR_A} index=7"
            )]
        );
        assert_eq!(
            f.repo()
                .find_current(CHAT, CHAR_A)
                .unwrap()
                .unwrap()
                .entry_index,
            0
        );
    }

    /// v4 `never throws: a database failure is logged and swallowed` — v5's
    /// analogue is a dropped table.
    #[test]
    fn never_throws_a_database_failure_is_logged_and_swallowed() {
        let f = Fixture::new();
        f.conn
            .execute_batch("DROP TABLE chat_wardrobe_history")
            .unwrap();
        let intent = WardrobeHistoryIntent::Append {
            provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Pascal)
                .with_tool_name("t"),
        };
        let (outcome, lines) = captured_with(|| f.settle(CHAR_A, &intent));
        assert_eq!(outcome, SettleOutcome::Skipped { reason: "error" });
        assert_eq!(
            lines,
            vec![format!(
                "WARN quilltap::wardrobe_history Wardrobe history settle failed; the outfit change stands unrecorded module=wardrobe-history chatId={CHAT} characterId={CHAR_A} intentJson={{\"kind\":\"append\",\"provenance\":{{\"source\":\"pascal\",\"toolName\":\"t\"}}}} error=no such table: chat_wardrobe_history"
            )]
        );
    }

    /// R-10: ONE savepoint opened by the test as a "caller", a caller write,
    /// then a settle that fails (the table dropped mid-closure) → the
    /// caller's write COMMITS, the outcome `Skipped`.
    #[test]
    fn a_failed_settle_never_rolls_back_the_callers_writes() {
        let f = Fixture::new();
        f.conn.execute_batch("BEGIN").unwrap();
        f.conn
            .execute("INSERT INTO chats VALUES ('caller-write')", [])
            .unwrap();
        f.conn
            .execute_batch("DROP TABLE chat_wardrobe_history")
            .unwrap();
        assert_eq!(
            f.settle(CHAR_A, &append(Operator)),
            SettleOutcome::Skipped { reason: "error" }
        );
        f.conn.execute_batch("COMMIT").unwrap();
        let n: i64 = f
            .conn
            .query_row(
                "SELECT count(*) FROM chats WHERE id = 'caller-write'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        // And a successful settle inside the caller's transaction commits with it.
        let f = Fixture::new();
        f.conn.execute_batch("BEGIN").unwrap();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.conn.execute_batch("ROLLBACK").unwrap();
        assert!(
            f.rows(CHAR_A).is_empty(),
            "the savepoint nests in the caller's transaction"
        );
    }

    /// A settle on a chat id that does not exist: the FK refuses the INSERT
    /// (`foreign_keys = ON`) → WARN → `skipped`.
    #[test]
    fn a_settle_on_a_missing_chat_is_skipped() {
        let f = Fixture::new();
        f.conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        let (outcome, lines) = captured_with(|| {
            f.repo()
                .settle_with_live("no-such-chat", CHAR_A, &append(Operator), Slots::fresh)
        });
        assert_eq!(outcome, SettleOutcome::Skipped { reason: "error" });
        assert!(
            lines[0].ends_with("error=FOREIGN KEY constraint failed"),
            "{lines:?}"
        );
    }

    #[test]
    fn keeps_each_character_on_their_own_timeline() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.set(CHAR_B, slots(&[], &[BOOTS], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.settle(CHAR_B, &append(ChatStart));
        assert_eq!(f.rows(CHAR_A).len(), 1);
        assert_eq!(f.rows(CHAR_B).len(), 1);
        f.assert_invariants(CHAR_A);
        f.assert_invariants(CHAR_B);
    }

    /// v4 `upsertRows › round-trips rows whole, and skips one that would make
    /// a second current row`.
    #[test]
    fn upsert_rows_round_trips_rows_whole_and_skips_one_that_would_make_a_second_current_row() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.set(CHAR_A, slots(&[], &[BOOTS], &[]));
        f.settle(CHAR_A, &append(Operator));
        let exported = f.repo().find_by_chat_id(CHAT).unwrap();
        f.conn
            .execute_batch("DELETE FROM chat_wardrobe_history")
            .unwrap();
        let mut bogus = exported[0].clone();
        bogus.id = "99999999-9999-4999-8999-999999999999".into();
        bogus.entry_index = 5;
        bogus.is_current = true;
        let mut all = exported.clone();
        all.push(bogus);
        let (written, lines) = captured_with(|| f.repo().upsert_rows(&all));
        assert_eq!(written, 2);
        assert_eq!(f.repo().find_by_chat_id(CHAT).unwrap(), exported);
        assert_eq!(
            lines,
            vec![
                format!(
                    "WARN quilltap::wardrobe_history Skipped a wardrobe history row that would break the timeline module=wardrobe-history id=99999999-9999-4999-8999-999999999999 chatId={CHAT} characterId={CHAR_A} entryIndex=5 error=UNIQUE constraint failed: chat_wardrobe_history.chatId, chat_wardrobe_history.characterId"
                ),
                "DEBUG quilltap::wardrobe_history Upserted wardrobe history rows module=wardrobe-history requested=3 written=2".to_string(),
            ]
        );
        f.assert_invariants(CHAR_A);
    }

    #[test]
    fn delete_by_character_drops_only_that_character() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.set(CHAR_B, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.settle(CHAR_B, &append(ChatStart));
        let (n, lines) = captured_with(|| f.repo().delete_by_character(CHAR_A).unwrap());
        assert_eq!(n, 1);
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::wardrobe_history Dropped wardrobe history for a deleted character module=wardrobe-history characterId={CHAR_A}"
            )]
        );
        assert!(f.rows(CHAR_A).is_empty());
        assert_eq!(f.rows(CHAR_B).len(), 1);
    }

    #[test]
    fn cascades_with_the_chat() {
        let f = Fixture::new();
        f.conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        f.settle(CHAR_A, &append(ChatStart));
        f.conn
            .execute("DELETE FROM chats WHERE id = ?1", [CHAT])
            .unwrap();
        assert!(f.rows(CHAR_A).is_empty());
    }

    /// The `Recorded wardrobe history entry` DEBUG — v4's field order, a
    /// `null` actor rendered `null`, the outcome as `outcomeJson`.
    #[test]
    fn the_recorded_debug_line() {
        let f = Fixture::new();
        f.set(CHAR_A, slots(&[DRESS], &[], &[]));
        let (_, lines) = captured_with(|| f.settle(CHAR_A, &append(ChatStart)));
        let hash = hash_equipped_slots(Some(&slots(&[DRESS], &[], &[])));
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::wardrobe_history Recorded wardrobe history entry module=wardrobe-history chatId={CHAT} characterId={CHAR_A} source=chat-start actorCharacterId=null actorToolName=null outcomeJson={{\"kind\":\"appended\",\"entryIndex\":0,\"seeded\":false}} slotsHash={hash}"
            )]
        );
    }

    /// The wire shapes: the intent's and the outcome's v4 object bytes, and
    /// the row's FIELD ORDER (C1 §2 — `actorToolName` before `isCurrent`).
    #[test]
    fn the_wire_bytes() {
        let append = WardrobeHistoryIntent::Append {
            provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Pascal)
                .with_actor("x")
                .with_tool_name("t"),
        };
        assert_eq!(
            serde_json::to_string(&append).unwrap(),
            r#"{"kind":"append","provenance":{"source":"pascal","actorCharacterId":"x","toolName":"t"}}"#
        );
        assert_eq!(
            serde_json::to_string(&WardrobeHistoryIntent::Append {
                provenance: WardrobeHistoryProvenance::new(Operator)
            })
            .unwrap(),
            r#"{"kind":"append","provenance":{"source":"operator"}}"#
        );
        assert_eq!(
            serde_json::to_string(&WardrobeHistoryIntent::Cursor { index: 3 }).unwrap(),
            r#"{"kind":"cursor","index":3}"#
        );
        for (o, want) in [
            (
                SettleOutcome::Appended {
                    index: 1,
                    seeded: true,
                },
                r#"{"kind":"appended","entryIndex":1,"seeded":true}"#,
            ),
            (
                SettleOutcome::Unchanged { index: 2 },
                r#"{"kind":"unchanged","entryIndex":2}"#,
            ),
            (
                SettleOutcome::CursorMoved { index: 0 },
                r#"{"kind":"cursor-moved","entryIndex":0}"#,
            ),
            (
                SettleOutcome::Skipped { reason: "error" },
                r#"{"kind":"skipped","reason":"error"}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&o).unwrap(), want);
        }
        let row = ChatWardrobeHistoryRow {
            id: "i".into(),
            chat_id: "c".into(),
            character_id: "a".into(),
            entry_index: 0,
            slots: Slots::fresh(),
            slots_hash: "h".into(),
            source: "seeded".into(),
            actor_character_id: None,
            actor_tool_name: None,
            is_current: true,
            created_at: "t".into(),
        };
        assert_eq!(
            serde_json::to_string(&row).unwrap(),
            r#"{"id":"i","chatId":"c","characterId":"a","entryIndex":0,"slots":{"top":[],"bottom":[],"footwear":[],"accessories":[],"hair":[]},"slotsHash":"h","source":"seeded","actorCharacterId":null,"actorToolName":null,"isCurrent":true,"createdAt":"t"}"#
        );
    }

    /// `normalizeRow`: an unknown stored `source` reads `seeded`; an
    /// `isCurrent` of `2` reads `false`; the STORED hash passes through.
    #[test]
    fn normalize_row_reads_like_v4() {
        let f = Fixture::new();
        f.conn
            .execute(
                "INSERT INTO chat_wardrobe_history VALUES ('r', ?1, ?2, 0, 'not json', 'stale', 'bogus', NULL, 2, 't', NULL)",
                params![CHAT, CHAR_A],
            )
            .unwrap();
        let rows = f.repo().find_all().unwrap();
        assert_eq!(rows[0].source, "seeded");
        assert!(!rows[0].is_current);
        assert_eq!(rows[0].slots_hash, "stale");
        assert_eq!(rows[0].slots, Slots::fresh());
    }
}
