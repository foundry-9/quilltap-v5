//! The chat-scoped outfit dispatch handlers (P4.9f1) — v4
//! `app/api/v1/chats/[id]/actions/outfit.ts` (`handleGetOutfit`,
//! `handleGetOutfitSummary` + `handleEquipSlot`, all SEVEN modes) and
//! `app/api/v1/chats/[id]/actions/regenerate-avatar.ts`.
//!
//! Each handler is a differential port of a v4 route handler and returns a
//! [`Response`] directly; the exact bytes are pinned by
//! `wardrobe_routes_equivalence`. `user_id` is a parameter so the harness can
//! drive with the fixture's own user id; the engine passes `SINGLE_USER_ID`.
//!
//! ## The seven modes (v4's enum order — `equip` is the DEPRECATED seventh)
//!
//! `wear`, `replace`, `equip`, `add_to_slot`, `remove_from_slot`, `clear_slot`,
//! `set_all`. v4's own comment (`outfit.ts:37-39`): "`wear` honors the item's
//! `replace` flag; `replace` force-swaps the slots it covers. `equip` is a
//! deprecated alias for `wear`." The alias is ported as its own enum member —
//! it is NOT collapsed into `wear` (the wire rejects/accepts identically).
//!
//! ## One pool, three put-on modes (v4 `9ecebf986`)
//!
//! The put-on modes (`wear` / its alias `equip`, `replace`, `add_to_slot`) and
//! `set_all` read ONE wearable pool for the character
//! ([`load_wearable_pool`] — own vault, their groups, the chat's project
//! stores, Quilltap General; the operator's project tier is not
//! roster-gated). A put-on resolves its item through [`resolve_wearable`]: a
//! reference outside the pool answers 404 `Wardrobe item not found`; an
//! archived item (bug 191) answers 400 `Item "<title>" is archived and cannot
//! be worn`; `add_to_slot` into a slot the item's `types` do not cover answers
//! 400 `Item "<title>" (types: …) cannot be added to the "<slot>" slot` — each
//! with INFO `[Chats v1] Refused to put on a wardrobe item` `{reason}`. The
//! take-off modes (`remove_from_slot`, `clear_slot`) read nothing but the
//! slots and go straight to [`apply_displacement`].
//!
//! ## Chat-existence looseness (v4-faithful, deliberately)
//!
//! `handleEquipSlot` never loads the chat itself: a mutation against a
//! nonexistent chat still answers 200 with the computed slots (v4's repo
//! `update` is a silent no-op on a missing row, and `setEquippedOutfit`
//! returns the passed slots regardless). Every mode writes through the
//! chokepoint ([`commit_equipped_outfit`]), which — for the same reason —
//! credits that wear too (the tier-2 `route_wear_missing_chat` row). The
//! pool's project tier reads the chat; a missing chat is an empty project
//! tier, never a refusal. The differentials pin this arm — do not "fix" it.
//!
//! ## A FAILED slot write is a 500 (v4 `3ee3b1342`)
//!
//! When the slot write itself fails (a database error — v4's `safeQuery`
//! fallback `null`), the chokepoint THROWS `Failed to save the equipped outfit
//! …`, so v4's outer catch answers 500 `Failed to equip wardrobe slot`
//! (ERROR `[Chats v1] Error equipping wardrobe slot` `{chatId}`) and the
//! outfit-change effects are skipped — no caller reports a change that was not
//! saved.
//!
//! ## `set_all` (v4 `3ee3b1342` + bug 191)
//!
//! Every id must be in the pool (400 `Wardrobe item <id> not available to this
//! character`, no line), and no archived item may be NEWLY put on (400 + INFO
//! `[Chats v1] Refused a fitting that puts on an archived item`) — an archived
//! item already in the character's slots stays. The dialog stages composites
//! as their leaves, so `set_all` accepts the composites the client dissolved
//! as a CLAIM ([`resolve_worn_bundles`]): ids the character cannot reach and
//! items that are not composites are dropped (the `Some claimed worn bundles
//! were not credited` DEBUG), each survivor is expanded to its leaves over the
//! pool, and the ledger credits it only when one of those leaves was newly put
//! on.
//!
//! ## The effects
//!
//! Every success ends in [`notify_wardrobe_changed`] (`[Chats v1]
//! outfit-equip`): the avatar refresh when the chat opted in, then Aurora's
//! announcement, enqueued at once (the job itself is debounced) with its own
//! DEBUG / WARN lines.
//!
//! Every route line is logged on the caller thread from what the writer
//! closure hands back; the pool's `[WearablePool]` and the displacement's
//! `[applyDisplacement]` lines run inside the closure (the keystone's, R-H).
//!
//! ## The equip body parse
//!
//! The route CATCHES its `ZodError` and joins the issue messages with `', '`,
//! so the Zod-4 message bytes (including the three superRefine arms) are wire
//! payload, ported in [`parse_equip_body`]. superRefine runs only when the
//! base object parse succeeds — Zod's refinement ordering, preserved.

