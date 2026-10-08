//! The chat-scoped outfit dispatch handlers (P4.9f1) — v4
//! `app/api/v1/chats/[id]/actions/outfit.ts` (`handleGetOutfit` +
//! `handleEquipSlot`, all SEVEN modes) and
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
//! ## Chat-existence looseness (v4-faithful, deliberately)
//!
//! `handleEquipSlot` never loads the chat: a mutation against a nonexistent
//! chat still answers 200 with the computed slots (v4's repo `update` is a
//! silent no-op on a missing row, and `setEquippedOutfit` returns the passed
//! slots regardless). Since the wear ledger (v4 `3ee3b1342`) every mode writes
//! through the chokepoint ([`commit_equipped_outfit`]), which — for the same
//! reason — credits that wear too (measured at `f5e953a3f`: the tier-2
//! `route_wear_missing_chat` row). The differentials pin this arm — do not
//! "fix" it.
//!
//! ## A FAILED slot write is a 500 (v4 `3ee3b1342`)
//!
//! When the slot write itself fails (a database error — v4's `safeQuery`
//! fallback `null`), the chokepoint THROWS `Failed to save the equipped outfit
//! …`, so v4's outer catch answers 500 `Failed to equip wardrobe slot`
//! (ERROR `[Chats v1] Error equipping wardrobe slot` `{chatId}`) and the avatar
//! trigger + announcement are skipped — no caller reports a change that was
//! not saved. The old `Failed to update equipped slot` arm is unreachable in
//! v4 now and is not ported.
//!
//! ## `set_all`'s `wornBundleIds` (v4 `3ee3b1342`)
//!
//! The dialog stages bundles as their leaves, so the stored slots alone cannot
//! say an outfit was put on. `set_all` accepts the bundles the client
//! dissolved as a CLAIM ([`resolve_worn_bundles`]): ids the character cannot
//! reach and items that are not bundles are dropped (the `Some claimed worn
//! bundles were not credited` DEBUG), each survivor is expanded to its leaves
//! server-side, and the ledger credits it only when one of those leaves was
//! newly put on.
//!
//! ## The equip body parse
//!
//! The route CATCHES its `ZodError` and joins the issue messages with `', '`,
//! so the Zod-4 message bytes (including the three superRefine arms) are wire
//! payload, ported in [`parse_equip_body`]. superRefine runs only when the
//! base object parse succeeds — Zod's refinement ordering, preserved.

