//! The equipped-outfit resolver — v4 `lib/wardrobe/resolve-equipped.ts` at
//! `9ecebf986` (pin `4a38222e6`).
//!
//! Equipped slots store arrays of item ids, which may be composites
//! referencing other items. Most callers want the same thing: a per-slot list
//! of leaf items and their titles, ready to feed into
//! [`describe_outfit`](crate::wardrobe::describe_outfit) or to render in a
//! prompt block.
//!
//! The resolver walks the character's wearable pool
//! ([`crate::wearable_pool`]), which already holds every tier the character
//! can reach, archived items included (an item archived after the chat last
//! loaded still resolves to its title):
//!
//! 1. Expands each input slot's array via
//!    [`expand_composites`](crate::wardrobe::expand_composites) over the pool,
//!    then routes each resulting leaf into EVERY output slot the leaf's own
//!    `types` declares — deduped across input slots. An atomic dress with
//!    `types=[top,bottom]` shows up in both rendered slots even if it was only
//!    equipped to one, and a composite outfit whose components have
//!    heterogeneous types distributes those components correctly.
//! 2. Returns per-slot leaf items, the title-array [`OutfitSlotValues`] for
//!    `describe_outfit`, and the `items_by_id` map of everything it touched.
//!
//! No I/O: the caller loads the pool once and may reuse it. An id outside the
//! character's pool is dropped (DEBUG `[resolveEquippedOutfitForCharacter]
//! Equipped ids not in the wearable pool`) rather than borrowing another
//! character's item.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use crate::tools::wardrobe_shared::LeafItemsBySlot;
use crate::wardrobe::{
    build_outfit_slot_values, decorate_outfit_items_title_only, describe_outfit, expand_composites,
    OutfitSlotValues, Slots, WARDROBE_SLOT_TYPES,
};
use crate::wearable_pool::{
    component_graph_lookup, load_wearable_pool, LoadWearablePoolOptions, WearablePool,
};

/// v4 `ResolvedEquippedOutfit`.
#[derive(Debug, Clone, Default)]
pub struct ResolvedEquippedOutfit {
    /// Per-slot title arrays, ready for `describe_outfit`.
    pub outfit_values: OutfitSlotValues,
    /// Per-slot leaf items (composites expanded), in the order they appear in
    /// equipped state.
    pub leaf_items_by_slot: LeafItemsBySlot,
    /// Every item id encountered during resolution (composites + leaves).
    pub items_by_id: HashMap<String, Value>,
}

/// v4 `allEquippedItemIds(slots)` — every id across the slots, in slot order,
/// first-seen deduped.
fn all_equipped_item_ids(slots: &Slots) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        for id in slots.slot(slot) {
            if seen.insert(id.as_str()) {
                out.push(id.clone());
            }
        }
    }
    out
}