use serde_json::{json, Map, Value};

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::runtime::Db;
use crate::db::wardrobe_wear_stats::{EquipSource, WornBundle};
use crate::services::avatar_generation::AvatarGenerationParams;
use crate::services::image_job_common::with_both_conns;
use crate::services::outfit_change_effects::{notify_wardrobe_changed, OutfitChangeContext};
use crate::services::outfit_displacement::apply_displacement;
use crate::services::resolve_equipped::resolve_equipped_outfit_for_character;
use crate::services::wardrobe_wear_commit::{commit_equipped_outfit, CommitEquippedOutfitInput};
use crate::slot_ops::{worn_bundles_for, ComputeDisplacedOptions, DisplacementMode, WearableNode};
use crate::wardrobe::{is_composite, Slots};
use crate::wear_ops::{
    resolve_wearable, wear_item, PutOnMode, WearEffect, WearableRef, WearableRefusal,
    WearableResolution,
};
use crate::wearable::{newly_worn_archived_items, wear_refusal};
use crate::wearable_pool::{
    load_cast_pools, load_wearable_pool, LoadWearablePoolOptions, WearablePool,
};

use super::types::{ErrorKind, Response};

/// The coverage slots, in v4's declaration order — read from the ONE registry
/// (`4423ad10`'s consolidation).
const WARDROBE_SLOT_TYPES: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

/// The seven modes, in v4's enum order (`equip` = the deprecated `wear` alias).
const EQUIP_MODES: [&str; 7] = [
    "wear",
    "replace",
    "equip",
    "add_to_slot",
    "remove_from_slot",
    "clear_slot",
    "set_all",
];

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg.into())
}

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

// ===========================================================================
// GET ?action=outfit
// ===========================================================================

/// v4 `handleGetOutfit` — `{equippedOutfit: getEquippedOutfit(chatId) ?? {}}`.
/// A missing chat and a chat with no stored outfit both answer `{}` (v4's
/// `?? {}` folds them together).
pub fn chat_outfit_get(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let out =
        db.read_main(move |conn| Ok(ChatOutfitsRepository::new(conn).get_equipped_outfit(&cid)));
    match out {
        Ok(state) => Response::ChatOutfit(json!({
            "equippedOutfit": state.unwrap_or_else(|| Value::Object(Map::new()))
        })),
        // v4 catches → serverError with this exact message.
        Err(_) => internal("Failed to fetch equipped outfit"),
    }
}

// ===========================================================================
// GET ?action=outfit-summary (P4.9E3B — v4 handleGetOutfitSummary, outfit.ts:106)
// ===========================================================================

