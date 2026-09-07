//! P4.80 — the chat DELETE handler (dogfood finding #117).
//!
//! v4 `handleDelete`'s no-action branch (`app/api/v1/chats/[id]/handlers/
//! delete.ts:48-63`): find the chat, 404 `Chat not found` when it is gone,
//! otherwise `repos.chats.delete(chatId)` and answer `{ success: true }`; the
//! handler's own `catch` logs `[Chats v1] Error deleting chat` and answers 500
//! `Failed to delete chat`.
//!
//! The cascade itself is NOT here — it is the already-ported
//! [`delete_conversation_with_vault_sweep`], which this verb is the first
//! caller of. That wrapper is v4's `ChatsRepository.delete(id, {syncVaults})`:
//! capture the participants, drop the `chats` row (which also drops
//! `chat_messages` and sweeps `conversation_annotations`), clear the messages
//! best-effort, then sweep each participant vault's summary file.
//!
//! **What v4 deliberately does NOT cascade** — and therefore what v5 must leave
//! standing: `memories` rows carrying the `chatId` (the client's separate
//! `DELETE /api/v1/memories?chatId=` is the re-extract flow), `llm_logs`,
//! `background_jobs`, `chat_files` links, `folders`, and the mount-index
//! document rows a Scriptorium render wrote. `chat_delete_equivalence`
//! censuses every one of those tables on both sides.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::services::conversation_summary_vault_bridge::delete_conversation_with_vault_sweep;

use super::settings::{zod_parsed_type, zod_uuid_ok, ZOD_UUID_PATTERN};
use super::types::{ErrorKind, Response};

/// v4 `handleDelete` with no `?action=` (`delete.ts:48-63`).
pub async fn chat_delete(db: &Db, chat_id: &str) -> Response {
    // v4 reads the row first and answers `notFound('Chat')` — i.e. the literal
    // `Chat not found` — before it touches the cascade.
    let id_owned = chat_id.to_string();
    match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error deleting chat");
            return Response::error(ErrorKind::Internal, "Failed to delete chat");
        }
    }

    match delete_conversation_with_vault_sweep(db, chat_id, true).await {
        // v4's `repos.chats.delete` returning `false` means the row vanished
        // between the read and the delete. v4 ignores the boolean and answers
        // `{success: true}` regardless — the row is gone either way, which is
        // what the caller asked for. Reproduced verbatim.
        Ok(_) => {
            tracing::info!(chat_id = %chat_id, "[Chats v1] Chat deleted");
            Response::ChatAdmin(json!({ "success": true }))
        }
        // v4's repository wraps the whole cascade in `safeQuery(…, 'Failed to
        // delete chat')` with NO fallback, so a DB error THROWS and lands in
        // the handler's catch → 500 `Failed to delete chat`.
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error deleting chat");
            Response::error(ErrorKind::Internal, "Failed to delete chat")
        }
    }
}

// ---------------------------------------------------------------------------
// The whole `handleDelete` dispatch
// ---------------------------------------------------------------------------

/// v4's own list, and the tail of the unknown-action sentence
/// (`delete.ts:43` spells the two names inline; this is their single home).
pub const CHAT_DELETE_ACTIONS: &[&str] = &["reset-state", "stop-impersonate"];

/// Which leg of v4's `handleDelete` a `?action=` value takes.
///
/// The `Unknown` arm carries the action so the caller can build v4's sentence;
/// `Delete` is BOTH the absent parameter and the present-but-empty one, because
/// v4's gate is `if (action)` — JS truthiness — and `''` is falsy
/// (`delete.ts:42`). Taking the RAW `searchParams.get('action')` here rather
/// than a pre-folded value is what puts that fold under the differential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteAction {
    ResetState,
    StopImpersonate,
    Unknown(String),
    Delete,
}

/// v4 `handleDelete`'s action classification (`delete.ts:25-45`).
pub fn classify_delete_action(raw_action: Option<&str>) -> DeleteAction {
    match raw_action {
        Some("reset-state") => DeleteAction::ResetState,
        Some("stop-impersonate") => DeleteAction::StopImpersonate,
        // JS truthiness: `''` takes the same leg as an absent parameter.
        Some(a) if !a.is_empty() => DeleteAction::Unknown(a.to_string()),
        _ => DeleteAction::Delete,
    }
}

/// v4's unknown-DELETE-action sentence (`delete.ts:43`), verbatim.
pub fn unknown_delete_action_message(action: &str) -> String {
    format!(
        "Unknown DELETE action: {action}. Available DELETE actions: {}",
        CHAT_DELETE_ACTIONS.join(", ")
    )
}

