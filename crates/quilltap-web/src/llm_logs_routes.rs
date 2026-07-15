//! P4.6ar REST edges (lane A): the llm-logs read/delete surface + the
//! instance-wide default-image-aesthetics pair. Each edge dispatches the
//! corresponding `Request` and UNWRAPS the dispatch envelope to v4's RAW route
//! body (the P4.6ah lesson — a v4-shaped client reads the raw body, not the
//! tagged `Response`). The JSON verbs also ride `POST /api/dispatch`; these edges
//! give v4-URL parity.
//!
//! - `GET    /api/v1/llm-logs`                → `{logs, count, total, limit, offset}`
//! - `GET    /api/v1/llm-logs/{id}`           → the RAW log object
//! - `DELETE /api/v1/llm-logs/{id}`           → `{success, deletedId}`
//! - `GET    /api/v1/system/image-aesthetics?kind=` → `{content}`
//! - `PUT    /api/v1/system/image-aesthetics?kind=` → `{success: true}`

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap an `LlmLog`/`SystemAesthetic` body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::LlmLog(v) | CoreResponse::SystemAesthetic(v) => (
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

// ===========================================================================
// GET /api/v1/llm-logs
// ===========================================================================

pub async fn llm_logs_get(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let get = |k: &str| query.get(k).cloned();
    let req = CoreRequest::LlmLogsList {
        message_id: get("messageId"),
        chat_id: get("chatId"),
        character_id: get("characterId"),
        // v4's `type` query param → the contract's `logType` field (a field named
        // `type` would collide with the internally-tagged envelope key).
        log_type: get("type"),
        // Both flags are v4's strict `=== 'true'` compare: any other value, and
        // an absent param, are false.
        standalone: query.get("standalone").map(String::as_str) == Some("true"),
        include_messages: query.get("includeMessages").map(String::as_str) == Some("true"),
        // The RAW strings — the handler owns v4's parseInt/Math.min body so that
        // the NaN no-slice quirk survives identically on both transports.
        limit: get("limit"),
        offset: get("offset"),
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / DELETE /api/v1/llm-logs/{id}
// ===========================================================================

pub async fn llm_log_get(State(state): State<SharedState>, Path(id): Path<String>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::LlmLogGet { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn llm_log_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::LlmLogDelete { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / PUT /api/v1/system/image-aesthetics
// ===========================================================================

pub async fn system_image_aesthetics_get(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    // An absent `kind` reaches the handler as `''`, which fails its literal test
    // and answers v4's 400 — the same place v4's `parseAestheticKind` lands it.
    let kind = query.get("kind").cloned().unwrap_or_default();
    match dispatch_core(&state, CoreRequest::SystemImageAestheticsGet { kind }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn system_image_aesthetics_put(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let kind = query.get("kind").cloned().unwrap_or_default();
    // v4: `aestheticContentSchema.safeParse(await req.json().catch(() => ({})))
    //      .data?.content ?? ''`
    // — a malformed body, an absent `content`, and a NON-STRING `content` all
    // resolve to `''`, and `''` DELETES the file. So every non-string path maps
    // to `None` here, which the handler defaults to `''`. Note the kind guard
    // runs BEFORE the body read in v4, and does here too (the handler checks it
    // first), so an invalid kind 400s regardless of the body.
    let content = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v.get("content").and_then(Value::as_str).map(String::from));
    let req = CoreRequest::SystemImageAestheticsSet { kind, content };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
