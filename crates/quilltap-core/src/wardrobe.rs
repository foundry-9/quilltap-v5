//! Pure wardrobe leaves the wardrobe tool handlers compose (v4 `lib/wardrobe/`,
//! the no-DB parts). Everything DB-touching (the equip primitives that read/write
//! `chats.equippedOutfit`, the across-tier item resolver, the coverage summary)
//! lives in [`crate::tools::wardrobe_shared`]; this module holds only the pure
//! functions:
//!
//! - [`union_types`] (v4 `composite-types.ts` `unionTypes`) — the canonical
//!   slot-order union of a composite's components.
//! - [`describe_outfit`] (v4 `outfit-description.ts` `describeOutfit`) — the
//!   byte-load-bearing markdown rendering of an equipped outfit.
//! - [`expand_composites`] (v4 `expand-composites.ts`) — cycle-tolerant expansion
//!   of composite item ids into their leaf components.
//! - [`wear_item_into_slots`] / [`replace_item_into_slots`] (v4
//!   `outfit-displacement.ts`, the pure variants) — the flag-driven slot mutation.
//! - [`describe_wardrobe_effect`] (v4 `wardrobe-handler-shared.ts`) — the
//!   one-sentence layer-vs-replace description.
//! - [`normalize_no_item_sentinel`] (v4 `wardrobe-handler-shared.ts`) — the
//!   `"none"`/`"null"`/`""` → absent coercion.

use std::collections::HashSet;

use serde_json::Value;

/// The canonical slot order (v4 `WARDROBE_SLOT_TYPES`). Every union / render walks
/// this order so output is deterministic.
pub const WARDROBE_SLOT_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

// ===========================================================================
// composite-types.ts — unionTypes
// ===========================================================================

/// v4 `unionTypes(components)` — the union of the components' slot types, in
/// canonical slot order (`top → bottom → footwear → accessories`).
pub fn union_types<'a, I>(component_types: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a [String]>,
{
    let mut set: HashSet<&str> = HashSet::new();
    for types in component_types {
        for t in types {
            set.insert(t.as_str());
        }
    }
    WARDROBE_SLOT_TYPES
        .iter()
        .filter(|s| set.contains(**s))
        .map(|s| s.to_string())
        .collect()
}

// ===========================================================================
// outfit-description.ts — describeOutfit
// ===========================================================================

/// Per-slot title arrays for [`describe_outfit`] (v4 `OutfitSlotValues`). Empty
/// array = nothing worn in that slot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutfitSlotValues {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub footwear: Vec<String>,
    pub accessories: Vec<String>,
}

/// An insertion-ordered `value → [slot]` accumulator (v4's `Map<string,
/// string[]>` — JS `Map` iterates in insertion order, which the rendered output
/// depends on).
struct OrderedGroups {
    entries: Vec<(String, Vec<String>)>,
}

impl OrderedGroups {
    fn new() -> Self {
        OrderedGroups {
            entries: Vec::new(),
        }
    }
    fn add(&mut self, slot: &str, value: String) {
        if let Some((_, slots)) = self.entries.iter_mut().find(|(v, _)| *v == value) {
            slots.push(slot.to_string());
        } else {
            self.entries.push((value, vec![slot.to_string()]));
        }
    }
}

fn join_or_fallback(items: &[String], fallback: &str) -> String {
    if items.is_empty() {
        fallback.to_string()
    } else {
        items.join(", ")
    }
}

/// v4 `describeOutfit(slots)` (no `omit`) — the markdown outfit description. The
/// handlers always call it without `omit`, so all four slots are visible; the
/// `omit`-driven branches are reproduced as the all-visible case (topless/
/// bottomless/barefoot/no-accessories fallbacks, the top+bottom "naked" collapse,
/// the shared-value slot grouping).
pub fn describe_outfit(slots: &OutfitSlotValues) -> String {
    let all_empty = slots.top.is_empty()
        && slots.bottom.is_empty()
        && slots.footwear.is_empty()
        && slots.accessories.is_empty();
    if all_empty {
        return "- completely naked and unadorned\n".to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut groups = OrderedGroups::new();

    // The "naked" collapse only applies when both top and bottom are empty.
    if slots.top.is_empty() && slots.bottom.is_empty() {
        lines.push("- naked".to_string());
    } else {
        groups.add("top", join_or_fallback(&slots.top, "topless"));
        groups.add("bottom", join_or_fallback(&slots.bottom, "bottomless"));
    }
    groups.add("footwear", join_or_fallback(&slots.footwear, "barefoot"));
    groups.add(
        "accessories",
        join_or_fallback(&slots.accessories, "no accessories"),
    );

    for (value, slots_for_value) in &groups.entries {
        lines.push(format!("- **{}:** {}", slots_for_value.join(", "), value));
    }

    format!("{}\n", lines.join("\n"))
}

// ===========================================================================
// expand-composites.ts — expandComposites
// ===========================================================================

/// v4 `ExpandResult` (the `cycles` / `truncated` fields are not consumed by the
/// wardrobe tool handlers, but are ported for fidelity).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExpandResult {
    pub leaf_ids: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub truncated: bool,
}

