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
/// this order so output is deterministic. Append new slots at the END — inserting
/// mid-list rewrites every serialized `types` array and reorders existing UI.
pub const WARDROBE_SLOT_TYPES: [&str; 5] = ["top", "bottom", "footwear", "accessories", "hair"];

/// Per-slot presentation and semantics metadata (v4 `WardrobeSlotMeta`). One
/// place to describe a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WardrobeSlotMeta {
    /// Singular display label ("Hair").
    pub label: &'static str,
    /// Group-header label in the item editor ("Tops", "Hair").
    pub group_label: &'static str,
    /// qt-* badge class for this slot's chip.
    pub badge_class: &'static str,
    /// True for garment slots that participate in nudity semantics; false for
    /// styling slots (hair). Drives naked collapses and deliberate-unclothed
    /// detection.
    pub is_clothing: bool,
    /// Whether an EMPTY slot is reported at all — to an LLM, to an image model,
    /// or to the user.
    ///
    /// True for garment slots, where emptiness is real information: an empty top
    /// means "topless", an empty footwear slot means "barefoot".
    ///
    /// False for styling slots ("unreported-if-blank"). An empty `hair` slot does
    /// NOT mean the character has no hair — it means their hair is in its natural,
    /// unstyled state, which the physical description already covers. Saying
    /// anything at all about it invites a model to render a bald character or to
    /// narrate the absence. Every surface that lists slots must SKIP an empty
    /// unreported-if-blank slot entirely rather than emitting a phrase, a label,
    /// or an "(empty)" marker for it.
    ///
    /// Use [`is_slot_reported_when_empty`] at reporting sites rather than testing
    /// slot names.
    pub report_when_empty: bool,
    /// Phrase rendered when a *reported* slot is empty. `Some` exactly when
    /// `report_when_empty` is true; `None` for unreported-if-blank slots, which
    /// render nothing.
    pub empty_fallback: Option<&'static str>,
}

/// The registry rows (v4 `WARDROBE_SLOT_META`), index-aligned with
/// [`WARDROBE_SLOT_TYPES`].
pub const WARDROBE_SLOT_META: [WardrobeSlotMeta; 5] = [
    WardrobeSlotMeta {
        label: "Top",
        group_label: "Tops",
        badge_class: "qt-badge-wardrobe-top",
        is_clothing: true,
        report_when_empty: true,
        empty_fallback: Some("topless"),
    },
    WardrobeSlotMeta {
        label: "Bottom",
        group_label: "Bottoms",
        badge_class: "qt-badge-wardrobe-bottom",
        is_clothing: true,
        report_when_empty: true,
        empty_fallback: Some("bottomless"),
    },
    WardrobeSlotMeta {
        label: "Footwear",
        group_label: "Footwear",
        badge_class: "qt-badge-wardrobe-footwear",
        is_clothing: true,
        report_when_empty: true,
        empty_fallback: Some("barefoot"),
    },
    WardrobeSlotMeta {
        label: "Accessories",
        group_label: "Accessories",
        badge_class: "qt-badge-wardrobe-accessories",
        is_clothing: true,
        report_when_empty: true,
        empty_fallback: Some("no accessories"),
    },
    WardrobeSlotMeta {
        label: "Hair",
        group_label: "Hair",
        badge_class: "qt-badge-wardrobe-hair",
        is_clothing: false,
        report_when_empty: false,
        empty_fallback: None,
    },
];

/// The registry row for a slot name (v4 `WARDROBE_SLOT_META[slot]`); `None` for
/// an unknown slot.
pub fn slot_meta(slot: &str) -> Option<&'static WardrobeSlotMeta> {
    WARDROBE_SLOT_TYPES
        .iter()
        .position(|s| *s == slot)
        .map(|i| &WARDROBE_SLOT_META[i])
}

/// Slots that count as clothing for nudity/undress semantics (v4
/// `CLOTHING_SLOT_TYPES`, derived from the registry's `isClothing` — the
/// derivation is pinned by a unit test since Rust consts can't filter).
pub const CLOTHING_SLOT_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