/// v4 `handleGetOutfitSummary` (`outfit.ts:112-151`) — per-character equipped
/// outfit with resolved item titles. Each character's slots resolve against
/// THEIR OWN wearable pool ([`load_cast_pools`] — own vault, own groups, with
/// the chat project's stores and Quilltap General read once for the cast; the
/// operator is reading, so the project roster does not apply) by the canonical
/// rule ([`resolve_equipped_outfit_for_character`]): composites expanded, each
/// leaf routed into EVERY slot its own `types` cover, deduped across the whole
/// equipped set. An id outside the character's own pool is dropped rather than
/// borrowing another participant's item. Shape:
/// `{ summary: { [characterId]: { [slot]: [{itemId, title}, …] } } }`, the
/// characters in the stored `equippedOutfit` object's own key order (v4
/// iterates its keys, not the participant roster); DEBUG `[Chats v1] Built
/// outfit summary`.
pub fn chat_outfit_summary(db: &Db, chat_id: &str) -> Response {
    // v4 `repos.chats.findById` is a FALLBACK read: a failed read logs the
    // repository's `Error finding entity by ID` and answers `null` — the 404,
    // never the route's catch.
    let lookup = chat_id.to_string();
    let chat = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &lookup))
    });
    let Some(chat) = chat else {
        return not_found("Chat");
    };
    let cid = chat_id.to_string();
    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let equipped = ChatOutfitsRepository::new(main)
                .get_equipped_outfit(&cid)
                .unwrap_or_else(|| Value::Object(Map::new()));
            let equipped_obj = equipped.as_object().cloned().unwrap_or_default();
            let character_ids: Vec<String> = equipped_obj.keys().cloned().collect();
            // ALWAYS called — an empty cast still reads the project + General
            // tiers once (v4 `loadCastPools` invokes the shared loader).
            let pools = load_cast_pools(
                main,
                mount,
                chat.get("projectId").and_then(Value::as_str),
                &character_ids,
            );

            let mut summary = Map::new();
            for (character_id, slots) in &equipped_obj {
                let pool = pools
                    .iter()
                    .find(|(id, _)| id == character_id)
                    .map(|(_, pool)| pool);
                let mut slot_map = Map::new();
                match pool.filter(|_| !slots.is_null()) {
                    None => {
                        for slot_key in WARDROBE_SLOT_TYPES {
                            slot_map.insert(slot_key.to_string(), Value::Array(Vec::new()));
                        }
                    }
                    Some(pool) => {
                        let resolved = resolve_equipped_outfit_for_character(
                            pool,
                            &Slots::from_value(Some(slots)),
                        );
                        for slot_key in WARDROBE_SLOT_TYPES {
                            let entries: Vec<Value> = resolved
                                .leaf_items_by_slot
                                .slot(slot_key)
                                .iter()
                                .map(|leaf| {
                                    json!({
                                        "itemId": leaf.get("id").cloned().unwrap_or(Value::Null),
                                        "title": leaf.get("title").cloned().unwrap_or(Value::Null),
                                    })
                                })
                                .collect();
                            slot_map.insert(slot_key.to_string(), Value::Array(entries));
                        }
                    }
                }
                summary.insert(character_id.clone(), Value::Object(slot_map));
            }

            tracing::debug!(
                chatId = cid.as_str(),
                characterCount = character_ids.len(),
                context = "wardrobe",
                "[Chats v1] Built outfit summary"
            );
            Ok(Response::ChatDialog(json!({ "summary": summary })))
        })
    });
    match out {
        Ok(r) => r,
        // v4 catches → ERROR + serverError with this exact message. Every
        // read above is a fallback, so only an unopenable partition (the
        // mount index missing) reaches here.
        Err(e) => {
            tracing::error!(
                chatId = chat_id,
                error = %crate::db::fallback::error_text(&e),
                "[Chats v1] Error fetching equipped outfit summary"
            );
            internal("Failed to fetch equipped outfit summary")
        }
    }
}

// ===========================================================================
// POST ?action=equip — the body parse (v4 equipBodySchema, message-faithful)
// ===========================================================================

/// The parsed equip body (v4 `equipBodySchema.parse` output).
#[derive(Debug)]
struct EquipBody {
    character_id: String,
    mode: String,
    slot: Option<String>,
    item_id: Option<String>,
    /// The Zod-parsed `slots` (all four keys materialized, defaults applied,
    /// unknown keys stripped) — present only when the body carried `slots`.
    slots: Option<Value>,
    /// `wornBundleIds: z.array(z.string().min(1)).optional()` (v4
    /// `3ee3b1342`) — read by `set_all` only.
    worn_bundle_ids: Option<Vec<String>>,
}

/// Zod type-name for the `received …` clause of a Zod-4 message.
pub(crate) fn received(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(_)) => "string".to_string(),
        Some(Value::Bool(_)) => "boolean".to_string(),
        Some(Value::Number(_)) => "number".to_string(),
        Some(Value::Array(_)) => "array".to_string(),
        Some(Value::Object(_)) => "object".to_string(),
    }
}

/// Zod 4's UUID format check (`z.uuid()`) — a delegation to the ONE home,
/// [`zod_uuid_ok`](crate::api::zod_issues::zod_uuid_ok) (P4.155, R-D: this was
/// a full hand copy, measured identical to the home over 1,291 inputs before
/// the fold). Kept as a name because ~20 callers import it from here.
pub(crate) fn is_zod_uuid(s: &str) -> bool {
    crate::api::zod_issues::zod_uuid_ok(s)
}

/// The enum-issue message. Zod 4's default for `z.enum` carries NO
/// `received …` clause (oracle-confirmed: `eq_zod_bad_mode`).
fn enum_issue(options: &[&str]) -> String {
    let expected = options
        .iter()
        .map(|o| format!("\"{o}\""))
        .collect::<Vec<_>>()
        .join("|");
    format!("Invalid option: expected one of {expected}")
}

/// Parse a whole `EquippedSlotsSchema` object body: the four slot keys in
/// schema order, `.default([])` applied, unknown keys stripped. Shared by the
/// equip / regenerate-avatar / preview-avatar parses.
pub(crate) fn parse_slots_object(s: &Map<String, Value>, issues: &mut Vec<String>) -> Value {
    let mut parsed = Map::new();
    for key in WARDROBE_SLOT_TYPES {
        let arr = parse_slot_array(s, key, issues);
        parsed.insert(key.to_string(), arr);
    }
    Value::Object(parsed)
}

