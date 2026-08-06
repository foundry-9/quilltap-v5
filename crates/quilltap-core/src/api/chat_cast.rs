//! The chat **cast** + **avatar-override** dispatch handlers (P4.9E1A) — a
//! differential port of v4's eight chat actions:
//!
//!   - `chatAddParticipant` — `actions/participants.ts:221` (`handleAddParticipantAction`)
//!   - `chatUpdateParticipant` — `actions/participants.ts:438` (`handleUpdateParticipantAction`)
//!   - `chatRemoveParticipant` — `actions/participants.ts:486` (`handleRemoveParticipantAction`)
//!   - `chatRebuildSystemPrompt` — `actions/participants.ts:396` (`handleRebuildSystemPromptAction`)
//!   - `chatGetAvatars` / `chatSetAvatar` / `chatRemoveAvatar` — `actions/avatars.ts`
//!   - `chatToggleAvatarGeneration` — `actions/toggle-avatar-generation.ts`
//!
//! Everything below the action layer lives in
//! [`crate::services::chat_participants`] and [`crate::services::chat_avatars`],
//! shared verbatim with the chat-PUT bag entrance
//! ([`crate::api::salon::chat_update`]) exactly as v4 shares `helpers.ts`
//! between `?action=…` and `processChatUpdates`.
//!
//! ## The ownership pre-read
//!
//! v4's POST/GET dispatchers read the chat FIRST and answer `notFound('Chat')`
//! before any action runs (`handlers/post.ts:112`). v5 has no `?action=` REST
//! edge for chats, so each handler here does that read itself — which is also
//! why the handlers that v4 hands a pre-read `chat` (`add-participant`,
//! `remove-participant`) read it here rather than taking it as an argument.
//!
//! ## Status codes at the dispatch boundary
//!
//! v4 answers **201** for a fresh `add-participant` (200 for a reactivation).
//! The dispatch boundary carries no per-verb success status — every success is
//! 200 at the HTTP edge (the standing `ChatCreate` precedent) — so
//! `chat_cast_routes_equivalence` asserts that difference in both directions
//! rather than normalizing it away. The BODY is compared byte-for-byte.
//!
//! Pinned by `chat_cast_routes_equivalence` (tier 2).

use serde_json::{json, Value};

use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::services::chat_avatars;
use crate::services::chat_participants::{
    apply_outfit_for_added_participant, enrich_participant, handle_add_participant,
    handle_participant_update, handle_remove_participant, is_present,
    resolve_participant_character_name, ParticipantAddData, ParticipantError,
    ParticipantUpdateData,
};
use crate::services::host_notifications::{
    post_host_add_announcement, post_host_join_scenario_announcement,
    post_host_remove_announcement, HostAddAnnouncement, HostCharacter,
    HostJoinScenarioAnnouncement, HostRemoveAnnouncement,
};
use crate::services::outfit_selections::OutfitLlmChooseRunner;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers (v4 `lib/api/responses.ts` semantics)
// ===========================================================================

