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
//! item out of one named slot; [`take_off_wearable`] is the by-reference form
//! the `wardrobe_take_off` tool's `remove` and Pascal's `take_off` wardrobe
//! effect share (v4 `f0db8cbb8`). Worn composites are stored dissolved, so
//! filtering the composite's own id out of a slot would remove nothing — a
//! composite comes off by its leaves. [`load_current_slots`] is the ONE reader
//! of a character's slots in a chat ([`apply_displacement`] reads through it).
//!
//! v4 keeps `takeOffItem` exported with no production caller left; v5 retired
//! its twin (P4.D306 R-4) — [`take_off_wearable`]'s single-garment arm is the
//! same loop.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::services::outfit_displacement::apply_displacement;
use crate::slot_ops::{ComputeDisplacedOptions, DisplacementMode, WearableNode};
use crate::wardrobe::{Slots, WARDROBE_SLOT_TYPES};
use crate::wearable::wear_refusal;
use crate::wearable_pool::{component_graph, WearablePool};

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

/// v4 `TakeOffOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeOffOutcome {
    /// The slots after the last commit, or what was loaded when nothing was
    /// committed.
    pub slots: Slots,
    /// Every slot a removal was committed against, in slot order.
    pub slots_affected: Vec<String>,
}

/// One `remove_from_slot` commit. v4 passes no `source`; the take-off mode
/// commits as `take-off` regardless (the chokepoint forces it).
fn remove_from_slot(
    main: &Connection,
    chat_id: &str,
    character_id: &str,
    slot: &str,
    item_id: &str,
) -> Result<Slots, DbError> {
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
    )
}

/// Take a resolved item off, by reference (v4 `takeOffWearable`) — the
/// `wardrobe_take_off` tool's `remove` and Pascal's `take_off` effect both
/// come through here.
///
/// - A single garment comes off every slot it covers, or only `slot` when one
///   is named — other layers stay.
/// - A composite comes off by its leaves: every item in its component graph
///   (the composite itself included, in case it was stored whole) is taken out
///   of each slot it currently occupies, restricted to `slot` when one is
///   named. The occupancy is read from ONE snapshot, slots walked in
///   [`WARDROBE_SLOT_TYPES`] order and ids in each slot's stored order. A
///   composite with nothing of it on commits nothing.
///
/// Archived items may be taken off; archiving never undresses anyone.
pub fn take_off_wearable(
    main: &Connection,
    chat_id: &str,
    pool: &WearablePool,
    item: &Value,
    slot: Option<&str>,
) -> Result<TakeOffOutcome, DbError> {
    let character_id = pool.character_id.as_str();
    let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default();
    let is_composite = item
        .get("componentItemIds")
        .and_then(Value::as_array)
        .is_some_and(|c| !c.is_empty());

    if !is_composite {
        let slots_affected: Vec<String> = match slot {
            Some(s) => vec![s.to_string()],
            None => item_types(item),
        };
        let mut slots: Option<Slots> = None;
        for s in &slots_affected {
            slots = Some(remove_from_slot(main, chat_id, character_id, s, item_id)?);
        }
        let slots = match slots {
            Some(s) => s,
            None => load_current_slots(main, chat_id, character_id),
        };
        return Ok(TakeOffOutcome {
            slots,
            slots_affected,
        });
    }

    let current = load_current_slots(main, chat_id, character_id);
    let graph_ids: std::collections::HashSet<String> =
        component_graph(pool, &[item_id.to_string()])
            .iter()
            .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
    let candidate_slots: Vec<&str> = match slot {
        Some(s) => vec![s],
        None => WARDROBE_SLOT_TYPES.to_vec(),
    };

    let mut slots = current.clone();
    let mut slots_affected: Vec<String> = Vec::new();
    for s in candidate_slots {
        let worn: Vec<&String> = current
            .slot(s)
            .iter()
            .filter(|id| graph_ids.contains(id.as_str()))
            .collect();
        for id in &worn {
            slots = remove_from_slot(main, chat_id, character_id, s, id)?;
        }
        if !worn.is_empty() {
            slots_affected.push(s.to_string());
        }
    }
    Ok(TakeOffOutcome {
        slots,
        slots_affected,
    })
}

