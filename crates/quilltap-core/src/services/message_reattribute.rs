//! Per-message re-attribution (P4.9E3B) — v4
//! `POST /api/v1/messages/{id}?action=reattribute`
//! (`app/api/v1/messages/[id]/route.ts` `handleReattributeAction`, ~:336–395).
//! NOT `bulk-reattribute` (a different verb, `services::chat_admin`).
//!
//! The sequence, v4-faithful: find the message across the user's chats (single
//! indexed lookup + ownership check) → validate the TARGET participant is in
//! the chat → delete every memory sourced from the message (failures logged
//! and swallowed, successes counted) → update only the row's `participantId`
//! → bump the chat's `updatedAt` via an empty `chats.update` (which PRESERVES
//! it — v4's `update({})` reads the existing stamp back; no clock mint) →
//! `{success, message, memoriesDeleted}`.

use serde_json::{json, Value};

use crate::api::chat_outfits::is_zod_uuid;
use crate::api::types::{ErrorKind, Response};
use crate::db::chats::ChatUpdate;
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, memories_read, DbError};
use crate::services::memory_service::delete_memory_with_vector;

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4 `handleReattributeAction`. `new_participant_id` re-runs v4's
/// `z.uuid()` (a non-uuid → the middleware's 400 `Validation error`).
pub async fn message_reattribute(
    db: &Db,
    user_id: &str,
    message_id: &str,
    new_participant_id: &str,
) -> Response {
    if !is_zod_uuid(new_participant_id) {
        return crate::services::chat_admin::validation_error();
    }

    // findMessageInUserChats: indexed chat lookup → ownership → the message row.
    let mid = message_id.to_string();
    let chat_id =
        match db.read_main(move |c| chats_messages_read::find_chat_id_for_message(c, &mid)) {
            Ok(Some(id)) => id,
            Ok(None) => return not_found("Message"),
            Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
        };
    let cid = chat_id.clone();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Message"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };
    if chat.get("userId").and_then(Value::as_str) != Some(user_id) {
        return not_found("Message");
    }
    let cid = chat_id.clone();
    let messages = match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(m) => m,
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };
    let found = messages.iter().any(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("id").and_then(Value::as_str) == Some(message_id)
    });
    if !found {
        return not_found("Message");
    }

    // Validate the target participant exists in the chat.
    let target_in_chat = chat
        .get("participants")
        .and_then(Value::as_array)
        .is_some_and(|ps| {
            ps.iter()
                .any(|p| p.get("id").and_then(Value::as_str) == Some(new_participant_id))
        });
    if !target_in_chat {
        return Response::error(
            ErrorKind::BadRequest,
            "Target participant not found in chat",
        );
    }

    // Delete every memory sourced from the message; failures are logged and
    // swallowed (v4 counts only confirmed deletes).
    let mid = message_id.to_string();
    let memories_from_message =
        match db.read_main(move |c| memories_read::find_by_source_message_id(c, &mid)) {
            Ok(rows) => rows,
            Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
        };
    let mut memories_deleted: i64 = 0;
    for memory in &memories_from_message {
        let character_id = memory
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let memory_id = memory.get("id").and_then(Value::as_str).unwrap_or_default();
        match delete_memory_with_vector(db, character_id, memory_id).await {
            Ok(true) => memories_deleted += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::error!(memory_id, error = %e,
                    "[Messages API v1] Failed to delete memory during re-attribution");
            }
        }
    }

    // Update only the re-attributed row's participantId, then the empty
    // chats.update (updatedAt PRESERVED — v4's update() reads it back).
    let cid = chat_id.clone();
    let mid = message_id.to_string();
    let pid = new_participant_id.to_string();
    let write: Result<bool, DbError> = db
        .write(move |w| {
            let updated = w.main().chat_messages().update_message(
                &cid,
                &mid,
                &json!({ "participantId": pid }),
            )?;
            if updated {
                w.main().chats().update(&cid, &ChatUpdate::default())?;
            }
            Ok(updated)
        })
        .await;
    match write {
        Ok(true) => {}
        Ok(false) => return not_found("Message"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    }

    // The response carries the updated row as read back (the shape v4's Zod
    // re-parse returns — the marshaled event, proven equivalent by the
    // chats-messages families).
    let cid = chat_id.clone();
    let updated_message = match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(rows) => rows.into_iter().find(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && m.get("id").and_then(Value::as_str) == Some(message_id)
        }),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };
    let Some(updated_message) = updated_message else {
        return not_found("Message");
    };

    Response::ChatDialog(json!({
        "success": true,
        "message": updated_message,
        "memoriesDeleted": memories_deleted,
    }))
}
