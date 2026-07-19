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
//! slots regardless). The differential pins this arm — do not "fix" it.
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
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::image_job_common::with_both_conns;
use crate::services::queue_service::enqueue_wardrobe_outfit_announcement;
use crate::tools::wardrobe_shared::{
    add_to_slot, equip_item, remove_from_slot, replace_item,
    resolve_project_mount_point_ids_for_chat,
};

use super::types::{ErrorKind, Response};

/// The four coverage slots, in v4's declaration order (`WARDROBE_SLOT_TYPES`).
const WARDROBE_SLOT_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

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
    let out = db.read_main(move |conn| ChatOutfitsRepository::new(conn).get_equipped_outfit(&cid));
    match out {
        Ok(state) => Response::ChatOutfit(json!({
            "equippedOutfit": state.unwrap_or_else(|| Value::Object(Map::new()))
        })),
        // v4 catches → serverError with this exact message.
        Err(_) => internal("Failed to fetch equipped outfit"),
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

/// Zod 4's UUID format check (`z.uuid()` — RFC-9562-shaped: version 1–8,
/// variant 8/9/a/b; the all-zero nil and all-f max UUIDs also pass).
pub(crate) fn is_zod_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    if s == "00000000-0000-0000-0000-000000000000" || s == "ffffffff-ffff-ffff-ffff-ffffffffffff" {
        return true;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            14 => {
                if !matches!(*c, b'1'..=b'8') {
                    return false;
                }
            }
            19 => {
                if !matches!(*c, b'8' | b'9' | b'a' | b'b' | b'A' | b'B') {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
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
    })
}

// ===========================================================================
// POST ?action=equip — the handler
// ===========================================================================

/// The mode-dispatch result computed inside the write closure.
enum EquipOutcome {
    Updated(Value),
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

    let out = with_both_conns(db, move |main, mount| {
        // Project tier for tri-tier wardrobe resolution (v4
        // `resolveProjectMountPointIdsForChat` — `[]` on any failure).
        let project_mounts = resolve_project_mount_point_ids_for_chat(main, mount, &cid);
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
                        &project_mounts,
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
                // v4 `setEquippedOutfit` returns the passed slots (even for a
                // missing chat — the repo update is a silent no-op there).
                let repo = ChatOutfitsRepository::new(main);
                if repo
                    .set_equipped_outfit(&cid, &parsed.character_id, slots_value)
                    .is_err()
                {
                    // safeQuery fallback null → v4's 500 with this message.
                    return Ok(EquipOutcome::Refused(internal(
                        "Failed to update equipped slot",
                    )));
                }
                slots_value.clone()
            }
            m @ ("wear" | "equip" | "replace") => {
                // itemId guaranteed by the schema refine.
                let item_id = parsed.item_id.as_deref().expect("schema-guaranteed");
                let Some(item) = wardrobe_read::find_by_id_for_character(
                    main,
                    &docs,
                    &parsed.character_id,
                    item_id,
                    &project_mounts,
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
                let next = if m == "replace" {
                    replace_item(main, &cid, &parsed.character_id, &id, &types)
                } else {
                    // `wear` (and its deprecated alias `equip`) honor the flag.
                    let replace_flag = item.get("replace").and_then(Value::as_bool) == Some(true);
                    equip_item(main, &cid, &parsed.character_id, &id, &types, replace_flag)
                };
                match next {
                    Ok(slots) => slots.to_value(),
                    Err(_) => {
                        return Ok(EquipOutcome::Refused(internal(
                            "Failed to update equipped slot",
                        )))
                    }
                }
            }
            "add_to_slot" => {
                let item_id = parsed.item_id.as_deref().expect("schema-guaranteed");
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                let Some(item) = wardrobe_read::find_by_id_for_character(
                    main,
                    &docs,
                    &parsed.character_id,
                    item_id,
                    &project_mounts,
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
                match add_to_slot(main, &cid, &parsed.character_id, slot, &id, &types) {
                    Ok(slots) => slots.to_value(),
                    Err(_) => {
                        return Ok(EquipOutcome::Refused(internal(
                            "Failed to update equipped slot",
                        )))
                    }
                }
            }
            "remove_from_slot" => {
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                // v4 `itemId ?? undefined` — an empty string is a VALUE here
                // (only the refine arms treat '' as falsy, and remove has none).
                match remove_from_slot(
                    main,
                    &cid,
                    &parsed.character_id,
                    slot,
                    parsed.item_id.as_deref(),
                ) {
                    Ok(slots) => slots.to_value(),
                    Err(_) => {
                        return Ok(EquipOutcome::Refused(internal(
                            "Failed to update equipped slot",
                        )))
                    }
                }
            }
            _ => {
                // mode === 'clear_slot'
                let slot = parsed.slot.as_deref().expect("schema-guaranteed");
                match remove_from_slot(main, &cid, &parsed.character_id, slot, None) {
                    Ok(slots) => slots.to_value(),
                    Err(_) => {
                        return Ok(EquipOutcome::Refused(internal(
                            "Failed to update equipped slot",
                        )))
                    }
                }
            }
        };
        Ok(EquipOutcome::Updated(updated))
    })
    .await;

    let updated = match out {
        Ok(EquipOutcome::Updated(v)) => v,
        Ok(EquipOutcome::Refused(r)) => return r,
        // The route's outer catch (a failure before/around the primitives).
        Err(_) => return internal("Failed to equip wardrobe slot"),
    };

    // Post-mutation legs, both failure-swallowed (v4 awaits them in order).
    let params = AvatarGenerationParams {
        user_id: user_id.to_string(),
        chat_id: chat_id.to_string(),
        character_id: character_id.clone(),
        image_profile_id_override: None,
        equipped_slots_override: None,
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
            image_profile_id_override: parsed.image_profile_id.clone(),
            equipped_slots_override: parsed.equipped_slots.clone(),
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
        assert_eq!(keys, ["top", "bottom", "footwear", "accessories"]);
    }
}
