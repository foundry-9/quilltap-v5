//! Phase 7e — the wardrobe wear ledger (v4 `lib/import/quilltap-import/
//! import-wardrobe-wear.ts`, `3ee3b1342` #81, unchanged through `f5e953a3f`;
//! P4.D264).
//!
//! v4's header, carried: a `.qtap` bundle carries the ledger rows for every
//! wardrobe item it exports. On the way in each row's item, wearer and chat id
//! is remapped onto this instance; a row whose item did not come along is
//! dropped; a wearer that cannot be resolved folds into the item's
//! unattributed row; a chat that cannot be resolved is cleared. An existing
//! tally is never rewound — a key with a live row takes the LARGER count
//! (MAX, not SUM), so a second import of the same bundle changes nothing.
//! Rows that collapse onto one key WITHIN the bundle are summed.
//!
//! The pure half ([`remap_wardrobe_wear_rows`], [`build_imported_wardrobe_item_
//! id_map`]) takes its clock and id source by injection, as v4's
//! `WardrobeWearRemapContext` does.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use super::IdMaps;
use crate::api::zod_issues::{zod_iso_datetime_ok, zod_uuid_ok};
use crate::db::wardrobe_wear_stats::{WardrobeWearStatsRepository, WardrobeWearStatsRow};
use crate::db::DbError;
use crate::vault_overlay::{is_wardrobe_item_document_path, wardrobe_item_id_for_document};

/// v4 `WardrobeWearRemapContext` (`:35-51`).
pub struct RemapContext<'a> {
    /// Source item id → destination item id; a row whose item is absent is dropped.
    pub item_ids: &'a HashMap<String, String>,
    /// Source character id → destination id, or `None` to fold into unattributed.
    pub resolve_wearer: &'a dyn Fn(&str) -> Option<String>,
    /// Source chat id → destination id, or `None` to clear the pointer.
    pub resolve_chat: &'a dyn Fn(&str) -> Option<String>,
    /// Live rows already on this instance for the DESTINATION items.
    pub existing: &'a [WardrobeWearStatsRow],
    pub mint_id: &'a mut dyn FnMut() -> String,
    pub now: &'a str,
}

/// v4 `WardrobeWearRemapResult` (`:53-64`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RemapResult {
    pub rows: Vec<WardrobeWearStatsRow>,
    pub dropped_missing_item: usize,
    pub dropped_invalid: usize,
    pub folded_wearers: usize,
    pub cleared_chats: usize,
}

/// v4 `keyOf` (`:66-68`) — `${itemId}\u0000${wearer ?? ''}`.
fn key_of(item_id: &str, wearer: Option<&str>) -> String {
    format!("{item_id}\u{0}{}", wearer.unwrap_or(""))
}

/// v4 `laterLastWear(a, b)` (`:71-78`): "Take `b`'s last wear (and its chat)
/// when it is later than `a`'s; `a` wins ties." A string compare, strict `>`.
fn later_last_wear(a: (&str, Option<&str>), b: (&str, Option<&str>)) -> (String, Option<String>) {
    let (pick_at, pick_chat) = if b.0 > a.0 { b } else { a };
    (pick_at.to_string(), pick_chat.map(str::to_string))
}

/// v4 `minIso` (`:80-82`) — the lexical minimum (`b < a ? b : a`).
fn min_iso(a: &str, b: &str) -> String {
    if b < a { b } else { a }.to_string()
}

