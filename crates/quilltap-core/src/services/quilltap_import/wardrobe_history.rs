//! Phase 7f — the outfit history (v4 `lib/import/quilltap-import/
//! import-wardrobe-history.ts`, `7953da3a0` + `4ae20977f`; P4.D317).
//!
//! v4's header, carried: bring a bundle's `wardrobe_history` rows across, keyed
//! to the chats and characters as they landed on this instance. Runs after
//! characters, chats, document stores and the wear ledger, so every id map it
//! reads is final. Each row gets a fresh `id`; `chatId`, `characterId` and
//! `actorCharacterId` are remapped; `entryIndex`, `isCurrent`, `slots`,
//! `actorToolName` and `createdAt` are copied as they are, so the timeline and
//! its cursor survive the trip. An item id in `slots` is remapped when its item
//! imported with a new id and otherwise kept — one that resolves to nothing
//! here falls under the stale-item rule at rollback and render time.
//!
//! A timeline that already has rows on this instance (the chat was skipped as a
//! duplicate, or the bundle is imported twice) is left alone: history is never
//! merged into a live timeline. A chat imported without history rows (an older
//! bundle) gets its entry 0 from the runtime first-touch seed.
//!
//! ⚠ The never-merge check guards only a NON-EMPTY local timeline (P4.D317
//! R-9 (c), v4-faithful — a filing candidate): a bundle chat skipped as a
//! duplicate has no `idMaps.chats` entry, so its rows keep the SOURCE `chatId`;
//! a local chat with that id and an empty timeline RECEIVES them, and with no
//! such chat every row fails the FK (the repository's per-row skip).
//!
//! The pure half ([`parse_wardrobe_history_row`], [`remap_wardrobe_history_rows`])
//! takes its id source by injection, as v4's `WardrobeHistoryRemapContext` does.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::Value;

use super::wardrobe_wear::build_imported_wardrobe_item_id_map;
use super::IdMaps;
use crate::api::zod_issues::{zod_iso_datetime_ok, zod_uuid_ok};
use crate::db::chat_wardrobe_history::{ChatWardrobeHistoryRepository, ChatWardrobeHistoryRow};
use crate::db::DbError;
use crate::wardrobe::{hash_equipped_slots, Slots, WARDROBE_SLOT_TYPES};

/// v4 `WardrobeHistorySourceSchema` — the seven values a row may carry.
const SOURCES: [&str; 7] = [
    "chat-start",
    "participant-added",
    "merge",
    "operator",
    "character",
    "pascal",
    "seeded",
];