/// v4 `resolveEquippedOutfitForCharacter(pool, slots)` — resolve a
/// character's equipped slots into per-slot leaf items and a
/// `describe_outfit`-ready [`OutfitSlotValues`]. Composites are expanded
/// transitively over the pool.
pub fn resolve_equipped_outfit_for_character(
    pool: &WearablePool,
    slots: &Slots,
) -> ResolvedEquippedOutfit {
    let equipped_item_ids = all_equipped_item_ids(slots);
    if equipped_item_ids.is_empty() {
        return ResolvedEquippedOutfit::default();
    }

    let items_by_id = component_graph_lookup(pool, &equipped_item_ids);
    let unresolved = equipped_item_ids
        .iter()
        .filter(|id| !items_by_id.contains_key(*id))
        .count();
    if unresolved > 0 {
        tracing::debug!(
            context = "wardrobe",
            characterId = pool.character_id.as_str(),
            unresolvedCount = unresolved,
            "[resolveEquippedOutfitForCharacter] Equipped ids not in the wearable pool"
        );
    }

    // First pass: expand each input slot's composites, dedupe by leaf id across
    // the whole equipped set, and remember the order leaves were first seen.
    // Second pass: route each leaf into every output slot its own `types`
    // declares. That spreads atomic multi-slot items (a dress with
    // `types=[top,bottom]` lands in both rendered slots) and routes composite
    // components to the slots their own `types` say (a "casual outfit"
    // composite whose components are blouse(top)/slacks(bottom)/loafers
    // (footwear) distributes correctly even if the composite itself was
    // equipped to one slot).
    let mut seen_leaf_ids: HashSet<String> = HashSet::new();
    let mut ordered_leaves: Vec<&Value> = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        // A legacy slot bag with no key at all reads as empty (`?? []`) —
        // `Slots` already materialises every slot.
        let expanded = expand_composites(slots.slot(slot), &items_by_id, None);
        if !expanded.cycles.is_empty() || expanded.truncated {
            tracing::warn!(
                context = "wardrobe",
                characterId = pool.character_id.as_str(),
                slot = slot,
                cycles = expanded.cycles.len(),
                truncated = expanded.truncated,
                "[resolveEquippedOutfitForCharacter] Malformed composite graph; expansion truncated"
            );
        }
        for id in expanded.leaf_ids {
            if seen_leaf_ids.contains(&id) {
                continue;
            }
            let Some(item) = items_by_id.get(&id) else {
                continue;
            };
            seen_leaf_ids.insert(id);
            ordered_leaves.push(item);
        }
    }

    let mut leaf_items_by_slot = LeafItemsBySlot::default();
    let mut outfit_values = OutfitSlotValues::default();
    for item in ordered_leaves {
        // A leaf's `types` declare which slots it covers. Route into each. If
        // `types` is somehow empty (the schema requires min(1)), fall back to
        // no-op rather than guessing.
        let types = item.get("types").and_then(Value::as_array);
        for slot in types.into_iter().flatten().filter_map(Value::as_str) {
            if !WARDROBE_SLOT_TYPES.contains(&slot) {
                continue;
            }
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let (leaves, titles) = match slot {
                "top" => (&mut leaf_items_by_slot.top, &mut outfit_values.top),
                "bottom" => (&mut leaf_items_by_slot.bottom, &mut outfit_values.bottom),
                "footwear" => (
                    &mut leaf_items_by_slot.footwear,
                    &mut outfit_values.footwear,
                ),
                "accessories" => (
                    &mut leaf_items_by_slot.accessories,
                    &mut outfit_values.accessories,
                ),
                _ => (&mut leaf_items_by_slot.hair, &mut outfit_values.hair),
            };
            leaves.push(item.clone());
            titles.push(title);
        }
    }

    ResolvedEquippedOutfit {
        outfit_values,
        leaf_items_by_slot,
        items_by_id,
    }
}

/// v4 `describeEquippedOutfitTitleOnly(repos, characterId, equippedSlots,
/// projectMountPointIds)` — a character's currently equipped outfit as a
/// concise, title-only markdown block: the shared "what are they wearing right
/// now" pipeline for prompt builders that must stay terse. The project tier is
/// passed in, already resolved by the caller (NOT roster-gated — the survey's
/// §14.5: never "fix" it onto `operator: false`); the pool resolves the group
/// tier itself.
pub fn describe_equipped_outfit_title_only(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    equipped_slots: &Slots,
    project_mount_point_ids: Option<&[String]>,
) -> String {
    let pool = load_wearable_pool(
        main,
        mount,
        character_id,
        Some(project_mount_point_ids.unwrap_or_default()),
        LoadWearablePoolOptions::default(),
    );
    let resolved = resolve_equipped_outfit_for_character(&pool, equipped_slots);
    describe_outfit(&build_outfit_slot_values(|slot| {
        decorate_outfit_items_title_only(resolved.leaf_items_by_slot.slot(slot))
    }))
}

#[cfg(test)]
mod tests {
    //! v4 `resolve-equipped.test.ts`'s shapes over pools built with
    //! [`build_wearable_pool`] (v4's own seam), plus the two lines
    //! capture-pinned with their silence legs.
    use super::*;
    use crate::test_support::global_capture;
    use crate::wearable_pool::{build_wearable_pool, WardrobeTierMounts};
    use serde_json::json;

    fn item(id: &str, types: &[&str], components: &[&str]) -> Value {
        json!({
            "id": id, "title": format!("T-{id}"), "types": types,
            "componentItemIds": components, "archivedAt": null,
        })
    }

    fn pool(own: Vec<Value>, general: Vec<Value>) -> WearablePool {
        build_wearable_pool(
            "c1",
            WardrobeTierMounts::default(),
            own,
            vec![],
            vec![],
            general,
        )
    }

    fn slots(v: Value) -> Slots {
        Slots::from_value(Some(&v))
    }

    fn titles(r: &ResolvedEquippedOutfit, slot: &str) -> Vec<String> {
        r.outfit_values.slot(slot).to_vec()
    }