/// Parse one `EquippedSlotsSchema` slot key: absent → `.default([])`; present
/// must be an array of Zod UUIDs. Pushes Zod-4 messages into `issues`.
fn parse_slot_array(slots: &Map<String, Value>, key: &str, issues: &mut Vec<String>) -> Value {
    match slots.get(key) {
        None => Value::Array(Vec::new()),
        Some(Value::Array(a)) => {
            let mut ok = true;
            for v in a {
                match v.as_str() {
                    Some(s) if is_zod_uuid(s) => {}
                    Some(_) => {
                        issues.push("Invalid UUID".to_string());
                        ok = false;
                    }
                    None => {
                        issues.push(format!(
                            "Invalid input: expected string, received {}",
                            received(Some(v))
                        ));
                        ok = false;
                    }
                }
            }
            if ok {
                Value::Array(a.clone())
            } else {
                Value::Array(Vec::new())
            }
        }
        v => {
            issues.push(format!(
                "Invalid input: expected array, received {}",
                received(v)
            ));
            Value::Array(Vec::new())
        }
    }
}

/// Message-faithful port of v4 `equipBodySchema` (`outfit.ts:35-79`). On
/// failure returns the issue messages joined with `', '` (the route's own
/// catch). Base issues collect in shape order (`characterId`, `mode`, `slot`,
/// `itemId`, `slots`); the three superRefine arms run ONLY when the base parse
/// succeeded, in v4's refine order (`slot`, `itemId`, `slots`).
fn parse_equip_body(body: &Value) -> Result<EquipBody, String> {
    let obj = body.as_object();
    // A non-object root is ONE root issue in Zod, not per-field issues.
    if obj.is_none() {
        return Err(format!(
            "Invalid input: expected object, received {}",
            received(Some(body))
        ));
    }
    let mut issues: Vec<String> = Vec::new();
    let get = |key: &str| obj.and_then(|o| o.get(key));

    // characterId: z.string().min(1, 'characterId is required')
    let character_id = match get("characterId") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push("characterId is required".to_string());
            None
        }
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // mode: the seven-member enum.
    let mode = match get("mode") {
        Some(Value::String(s)) if EQUIP_MODES.contains(&s.as_str()) => Some(s.clone()),
        _ => {
            issues.push(enum_issue(&EQUIP_MODES));
            None
        }
    };

    // slot: the four-member enum, optional.
    let slot = match get("slot") {
        None => None,
        Some(Value::String(s)) if WARDROBE_SLOT_TYPES.contains(&s.as_str()) => Some(s.clone()),
        _ => {
            issues.push(enum_issue(&WARDROBE_SLOT_TYPES));
            None
        }
    };

    // itemId: z.string().nullable().optional() — null and absent both fold to None.
    let item_id = match get("itemId") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // slots: EquippedSlotsSchema.optional() — four UUID arrays with `.default([])`,
    // unknown keys stripped, result keys in schema order.
    let slots = match get("slots") {
        None => None,
        Some(Value::Object(s)) => Some(parse_slots_object(s, &mut issues)),
        v => {
            issues.push(format!(
                "Invalid input: expected object, received {}",
                received(v)
            ));
            None
        }
    };

    // wornBundleIds: z.array(z.string().min(1)).optional() — every bad element
    // is its own issue (Zod 4 keeps walking the array).
    let worn_bundle_ids = match get("wornBundleIds") {
        None => None,
        Some(Value::Array(a)) => {
            let mut ids = Vec::with_capacity(a.len());
            for v in a {
                match v {
                    Value::String(s) if !s.is_empty() => ids.push(s.clone()),
                    Value::String(_) => {
                        issues.push("Too small: expected string to have >=1 characters".to_string())
                    }
                    other => issues.push(format!(
                        "Invalid input: expected string, received {}",
                        received(Some(other))
                    )),
                }
            }
            Some(ids)
        }
        v => {
            issues.push(format!(
                "Invalid input: expected array, received {}",
                received(v)
            ));
            None
        }
    };

    if !issues.is_empty() {
        return Err(issues.join(", "));
    }
    let (character_id, mode) = (character_id.unwrap(), mode.unwrap());

    // The three superRefine arms (base parse succeeded), in v4's order.
    let mut refine: Vec<String> = Vec::new();
    if matches!(
        mode.as_str(),
        "add_to_slot" | "remove_from_slot" | "clear_slot"
    ) && slot.is_none()
    {
        refine.push(format!("slot is required for mode \"{mode}\""));
    }
    // v4 `!value.itemId` — null/absent/empty-string are all falsy.
    let item_id_falsy = item_id.as_deref().is_none_or(str::is_empty);
    if matches!(mode.as_str(), "wear" | "replace" | "equip" | "add_to_slot") && item_id_falsy {
        refine.push(format!("itemId is required for mode \"{mode}\""));
    }
    if mode == "set_all" && slots.is_none() {
        refine.push("slots is required for mode \"set_all\"".to_string());
    }
    if !refine.is_empty() {
        return Err(refine.join(", "));
    }

    Ok(EquipBody {
        character_id,
        mode,
        slot,
        item_id,
        slots,
        worn_bundle_ids,
    })
}