const DEFAULT_MAX_DEPTH: usize = 4;

/// The `componentItemIds` of an item id (`[]` for a leaf or an unknown id).
fn component_ids_of(
    items_by_id: &std::collections::HashMap<String, Value>,
    id: &str,
) -> Vec<String> {
    items_by_id
        .get(id)
        .and_then(|v| v.get("componentItemIds"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `expandComposites(rootIds, itemsById, { maxDepth })` — expand composite ids
/// into their leaf components, cycle-tolerant (a cycle truncates the branch,
/// never panics), deduped in first-seen order. Unknown ids surface as leaves.
pub fn expand_composites(
    root_ids: &[String],
    items_by_id: &std::collections::HashMap<String, Value>,
    max_depth: Option<usize>,
) -> ExpandResult {
    let max_depth = max_depth.unwrap_or(DEFAULT_MAX_DEPTH);
    let mut leaf_ids: Vec<String> = Vec::new();
    let mut seen_leaves: HashSet<String> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut truncated = false;

    // A stack frame carries the id, the path so far, and the depth (an explicit
    // stack mirrors v4's recursion without a closure-recursion dance).
    fn emit_leaf(leaf_ids: &mut Vec<String>, seen: &mut HashSet<String>, id: &str) {
        if seen.insert(id.to_string()) {
            leaf_ids.push(id.to_string());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        id: &str,
        path: &[String],
        depth: usize,
        max_depth: usize,
        items_by_id: &std::collections::HashMap<String, Value>,
        leaf_ids: &mut Vec<String>,
        seen_leaves: &mut HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
        truncated: &mut bool,
    ) {
        let known = items_by_id.contains_key(id);
        if !known {
            emit_leaf(leaf_ids, seen_leaves, id);
            return;
        }
        if path.iter().any(|p| p == id) {
            let mut cycle = path.to_vec();
            cycle.push(id.to_string());
            cycles.push(cycle);
            return;
        }
        if depth >= max_depth {
            *truncated = true;
            emit_leaf(leaf_ids, seen_leaves, id);
            return;
        }
        let components = component_ids_of(items_by_id, id);
        if components.is_empty() {
            emit_leaf(leaf_ids, seen_leaves, id);
            return;
        }
        let mut next_path = path.to_vec();
        next_path.push(id.to_string());
        for child in &components {
            visit(
                child,
                &next_path,
                depth + 1,
                max_depth,
                items_by_id,
                leaf_ids,
                seen_leaves,
                cycles,
                truncated,
            );
        }
    }

    for root in root_ids {
        visit(
            root,
            &[],
            0,
            max_depth,
            items_by_id,
            &mut leaf_ids,
            &mut seen_leaves,
            &mut cycles,
            &mut truncated,
        );
    }

    ExpandResult {
        leaf_ids,
        cycles,
        truncated,
    }
}

// ===========================================================================
// outfit-displacement.ts — the pure slot mutators
// ===========================================================================

/// The four equipped slots (v4 `EquippedSlots`), each an array of item ids. Slot
/// order is fixed (`top → bottom → footwear → accessories`) so it serializes
/// byte-identically to v4's `JSON.stringify`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slots {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub footwear: Vec<String>,
    pub accessories: Vec<String>,
}

impl Slots {
    /// A fresh empty outfit (v4 `freshSlots`).
    pub fn fresh() -> Self {
        Slots::default()
    }

    /// Read a slot's array by name (an unknown name is unreachable — the callers
    /// only pass the four valid slots).
    fn slot(&self, name: &str) -> &Vec<String> {
        match name {
            "top" => &self.top,
            "bottom" => &self.bottom,
            "footwear" => &self.footwear,
            "accessories" => &self.accessories,
            _ => &self.top,
        }
    }

    fn slot_mut(&mut self, name: &str) -> &mut Vec<String> {
        match name {
            "top" => &mut self.top,
            "bottom" => &mut self.bottom,
            "footwear" => &mut self.footwear,
            "accessories" => &mut self.accessories,
            _ => &mut self.top,
        }
    }

    /// Parse a stored `equippedOutfit[characterId]` value into [`Slots`] (v4
    /// `cloneSlots(current)` after `getEquippedOutfitForCharacter`; a missing /
    /// non-array slot reads as empty). A `None` value ⇒ `fresh()` (v4's `current ?
    /// clone : freshSlots()`).
    pub fn from_value(value: Option<&Value>) -> Self {
        let Some(v) = value else {
            return Slots::fresh();
        };
        let read = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|e| e.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        Slots {
            top: read("top"),
            bottom: read("bottom"),
            footwear: read("footwear"),
            accessories: read("accessories"),
        }
    }

    /// Serialize to the ordered `{top,bottom,footwear,accessories}` JSON object v4
    /// persists (slot order fixed; `serde_json` preserve-order keeps it).
    pub fn to_value(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("top".into(), to_str_array(&self.top));
        m.insert("bottom".into(), to_str_array(&self.bottom));
        m.insert("footwear".into(), to_str_array(&self.footwear));
        m.insert("accessories".into(), to_str_array(&self.accessories));
        Value::Object(m)
    }
}

fn to_str_array(v: &[String]) -> Value {
    Value::Array(v.iter().map(|s| Value::String(s.clone())).collect())
}

/// v4 `wearItemIntoSlots` — for each slot in `types`, replace with `[id]` when
/// `replace`, else append `id` (no-op if already present).
pub fn wear_item_into_slots(current: &Slots, id: &str, types: &[String], replace: bool) -> Slots {
    let mut slots = current.clone();
    for slot in types {
        if replace {
            *slots.slot_mut(slot) = vec![id.to_string()];
        } else if !slots.slot(slot).iter().any(|x| x == id) {
            slots.slot_mut(slot).push(id.to_string());
        }
    }
    slots
}

/// v4 `replaceItemIntoSlots` — clear each slot in `types` and set it to `[id]`,
/// ignoring the `replace` flag.
pub fn replace_item_into_slots(current: &Slots, id: &str, types: &[String]) -> Slots {
    let mut slots = current.clone();
    for slot in types {
        *slots.slot_mut(slot) = vec![id.to_string()];
    }
    slots
}

// ===========================================================================
// wardrobe-handler-shared.ts — describeWardrobeEffect + normalizeNoItemSentinel
// ===========================================================================

/// What a wardrobe mutation did to the slots it touched (v4 `WardrobeEffect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeEffect {
    Layered,
    Replaced,
    Removed,
    Cleared,
}

impl WardrobeEffect {
    /// The wire string for `effect` in a tool result.
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeEffect::Layered => "layered",
            WardrobeEffect::Replaced => "replaced",
            WardrobeEffect::Removed => "removed",
            WardrobeEffect::Cleared => "cleared",
        }
    }
}