    #[test]
    fn a_multi_slot_leaf_lands_in_every_slot_it_covers() {
        global_capture::install();
        let p = pool(vec![item("dress", &["top", "bottom"], &[])], vec![]);
        let (r, lines) = global_capture::capture(|| {
            resolve_equipped_outfit_for_character(&p, &slots(json!({ "top": ["dress"] })))
        });
        assert_eq!(titles(&r, "top"), ["T-dress"]);
        assert_eq!(titles(&r, "bottom"), ["T-dress"]);
        assert_eq!(r.leaf_items_by_slot.bottom.len(), 1);
        // Silence leg: nothing unresolved, nothing malformed.
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[resolveEquippedOutfitForCharacter]")),
            "{lines:#?}"
        );
    }

    #[test]
    fn composite_parts_route_to_their_own_slots_without_double_counting() {
        global_capture::install();
        let p = pool(
            vec![
                item("blouse", &["top"], &[]),
                item("slacks", &["bottom"], &[]),
                item("loafers", &["footwear"], &[]),
                item("casual", &["top"], &["blouse", "slacks", "loafers"]),
            ],
            vec![],
        );
        // The composite sits in `top` alone; a part is also equipped directly.
        let r = resolve_equipped_outfit_for_character(
            &p,
            &slots(json!({ "top": ["casual"], "footwear": ["loafers"] })),
        );
        assert_eq!(titles(&r, "top"), ["T-blouse"]);
        assert_eq!(titles(&r, "bottom"), ["T-slacks"]);
        assert_eq!(titles(&r, "footwear"), ["T-loafers"]);
        assert!(r.items_by_id.contains_key("casual"));
    }

    #[test]
    fn shared_tier_parts_and_archived_items_resolve() {
        global_capture::install();
        let mut gone = item("gone", &["accessories"], &[]);
        gone["archivedAt"] = json!("2026-01-01T00:00:00.000Z");
        let p = pool(
            vec![gone, item("kit", &["top"], &["cape"])],
            vec![item("cape", &["top"], &[])],
        );
        let r = resolve_equipped_outfit_for_character(
            &p,
            &slots(json!({ "top": ["kit"], "accessories": ["gone"] })),
        );
        assert_eq!(titles(&r, "top"), ["T-cape"]);
        assert_eq!(titles(&r, "accessories"), ["T-gone"]);
    }

    #[test]
    fn an_id_outside_the_pool_is_dropped_with_its_debug_line() {
        global_capture::install();
        let p = pool(vec![item("coat", &["top"], &[])], vec![]);
        let (r, lines) = global_capture::capture(|| {
            resolve_equipped_outfit_for_character(
                &p,
                &slots(json!({ "top": ["coat", "elsewhere"], "hair": ["nowhere"] })),
            )
        });
        assert_eq!(titles(&r, "top"), ["T-coat"]);
        assert!(titles(&r, "hair").is_empty());
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Equipped ids not in the wearable pool"))
            .collect();
        assert_eq!(hits.len(), 1, "{lines:#?}");
        let l = hits[0];
        assert!(l.starts_with("DEBUG "), "{l}");
        for field in ["context=wardrobe", "characterId=c1", "unresolvedCount=2"] {
            assert!(l.contains(field), "{field} in {l}");
        }
    }

    #[test]
    fn a_cycle_warns_once_for_its_slot() {
        global_capture::install();
        let p = pool(
            vec![
                item("a", &["top"], &["b"]),
                item("b", &["top"], &["a"]),
                item("hat", &["accessories"], &[]),
            ],
            vec![],
        );
        let (_r, lines) = global_capture::capture(|| {
            resolve_equipped_outfit_for_character(
                &p,
                &slots(json!({ "top": ["a"], "accessories": ["hat"] })),
            )
        });
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Malformed composite graph; expansion truncated"))
            .collect();
        assert_eq!(hits.len(), 1, "{lines:#?}");
        let l = hits[0];
        assert!(l.starts_with("WARN "), "{l}");
        for field in [
            "context=wardrobe",
            "characterId=c1",
            "slot=top",
            "cycles=1",
            "truncated=false",
        ] {
            assert!(l.contains(field), "{field} in {l}");
        }
    }

    #[test]
    fn an_empty_outfit_resolves_to_nothing() {
        global_capture::install();
        let p = pool(vec![item("coat", &["top"], &[])], vec![]);
        let r = resolve_equipped_outfit_for_character(&p, &Slots::default());
        assert!(r.items_by_id.is_empty());
        assert_eq!(r.outfit_values, OutfitSlotValues::default());
    }
}