fn ok(body: Value) -> Response {
    Response::ChatCast(body)
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
/// v4 `notFound(resource)` → `` `${resource} not found` `` at 404.
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// Map the service layer's `{status, message}` onto the envelope. The three
/// statuses v4's action handlers translate are 404 → `errorResponse(msg, 404)`
/// (the RAW message, NOT the `notFound` suffix form), 400 → `badRequest(msg)`,
/// and everything else → `serverError(msg)`.
fn from_participant_error(e: ParticipantError) -> Response {
    match e.status {
        400 => Response::error(ErrorKind::BadRequest, e.message),
        404 => Response::error(ErrorKind::NotFound, e.message),
        _ => Response::error(ErrorKind::Internal, e.message),
    }
}

fn from_avatar_error(e: chat_avatars::AvatarError) -> Response {
    match e.status {
        400 => Response::error(ErrorKind::BadRequest, e.message),
        404 => Response::error(ErrorKind::NotFound, e.message),
        _ => Response::error(ErrorKind::Internal, e.message),
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

fn participants_of(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The `{name, description, characterDocumentMountPointId}` triple the Host
/// add-announcement writer needs.
fn host_character(character: &Value) -> HostCharacter {
    HostCharacter {
        name: s(character, "name").unwrap_or_default(),
        description: s(character, "description"),
        character_document_mount_point_id: s(character, "characterDocumentMountPointId"),
    }
}

/// `enrichParticipant` over the read pool, as a `Value` for the response body.
fn enrich(db: &Db, participant: &Value) -> Result<Value, DbError> {
    let p = participant.clone();
    let enriched = read_main_mount(db, |main, mount| enrich_participant(main, mount, &p))?;
    serde_json::to_value(enriched).map_err(|e| DbError::Key(e.to_string()))
}

// ===========================================================================
// `?action=add-participant`
// ===========================================================================

/// v4 `handleAddParticipantAction`.
#[allow(clippy::too_many_arguments)]
pub async fn chat_add_participant(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    data: &ParticipantAddData,
    outfit_runner: Option<&std::sync::Arc<dyn OutfitLlmChooseRunner>>,
) -> Response {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    if let Err(e) = data.validate() {
        return from_participant_error(e);
    }

    let participants = participants_of(&chat);
    let matches_character = |p: &Value| {
        s(p, "type").as_deref() == Some("CHARACTER")
            && s(p, "characterId").as_deref() == Some(data.character_id.as_str())
    };
    let status_of = |p: &Value| s(p, "status").unwrap_or_else(|| "active".to_string());

    if participants
        .iter()
        .any(|p| matches_character(p) && is_present(&status_of(p)))
    {
        return bad_request("Character is already in this chat");
    }
    if participants
        .iter()
        .any(|p| matches_character(p) && status_of(p) == "absent")
    {
        return bad_request("Character is already in this chat (currently deactivated)");
    }

    // ── The reactivation branch (a soft-removed participant comes back) ──
    if let Some(removed) = participants
        .iter()
        .find(|p| matches_character(p) && status_of(p) == "removed")
        .cloned()
    {
        return reactivate_participant(db, user_id, chat_id, &chat, &removed, data, outfit_runner)
            .await;
    }

    // ── The fresh-add branch ──
    let count = participants.len() as i64;
    let result_chat = match handle_add_participant(db, chat_id, data, count, user_id).await {
        Ok(c) => c,
        Err(e) => return from_participant_error(e),
    };

    // v4 `result.chat.participants.find(p => p.characterId === characterId)` —
    // the FIRST match; every earlier same-character participant was rejected or
    // reactivated above, so this is the one just appended.
    let new_participant = participants_of(&result_chat)
        .into_iter()
        .find(|p| s(p, "characterId").as_deref() == Some(data.character_id.as_str()));

    let enriched = match &new_participant {
        Some(p) => match enrich(db, p) {
            Ok(v) => v,
            Err(e) => return internal(e),
        },
        None => Value::Null,
    };

    let ccid = data.character_id.clone();
    let added_character = match read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &ccid)
    }) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };

    if let (Some(character), Some(participant)) = (&added_character, &new_participant) {
        let participant_id = s(participant, "id").unwrap_or_default();
        post_host_add_announcement(
            db,
            HostAddAnnouncement {
                chat_id: chat_id.to_string(),
                character: host_character(character),
                participant_id: participant_id.clone(),
                initial_status: s(participant, "status"),
            },
        )
        .await;
        // Phase H: compile the identity stack for the new participant.
        crate::services::chat_participants::compile_stack_best_effort(
            db,
            result_chat.clone(),
            &participant_id,
        )
        .await;

        // v4: only when a joinScenario was requested AND the newcomer has no
        // history access (`hasHistoryAccess ?? false`).
        let join_scenario = data
            .join_scenario
            .clone()
            .flatten()
            .filter(|v| !v.is_empty());
        if let Some(join_scenario) = join_scenario {
            if !data.has_history_access.unwrap_or(false) {
                post_host_join_scenario_announcement(
                    db,
                    HostJoinScenarioAnnouncement {
                        chat_id: chat_id.to_string(),
                        character_name: s(character, "name").unwrap_or_default(),
                        target_participant_id: participant_id,
                        join_scenario,
                    },
                )
                .await;
            }
        }
    }

    // The starting outfit — defaults to the character's wardrobe defaults.
    apply_outfit_for_added_participant(
        db,
        user_id,
        chat_id,
        &data.character_id,
        data.outfit_selection.as_ref(),
        outfit_runner,
    )
    .await;

    ok(json!({ "participant": enriched, "chat": result_chat }))
}