/// v4 `describeWardrobeEffect(effect, slots, itemTitle?)` — the one-sentence,
/// model-facing description of a wardrobe mutation, phrased identically across the
/// wardrobe tools.
pub fn describe_wardrobe_effect(
    effect: WardrobeEffect,
    slots: &[String],
    item_title: Option<&str>,
) -> String {
    let slot_list = if slots.is_empty() {
        "the slot".to_string()
    } else {
        slots.join(", ")
    };
    let those = if slots.len() > 1 {
        "those slots"
    } else {
        "that slot"
    };
    let title = match item_title {
        Some(t) => format!("\"{t}\""),
        None => "the item".to_string(),
    };
    match effect {
        WardrobeEffect::Layered => format!(
            "Layered {title} into {slot_list}. The item's replace flag is off, so whatever was already in {those} was kept."
        ),
        WardrobeEffect::Replaced => format!(
            "Replaced {slot_list} with {title} — anything previously in {those} was cleared."
        ),
        WardrobeEffect::Removed => match item_title {
            Some(_) => format!("Took {title} off {slot_list}; any other layers there stayed."),
            None => format!("Cleared {slot_list}."),
        },
        WardrobeEffect::Cleared => format!("Cleared {slot_list} entirely."),
    }
}

/// v4 `NO_ITEM_SENTINELS` — strings an LLM emits for "no item".
const NO_ITEM_SENTINELS: [&str; 3] = ["none", "null", ""];

