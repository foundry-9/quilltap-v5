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

use super::types::{ErrorKind, Response};
use super::zod_issues::{key, zod_uuid_ok, ZodIssue};

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

/// v4's `handleDelete` thunk map, in literal order (`delete.ts` at
/// `ad1c4c37f`): the `availableActions` of the `Unknown action` refusal.
pub const CHAT_DELETE_ACTIONS: &[&str] = &["reset-state", "stop-impersonate"];

/// Which leg of v4's `handleDelete` a `?action=` value takes.
///
/// `Delete` is the ABSENT parameter only. Until `ad1c4c37f` it was also the
/// present-but-empty one — v4 gated on `if (action)`, JS truthiness, so a bare
/// `?action=` DELETED the chat; v4's one `dispatchAction` primitive now refuses
/// it as an unknown action ([`classify_delete_action`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteAction {
    ResetState,
    StopImpersonate,
    Delete,
}

/// A `?action=` value v4's `dispatchAction` refuses on this route: a bare
/// `?action=` (`action == ""`) or a name that is not an own key of the map.
/// The transport renders it as v4's envelope — `{"error":"Unknown action:
/// <action>","availableActions":`[`CHAT_DELETE_ACTIONS`]`}` at 400, plus the
/// middleware's `Unknown action requested` WARN (`quilltap-web`'s
/// `query::unknown_action_response`); it never reaches the cascade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedDeleteAction {
    pub action: String,
}

/// v4 `handleDelete`'s action classification — `dispatchAction(req,
/// {'reset-state', 'stop-impersonate'}, deleteChat)` (`delete.ts` at
/// `ad1c4c37f`). Takes the RAW `searchParams.get('action')`, so the
/// absent-vs-bare distinction is under the differential.
pub fn classify_delete_action(
    raw_action: Option<&str>,
) -> Result<DeleteAction, RefusedDeleteAction> {
    match raw_action {
        None => Ok(DeleteAction::Delete),
        Some("reset-state") => Ok(DeleteAction::ResetState),
        Some("stop-impersonate") => Ok(DeleteAction::StopImpersonate),
        // A bare `?action=` and any unknown name alike — the fallback deletes
        // the whole chat, so neither may fall through to it.
        Some(a) => Err(RefusedDeleteAction {
            action: a.to_string(),
        }),
    }
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
    // contractual (they ride `details` to the client). Both shapes come from
    // the ONE home (P4.101), rendered as values for the bag this route carries.
    let invalid_type = |field: &str, got: Option<&Value>| {
        ZodIssue::invalid_type("string", vec![key(field)], got).to_value()
    };
    let invalid_uuid = |field: &str| ZodIssue::invalid_uuid(vec![key(field)]).to_value();

    // Zod 4.5.4's `z.object` refuses a NON-object body BEFORE any field check,
    // with ONE issue at the root path (`{expected:"object", code:"invalid_type",
    // path:[], message:"Invalid input: expected object, received array"}`) —
    // a per-field walk over `null` / `[]` / `42` would instead invent a
    // `participantId … received undefined` issue (the §3 unification review's
    // catch; `stop_impersonate_null_body` / `_array_body` pin it).
    if !body.is_object() {
        return Err(json!([ZodIssue::invalid_type(
            "object",
            vec![],
            Some(body)
        )
        .to_value()]));
    }

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

/// v4 `handleDelete` (`app/api/v1/chats/[id]/handlers/delete.ts`), whole bar
/// the action gate, which runs first ([`classify_delete_action`] — v4's
/// middleware refuses a bare/unknown action before any thunk). `body` is what `await req.json()` would yield (`{}` when there is
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
    // Classified by [`classify_delete_action`]; a refused action never gets
    // here (the transport answers v4's envelope first).
    action: DeleteAction,
    // `None` = the request bytes were not JSON at all (an EMPTY body included):
    // v4's `await req.json()` throws a `SyntaxError` the middleware turns into
    // 500 `Internal server error` — but only on the leg that READS the body,
    // and only AFTER that leg's chat gate. The edge parses and passes the
    // outcome; the composite decides where it matters (§3 unification review).
    body: Option<&Value>,
) -> Response {
    match action {
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
            // Only now — after the 404 gate — does v4 `await req.json()`.
            let Some(body) = body else {
                return Response::error(ErrorKind::Internal, "Internal server error");
            };
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
        DeleteAction::Delete => chat_delete(db, chat_id).await,
    }
}
