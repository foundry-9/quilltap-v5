//! Bundle dissolution (v4 `lib/wardrobe/dissolve-bundles.ts`, `61574563`).
//!
//! A "bundle" is a wardrobe item with `componentItemIds` — Man in Black, the
//! House Livery, a three-piece suit. Equipped state used to keep the bundle's
//! own id in every slot it covered, expanding to leaf garments only at read
//! time. That made the wardrobe dialog read badly: the bundle showed as one
//! card and all four slot rows underneath said *Empty*, so nothing on screen
//! told you which shirt you actually had on.
//!
//! Putting a bundle on now dissolves it in the same gesture: the leaves go into
//! the slots their own `types` declare and the bundle's id is never stored.
//! There is no separate "Break apart" step for anything worn from here on (the
//! button survives for outfits equipped before this change, which still carry a
//! composite id).
//!
//! Dissolution is deliberately *total* — [`expand_composites`] walks to leaves,
//! so a bundle nested inside a bundle dissolves too and no bundle card can come
//! back. It is also fail-safe: an unresolvable bundle (components live in a
//! store this caller can't see) returns `None`, and callers fall back to
//! storing the bundle whole, exactly as before. Wearing something is never
//! allowed to resolve to wearing nothing.

use std::collections::HashMap;

use serde_json::Value;

use crate::wardrobe::{expand_composites, Slots, WARDROBE_SLOT_TYPES};

/// The item lookup used to resolve a bundle's components (v4 `WearableLookup`,
/// a `ReadonlyMap<string, WearableNode>`). v5 reuses the `id → item JSON` map
/// [`expand_composites`] already walks, so no conversion is needed at any call
/// site.
pub type WearableLookup = HashMap<String, Value>;

/// The minimum an item has to expose to be dissolved or worn (v4
/// `WearableNode`). v4 states it structurally so the server's full
/// `WardrobeItem` and the client's lighter `WardrobeItemSummary` both satisfy
/// it; v5 states it as an owned struct the call sites build from whichever
/// shape they hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WearableNode {
    pub id: String,
    pub types: Vec<String>,
    pub component_item_ids: Vec<String>,
}

impl WearableNode {
    /// Build from the pieces the persisted equip primitives already carry.
    pub fn new(id: &str, types: &[String], component_item_ids: &[String]) -> Self {
        WearableNode {
            id: id.to_string(),
            types: types.to_vec(),
            component_item_ids: component_item_ids.to_vec(),
        }
    }

    /// Build from a wardrobe-item JSON value (a missing/none `componentItemIds`
    /// reads as empty, v4's `?? []`).
    pub fn from_value(item: &Value) -> Self {
        WearableNode {
            id: item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            types: string_array(item, "types"),
            component_item_ids: string_array(item, "componentItemIds"),
        }
    }
}

/// A dissolved leaf: the id to store, and the slots it occupies (v4
/// `DissolvedLeaf`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DissolvedLeaf {
    pub id: String,
    pub slots: Vec<String>,
}