/// What [`resolve_worn_bundles`] resolved, for its DEBUG line (logged by the
/// caller on its own thread — the resolution runs inside the writer closure,
/// where a line would be invisible to the capture rig).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WornBundleClaim {
    claimed: usize,
    resolved: usize,
}

/// v4 `resolveWornBundles(pool, wornBundleIds)` (`outfit.ts:159-175`) — turn a
/// `set_all` request's `wornBundleIds` into the ledger's composite credit. Ids
/// the character cannot reach, and items that are not composites, are dropped
/// (a client cannot credit a garment it cannot see); each survivor is expanded
/// to its leaves over the pool's archived-inclusive `by_id` — no read of its
/// own. The claim counts ride back for v4's DEBUG `[Chats v1] Some claimed
/// worn bundles were not credited`, which the caller logs when they differ.
fn resolve_worn_bundles(
    pool: &WearablePool,
    worn_bundle_ids: &[String],
) -> (Vec<WornBundle>, WornBundleClaim) {
    // `Array.from(new Set(ids))` — first-seen order.
    let mut ids: Vec<String> = Vec::new();
    for id in worn_bundle_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    if ids.is_empty() {
        let none = WornBundleClaim {
            claimed: 0,
            resolved: 0,
        };
        return (Vec::new(), none);
    }
    let result: Vec<WornBundle> = pool
        .get_many(&ids)
        .into_iter()
        .map(WearableNode::from_value)
        .filter(|node| is_composite(&node.component_item_ids))
        .flat_map(|node| worn_bundles_for(&node, Some(pool.by_id()), None))
        .collect();
    let claim = WornBundleClaim {
        claimed: ids.len(),
        resolved: result.len(),
    };
    (result, claim)
}

// ===========================================================================
// POST ?action=equip — the handler
// ===========================================================================

/// The mode-dispatch result computed inside the write closure. Every line the
/// route logs is carried out of the closure and logged on the caller thread.
enum EquipOutcome {
    /// `remove_from_slot` / `clear_slot` — INFO `Wardrobe slot changed`.
    SlotChanged { slots: Value },
    /// `wear` / `equip` / `replace` / `add_to_slot` — INFO `Wardrobe item put
    /// on`.
    PutOn {
        slots: Value,
        mode: PutOnMode,
        item_id: String,
        outcome_effect: WearEffect,
        slots_affected: Vec<String>,
    },
    /// `set_all` — INFO `Equipped outfit replaced (set_all)`.
    SetAll {
        slots: Value,
        claim: WornBundleClaim,
    },
    /// A put-on mode the pool refused — INFO `Refused to put on a wardrobe
    /// item` with the reason.
    RefusedPutOn {
        reason: WearableRefusal,
        response: Response,
    },
    /// A fitting that puts an archived item on — INFO `Refused a fitting that
    /// puts on an archived item`.
    RefusedFitting {
        item_ids: Vec<String>,
        response: Response,
    },
    /// `set_all` naming an id outside the pool — 400, no line (v4 never logged
    /// it).
    Refused(Response),
}

/// v4 `setAll(ctx, chatId, pool, slots, wornBundleIds)` (`outfit.ts:178-215`)
/// — commit a whole fitting at once (the dialog's Done / "Wear this fitting").
/// Every id must be something this character can reach, and none may be an
/// archived item being NEWLY put on: an archived garment already in one of the
/// character's slots stays (archiving never undresses anyone).
fn set_all(
    main: &rusqlite::Connection,
    chat_id: &str,
    pool: &WearablePool,
    slots_value: &Value,
    worn_bundle_ids: &[String],
) -> Result<EquipOutcome, crate::db::DbError> {
    let character_id = pool.character_id.as_str();
    // `Array.from(new Set(WARDROBE_SLOT_TYPES.flatMap((key) => slots[key])))`.
    let mut all_ids: Vec<String> = Vec::new();
    for key in WARDROBE_SLOT_TYPES {
        for v in slots_value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(s) = v.as_str() {
                if !all_ids.iter().any(|x| x == s) {
                    all_ids.push(s.to_string());
                }
            }
        }
    }
    let found: Vec<Value> = pool.get_many(&all_ids).into_iter().cloned().collect();
    if found.len() != all_ids.len() {
        let found_ids: Vec<&str> = found
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str))
            .collect();
        let missing = all_ids
            .iter()
            .find(|id| !found_ids.contains(&id.as_str()))
            .map(String::as_str)
            .unwrap_or_default();
        return Ok(EquipOutcome::Refused(bad_request(format!(
            "Wardrobe item {missing} not available to this character"
        ))));
    }
    let current = ChatOutfitsRepository::new(main)
        .get_equipped_outfit_for_character(chat_id, character_id)
        .map(|v| Slots::from_value(Some(&v)));
    let archived = newly_worn_archived_items(&found, current.as_ref());
    if let Some(first) = archived.first() {
        let response = bad_request(wear_refusal(first).unwrap_or_default());
        return Ok(EquipOutcome::RefusedFitting {
            item_ids: archived
                .iter()
                .filter_map(|i| i.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect(),
            response,
        });
    }
    let (worn_bundles, claim) = resolve_worn_bundles(pool, worn_bundle_ids);
    // Through the chokepoint: a lost write throws to the outer catch (500, no
    // effects).
    commit_equipped_outfit(
        main,
        CommitEquippedOutfitInput {
            chat_id,
            character_id,
            next_slots: &Slots::from_value(Some(slots_value)),
            worn_bundles: &worn_bundles,
            source: EquipSource::Ui,
            at: None,
        },
    )?;
    Ok(EquipOutcome::SetAll {
        slots: slots_value.clone(),
        claim,
    })
}

