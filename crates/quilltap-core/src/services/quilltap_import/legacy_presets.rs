//! v4 `legacy-presets.ts` — fold pre-rework `.qtap` outfit presets into
//! composite wardrobe items at import time. Pure transform, no DB access; the
//! legacy type is otherwise gone from the data model and kept local here.
//!
//! The output is a JSON object in the composite-WardrobeItem shape the shared
//! wardrobe-import path consumes (it strips id/characterId/timestamps before
//! creating, exactly as it does for a real exported item), so the fold's only
//! observable job is the slot → types/componentItemIds computation and the
//! `replace: true` semantics.

use serde_json::{json, Value};

/// v4 `LEGACY_PRESET_SLOT_ORDER` — the deterministic slot walk. **FROZEN at
/// the four pre-hair names**: legacy presets predate the hair slot, and v4
/// `4423ad10` deliberately left this list at four. Do NOT widen.
const LEGACY_PRESET_SLOT_ORDER: &[&str] = &["top", "bottom", "footwear", "accessories"];

/// v4 `legacyPresetToComposite` (`legacy-presets.ts:43`). Preserves the preset id
/// (any pre-rework reference remains valid); each non-empty slot contributes its
/// item id to `componentItemIds` and its slot name (deduped, first-seen order) to
/// `types`; an all-null preset falls back to `['accessories']`. `replace: true`
/// because legacy presets were worn as whole outfits.
pub(super) fn legacy_preset_to_composite(preset: &Value) -> Value {
    let mut types: Vec<Value> = Vec::new();
    let mut component_item_ids: Vec<Value> = Vec::new();
    for slot in LEGACY_PRESET_SLOT_ORDER {
        // v4 `preset.slots?.[slot]` + JS truthiness: a null/absent/empty-string
        // slot contributes nothing.
        let value = preset
            .get("slots")
            .and_then(|s| s.get(*slot))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !value.is_empty() {
            component_item_ids.push(Value::String(value.to_string()));
            if !types.iter().any(|t| t == slot) {
                types.push(Value::String(slot.to_string()));
            }
        }
    }
    // A composite must always declare at least one type.
    if types.is_empty() {
        types.push(Value::String("accessories".to_string()));
    }
    json!({
        "id": preset.get("id").cloned().unwrap_or(Value::Null),
        "characterId": preset.get("characterId").cloned().unwrap_or(Value::Null),
        "title": preset.get("name").cloned().unwrap_or(Value::Null),
        "description": preset.get("description").cloned().unwrap_or(Value::Null),
        "types": types,
        "componentItemIds": component_item_ids,
        "appropriateness": Value::Null,
        "isDefault": false,
        // Legacy presets were worn as a whole outfit — preserve replace semantics.
        "replace": true,
        "migratedFromClothingRecordId": Value::Null,
        "archivedAt": Value::Null,
        "createdAt": preset.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": preset.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_slots_in_order_and_dedupes_types() {
        let preset = json!({
            "id": "p1",
            "characterId": "c1",
            "name": "Town Outfit",
            "description": null,
            "slots": {"top": "w-top", "bottom": "w-bottom", "footwear": null, "accessories": "w-hat"},
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        });
        let item = legacy_preset_to_composite(&preset);
        assert_eq!(
            item["componentItemIds"],
            json!(["w-top", "w-bottom", "w-hat"])
        );
        assert_eq!(item["types"], json!(["top", "bottom", "accessories"]));
        assert_eq!(item["replace"], json!(true));
        assert_eq!(item["title"], json!("Town Outfit"));
    }

    #[test]
    fn all_null_slots_fall_back_to_accessories() {
        let preset = json!({"id": "p2", "characterId": null, "name": "Empty", "slots": {}});
        let item = legacy_preset_to_composite(&preset);
        assert_eq!(item["types"], json!(["accessories"]));
        assert_eq!(item["componentItemIds"], json!([]));
    }
}