fn string_array(item: &Value, key: &str) -> Vec<String> {
    item.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `slotsCoveredBy(item)` — the recognized slots an item covers, filtering
/// out anything unknown. Order is the item's OWN `types` order (v4 filters the
/// array, it does not re-sort into canonical slot order).
pub fn slots_covered_by(item: &WearableNode) -> Vec<String> {
    item.types
        .iter()
        .filter(|t| WARDROBE_SLOT_TYPES.contains(&t.as_str()))
        .cloned()
        .collect()
}

/// v4 `isBundle(item)` — true when the item bundles other items.
pub fn is_bundle(item: &WearableNode) -> bool {
    !item.component_item_ids.is_empty()
}

/// v4 `dissolveBundleToLeaves` — expand a bundle into the leaf garments that
/// should be worn in its place.
///
/// Returns `None` — meaning "wear this item as its own id, the way we always
/// did" — when the item isn't a bundle, when no lookup was supplied, or when
/// not one component resolved to something wearable. That last case is the
/// important one: a shared bundle whose parts live in a store the caller can't
/// read must still put *something* on.
pub fn dissolve_bundle_to_leaves(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Option<Vec<DissolvedLeaf>> {
    let items_by_id = items_by_id?;
    if !is_bundle(item) {
        return None;
    }

    let expanded = expand_composites(&item.component_item_ids, items_by_id, None);

    let mut leaves: Vec<DissolvedLeaf> = Vec::new();
    for leaf_id in &expanded.leaf_ids {
        // A component pointing back at its own bundle would have the bundle wear
        // itself. `expand_composites` truncates the cycle; drop the echo here.
        if *leaf_id == item.id {
            continue;
        }
        let Some(leaf) = items_by_id.get(leaf_id) else {
            continue;
        };
        let slots = slots_covered_by(&WearableNode::from_value(leaf));
        if slots.is_empty() {
            continue;
        }
        leaves.push(DissolvedLeaf {
            id: leaf_id.clone(),
            slots,
        });
    }

    if !expanded.cycles.is_empty() || expanded.truncated {
        tracing::warn!(
            item_id = item.id.as_str(),
            cycles = expanded.cycles.len(),
            truncated = expanded.truncated,
            "[dissolveBundleToLeaves] Malformed bundle graph; expansion truncated"
        );
    }

    if leaves.is_empty() {
        tracing::warn!(
            item_id = item.id.as_str(),
            component_count = item.component_item_ids.len(),
            "[dissolveBundleToLeaves] Bundle resolved to no wearable parts; wearing it whole"
        );
        return None;
    }

    Some(leaves)
}

/// v4 `layLeavesIntoSlots` — lay a dissolved bundle's leaves into a slots
/// snapshot.
///
/// Each leaf lands in every slot its *own* `types` declare — which is what read
/// time already did, so an outfit whose components are blouse(top) /
/// slacks(bottom) / loafers(footwear) distributes correctly no matter which
/// slot the bundle nominally claimed.
///
/// `clear_covered_slots` is the bundle's `replace` gesture, and it clears the
/// union of the bundle's own `types` and every slot its leaves occupy. The
/// union matters: an assembled outfit that brings boots should swap the boots
/// that were already on, not layer over them.
pub fn lay_leaves_into_slots(
    current_slots: &Slots,
    bundle: &WearableNode,
    leaves: &[DissolvedLeaf],
    clear_covered_slots: bool,
) -> Slots {
    let mut slots = current_slots.clone();

    if clear_covered_slots {
        let mut covered: Vec<String> = Vec::new();
        for slot in slots_covered_by(bundle) {
            if !covered.contains(&slot) {
                covered.push(slot);
            }
        }
        for leaf in leaves {
            for slot in &leaf.slots {
                if !covered.contains(slot) {
                    covered.push(slot.clone());
                }
            }
        }
        for slot in &covered {
            slot_mut(&mut slots, slot).clear();
        }
    }

    for leaf in leaves {
        for slot in &leaf.slots {
            let arr = slot_mut(&mut slots, slot);
            if !arr.contains(&leaf.id) {
                arr.push(leaf.id.clone());
            }
        }
    }

    slots
}

/// v4 `dissolveBundlesInSlots` — dissolve every bundle already sitting in a
/// slots snapshot.
///
/// For the snapshot builders — the default outfit, and the cheap LLM's
/// chat-start outfit pick — which compose slots directly instead of going
/// through the wear primitives. Layer order is preserved: a bundle's leaves are
/// substituted where the bundle's id sat. Leaves covering a slot the bundle
/// never occupied are appended to that slot, matching read-time routing.
///
/// Bundles that can't be resolved are left in place untouched.
pub fn dissolve_bundles_in_slots(current_slots: &Slots, items_by_id: &WearableLookup) -> Slots {
    // v4's `Map<string, DissolvedLeaf[]>` — insertion-ordered, and the second
    // pass below walks it in that order, so a Vec is the faithful shape.
    let mut dissolved: Vec<(String, Vec<DissolvedLeaf>)> = Vec::new();
    let mut considered: Vec<String> = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        for id in slot_of(current_slots, slot) {
            if considered.contains(id) {
                continue;
            }
            considered.push(id.clone());
            let Some(item) = items_by_id.get(id) else {
                continue;
            };
            if let Some(leaves) =
                dissolve_bundle_to_leaves(&WearableNode::from_value(item), Some(items_by_id))
            {
                dissolved.push((id.clone(), leaves));
            }
        }
    }

    if dissolved.is_empty() {
        return current_slots.clone();
    }

    let leaves_of = |id: &str| -> Option<&Vec<DissolvedLeaf>> {
        dissolved.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    };

    let mut next = Slots::default();

    // Substitute in place, so a bundle's parts inherit its position in the
    // layering order rather than jumping to the end of the slot.
    for slot in WARDROBE_SLOT_TYPES {
        for id in slot_of(current_slots, slot) {
            let Some(leaves) = leaves_of(id) else {
                let arr = slot_mut(&mut next, slot);
                if !arr.contains(id) {
                    arr.push(id.clone());
                }
                continue;
            };
            for leaf in leaves {
                if leaf.slots.iter().any(|s| s == slot) {
                    let arr = slot_mut(&mut next, slot);
                    if !arr.contains(&leaf.id) {
                        arr.push(leaf.id.clone());
                    }
                }
            }
        }
    }

    // Second pass: a leaf whose slot the bundle never claimed still belongs in
    // that slot (a bundle equipped to top/bottom whose parts include a hat).
    for (_, leaves) in &dissolved {
        for leaf in leaves {
            for slot in &leaf.slots {
                let arr = slot_mut(&mut next, slot);
                if !arr.contains(&leaf.id) {
                    arr.push(leaf.id.clone());
                }
            }
        }
    }

    next
}

/// A slot's id array by name. An unrecognized name is unreachable — every
/// caller filters through [`slots_covered_by`] or walks [`WARDROBE_SLOT_TYPES`].
fn slot_of<'a>(slots: &'a Slots, name: &str) -> &'a Vec<String> {
    match name {
        "bottom" => &slots.bottom,
        "footwear" => &slots.footwear,
        "accessories" => &slots.accessories,
        _ => &slots.top,
    }
}

