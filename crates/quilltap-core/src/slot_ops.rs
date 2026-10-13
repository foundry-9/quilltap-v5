//! Slot operations — the pure math of putting things on and taking them off
//! (v4 `lib/wardrobe/slot-ops.ts`, `9ecebf986`; formerly
//! `dissolve-bundles.ts` + the pure half of `outfit-displacement.ts`, whose
//! v5 homes — `dissolve_bundles` and the three mutators in
//! [`crate::wardrobe`] — were LANE-REMOVE shims over this module until P4.D282
//! re-pointed their last callers and deleted them).
//!
//! Every "put it on" gesture obeys a single rule, keyed on the item's
//! `replace` flag and applied to *each* slot the item's `types` designate:
//!
//! - `replace: false` (the default for both leaf garments and additive
//!   composites) — the item is *layered* into the slot: its id is appended,
//!   keeping whatever is already there.
//! - `replace: true` — the item *replaces* the slot: the slot becomes just
//!   `[item.id]`. Used for full-outfit swaps and "clear everything"
//!   composites like Naked.
//!
//! Composites (items with `componentItemIds`) dissolve as they go on: the
//! leaves are stored in the slots their own `types` declare and the
//! composite's id never lands in equipped state, so what you're wearing always
//! reads as garments rather than as one opaque card over empty slot rows.
//! Dissolution is deliberately *total* — [`expand_composites`] walks to
//! leaves, so a composite nested inside a composite dissolves too. It is also
//! fail-safe: a composite whose parts can't be resolved (they live in a store
//! this caller can't see) is stored whole, the pre-4.8.1 behaviour, and
//! read-time `expand_composites` still covers it. Wearing something never
//! resolves to wearing nothing.
//!
//! Pure and SILENT — no I/O and no logger, so the client's optimistic updates
//! run exactly the code the server runs. The persisted primitive (load → this
//! → commit) is [`crate::services::outfit_displacement::apply_displacement`];
//! the server logs a malformed graph from [`analyze_composite`]'s report, not
//! from here (the two `[dissolveBundleToLeaves]` WARNs the old module logged
//! are gone with it — v4 deleted them in `9ecebf986`).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

pub use crate::db::wardrobe_wear_stats::WornBundle;
use crate::wardrobe::{
    add_id_to_slot, expand_composites, is_composite, remove_id_from_slot, Slots,
    WARDROBE_SLOT_TYPES,
};

/// The minimum an item has to expose to be dissolved or worn (v4
/// `WearableNode`). v4 states it structurally so the server's full
/// `WardrobeItem` and the client's lighter summaries both satisfy it; v5
/// states it as an owned struct the call sites build from whichever shape they
/// hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WearableNode {
    pub id: String,
    pub types: Vec<String>,
    /// v4's optional `componentItemIds` — absent reads as empty (`?? []`).
    pub component_item_ids: Vec<String>,
    /// v4's optional `replace` — absent reads as `false`.
    pub replace: bool,
}

impl WearableNode {
    /// Build from the pieces the persisted equip primitives already carry
    /// (`replace` false — chain [`WearableNode::with_replace`] for the flag).
    pub fn new(id: &str, types: &[String], component_item_ids: &[String]) -> Self {
        WearableNode {
            id: id.to_string(),
            types: types.to_vec(),
            component_item_ids: component_item_ids.to_vec(),
            replace: false,
        }
    }

    /// The same node with its `replace` flag set.
    pub fn with_replace(mut self, replace: bool) -> Self {
        self.replace = replace;
        self
    }

    /// Build from a wardrobe-item JSON value: a missing / non-array
    /// `componentItemIds` reads as empty (v4's `?? []`), and `replace` reads
    /// as v4's `item.replace === true` (anything but boolean `true` is off).
    pub fn from_value(item: &Value) -> Self {
        WearableNode {
            id: item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            types: string_array(item, "types"),
            component_item_ids: string_array(item, "componentItemIds"),
            replace: item.get("replace").and_then(Value::as_bool) == Some(true),
        }
    }
}

/// Item lookup used to resolve a composite's components (v4 `WearableLookup`,
/// a `ReadonlyMap<string, WearableNode>`). v5 reuses the `id → item JSON` map
/// [`expand_composites`] already walks, so no conversion is needed at any call
/// site.
pub type WearableLookup = HashMap<String, Value>;

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

/// v4 `slotsOf(item)` — the recognized slots an item covers, filtering out
/// anything unknown. Order is the item's OWN `types` order (v4 filters the
/// array, it does not re-sort into canonical slot order). Private in v4; pub
/// here for the harness (`dissolve_bundles_equivalence`'s `coveredSlots`).
pub fn slots_of(item: &WearableNode) -> Vec<String> {
    item.types
        .iter()
        .filter(|t| WARDROBE_SLOT_TYPES.contains(&t.as_str()))
        .cloned()
        .collect()
}