use serde_json::{json, Map, Value};

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::runtime::Db;
use crate::db::wardrobe_read;
use crate::db::wardrobe_wear_stats::{EquipSource, WornBundle};
use crate::dissolve_bundles::WearableNode;
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::image_job_common::with_both_conns;
use crate::services::queue_service::enqueue_wardrobe_outfit_announcement;
use crate::services::wardrobe_wear_commit::{commit_equipped_outfit, CommitEquippedOutfitInput};
use crate::tools::wardrobe_shared::{
    add_to_slot, equip_item, lookup_for_bundle, remove_from_slot, replace_item, worn_bundles_for,
};
use crate::wardrobe::Slots;
use crate::wardrobe_tiers::{
    resolve_shared_wardrobe_tiers_for_chat, SharedWardrobeTierOptions, SharedWardrobeTiers,
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

/// v4 `handleGetOutfitSummary` — per-character equipped outfit with resolved
/// item titles. Each slot is an array of `{itemId, title}` (composites are
/// expanded to their leaves before mapping; a leaf is projected only into slots
/// its own `types` cover; an unresolvable id is silently dropped). Shape:
/// `{ summary: { [characterId]: { [slot]: [{itemId, title}, …] } } }`.
///
/// The item pool seeds the shared archetype tier (Quilltap General + the chat
/// project's stores, `includeArchived: true`) and then layers each OUTFIT-KEYED
/// character's own vault wardrobe on top — v4 iterates the `equippedOutfit`
/// object's own keys, not the participant roster.
pub fn chat_outfit_summary(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = crate::db::chats_read::find_by_id(main, &cid)? else {
                return Ok(Err(not_found("Chat")));
            };
            let equipped = ChatOutfitsRepository::new(main)
                .get_equipped_outfit(&cid)
                .unwrap_or_else(|| Value::Object(Map::new()));
            let equipped_obj = equipped.as_object().cloned().unwrap_or_default();

            // Collect every itemId across all characters/slots, then bulk-resolve.
            let mut all_item_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for slots in equipped_obj.values() {
                if slots.is_null() {
                    continue;
                }
                for slot_key in WARDROBE_SLOT_TYPES {
                    if let Some(ids) = slots.get(slot_key).and_then(Value::as_array) {
                        for id in ids {
                            if let Some(id) = id.as_str().filter(|s| !s.is_empty()) {
                                all_item_ids.insert(id.to_string());
                            }
                        }
                    }
                }
            }

            let docs = DocMountDocumentsRepository::new(mount);
            let mut items_by_id: std::collections::HashMap<String, Value> =
                std::collections::HashMap::new();
            if !all_item_ids.is_empty() {
                // v4 `resolveProjectMountPointIds(chat.projectId)` — [] for a
                // project-less chat or any lookup failure.
                let project_mount_point_ids: Vec<String> = match chat
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    Some(pid) => {
                        crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(
                            mount,
                        )
                        .find_by_project_id(pid)
                        .unwrap_or_default()
                    }
                    None => Vec::new(),
                };
                // This summary spans the whole cast, so the group tier is the
                // *union* of the participants' memberships — unlike the
                // per-character equip paths below, where each character sees
                // only their own groups (v4 `8600c83f`, the one documented
                // cast-spanning exception).
                let mut group_mount_point_ids: Vec<String> = Vec::new();
                for character_id in equipped_obj.keys() {
                    for id in
                        crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character(
                            main,
                            mount,
                            character_id,
                        )
                    {
                        if !group_mount_point_ids.contains(&id) {
                            group_mount_point_ids.push(id);
                        }
                    }
                }
                let summary_tiers = SharedWardrobeTiers {
                    group_mount_point_ids,
                    project_mount_point_ids,
                };
                for arche in crate::db::archetype_wardrobe::find_archetypes(
                    main,
                    &docs,
                    true,
                    &summary_tiers,
                )? {
                    if let Some(id) = arche.get("id").and_then(Value::as_str) {
                        items_by_id.insert(id.to_string(), arche.clone());
                    }
                }
                for character_id in equipped_obj.keys() {
                    for item in
                        wardrobe_read::find_by_character_id(main, &docs, character_id, true)?
                    {
                        if let Some(id) = item.get("id").and_then(Value::as_str) {
                            items_by_id.insert(id.to_string(), item.clone());
                        }
                    }
                }
            }

            let mut summary = Map::new();
            for (character_id, slots) in &equipped_obj {
                let mut slot_map = Map::new();
                for slot_key in WARDROBE_SLOT_TYPES {
                    slot_map.insert(slot_key.to_string(), Value::Array(Vec::new()));
                }
                if !slots.is_null() {
                    for slot_key in WARDROBE_SLOT_TYPES {
                        let equipped_ids: Vec<String> = slots
                            .get(slot_key)
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if equipped_ids.is_empty() {
                            continue;
                        }
                        let expansion =
                            crate::wardrobe::expand_composites(&equipped_ids, &items_by_id, None);
                        let mut seen: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        let mut entries: Vec<Value> = Vec::new();
                        for leaf_id in &expansion.leaf_ids {
                            if seen.contains(leaf_id) {
                                continue;
                            }
                            let Some(leaf) = items_by_id.get(leaf_id) else {
                                continue;
                            };
                            // Only project the leaf into slots its own types cover.
                            let covers = leaf
                                .get("types")
                                .and_then(Value::as_array)
                                .is_some_and(|a| a.iter().any(|t| t.as_str() == Some(slot_key)));
                            if !covers {
                                continue;
                            }
                            entries.push(json!({
                                "itemId": leaf.get("id").cloned().unwrap_or(Value::Null),
                                "title": leaf.get("title").cloned().unwrap_or(Value::Null),
                            }));
                            seen.insert(leaf_id.clone());
                        }
                        slot_map.insert(slot_key.to_string(), Value::Array(entries));
                    }
                }
                summary.insert(character_id.clone(), Value::Object(slot_map));
            }

            Ok(Ok(Response::ChatDialog(json!({ "summary": summary }))))
        })
    });
    match out {
        Ok(Ok(r)) => r,
        Ok(Err(r)) => r,
        // v4 catches → serverError with this exact message.
        Err(_) => internal("Failed to fetch equipped outfit summary"),
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

/// v4 `resolveWornBundles` (`outfit.ts:220-243`) — turn a `set_all` request's
/// `wornBundleIds` into the ledger's bundle credit. Ids the character cannot
/// reach, and items that are not bundles, are dropped (a client cannot credit
/// a garment it cannot see); each survivor is expanded to its leaves
/// server-side. A read failure propagates (v4 throws to the route's catch).
/// The claim counts ride back for v4's DEBUG `[Chats v1] Some claimed worn
/// bundles were not credited`, which the caller logs when they differ.
fn resolve_worn_bundles(
    main: &rusqlite::Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    worn_bundle_ids: &[String],
    tiers: &crate::wardrobe_tiers::SharedWardrobeTiers,
) -> Result<(Vec<WornBundle>, WornBundleClaim), crate::db::DbError> {
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
        return Ok((Vec::new(), none));
    }
    let bundles: Vec<Value> =
        wardrobe_read::find_by_ids_for_character(main, docs, character_id, &ids, tiers)?
            .into_iter()
            .filter(|item| crate::dissolve_bundles::is_bundle(&WearableNode::from_value(item)))
            .collect();
    let mut result: Vec<WornBundle> = Vec::new();
    for bundle in &bundles {
        let node = WearableNode::from_value(bundle);
        let lookup = lookup_for_bundle(main, docs, character_id, &node.component_item_ids, tiers);
        result.extend(worn_bundles_for(&node, lookup.as_ref(), None));
    }
    let claim = WornBundleClaim {
        claimed: ids.len(),
        resolved: result.len(),
    };
    Ok((result, claim))
}

