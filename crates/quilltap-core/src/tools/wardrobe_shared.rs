//! DB-touching wardrobe helpers the seven wardrobe tool handlers compose (v4
//! `lib/tools/handlers/wardrobe-handler-shared.ts` + the persisted variants of
//! `lib/wardrobe/outfit-displacement.ts` + `lib/wardrobe/resolve-equipped.ts` +
//! `lib/mount-index/tiered-mount-pool.ts`'s `resolveProjectMountPointIdsForChat`).
//!
//! The pure leaves (union/describe/expand/effect/sentinel) live in
//! [`crate::wardrobe`]; this module holds the parts that read/write the database:
//! item resolution across tiers, the equipped-outfit displacement primitives (over
//! [`ChatOutfitsRepository`]), and the coverage-summary composition (over the
//! seeded read overlay).
//!
//! Every function takes raw connections — `main: &Connection` (the `chats` +
//! `characters` reads/writes) and a [`DocMountDocumentsRepository`] over the
//! mount-index connection (the vault reads) — so the handlers run inside a single
//! writer closure that holds both (the `wardrobe_transfers` precedent), or, in the
//! differential, against two directly-opened `Writer`s.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::archetype_wardrobe::find_archetypes;
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::chats_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::vault_wardrobe_public::{WardrobePublicError, NO_MOUNT_MESSAGE};
use crate::db::wardrobe_read::{
    find_by_character_id, find_by_id_for_character, find_by_ids_for_character,
};
use crate::db::DbError;
use crate::wardrobe::{
    describe_outfit, expand_composites, OutfitSlotValues, Slots, WARDROBE_SLOT_TYPES,
};

/// v4 `resolveProjectMountPointIdsForChat(chatId)` — the project tier for a chat:
/// load the chat, then its project's linked stores. `[]` for a project-less chat
/// or any lookup failure (v4 catches → `[]`). `main` reads `chats`, `mount` reads
/// `project_doc_mount_links`.
pub fn resolve_project_mount_point_ids_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
) -> Vec<String> {
    let chat = match chats_read::find_by_id(main, chat_id) {
        Ok(Some(c)) => c,
        _ => return Vec::new(),
    };
    let Some(project_id) = chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Vec::new();
    };
    ProjectDocMountLinksRepository::new(mount)
        .find_by_project_id(project_id)
        .unwrap_or_default()
}

/// v4 `resolveWardrobeItemAcrossTiers` — resolve one wardrobe item by id
/// (preferred, via [`find_by_id_for_character`] which spans character → project →
/// general) then by title (case-insensitive; the character's own items first, then
/// the merged archetype set — non-archived). `None` when nothing matches.
pub fn resolve_wardrobe_item_across_tiers(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    item_id: Option<&str>,
    item_title: Option<&str>,
    project_mount_point_ids: &[String],
) -> Result<Option<Value>, DbError> {
    if let Some(id) = item_id {
        if let Some(found) =
            find_by_id_for_character(main, docs, character_id, id, project_mount_point_ids)?
        {
            return Ok(Some(found));
        }
    }

    if let Some(title) = item_title {
        let lower = title.trim().to_lowercase();
        let own = find_by_character_id(main, docs, character_id, true)?;
        if let Some(m) = own.into_iter().find(|i| title_lower(i) == lower) {
            return Ok(Some(m));
        }
        // v4 uses includeArchived = false for the archetype title fallback.
        let archetypes = find_archetypes(main, docs, false, project_mount_point_ids)?;
        if let Some(m) = archetypes.into_iter().find(|i| title_lower(i) == lower) {
            return Ok(Some(m));
        }
    }

    Ok(None)
}

/// Render a [`WardrobePublicError`] to v4's thrown-error message (the handlers'
/// outer try/catch maps a thrown mount error to a failure result). `verb` is
/// `create`/`update`/`delete` — the only difference between the three no-mount
/// messages. (`NoMount` is unreachable for an own-item mutation in a vaulted
/// character; ported for fidelity.)
pub fn public_error_message(e: WardrobePublicError, verb: &str) -> String {
    match e {
        WardrobePublicError::NoMount => {
            format!("Cannot {verb} wardrobe item: {NO_MOUNT_MESSAGE}")
        }
        WardrobePublicError::Cycle(m) => m,
        WardrobePublicError::Db(d) => d.to_string(),
    }
}

/// The item's `title` lowercased (v4 `i.title.toLowerCase()` — the title is NOT
/// trimmed, only the search term is).
fn title_lower(item: &Value) -> String {
    item.get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase()
}