/// v4's reactivation branch (`participants.ts:245-311`): a soft-removed
/// participant is patched back to `active` rather than duplicated.
async fn reactivate_participant(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    chat: &Value,
    removed: &Value,
    data: &ParticipantAddData,
    outfit_runner: Option<&std::sync::Arc<dyn OutfitLlmChooseRunner>>,
) -> Response {
    let removed_id = s(removed, "id").unwrap_or_default();
    let controlled_by = data
        .controlled_by
        .clone()
        .filter(|v| !v.is_empty())
        .or_else(|| s(removed, "controlledBy").filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "llm".to_string());
    let connection_profile_id = data
        .connection_profile_id
        .clone()
        .filter(|v| !v.is_empty())
        .or_else(|| s(removed, "connectionProfileId"));
    let display_order = participants_of(chat)
        .iter()
        .filter(|p| is_present(&s(p, "status").unwrap_or_else(|| "active".to_string())))
        .count() as i64;

    let patch = json!({
        "status": "active",
        "isActive": true,
        "removedAt": Value::Null,
        "controlledBy": controlled_by,
        "connectionProfileId": connection_profile_id,
        "displayOrder": display_order,
    });

    let (cid, pid) = (chat_id.to_string(), removed_id.clone());
    let applied = db
        .write(move |w| {
            w.main()
                .chat_participants()
                .update_participant(&cid, &pid, &patch)
        })
        .await;
    match applied {
        Ok(true) => {}
        Ok(false) => return internal("Failed to reactivate participant"),
        Err(e) => return internal(e),
    }
    let cid = chat_id.to_string();
    let updated_chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return internal("Failed to reactivate participant"),
        Err(e) => return internal(e),
    };

    let reactivated = participants_of(&updated_chat)
        .into_iter()
        .find(|p| s(p, "id").as_deref() == Some(removed_id.as_str()));
    let enriched = match &reactivated {
        Some(p) => match enrich(db, p) {
            Ok(v) => v,
            Err(e) => return internal(e),
        },
        None => Value::Null,
    };

    let character_id = reactivated.as_ref().and_then(|p| s(p, "characterId"));
    let character = match character_id {
        Some(cid) => match read_main_mount(db, |main, mount| {
            characters_read::find_by_id(main, mount, &cid)
        }) {
            Ok(c) => c,
            Err(e) => return internal(e),
        },
        None => None,
    };

    if let (Some(character), Some(participant)) = (&character, &reactivated) {
        post_host_add_announcement(
            db,
            HostAddAnnouncement {
                chat_id: chat_id.to_string(),
                character: host_character(character),
                participant_id: s(participant, "id").unwrap_or_default(),
                initial_status: s(participant, "status"),
            },
        )
        .await;
        crate::services::chat_participants::compile_stack_best_effort(
            db,
            updated_chat.clone(),
            &removed_id,
        )
        .await;
    }

    // Reactivation re-applies an outfit ONLY when the caller explicitly sent
    // one — otherwise the character keeps whatever they had on before.
    if data.outfit_selection.is_some() {
        if let Some(character_id) = reactivated.as_ref().and_then(|p| s(p, "characterId")) {
            apply_outfit_for_added_participant(
                db,
                user_id,
                chat_id,
                &character_id,
                data.outfit_selection.as_ref(),
                outfit_runner,
            )
            .await;
        }
    }

    ok(json!({ "participant": enriched, "chat": updated_chat }))
}

