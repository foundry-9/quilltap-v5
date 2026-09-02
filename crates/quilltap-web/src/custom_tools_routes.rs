//! P4.6ay REST edge: Pascal's custom-tools route (the composer popup's surface).
//! Both edges UNWRAP the dispatch envelope to v4's RAW route body (the SPA
//! client reads the raw body). The `chatCustomToolsList` / `chatCustomToolRun`
//! verbs also ride `POST /api/dispatch` (the SPA's path); this edge gives v4-URL
//! parity.
//!
//! - `GET  /api/v1/chats/{id}/custom-tools`            → `{tools, errors, droppedForCap?}`
//! - `POST /api/v1/chats/{id}/custom-tools?action=run` → `{messages, result}`

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
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    // v4: `POST` is `withActionDispatch({ run: handleRun })` with NO default
    // handler, so the middleware answers its own two envelopes — a
    // present-but-empty `?action=` is JS-falsy and lands on the no-action leg.
    const AVAILABLE: &[&str] = &["run"];
    const PATH: &str = "/api/v1/chats/[id]/custom-tools";
    match crate::query::action(&query) {
        Some("run") => {}
        Some(other) => {
            return crate::query::unknown_action_response(other, AVAILABLE, "POST", PATH)
        }
        None => return crate::query::action_required_response(AVAILABLE, "POST", PATH),
    }
    let json_body: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        }
    };

    // v4's `runSchema.parse` — UNCAUGHT, so any failure is the middleware's flat
    // 400 `{error: 'Validation error'}` (P4.60: this used to read each key with
    // `and_then(Value::as_str)`/`as_bool`/`as_object`, which turned a
    // present-but-wrong-typed value into "the caller didn't say").
    let body = match quilltap_core::api::custom_tools::parse_run_body(&json_body) {
        Ok(b) => b,
        Err(resp) => return unwrap_to_http(resp, StatusCode::OK),
    };

    let req = CoreRequest::ChatCustomToolRun {
        chat_id: id,
        tool: body.tool,
        parameters: body.parameters,
        private: body.private,
        as_character_id: body.as_character_id,
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
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4 `withCollectionActionDispatch({destinations}, handleLibrary)` — the
    // library IS the default handler, so absent AND `?action=` both list it.
    let req = match crate::query::action(&query) {
        Some("destinations") => CoreRequest::CustomToolsDestinations,
        None => CoreRequest::CustomToolsLibrary,
        Some(other) => {
            return crate::query::unknown_action_response(
                other,
                &["destinations"],
                "GET",
                "/api/v1/custom-tools",
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
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    const AVAILABLE: &[&str] = &["preview", "audit"];
    const PATH: &str = "/api/v1/custom-tools";
    let action = crate::query::action(&query);
    match action {
        Some("preview") | Some("audit") => {}
        Some(other) => {
            return crate::query::unknown_action_response(other, AVAILABLE, "POST", PATH)
        }
        None => return crate::query::action_required_response(AVAILABLE, "POST", PATH),
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
