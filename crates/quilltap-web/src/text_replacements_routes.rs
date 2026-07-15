//! P4.6ak REST edges (lane A): the text-replacement-rules settings surface +
//! the chat story-background resolver. Each edge dispatches the corresponding
//! `Request` and UNWRAPS the dispatch envelope to v4's RAW route body (the
//! P4.6ah lesson — the LOCKED SPA client reads the raw body, not the tagged
//! `Response`). The JSON verbs also ride `POST /api/dispatch`; these edges give
//! v4-URL parity for the composer/settings clients.
//!
//! - `GET  /api/v1/settings/text-replacements`               → `{rules, count}`
//! - `POST /api/v1/settings/text-replacements`               → `{rule}` (201)
//! - `POST /api/v1/settings/text-replacements?action=bulk-replace` → `{rules, count}`
//! - `PATCH  /api/v1/settings/text-replacements/{id}`        → `{rule}`
//! - `DELETE /api/v1/settings/text-replacements/{id}`        → 204 (empty)
//! - `GET  /api/v1/chats/{id}?action=get-background`         → the background body
//! - `GET  /api/v1/chats/{id}?action=cost[&detailed=true]`   → the cost breakdown (P4.6ao)

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{
    ErrorKind, QuilltapCore, Request as CoreRequest, Response as CoreResponse,
};
use serde_json::{json, Value};

use crate::files_routes::error_json;
use crate::state::SharedState;

pub(crate) async fn dispatch_core(
    state: &SharedState,
    req: CoreRequest,
) -> Result<CoreResponse, AxumResponse> {
    let Some(host) = state.host() else {
        return Err(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The engine is not running",
        ));
    };
    Ok(host.core().dispatch(req).await)
}

pub(crate) fn error_to_http(e: quilltap_core::api::CoreError) -> AxumResponse {
    let status = match e.kind {
        ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorKind::Locked => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        [("content-type", "application/json")],
        json!({ "error": e.message }).to_string(),
    )
        .into_response()
}

/// Unwrap a `TextReplacement`/`ChatBackground`/`ChatCost` body to the raw route
/// shape. (`ChatCost` is raw in v4 too — `NextResponse.json(breakdown)`, not the
/// successResponse envelope — so it needs no special casing here.)
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::TextReplacement(v)
        | CoreResponse::ChatBackground(v)
        | CoreResponse::ChatCost(v) => (
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
// GET / POST /api/v1/settings/text-replacements
// ===========================================================================

pub async fn text_replacements_get(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::TextReplacementsList).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn text_replacements_post(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        }
    };
    // v4's action dispatch: `?action=bulk-replace` → the full-list replace (200);
    // otherwise create one rule (201).
    if query.get("action").map(String::as_str) == Some("bulk-replace") {
        let req = CoreRequest::TextReplacementsBulkReplace { body: json_body };
        return match dispatch_core(&state, req).await {
            Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
            Err(r) => r,
        };
    }
    let req = CoreRequest::TextReplacementCreate { body: json_body };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// PATCH / DELETE /api/v1/settings/text-replacements/{id}
// ===========================================================================

pub async fn text_replacement_patch(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        }
    };
    let req = CoreRequest::TextReplacementUpdate {
        id,
        body: json_body,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn text_replacement_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::TextReplacementDelete { id }).await {
        // v4 `noContent()` — 204 with an EMPTY body (the dispatch carries null).
        Ok(CoreResponse::TextReplacement(Value::Null)) => StatusCode::NO_CONTENT.into_response(),
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// GET /api/v1/chats/{id}?action=get-background | ?action=cost
// ===========================================================================

pub async fn chat_get_background(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let req = match query.get("action").map(String::as_str) {
        Some("get-background") => CoreRequest::ChatGetBackground { chat_id: id },
        Some("cost") => CoreRequest::ChatGetCost {
            chat_id: id,
            // v4: `searchParams.get('detailed') === 'true'` — the EXACT string
            // compare, so `detailed=1` / `detailed=TRUE` are both false.
            detailed: Some(query.get("detailed").map(String::as_str) == Some("true")),
        },
        // Only these two actions live on this REST edge; the default chat GET +
        // the other actions ride POST /api/dispatch (a loud pointer, not a
        // silent 404 — the mount-point-action precedent).
        _ => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Only the get-background and cost actions are served on this route; the chat GET rides POST /api/dispatch",
            )
        }
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