/// v4 `handleEquipSlot` (`outfit.ts:224-283`) — parse, then:
///
/// - the take-off modes (`remove_from_slot`, `clear_slot`) go straight to
///   [`apply_displacement`] — no tiers, no pool, only the character's slots;
/// - every other mode loads ONE wearable pool for the character, with the
///   operator's project tier (the operator is dressing the character, so the
///   project roster does not apply): `set_all` checks the fitting against it;
///   the three put-on modes resolve the item through [`resolve_wearable`]
///   (a reference outside the pool → 404, an archived item or a slot the item
///   does not cover → 400) and put it on through [`wear_item`];
///
/// then the outfit-change effects ([`notify_wardrobe_changed`] — the avatar
/// refresh, then Aurora's announcement, enqueued at once) and
/// `{equippedSlots}`.
pub async fn chat_equip(db: &Db, user_id: &str, chat_id: &str, body: Value) -> Response {
    let parsed = match parse_equip_body(&body) {
        Ok(p) => p,
        Err(joined) => return bad_request(joined),
    };

    let cid = chat_id.to_string();
    let character_id = parsed.character_id.clone();
    let mode = parsed.mode.clone();

    let out = with_both_conns(db, move |main, mount| {
        let slot = parsed.slot.as_deref();
        if matches!(parsed.mode.as_str(), "remove_from_slot" | "clear_slot") {
            let take_off = if parsed.mode == "remove_from_slot" {
                DisplacementMode::RemoveFromSlot
            } else {
                DisplacementMode::ClearSlot
            };
            let slots = apply_displacement(
                main,
                &cid,
                &parsed.character_id,
                &ComputeDisplacedOptions {
                    mode: take_off,
                    item: None,
                    slot,
                    // v4 `mode === 'remove_from_slot' ? itemId ?? undefined :
                    // undefined` — an empty string is a VALUE here (only the
                    // refine arms treat '' as falsy, and take-off has none).
                    item_id: if take_off == DisplacementMode::RemoveFromSlot {
                        parsed.item_id.as_deref()
                    } else {
                        None
                    },
                    items_by_id: None,
                },
                EquipSource::Ui,
            )?;
            return Ok(EquipOutcome::SlotChanged {
                slots: slots.to_value(),
            });
        }

        let pool = load_wearable_pool(
            main,
            mount,
            &parsed.character_id,
            None,
            LoadWearablePoolOptions {
                operator: true,
                chat_id: Some(&cid),
                ..LoadWearablePoolOptions::default()
            },
        );
        if parsed.mode == "set_all" {
            let slots_value = parsed.slots.as_ref().expect("schema-guaranteed");
            return set_all(
                main,
                &cid,
                &pool,
                slots_value,
                parsed.worn_bundle_ids.as_deref().unwrap_or_default(),
            );
        }

        // `equip` is the deprecated alias for `wear`.
        let put_on = if parsed.mode == "equip" {
            PutOnMode::Wear
        } else {
            PutOnMode::parse(&parsed.mode).expect("schema-guaranteed put-on mode")
        };
        let reference = WearableRef {
            item_id: parsed.item_id.as_deref(),
            item_title: None,
        };
        let item = match resolve_wearable(&pool, reference, put_on, slot) {
            WearableResolution::Ok(item) => item.clone(),
            WearableResolution::Refused { reason, message } => {
                let response = if reason == WearableRefusal::NotFound {
                    not_found("Wardrobe item")
                } else {
                    bad_request(message)
                };
                return Ok(EquipOutcome::RefusedPutOn { reason, response });
            }
        };
        let outcome = wear_item(main, &cid, &pool, &item, put_on, slot, EquipSource::Ui)?;
        Ok(EquipOutcome::PutOn {
            slots: outcome.slots.to_value(),
            mode: put_on,
            item_id: item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            outcome_effect: outcome.effect,
            slots_affected: outcome.slots_affected,
        })
    })
    .await;

    let updated = match out {
        Ok(EquipOutcome::SlotChanged { slots }) => {
            let slot = body.get("slot").and_then(Value::as_str).unwrap_or_default();
            // v4 `itemId ?? null` — the BODY's id, even for `clear_slot`.
            let item_id = body.get("itemId").and_then(Value::as_str).unwrap_or("null");
            tracing::info!(
                chatId = chat_id,
                characterId = character_id.as_str(),
                mode = mode.as_str(),
                slot = slot,
                itemId = item_id,
                context = "wardrobe",
                "[Chats v1] Wardrobe slot changed"
            );
            slots
        }
        Ok(EquipOutcome::PutOn {
            slots,
            mode: put_on,
            item_id,
            outcome_effect,
            slots_affected,
        }) => {
            let slots_affected = Value::from(slots_affected).to_string();
            tracing::info!(
                chatId = chat_id,
                characterId = character_id.as_str(),
                mode = put_on.as_str(),
                itemId = item_id.as_str(),
                slotsAffectedJson = slots_affected.as_str(),
                effect = outcome_effect.as_str(),
                context = "wardrobe",
                "[Chats v1] Wardrobe item put on"
            );
            slots
        }
        Ok(EquipOutcome::SetAll { slots, claim }) => {
            if claim.resolved != claim.claimed {
                tracing::debug!(
                    characterId = character_id.as_str(),
                    claimed = claim.claimed,
                    resolved = claim.resolved,
                    context = "wardrobe",
                    "[Chats v1] Some claimed worn bundles were not credited"
                );
            }
            // `wornBundleCount` is the credit CLAIM that survived resolution
            // (v4 `wornBundles.length`).
            tracing::info!(
                chatId = chat_id,
                characterId = character_id.as_str(),
                wornBundleCount = claim.resolved,
                context = "wardrobe",
                "[Chats v1] Equipped outfit replaced (set_all)"
            );
            slots
        }
        Ok(EquipOutcome::RefusedPutOn { reason, response }) => {
            let item_id = body
                .get("itemId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tracing::info!(
                chatId = chat_id,
                characterId = character_id.as_str(),
                // The RAW body mode — `equip` stays `equip` here.
                mode = mode.as_str(),
                itemId = item_id,
                reason = reason.as_str(),
                context = "wardrobe",
                "[Chats v1] Refused to put on a wardrobe item"
            );
            return response;
        }
        Ok(EquipOutcome::RefusedFitting { item_ids, response }) => {
            let item_ids = Value::from(item_ids).to_string();
            tracing::info!(
                chatId = chat_id,
                characterId = character_id.as_str(),
                itemIdsJson = item_ids.as_str(),
                context = "wardrobe",
                "[Chats v1] Refused a fitting that puts on an archived item"
            );
            return response;
        }
        Ok(EquipOutcome::Refused(r)) => return r,
        // The route's outer catch — a lost slot write (the chokepoint's
        // `Failed to save the equipped outfit …`), a failed read, a failed
        // credit — skips the effects.
        Err(e) => {
            tracing::error!(
                chatId = chat_id,
                error = %crate::db::fallback::error_text(&e),
                "[Chats v1] Error equipping wardrobe slot"
            );
            return internal("Failed to equip wardrobe slot");
        }
    };

    notify_wardrobe_changed(
        db,
        &OutfitChangeContext {
            user_id,
            chat_id,
            character_id: &character_id,
            pending_wardrobe_announcements: None,
        },
        "[Chats v1] outfit-equip",
    )
    .await;

    Response::ChatOutfit(json!({ "equippedSlots": updated }))
}