/// Slots that vanish from every report when empty (v4
/// `UNREPORTED_IF_BLANK_SLOT_TYPES`; today: `hair`). Derivation pinned by a
/// unit test, like [`CLOTHING_SLOT_TYPES`].
pub const UNREPORTED_IF_BLANK_SLOT_TYPES: [&str; 1] = ["hair"];

/// v4 `isSlotReportedWhenEmpty(slot)` — true when an empty `slot` should still
/// be reported (as a phrase, a label, or an "(empty)" marker). False for
/// unreported-if-blank slots — skip them. Call this at every site that
/// enumerates slots for an LLM, an image model, or the reader. An unknown slot
/// reads as reported (the conservative default; v4 would throw).
pub fn is_slot_reported_when_empty(slot: &str) -> bool {
    slot_meta(slot).is_none_or(|m| m.report_when_empty)
}

// ===========================================================================
// slot-guidance.ts — the shared LLM-facing wording for the hair slot
// ===========================================================================
//
// The `hair` slot is the one that needs explaining every time it is offered to
// a model: it holds a *hairdo*, not hair. Keeping the sentence in one place
// means the wardrobe tools, the outfit-choosing prompt, the character
// generators, and the image analyser all draw the wardrobe/physical line the
// same way — a model that reads two different versions of this rule files
// hair colour as a garment.

/// v4 `HAIR_SLOT_GUIDANCE` — the wardrobe/physical boundary for hair, phrased
/// for a tool parameter description. Append to any `types`/`slot` parameter
/// gloss.
pub const HAIR_SLOT_GUIDANCE: &str = "The \"hair\" slot holds a hairstyle or hairdo (braided, permed, an updo, a wig) — the styling, not the hair itself; natural hair colour and length belong in the character's physical description, and an empty hair slot simply means unstyled hair.";

/// v4 `HAIR_PHYSICAL_BOUNDARY` — the same boundary phrased for a prose prompt
/// paragraph (the character generators and optimizer), where it reads as an
/// exception to the "permanent body features are physical description" rule.
/// (The generator surfaces are unported; the sentence lives here so the future
/// generators lane composes the same bytes.)
pub const HAIR_PHYSICAL_BOUNDARY: &str = "Anything permanently part of the body (scars, tattoos, fur) is PHYSICAL DESCRIPTION, not wardrobe — with one deliberate exception: a hairSTYLE goes in the wardrobe's \"hair\" slot, while the hair's natural colour, length, and texture stay in the physical description.";

/// v4 `HAIR_PHYSICAL_DESCRIPTION_NOTE` — the complement of
/// [`HAIR_PHYSICAL_BOUNDARY`], for physical-description prompts that would
/// otherwise claim all hair. (Same unported-surface note as above.)
pub const HAIR_PHYSICAL_DESCRIPTION_NOTE: &str = "Natural hair — colour, length, texture — belongs here; a deliberate hairSTYLE (braids, an updo, a wig) belongs in the WARDROBE's \"hair\" slot instead. Describe the hair itself, not its styling.";

// ===========================================================================
// composite-types.ts — unionTypes
// ===========================================================================

/// v4 `unionTypes(components)` — the union of the components' slot types, in
/// canonical slot order (`top → bottom → footwear → accessories → hair`).
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
    pub hair: Vec<String>,
}

impl OutfitSlotValues {
    /// Read a slot's titles by canonical name (an unknown name reads as `top`,
    /// unreachable — callers only walk [`WARDROBE_SLOT_TYPES`]).
    pub fn slot(&self, name: &str) -> &[String] {
        match name {
            "top" => &self.top,
            "bottom" => &self.bottom,
            "footwear" => &self.footwear,
            "accessories" => &self.accessories,
            "hair" => &self.hair,
            _ => &self.top,
        }
    }
}

