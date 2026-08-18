//! v4 `lib/backup/restore/legacy-migrations.ts` — the three pure transforms
//! that fold pre-rework backup shapes. No DB, no filesystem.
//!
//! Both are consumed at PARSE time (`archive::parse_backup_zip`), not by the
//! orchestrator: legacy outfit presets become composite wardrobe items, and the
//! legacy per-character equipped-slot map (a single UUID-or-null per slot)
//! becomes the array shape the restore path expects.

use serde_json::{Map, Value};

/// v4 `LEGACY_SLOT_ORDER` (`:46`) — the order `componentItemIds` and `types`
/// are derived in, so a folded composite is stable. **FROZEN at the four
/// pre-hair names** — legacy presets and the `looksLegacy` probe address a
/// shape that predates the hair slot; v4 `4423ad10` deliberately left them at
/// four (its design doc §7.4 says otherwise — trust the code). Do NOT widen.
const LEGACY_SLOT_ORDER: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

/// v4 `dedupeAndOrderSlotTypes` (`:56`). The slot NAMES double as wardrobe item
/// types, so the "dedupe" is vacuous over four distinct keys — carried anyway.
///
/// The tail matters: **if every slot is null it returns `['accessories']`**
/// (`:70`), so a malformed legacy preset still satisfies the schema's
/// "a composite declares at least one type".
pub fn dedupe_and_order_slot_types(slots: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for slot in LEGACY_SLOT_ORDER {
        if is_truthy(slots.get(slot)) && !out.iter().any(|s| s == slot) {
            out.push(slot.to_string());
        }
    }
    if out.is_empty() {
        out.push("accessories".to_string());
    }
    out
}

/// v4 `orderedComponentIds` (`:77`) — the non-null ids in slot order.
pub fn ordered_component_ids(slots: &Value) -> Vec<Value> {
    let mut ids = Vec::new();
    for slot in LEGACY_SLOT_ORDER {
        let v = slots.get(slot);
        if is_truthy(v) {
            ids.push(v.cloned().unwrap_or(Value::Null));
        }
    }
    ids
}

/// v4's `if (slots[slot])` / `if (id)` — JS truthiness, so `null`, absent, `""`
/// and `0` are all falsy.
fn is_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(_) => true,
    }
}

/// v4 `upgradeLegacyEquippedSlots` (`:90`) — `null`/absent → `null`; otherwise
/// each slot becomes `Array.isArray ? filter(string) : string ? [string] : []`.
/// **Idempotent**: an already-array shape passes through.
///
/// Built over the LIVE slot list rather than the four legacy names (v4
/// `4423ad10`): slots the legacy shape never had (hair) simply upgrade to
/// `[]`, which is the correct reading of a backup written before they existed.
pub fn upgrade_legacy_equipped_slots(raw: &Value) -> Option<Value> {
    // v4's `if (!raw) return null` — JS falsiness on the whole object.
    if !is_truthy(Some(raw)) {
        return None;
    }
    let upgrade = |val: Option<&Value>| -> Value {
        match val {
            Some(Value::Array(a)) => Value::Array(
                a.iter()
                    .filter(|v| matches!(v, Value::String(_)))
                    .cloned()
                    .collect(),
            ),
            Some(Value::String(s)) => Value::Array(vec![Value::String(s.clone())]),
            _ => Value::Array(Vec::new()),
        }
    };
    let mut out = Map::new();
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        out.insert(slot.to_string(), upgrade(raw.get(slot)));
    }
    Some(Value::Object(out))
}

/// v4 `archive.ts:157-167`'s `looksLegacy` predicate: any of the four slots
/// holding a **string** (rather than an array) OR holding **null** (the old
/// shape uses null where the new one uses `[]`).
pub fn looks_legacy(slots: &Value) -> bool {
    if !slots.is_object() {
        return false;
    }
    LEGACY_SLOT_ORDER
        .iter()
        .any(|slot| matches!(slots.get(*slot), Some(Value::String(_)) | Some(Value::Null)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slot_types_follow_slot_order_and_fall_back_to_accessories() {
        let slots = json!({"top": "a", "bottom": null, "footwear": "c", "accessories": null});
        assert_eq!(dedupe_and_order_slot_types(&slots), vec!["top", "footwear"]);
        let empty = json!({"top": null, "bottom": null, "footwear": null, "accessories": null});
        assert_eq!(dedupe_and_order_slot_types(&empty), vec!["accessories"]);
    }

    #[test]
    fn component_ids_follow_slot_order() {
        let slots = json!({"top": "a", "bottom": null, "footwear": "c", "accessories": "d"});
        assert_eq!(
            ordered_component_ids(&slots),
            vec![json!("a"), json!("c"), json!("d")]
        );
    }

    #[test]
    fn equipped_slot_upgrade_is_idempotent() {
        let legacy = json!({"top": "a", "bottom": null, "footwear": null, "accessories": null});
        let once = upgrade_legacy_equipped_slots(&legacy).unwrap();
        // Slots the legacy shape never had (hair) upgrade to [] — the correct
        // reading of a backup written before they existed.
        assert_eq!(
            once,
            json!({"top": ["a"], "bottom": [], "footwear": [], "accessories": [], "hair": []})
        );
        assert_eq!(upgrade_legacy_equipped_slots(&once).unwrap(), once);
        assert_eq!(upgrade_legacy_equipped_slots(&Value::Null), None);
    }

    #[test]
    fn non_string_array_members_are_dropped() {
        let raw =
            json!({"top": ["a", 3, null, "b"], "bottom": [], "footwear": [], "accessories": []});
        assert_eq!(
            upgrade_legacy_equipped_slots(&raw).unwrap()["top"],
            json!(["a", "b"])
        );
    }

    #[test]
    fn looks_legacy_detects_both_string_and_null_slots() {
        assert!(looks_legacy(&json!({"top": "a"})));
        assert!(looks_legacy(
            &json!({"top": [], "bottom": null, "footwear": [], "accessories": []})
        ));
        assert!(!looks_legacy(
            &json!({"top": [], "bottom": [], "footwear": [], "accessories": []})
        ));
    }
}