/// v4 `ChatWardrobeHistoryRowSchema.safeParse(raw)` (`chat-wardrobe-history.
/// types.ts:37-66`), whole-row (P4.161's idiom): `id` / `chatId` `z.uuid()`;
/// `characterId` a string of length ≥ 1; `entryIndex` a SAFE integer ≥ 0;
/// `slots` an OBJECT (required) whose five slots are each `z.array(z.uuid())
/// .default([])` — absent reads `[]`, a non-array or a non-UUID element fails
/// the ROW; `slotsHash` a string of exactly 16 code points (Zod 4.6.7's
/// measure); `source` the
/// seven-value enum; `actorCharacterId` a string or `null` (ABSENT fails);
/// `actorToolName` a string or `null`, absent → `null` (`.default(null)` —
/// "so bundles and backups from before the column still parse"); `isCurrent`
/// strictly boolean; `createdAt` an ISO datetime. Unknown keys stripped. Only
/// pass / fail matters: v4 counts a failure as `droppedInvalid`.
pub fn parse_wardrobe_history_row(raw: &Value) -> Option<ChatWardrobeHistoryRow> {
    let obj = raw.as_object()?;
    let string = |k: &str| obj.get(k).and_then(Value::as_str);
    let nullable = |v: Option<&Value>| -> Option<Option<String>> {
        match v {
            Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(Some(s.clone())),
            _ => None,
        }
    };
    let id = string("id").filter(|s| zod_uuid_ok(s))?.to_string();
    let chat_id = string("chatId").filter(|s| zod_uuid_ok(s))?.to_string();
    let character_id = string("characterId")
        .filter(|s| s.encode_utf16().count() >= 1)?
        .to_string();
    let entry_index = match obj.get("entryIndex")? {
        Value::Number(n) => {
            let f = n.as_f64()?;
            // `z.number().int().nonnegative()` — `int()` is a SAFE integer.
            if f.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&f) {
                return None;
            }
            f as i64
        }
        _ => return None,
    };
    let slots_obj = obj.get("slots")?.as_object()?;
    let mut slots = Slots::fresh();
    for slot in WARDROBE_SLOT_TYPES {
        match slots_obj.get(slot) {
            None => {}
            Some(Value::Array(ids)) => {
                let target = slots.slot_mut(slot);
                for id in ids {
                    target.push(id.as_str().filter(|s| zod_uuid_ok(s))?.to_string());
                }
            }
            Some(_) => return None,
        }
    }
    // `z.string().length(16)` — Zod 4.6.7 counts CODE POINTS (its
    // `$ZodCheckLengthEquals` measures `util.codePointLength` whenever the
    // UTF-16 length could straddle the target; measured: eight astral
    // characters — sixteen UTF-16 units — FAIL).
    let slots_hash = string("slotsHash")
        .filter(|s| s.chars().count() == 16)?
        .to_string();
    let source = string("source")
        .filter(|s| SOURCES.contains(s))?
        .to_string();
    let actor_character_id = nullable(obj.get("actorCharacterId"))?;
    let actor_tool_name = match obj.get("actorToolName") {
        None => None,
        present => nullable(present)?,
    };
    let is_current = obj.get("isCurrent")?.as_bool()?;
    let created_at = string("createdAt")
        .filter(|s| zod_iso_datetime_ok(s))?
        .to_string();
    Some(ChatWardrobeHistoryRow {
        id,
        chat_id,
        character_id,
        entry_index,
        slots,
        slots_hash,
        source,
        actor_character_id,
        actor_tool_name,
        is_current,
        created_at,
    })
}

/// v4 `WardrobeHistoryRemapContext` (`:40-45`).
pub struct HistoryRemapContext<'a> {
    pub chat_ids: &'a HashMap<String, String>,
    pub character_ids: &'a HashMap<String, String>,
    pub item_ids: &'a HashMap<String, String>,
    pub mint_id: &'a mut dyn FnMut() -> String,
}

/// v4 `WardrobeHistoryRemapResult` (`:47-50`).
#[derive(Debug, Default, PartialEq)]
pub struct HistoryRemapResult {
    pub rows: Vec<ChatWardrobeHistoryRow>,
    pub dropped_invalid: usize,
}

/// v4 `remapWardrobeHistoryRows(incoming, ctx)` (`:53-81`) — "Remap a bundle's
/// history rows onto this instance's ids. Pure." Per VALID row, in input
/// order (the mint order): slots through `item_ids` (a miss keeps the id), a
/// fresh `id`, `chatId` / `characterId` through their maps or kept, the actor
/// through the character map — JS truthiness, so `""` becomes `null` — and the
/// hash recomputed.
pub fn remap_wardrobe_history_rows(
    incoming: &[Value],
    ctx: &mut HistoryRemapContext,
) -> HistoryRemapResult {
    let mut out = HistoryRemapResult::default();
    for raw in incoming {
        let Some(row) = parse_wardrobe_history_row(raw) else {
            out.dropped_invalid += 1;
            continue;
        };
        let mut slots = Slots::fresh();
        for slot in WARDROBE_SLOT_TYPES {
            *slots.slot_mut(slot) = row
                .slots
                .slot(slot)
                .iter()
                .map(|id| ctx.item_ids.get(id).cloned().unwrap_or_else(|| id.clone()))
                .collect();
        }
        let mapped = |map: &HashMap<String, String>, id: &str| {
            map.get(id).cloned().unwrap_or_else(|| id.to_string())
        };
        let id = (ctx.mint_id)();
        let actor_character_id = row
            .actor_character_id
            .as_deref()
            .filter(|a| !a.is_empty())
            .map(|a| mapped(ctx.character_ids, a));
        out.rows.push(ChatWardrobeHistoryRow {
            id,
            chat_id: mapped(ctx.chat_ids, &row.chat_id),
            character_id: mapped(ctx.character_ids, &row.character_id),
            actor_character_id,
            slots_hash: hash_equipped_slots(Some(&slots)),
            slots,
            ..row
        });
    }
    out
}

