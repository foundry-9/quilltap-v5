//! Wear operations — the shared front half of every "put it on" / "take it
//! off" gesture, for the wardrobe tools and the chat's `?action=equip` alike
//! (v4 `lib/wardrobe/wear-ops.ts`, `9ecebf986`; P4.D279, keystone C1 §5).
//!
//!   - [`resolve_wearable`] turns a reference (id, or title for the tools)
//!     into an item the character may wear in the requested way, or one
//!     refusal with one set of words: not found, archived (bug 191), or a slot
//!     the item doesn't cover.
//!   - [`wear_item`] maps a put-on mode to its gesture, applies it through
//!     [`apply_displacement`], and says what happened (layered / replaced) and
//!     where.
//!
//! Take-off is narrower on purpose. The route's `remove_from_slot` takes one
//! item out of one named slot; the `wardrobe_take_off` tool's `remove` takes
//! an item out of every slot it covers, which is that same gesture looped
//! over the item's slots ([`take_off_item`]).

use rusqlite::Connection;
use serde_json::Value;

use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::services::outfit_displacement::apply_displacement;
use crate::slot_ops::{ComputeDisplacedOptions, DisplacementMode, WearableNode};
use crate::wardrobe::Slots;
use crate::wearable::wear_refusal;
use crate::wearable_pool::WearablePool;

/// The put-on modes (v4 `PutOnMode`). `equip` is the ROUTE's deprecated alias
/// for `wear` — the route maps it, not this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutOnMode {
    Wear,
    Replace,
    AddToSlot,
}

impl PutOnMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PutOnMode::Wear => "wear",
            PutOnMode::Replace => "replace",
            PutOnMode::AddToSlot => "add_to_slot",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "wear" => Some(PutOnMode::Wear),
            "replace" => Some(PutOnMode::Replace),
            "add_to_slot" => Some(PutOnMode::AddToSlot),
            _ => None,
        }
    }

    fn displacement_mode(self) -> DisplacementMode {
        match self {
            PutOnMode::Wear => DisplacementMode::Wear,
            PutOnMode::Replace => DisplacementMode::Replace,
            PutOnMode::AddToSlot => DisplacementMode::AddToSlot,
        }
    }
}

/// v4 `WearableRef` — an id, or (tools only) a case-insensitive title, own
/// items first. JS truthiness: an empty string is no reference.
#[derive(Debug, Clone, Copy, Default)]
pub struct WearableRef<'a> {
    pub item_id: Option<&'a str>,
    pub item_title: Option<&'a str>,
}

/// v4's `reason` on a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WearableRefusal {
    NotFound,
    Archived,
    Slot,
}

impl WearableRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            WearableRefusal::NotFound => "not_found",
            WearableRefusal::Archived => "archived",
            WearableRefusal::Slot => "slot",
        }
    }
}

/// v4 `WearableResolution`.
#[derive(Debug, Clone, PartialEq)]
pub enum WearableResolution<'p> {
    Ok(&'p Value),
    Refused {
        reason: WearableRefusal,
        message: String,
    },
}

/// The not-found wording every wardrobe surface uses (v4
/// `wardrobeItemNotFoundMessage`): `Wardrobe item not found` + ` with ID
/// "<id>"` when the id is truthy + ` with title "<title>"` when the title is.
pub fn wardrobe_item_not_found_message(item_id: Option<&str>, item_title: Option<&str>) -> String {
    let mut out = String::from("Wardrobe item not found");
    if let Some(id) = item_id.filter(|s| !s.is_empty()) {
        out.push_str(&format!(" with ID \"{id}\""));
    }
    if let Some(title) = item_title.filter(|s| !s.is_empty()) {
        out.push_str(&format!(" with title \"{title}\""));
    }
    out
}

/// Find an item in the pool by id, then by title (v4 `findInPool`). An id
/// miss falls through to the title; archived items ARE found.
pub fn find_in_pool<'p>(pool: &'p WearablePool, r: WearableRef<'_>) -> Option<&'p Value> {
    if let Some(id) = r.item_id.filter(|s| !s.is_empty()) {
        if let Some(found) = pool.get(id) {
            return Some(found);
        }
    }
    if let Some(title) = r.item_title.filter(|s| !s.is_empty()) {
        return pool.find_by_title(title);
    }
    None
}

