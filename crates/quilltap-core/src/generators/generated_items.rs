//! v4 `lib/wardrobe/generated-items.ts` — LLM-generated wardrobe items: the
//! shared shape, generation prompt, and sanitizer used by every system that
//! generates wardrobe content with AI (the AI Wizard and Summon From Lore; the
//! character optimizer reuses the same item shape for its wardrobe
//! suggestions). `p4.9k` — the one shared leaf P4.9K0's substrate did not port
//! (its record names it "out of this lane's owned tree"); landed here under
//! the P4.9K1 fence because the optimizer runner is its first v5 consumer, and
//! consumed as-is by P4.9K2's wizard + import.
//!
//! Generated composites reference their components by *title* (the LLM has no
//! ids); each persistence path resolves titles to real item ids after
//! creation.
//!
//! ## Input shape
//!
//! [`sanitize_generated_wardrobe_items`] takes the PARSED model answer as a
//! `serde_json::Value` and returns typed [`GeneratedWardrobeItem`]s: v4's
//! sanitizer is a `typeof`-driven coercion over an untyped array, and the
//! corpus rows that matter are exactly the ones a typed input would refuse
//! before the coercion ran (a numeric `description` → `''`, a string
//! `isDefault` → `false`, a `components` that is not an array → absent).
//! Serialization reproduces v4's object-literal key order with `undefined`
//! keys OMITTED, which is what `JSON.stringify` emits and what the
//! `generated_wardrobe_items_equivalence` corpus compares as bytes.

use serde::Serialize;
use serde_json::Value;

use crate::generators::field_semantics::WARDROBE_SEMANTICS;
use crate::jsstr::js_trim;
use crate::wardrobe::WARDROBE_SLOT_TYPES;

/// v4 `GeneratedWardrobeItem`, in v4's key order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedWardrobeItem {
    pub title: String,
    pub description: String,
    /// Terse literal visual cue for image generation; never Markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_prompt: Option<String>,
    pub types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appropriateness: Option<String>,
    /// Part of the character's default outfit.
    pub is_default: bool,
    /// Composite outfit: titles of other items from the same generation batch
    /// that this entry bundles. The persistence layer resolves titles to the
    /// created items' ids (`componentItemIds`). Absent = leaf garment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
    /// Composite-only: clear the designated slots on equip instead of layering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<bool>,
}

/// v4 `WARDROBE_ITEMS_GENERATION_PROMPT` (byte-exact) — `${WARDROBE_SEMANTICS}`
/// followed by the generation instructions.
pub fn wardrobe_items_generation_prompt() -> String {
    format!(
        r#"{WARDROBE_SEMANTICS}

Generate this character's wardrobe based on the context provided and their typical clothing and style.
Each item must cover one or more of these slot types: "top" (shirts, jackets, dresses that cover the torso), "bottom" (pants, skirts, shorts), "footwear" (shoes, boots, sandals), "accessories" (jewelry, hats, belts, scarves, bags), "hair" (a hairstyle or hairdo — braided, permed, an updo, a wig; the styling, not the hair itself).

A single item can cover multiple slots — for example, a full-length dress would have types ["top", "bottom"].

Respond with ONLY valid JSON, no markdown fences:
[
  {{
    "title": "Short descriptive name for the item",
    "description": "A sentence or two describing the item's appearance in detail",
    "imagePrompt": "Terse literal visual cue for image generation, e.g. 'worn brown leather duster with brass buttons'",
    "types": ["top"],
    "appropriateness": "casual, everyday",
    "isDefault": false
  }},
  {{
    "title": "Name for a complete outfit",
    "description": "One sentence describing the ensemble as a whole",
    "types": ["top", "bottom", "footwear"],
    "appropriateness": "casual, everyday",
    "isDefault": true,
    "components": ["exact title of item 1", "exact title of item 2", "exact title of item 3"],
    "replace": true
  }}
]

Rules:
- Generate 4-8 individual garments/accessories representing this character's typical wardrobe — a mix of everyday and situational items, plus optionally ONE signature hairstyle item (types: ["hair"]) if the character's look calls for a deliberate hairdo.
- Then add 1-2 composite OUTFITS: entries whose "components" array lists the exact titles of items from THIS response that are worn together. An outfit's "types" is the union of its components' slot types. Set "replace": true so equipping the outfit swaps out whatever was worn before; individual garments never need "components" or "replace".
- Mark the character's everyday outfit as the default: set "isDefault": true on the everyday composite outfit if you made one (preferred), otherwise on each everyday individual item. Everything else gets "isDefault": false.
- "imagePrompt" is fed directly to an image-generation model: short, literal, comma-friendly, never Markdown. Omit it only when the title already says everything visual.
- "appropriateness" is a comma-separated list of context tags describing when the item is appropriate (e.g., "casual", "formal", "combat", "sleepwear", "intimate").
- The wardrobe holds only removable things. Permanent bodily features (scars, tattoos, fur, anatomy) belong to the physical description and must never appear as wardrobe items — with one deliberate exception: a hairSTYLE goes in the "hair" slot, while the hair's natural colour, length, and texture stay in the physical description."#
    )
}