/// v4 `isOwnWardrobeItem` — the item belongs to THIS character (editable), vs a
/// shared archetype (`characterId === null`) or someone else's.
pub fn is_own_wardrobe_item(item: &Value, character_id: &str) -> bool {
    item.get("characterId").and_then(Value::as_str) == Some(character_id)
}

/// Every slot an item is equipped in (v4 `findEquippedSlots`, in the list/read
/// handlers): walk [`WARDROBE_SLOT_TYPES`] collecting the slots whose id array
/// contains `item_id`.
pub fn find_equipped_slots(item_id: &str, equipped: Option<&Value>) -> Vec<String> {
    let Some(eq) = equipped else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        let present = eq
            .get(slot)
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(item_id)));
        if present {
            slots.push(slot.to_string());
        }
    }
    slots
}

/// A fresh empty equipped state (v4 `emptyEquippedState`).
pub fn empty_equipped_state() -> Slots {
    Slots::fresh()
}

/// v4 `loadCurrentWardrobeState` — the character's equipped slots for a chat, or
/// an empty state when none is stored.
pub fn load_current_wardrobe_state(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
) -> Result<Slots, DbError> {
    let repo = ChatOutfitsRepository::new(main);
    let stored = repo.get_equipped_outfit_for_character(chat_id, character_id)?;
    Ok(Slots::from_value(stored.as_ref()))
}

// ── The persisted equip primitives (v4 `outfit-displacement.ts`) ─────────────
// Each loads the character's slots, mutates them, and persists via
// `ChatOutfitsRepository::set_equipped_outfit` (RMW through `chats.update`, which
// preserves the chat's `updatedAt`). The returned `Slots` are the computed next
// state (v4's `result ?? next`; the stored value equals `next`).

/// v4 `equipItem` — wear an item into the slots its `types` designate, honoring
/// the `replace` flag.
pub fn equip_item(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    id: &str,
    types: &[String],
    replace: bool,
) -> Result<Slots, DbError> {
    let current = load_current_wardrobe_state(main, chat_id, character_id)?;
    let next = crate::wardrobe::wear_item_into_slots(&current, id, types, replace);
    persist(main, chat_id, character_id, &next)?;
    Ok(next)
}

/// v4 `replaceItem` — force-swap: clear each designated slot and set it to `[id]`.
pub fn replace_item(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    id: &str,
    types: &[String],
) -> Result<Slots, DbError> {
    let current = load_current_wardrobe_state(main, chat_id, character_id)?;
    let next = crate::wardrobe::replace_item_into_slots(&current, id, types);
    persist(main, chat_id, character_id, &next)?;
    Ok(next)
}

/// v4 `addToSlot` — append `id` to one named slot (validates `slot ∈ types`, a
/// guard the wear handler already enforces upstream; no-op if already present).
pub fn add_to_slot(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    slot: &str,
    id: &str,
    types: &[String],
) -> Result<Slots, DbError> {
    // v4 throws if the item can't occupy the slot; the wear handler checks first,
    // so this is unreachable there, but ported for fidelity.
    debug_assert!(types.iter().any(|t| t == slot));
    let mut current = load_current_wardrobe_state(main, chat_id, character_id)?;
    if !current_slot(&current, slot).iter().any(|x| x == id) {
        current_slot_mut(&mut current, slot).push(id.to_string());
    }
    persist(main, chat_id, character_id, &current)?;
    Ok(current)
}

/// v4 `removeFromSlot` — filter `item_id` out of a slot (or clear it entirely when
/// `item_id` is `None`).
pub fn remove_from_slot(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    slot: &str,
    item_id: Option<&str>,
) -> Result<Slots, DbError> {
    let mut current = load_current_wardrobe_state(main, chat_id, character_id)?;
    match item_id {
        None => current_slot_mut(&mut current, slot).clear(),
        Some(id) => current_slot_mut(&mut current, slot).retain(|x| x != id),
    }
    persist(main, chat_id, character_id, &current)?;
    Ok(current)
}

fn persist(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    slots: &Slots,
) -> Result<(), DbError> {
    ChatOutfitsRepository::new(main).set_equipped_outfit(
        chat_id,
        character_id,
        &slots.to_value(),
    )?;
    Ok(())
}

fn current_slot<'a>(slots: &'a Slots, name: &str) -> &'a Vec<String> {
    match name {
        "top" => &slots.top,
        "bottom" => &slots.bottom,
        "footwear" => &slots.footwear,
        _ => &slots.accessories,
    }
}

fn current_slot_mut<'a>(slots: &'a mut Slots, name: &str) -> &'a mut Vec<String> {
    match name {
        "top" => &mut slots.top,
        "bottom" => &mut slots.bottom,
        "footwear" => &mut slots.footwear,
        _ => &mut slots.accessories,
    }
}