// ===========================================================================
// `?action=update-participant`
// ===========================================================================

/// v4 `handleUpdateParticipantAction`. (v4 also accepts the request body wrapped
/// as `{updateParticipant: {…}}`; the typed boundary carries one shape, and the
/// bag entrance is [`crate::api::salon::chat_update`].)
pub async fn chat_update_participant(
    db: &Db,
    chat_id: &str,
    data: &ParticipantUpdateData,
) -> Response {
    if let Err(e) = data.validate() {
        return from_participant_error(e);
    }
    let result_chat = match handle_participant_update(db, chat_id, data).await {
        Ok(c) => c,
        Err(e) => return from_participant_error(e),
    };
    let updated = participants_of(&result_chat)
        .into_iter()
        .find(|p| s(p, "id").as_deref() == Some(data.participant_id.as_str()));
    let enriched = match &updated {
        Some(p) => match enrich(db, p) {
            Ok(v) => v,
            Err(e) => return internal(e),
        },
        None => Value::Null,
    };
    ok(json!({ "participant": enriched, "chat": result_chat }))
}

// ===========================================================================
// `?action=remove-participant`
// ===========================================================================

/// v4 `handleRemoveParticipantAction`.
pub async fn chat_remove_participant(db: &Db, chat_id: &str, participant_id: &str) -> Response {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    if !crate::services::chat_participants::is_uuid(participant_id) {
        return bad_request(crate::services::chat_participants::VALIDATION_ERROR);
    }

    let participants = participants_of(&chat);
    let Some(target) = participants
        .iter()
        .find(|p| s(p, "id").as_deref() == Some(participant_id))
        .cloned()
    else {
        return not_found("Participant");
    };

    let character_name = match read_main_mount(db, |main, mount| {
        resolve_participant_character_name(main, mount, Some(&target))
    }) {
        Ok(n) => n,
        Err(e) => return internal(e),
    };

    // The last-CHARACTER guard, ahead of the repo's own last-PARTICIPANT throw.
    let active_characters = participants
        .iter()
        .filter(|p| {
            s(p, "type").as_deref() == Some("CHARACTER")
                && is_present(&s(p, "status").unwrap_or_else(|| "active".to_string()))
        })
        .count();
    if active_characters <= 1 && s(&target, "type").as_deref() == Some("CHARACTER") {
        return bad_request("Cannot remove the last character from the chat");
    }

    let mut final_chat = match handle_remove_participant(db, chat_id, participant_id).await {
        Ok(c) => c,
        Err(e) => return from_participant_error(e),
    };

    // Impersonation clean-up. v4 `bd419ae9` (bug 24): the cleanup update happens
    // AFTER `result.chat` was captured, so the RESPONSE must reflect it — otherwise
    // it still lists the removed participant in `impersonatingParticipantIds`
    // (stale client state until a refetch). v4 captures the update's return into
    // `finalChat`; v5's `update` returns a bool, so we re-read the post-cleanup row.
    let impersonating: Vec<String> = final_chat
        .get("impersonatingParticipantIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if impersonating.iter().any(|id| id == participant_id) {
        let cleaned: Vec<String> = impersonating
            .iter()
            .filter(|id| id.as_str() != participant_id)
            .cloned()
            .collect();
        let promote =
            s(&final_chat, "activeTypingParticipantId").as_deref() == Some(participant_id);
        let patch = ChatUpdate {
            active_typing_participant_id: promote.then(|| cleaned.first().cloned()),
            impersonating_participant_ids: Some(cleaned),
            ..Default::default()
        };
        let cid = chat_id.to_string();
        if let Err(e) = db
            .write(move |w| ChatsRepository::new(w.main().connection()).update(&cid, &patch))
            .await
        {
            return internal(e);
        }
        // v4 `finalChat = cleanedChat` (only when the update returned a row).
        let cid = chat_id.to_string();
        match db.read_main(move |conn| chats_read::find_by_id(conn, &cid)) {
            Ok(Some(c)) => final_chat = c,
            Ok(None) => {}
            Err(e) => return internal(e),
        }
    }

    if character_name != "Unknown" {
        post_host_remove_announcement(
            db,
            HostRemoveAnnouncement {
                chat_id: chat_id.to_string(),
                character_name,
                participant_id: participant_id.to_string(),
            },
        )
        .await;
    }

    ok(json!({ "success": true, "chat": final_chat }))
}

