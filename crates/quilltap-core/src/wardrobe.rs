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
//! - [`sort_for_default_outfit`] (v4 `default-outfit.ts`) — the deterministic
//!   layer order for a default outfit.

use std::collections::HashSet;

use serde_json::Value;

use crate::clock::iso_to_ms;

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

/// A slot name for [`describe_outfit_with_omit`]'s `omit` set (v4
/// `OutfitSlotName`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutfitSlotName {
    Top,
    Bottom,
    Footwear,
    Accessories,
}

/// v4 `describeOutfit(slots)` (no `omit`) — the markdown outfit description. Thin
/// wrapper over [`describe_outfit_with_omit`] with an empty omit set (all four
/// slots visible), byte-identical to v4's default-options call.
pub fn describe_outfit(slots: &OutfitSlotValues) -> String {
    describe_outfit_with_omit(slots, &[])
}

/// v4 `describeOutfit(slots, { omit })` — the markdown outfit description with a
/// set of slots left out of the render entirely. Omitted slots produce no line
/// and don't participate in the "all empty → naked" / "top+bottom empty → naked"
/// fallbacks (the avatar/portrait crop routes through this: bottom/footwear are
/// omitted so a cropped torso never grows shoes/pants, and the bare-top path
/// omits top/bottom/footwear so an empty-but-visible top can't emit "topless").
pub fn describe_outfit_with_omit(slots: &OutfitSlotValues, omit: &[OutfitSlotName]) -> String {
    let omitted = |name: OutfitSlotName| omit.contains(&name);
    // `null` (v4) = omitted; `Some(&slice)` = visible.
    let top = (!omitted(OutfitSlotName::Top)).then_some(slots.top.as_slice());
    let bottom = (!omitted(OutfitSlotName::Bottom)).then_some(slots.bottom.as_slice());
    let footwear = (!omitted(OutfitSlotName::Footwear)).then_some(slots.footwear.as_slice());
    let accessories =
        (!omitted(OutfitSlotName::Accessories)).then_some(slots.accessories.as_slice());

    // All VISIBLE slots empty → "completely naked and unadorned".
    let all_visible_empty = [top, bottom, footwear, accessories]
        .iter()
        .all(|v| v.map(|s| s.is_empty()).unwrap_or(true));
    if all_visible_empty {
        return "- completely naked and unadorned\n".to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut groups = OrderedGroups::new();

    // The "naked" collapse only applies when both top and bottom are VISIBLE.
    if top.map(<[String]>::is_empty).unwrap_or(false)
        && bottom.map(<[String]>::is_empty).unwrap_or(false)
    {
        lines.push("- naked".to_string());
    } else {
        if let Some(top) = top {
            groups.add("top", join_or_fallback(top, "topless"));
        }
        if let Some(bottom) = bottom {
            groups.add("bottom", join_or_fallback(bottom, "bottomless"));
        }
    }
    if let Some(footwear) = footwear {
        groups.add("footwear", join_or_fallback(footwear, "barefoot"));
    }
    if let Some(accessories) = accessories {
        groups.add(
            "accessories",
            join_or_fallback(accessories, "no accessories"),
        );
    }

    for (value, slots_for_value) in &groups.entries {
        lines.push(format!("- **{}:** {}", slots_for_value.join(", "), value));
    }

    format!("{}\n", lines.join("\n"))
}

// ===========================================================================
// expand-composites.ts — expandComposites
// ===========================================================================

/// v4 `sortForDefaultOutfit(items)` — the deterministic layer order for a
/// default outfit: oldest `createdAt` first, items lacking one last.
///
/// Ordering is observable now that personal and shared defaults can occupy the
/// same slot — slot arrays are read inner-to-outer. Both sides of the wire apply
/// it (`apps/web`'s `sortForDefaultOutfit` is the mirror) so the composer's
/// preview and the chat that opens agree.
///
/// Nuance carried from the pre-drift port: v4 maps a *missing* `createdAt` to
/// `+Infinity` and an unparseable one to `NaN` (whose comparator reads as 0 and
/// so leaves the pair's relative order alone). v5 maps both to `i64::MAX`, which
/// agrees with v4 on the only shape Quilltap data actually holds — a valid ISO
/// string or nothing — and keeps the comparator a total order, which Rust's
/// stable `sort_by` requires.
pub fn sort_for_default_outfit(items: &[Value]) -> Vec<Value> {
    let mut out = items.to_vec();
    let key = |v: &Value| -> i64 {
        v.get("createdAt")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .and_then(iso_to_ms)
            .unwrap_or(i64::MAX)
    };
    out.sort_by_key(key);
    out
}

/// v4 `ExpandResult` (the `cycles` / `truncated` fields are not consumed by the
/// wardrobe tool handlers, but are ported for fidelity).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExpandResult {
    pub leaf_ids: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub truncated: bool,
}