/// v4 `WardrobeWearStatsRowSchema.safeParse(raw)` (`wardrobe-wear.types.ts:
/// 24-37`), whole-row (P4.161's idiom): `id` `z.uuid()`, `itemId` a string of
/// length ≥ 1, `wearerCharacterId` / `lastWornChatId` a string or `null`
/// (`.nullable()` — ABSENT fails), `wearCount` an integer ≥ 0, the four stamps
/// `TimestampSchema` (an ISO datetime string). Only pass / fail matters here:
/// v4 counts a failure as `droppedInvalid` and keeps going. A passing row comes
/// back as Zod's OUTPUT (unknown keys stripped).
pub fn parse_wardrobe_wear_row(raw: &Value) -> Option<WardrobeWearStatsRow> {
    let obj = raw.as_object()?;
    let string = |k: &str| obj.get(k).and_then(Value::as_str);
    let nullable = |k: &str| -> Option<Option<String>> {
        match obj.get(k) {
            Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(Some(s.clone())),
            _ => None,
        }
    };
    let stamp = |k: &str| {
        string(k)
            .filter(|s| zod_iso_datetime_ok(s))
            .map(str::to_string)
    };
    let id = string("id").filter(|s| zod_uuid_ok(s))?.to_string();
    let item_id = string("itemId")
        .filter(|s| s.encode_utf16().count() >= 1)?
        .to_string();
    let wearer_character_id = nullable("wearerCharacterId")?;
    let wear_count = match obj.get("wearCount")? {
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
    let first_worn_at = stamp("firstWornAt")?;
    let last_worn_at = stamp("lastWornAt")?;
    let last_worn_chat_id = nullable("lastWornChatId")?;
    let created_at = stamp("createdAt")?;
    let updated_at = stamp("updatedAt")?;
    Some(WardrobeWearStatsRow {
        id,
        item_id,
        wearer_character_id,
        wear_count,
        first_worn_at,
        last_worn_at,
        last_worn_chat_id,
        created_at,
        updated_at,
    })
}

/// v4 `remapWardrobeWearRows(incoming, ctx)` (`:102-185`) — "Remap a bundle's
/// wear-ledger rows onto this instance. `itemId` goes through `ctx.itemIds`; a
/// row whose item did not import is dropped. `wearerCharacterId` goes through
/// `ctx.resolveWearer`; a wearer that does not resolve folds into the item's
/// unattributed row (`null`). `lastWornChatId` goes through `ctx.resolveChat`;
/// an unresolved chat is cleared. Rows that collapse onto one (item, wearer)
/// key — several unknown wearers folding into unattributed, say — are summed
/// (earliest first wear, latest last wear and its chat), because `upsertRows`
/// replaces a tally on collision rather than adding to it. A key that already
/// has a live row keeps that row's id and creation stamp and takes the larger
/// count. Every new row gets a freshly minted id."
pub fn remap_wardrobe_wear_rows(incoming: &[Value], ctx: &mut RemapContext) -> RemapResult {
    let mut merged: Vec<(String, WardrobeWearStatsRow)> = Vec::new();
    let mut out = RemapResult::default();

    for raw in incoming {
        let Some(row) = parse_wardrobe_wear_row(raw) else {
            out.dropped_invalid += 1;
            continue;
        };
        let Some(item_id) = ctx.item_ids.get(&row.item_id).cloned() else {
            out.dropped_missing_item += 1;
            continue;
        };
        // `if (row.wearerCharacterId)` — JS truthiness: `''` is "no wearer".
        let mut wearer_character_id = None;
        if let Some(src) = row.wearer_character_id.as_deref().filter(|s| !s.is_empty()) {
            wearer_character_id = (ctx.resolve_wearer)(src);
            if wearer_character_id.is_none() {
                out.folded_wearers += 1;
            }
        }
        let mut last_worn_chat_id = None;
        if let Some(src) = row.last_worn_chat_id.as_deref().filter(|s| !s.is_empty()) {
            last_worn_chat_id = (ctx.resolve_chat)(src);
            if last_worn_chat_id.is_none() {
                out.cleared_chats += 1;
            }
        }

        let key = key_of(&item_id, wearer_character_id.as_deref());
        match merged.iter_mut().find(|(k, _)| *k == key) {
            None => {
                let id = (ctx.mint_id)();
                merged.push((
                    key,
                    WardrobeWearStatsRow {
                        id,
                        item_id,
                        wearer_character_id,
                        wear_count: row.wear_count,
                        first_worn_at: row.first_worn_at,
                        last_worn_at: row.last_worn_at,
                        last_worn_chat_id,
                        created_at: row.created_at,
                        updated_at: ctx.now.to_string(),
                    },
                ));
            }
            Some((_, prior)) => {
                let (last_at, last_chat) = later_last_wear(
                    (&prior.last_worn_at, prior.last_worn_chat_id.as_deref()),
                    (&row.last_worn_at, last_worn_chat_id.as_deref()),
                );
                prior.wear_count += row.wear_count;
                prior.first_worn_at = min_iso(&prior.first_worn_at, &row.first_worn_at);
                prior.last_worn_at = last_at;
                prior.last_worn_chat_id = last_chat;
                prior.created_at = min_iso(&prior.created_at, &row.created_at);
            }
        }
    }

    // `new Map(existing.map(...))` — a later live row on one key wins.
    let mut existing_by_key: HashMap<String, &WardrobeWearStatsRow> = HashMap::new();
    for live in ctx.existing {
        existing_by_key.insert(
            key_of(&live.item_id, live.wearer_character_id.as_deref()),
            live,
        );
    }
    for (key, row) in merged {
        let Some(live) = existing_by_key.get(&key) else {
            out.rows.push(row);
            continue;
        };
        let (last_at, last_chat) = later_last_wear(
            (&live.last_worn_at, live.last_worn_chat_id.as_deref()),
            (&row.last_worn_at, row.last_worn_chat_id.as_deref()),
        );
        out.rows.push(WardrobeWearStatsRow {
            id: live.id.clone(),
            wear_count: live.wear_count.max(row.wear_count),
            first_worn_at: min_iso(&live.first_worn_at, &row.first_worn_at),
            last_worn_at: last_at,
            last_worn_chat_id: last_chat,
            created_at: live.created_at.clone(),
            ..row
        });
    }
    out
}

/// v4 `buildImportedWardrobeItemIdMap(documents, idMaps)` (`:207-221`) —
/// "Source item id → destination item id for every wardrobe item this import
/// brought across. Two sources, in precedence order: 1. `Wardrobe/*.md`
/// documents carried by an imported store — shared stores and each
/// character's own vault. The item id lives in the frontmatter and travels
/// verbatim, so source and destination agree unless the file has no
/// frontmatter id and its id is derived from the (remapped) mount point. These
/// win: when a bundle carries a character's vault, reconciliation repoints the
/// character at it and discards the scaffold vault the `wardrobe_item`
/// records were written into. 2. `idMaps.wardrobeItems` — the ids
/// `importCharacterWardrobeItems` minted, which are the live ones only for a
/// bundle that carried no vault." A store that did not import (a skipped
/// character's vault) contributes nothing.
///
/// The two id maps come in as insertion-ordered pairs (the importer's
/// `IdMap`s, or the differential's corpus).
pub fn build_imported_wardrobe_item_id_map(
    documents: &[Value],
    mount_points: &[(String, String)],
    wardrobe_items: &[(String, String)],
) -> Vec<(String, String)> {
    let mut map: Vec<(String, String)> = wardrobe_items.to_vec();
    let mut set = |k: String, v: String| {
        if let Some(entry) = map.iter_mut().find(|(key, _)| *key == k) {
            entry.1 = v;
        } else {
            map.push((k, v));
        }
    };
    for doc in documents {
        let relative_path = doc
            .get("relativePath")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !is_wardrobe_item_document_path(relative_path) {
            continue;
        }
        let source_mount = doc
            .get("mountPointId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(target_mount) = mount_points
            .iter()
            .find(|(k, _)| k == source_mount)
            .map(|(_, v)| v.as_str())
        else {
            continue;
        };
        let content = doc.get("content").and_then(Value::as_str).unwrap_or("");
        set(
            wardrobe_item_id_for_document(source_mount, relative_path, content),
            wardrobe_item_id_for_document(target_mount, relative_path, content),
        );
    }
    map
}

/// v4 `importWardrobeWear(incoming, documents, idMaps, warnings)` (`:235-298`)
/// — "Import a bundle's wear-ledger rows. Returns how many rows were written.
/// A wearer resolves through `idMaps.characters` when the bundle carried them,
/// otherwise to themselves when that character exists on this instance (a
/// same-instance round trip of a shared store keeps its attribution); anything
/// else folds into unattributed. Chats resolve the same way, else clear."
///
/// The pre-resolution runs over the RAW rows, before validation (v4 `:248-266`,
/// a `findByIdRaw` / `findById` once per distinct unmapped id). The ONE
/// warning is v4's `Dropped N malformed wardrobe wear-ledger row(s).`;
/// `upsert_rows` runs ALWAYS (a no-op on `[]`); v4's INFO `Imported wardrobe
/// wear ledger` carries its eight keys. A failed read or write propagates —
/// the phase's catch is the caller's.
pub(crate) fn import_wardrobe_wear(
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
    let item_id_pairs = build_imported_wardrobe_item_id_map(
        documents,
        &pairs(&id_maps.mount_points),
        &pairs(&id_maps.wardrobe_items),
    );
    let item_ids: HashMap<String, String> = item_id_pairs.iter().cloned().collect();

    // Pre-resolve the ids the bundle did not map, once per distinct id, in
    // first-seen order (v4's two `Set`s).
    let mut unmapped_wearers: Vec<String> = Vec::new();
    let mut unmapped_chats: Vec<String> = Vec::new();
    for raw in incoming {
        let field = |k: &str| {
            raw.get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        if let Some(w) = field("wearerCharacterId") {
            if id_maps.characters.get(&w).is_none() && !unmapped_wearers.contains(&w) {
                unmapped_wearers.push(w);
            }
        }
        if let Some(c) = field("lastWornChatId") {
            if id_maps.chats.get(&c).is_none() && !unmapped_chats.contains(&c) {
                unmapped_chats.push(c);
            }
        }
    }
    let mut local_character_ids: HashSet<String> = HashSet::new();
    for id in unmapped_wearers {
        if crate::db::characters_read::find_by_id_raw(main, &id)?.is_some() {
            local_character_ids.insert(id);
        }
    }
    let mut local_chat_ids: HashSet<String> = HashSet::new();
    for id in unmapped_chats {
        if crate::db::chats_read::find_by_id(main, &id)?.is_some() {
            local_chat_ids.insert(id);
        }
    }

    // `[...new Set(itemIds.values())]` — distinct destination ids, map order.
    let mut destination_ids: Vec<String> = Vec::new();
    for (_, v) in &item_id_pairs {
        if !destination_ids.contains(v) {
            destination_ids.push(v.clone());
        }
    }
    let repo = WardrobeWearStatsRepository::new(main);
    let existing = repo.find_rows_for_items(&destination_ids)?;

    let resolve_wearer = |id: &str| -> Option<String> {
        id_maps
            .characters
            .get(id)
            .map(str::to_string)
            .or_else(|| local_character_ids.contains(id).then(|| id.to_string()))
    };
    let resolve_chat = |id: &str| -> Option<String> {
        id_maps
            .chats
            .get(id)
            .map(str::to_string)
            .or_else(|| local_chat_ids.contains(id).then(|| id.to_string()))
    };
    let mut mint = || uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let result = remap_wardrobe_wear_rows(
        incoming,
        &mut RemapContext {
            item_ids: &item_ids,
            resolve_wearer: &resolve_wearer,
            resolve_chat: &resolve_chat,
            existing: &existing,
            mint_id: &mut mint,
            now: &now,
        },
    );

    if result.dropped_invalid > 0 {
        warnings.push(format!(
            "Dropped {} malformed wardrobe wear-ledger row(s).",
            result.dropped_invalid
        ));
    }

    repo.upsert_rows(&result.rows)?;

    let merged_into_live = result
        .rows
        .iter()
        .filter(|r| existing.iter().any(|e| e.id == r.id))
        .count();
    tracing::info!(
        incoming = incoming.len(),
        written = result.rows.len(),
        mergedIntoLive = merged_into_live,
        droppedMissingItem = result.dropped_missing_item,
        droppedInvalid = result.dropped_invalid,
        foldedWearers = result.folded_wearers,
        clearedChats = result.cleared_chats,
        importedItemCount = item_ids.len(),
        "Imported wardrobe wear ledger"
    );
    Ok(result.rows.len())
}