fn slot_mut<'a>(slots: &'a mut Slots, name: &str) -> &'a mut Vec<String> {
    match name {
        "bottom" => &mut slots.bottom,
        "footwear" => &mut slots.footwear,
        "accessories" => &mut slots.accessories,
        _ => &mut slots.top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wardrobe::{add_item_to_slot, replace_item_into_slots, wear_item_into_slots};
    use serde_json::json;

    const NOW: &str = "2026-01-01T00:00:00.000Z";

    /// v4's `makeItem` from `__tests__/unit/lib/wardrobe/dissolve-bundles.test.ts`.
    fn make_item(id: &str, types: &[&str], component_item_ids: &[&str]) -> Value {
        json!({
            "id": id,
            "characterId": "c1c1c1c1-0000-0000-0000-000000000001",
            "title": id,
            "types": types,
            "componentItemIds": component_item_ids,
            "isDefault": false,
            "archivedAt": null,
            "createdAt": NOW,
            "updatedAt": NOW,
        })
    }

    fn build_map(items: &[Value]) -> WearableLookup {
        items
            .iter()
            .map(|i| {
                (
                    i.get("id").and_then(Value::as_str).unwrap().to_string(),
                    i.clone(),
                )
            })
            .collect()
    }

    fn node(item: &Value) -> WearableNode {
        WearableNode::from_value(item)
    }

    fn slots(top: &[&str], bottom: &[&str], footwear: &[&str], accessories: &[&str]) -> Slots {
        Slots {
            top: top.iter().map(|s| s.to_string()).collect(),
            bottom: bottom.iter().map(|s| s.to_string()).collect(),
            footwear: footwear.iter().map(|s| s.to_string()).collect(),
            accessories: accessories.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn leaf(id: &str, s: &[&str]) -> DissolvedLeaf {
        DissolvedLeaf {
            id: id.to_string(),
            slots: s.iter().map(|x| x.to_string()).collect(),
        }
    }

    // "Man in Black": a four-slot bundle over four single-slot garments.
    fn wardrobe() -> (Vec<Value>, WearableLookup) {
        let items = vec![
            make_item("shirt", &["top"], &[]),
            make_item("trousers", &["bottom"], &[]),
            make_item("boots", &["footwear"], &[]),
            make_item("gloves", &["accessories"], &[]),
            make_item(
                "man-in-black",
                &["top", "bottom", "footwear", "accessories"],
                &["shirt", "trousers", "boots", "gloves"],
            ),
        ];
        let map = build_map(&items);
        (items, map)
    }

    // ── isBundle ────────────────────────────────────────────────────────────

    #[test]
    fn is_bundle_only_when_the_item_has_components() {
        let (items, _) = wardrobe();
        assert!(is_bundle(&node(&items[4])));
        assert!(!is_bundle(&node(&items[0])));
        assert!(!is_bundle(&node(&json!({ "id": "x", "types": ["top"] }))));
    }

    // ── dissolveBundleToLeaves ──────────────────────────────────────────────

    #[test]
    fn dissolve_expands_a_bundle_into_its_parts_with_the_slots_each_covers() {
        let (items, map) = wardrobe();
        assert_eq!(
            dissolve_bundle_to_leaves(&node(&items[4]), Some(&map)),
            Some(vec![
                leaf("shirt", &["top"]),
                leaf("trousers", &["bottom"]),
                leaf("boots", &["footwear"]),
                leaf("gloves", &["accessories"]),
            ])
        );
    }

    #[test]
    fn dissolve_walks_all_the_way_to_leaves_so_a_nested_bundle_dissolves_too() {
        let jewelry = make_item("jewelry", &["accessories"], &["locket", "ring"]);
        let locket = make_item("locket", &["accessories"], &[]);
        let ring = make_item("ring", &["accessories"], &[]);
        let gala = make_item("gala", &["top", "accessories"], &["gown", "jewelry"]);
        let gown = make_item("gown", &["top", "bottom"], &[]);
        let map = build_map(&[jewelry, locket, ring, gala.clone(), gown]);

        assert_eq!(
            dissolve_bundle_to_leaves(&node(&gala), Some(&map)),
            Some(vec![
                leaf("gown", &["top", "bottom"]),
                leaf("locket", &["accessories"]),
                leaf("ring", &["accessories"]),
            ])
        );
    }

    #[test]
    fn dissolve_returns_none_for_a_plain_garment() {
        let (items, map) = wardrobe();
        assert_eq!(
            dissolve_bundle_to_leaves(&node(&items[0]), Some(&map)),
            None
        );
    }

    #[test]
    fn dissolve_returns_none_when_no_lookup_is_supplied() {
        let (items, _) = wardrobe();
        assert_eq!(dissolve_bundle_to_leaves(&node(&items[4]), None), None);
    }

    #[test]
    fn dissolve_returns_none_when_not_one_component_resolves() {
        let orphan = make_item("orphan", &["top"], &["nowhere-1", "nowhere-2"]);
        let map = build_map(std::slice::from_ref(&orphan));
        assert_eq!(dissolve_bundle_to_leaves(&node(&orphan), Some(&map)), None);
    }

    #[test]
    fn dissolve_drops_a_component_that_points_back_at_its_own_bundle() {
        let (items, _) = wardrobe();
        let selfish = make_item("selfish", &["top"], &["selfish", "shirt"]);
        let map = build_map(&[selfish.clone(), items[0].clone()]);
        assert_eq!(
            dissolve_bundle_to_leaves(&node(&selfish), Some(&map)),
            Some(vec![leaf("shirt", &["top"])])
        );
    }

    #[test]
    fn dissolve_skips_parts_that_cover_no_recognized_slot() {
        let (items, _) = wardrobe();
        let junk = make_item("junk", &["hat"], &[]);
        let bundle = make_item("bundle", &["top"], &["shirt", "junk"]);
        let map = build_map(&[items[0].clone(), junk, bundle.clone()]);
        assert_eq!(
            dissolve_bundle_to_leaves(&node(&bundle), Some(&map)),
            Some(vec![leaf("shirt", &["top"])])
        );
    }

    // ── layLeavesIntoSlots ──────────────────────────────────────────────────

    fn two_leaves() -> Vec<DissolvedLeaf> {
        vec![leaf("shirt", &["top"]), leaf("boots", &["footwear"])]
    }

    #[test]
    fn lay_layers_parts_over_what_is_already_worn_when_not_replacing() {
        let (items, _) = wardrobe();
        let worn = slots(&["vest"], &[], &["sandals"], &[]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&items[4]), &two_leaves(), false),
            slots(&["vest", "shirt"], &[], &["sandals", "boots"], &[])
        );
    }

    #[test]
    fn lay_clears_the_slots_it_lands_in_when_replacing() {
        let (items, _) = wardrobe();
        let worn = slots(&["vest"], &["skirt"], &["sandals"], &["hat"]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&items[4]), &two_leaves(), true),
            slots(&["shirt"], &[], &["boots"], &[])
        );
    }

    #[test]
    fn lay_replacing_also_clears_slots_the_bundle_itself_does_not_claim() {
        // The bundle nominally covers top only, but brings boots along. The boots
        // should swap the sandals rather than layer over them.
        let top_only = make_item("top-only", &["top"], &["shirt", "boots"]);
        let worn = slots(&["vest"], &[], &["sandals"], &[]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&top_only), &two_leaves(), true),
            slots(&["shirt"], &[], &["boots"], &[])
        );
    }

    #[test]
    fn lay_does_not_duplicate_a_part_already_worn() {
        let (items, _) = wardrobe();
        let worn = slots(&["shirt"], &[], &[], &[]);
        let result = lay_leaves_into_slots(&worn, &node(&items[4]), &two_leaves(), false);
        assert_eq!(result.top, vec!["shirt".to_string()]);
    }

    // ── wearItemIntoSlots with a bundle ─────────────────────────────────────

    #[test]
    fn wear_stores_the_parts_never_the_bundle_id() {
        let (items, map) = wardrobe();
        let result = wear_item_into_slots(&Slots::fresh(), &node(&items[4]), false, Some(&map));
        assert_eq!(
            result,
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
        assert!(!result.to_value().to_string().contains("man-in-black"));
    }

    #[test]
    fn wear_layers_the_parts_over_what_is_worn_by_default() {
        let (items, map) = wardrobe();
        let worn = slots(&["vest"], &[], &[], &[]);
        assert_eq!(
            wear_item_into_slots(&worn, &node(&items[4]), false, Some(&map)).top,
            vec!["vest".to_string(), "shirt".to_string()]
        );
    }

    #[test]
    fn wear_honours_the_replace_flag() {
        let (items, map) = wardrobe();
        let worn = slots(&["vest"], &["skirt"], &["sandals"], &["hat"]);
        assert_eq!(
            wear_item_into_slots(&worn, &node(&items[4]), true, Some(&map)),
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
    }

    #[test]
    fn wear_falls_back_to_storing_the_bundle_whole_without_a_lookup() {
        let (items, _) = wardrobe();
        assert_eq!(
            wear_item_into_slots(&Slots::fresh(), &node(&items[4]), false, None),
            slots(
                &["man-in-black"],
                &["man-in-black"],
                &["man-in-black"],
                &["man-in-black"]
            )
        );
    }

    #[test]
    fn wear_leaves_plain_garments_alone() {
        let (items, map) = wardrobe();
        assert_eq!(
            wear_item_into_slots(&Slots::fresh(), &node(&items[0]), false, Some(&map)),
            slots(&["shirt"], &[], &[], &[])
        );
    }

    // ── replaceItemIntoSlots with a bundle ──────────────────────────────────

    #[test]
    fn replace_clears_the_slots_and_stores_the_parts() {
        let (items, map) = wardrobe();
        let worn = slots(&["vest", "coat"], &["skirt"], &[], &[]);
        assert_eq!(
            replace_item_into_slots(&worn, &node(&items[4]), Some(&map)),
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
    }

    // ── addItemToSlot with a bundle ─────────────────────────────────────────

    #[test]
    fn add_to_slot_adds_only_the_parts_covering_the_named_slot() {
        let (items, map) = wardrobe();
        assert_eq!(
            add_item_to_slot(&Slots::fresh(), "top", &node(&items[4]), Some(&map)),
            slots(&["shirt"], &[], &[], &[])
        );
    }

    #[test]
    fn add_to_slot_falls_back_to_the_bundle_id_when_no_part_covers_the_slot() {
        let (items, _) = wardrobe();
        let top_only = make_item("top-only", &["top", "bottom"], &["shirt"]);
        let map = build_map(&[items[0].clone(), top_only.clone()]);
        assert_eq!(
            add_item_to_slot(&Slots::fresh(), "bottom", &node(&top_only), Some(&map)).bottom,
            vec!["top-only".to_string()]
        );
    }

    // ── dissolveBundlesInSlots ──────────────────────────────────────────────

    #[test]
    fn dissolve_in_slots_substitutes_a_bundle_in_place_preserving_layering_order() {
        let (_, map) = wardrobe();
        let worn = slots(
            &["undershirt", "man-in-black", "scarf"],
            &["man-in-black"],
            &["man-in-black"],
            &["man-in-black"],
        );
        assert_eq!(
            dissolve_bundles_in_slots(&worn, &map),
            slots(
                &["undershirt", "shirt", "scarf"],
                &["trousers"],
                &["boots"],
                &["gloves"]
            )
        );
    }

    #[test]
    fn dissolve_in_slots_routes_a_part_into_a_slot_the_bundle_never_occupied() {
        let (items, _) = wardrobe();
        let hat = make_item("hat", &["accessories"], &[]);
        let kit = make_item("kit", &["top"], &["shirt", "hat"]);
        let map = build_map(&[items[0].clone(), hat, kit]);
        let worn = slots(&["kit"], &[], &[], &[]);
        assert_eq!(
            dissolve_bundles_in_slots(&worn, &map),
            slots(&["shirt"], &[], &[], &["hat"])
        );
    }

    #[test]
    fn dissolve_in_slots_returns_the_snapshot_untouched_when_nothing_is_a_bundle() {
        let (_, map) = wardrobe();
        let worn = slots(&["shirt"], &["trousers"], &[], &[]);
        assert_eq!(dissolve_bundles_in_slots(&worn, &map), worn);
    }

    #[test]
    fn dissolve_in_slots_leaves_an_unresolvable_bundle_in_place() {
        let orphan = make_item("orphan", &["top"], &["nowhere"]);
        let map = build_map(&[orphan]);
        let worn = slots(&["orphan"], &[], &[], &[]);
        assert_eq!(
            dissolve_bundles_in_slots(&worn, &map).top,
            vec!["orphan".to_string()]
        );
    }
}