/// v4 `buildOutfitSlotValues(fn)` — build a full [`OutfitSlotValues`] by asking
/// `f` for each slot's strings, in canonical slot order.
pub fn build_outfit_slot_values<F: FnMut(&str) -> Vec<String>>(mut f: F) -> OutfitSlotValues {
    OutfitSlotValues {
        top: f("top"),
        bottom: f("bottom"),
        footwear: f("footwear"),
        accessories: f("accessories"),
        hair: f("hair"),
    }
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

/// A slot name for [`describe_outfit_with_omit`]'s `omit` set (v4
/// `OutfitSlotName`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutfitSlotName {
    Top,
    Bottom,
    Footwear,
    Accessories,
    Hair,
}

impl OutfitSlotName {
    /// The canonical slot string this name addresses.
    pub fn as_str(self) -> &'static str {
        match self {
            OutfitSlotName::Top => "top",
            OutfitSlotName::Bottom => "bottom",
            OutfitSlotName::Footwear => "footwear",
            OutfitSlotName::Accessories => "accessories",
            OutfitSlotName::Hair => "hair",
        }
    }
}

/// v4 `describeOutfit(slots)` (no `omit`) — the markdown outfit description. Thin
/// wrapper over [`describe_outfit_with_omit`] with an empty omit set (all slots
/// visible), byte-identical to v4's default-options call.
pub fn describe_outfit(slots: &OutfitSlotValues) -> String {
    describe_outfit_with_omit(slots, &[])
}