/// v4 `sanitizeGeneratedWardrobeItems` — "Validate and normalize
/// LLM-generated wardrobe items: drop invalid slot types, coerce optional
/// fields, and keep composite `components` references only when they resolve
/// to another item's title in the same batch."
pub fn sanitize_generated_wardrobe_items(items: &Value) -> Vec<GeneratedWardrobeItem> {
    let Some(items) = items.as_array() else {
        return Vec::new();
    };
    // Pass 1: the typed projection.
    //   filter: item && typeof item.title === 'string' && item.title.trim() && Array.isArray(item.types)
    let mut typed: Vec<GeneratedWardrobeItem> = items
        .iter()
        .filter(|item| {
            // `item &&` — a falsy entry (null, 0, '') is dropped; a truthy
            // non-object has no `.title`, so the typeof test drops it too.
            let Some(obj) = item.as_object() else {
                return false;
            };
            let title_ok =
                matches!(obj.get("title"), Some(Value::String(t)) if !js_trim(t).is_empty());
            title_ok && matches!(obj.get("types"), Some(Value::Array(_)))
        })
        .map(|item| {
            let obj = item.as_object().unwrap();
            let title = js_trim(obj["title"].as_str().unwrap()).to_string();
            let description = match obj.get("description") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let image_prompt = match obj.get("imagePrompt") {
                Some(Value::String(s)) if !js_trim(s).is_empty() => Some(js_trim(s).to_string()),
                _ => None,
            };
            let types: Vec<String> = obj["types"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|t| t.as_str())
                .filter(|t| WARDROBE_SLOT_TYPES.contains(t))
                .map(str::to_string)
                .collect();
            let appropriateness = match obj.get("appropriateness") {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            };
            let is_default = obj.get("isDefault") == Some(&Value::Bool(true));
            let components = match obj.get("components") {
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .filter_map(|c| c.as_str())
                        .filter(|c| !js_trim(c).is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            };
            let replace = if obj.get("replace") == Some(&Value::Bool(true)) {
                Some(true)
            } else {
                None
            };
            GeneratedWardrobeItem {
                title,
                description,
                image_prompt,
                types,
                appropriateness,
                is_default,
                components,
                replace,
            }
        })
        .filter(|item| !item.types.is_empty())
        .collect();

    // Pass 2: composite components must reference other titles from this same
    // batch (case-insensitively, trimmed) and never the item itself.
    let titles: Vec<String> = typed.iter().map(|i| i.title.to_lowercase()).collect();
    for item in typed.iter_mut() {
        let own = item.title.to_lowercase();
        let resolvable: Vec<String> = match &item.components {
            Some(c) if !c.is_empty() => c
                .iter()
                .filter(|c| {
                    let key = js_trim(c).to_lowercase();
                    titles.contains(&key) && key != own
                })
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        if resolvable.is_empty() {
            item.components = None;
            item.replace = None;
        } else {
            item.components = Some(resolvable);
        }
    }
    typed
}

/// v4 `orderGeneratedItemsLeafFirst` — "Order generated items so that leaf
/// garments come before the composites that reference them (and shallower
/// composites before deeper ones), so persistence paths that create items one
/// at a time never write a composite ahead of its components." Generic over
/// the item shape (a title + optional component titles, read through the two
/// projections), a STABLE sort by depth, as JS's `sort` is.
pub fn order_generated_items_leaf_first<T: Clone>(
    items: &[T],
    title: impl Fn(&T) -> String,
    components: impl Fn(&T) -> Option<Vec<String>>,
) -> Vec<T> {
    let titles: Vec<String> = items.iter().map(|i| title(i).to_lowercase()).collect();
    let comps: Vec<Option<Vec<String>>> = items.iter().map(&components).collect();

    fn depth(
        idx: usize,
        titles: &[String],
        comps: &[Option<Vec<String>>],
        seen: &mut Vec<String>,
    ) -> usize {
        let Some(c) = comps[idx].as_ref().filter(|c| !c.is_empty()) else {
            return 0;
        };
        let own = titles[idx].clone();
        if seen.contains(&own) {
            return 0; // defensive: sanitizer already rejects self-reference
        }
        seen.push(own);
        let mut max = 0usize;
        for t in c {
            let key = js_trim(t).to_lowercase();
            // `items.find(...)` — the FIRST item whose title matches.
            if let Some(j) = titles.iter().position(|title| *title == key) {
                max = max.max(depth(j, titles, comps, seen) + 1);
            }
        }
        max
    }

    let mut indexed: Vec<(usize, T)> = items.iter().cloned().enumerate().collect();
    indexed.sort_by_key(|(i, _)| depth(*i, &titles, &comps, &mut Vec::new()));
    indexed.into_iter().map(|(_, item)| item).collect()
}

/// [`order_generated_items_leaf_first`] over the typed items.
pub fn order_typed_leaf_first(items: &[GeneratedWardrobeItem]) -> Vec<GeneratedWardrobeItem> {
    order_generated_items_leaf_first(items, |i| i.title.clone(), |i| i.components.clone())
}

/// [`order_generated_items_leaf_first`] over RAW JSON objects (the corpus's
/// client-shaped rows: `{title, components?}` with any other keys along) —
/// v4's generic parameter admits both shapes. A non-array / absent
/// `components` is "no components"; a non-string entry renders as JS would.
pub fn order_json_leaf_first(items: &[Value]) -> Vec<Value> {
    order_generated_items_leaf_first(
        items,
        |v| {
            v.get("title")
                .map(|t| match t {
                    Value::String(s) => s.clone(),
                    other => crate::pascal::js_value::to_js_string(other),
                })
                .unwrap_or_else(|| "undefined".to_string())
        },
        |v| {
            v.get("components").and_then(Value::as_array).map(|a| {
                a.iter()
                    .map(|c| match c {
                        Value::String(s) => s.clone(),
                        other => crate::pascal::js_value::to_js_string(other),
                    })
                    .collect()
            })
        },
    )
}
