//! Whether a resolved wardrobe item may be put on — the one rule the
//! `wardrobe_wear` tool and the chat's `?action=equip` route share (v4
//! `lib/wardrobe/wearable.ts`, bug 191 / `9ecebf986`; P4.D279, keystone C1 §5).
//!
//! Item lookups (the pool's `get` / `get_many`) include archived items on
//! purpose, so a garment archived after a chat dressed in it can still be
//! named. Putting one *on* is another matter: an archived item is retired, and
//! every wear path refuses it here with the same words.
//!
//! Pure — no I/O.

use serde_json::Value;

use crate::wardrobe::{Slots, WARDROBE_SLOT_TYPES};
use crate::wearable_pool::is_archived_truthy;

/// The refusal every wear path gives for an archived item (v4
/// `archivedWearMessage`).
pub fn archived_wear_message(title: &str) -> String {
    format!("Item \"{title}\" is archived and cannot be worn")
}

/// Why `item` may not be put on, or `None` when it may (v4 `wearRefusal`):
/// a truthy `archivedAt` is the one reason.
pub fn wear_refusal(item: &Value) -> Option<String> {
    if is_archived_truthy(item) {
        Some(archived_wear_message(
            item.get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ))
    } else {
        None
    }
}

/// The archived items among `items` that a whole-outfit commit (`set_all`)
/// would NEWLY put on (v4 `newlyWornArchivedItems`). An archived item already
/// in `current_slots` stays where it is: archiving never undresses anyone, so
/// re-committing a fitting that still holds it changes nothing about it.
pub fn newly_worn_archived_items<'a>(
    items: &'a [Value],
    current_slots: Option<&Slots>,
) -> Vec<&'a Value> {
    let mut already_worn: std::collections::HashSet<&str> = std::collections::HashSet::new();
    if let Some(slots) = current_slots {
        for slot in WARDROBE_SLOT_TYPES {
            for id in slots.slot(slot) {
                already_worn.insert(id.as_str());
            }
        }
    }
    items
        .iter()
        .filter(|item| {
            is_archived_truthy(item)
                && !already_worn
                    .contains(item.get("id").and_then(Value::as_str).unwrap_or_default())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refusal_and_message() {
        assert_eq!(
            archived_wear_message("Old Coat"),
            "Item \"Old Coat\" is archived and cannot be worn"
        );
        assert_eq!(
            wear_refusal(&json!({"title": "Old Coat", "archivedAt": "2026-01-01T00:00:00.000Z"})),
            Some("Item \"Old Coat\" is archived and cannot be worn".to_string())
        );
        assert_eq!(
            wear_refusal(&json!({"title": "Coat", "archivedAt": null})),
            None
        );
        assert_eq!(
            wear_refusal(&json!({"title": "Coat", "archivedAt": ""})),
            None
        );
    }

    #[test]
    fn newly_worn_keeps_an_already_worn_archived_item() {
        let items = vec![
            json!({"id": "a", "archivedAt": "2026-01-01T00:00:00.000Z"}),
            json!({"id": "b", "archivedAt": "2026-01-01T00:00:00.000Z"}),
            json!({"id": "c", "archivedAt": null}),
        ];
        let mut slots = Slots::fresh();
        slots.top.push("a".into());
        let newly: Vec<&str> = newly_worn_archived_items(&items, Some(&slots))
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(newly, vec!["b"]);
        let none: Vec<&str> = newly_worn_archived_items(&items, None)
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(none, vec!["a", "b"]);
    }
}
