//! P4.9I1A REST edges: the dedicated brahma-console CRUD + send surface, v4-URL
//! faithful. Each edge dispatches the corresponding `Request` and UNWRAPS the
//! dispatch envelope to v4's RAW route body (the P4.6ah lesson — a v4-shaped
//! client reads the raw body, not the tagged `Response`). The JSON verbs also ride
//! `POST /api/dispatch`; these edges give v4-URL parity. `POST /brahma-console`
//! answers 201 (v4 `created`); `POST /{id}/messages` returns the send reply body
//! (`{ messageId }`) — the seven stream frames ride the global `/api/events`
//! stream, scope-tagged by `chatId` (the `ChatSend` architecture; v5 has no
//! per-request SSE endpoint).
//!
//! - `GET    /api/v1/brahma-console`                     → `{ chats }`
//! - `POST   /api/v1/brahma-console`                     → `{ chat }` (201)
//! - `GET    /api/v1/brahma-console/{id}`                → `{ chat }`
//! - `PATCH  /api/v1/brahma-console/{id}`                → rename → `{ chat }`
//! - `PATCH  /api/v1/brahma-console/{id}?action=set-model` → `{ chat }`
//! - `DELETE /api/v1/brahma-console/{id}`                → `{ message }`
//! - `GET    /api/v1/brahma-console/{id}/messages`       → `{ messages }`
//! - `POST   /api/v1/brahma-console/{id}/messages`       → `{ messageId }`

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a `BrahmaConsole` / `BrahmaConsoleSend` body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::BrahmaConsole(v) | CoreResponse::BrahmaConsoleSend(v) => (
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

// ===========================================================================
// GET / POST /api/v1/brahma-console
// ===========================================================================

pub async fn brahma_console_collection_get(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::BrahmaConsoleList).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn brahma_console_collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    // v4's `createBrahmaChatSchema` runs INSIDE the handler; the value rides raw
    // so the dispatch arm can refuse a null / empty / non-uuid the way v4 does
    // (P4.60).
    let req = CoreRequest::BrahmaConsoleCreate {
        console_connection_profile_id: parsed.get("connectionProfileId").cloned(),
    };
    match dispatch_core(&state, req).await {
        // v4 `created(...)` → 201.
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / PATCH / DELETE /api/v1/brahma-console/{id}
// ===========================================================================

pub async fn brahma_console_item_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::BrahmaConsoleGet { chat_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn brahma_console_item_patch(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    match query.get("action").map(String::as_str) {
        None => {
            // v4 `handleRename` parses `renameSchema` AFTER `verifyBrahmaChat`,
            // so the title rides raw and the dispatch arm refuses it in v4's
            // order (P4.60).
            let req = CoreRequest::BrahmaConsoleRename {
                chat_id: id,
                title: parsed.get("title").cloned().unwrap_or(Value::Null),
            };
            match dispatch_core(&state, req).await {
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        Some("set-model") => {
            // Likewise `setModelSchema` — parsed after the verify (P4.60).
            let req = CoreRequest::BrahmaConsoleSetModel {
                chat_id: id,
                connection_profile_id: parsed
                    .get("connectionProfileId")
                    .cloned()
                    .unwrap_or(Value::Null),
            };
            match dispatch_core(&state, req).await {
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        Some(other) => error_json(
            StatusCode::BAD_REQUEST,
            &format!("Unknown action: {other}. Available actions: set-model"),
        ),
    }
}

pub async fn brahma_console_item_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::BrahmaConsoleDelete { chat_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / POST /api/v1/brahma-console/{id}/messages
// ===========================================================================

pub async fn brahma_console_messages_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::BrahmaConsoleMessages { chat_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn brahma_console_messages_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    // v4 `sendMessageSchema` is parsed INSIDE the handler, after
    // `verifyBrahmaChat` — so both body fields ride raw and the dispatch arm
    // validates them in v4's order (P4.60). Reading them here also collapsed a
    // wrong-typed value into "the caller didn't say".
    let req = CoreRequest::BrahmaConsoleSend {
        chat_id: id,
        content: parsed.get("content").cloned().unwrap_or(Value::Null),
        file_ids: parsed.get("fileIds").cloned(),
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