/// v4 `COMPOSITE_MAX_DEPTH` — the default recursion bound for composite
/// expansion. Exported so callers that hydrate the component graph ahead of
/// expansion (see [`crate::tools::wardrobe_shared`]) fetch exactly as many levels
/// as expansion will actually walk.
pub const COMPOSITE_MAX_DEPTH: usize = 4;

const DEFAULT_MAX_DEPTH: usize = COMPOSITE_MAX_DEPTH;

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
///
/// A **bundle** (`61574563`) goes on as its parts, not as itself: given an
/// `items_by_id` lookup that resolves its components, the leaves are laid into
/// the slots their own `types` declare and the bundle's id is never stored.
/// `replace` still governs — an assembled outfit set to replace clears the slots
/// it lands in first. Without a lookup (or when a bundle's parts can't be
/// resolved) the bundle is stored whole, the pre-4.8.2 behaviour, and read-time
/// [`expand_composites`] still covers it.
pub fn wear_item_into_slots(
    current: &Slots,
    item: &crate::dissolve_bundles::WearableNode,
    replace: bool,
    items_by_id: Option<&crate::dissolve_bundles::WearableLookup>,
) -> Slots {
    if let Some(leaves) = crate::dissolve_bundles::dissolve_bundle_to_leaves(item, items_by_id) {
        return crate::dissolve_bundles::lay_leaves_into_slots(current, item, &leaves, replace);
    }

    let mut slots = current.clone();
    for slot in &item.types {
        if replace {
            *slots.slot_mut(slot) = vec![item.id.clone()];
        } else if !slots.slot(slot).contains(&item.id) {
            slots.slot_mut(slot).push(item.id.clone());
        }
    }
    slots
}

/// v4 `replaceItemIntoSlots` — clear each slot in `types` and set it to `[id]`,
/// ignoring the `replace` flag. A resolvable bundle dissolves into its leaves
/// with the covered slots cleared first (see [`wear_item_into_slots`]).
pub fn replace_item_into_slots(
    current: &Slots,
    item: &crate::dissolve_bundles::WearableNode,
    items_by_id: Option<&crate::dissolve_bundles::WearableLookup>,
) -> Slots {
    if let Some(leaves) = crate::dissolve_bundles::dissolve_bundle_to_leaves(item, items_by_id) {
        return crate::dissolve_bundles::lay_leaves_into_slots(current, item, &leaves, true);
    }

    let mut slots = current.clone();
    for slot in &item.types {
        *slots.slot_mut(slot) = vec![item.id.clone()];
    }
    slots
}

/// v4 `addItemToSlot` (`61574563`) — pure single-slot layering. A bundle
/// contributes the parts that cover this slot rather than its own id; if none of
/// them do (the caller asked for a slot the bundle claims but no part fills), the
/// bundle's id goes in as before so the gesture is never silently a no-op.
pub fn add_item_to_slot(
    current: &Slots,
    slot: &str,
    item: &crate::dissolve_bundles::WearableNode,
    items_by_id: Option<&crate::dissolve_bundles::WearableLookup>,
) -> Slots {
    let mut slots = current.clone();
    let leaves = crate::dissolve_bundles::dissolve_bundle_to_leaves(item, items_by_id);
    let for_slot: Vec<&crate::dissolve_bundles::DissolvedLeaf> = leaves
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|leaf| leaf.slots.iter().any(|s| s == slot))
        .collect();

    if !for_slot.is_empty() {
        for leaf in for_slot {
            if !slots.slot(slot).contains(&leaf.id) {
                slots.slot_mut(slot).push(leaf.id.clone());
            }
        }
        return slots;
    }

    if !slots.slot(slot).contains(&item.id) {
        slots.slot_mut(slot).push(item.id.clone());
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

/// v4 `hashEquippedSlots` (`lib/wardrobe/outfit-hash.ts`): the deterministic short
/// hash of a character's equipped wardrobe slots. Slot-key order is normalized
/// (`{top, bottom, footwear, accessories}`); each slot's array is hashed in its
/// stored order (layering matters). A null/empty outfit hashes to a stable
/// sentinel. SHA-256 over `JSON.stringify(normalized)`, first 16 hex chars.
pub fn hash_equipped_slots(slots: Option<&Slots>) -> String {
    // `JSON.stringify({top, bottom, footwear, accessories})` in key order — a
    // typed serde struct declares that order here, where the hash depends on it.
    #[derive(serde::Serialize)]
    struct Normalized<'a> {
        top: &'a [String],
        bottom: &'a [String],
        footwear: &'a [String],
        accessories: &'a [String],
    }
    let empty: Vec<String> = Vec::new();
    let n = Normalized {
        top: slots.map(|s| s.top.as_slice()).unwrap_or(&empty),
        bottom: slots.map(|s| s.bottom.as_slice()).unwrap_or(&empty),
        footwear: slots.map(|s| s.footwear.as_slice()).unwrap_or(&empty),
        accessories: slots.map(|s| s.accessories.as_slice()).unwrap_or(&empty),
    };
    let json = serde_json::to_string(&n).unwrap_or_else(|_| "{}".to_string());
    let hex = crate::db::doc_mount_file_links::sha256_of_string(&json);
    hex.chars().take(16).collect()
}

