//! P4.9I2A REST edges: the help-docs read surface + the help-chats CRUD/send
//! surface, v4-URL faithful. Each edge dispatches the corresponding `Request` and
//! UNWRAPS the dispatch envelope to v4's RAW route body (the P4.6ah lesson — a
//! v4-shaped client reads the raw body, not the tagged `Response`). The JSON
//! verbs also ride `POST /api/dispatch`; these edges give v4-URL parity.
//! `POST /help-chats` answers 201 (v4 `created`); `POST /{id}/messages` returns
//! the send reply body (`{ messageId }`) — the stream frames ride the global
//! `/api/events` stream, scope-tagged by `chatId` (the `ChatSend` architecture).
//!
//! - `GET    /api/v1/help-docs`                       → `{ documents }`
//! - `GET    /api/v1/help-docs?action=chat-count`     → `{ count }`
//! - `GET    /api/v1/help-docs?action=search&q=`      → `{ matches }`
//! - `GET    /api/v1/help-docs/{id}`                  → `{ document }` (id OR slug)
//! - `GET    /api/v1/help-chats`                      → `{ chats }`
//! - `GET    /api/v1/help-chats?action=eligibility`   → `{ eligible, characters, reasons }`
//! - `POST   /api/v1/help-chats`                      → `{ chat }` (201)
//! - `GET    /api/v1/help-chats/{id}`                 → `{ chat }`
//! - `PATCH  /api/v1/help-chats/{id}`                 → rename → `{ chat }`
//! - `PATCH  /api/v1/help-chats/{id}?action=update-context` → `{ chat }`
//! - `DELETE /api/v1/help-chats/{id}`                 → `{ message }`
//! - `GET    /api/v1/help-chats/{id}/messages`        → `{ messages }`
//! - `POST   /api/v1/help-chats/{id}/messages`        → `{ messageId }`
//!
//! ## Two `?action=` shapes on one surface (memory note
//! `v4-has-three-action-dispatch-shapes`)
//!
//! The help-DOCS route is one of v4's DEFAULT-SERVING shapes: `if (action ===
//! 'chat-count') … if (action === 'search') … return handleList` — an unknown
//! or empty action falls through to the LIST. The help-CHATS routes use the
//! hand-rolled `isValidAction` envelope: `if (!action) → default`, else an
//! unknown action is `badRequest('Unknown action: X. Available actions: …')`.
//! Both are reproduced exactly, so the two must not be "unified".

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a `HelpDocs` / `HelpChat` / `HelpChatSend` body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::HelpDocs(v) | CoreResponse::HelpChat(v) | CoreResponse::HelpChatSend(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => error_to_http(e),
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

fn parse_body(body: &axum::body::Bytes) -> Value {
    serde_json::from_slice::<Value>(body).unwrap_or_else(|_| Value::Object(Default::default()))
}

async fn dispatch(state: &SharedState, req: CoreRequest, ok: StatusCode) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(resp) => unwrap_to_http(resp, ok),
        Err(r) => r,
    }
}

// ===========================================================================
// GET /api/v1/help-docs (+ ?action=chat-count / ?action=search&q=)
// ===========================================================================

pub async fn help_docs_collection_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4 `getActionParam` = `searchParams.get('action')` (FIRST wins), compared
    // with `===` — so `?action=` (empty) and any unknown action fall through to
    // the list. NOT the truthiness gate of the envelope shape.
    match crate::query::first(&query, "action") {
        Some("chat-count") => {
            dispatch(&state, CoreRequest::HelpDocsChatCount, StatusCode::OK).await
        }
        Some("search") => {
            // `searchParams.get('q') ?? ''` — FIRST wins; absent → ''.
            let q = crate::query::first(&query, "q").map(str::to_string);
            dispatch(&state, CoreRequest::HelpDocsSearch { q }, StatusCode::OK).await
        }
        _ => dispatch(&state, CoreRequest::HelpDocsList, StatusCode::OK).await,
    }
}

// ===========================================================================
// GET /api/v1/help-docs/{id}
// ===========================================================================

pub async fn help_docs_item_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(&state, CoreRequest::HelpDocGet { id }, StatusCode::OK).await
}

// ===========================================================================
// GET / POST /api/v1/help-chats
// ===========================================================================

pub async fn help_chats_collection_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4: `if (!action) return handleList` (truthiness — `?action=` lists), then
    // `isValidAction(action, ['eligibility'])` else the envelope-shaped 400.
    match crate::query::action(&query) {
        None => dispatch(&state, CoreRequest::HelpChatList, StatusCode::OK).await,
        Some("eligibility") => {
            dispatch(&state, CoreRequest::HelpChatEligibility, StatusCode::OK).await
        }
        Some(other) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Unknown action: {other}. Available actions: eligibility"),
        ),
    }
}

pub async fn help_chats_collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    // v4's `createHelpChatSchema` runs INSIDE the handler (uncaught); both
    // fields ride raw so the dispatch arm refuses a wrong-typed or missing
    // value the way v4 does (a missing key is `undefined` → Zod refuses).
    let req = CoreRequest::HelpChatCreate {
        character_ids: parsed.get("characterIds").cloned().unwrap_or(Value::Null),
        page_url: parsed.get("pageUrl").cloned().unwrap_or(Value::Null),
    };
    // v4 `created(...)` → 201.
    dispatch(&state, req, StatusCode::CREATED).await
}

// ===========================================================================
// GET / PATCH / DELETE /api/v1/help-chats/{id}
// ===========================================================================

pub async fn help_chats_item_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::HelpChatGet { chat_id: id },
        StatusCode::OK,
    )
    .await
}

pub async fn help_chats_item_patch(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    // v4's gate is `if (!action) return handleRename(...)` — JS truthiness, so
    // a present-but-empty `?action=` renames exactly like an absent one.
    match crate::query::action(&query) {
        None => {
            // `renameSchema` parses AFTER `verifyHelpChat` — the title rides raw.
            let req = CoreRequest::HelpChatRename {
                chat_id: id,
                title: parsed.get("title").cloned().unwrap_or(Value::Null),
            };
            dispatch(&state, req, StatusCode::OK).await
        }
        Some("update-context") => {
            // Likewise `updateContextSchema` — parsed after the verify.
            let req = CoreRequest::HelpChatUpdateContext {
                chat_id: id,
                page_url: parsed.get("pageUrl").cloned().unwrap_or(Value::Null),
            };
            dispatch(&state, req, StatusCode::OK).await
        }
        Some(other) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Unknown action: {other}. Available actions: update-context"),
        ),
    }
}

pub async fn help_chats_item_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::HelpChatDelete { chat_id: id },
        StatusCode::OK,
    )
    .await
}

// ===========================================================================
// GET / POST /api/v1/help-chats/{id}/messages
// ===========================================================================

pub async fn help_chats_messages_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::HelpChatMessages { chat_id: id },
        StatusCode::OK,
    )
    .await
}

pub async fn help_chats_messages_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    // v4 `sendMessageSchema` is parsed INSIDE the handler, after
    // `verifyHelpChat` — both body fields ride raw and the dispatch arm
    // validates them in v4's order.
    let req = CoreRequest::HelpChatSend {
        chat_id: id,
        content: parsed.get("content").cloned().unwrap_or(Value::Null),
        file_ids: parsed.get("fileIds").cloned(),
    };
    dispatch(&state, req, StatusCode::OK).await
}
