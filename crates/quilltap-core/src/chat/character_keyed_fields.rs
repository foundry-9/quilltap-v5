//! The chat-row JSON columns keyed by character id, and the one pass that
//! carries them across an id change — v4 `lib/chat/character-keyed-fields.ts`
//! (`e8e17b2d9`, bug 252), ported whole (P4.D317).
//!
//! A new-account restore (`services::backup::uuid_remap`) gives every character
//! a fresh id, and a `.qtap` import (`services::quilltap_import::reconcile`) can
//! map a bundled character onto another id. Each column below is
//! `{ [characterId]: … }`, so left alone it names characters that no longer
//! exist (bug 252):
//!
//! - `equippedOutfit` — `{ top, bottom, footwear, accessories, hair }` per
//!   character; slot item ids follow the wardrobe item remap.
//! - `characterAvatars` — `{ imageId, generatedAt, afterMessageCount }`;
//!   `imageId` follows the file remap.
//! - `courierCheckpoints` — `{ lastResolvedMessageId, resolvedAt }`; the
//!   message id follows the message remap.
//! - `pendingOutfitNotifications` — legacy, keyed only; values are kept.
//!
//! Two keyed caches are deliberately left out: `compiledIdentityStacks` (keyed
//! by participant, rebuilt on a miss) and `commonplaceSceneCache` (a miss only
//! re-emits the scene block).
//!
//! Pure: the caller supplies the remaps and does the logging. A character remap
//! returning `None` drops that character's entry (restore uses it for a
//! character the backup does not carry, rather than minting a phantom id).
//!
//! ## JS fidelity
//!
//! - **`JSON.parse`.** v4's `asObject` parses a string column with V8's
//!   `JSON.parse`; this port uses `serde_json::from_str`. They agree on every
//!   input but three classes V8 accepts and serde refuses (a number past `f64`
//!   such as `1e400`, a lone-surrogate `\u` escape, nesting deeper than 128):
//!   there v4 reads the column and v5 reports it `unreadable`. Recorded, not
//!   closed (P4.D317 R-2) — the tier-1 family carries one EXPECTED-DIFFERENT
//!   row per class.
//! - **Own-key order.** Every object this pass emits is a JS object, which
//!   `JSON.stringify` renders array-index keys first (ascending), then the rest
//!   in insertion order — at every depth. A v5 `Value` keeps its source order,
//!   so each emitted value is re-ordered the V8 way ([`v8_ordered_deep`]), and
//!   the entries of a column are walked in that order (`Object.entries`).
//! - **Truthiness.** `if (newId)` drops an entry whose new id is `null` OR `""`.

use serde_json::{Map, Value};

use crate::wardrobe::{normalize_equipped_slots, WARDROBE_SLOT_TYPES};

/// v4 `CHARACTER_KEYED_CHAT_FIELDS` — the four columns, in v4's order (also
/// the order the pass re-keys them and the order of [`CharacterKeyedChatRemap::patch`]).
pub const CHARACTER_KEYED_CHAT_FIELDS: [&str; 4] = [
    "equippedOutfit",
    "characterAvatars",
    "courierCheckpoints",
    "pendingOutfitNotifications",
];

/// v4 `CharacterKeyedRemaps` — the four id maps the caller supplies. Closures
/// over `&dyn Fn`, so a restore can route three of them through one
/// `RefCell<&mut UuidRemapper>` (the mint is shared).
pub struct CharacterKeyedRemaps<'a> {
    /// `None` (or an empty string) DROPS the character's entry.
    pub character_id: &'a dyn Fn(&str) -> Option<String>,
    pub wardrobe_item_id: &'a dyn Fn(&str) -> String,
    pub file_id: &'a dyn Fn(&str) -> String,
    pub message_id: &'a dyn Fn(&str) -> String,
}

/// v4 `CharacterKeyedChatRemap`.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterKeyedChatRemap {
    /// The rewritten columns; a column absent here is left as it was. Keys in
    /// [`CHARACTER_KEYED_CHAT_FIELDS`] order.
    pub patch: Map<String, Value>,
    /// Columns that held a value but not an object (nor a JSON object string),
    /// so were left alone.
    pub unreadable: Vec<&'static str>,
    /// Character entries dropped because the character remap returned `None`.
    pub dropped_entries: usize,
}