/// What dissolving a composite found — the leaves, plus what went wrong on
/// the way (v4 `CompositeAnalysis`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositeAnalysis {
    /// `None` = wear the item as its own id (not a composite, no lookup, or
    /// nothing resolved).
    pub leaves: Option<Vec<DissolvedLeaf>>,
    pub cycles: usize,
    pub truncated: bool,
    /// True when the item is a composite and a lookup was given but no part
    /// was wearable.
    pub unresolved: bool,
}

/// v4 `analyzeComposite` — dissolve a composite and report how it went. The
/// server logs from this; the client only needs
/// [`dissolve_composite_to_leaves`]. SILENT.
pub fn analyze_composite(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> CompositeAnalysis {
    let Some(items_by_id) = items_by_id else {
        return CompositeAnalysis::default();
    };
    if !is_composite(&item.component_item_ids) {
        return CompositeAnalysis::default();
    }

    let expanded = expand_composites(&item.component_item_ids, items_by_id, None);

    let mut leaves: Vec<DissolvedLeaf> = Vec::new();
    for leaf_id in &expanded.leaf_ids {
        // A component pointing back at its own composite would have the
        // composite wear itself. `expand_composites` truncates the cycle; drop
        // the echo here.
        if *leaf_id == item.id {
            continue;
        }
        let Some(leaf) = items_by_id.get(leaf_id) else {
            continue;
        };
        let slots = slots_of(&WearableNode::from_value(leaf));
        if slots.is_empty() {
            continue;
        }
        leaves.push(DissolvedLeaf {
            id: leaf_id.clone(),
            slots,
        });
    }

    let unresolved = leaves.is_empty();
    CompositeAnalysis {
        leaves: if unresolved { None } else { Some(leaves) },
        cycles: expanded.cycles.len(),
        truncated: expanded.truncated,
        unresolved,
    }
}

/// v4 `dissolveCompositeToLeaves` — expand a composite into the leaf garments
/// that should be worn in its place, or `None` — "wear this item as its own
/// id" — when the item isn't a composite, no lookup was supplied, or not one
/// component resolved to something wearable.
pub fn dissolve_composite_to_leaves(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Option<Vec<DissolvedLeaf>> {
    analyze_composite(item, items_by_id).leaves
}

/// v4 `layLeavesIntoSlots` — lay a dissolved composite's leaves into a slots
/// snapshot. Each leaf lands in every slot its *own* `types` declare.
/// `clear_covered_slots` is the composite's `replace` gesture, and it clears
/// the union of the composite's own `types` and every slot its leaves occupy
/// — an assembled outfit that brings boots swaps the boots already on rather
/// than layering over them.
pub fn lay_leaves_into_slots(
    current_slots: &Slots,
    composite: &WearableNode,
    leaves: &[DissolvedLeaf],
    clear_covered_slots: bool,
) -> Slots {
    let mut slots = current_slots.clone();

    if clear_covered_slots {
        // v4's `Set<WardrobeItemType>` — only recognized slot names get in.
        let mut covered: Vec<String> = Vec::new();
        for slot in slots_of(composite) {
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
            slots.slot_mut(slot).clear();
        }
    }

    for leaf in leaves {
        for slot in &leaf.slots {
            slots = add_id_to_slot(&slots, slot, &leaf.id);
        }
    }

    slots
}

/// v4 `dissolveCompositesInSlots` — dissolve every composite already sitting
/// in a slots snapshot, reporting which ones it dissolved and the leaves each
/// contributed — the claim the wear ledger needs to credit an outfit as worn.
///
/// For the snapshot builders (the default outfit, the cheap LLM's chat-start
/// pick, a manual selection) which compose slots directly instead of going
/// through the wear primitives. Layer order is preserved: a composite's leaves
/// are substituted where its id sat; leaves covering a slot it never occupied
/// are appended to that slot. Composites that can't be resolved stay as they
/// are.
///
/// `only` restricts the pass to the named composites — the "Break apart"
/// gesture on one legacy outfit card, which must dissolve exactly as wearing
/// it would.
///
/// Returns v4's `{ slots, wornBundles }` as a pair.
pub fn dissolve_composites_in_slots(
    current_slots: &Slots,
    items_by_id: &WearableLookup,
    only: Option<&HashSet<String>>,
) -> (Slots, Vec<WornBundle>) {
    // v4's `Map<string, DissolvedLeaf[]>` — insertion-ordered, and the second
    // pass below walks it in that order, so a Vec is the faithful shape.
    let mut dissolved: Vec<(String, Vec<DissolvedLeaf>)> = Vec::new();
    let position_of = |dissolved: &[(String, Vec<DissolvedLeaf>)], id: &str| -> Option<usize> {
        dissolved.iter().position(|(k, _)| k == id)
    };
    for slot in WARDROBE_SLOT_TYPES {
        for id in current_slots.slot(slot) {
            if position_of(&dissolved, id).is_some() {
                continue;
            }
            if only.is_some_and(|only| !only.contains(id)) {
                continue;
            }
            let Some(item) = items_by_id.get(id) else {
                continue;
            };
            if let Some(leaves) =
                dissolve_composite_to_leaves(&WearableNode::from_value(item), Some(items_by_id))
            {
                dissolved.push((id.clone(), leaves));
            }
        }
    }

    if dissolved.is_empty() {
        return (current_slots.clone(), Vec::new());
    }

    let mut next = Slots::default();

    // Substitute in place, so a composite's parts inherit its position in the
    // layering order rather than jumping to the end of the slot.
    for slot in WARDROBE_SLOT_TYPES {
        for id in current_slots.slot(slot) {
            let Some(at) = position_of(&dissolved, id) else {
                let arr = next.slot_mut(slot);
                if !arr.contains(id) {
                    arr.push(id.clone());
                }
                continue;
            };
            for leaf in &dissolved[at].1 {
                if leaf.slots.iter().any(|s| s == slot) {
                    let arr = next.slot_mut(slot);
                    if !arr.contains(&leaf.id) {
                        arr.push(leaf.id.clone());
                    }
                }
            }
        }
    }

    // A leaf whose slot the composite never claimed still belongs in that slot.
    for (_, leaves) in &dissolved {
        for leaf in leaves {
            for slot in &leaf.slots {
                let arr = next.slot_mut(slot);
                if !arr.contains(&leaf.id) {
                    arr.push(leaf.id.clone());
                }
            }
        }
    }

    let worn_bundles = dissolved
        .into_iter()
        .map(|(id, leaves)| WornBundle {
            id,
            leaf_ids: leaves.into_iter().map(|leaf| leaf.id).collect(),
        })
        .collect();
    (next, worn_bundles)
}

/// v4 `wornBundlesFor` — the composite credit a put-on gesture claims: the
/// composite and the leaves it dissolved into, or nothing when the item is a
/// leaf or could not dissolve. With `only_slot` (`add_to_slot`), only the
/// leaves covering that slot count; none ⇒ nothing.
pub fn worn_bundles_for(
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
    only_slot: Option<&str>,
) -> Vec<WornBundle> {
    let Some(leaves) = dissolve_composite_to_leaves(item, items_by_id) else {
        return Vec::new();
    };
    let contributed: Vec<String> = leaves
        .into_iter()
        .filter(|leaf| only_slot.is_none_or(|slot| leaf.slots.iter().any(|s| s == slot)))
        .map(|leaf| leaf.id)
        .collect();
    if contributed.is_empty() {
        return Vec::new();
    }
    vec![WornBundle {
        id: item.id.clone(),
        leaf_ids: contributed,
    }]
}

/// v4 `wearItemIntoSlots` — flag-driven wear: for each slot in `item.types`,
/// replace the slot with `[item.id]` when `item.replace` is true, otherwise
/// append `item.id`. A composite goes on as its parts; `replace` still clears
/// the slots it lands in.
pub fn wear_item_into_slots(
    current_slots: &Slots,
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Slots {
    if let Some(leaves) = dissolve_composite_to_leaves(item, items_by_id) {
        return lay_leaves_into_slots(current_slots, item, &leaves, item.replace);
    }

    let mut slots = current_slots.clone();
    for slot_type in slots_of(item) {
        if item.replace {
            *slots.slot_mut(&slot_type) = vec![item.id.clone()];
        } else {
            slots = add_id_to_slot(&slots, &slot_type, &item.id);
        }
    }
    slots
}

/// v4 `replaceItemIntoSlots` — force-swap: clear each slot in `item.types` and
/// put the item there, ignoring the flag. (Private in v4, reached through
/// [`compute_displaced_slots`]'s `replace` mode; pub here for the
/// `wardrobe::replace_item_into_slots` shim.)
pub fn replace_item_into_slots(
    current_slots: &Slots,
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Slots {
    if let Some(leaves) = dissolve_composite_to_leaves(item, items_by_id) {
        return lay_leaves_into_slots(current_slots, item, &leaves, true);
    }

    let mut slots = current_slots.clone();
    for slot_type in slots_of(item) {
        *slots.slot_mut(&slot_type) = vec![item.id.clone()];
    }
    slots
}

/// v4 `addItemToSlot` — single-slot layering. A composite contributes the
/// parts that cover this slot rather than its own id; if none do, the
/// composite's id goes in so the gesture is never silently a no-op.
pub fn add_item_to_slot(
    current_slots: &Slots,
    slot: &str,
    item: &WearableNode,
    items_by_id: Option<&WearableLookup>,
) -> Slots {
    let leaves = dissolve_composite_to_leaves(item, items_by_id);
    let for_slot: Vec<&DissolvedLeaf> = leaves
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|leaf| leaf.slots.iter().any(|s| s == slot))
        .collect();

    if !for_slot.is_empty() {
        let mut slots = current_slots.clone();
        for leaf in for_slot {
            slots = add_id_to_slot(&slots, slot, &leaf.id);
        }
        return slots;
    }

    add_id_to_slot(current_slots, slot, &item.id)
}

/// The five gestures, by name (v4 `DisplacementMode`):
///
/// - `wear` — the flag-driven rule ([`wear_item_into_slots`]).
/// - `replace` — force-swap, ignoring the flag.
/// - `add_to_slot` — layer onto one named slot.
/// - `remove_from_slot` — take one item out of one slot (`item_id` omitted:
///   clear the slot). Single-slot by design; the `wardrobe_remove` tool's
///   "take it off everywhere" is a loop of these over the slots the item
///   occupies.
/// - `clear_slot` — empty one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplacementMode {
    Wear,
    Replace,
    AddToSlot,
    RemoveFromSlot,
    ClearSlot,
}

impl DisplacementMode {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DisplacementMode::Wear => "wear",
            DisplacementMode::Replace => "replace",
            DisplacementMode::AddToSlot => "add_to_slot",
            DisplacementMode::RemoveFromSlot => "remove_from_slot",
            DisplacementMode::ClearSlot => "clear_slot",
        }
    }

    /// The inverse of [`DisplacementMode::as_str`]; `None` for anything else.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "wear" => DisplacementMode::Wear,
            "replace" => DisplacementMode::Replace,
            "add_to_slot" => DisplacementMode::AddToSlot,
            "remove_from_slot" => DisplacementMode::RemoveFromSlot,
            "clear_slot" => DisplacementMode::ClearSlot,
            _ => return None,
        })
    }

    /// The take-off modes (`remove_from_slot`, `clear_slot`) — what
    /// `applyDisplacement` always commits as `'take-off'`.
    pub fn is_take_off(self) -> bool {
        matches!(
            self,
            DisplacementMode::RemoveFromSlot | DisplacementMode::ClearSlot
        )
    }
}

