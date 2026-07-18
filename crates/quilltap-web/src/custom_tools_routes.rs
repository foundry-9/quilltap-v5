//! P4.6ay REST edge: Pascal's custom-tools route (the composer popup's surface).
//! Both edges UNWRAP the dispatch envelope to v4's RAW route body (the SPA
//! client reads the raw body). The `chatCustomToolsList` / `chatCustomToolRun`
//! verbs also ride `POST /api/dispatch` (the SPA's path); this edge gives v4-URL
//! parity.
//!
//! - `GET  /api/v1/chats/{id}/custom-tools`            → `{tools, errors, droppedForCap?}`
//! - `POST /api/v1/chats/{id}/custom-tools?action=run` → `{messages, result}`

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::CustomToolsList(v) | CoreResponse::CustomToolRun(v) => (
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

/// `GET /api/v1/chats/{id}/custom-tools` — the popup roster.
pub async fn custom_tools_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::ChatCustomToolsList { chat_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// `POST /api/v1/chats/{id}/custom-tools?action=run` — a manual run.
pub async fn custom_tools_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    // v4: `POST` is `withActionDispatch({ run: handleRun })` — only `?action=run`
    // is served; anything else is not a valid action.
    if query.get("action").map(String::as_str) != Some("run") {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Only the run action is served on this route",
        );
    }
    let json_body: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        }
    };

    // v4's runSchema: tool (required, min 1), parameters (record, nullish),
    // private (bool, optional), asCharacterId (string, nullish).
    let tool = match json_body.get("tool").and_then(Value::as_str) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return error_json(StatusCode::BAD_REQUEST, "A tool name is required"),
    };
    let parameters = json_body
        .get("parameters")
        .and_then(Value::as_object)
        .cloned();
    let private = json_body.get("private").and_then(Value::as_bool);
    let as_character_id = json_body
        .get("asCharacterId")
        .and_then(Value::as_str)
        .map(str::to_string);

    let req = CoreRequest::ChatCustomToolRun {
        chat_id: id,
        tool,
        parameters,
        private,
        as_character_id,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