/// v4 `describeOutfit(slots, { omit })` — the markdown outfit description with a
/// set of slots left out of the render entirely. Omitted slots produce no line
/// and don't participate in the "all empty → naked" / "top+bottom empty → naked"
/// fallbacks (the avatar/portrait crop routes through this: bottom/footwear are
/// omitted so a cropped torso never grows shoes/pants, and the bare-top path
/// omits top/bottom/footwear so an empty-but-visible top can't emit "topless").
///
/// An empty slot whose `report_when_empty` is false (hair) contributes nothing
/// at all: a hairdo is styling, not a garment — an empty hair slot means the
/// character's hair is in its natural state, which the physical description
/// already covers. Such a slot never emits a negative-space line (a character
/// with no hair item must never read as bald) and never blocks the "naked"
/// collapse — but a STYLED hairdo does block the "completely naked and
/// unadorned" collapse, so an unclothed character with braids reads as
/// `- naked` + the garment fallbacks + the hair line (v4's rule 13).
pub fn describe_outfit_with_omit(slots: &OutfitSlotValues, omit: &[OutfitSlotName]) -> String {
    // Slot → `Some(titles)` when visible, `None` when omitted (v4's `visible`
    // map), index-aligned with WARDROBE_SLOT_TYPES.
    let visible: Vec<Option<&[String]>> = WARDROBE_SLOT_TYPES
        .iter()
        .map(|name| {
            let omitted = omit.iter().any(|o| o.as_str() == *name);
            (!omitted).then(|| slots.slot(name))
        })
        .collect();

    // All VISIBLE slots empty (clothing AND hair) → "completely naked and
    // unadorned".
    let all_visible_empty = visible
        .iter()
        .all(|v| v.map(|s| s.is_empty()).unwrap_or(true));
    if all_visible_empty {
        return "- completely naked and unadorned\n".to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut groups = OrderedGroups::new();

    // The "naked" collapse only applies when both top and bottom are VISIBLE.
    let naked_collapse = matches!(
        (visible[0], visible[1]),
        (Some(top), Some(bottom)) if top.is_empty() && bottom.is_empty()
    );
    if naked_collapse {
        lines.push("- naked".to_string());
    }

    for (i, slot) in WARDROBE_SLOT_TYPES.iter().enumerate() {
        if naked_collapse && (*slot == "top" || *slot == "bottom") {
            continue;
        }
        let Some(items) = visible[i] else { continue };
        if !items.is_empty() {
            groups.add(slot, items.join(", "));
            continue;
        }
        // Empty slot: render its negative-space phrase, unless the slot is
        // unreported-if-blank (hair) — those vanish entirely rather than telling
        // a reader or an image model that something is missing.
        if !is_slot_reported_when_empty(slot) {
            continue;
        }
        if let Some(fallback) = slot_meta(slot).and_then(|m| m.empty_fallback) {
            groups.add(slot, fallback.to_string());
        }
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

/// The equipped slots (v4 `EquippedSlots`), each an array of item ids. Slot
/// order is fixed (`top → bottom → footwear → accessories → hair`) so it
/// serializes byte-identically to v4's `JSON.stringify`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slots {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub footwear: Vec<String>,
    pub accessories: Vec<String>,
    pub hair: Vec<String>,
}

impl Slots {
    /// A fresh empty outfit (v4 `freshSlots`).
    pub fn fresh() -> Self {
        Slots::default()
    }

    /// Read a slot's array by name (an unknown name is unreachable — the callers
    /// only pass the valid slots). Public so slot walks iterate
    /// [`WARDROBE_SLOT_TYPES`] instead of restating the field list.
    pub fn slot(&self, name: &str) -> &Vec<String> {
        match name {
            "top" => &self.top,
            "bottom" => &self.bottom,
            "footwear" => &self.footwear,
            "accessories" => &self.accessories,
            "hair" => &self.hair,
            _ => &self.top,
        }
    }

    /// Mutable sibling of [`Slots::slot`], public for the same reason.
    pub fn slot_mut(&mut self, name: &str) -> &mut Vec<String> {
        match name {
            "top" => &mut self.top,
            "bottom" => &mut self.bottom,
            "footwear" => &mut self.footwear,
            "accessories" => &mut self.accessories,
            "hair" => &mut self.hair,
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
            // Absent on any payload written before the hair slot existed —
            // reads as [] with nothing else disturbed (v4's no-migration
            // guarantee: every slot on EquippedSlotsSchema defaults to []).
            hair: read("hair"),
        }
    }

    /// Serialize to the ordered `{top,bottom,footwear,accessories,hair}` JSON
    /// object v4 persists (slot order fixed; `serde_json` preserve-order keeps
    /// it).
    pub fn to_value(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("top".into(), to_str_array(&self.top));
        m.insert("bottom".into(), to_str_array(&self.bottom));
        m.insert("footwear".into(), to_str_array(&self.footwear));
        m.insert("accessories".into(), to_str_array(&self.accessories));
        m.insert("hair".into(), to_str_array(&self.hair));
        Value::Object(m)
    }
}

fn to_str_array(v: &[String]) -> Value {
    Value::Array(v.iter().map(|s| Value::String(s.clone())).collect())
}

/// v4 `normalizeEquippedSlots` (`lib/schemas/wardrobe.types.ts`, `275cd7bc`) —
/// coerce a raw stored slot bag into a full five-slot outfit.
///
/// `chats.equippedOutfit` is unconstrained JSON and slots were added over time
/// (hair arrived after real instances were already writing four-key rows), so
/// anything read back off a chat row may be missing keys entirely. v4 parses
/// through `EquippedSlotsSchema` — every slot `.default([])` — and on a failed
/// parse salvages key by key, keeping each slot's string elements and reading a
/// non-array slot as empty. [`Slots::from_value`] already IS that salvage, and
/// the two agree case for case: the schema only parses when every element is a
/// UUID, in which case the parse and the salvage produce the same arrays.
///
/// Returned as the serialized [`Slots::to_value`] form — five keys in canonical
/// order, which is exactly what v4's Zod parse and its salvage both emit.
pub fn normalize_equipped_slots(raw: Option<&Value>) -> Value {
    Slots::from_value(raw).to_value()
}

/// The `EquippedOutfitState` map with every character's entry normalized (v4's
/// `getEquippedOutfit` maps `Object.entries(stored)` through
/// [`normalize_equipped_slots`]).
///
/// `Object.entries` is mirrored for the non-object shapes too — an array or a
/// string yields index-keyed entries, any other primitive yields none. No v5
/// writer stores anything but an object there, but the column is raw JSON and a
/// foreign archive is not obliged to agree.
pub fn normalize_equipped_outfit_state(stored: &Value) -> Value {
    let mut out = serde_json::Map::new();
    match stored {
        Value::Object(m) => {
            for (character_id, slots) in m {
                out.insert(character_id.clone(), normalize_equipped_slots(Some(slots)));
            }
        }
        Value::Array(a) => {
            for (i, slots) in a.iter().enumerate() {
                out.insert(i.to_string(), normalize_equipped_slots(Some(slots)));
            }
        }
        // Every entry of a string is a one-character string, which normalizes
        // to empty slots — so only the COUNT matters, and JS counts UTF-16
        // code units.
        Value::String(s) => {
            for i in 0..s.encode_utf16().count() {
                out.insert(i.to_string(), normalize_equipped_slots(None));
            }
        }
        _ => {}
    }
    Value::Object(out)
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
/// (`{top, bottom, footwear, accessories, hair}`); each slot's array is hashed in
/// its stored order (layering matters). A null/empty outfit hashes to a stable
/// sentinel. SHA-256 over `JSON.stringify(normalized)`, first 16 hex chars.
///
/// The hair key is serialized UNCONDITIONALLY (v4's design doc forbids
/// conditional key omission): every chat with a cached pre-hair hash misses
/// exactly once and re-derives its clothing summary — the accepted upgrade
/// cost of adding a slot.
pub fn hash_equipped_slots(slots: Option<&Slots>) -> String {
    // `JSON.stringify({top, bottom, footwear, accessories, hair})` in key order
    // — a typed serde struct declares that order here, where the hash depends
    // on it.
    #[derive(serde::Serialize)]
    struct Normalized<'a> {
        top: &'a [String],
        bottom: &'a [String],
        footwear: &'a [String],
        accessories: &'a [String],
        hair: &'a [String],
    }
    let empty: Vec<String> = Vec::new();
    let n = Normalized {
        top: slots.map(|s| s.top.as_slice()).unwrap_or(&empty),
        bottom: slots.map(|s| s.bottom.as_slice()).unwrap_or(&empty),
        footwear: slots.map(|s| s.footwear.as_slice()).unwrap_or(&empty),
        accessories: slots.map(|s| s.accessories.as_slice()).unwrap_or(&empty),
        hair: slots.map(|s| s.hair.as_slice()).unwrap_or(&empty),
    };
    let json = serde_json::to_string(&n).unwrap_or_else(|_| "{}".to_string());
    let hex = crate::db::doc_mount_file_links::sha256_of_string(&json);
    hex.chars().take(16).collect()
}

/// v4 `hasEquippedItems`: true when at least one slot holds an equipped item —
/// ALL slots, hair included (a hair-only outfit counted as "nothing equipped"
/// before v4 `4423ad10`, a silent bug its `.some()` rewrite fixed).
pub fn has_equipped_items(slots: Option<&Slots>) -> bool {
    match slots {
        None => false,
        Some(s) => WARDROBE_SLOT_TYPES
            .iter()
            .any(|slot| !s.slot(slot).is_empty()),
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
        let b = types(&["bottom", "hair"]);
        let got = union_types([a.as_slice(), b.as_slice()]);
        // Canonical order regardless of input order — hair LAST.
        assert_eq!(got, types(&["top", "bottom", "accessories", "hair"]));
    }

    #[test]
    fn registry_derivations_are_pinned() {
        // CLOTHING_SLOT_TYPES / UNREPORTED_IF_BLANK_SLOT_TYPES are v4-derived
        // (filters of the registry); Rust consts can't filter, so this test IS
        // the derivation.
        let clothing: Vec<&str> = WARDROBE_SLOT_TYPES
            .iter()
            .zip(WARDROBE_SLOT_META.iter())
            .filter(|(_, m)| m.is_clothing)
            .map(|(s, _)| *s)
            .collect();
        assert_eq!(clothing, CLOTHING_SLOT_TYPES);
        let unreported: Vec<&str> = WARDROBE_SLOT_TYPES
            .iter()
            .zip(WARDROBE_SLOT_META.iter())
            .filter(|(_, m)| !m.report_when_empty)
            .map(|(s, _)| *s)
            .collect();
        assert_eq!(unreported, UNREPORTED_IF_BLANK_SLOT_TYPES);
        // v4's invariant: emptyFallback is non-null exactly for reported slots.
        for m in &WARDROBE_SLOT_META {
            assert_eq!(m.empty_fallback.is_none(), !m.report_when_empty);
        }
        // The registry rows are addressable by name and hair is the styling slot.
        assert!(!is_slot_reported_when_empty("hair"));
        for slot in CLOTHING_SLOT_TYPES {
            assert!(is_slot_reported_when_empty(slot));
        }
        assert_eq!(slot_meta("hair").unwrap().label, "Hair");
        assert!(slot_meta("bald").is_none());
    }

    #[test]
    fn describe_outfit_naked_and_fallbacks() {
        assert_eq!(
            describe_outfit(&OutfitSlotValues::default()),
            "- completely naked and unadorned\n"
        );
        // Top+bottom empty → "- naked"; footwear/accessories fall back; empty
        // hair says NOTHING.
        let s = OutfitSlotValues {
            top: vec![],
            bottom: vec![],
            footwear: vec!["Boots".into()],
            accessories: vec![],
            hair: vec![],
        };
        assert_eq!(
            describe_outfit(&s),
            "- naked\n- **footwear:** Boots\n- **accessories:** no accessories\n"
        );
    }

    #[test]
    fn describe_outfit_rule_13_styled_hair_blocks_the_full_collapse() {
        // All clothing empty + hair styled → NOT "completely naked and
        // unadorned"; fall through to "- naked" + fallbacks + the hair line.
        let s = OutfitSlotValues {
            hair: vec!["marcel waves".into()],
            ..Default::default()
        };
        assert_eq!(
            describe_outfit(&s),
            "- naked\n- **footwear:** barefoot\n- **accessories:** no accessories\n- **hair:** marcel waves\n"
        );
    }

    #[test]
    fn describe_outfit_renders_a_styled_hairdo_and_skips_a_blank_one() {
        let dressed = OutfitSlotValues {
            top: vec!["linen shirt".into()],
            bottom: vec!["wool trousers".into()],
            footwear: vec!["oxfords".into()],
            accessories: vec!["pocket watch".into()],
            hair: vec!["braided crown".into()],
        };
        assert_eq!(
            describe_outfit(&dressed),
            "- **top:** linen shirt\n- **bottom:** wool trousers\n- **footwear:** oxfords\n- **accessories:** pocket watch\n- **hair:** braided crown\n"
        );
        // Same outfit, no hairdo → the hair line vanishes entirely (never
        // "(empty)", never a fallback).
        let unstyled = OutfitSlotValues {
            hair: vec![],
            ..dressed
        };
        assert_eq!(
            describe_outfit(&unstyled),
            "- **top:** linen shirt\n- **bottom:** wool trousers\n- **footwear:** oxfords\n- **accessories:** pocket watch\n"
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
            hair: vec![],
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
            hair: vec![],
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
            hair: vec![],
        };
        assert_eq!(
            describe_outfit(&s),
            "- **top, bottom:** Dress\n- **footwear:** Heels\n- **accessories:** no accessories\n"
        );
    }

    /// The no-migration guarantee for wardrobe slots (v4
    /// `equipped-slots-forward-compat.test.ts`): `chats.equippedOutfit` is
    /// unconstrained JSON and every slot defaults to `[]`, so a row written
    /// before a slot existed parses cleanly with the new slot arriving empty.
    #[test]
    fn equipped_slots_forward_compat() {
        const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440000";
        const UUID_B: &str = "550e8400-e29b-41d4-a716-446655440001";
        // A pre-hair four-key row parses and supplies an empty hair slot.
        let legacy_row = json!({
            "top": [UUID_A],
            "bottom": [UUID_B],
            "footwear": [],
            "accessories": [],
        });
        let parsed = Slots::from_value(Some(&legacy_row));
        assert!(parsed.hair.is_empty());
        // Nothing the old row DID say is disturbed.
        assert_eq!(parsed.top, vec![UUID_A.to_string()]);
        assert_eq!(parsed.bottom, vec![UUID_B.to_string()]);

        // An empty object parses into every slot, all empty.
        assert_eq!(Slots::from_value(Some(&json!({}))), Slots::fresh());

        // Every declared slot round-trips: the serialization carries exactly
        // the canonical slot list, in order — no more, no less.
        let keys: Vec<String> = Slots::fresh()
            .to_value()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, WARDROBE_SLOT_TYPES.map(String::from).to_vec());
    }

    /// v4's `normalizeEquippedSlots` describe block (`275cd7bc`), case for
    /// case. v4 parses through Zod first and salvages key by key only when the
    /// parse fails; [`Slots::from_value`] always salvages. The two agree
    /// everywhere, because the schema's `z.array(UUIDSchema)` only parses when
    /// every element is a UUID — and in that case the salvage keeps exactly the
    /// same strings.
    #[test]
    fn normalize_equipped_slots_matches_v4() {
        const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440000";
        const UUID_B: &str = "550e8400-e29b-41d4-a716-446655440001";
        let empty = json!({
            "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": []
        });

        // "fills in a slot a legacy row never had"
        let normalized = normalize_equipped_slots(Some(&json!({
            "top": [UUID_A], "bottom": [], "footwear": [], "accessories": []
        })));
        assert_eq!(normalized["hair"], json!([]));
        assert_eq!(normalized["top"], json!([UUID_A]));

        // "returns all-empty slots for null, undefined, and an empty object"
        assert_eq!(normalize_equipped_slots(Some(&Value::Null)), empty);
        assert_eq!(normalize_equipped_slots(None), empty);
        assert_eq!(normalize_equipped_slots(Some(&json!({}))), empty);

        // "salvages the legible slots of a malformed bag rather than discarding
        // it" — `bottom` is a non-UUID string, so v4's whole-object parse fails
        // and the salvage runs; a non-array slot and a non-string element both
        // degrade to their legible remainder.
        let normalized = normalize_equipped_slots(Some(&json!({
            "top": [UUID_A],
            "bottom": ["not-a-uuid"],
            "footwear": "nonsense",
            "accessories": [7, UUID_B],
        })));
        assert_eq!(normalized["top"], json!([UUID_A]));
        assert_eq!(normalized["bottom"], json!(["not-a-uuid"]));
        assert_eq!(normalized["footwear"], json!([]));
        assert_eq!(normalized["accessories"], json!([UUID_B]));
        assert_eq!(normalized["hair"], json!([]));

        // "always answers with the full canonical slot list" — an extra key is
        // dropped on both of v4's paths (the parse strips it, the salvage never
        // looks for it).
        let normalized = normalize_equipped_slots(Some(&json!({
            "top": [UUID_A], "stowaway": ["x"]
        })));
        let keys: Vec<&String> = normalized.as_object().unwrap().keys().collect();
        assert_eq!(keys, WARDROBE_SLOT_TYPES.iter().collect::<Vec<_>>());

        // A bag that is not an object at all: v4's parse fails and the salvage
        // indexes a primitive, so every slot reads empty.
        for shape in [json!("nonsense"), json!(7), json!([UUID_A])] {
            assert_eq!(normalize_equipped_slots(Some(&shape)), empty, "{shape}");
        }
    }

    /// The state-level map v4's `getEquippedOutfit` performs
    /// (`Object.entries(stored).map(normalizeEquippedSlots)`).
    #[test]
    fn normalize_equipped_outfit_state_matches_v4() {
        const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440000";
        let empty = json!({
            "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": []
        });

        // Character keys survive in INSERTION order; only the values change.
        let state = normalize_equipped_outfit_state(&json!({
            "zeta": { "top": [UUID_A], "bottom": [], "footwear": [], "accessories": [] },
            "alpha": {},
        }));
        let keys: Vec<&String> = state.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["zeta", "alpha"]);
        assert_eq!(
            state["zeta"],
            json!({
                "top": [UUID_A], "bottom": [], "footwear": [],
                "accessories": [], "hair": []
            })
        );
        assert_eq!(state["alpha"], empty);

        // `Object.entries` of the non-object shapes: index keys for an array or
        // a string, nothing for any other primitive.
        assert_eq!(
            normalize_equipped_outfit_state(&json!([{ "top": [UUID_A] }])),
            json!({ "0": { "top": [UUID_A], "bottom": [], "footwear": [], "accessories": [], "hair": [] } })
        );
        assert_eq!(
            normalize_equipped_outfit_state(&json!("ab")),
            json!({ "0": empty, "1": empty })
        );
        assert_eq!(normalize_equipped_outfit_state(&json!(7)), json!({}));
    }

    #[test]
    fn hash_gains_the_hair_key_unconditionally() {
        // The five-key hash differs from the historical four-key hash even for
        // an all-empty outfit — the accepted one-miss-per-chat upgrade. Pin the
        // serialized preimage shape via a worn hairdo actually changing it.
        let empty_hash = hash_equipped_slots(None);
        let styled = Slots {
            hair: vec!["braid-1".into()],
            ..Default::default()
        };
        assert_ne!(hash_equipped_slots(Some(&styled)), empty_hash);
        // And a hair-only outfit now counts as equipped (v4's `.some()` fix).
        assert!(has_equipped_items(Some(&styled)));
        assert!(!has_equipped_items(Some(&Slots::fresh())));
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

/// v4 `archivedPatch` (`lib/wardrobe/archived-patch.ts`, `d25dacc1`) — the ONE
/// place `archived: boolean` (what the API accepts) becomes
/// `archivedAt: string | null` (what a wardrobe item stores).
///
/// All four item endpoints — character, General, project, group — take the
/// boolean and route it through here so the semantics cannot drift:
///
///   * archiving is **idempotent**: re-archiving an already-archived item keeps
///     its ORIGINAL `archivedAt` rather than resetting the clock;
///   * restoring clears the stamp outright.
///
/// `None` means the item is already in the requested state, so callers can skip
/// a pointless vault rewrite. v4 reads "already archived" as `Boolean(current)`
/// — an empty string counts as NOT archived, which is JS truthiness, not
/// null-ness.
pub fn archived_patch(
    current_archived_at: Option<&str>,
    archived: bool,
    now: &str,
) -> Option<Option<String>> {
    let is_archived = current_archived_at.is_some_and(|s| !s.is_empty());
    if is_archived == archived {
        return None;
    }
    Some(if archived {
        Some(now.to_string())
    } else {
        None
    })
}

#[cfg(test)]
mod archived_patch_tests {
    use super::archived_patch;

    const NOW: &str = "2026-08-26T00:00:00.000Z";
    const THEN: &str = "2026-01-01T00:00:00.000Z";

    #[test]
    fn archiving_an_active_item_stamps_now() {
        assert_eq!(archived_patch(None, true, NOW), Some(Some(NOW.to_string())));
    }

    #[test]
    fn re_archiving_is_a_no_op_so_the_original_stamp_survives() {
        assert_eq!(archived_patch(Some(THEN), true, NOW), None);
    }

    #[test]
    fn restoring_an_archived_item_clears_the_stamp() {
        assert_eq!(archived_patch(Some(THEN), false, NOW), Some(None));
    }

    #[test]
    fn re_restoring_is_a_no_op() {
        assert_eq!(archived_patch(None, false, NOW), None);
    }

    #[test]
    fn an_empty_stamp_is_not_archived_js_truthiness_not_null_ness() {
        assert_eq!(
            archived_patch(Some(""), true, NOW),
            Some(Some(NOW.to_string()))
        );
        assert_eq!(archived_patch(Some(""), false, NOW), None);
    }
}