/// v4 `ComputeDisplacedOptions`.
#[derive(Debug, Clone, Copy)]
pub struct ComputeDisplacedOptions<'a> {
    pub mode: DisplacementMode,
    /// Required for `wear`, `replace`, and `add_to_slot`.
    pub item: Option<&'a WearableNode>,
    /// Required for `add_to_slot`, `remove_from_slot`, `clear_slot`.
    pub slot: Option<&'a str>,
    /// Filter target for `remove_from_slot`; omit to clear the slot.
    pub item_id: Option<&'a str>,
    /// Item lookup used to dissolve a composite as it goes on.
    pub items_by_id: Option<&'a WearableLookup>,
}

/// v4 `computeDisplacedSlots` — the one mode → primitive table. Returns a
/// fresh slots object (a missing `item` / `slot` is v4's
/// `cloneEquippedSlots(currentSlots)` — the gesture is a no-op copy).
pub fn compute_displaced_slots(
    current_slots: &Slots,
    options: &ComputeDisplacedOptions<'_>,
) -> Slots {
    let ComputeDisplacedOptions {
        mode,
        item,
        slot,
        item_id,
        items_by_id,
    } = *options;
    match mode {
        DisplacementMode::Wear => match item {
            Some(item) => wear_item_into_slots(current_slots, item, items_by_id),
            None => current_slots.clone(),
        },
        DisplacementMode::Replace => match item {
            Some(item) => replace_item_into_slots(current_slots, item, items_by_id),
            None => current_slots.clone(),
        },
        DisplacementMode::AddToSlot => match (item, slot) {
            (Some(item), Some(slot)) => add_item_to_slot(current_slots, slot, item, items_by_id),
            _ => current_slots.clone(),
        },
        DisplacementMode::RemoveFromSlot => match slot {
            Some(slot) => remove_id_from_slot(current_slots, slot, item_id),
            None => current_slots.clone(),
        },
        DisplacementMode::ClearSlot => match slot {
            Some(slot) => remove_id_from_slot(current_slots, slot, None),
            None => current_slots.clone(),
        },
    }
}