// ===========================================================================
// `?action=rebuild-system-prompt`
// ===========================================================================

/// v4 `handleRebuildSystemPromptAction` — force-recompile one participant's
/// cached identity stack (the Participants sidebar button, for character edits
/// the compiler does not auto-invalidate).
pub async fn chat_rebuild_system_prompt(db: &Db, chat_id: &str, participant_id: &str) -> Response {
    // v4 reads `participantId` off a permissive `req.json().catch(() => ({}))`
    // and rejects a non-string.
    if participant_id.is_empty() {
        return bad_request("participantId is required");
    }
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let Some(participant) = participants_of(&chat)
        .into_iter()
        .find(|p| s(p, "id").as_deref() == Some(participant_id))
    else {
        return not_found("Participant");
    };
    if s(&participant, "type").as_deref() != Some("CHARACTER")
        || s(&participant, "controlledBy").as_deref() == Some("user")
    {
        return bad_request(
            "System prompt rebuild is only available for LLM-controlled characters",
        );
    }

    if crate::services::chat_participants::compile_stack(db, chat.clone(), participant_id)
        .await
        .is_err()
    {
        return internal("Failed to rebuild system prompt");
    }

    let cid = chat_id.to_string();
    let refreshed = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    ok(json!({ "ok": true, "chat": refreshed.unwrap_or(chat) }))
}

// ===========================================================================
// The avatar-override family
// ===========================================================================

/// v4 `handleGetAvatars` (`GET …?action=get-avatars`).
pub fn chat_get_avatars(db: &Db, user_id: &str, chat_id: &str) -> Response {
    match chat_avatars::get_avatars(db, user_id, chat_id) {
        Ok(body) => ok(body),
        Err(e) => from_avatar_error(e),
    }
}

/// v4 `handleSetAvatar` (`POST …?action=set-avatar`).
pub async fn chat_set_avatar(
    db: &Db,
    chat_id: &str,
    character_id: &str,
    image_id: &str,
) -> Response {
    match chat_avatars::set_avatar(db, chat_id, character_id, image_id).await {
        Ok(body) => ok(body),
        Err(e) => from_avatar_error(e),
    }
}

/// v4 `handleRemoveAvatar` (`POST …?action=remove-avatar`).
pub async fn chat_remove_avatar(db: &Db, chat_id: &str, character_id: &str) -> Response {
    match chat_avatars::remove_avatar(db, chat_id, character_id).await {
        Ok(body) => ok(body),
        Err(e) => from_avatar_error(e),
    }
}

/// v4 `handleToggleAvatarGeneration` (`POST …?action=toggle-avatar-generation`).
/// The dispatcher's ownership pre-read is reproduced here, so a missing chat is
/// v4's `notFound('Chat')` rather than the handler's own 500 arm.
pub async fn chat_toggle_avatar_generation(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    }
    match chat_avatars::toggle_avatar_generation(db, user_id, chat_id).await {
        Ok(body) => ok(body),
        Err(e) => from_avatar_error(e),
    }
}