/// v4 `importWardrobeHistory(incoming, documents, idMaps, warnings)`
/// (`:83-127`) — "Import the bundle's history rows. Returns the number
/// written." `preserveIds` plays no part: history ids are always fresh. The
/// ONE warning is `Dropped N malformed outfit-history row(s).`; the
/// never-merge check reads each (chat, character) pair's timeline ONCE, in
/// first-seen pair order, and the write order is GROUPED BY PAIR (not input
/// order, where pairs interleave); `upsert_rows` runs ALWAYS (`[]` included —
/// its DEBUG fires); v4's INFO `Imported outfit history` carries its four
/// keys. A failed read propagates — the phase's catch is the caller's.
pub(crate) fn import_wardrobe_history(
    main: &Connection,
    incoming: &[Value],
    documents: &[Value],
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> Result<usize, DbError> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let pairs = |m: &super::IdMap| -> Vec<(String, String)> {
        m.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    let to_map = |m: &super::IdMap| -> HashMap<String, String> { pairs(m).into_iter().collect() };
    let item_ids: HashMap<String, String> = build_imported_wardrobe_item_id_map(
        documents,
        &pairs(&id_maps.mount_points),
        &pairs(&id_maps.wardrobe_items),
    )
    .into_iter()
    .collect();
    let (chat_ids, character_ids) = (to_map(&id_maps.chats), to_map(&id_maps.characters));
    let mut mint = || uuid::Uuid::new_v4().to_string();
    let result = remap_wardrobe_history_rows(
        incoming,
        &mut HistoryRemapContext {
            chat_ids: &chat_ids,
            character_ids: &character_ids,
            item_ids: &item_ids,
            mint_id: &mut mint,
        },
    );
    if result.dropped_invalid > 0 {
        warnings.push(format!(
            "Dropped {} malformed outfit-history row(s).",
            result.dropped_invalid
        ));
    }

    // "Never merge into a live timeline." v4's `Map` keyed
    // `${chatId}\u0000${characterId}` — pairs in FIRST-SEEN order.
    let mut by_pair: Vec<(String, Vec<ChatWardrobeHistoryRow>)> = Vec::new();
    for row in result.rows {
        let key = format!("{}\u{0}{}", row.chat_id, row.character_id);
        match by_pair.iter_mut().find(|(k, _)| *k == key) {
            Some((_, rows)) => rows.push(row),
            None => by_pair.push((key, vec![row])),
        }
    }
    let repo = ChatWardrobeHistoryRepository::new(main);
    let mut to_write: Vec<ChatWardrobeHistoryRow> = Vec::new();
    let mut skipped_timelines = 0usize;
    for (_, pair_rows) in by_pair {
        let first = &pair_rows[0];
        if !repo
            .find_entries(&first.chat_id, &first.character_id)?
            .is_empty()
        {
            skipped_timelines += 1;
            continue;
        }
        to_write.extend(pair_rows);
    }

    let written = repo.upsert_rows(&to_write);
    tracing::info!(
        target: "quilltap::import::wardrobe_history",
        module = "import:wardrobe-history",
        incoming = incoming.len(),
        written,
        droppedInvalid = result.dropped_invalid,
        skippedTimelines = skipped_timelines,
        "Imported outfit history"
    );
    Ok(written)
}