// ===========================================================================
// POST ?action=regenerate-avatar (tier 2)
// ===========================================================================

/// The parsed regenerate body (v4 `regenerateAvatarSchema`).
struct RegenerateBody {
    character_id: String,
    image_profile_id: Option<String>,
    equipped_slots: Option<Value>,
}

/// Message-faithful port of v4 `regenerateAvatarSchema`
/// (`regenerate-avatar.ts:15-26`) — the route catches its ZodError and joins
/// with `', '`. Shape order: `characterId`, `imageProfileId`, `equippedSlots`.
fn parse_regenerate_body(body: &Value) -> Result<RegenerateBody, String> {
    let obj = body.as_object();
    if obj.is_none() {
        return Err(format!(
            "Invalid input: expected object, received {}",
            received(Some(body))
        ));
    }
    let mut issues: Vec<String> = Vec::new();
    let get = |key: &str| obj.and_then(|o| o.get(key));

    // characterId: z.string().min(1, 'characterId is required')
    let character_id = match get("characterId") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push("characterId is required".to_string());
            None
        }
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // imageProfileId: z.string().min(1).optional() — the default too-small text.
    let image_profile_id = match get("imageProfileId") {
        None => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push("Too small: expected string to have >=1 characters".to_string());
            None
        }
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // equippedSlots: EquippedSlotsSchema.optional().
    let equipped_slots = match get("equippedSlots") {
        None => None,
        Some(Value::Object(s)) => Some(parse_slots_object(s, &mut issues)),
        v => {
            issues.push(format!(
                "Invalid input: expected object, received {}",
                received(v)
            ));
            None
        }
    };

    if !issues.is_empty() {
        return Err(issues.join(", "));
    }
    Ok(RegenerateBody {
        character_id: character_id.unwrap(),
        image_profile_id,
        equipped_slots,
    })
}