/// v4 `asObject` — an object, or a JSON string holding one (a column read back
/// raw). Never an array, never `null` (the string `"null"` parses to `null`).
fn as_object(value: &Value) -> Option<Map<String, Value>> {
    let parsed;
    let value = match value {
        Value::String(s) => {
            parsed = serde_json::from_str::<Value>(s).ok()?;
            &parsed
        }
        other => other,
    };
    match value {
        Value::Object(map) => match v8_ordered_deep(Value::Object(map.clone())) {
            Value::Object(m) => Some(m),
            _ => None,
        },
        _ => None,
    }
}

/// v4 `withMappedId` — replace one string field on an object entry; anything
/// else is kept as given. A JSON-string entry whose field is a string comes
/// back an OBJECT (v4 spreads the parsed value).
fn with_mapped_id(entry: &Value, field: &str, map: &dyn Fn(&str) -> String) -> Value {
    let Some(mut obj) = as_object(entry) else {
        return entry.clone();
    };
    let Some(id) = obj.get(field).and_then(Value::as_str).map(str::to_string) else {
        return entry.clone();
    };
    // `{ ...obj, [field]: map(obj[field]) }` — the key already exists, so the
    // spread keeps its position (`IndexMap::insert` on an existing key).
    obj.insert(field.to_string(), Value::String(map(&id)));
    Value::Object(obj)
}

/// v4 `remapCharacterKeyedChatFields` — remap every character-keyed column
/// present on the chat. A null or missing column is skipped; one that is not an
/// object is reported in `unreadable`.
pub fn remap_character_keyed_chat_fields(
    chat: &Value,
    remaps: &CharacterKeyedRemaps<'_>,
) -> CharacterKeyedChatRemap {
    let mut patch = Map::new();
    let mut unreadable: Vec<&'static str> = Vec::new();
    let mut dropped_entries = 0usize;

    for field in CHARACTER_KEYED_CHAT_FIELDS {
        let value = chat.get(field).unwrap_or(&Value::Null);
        if value.is_null() {
            continue;
        }
        let Some(map) = as_object(value) else {
            unreadable.push(field);
            continue;
        };
        let mut next = Map::new();
        // `Object.entries(map)` — `as_object` already V8-ordered it.
        for (character_id, entry) in &map {
            // The character remap runs BEFORE the value remap, and a dropped
            // entry's value is never remapped (the restore's mint order).
            match (remaps.character_id)(character_id) {
                Some(new_id) if !new_id.is_empty() => {
                    let mapped = map_value(field, entry, remaps);
                    next.insert(new_id, mapped);
                }
                _ => dropped_entries += 1,
            }
        }
        // `next[newId] = …` builds a JS object: an array-index new id sorts
        // first, as v4's `JSON.stringify` will render it.
        patch.insert(field.to_string(), v8_ordered_deep(Value::Object(next)));
    }

    CharacterKeyedChatRemap {
        patch,
        unreadable,
        dropped_entries,
    }
}

/// The per-column `mapValue` of v4's four `rekey` calls.
fn map_value(field: &str, entry: &Value, remaps: &CharacterKeyedRemaps<'_>) -> Value {
    match field {
        // "Same shape as the outfit history's remap: a full five-slot bag."
        // `normalizeEquippedSlots` salvages a malformed slot (an array keeps
        // its string elements, anything else reads `[]`) and fills a pre-hair
        // four-slot bag out to five; a JSON-STRING entry is not parsed here,
        // so it becomes five empty slots.
        "equippedOutfit" => {
            let normalized = normalize_equipped_slots(Some(entry));
            let mut out = Map::new();
            for slot in WARDROBE_SLOT_TYPES {
                let ids = normalized
                    .get(slot)
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(|id| Value::String((remaps.wardrobe_item_id)(id)))
                            .collect()
                    })
                    .unwrap_or_default();
                out.insert(slot.to_string(), Value::Array(ids));
            }
            Value::Object(out)
        }
        "characterAvatars" => with_mapped_id(entry, "imageId", remaps.file_id),
        "courierCheckpoints" => with_mapped_id(entry, "lastResolvedMessageId", remaps.message_id),
        // `pendingOutfitNotifications`: the entry as given; only the KEY moves.
        _ => v8_ordered_deep(entry.clone()),
    }
}