/// v4 `stopImpersonateSchema` (`app/api/v1/chats/[id]/schemas.ts:140-143`):
/// `{ participantId: z.uuid(), newConnectionProfileId: z.uuid().optional() }`.
///
/// Transcribed issue-for-issue from Zod 4's own output (measured against the
/// installed `zod` 4.5.4 at the `f699da6f6` baseline). `z.object` collects
/// EVERY field's first failing check in declaration order, so a body that gets
/// both fields wrong answers two issues. `.optional()` is not `.nullable()`:
/// an absent `newConnectionProfileId` passes, an explicit `null` does not.
fn parse_stop_impersonate(body: &Value) -> Result<(String, Option<String>), Value> {
    // Zod's issue objects put their keys in a per-code order — `invalid_type`
    // leads with `expected`, `invalid_format` with `origin` — and the bytes are
    // contractual (they ride `details` to the client). `preserve_order` keeps
    // the literals below exactly as written.
    let invalid_type = |field: &str, got: Option<&Value>| {
        json!({
            "expected": "string",
            "code": "invalid_type",
            "path": [field],
            "message": format!(
                "Invalid input: expected string, received {}",
                zod_parsed_type(got)
            ),
        })
    };
    let invalid_uuid = |field: &str| {
        json!({
            "origin": "string",
            "code": "invalid_format",
            "format": "uuid",
            "pattern": ZOD_UUID_PATTERN,
            "path": [field],
            "message": "Invalid UUID",
        })
    };

    let mut issues: Vec<Value> = Vec::new();
    let raw_pid = body.get("participantId");
    let participant_id = match raw_pid.and_then(Value::as_str) {
        Some(s) if zod_uuid_ok(s) => s.to_string(),
        Some(_) => {
            issues.push(invalid_uuid("participantId"));
            String::new()
        }
        None => {
            issues.push(invalid_type("participantId", raw_pid));
            String::new()
        }
    };
    let new_profile = match body.get("newConnectionProfileId") {
        None => None,
        Some(v) => match v.as_str() {
            Some(s) if zod_uuid_ok(s) => Some(s.to_string()),
            Some(_) => {
                issues.push(invalid_uuid("newConnectionProfileId"));
                None
            }
            None => {
                issues.push(invalid_type("newConnectionProfileId", Some(v)));
                None
            }
        },
    };
    if issues.is_empty() {
        Ok((participant_id, new_profile))
    } else {
        Err(Value::Array(issues))
    }
}

/// v4 `handleDelete` (`app/api/v1/chats/[id]/handlers/delete.ts:19-64`), whole.
///
/// `raw_action` is `getActionParam(req)` — i.e. `searchParams.get('action')`,
/// UNFOLDED. `body` is what `await req.json()` would yield (`{}` when there is
/// no body); it is read ONLY on the `stop-impersonate` leg, and only AFTER the
/// chat exists, because that is v4's order (`delete.ts:33-40`): a malformed
/// body against a missing chat is a **404**, not a 400.
///
/// The whole dispatch lives here rather than in the transport so BOTH transports
/// answer v4's bytes from ONE piece of code, and so the differential can drive
/// the guard order (`edge-must-decode-through-the-request-enum`'s sibling
/// lesson: a transport-only composite is a composite no oracle measures).
pub async fn chat_delete_dispatch(
    db: &Db,
    chat_id: &str,
    raw_action: Option<&str>,
    body: &Value,
) -> Response {
    match classify_delete_action(raw_action) {
        // v4 hands the chat id straight to `handleResetState`; no body is read.
        DeleteAction::ResetState => super::salon::chat_state_reset(db, chat_id).await,
        DeleteAction::StopImpersonate => {
            // v4 fetches the chat FIRST and answers `notFound('Chat')` before
            // `handleStopImpersonate` ever calls `req.json()`.
            let id_owned = chat_id.to_string();
            match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &id_owned)) {
                Ok(Some(_)) => {}
                Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
                // v4's `findById` throws through `safeQuery`, landing in the
                // route middleware's catch → 500 `Internal server error`.
                Err(_) => {
                    return Response::error(ErrorKind::Internal, "Internal server error");
                }
            }
            match parse_stop_impersonate(body) {
                Ok((participant_id, new_profile)) => {
                    super::salon::chat_stop_impersonate(
                        db,
                        chat_id,
                        &participant_id,
                        new_profile.as_deref(),
                    )
                    .await
                }
                // An uncaught `.parse` escapes to v4's middleware, which turns a
                // ZodError into `validationError(err)`.
                Err(issues) => Response::validation_error(issues),
            }
        }
        // v4: "Reject unrecognized actions to prevent accidental chat deletion".
        DeleteAction::Unknown(action) => {
            tracing::warn!(
                chat_id = %chat_id,
                action = %action,
                "[Chats v1] Unknown DELETE action, rejecting to prevent data loss"
            );
            Response::error(
                ErrorKind::BadRequest,
                unknown_delete_action_message(&action),
            )
        }
        DeleteAction::Delete => chat_delete(db, chat_id).await,
    }
}