/// v4 `handleRegenerateAvatar` — participant-gate, then the UNCONDITIONAL
/// [`crate::services::avatar_generation::trigger_avatar_generation`] (the
/// chat-level `avatarGenerationEnabled` toggle governs *automatic* regeneration
/// only — manual clicks always fire), with the fitting-room `equippedSlots`
/// riding as a one-shot override. Success is `successResponse({message,
/// queued})` (RAW at the edge).
pub async fn chat_regenerate_avatar(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    body: Value,
) -> Response {
    let parsed = match parse_regenerate_body(&body) {
        Ok(p) => p,
        Err(joined) => return bad_request(joined),
    };

    // Verify the character is a participant (v4: findById → 'Chat not found';
    // participants.some → the participant message).
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid)) {
        Ok(c) => c,
        // The route's outer catch.
        Err(_) => return internal("Failed to queue avatar regeneration"),
    };
    let Some(chat) = chat else {
        return bad_request("Chat not found");
    };
    let is_participant = chat
        .get("participants")
        .and_then(Value::as_array)
        .is_some_and(|ps| {
            ps.iter().any(|p| {
                p.get("characterId").and_then(Value::as_str) == Some(parsed.character_id.as_str())
            })
        });
    if !is_participant {
        return bad_request("Character is not a participant in this chat.");
    }

    let result = crate::services::avatar_generation::trigger_avatar_generation(
        db,
        &AvatarGenerationParams {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
            character_id: parsed.character_id.clone(),
            // P4.D238 MARKED HUNK (the `caller_context` rider) — v4
            // `regenerate-avatar.ts:62`.
            caller_context: "[Chats v1] regenerate-avatar",
            image_profile_id_override: parsed.image_profile_id.clone(),
            equipped_slots_override: parsed.equipped_slots.clone(),
            // A manual click is a reroll: bypass the configuration cache and
            // rebind the key, so the new portrait becomes the canonical one for
            // this character in this outfit. v4's ONLY `force` setter.
            force: true,
        },
    )
    .await;

    match result {
        crate::services::avatar_generation::AvatarGenerationResult::Queued => {
            Response::ChatOutfit(json!({
                "message": "Avatar regeneration queued",
                "queued": true,
            }))
        }
        crate::services::avatar_generation::AvatarGenerationResult::NotQueued {
            message, ..
        } => bad_request(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zod_uuid_accepts_v4_and_rejects_bad_variant() {
        assert!(is_zod_uuid("a1000000-0000-4000-8000-000000000001"));
        assert!(is_zod_uuid("00000000-0000-0000-0000-000000000000")); // nil
        assert!(!is_zod_uuid("a1000000-0000-4000-c000-000000000001")); // variant c
        assert!(!is_zod_uuid("a1000000-0000-9000-8000-000000000001")); // version 9
        assert!(!is_zod_uuid("not-a-uuid"));
    }

    #[test]
    fn equip_parse_deprecated_alias_and_refines() {
        // `equip` (the deprecated seventh mode) parses and demands an itemId.
        let err = parse_equip_body(&serde_json::json!({
            "characterId": "c", "mode": "equip"
        }))
        .unwrap_err();
        assert_eq!(err, "itemId is required for mode \"equip\"");

        // Base-parse issues suppress the refines (Zod's ordering).
        let err = parse_equip_body(&serde_json::json!({ "mode": "set_all" })).unwrap_err();
        assert_eq!(err, "Invalid input: expected string, received undefined");
    }

    #[test]
    fn equip_parse_slots_defaults_in_schema_order() {
        let ok = parse_equip_body(&serde_json::json!({
            "characterId": "c", "mode": "set_all",
            "slots": { "bottom": ["a1000000-0000-4000-8000-000000000001"], "extra": 1 }
        }))
        .unwrap();
        let slots = ok.slots.unwrap();
        let keys: Vec<&String> = slots.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["top", "bottom", "footwear", "accessories", "hair"]);
    }
}