/// The character's slots in a chat right now — a fresh empty outfit when none
/// are stored (v4 `loadCurrentSlots`: `current ? cloneEquippedSlots(current) :
/// makeEmptyEquippedSlots()`; [`Slots::from_value`] IS that pair). The ONE
/// reader: [`apply_displacement`] loads through it too.
pub(crate) fn load_current_slots(main: &Connection, chat_id: &str, character_id: &str) -> Slots {
    let stored =
        ChatOutfitsRepository::new(main).get_equipped_outfit_for_character(chat_id, character_id);
    Slots::from_value(stored.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;
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

    // ---- v4 `wear-ops.test.ts` `describe('wearItem')` ×3 + `describe(
    // 'takeOffItem')`. v4 mocks `applyDisplacement` and asserts the call; v5
    // runs the REAL one over an in-memory `chats` + `wardrobe_wear_stats` pair
    // (the `services::outfit_displacement` rig) and reads the call back out of
    // what it committed and the `[applyDisplacement] Outfit change committed`
    // line it logs — mode, item, slot and source per call.

    const CHAT: &str = "c0000001-0000-4000-8000-000000000001";
    const CHAR: &str = "c1c1c1c1-0000-0000-0000-000000000001";

    /// v4's test wardrobe (`shirt`, the replace-flagged `dress`) plus a
    /// composite (`suit` = shirt + trousers) — the case v4's
    /// `itemsById: pool.byId` argument exists for.
    fn wear_pool() -> WearablePool {
        let own = |id: &str, title: &str, types: &[&str], extra: Value| {
            let mut v = json!({
                "id": id, "title": title, "types": types, "characterId": CHAR,
                "componentItemIds": [], "replace": false, "archivedAt": null,
            });
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            v
        };
        build_wearable_pool(
            CHAR,
            WardrobeTierMounts::default(),
            vec![
                own("shirt", "Linen Shirt", &["top"], json!({})),
                own("trousers", "Trousers", &["bottom"], json!({})),
                own(
                    "dress",
                    "Sundress",
                    &["top", "bottom"],
                    json!({"replace": true}),
                ),
                own(
                    "suit",
                    "Suit",
                    &["top", "bottom"],
                    json!({"componentItemIds": ["shirt", "trousers"]}),
                ),
                own("boots", "Riding Boots", &["footwear"], json!({})),
                own("bare", "Odd Charm", &[], json!({})),
                // v4's applier test wardrobe: a three-leaf composite.
                own(
                    "regalia",
                    "Regalia",
                    &["top", "bottom", "footwear"],
                    json!({"componentItemIds": ["shirt", "trousers", "boots"]}),
                ),
            ],
            vec![],
            vec![],
            vec![],
        )
    }

    /// A main partition with the CURRENT `chats` DDL, the wear ledger, and
    /// one chat whose `equippedOutfit` for [`CHAR`] is `initial`.
    fn rig(initial: Option<Value>) -> Connection {
        let schema: Value =
            serde_json::from_str(include_str!("services/provisioning/fresh_schema.json")).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .unwrap();
        conn.execute_batch(ddl).unwrap();
        for sql in crate::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL {
            conn.execute_batch(sql).unwrap();
        }
        let outfit = initial.map(|slots| json!({ CHAR: slots }).to_string());
        conn.execute(
            "INSERT INTO chats (id, userId, title, equippedOutfit, createdAt, updatedAt) \
             VALUES (?1, 'u', 't', ?2, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            rusqlite::params![CHAT, outfit],
        )
        .unwrap();
        conn
    }

    fn stored(conn: &Connection) -> Slots {
        Slots::from_value(
            crate::db::chats_outfits::ChatOutfitsRepository::new(conn)
                .get_equipped_outfit_for_character(CHAT, CHAR)
                .as_ref(),
        )
    }

    /// The tail of every `Outfit change committed` line — one per
    /// `applyDisplacement` call, in call order.
    fn commits(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains("[applyDisplacement] Outfit change committed"))
            .map(|l| {
                l.split_once(&format!("characterId={CHAR} "))
                    .unwrap()
                    .1
                    .to_string()
            })
            .collect()
    }

    fn strs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wear_item_commits_through_apply_displacement_with_the_source_passed() {
        let p = wear_pool();
        let conn = rig(None);
        let (outcome, lines) = captured_with(|| {
            wear_item(
                &conn,
                CHAT,
                &p,
                p.get("shirt").unwrap(),
                PutOnMode::Wear,
                None,
                EquipSource::Tool,
            )
        });
        let outcome = outcome.unwrap();
        assert_eq!(outcome.effect, WearEffect::Layered);
        assert_eq!(outcome.slots_affected, strs(&["top"]));
        assert_eq!(outcome.slots.top, strs(&["shirt"]));
        assert_eq!(stored(&conn), outcome.slots);
        // ONE call: `{ mode: 'wear', item, slot: undefined }`, source `tool`.
        assert_eq!(
            commits(&lines),
            vec!["mode=wear itemId=shirt slot=null source=tool"]
        );

        // The `ui` source passes straight through as well.
        let conn = rig(None);
        let (_, lines) = captured_with(|| {
            wear_item(
                &conn,
                CHAT,
                &p,
                p.get("shirt").unwrap(),
                PutOnMode::Wear,
                None,
                EquipSource::Ui,
            )
            .unwrap()
        });
        assert_eq!(
            commits(&lines),
            vec!["mode=wear itemId=shirt slot=null source=ui"]
        );
    }

    #[test]
    fn wear_item_hands_the_pool_lookup_so_a_composite_dissolves() {
        // v4's `itemsById: pool.byId`: without it the suit would be stored
        // whole; with it the suit goes on as its parts.
        let p = wear_pool();
        let conn = rig(None);
        let outcome = wear_item(
            &conn,
            CHAT,
            &p,
            p.get("suit").unwrap(),
            PutOnMode::Wear,
            None,
            EquipSource::Ui,
        )
        .unwrap();
        assert_eq!(outcome.slots.top, strs(&["shirt"]));
        assert_eq!(outcome.slots.bottom, strs(&["trousers"]));
        assert_eq!(stored(&conn), outcome.slots);
        assert_eq!(outcome.effect, WearEffect::Layered);
        assert_eq!(outcome.slots_affected, strs(&["top", "bottom"]));
    }

    #[test]
    fn wear_item_reports_replaced_for_a_replace_flagged_item_or_the_replace_mode() {
        let p = wear_pool();
        let conn = rig(None);
        let flagged = wear_item(
            &conn,
            CHAT,
            &p,
            p.get("dress").unwrap(),
            PutOnMode::Wear,
            None,
            EquipSource::Ui,
        )
        .unwrap();
        assert_eq!(flagged.effect, WearEffect::Replaced);
        assert_eq!(flagged.slots_affected, strs(&["top", "bottom"]));

        let conn = rig(Some(json!({ "top": ["vest"] })));
        let by_mode = wear_item(
            &conn,
            CHAT,
            &p,
            p.get("shirt").unwrap(),
            PutOnMode::Replace,
            None,
            EquipSource::Ui,
        )
        .unwrap();
        assert_eq!(by_mode.effect, WearEffect::Replaced);
        assert_eq!(by_mode.slots.top, strs(&["shirt"]), "the vest is replaced");

        // Neither: a plain `wear` of an unflagged item layers.
        let conn = rig(Some(json!({ "top": ["vest"] })));
        let layered = wear_item(
            &conn,
            CHAT,
            &p,
            p.get("shirt").unwrap(),
            PutOnMode::Wear,
            None,
            EquipSource::Ui,
        )
        .unwrap();
        assert_eq!(layered.effect, WearEffect::Layered);
        assert_eq!(layered.slots.top, strs(&["vest", "shirt"]));
    }

    #[test]
    fn wear_item_add_to_slot_layers_into_the_one_named_slot() {
        // `add_to_slot` is `layered` even for the replace-flagged dress, and
        // names only the slot it went into.
        let p = wear_pool();
        let conn = rig(None);
        let (outcome, lines) = captured_with(|| {
            wear_item(
                &conn,
                CHAT,
                &p,
                p.get("dress").unwrap(),
                PutOnMode::AddToSlot,
                Some("bottom"),
                EquipSource::Ui,
            )
        });
        let outcome = outcome.unwrap();
        assert_eq!(outcome.effect, WearEffect::Layered);
        assert_eq!(outcome.slots_affected, strs(&["bottom"]));
        assert_eq!(outcome.slots.bottom, strs(&["dress"]));
        assert!(outcome.slots.top.is_empty());
        assert_eq!(
            commits(&lines),
            vec!["mode=add_to_slot itemId=dress slot=bottom source=ui"]
        );
    }

    // ---- v4 `takeOffWearable` (`f0db8cbb8`): `wear-ops.ts`'s persisted half,
    // proven here (the oracle's `wear-ops.ts` corpus is the pure half only).

    #[test]
    fn take_off_wearable_single_garment_all_slots() {
        // The retired `take_off_item`'s own case: every slot the item covers,
        // one call per slot; other layers and unnamed slots stay.
        let p = wear_pool();
        let conn = rig(Some(json!({
            "top": ["vest", "dress"],
            "bottom": ["dress"],
            "footwear": ["dress"],
        })));
        let (result, lines) =
            captured_with(|| take_off_wearable(&conn, CHAT, &p, p.get("dress").unwrap(), None));
        let outcome = result.unwrap();
        assert_eq!(
            commits(&lines),
            vec![
                "mode=remove_from_slot itemId=dress slot=top source=take-off",
                "mode=remove_from_slot itemId=dress slot=bottom source=take-off",
            ]
        );
        assert_eq!(outcome.slots_affected, strs(&["top", "bottom"]));
        let after = stored(&conn);
        assert_eq!(outcome.slots, after, "the last commit's slots");
        assert_eq!(after.top, strs(&["vest"]), "other layers stay");
        assert!(after.bottom.is_empty());
        assert_eq!(
            after.footwear,
            strs(&["dress"]),
            "an unnamed slot is untouched"
        );
    }

    #[test]
    fn take_off_wearable_single_garment_restricted() {
        let p = wear_pool();
        let conn = rig(Some(json!({ "top": ["dress"], "bottom": ["dress"] })));
        let (result, lines) = captured_with(|| {
            take_off_wearable(&conn, CHAT, &p, p.get("dress").unwrap(), Some("top"))
        });
        let outcome = result.unwrap();
        assert_eq!(
            commits(&lines),
            vec!["mode=remove_from_slot itemId=dress slot=top source=take-off"]
        );
        assert_eq!(outcome.slots_affected, strs(&["top"]));
        assert_eq!(outcome.slots.bottom, strs(&["dress"]));
        assert_eq!(stored(&conn), outcome.slots);
    }

    #[test]
    fn take_off_wearable_single_garment_types_empty_no_commit() {
        // `types: []` and no slot named → zero commits, `slots_affected: []`,
        // and the slots are what was loaded.
        let p = wear_pool();
        let conn = rig(Some(json!({ "top": ["vest"] })));
        let (result, lines) =
            captured_with(|| take_off_wearable(&conn, CHAT, &p, p.get("bare").unwrap(), None));
        let outcome = result.unwrap();
        assert!(commits(&lines).is_empty());
        assert!(outcome.slots_affected.is_empty());
        assert_eq!(outcome.slots.top, strs(&["vest"]));
        assert_eq!(outcome.slots, stored(&conn));
    }

    #[test]
    fn take_off_wearable_composite_by_leaves() {
        // v4's jest expectation, exactly: worn dissolved, the composite comes
        // off by its leaves in slot order; the scarf (not of it) stays.
        let p = wear_pool();
        let conn = rig(Some(json!({
            "top": ["shirt"],
            "bottom": ["trousers"],
            "footwear": ["boots"],
            "accessories": ["scarf"],
        })));
        let (result, lines) =
            captured_with(|| take_off_wearable(&conn, CHAT, &p, p.get("regalia").unwrap(), None));
        let outcome = result.unwrap();
        assert_eq!(
            commits(&lines),
            vec![
                "mode=remove_from_slot itemId=shirt slot=top source=take-off",
                "mode=remove_from_slot itemId=trousers slot=bottom source=take-off",
                "mode=remove_from_slot itemId=boots slot=footwear source=take-off",
            ]
        );
        assert_eq!(outcome.slots_affected, strs(&["top", "bottom", "footwear"]));
        let after = stored(&conn);
        assert_eq!(outcome.slots, after);
        assert!(after.top.is_empty() && after.bottom.is_empty() && after.footwear.is_empty());
        assert_eq!(after.accessories, strs(&["scarf"]));
    }

    #[test]
    fn take_off_wearable_composite_stored_whole_comes_off_by_its_own_id() {
        // The graph includes the root, "in case it was stored whole".
        let p = wear_pool();
        let conn = rig(Some(json!({ "top": ["suit", "vest"], "bottom": ["suit"] })));
        let (result, lines) =
            captured_with(|| take_off_wearable(&conn, CHAT, &p, p.get("suit").unwrap(), None));
        let outcome = result.unwrap();
        assert_eq!(
            commits(&lines),
            vec![
                "mode=remove_from_slot itemId=suit slot=top source=take-off",
                "mode=remove_from_slot itemId=suit slot=bottom source=take-off",
            ]
        );
        assert_eq!(outcome.slots.top, strs(&["vest"]));
        assert_eq!(outcome.slots_affected, strs(&["top", "bottom"]));
    }

    #[test]
    fn take_off_wearable_composite_nothing_on_commits_nothing() {
        let p = wear_pool();
        let conn = rig(Some(json!({ "top": ["vest"] })));
        let (result, lines) =
            captured_with(|| take_off_wearable(&conn, CHAT, &p, p.get("regalia").unwrap(), None));
        let outcome = result.unwrap();
        assert!(commits(&lines).is_empty());
        assert!(outcome.slots_affected.is_empty());
        assert_eq!(outcome.slots, stored(&conn), "the loaded snapshot");
        assert_eq!(outcome.slots.top, strs(&["vest"]));
    }

    #[test]
    fn take_off_wearable_composite_restricted_slot() {
        let p = wear_pool();
        let conn = rig(Some(json!({
            "top": ["vest", "shirt"],
            "bottom": ["trousers"],
            "footwear": ["boots"],
        })));
        let (result, lines) = captured_with(|| {
            take_off_wearable(&conn, CHAT, &p, p.get("regalia").unwrap(), Some("top"))
        });
        let outcome = result.unwrap();
        assert_eq!(
            commits(&lines),
            vec!["mode=remove_from_slot itemId=shirt slot=top source=take-off"]
        );
        assert_eq!(outcome.slots_affected, strs(&["top"]));
        assert_eq!(outcome.slots.top, strs(&["vest"]));
        assert_eq!(outcome.slots.bottom, strs(&["trousers"]));
        assert_eq!(outcome.slots, stored(&conn));
    }

    #[test]
    fn load_current_slots_reads_the_stored_outfit_or_a_fresh_one() {
        let conn = rig(Some(json!({ "hair": ["ribbon"] })));
        let slots = load_current_slots(&conn, CHAT, CHAR);
        assert_eq!(slots.hair, strs(&["ribbon"]));
        assert!(slots.top.is_empty());
        let conn = rig(None);
        assert_eq!(load_current_slots(&conn, CHAT, CHAR), Slots::fresh());
    }
}