/// V8's own-key order at every depth: canonical array-index keys (decimal
/// integers below 2³² − 1) first, ascending, then the rest in insertion order.
fn v8_ordered_deep(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let index = |k: &str| -> Option<u32> {
                let n: u32 = k.parse().ok()?;
                (n != u32::MAX && k == n.to_string()).then_some(n)
            };
            let mut indexed: Vec<(u32, String, Value)> = Vec::new();
            let mut rest: Vec<(String, Value)> = Vec::new();
            for (k, v) in map {
                match index(&k) {
                    Some(n) => indexed.push((n, k, v8_ordered_deep(v))),
                    None => rest.push((k, v8_ordered_deep(v))),
                }
            }
            indexed.sort_by_key(|(n, _, _)| *n);
            let mut out = Map::new();
            for (_, k, v) in indexed {
                out.insert(k, v);
            }
            for (k, v) in rest {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(v8_ordered_deep).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    fn run(chat: Value, drop: &[&str]) -> (CharacterKeyedChatRemap, Vec<String>) {
        let minted = RefCell::new(Vec::<String>::new());
        let mint = |kind: &str, id: &str| {
            let n = minted.borrow().len() + 1;
            minted.borrow_mut().push(format!("{kind}:{id}"));
            format!("mint-{n}")
        };
        let character = |id: &str| {
            if drop.contains(&id) {
                None
            } else {
                Some(format!("new-{id}"))
            }
        };
        let item = |id: &str| format!("item-{id}");
        let file = |id: &str| mint("file", id);
        let message = |id: &str| mint("message", id);
        let out = remap_character_keyed_chat_fields(
            &chat,
            &CharacterKeyedRemaps {
                character_id: &character,
                wardrobe_item_id: &item,
                file_id: &file,
                message_id: &message,
            },
        );
        (out, minted.into_inner())
    }

    #[test]
    fn re_keys_all_four_columns_in_v4_order() {
        let (out, minted) = run(
            json!({
                "pendingOutfitNotifications": { "a": { "x": 1 } },
                "courierCheckpoints": { "a": { "lastResolvedMessageId": "m1", "resolvedAt": "t" } },
                "characterAvatars": { "a": { "imageId": "f1", "generatedAt": "g" } },
                "equippedOutfit": { "a": { "top": ["i1"] } },
            }),
            &[],
        );
        assert_eq!(
            serde_json::to_string(&out.patch).unwrap(),
            r#"{"equippedOutfit":{"new-a":{"top":["item-i1"],"bottom":[],"footwear":[],"accessories":[],"hair":[]}},"characterAvatars":{"new-a":{"imageId":"mint-1","generatedAt":"g"}},"courierCheckpoints":{"new-a":{"lastResolvedMessageId":"mint-2","resolvedAt":"t"}},"pendingOutfitNotifications":{"new-a":{"x":1}}}"#
        );
        assert_eq!(minted, vec!["file:f1", "message:m1"]);
    }

    #[test]
    fn a_dropped_character_is_counted_and_never_value_remapped() {
        let (out, minted) = run(
            json!({ "characterAvatars": { "gone": { "imageId": "f9" }, "a": { "imageId": "f1" } } }),
            &["gone"],
        );
        assert_eq!(out.dropped_entries, 1);
        assert_eq!(minted, vec!["file:f1"]);
    }

    #[test]
    fn empty_object_is_readable_and_unreadable_values_are_reported() {
        let (out, _) = run(
            json!({ "equippedOutfit": {}, "characterAvatars": "null", "courierCheckpoints": [1], "pendingOutfitNotifications": null }),
            &[],
        );
        assert_eq!(
            serde_json::to_string(&out.patch).unwrap(),
            r#"{"equippedOutfit":{}}"#
        );
        assert_eq!(
            out.unreadable,
            vec!["characterAvatars", "courierCheckpoints"]
        );
    }

    #[test]
    fn a_json_string_entry_with_a_string_field_comes_back_an_object() {
        let (out, _) = run(
            json!({ "characterAvatars": { "a": "{\"imageId\":\"f1\",\"2\":0,\"1\":0}" } }),
            &[],
        );
        assert_eq!(
            serde_json::to_string(&out.patch["characterAvatars"]).unwrap(),
            r#"{"new-a":{"1":0,"2":0,"imageId":"mint-1"}}"#
        );
    }
}