// ── resolve-equipped.ts — resolveEquippedOutfitForCharacter ──────────────────

/// v4 `resolveEquippedOutfitForCharacter` (the `outfitValues` half — the only part
/// the coverage summary needs). Loads the character's wardrobe (so composite
/// components resolve), fills missing equipped ids from the archetype tiers,
/// expands each slot's composites into leaves, and routes each leaf's title into
/// every output slot its own `types` declare (deduped across input slots).
pub fn resolve_equipped_outfit_values(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    project_mount_point_ids: &[String],
) -> Result<OutfitSlotValues, DbError> {
    // Unique equipped ids across all four input slots.
    let mut equipped_ids: Vec<String> = Vec::new();
    let mut seen_id: std::collections::HashSet<String> = std::collections::HashSet::new();
    for slot in [
        &slots.top,
        &slots.bottom,
        &slots.footwear,
        &slots.accessories,
    ] {
        for id in slot {
            if seen_id.insert(id.clone()) {
                equipped_ids.push(id.clone());
            }
        }
    }
    if equipped_ids.is_empty() {
        return Ok(OutfitSlotValues::default());
    }

    // Pull the character's own wardrobe (v4 try/catch → []) for composite resolution.
    let char_items = find_by_character_id(main, docs, character_id, true).unwrap_or_default();
    let mut items_by_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for it in char_items {
        if let Some(id) = it.get("id").and_then(Value::as_str) {
            items_by_id.insert(id.to_string(), it);
        }
    }

    // Fill any equipped ids the character wardrobe didn't supply (archetypes, etc.).
    let missing: Vec<String> = equipped_ids
        .iter()
        .filter(|id| !items_by_id.contains_key(*id))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let fallback =
            find_by_ids_for_character(main, docs, character_id, &missing, project_mount_point_ids)
                .unwrap_or_default();
        for it in fallback {
            if let Some(id) = it.get("id").and_then(Value::as_str) {
                items_by_id.insert(id.to_string(), it);
            }
        }
    }

    // First pass: expand each slot's composites, dedup leaves in first-seen order.
    let mut seen_leaf: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ordered_leaves: Vec<Value> = Vec::new();
    let slot_arrays = [
        &slots.top,
        &slots.bottom,
        &slots.footwear,
        &slots.accessories,
    ];
    for slot in slot_arrays {
        let expanded = expand_composites(slot, &items_by_id, None);
        for id in expanded.leaf_ids {
            if seen_leaf.contains(&id) {
                continue;
            }
            let Some(item) = items_by_id.get(&id) else {
                continue;
            };
            seen_leaf.insert(id);
            ordered_leaves.push(item.clone());
        }
    }

    // Second pass: route each leaf's title into every output slot its `types` declare.
    let mut out = OutfitSlotValues::default();
    for item in &ordered_leaves {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let types = item
            .get("types")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for t in &types {
            match t.as_str() {
                Some("top") => out.top.push(title.clone()),
                Some("bottom") => out.bottom.push(title.clone()),
                Some("footwear") => out.footwear.push(title.clone()),
                Some("accessories") => out.accessories.push(title.clone()),
                _ => {}
            }
        }
    }

    Ok(out)
}

/// v4 `buildWardrobeCoverageSummaryFromState` — resolve the equipped outfit into
/// leaf titles, then render via [`describe_outfit`].
pub fn build_wardrobe_coverage_summary_from_state(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    slots: &Slots,
    project_mount_point_ids: &[String],
) -> Result<String, DbError> {
    let values =
        resolve_equipped_outfit_values(main, docs, character_id, slots, project_mount_point_ids)?;
    Ok(describe_outfit(&values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_own_checks_character_id() {
        assert!(is_own_wardrobe_item(&json!({ "characterId": "c1" }), "c1"));
        assert!(!is_own_wardrobe_item(&json!({ "characterId": null }), "c1"));
        assert!(!is_own_wardrobe_item(
            &json!({ "characterId": "other" }),
            "c1"
        ));
    }

    #[test]
    fn find_equipped_slots_walks_all_slots() {
        let eq = json!({ "top": ["a", "b"], "bottom": [], "footwear": ["a"], "accessories": [] });
        assert_eq!(find_equipped_slots("a", Some(&eq)), vec!["top", "footwear"]);
        assert_eq!(find_equipped_slots("b", Some(&eq)), vec!["top"]);
        assert!(find_equipped_slots("z", Some(&eq)).is_empty());
        assert!(find_equipped_slots("a", None).is_empty());
    }
}