fn item_types(item: &Value) -> Vec<String> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a reference to an item the character may put on in `mode` (and,
/// for `add_to_slot`, into `slot`) — v4 `resolveWearable`.
pub fn resolve_wearable<'p>(
    pool: &'p WearablePool,
    r: WearableRef<'_>,
    mode: PutOnMode,
    slot: Option<&str>,
) -> WearableResolution<'p> {
    let Some(item) = find_in_pool(pool, r) else {
        return WearableResolution::Refused {
            reason: WearableRefusal::NotFound,
            message: wardrobe_item_not_found_message(r.item_id, r.item_title),
        };
    };
    if let Some(message) = wear_refusal(item) {
        return WearableResolution::Refused {
            reason: WearableRefusal::Archived,
            message,
        };
    }
    if mode == PutOnMode::AddToSlot {
        if let Some(slot) = slot.filter(|s| !s.is_empty()) {
            let types = item_types(item);
            if !types.iter().any(|t| t == slot) {
                return WearableResolution::Refused {
                    reason: WearableRefusal::Slot,
                    message: format!(
                        "Item \"{}\" (types: {}) cannot be added to the \"{slot}\" slot",
                        item.get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        types.join(", ")
                    ),
                };
            }
        }
    }
    WearableResolution::Ok(item)
}

/// What a put-on did to the slots it touched (v4 `WearOutcome.effect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WearEffect {
    Layered,
    Replaced,
}

impl WearEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            WearEffect::Layered => "layered",
            WearEffect::Replaced => "replaced",
        }
    }
}

/// v4 `WearOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub struct WearOutcome {
    pub slots: Slots,
    pub effect: WearEffect,
    pub slots_affected: Vec<String>,
}

/// Put a resolved item on in `mode` and commit it (v4 `wearItem`): one
/// [`apply_displacement`] over the pool's `by_id` (so a composite dissolves),
/// then the effect — `add_to_slot` is always `layered` into that one slot;
/// otherwise `replaced` when the mode is `replace` or the item's own `replace`
/// flag is set, else `layered`, across every slot the item's `types` name.
pub fn wear_item(
    main: &Connection,
    chat_id: &str,
    pool: &WearablePool,
    item: &Value,
    mode: PutOnMode,
    slot: Option<&str>,
    source: EquipSource,
) -> Result<WearOutcome, DbError> {
    let node = WearableNode::from_value(item);
    let slots = apply_displacement(
        main,
        chat_id,
        &pool.character_id,
        &ComputeDisplacedOptions {
            mode: mode.displacement_mode(),
            item: Some(&node),
            slot,
            item_id: None,
            items_by_id: Some(pool.by_id()),
        },
        source,
    )?;
    if mode == PutOnMode::AddToSlot {
        return Ok(WearOutcome {
            slots,
            effect: WearEffect::Layered,
            slots_affected: slot.map(|s| vec![s.to_string()]).unwrap_or_default(),
        });
    }
    let effect = if mode == PutOnMode::Replace || node.replace {
        WearEffect::Replaced
    } else {
        WearEffect::Layered
    };
    Ok(WearOutcome {
        slots,
        effect,
        slots_affected: item_types(item),
    })
}