// ===========================================================================
// POST ?action=equip — the handler
// ===========================================================================

/// The mode-dispatch result computed inside the write closure.
enum EquipOutcome {
    /// The written slots, plus — for `set_all` — the credited bundle count its
    /// INFO line reports (logged by the caller, outside the closure).
    Updated {
        slots: Value,
        set_all_claim: Option<WornBundleClaim>,
    },
    Refused(Response),
}

/// v4 `handleEquipSlot` — parse, resolve the chat's project tier, dispatch the
/// mode to the ported primitives, then the two post-mutation legs (the
/// avatar-if-enabled trigger + the debounced Aurora announcement, both
/// failure-swallowed) and `{equippedSlots}`.
pub async fn chat_equip(db: &Db, user_id: &str, chat_id: &str, body: Value) -> Response {
    let parsed = match parse_equip_body(&body) {
        Ok(p) => p,
        Err(joined) => return bad_request(joined),
    };

    let cid = chat_id.to_string();
    let character_id = parsed.character_id.clone();
    let character_id_for_tiers = character_id.clone();

    let out = with_both_conns(db, move |main, mount| {
        // Every shared tier in scope for THIS character (v4
        // `resolveSharedWardrobeTiersForChat` — each half `[]` on any failure).
        // The operator is dressing the character, so the project roster does not apply.
        let tiers = resolve_shared_wardrobe_tiers_for_chat(
            main,
            mount,
            &cid,
            &character_id_for_tiers,
            SharedWardrobeTierOptions { operator: true },
        );
        let docs = DocMountDocumentsRepository::new(mount);

        let updated: Value = match parsed.mode.as_str() {
            "set_all" => {
                // Atomic replace — validate every id resolves for this character
                // before persisting. Ids collect in slot order, first-seen
                // deduped (v4's `Set` insertion order).
                let slots_value = parsed.slots.as_ref().expect("schema-guaranteed");
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
                if !all_ids.is_empty() {
                    let found = wardrobe_read::find_by_ids_for_character(
                        main,
                        &docs,
                        &parsed.character_id,
                        &all_ids,
                        &tiers,
                    )?;
                    let found_ids: Vec<&str> = found
                        .iter()
                        .filter_map(|i| i.get("id").and_then(Value::as_str))
                        .collect();
                    for id in &all_ids {
                        if !found_ids.contains(&id.as_str()) {
                            return Ok(EquipOutcome::Refused(bad_request(format!(
                                "Wardrobe item {id} not available to this character"
                            ))));
                        }
                    }
                }
                let (worn_bundles, claim) = resolve_worn_bundles(
                    main,
                    &docs,
                    &parsed.character_id,
                    parsed.worn_bundle_ids.as_deref().unwrap_or_default(),
                    &tiers,
                )?;
                // Through the chokepoint: a lost write throws to the outer
                // catch (500, no trigger, no announcement).
                commit_equipped_outfit(
                    main,
                    CommitEquippedOutfitInput {
                        chat_id: &cid,
                        character_id: &parsed.character_id,
                        next_slots: &Slots::from_value(Some(slots_value)),
                        worn_bundles: &worn_bundles,
                        source: EquipSource::Ui,
                        at: None,
                    },
                )?;
                return Ok(EquipOutcome::Updated {
                    slots: slots_value.clone(),
                    set_all_claim: Some(claim),
                });
            }
            m @ ("wear" | "equip" | "replace") => {
                // itemId guaranteed by the schema refine.
                let item_id = parsed.item_id.as_deref().expect("schema-guaranteed");
                let Some(item) = wardrobe_read::find_by_id_for_character(
                    main,
                    &docs,
                    &parsed.character_id,
                    item_id,
                    &tiers,
                )?
                else {
                    return Ok(EquipOutcome::Refused(not_found("Wardrobe item")));
                };
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let types = types_of(&item);
                let component_item_ids = component_ids_of(&item);
                let next = if m == "replace" {
                    replace_item(
                        main,
                        &docs,
                        &cid,
                        &parsed.character_id,
                        &id,
                        &types,
                        &component_item_ids,
                        &tiers,
                        EquipSource::Ui,
                    )
                } else {
                    // `wear` (and its deprecated alias `equip`) honor the flag.
                    let replace_flag = item.get("replace").and_then(Value::as_bool) == Some(true);
                    equip_item(
                        main,
                        &docs,
                        &cid,
                        &parsed.character_id,
                        &id,
                        &types,
                        &component_item_ids,
                        replace_flag,
                        &tiers,
                        EquipSource::Ui,
                    )
                };
                next?.to_value()
            }
            "add_to_slot" => {
                let item_id = parsed.item_id.as_deref().expect("schema-guaranteed");
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                let Some(item) = wardrobe_read::find_by_id_for_character(
                    main,
                    &docs,
                    &parsed.character_id,
                    item_id,
                    &tiers,
                )?
                else {
                    return Ok(EquipOutcome::Refused(not_found("Wardrobe item")));
                };
                let types = types_of(&item);
                if !types.iter().any(|t| t == slot) {
                    let title = item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    return Ok(EquipOutcome::Refused(bad_request(format!(
                        "Wardrobe item \"{title}\" does not cover the {slot} slot"
                    ))));
                }
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                add_to_slot(
                    main,
                    &docs,
                    &cid,
                    &parsed.character_id,
                    slot,
                    &id,
                    &types,
                    &component_ids_of(&item),
                    &tiers,
                    EquipSource::Ui,
                )?
                .to_value()
            }
            "remove_from_slot" => {
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                // v4 `itemId ?? undefined` — an empty string is a VALUE here
                // (only the refine arms treat '' as falsy, and remove has none).
                remove_from_slot(
                    main,
                    &cid,
                    &parsed.character_id,
                    slot,
                    parsed.item_id.as_deref(),
                )?
                .to_value()
            }
            _ => {
                // mode === 'clear_slot'
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                remove_from_slot(main, &cid, &parsed.character_id, slot, None)?.to_value()
            }
        };
        Ok(EquipOutcome::Updated {
            slots: updated,
            set_all_claim: None,
        })
    })
    .await;

    let updated = match out {
        Ok(EquipOutcome::Updated {
            slots,
            set_all_claim,
        }) => {
            if let Some(claim) = set_all_claim {
                if claim.resolved != claim.claimed {
                    tracing::debug!(
                        characterId = character_id.as_str(),
                        claimed = claim.claimed,
                        resolved = claim.resolved,
                        context = "wardrobe",
                        "[Chats v1] Some claimed worn bundles were not credited"
                    );
                }
                // `wornBundleCount` is the credit CLAIM that survived
                // resolution (v4 `wornBundles.length`).
                tracing::info!(
                    chatId = chat_id,
                    characterId = character_id.as_str(),
                    wornBundleCount = claim.resolved,
                    context = "wardrobe",
                    "[Chats v1] Equipped outfit replaced (set_all)"
                );
            }
            slots
        }
        Ok(EquipOutcome::Refused(r)) => return r,
        // The route's outer catch — a lost slot write (the chokepoint's
        // `Failed to save the equipped outfit …`), a failed read, a failed
        // credit — skips the trigger and the announcement.
        Err(e) => {
            tracing::error!(
                chatId = chat_id,
                error = %crate::db::fallback::error_text(&e),
                "[Chats v1] Error equipping wardrobe slot"
            );
            return internal("Failed to equip wardrobe slot");
        }
    };

    // Post-mutation legs, both failure-swallowed (v4 awaits them in order).
    let params = AvatarGenerationParams {
        user_id: user_id.to_string(),
        chat_id: chat_id.to_string(),
        character_id: character_id.clone(),
        // P4.D238 MARKED HUNK (the `caller_context` rider) — v4 `outfit.ts:324`.
        caller_context: "[Chats v1] outfit-equip",
        image_profile_id_override: None,
        equipped_slots_override: None,
        // Automatic (a wardrobe change): the configuration cache is exactly what
        // should serve this — an outfit worn before costs nothing.
        force: false,
    };
    trigger_avatar_generation_if_enabled(db, &params).await;
    // v4 wraps the announcement enqueue in try/catch → warn.
    let _ = enqueue_wardrobe_outfit_announcement(db, user_id, chat_id, &character_id).await;

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

/// The item's `componentItemIds` (a missing/absent key reads as empty — v4's
/// `?? []`). A non-empty list makes the item a BUNDLE, which dissolves into its
/// leaves as it goes on.
fn component_ids_of(item: &Value) -> Vec<String> {
    item.get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The item's `types` array (the read shape always carries it).
fn types_of(item: &Value) -> Vec<String> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
