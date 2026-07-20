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

// ---------------------------------------------------------------------------
// P4.6ay unit 12: `/api/v1/custom-tools` — Pascal's Workbench collection
// resource. v4 dispatches on `?action=`; the default GET is the library and the
// default POST is not a served action.
//
// Body parsing lives HERE, mirroring v4's own split: `parseBody` is transport
// (a non-JSON body is a 400 before any Workbench code runs), while the
// definition validation, the metadata union, and the run-refusal arms are core
// logic and live behind the dispatch verbs so the route differential covers
// them.
// ---------------------------------------------------------------------------

fn workbench_unwrap(resp: CoreResponse) -> AxumResponse {
    match resp {
        CoreResponse::CustomToolsLibrary(v)
        | CoreResponse::CustomToolsDestinations(v)
        | CoreResponse::CustomToolPreview(v)
        | CoreResponse::CustomToolAudit(v) => (
            StatusCode::OK,
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

/// `GET /api/v1/custom-tools` (the library) and `?action=destinations`.
pub async fn workbench_get(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let req = match query.get("action").map(String::as_str) {
        Some("destinations") => CoreRequest::CustomToolsDestinations,
        None => CoreRequest::CustomToolsLibrary,
        Some(_) => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Only the destinations action is served on this route",
            )
        }
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => workbench_unwrap(resp),
        Err(r) => r,
    }
}

/// `POST /api/v1/custom-tools?action=preview` / `?action=audit`.
pub async fn workbench_post(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let action = query.get("action").map(String::as_str);
    if !matches!(action, Some("preview") | Some("audit")) {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Only the preview and audit actions are served on this route",
        );
    }

    // v4 `parseBody`: `await req.json()` throwing is the ONLY arm handled here.
    let json_body: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Request body must be JSON"),
    };
    let Some(obj) = json_body.as_object() else {
        return error_json(StatusCode::BAD_REQUEST, "Invalid request body");
    };

    let definition = obj.get("definition").cloned().unwrap_or(Value::Null);
    let params = obj.get("params").cloned();
    let metadata = obj.get("metadata").cloned();
    // §B: the bench's scripted oracle. The shape check lives in the core (the
    // preview union admits `{live:true}`, the audit union deliberately does not),
    // so the edge only forwards it.
    let llm = obj.get("llm").cloned();
    // §B (P4.d10): the mock merged state — forwarded opaquely, validated in the
    // core (`z.record(...).nullish()`). Named apart from the web `state` handle.
    let mock_state = obj.get("state").cloned();

    let req = if action == Some("preview") {
        // `private` is `z.boolean().optional()`: a present non-boolean is a body
        // rejection, an absent one is simply `None`.
        let private = match obj.get("private") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid request body"),
        };
        CoreRequest::CustomToolPreview {
            definition,
            params,
            private,
            metadata,
            state: mock_state,
            llm,
        }
    } else {
        CoreRequest::CustomToolAudit {
            definition,
            params,
            metadata,
            state: mock_state,
            llm,
        }
    };

    match dispatch_core(&state, req).await {
        Ok(resp) => workbench_unwrap(resp),
        Err(r) => r,
    }
}