/// v4 `wornBundlesForMode` — the composites a mode's gesture would credit to
/// the wear ledger as worn.
pub fn worn_bundles_for_mode(options: &ComputeDisplacedOptions<'_>) -> Vec<WornBundle> {
    let Some(item) = options.item else {
        return Vec::new();
    };
    match options.mode {
        DisplacementMode::Wear | DisplacementMode::Replace => {
            worn_bundles_for(item, options.items_by_id, None)
        }
        DisplacementMode::AddToSlot => worn_bundles_for(item, options.items_by_id, options.slot),
        DisplacementMode::RemoveFromSlot | DisplacementMode::ClearSlot => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    //! v4's `__tests__/unit/lib/wardrobe/slot-ops.test.ts` (the pure half;
    //! the `applyDisplacement` block lives with
    //! `services::outfit_displacement`), case for case.
    use super::*;
    use serde_json::json;

    const NOW: &str = "2026-01-01T00:00:00.000Z";

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
            .map(|i| (i["id"].as_str().unwrap().to_string(), i.clone()))
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
            hair: Vec::new(),
        }
    }

    fn leaf(id: &str, s: &[&str]) -> DissolvedLeaf {
        DissolvedLeaf {
            id: id.to_string(),
            slots: s.iter().map(|x| x.to_string()).collect(),
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
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

    fn shirt() -> Value {
        wardrobe().0[0].clone()
    }
    fn man_in_black() -> Value {
        wardrobe().0[4].clone()
    }

    // ── isComposite ─────────────────────────────────────────────────────────

    #[test]
    fn is_composite_only_when_the_item_has_components() {
        assert!(is_composite(&node(&man_in_black()).component_item_ids));
        assert!(!is_composite(&node(&shirt()).component_item_ids));
        assert!(!is_composite(
            &node(&json!({ "id": "x", "types": ["top"] })).component_item_ids
        ));
    }

    #[test]
    fn from_value_reads_replace_as_strict_true() {
        assert!(node(&json!({ "id": "x", "types": ["top"], "replace": true })).replace);
        assert!(!node(&json!({ "id": "x", "types": ["top"], "replace": "yes" })).replace);
        assert!(!node(&json!({ "id": "x", "types": ["top"] })).replace);
        assert!(WearableNode::new("x", &[], &[]).with_replace(true).replace);
    }

    // ── dissolveCompositeToLeaves ───────────────────────────────────────────

    #[test]
    fn dissolve_expands_a_bundle_into_its_parts_with_the_slots_each_covers() {
        let (_, map) = wardrobe();
        assert_eq!(
            dissolve_composite_to_leaves(&node(&man_in_black()), Some(&map)),
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
            dissolve_composite_to_leaves(&node(&gala), Some(&map)),
            Some(vec![
                leaf("gown", &["top", "bottom"]),
                leaf("locket", &["accessories"]),
                leaf("ring", &["accessories"]),
            ])
        );
    }

    #[test]
    fn dissolve_returns_none_for_a_plain_garment() {
        let (_, map) = wardrobe();
        assert_eq!(
            dissolve_composite_to_leaves(&node(&shirt()), Some(&map)),
            None
        );
    }

    #[test]
    fn dissolve_returns_none_when_no_lookup_is_supplied() {
        assert_eq!(
            dissolve_composite_to_leaves(&node(&man_in_black()), None),
            None
        );
    }

    #[test]
    fn dissolve_returns_none_when_not_one_component_resolves() {
        let orphan = make_item("orphan", &["top"], &["nowhere-1", "nowhere-2"]);
        let map = build_map(std::slice::from_ref(&orphan));
        assert_eq!(
            dissolve_composite_to_leaves(&node(&orphan), Some(&map)),
            None
        );
    }

    #[test]
    fn dissolve_drops_a_component_that_points_back_at_its_own_bundle() {
        let selfish = make_item("selfish", &["top"], &["selfish", "shirt"]);
        let map = build_map(&[selfish.clone(), shirt()]);
        assert_eq!(
            dissolve_composite_to_leaves(&node(&selfish), Some(&map)),
            Some(vec![leaf("shirt", &["top"])])
        );
    }

    #[test]
    fn dissolve_skips_parts_that_cover_no_recognized_slot() {
        let junk = make_item("junk", &["hat"], &[]);
        let bundle = make_item("bundle", &["top"], &["shirt", "junk"]);
        let map = build_map(&[shirt(), junk, bundle.clone()]);
        assert_eq!(
            dissolve_composite_to_leaves(&node(&bundle), Some(&map)),
            Some(vec![leaf("shirt", &["top"])])
        );
    }

    // ── analyzeComposite ────────────────────────────────────────────────────

    #[test]
    fn analyze_reports_cycles_truncation_and_unresolved() {
        let (_, map) = wardrobe();
        let clean = analyze_composite(&node(&man_in_black()), Some(&map));
        assert_eq!(
            (clean.cycles, clean.truncated, clean.unresolved),
            (0, false, false)
        );
        assert!(clean.leaves.is_some());

        assert_eq!(
            analyze_composite(&node(&man_in_black()), None),
            CompositeAnalysis::default()
        );
        assert_eq!(
            analyze_composite(&node(&shirt()), Some(&map)),
            CompositeAnalysis::default()
        );

        let orphan = make_item("orphan", &["top"], &["nowhere"]);
        let a = analyze_composite(
            &node(&orphan),
            Some(&build_map(std::slice::from_ref(&orphan))),
        );
        assert_eq!(
            (a.leaves, a.cycles, a.truncated, a.unresolved),
            (None, 0, false, true)
        );

        let yin = make_item("yin", &["top"], &["yang", "shirt"]);
        let yang = make_item("yang", &["bottom"], &["yin", "trousers"]);
        let map = build_map(&[yin.clone(), yang, shirt(), wardrobe().0[1].clone()]);
        let a = analyze_composite(&node(&yin), Some(&map));
        assert_eq!(a.cycles, 1);
        assert!(!a.unresolved);

        let chain: Vec<Value> = (1..=5)
            .map(|n| {
                let next = if n == 5 {
                    "shirt".to_string()
                } else {
                    format!("chain-{}", n + 1)
                };
                make_item(&format!("chain-{n}"), &["top"], &[&next])
            })
            .chain(std::iter::once(shirt()))
            .collect();
        let a = analyze_composite(&node(&chain[0]), Some(&build_map(&chain)));
        assert!(a.truncated);
    }

    // ── layLeavesIntoSlots ──────────────────────────────────────────────────

    fn two_leaves() -> Vec<DissolvedLeaf> {
        vec![leaf("shirt", &["top"]), leaf("boots", &["footwear"])]
    }

    #[test]
    fn lay_layers_parts_over_what_is_already_worn_when_not_replacing() {
        let worn = slots(&["vest"], &[], &["sandals"], &[]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&man_in_black()), &two_leaves(), false),
            slots(&["vest", "shirt"], &[], &["sandals", "boots"], &[])
        );
    }

    #[test]
    fn lay_clears_the_slots_it_lands_in_when_replacing() {
        let worn = slots(&["vest"], &["skirt"], &["sandals"], &["hat"]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&man_in_black()), &two_leaves(), true),
            slots(&["shirt"], &[], &["boots"], &[])
        );
    }

    #[test]
    fn lay_replacing_also_clears_slots_the_bundle_itself_does_not_claim() {
        let top_only = make_item("top-only", &["top"], &["shirt", "boots"]);
        let worn = slots(&["vest"], &[], &["sandals"], &[]);
        assert_eq!(
            lay_leaves_into_slots(&worn, &node(&top_only), &two_leaves(), true),
            slots(&["shirt"], &[], &["boots"], &[])
        );
    }

    #[test]
    fn lay_does_not_duplicate_a_part_already_worn() {
        let worn = slots(&["shirt"], &[], &[], &[]);
        let result = lay_leaves_into_slots(&worn, &node(&man_in_black()), &two_leaves(), false);
        assert_eq!(result.top, ids(&["shirt"]));
    }

    // ── wearItemIntoSlots ───────────────────────────────────────────────────

    #[test]
    fn wear_stores_the_parts_never_the_bundle_id() {
        let (_, map) = wardrobe();
        let result = wear_item_into_slots(&Slots::fresh(), &node(&man_in_black()), Some(&map));
        assert_eq!(
            result,
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
        assert!(!result.to_value().to_string().contains("man-in-black"));
    }

    #[test]
    fn wear_layers_the_parts_over_what_is_worn_by_default() {
        let (_, map) = wardrobe();
        let worn = slots(&["vest"], &[], &[], &[]);
        assert_eq!(
            wear_item_into_slots(&worn, &node(&man_in_black()), Some(&map)).top,
            ids(&["vest", "shirt"])
        );
    }

    #[test]
    fn wear_honours_the_replace_flag() {
        let (_, map) = wardrobe();
        let replacing = node(&man_in_black()).with_replace(true);
        let worn = slots(&["vest"], &["skirt"], &["sandals"], &["hat"]);
        assert_eq!(
            wear_item_into_slots(&worn, &replacing, Some(&map)),
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
    }

    #[test]
    fn wear_falls_back_to_storing_the_bundle_whole_without_a_lookup() {
        assert_eq!(
            wear_item_into_slots(&Slots::fresh(), &node(&man_in_black()), None),
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
        let (_, map) = wardrobe();
        assert_eq!(
            wear_item_into_slots(&Slots::fresh(), &node(&shirt()), Some(&map)),
            slots(&["shirt"], &[], &[], &[])
        );
    }

    #[test]
    fn wear_ignores_an_unrecognized_type() {
        // v4's `slotsOf` filters `types`; an unknown name lands nowhere (the
        // old mutator routed it into `top`).
        let odd = node(&make_item("odd", &["hat", "top"], &[]));
        assert_eq!(
            wear_item_into_slots(&Slots::fresh(), &odd, None),
            slots(&["odd"], &[], &[], &[])
        );
        assert_eq!(
            replace_item_into_slots(&Slots::fresh(), &odd, None),
            slots(&["odd"], &[], &[], &[])
        );
    }

    // ── computeDisplacedSlots({ mode: 'replace' }) ──────────────────────────

    #[test]
    fn replace_mode_clears_the_slots_and_stores_the_parts() {
        let (_, map) = wardrobe();
        let worn = slots(&["vest", "coat"], &["skirt"], &[], &[]);
        let mib = node(&man_in_black());
        let out = compute_displaced_slots(
            &worn,
            &ComputeDisplacedOptions {
                mode: DisplacementMode::Replace,
                item: Some(&mib),
                slot: None,
                item_id: None,
                items_by_id: Some(&map),
            },
        );
        assert_eq!(
            out,
            slots(&["shirt"], &["trousers"], &["boots"], &["gloves"])
        );
    }

    // ── addItemToSlot ───────────────────────────────────────────────────────

    #[test]
    fn add_to_slot_adds_only_the_parts_covering_the_named_slot() {
        let (_, map) = wardrobe();
        assert_eq!(
            add_item_to_slot(&Slots::fresh(), "top", &node(&man_in_black()), Some(&map)),
            slots(&["shirt"], &[], &[], &[])
        );
    }

    #[test]
    fn add_to_slot_falls_back_to_the_bundle_id_when_no_part_covers_the_slot() {
        let top_only = make_item("top-only", &["top", "bottom"], &["shirt"]);
        let map = build_map(&[shirt(), top_only.clone()]);
        assert_eq!(
            add_item_to_slot(&Slots::fresh(), "bottom", &node(&top_only), Some(&map)).bottom,
            ids(&["top-only"])
        );
    }

    // ── wornBundlesFor ──────────────────────────────────────────────────────

    #[test]
    fn worn_bundles_names_the_composite_and_its_leaves() {
        let (_, map) = wardrobe();
        assert_eq!(
            worn_bundles_for(&node(&man_in_black()), Some(&map), None),
            vec![WornBundle {
                id: "man-in-black".into(),
                leaf_ids: ids(&["shirt", "trousers", "boots", "gloves"]),
            }]
        );
        assert_eq!(
            worn_bundles_for(&node(&man_in_black()), Some(&map), Some("footwear")),
            vec![WornBundle {
                id: "man-in-black".into(),
                leaf_ids: ids(&["boots"]),
            }]
        );
        assert!(worn_bundles_for(&node(&shirt()), Some(&map), None).is_empty());
        assert!(worn_bundles_for(&node(&man_in_black()), None, None).is_empty());
    }

    // ── computeDisplacedSlots take-off modes + fallbacks ────────────────────

    fn opts<'a>(
        mode: DisplacementMode,
        item: Option<&'a WearableNode>,
        slot: Option<&'a str>,
        item_id: Option<&'a str>,
    ) -> ComputeDisplacedOptions<'a> {
        ComputeDisplacedOptions {
            mode,
            item,
            slot,
            item_id,
            items_by_id: None,
        }
    }

    #[test]
    fn take_off_modes() {
        let worn = slots(&["vest", "shirt"], &["skirt"], &[], &[]);
        assert_eq!(
            compute_displaced_slots(
                &worn,
                &opts(
                    DisplacementMode::RemoveFromSlot,
                    None,
                    Some("top"),
                    Some("vest")
                )
            )
            .top,
            ids(&["shirt"])
        );
        assert!(compute_displaced_slots(
            &worn,
            &opts(DisplacementMode::RemoveFromSlot, None, Some("top"), None)
        )
        .top
        .is_empty());
        let next = compute_displaced_slots(
            &worn,
            &opts(DisplacementMode::ClearSlot, None, Some("top"), None),
        );
        assert_eq!(next, slots(&[], &["skirt"], &[], &[]));
        assert_eq!(worn.top, ids(&["vest", "shirt"]));
    }

    #[test]
    fn a_missing_item_or_slot_is_a_no_op_copy() {
        let worn = slots(&["vest"], &[], &[], &[]);
        let shirt = node(&shirt());
        for mode in [
            DisplacementMode::Wear,
            DisplacementMode::Replace,
            DisplacementMode::AddToSlot,
            DisplacementMode::RemoveFromSlot,
            DisplacementMode::ClearSlot,
        ] {
            assert_eq!(
                compute_displaced_slots(&worn, &opts(mode, None, None, None)),
                worn
            );
        }
        // add_to_slot with an item but no slot.
        assert_eq!(
            compute_displaced_slots(
                &worn,
                &opts(DisplacementMode::AddToSlot, Some(&shirt), None, None)
            ),
            worn
        );
        assert!(worn_bundles_for_mode(&opts(DisplacementMode::Wear, None, None, None)).is_empty());
        assert!(worn_bundles_for_mode(&opts(
            DisplacementMode::ClearSlot,
            Some(&shirt),
            Some("top"),
            None
        ))
        .is_empty());
    }

    #[test]
    fn mode_wire_round_trips() {
        for mode in [
            DisplacementMode::Wear,
            DisplacementMode::Replace,
            DisplacementMode::AddToSlot,
            DisplacementMode::RemoveFromSlot,
            DisplacementMode::ClearSlot,
        ] {
            assert_eq!(DisplacementMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(DisplacementMode::parse("set_all"), None);
        assert!(DisplacementMode::ClearSlot.is_take_off());
        assert!(!DisplacementMode::Wear.is_take_off());
    }

    // ── dissolveCompositesInSlots ───────────────────────────────────────────

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
            dissolve_composites_in_slots(&worn, &map, None).0,
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
        let hat = make_item("hat", &["accessories"], &[]);
        let kit = make_item("kit", &["top"], &["shirt", "hat"]);
        let map = build_map(&[shirt(), hat, kit]);
        let worn = slots(&["kit"], &[], &[], &[]);
        assert_eq!(
            dissolve_composites_in_slots(&worn, &map, None).0,
            slots(&["shirt"], &[], &[], &["hat"])
        );
    }

    #[test]
    fn dissolve_in_slots_returns_the_snapshot_untouched_when_nothing_is_a_bundle() {
        let (_, map) = wardrobe();
        let worn = slots(&["shirt"], &["trousers"], &[], &[]);
        let (out, bundles) = dissolve_composites_in_slots(&worn, &map, None);
        assert_eq!(out, worn);
        assert!(bundles.is_empty());
    }

    #[test]
    fn dissolve_in_slots_leaves_an_unresolvable_bundle_in_place() {
        let orphan = make_item("orphan", &["top"], &["nowhere"]);
        let map = build_map(&[orphan]);
        let worn = slots(&["orphan"], &[], &[], &[]);
        assert_eq!(
            dissolve_composites_in_slots(&worn, &map, None).0.top,
            ids(&["orphan"])
        );
    }

    #[test]
    fn dissolve_in_slots_reports_each_dissolved_composite_for_the_ledger() {
        let (_, map) = wardrobe();
        let worn = slots(&["man-in-black"], &[], &[], &[]);
        assert_eq!(
            dissolve_composites_in_slots(&worn, &map, None).1,
            vec![WornBundle {
                id: "man-in-black".into(),
                leaf_ids: ids(&["shirt", "trousers", "boots", "gloves"]),
            }]
        );
    }

    #[test]
    fn dissolve_in_slots_restricted_by_only_dissolves_just_the_named_composite() {
        let (items, _) = wardrobe();
        let hat = make_item("hat", &["accessories"], &[]);
        let kit = make_item("kit", &["accessories"], &["hat"]);
        let mut all = items.clone();
        all.push(hat);
        all.push(kit);
        let map = build_map(&all);
        let worn = slots(&["man-in-black"], &[], &[], &["kit"]);
        let only: HashSet<String> = ["kit".to_string()].into_iter().collect();
        let (out, bundles) = dissolve_composites_in_slots(&worn, &map, Some(&only));
        assert_eq!(out.top, ids(&["man-in-black"]));
        assert_eq!(out.accessories, ids(&["hat"]));
        assert_eq!(
            bundles,
            vec![WornBundle {
                id: "kit".into(),
                leaf_ids: ids(&["hat"]),
            }]
        );
    }
}