/// v4 `hasEquippedItems`: true when at least one slot holds an equipped item.
pub fn has_equipped_items(slots: Option<&Slots>) -> bool {
    match slots {
        None => false,
        Some(s) => s.top.len() + s.bottom.len() + s.footwear.len() + s.accessories.len() > 0,
    }
}

/// v4 `decorateOutfitItems(items, { titleOnly: true })`: for each item, prefer its
/// trimmed `imagePrompt` as the visual cue, else its `title`. (The non-titleOnly
/// path — `title (description)` — is not the live-clothing shape and is not ported
/// here.) Each item is a resolved wardrobe `Value` with `title`/`imagePrompt`.
pub fn decorate_outfit_items_title_only(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|i| {
            let cue = i.get("imagePrompt").and_then(Value::as_str).map(str::trim);
            match cue {
                Some(c) if !c.is_empty() => c.to_string(),
                _ => i
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .collect()
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
    fn describe_outfit_with_omit_avatar_cases() {
        // Clothed avatar: omit bottom+footwear → only top + accessories render;
        // a populated bottom/footwear produce NO line (distinct from empty slots,
        // which would emit "bottomless"/"barefoot").
        let s = OutfitSlotValues {
            top: vec!["Shirt".into()],
            bottom: vec!["Pants".into()],
            footwear: vec!["Shoes".into()],
            accessories: vec!["Hat".into()],
        };
        assert_eq!(
            describe_outfit_with_omit(&s, &[OutfitSlotName::Bottom, OutfitSlotName::Footwear]),
            "- **top:** Shirt\n- **accessories:** Hat\n"
        );
        // Bare-top avatar with accessories: omit top+bottom+footwear → only the
        // accessories line, never a "topless"/"naked" fallback.
        let bare = OutfitSlotValues {
            top: vec![],
            bottom: vec![],
            footwear: vec![],
            accessories: vec!["Necklace".into()],
        };
        assert_eq!(
            describe_outfit_with_omit(
                &bare,
                &[
                    OutfitSlotName::Top,
                    OutfitSlotName::Bottom,
                    OutfitSlotName::Footwear
                ]
            ),
            "- **accessories:** Necklace\n"
        );
        // Bare-top with NO accessories under the same omit set → all-visible-empty
        // → "completely naked and unadorned" (this is why the avatar builder emits
        // '' in that branch instead of calling describeOutfit).
        assert_eq!(
            describe_outfit_with_omit(
                &OutfitSlotValues::default(),
                &[
                    OutfitSlotName::Top,
                    OutfitSlotName::Bottom,
                    OutfitSlotName::Footwear
                ]
            ),
            "- completely naked and unadorned\n"
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
        let node = |id: &str| crate::dissolve_bundles::WearableNode::new(id, &types(&["top"]), &[]);
        // Layer: append.
        let layered = wear_item_into_slots(&base, &node("vest"), false, None);
        assert_eq!(layered.top, types(&["shirt", "vest"]));
        // Replace: overwrite.
        let replaced = wear_item_into_slots(&base, &node("coat"), true, None);
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