/// v4 `normalizeNoItemSentinel` — coerce a sentinel (`none`/`null`/`""`,
/// case-insensitive, trimmed) to absent; otherwise return the value UNCHANGED (not
/// trimmed).
pub fn normalize_no_item_sentinel(value: Option<&str>) -> Option<String> {
    let v = value?;
    let key = v.trim().to_lowercase();
    if NO_ITEM_SENTINELS.contains(&key.as_str()) {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn types(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn union_types_is_canonical_slot_order() {
        let a = types(&["accessories", "top"]);
        let b = types(&["bottom"]);
        let got = union_types([a.as_slice(), b.as_slice()]);
        // Canonical order regardless of input order.
        assert_eq!(got, types(&["top", "bottom", "accessories"]));
    }

    #[test]
    fn describe_outfit_naked_and_fallbacks() {
        assert_eq!(
            describe_outfit(&OutfitSlotValues::default()),
            "- completely naked and unadorned\n"
        );
        // Top+bottom empty → "- naked"; footwear/accessories fall back.
        let s = OutfitSlotValues {
            top: vec![],
            bottom: vec![],
            footwear: vec!["Boots".into()],
            accessories: vec![],
        };
        assert_eq!(
            describe_outfit(&s),
            "- naked\n- **footwear:** Boots\n- **accessories:** no accessories\n"
        );
    }

    #[test]
    fn describe_outfit_shared_value_groups_collapse() {
        // A single multi-slot item across top+bottom collapses to one line.
        let s = OutfitSlotValues {
            top: vec!["Dress".into()],
            bottom: vec!["Dress".into()],
            footwear: vec!["Heels".into()],
            accessories: vec![],
        };
        assert_eq!(
            describe_outfit(&s),
            "- **top, bottom:** Dress\n- **footwear:** Heels\n- **accessories:** no accessories\n"
        );
    }

    #[test]
    fn expand_composites_leaf_and_composite() {
        let mut map = std::collections::HashMap::new();
        map.insert("leaf".to_string(), json!({ "componentItemIds": [] }));
        map.insert(
            "comp".to_string(),
            json!({ "componentItemIds": ["leaf", "x"] }),
        );
        let r = expand_composites(&[String::from("comp")], &map, None);
        // leaf resolves to itself; unknown "x" surfaces as a leaf.
        assert_eq!(r.leaf_ids, types(&["leaf", "x"]));
        assert!(r.cycles.is_empty());
    }

    #[test]
    fn expand_composites_cycle_tolerant() {
        let mut map = std::collections::HashMap::new();
        map.insert("a".to_string(), json!({ "componentItemIds": ["b"] }));
        map.insert("b".to_string(), json!({ "componentItemIds": ["a"] }));
        let r = expand_composites(&[String::from("a")], &map, None);
        assert!(!r.cycles.is_empty());
    }

    #[test]
    fn wear_layers_and_replaces() {
        let base = Slots {
            top: vec!["shirt".into()],
            ..Default::default()
        };
        // Layer: append.
        let layered = wear_item_into_slots(&base, "vest", &types(&["top"]), false);
        assert_eq!(layered.top, types(&["shirt", "vest"]));
        // Replace: overwrite.
        let replaced = wear_item_into_slots(&base, "coat", &types(&["top"]), true);
        assert_eq!(replaced.top, types(&["coat"]));
    }

    #[test]
    fn normalize_sentinels() {
        assert_eq!(normalize_no_item_sentinel(Some("  NONE ")), None);
        assert_eq!(normalize_no_item_sentinel(Some("null")), None);
        assert_eq!(normalize_no_item_sentinel(Some("")), None);
        assert_eq!(normalize_no_item_sentinel(None), None);
        // Non-sentinel: returned UNCHANGED (not trimmed).
        assert_eq!(
            normalize_no_item_sentinel(Some(" Hat ")),
            Some(" Hat ".to_string())
        );
    }

    #[test]
    fn effect_descriptions_match_v4() {
        assert_eq!(
            describe_wardrobe_effect(WardrobeEffect::Layered, &types(&["top"]), Some("Vest")),
            "Layered \"Vest\" into top. The item's replace flag is off, so whatever was already in that slot was kept."
        );
        assert_eq!(
            describe_wardrobe_effect(WardrobeEffect::Cleared, &types(&["top", "bottom"]), None),
            "Cleared top, bottom entirely."
        );
        assert_eq!(
            describe_wardrobe_effect(WardrobeEffect::Removed, &types(&["top"]), None),
            "Cleared top."
        );
    }
}