/// Take one item off the given slots (v4 `takeOffItem` — the tool's `remove`:
/// every slot the item covers, or the one it names). Other layers in those
/// slots stay. v4 passes no `source`; the take-off mode commits as `take-off`
/// regardless.
pub fn take_off_item(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    item_id: &str,
    slots: &[String],
) -> Result<(), DbError> {
    for slot in slots {
        apply_displacement(
            main,
            chat_id,
            character_id,
            &ComputeDisplacedOptions {
                mode: DisplacementMode::RemoveFromSlot,
                item: None,
                slot: Some(slot),
                item_id: Some(item_id),
                items_by_id: None,
            },
            EquipSource::Ui,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wearable_pool::{build_wearable_pool, WardrobeTierMounts};
    use serde_json::json;

    fn pool() -> WearablePool {
        build_wearable_pool(
            "c1",
            WardrobeTierMounts::default(),
            vec![
                json!({"id": "own-coat", "title": "Coat", "types": ["top"], "characterId": "c1", "archivedAt": null}),
                json!({"id": "own-old", "title": "Old Hat", "types": ["accessories"], "characterId": "c1", "archivedAt": "2026-01-01T00:00:00.000Z"}),
            ],
            vec![],
            vec![],
            vec![
                json!({"id": "gen-boots", "title": "Boots", "types": ["footwear"], "characterId": null, "archivedAt": null}),
                json!({"id": "gen-gone", "title": "Gone Cloak", "types": ["top"], "characterId": null, "archivedAt": "2026-01-01T00:00:00.000Z"}),
            ],
        )
    }

    fn id(v: &Value) -> &str {
        v["id"].as_str().unwrap()
    }

    #[test]
    fn not_found_message_shapes() {
        assert_eq!(
            wardrobe_item_not_found_message(None, None),
            "Wardrobe item not found"
        );
        assert_eq!(
            wardrobe_item_not_found_message(Some("x"), Some("")),
            "Wardrobe item not found with ID \"x\""
        );
        assert_eq!(
            wardrobe_item_not_found_message(Some("x"), Some("Hat")),
            "Wardrobe item not found with ID \"x\" with title \"Hat\""
        );
    }

    #[test]
    fn find_in_pool_by_id_then_title_archived_found() {
        let p = pool();
        assert_eq!(
            id(find_in_pool(
                &p,
                WearableRef {
                    item_id: Some("gen-boots"),
                    item_title: None
                }
            )
            .unwrap()),
            "gen-boots"
        );
        // An id miss falls through to the title.
        assert_eq!(
            id(find_in_pool(
                &p,
                WearableRef {
                    item_id: Some("nope"),
                    item_title: Some(" coat ")
                }
            )
            .unwrap()),
            "own-coat"
        );
        // An archived OWN item is found; an archived SHARED one is not.
        assert_eq!(
            id(find_in_pool(
                &p,
                WearableRef {
                    item_id: None,
                    item_title: Some("old hat")
                }
            )
            .unwrap()),
            "own-old"
        );
        assert!(find_in_pool(
            &p,
            WearableRef {
                item_id: None,
                item_title: Some("Gone Cloak")
            }
        )
        .is_none());
        assert!(find_in_pool(
            &p,
            WearableRef {
                item_id: Some(""),
                item_title: Some("")
            }
        )
        .is_none());
    }

    #[test]
    fn resolve_wearable_reasons_and_sentences() {
        let p = pool();
        match resolve_wearable(
            &p,
            WearableRef {
                item_id: Some("nope"),
                item_title: None,
            },
            PutOnMode::Wear,
            None,
        ) {
            WearableResolution::Refused { reason, message } => {
                assert_eq!(reason, WearableRefusal::NotFound);
                assert_eq!(message, "Wardrobe item not found with ID \"nope\"");
            }
            other => panic!("{other:?}"),
        }
        match resolve_wearable(
            &p,
            WearableRef {
                item_id: Some("own-old"),
                item_title: None,
            },
            PutOnMode::AddToSlot,
            Some("top"),
        ) {
            WearableResolution::Refused { reason, message } => {
                assert_eq!(reason, WearableRefusal::Archived, "archived wins over slot");
                assert_eq!(message, "Item \"Old Hat\" is archived and cannot be worn");
            }
            other => panic!("{other:?}"),
        }
        match resolve_wearable(
            &p,
            WearableRef {
                item_id: Some("own-coat"),
                item_title: None,
            },
            PutOnMode::AddToSlot,
            Some("footwear"),
        ) {
            WearableResolution::Refused { reason, message } => {
                assert_eq!(reason, WearableRefusal::Slot);
                assert_eq!(
                    message,
                    "Item \"Coat\" (types: top) cannot be added to the \"footwear\" slot"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            resolve_wearable(
                &p,
                WearableRef {
                    item_id: Some("own-coat"),
                    item_title: None
                },
                PutOnMode::AddToSlot,
                None
            ),
            WearableResolution::Ok(_)
        ));
        assert!(matches!(
            resolve_wearable(
                &p,
                WearableRef {
                    item_id: Some("own-coat"),
                    item_title: None
                },
                PutOnMode::Replace,
                Some("footwear")
            ),
            WearableResolution::Ok(_)
        ));
    }
}
