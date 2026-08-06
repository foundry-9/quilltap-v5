//! P4.9H2A REST edges: the embedding-profiles management surface. Each edge
//! dispatches the corresponding `Request` and UNWRAPS the dispatch envelope to
//! v4's RAW route body (the LOCKED SPA client reads the raw body, not the tagged
//! `Response`). The JSON verbs also ride `POST /api/dispatch`; these edges give
//! v4-URL parity for the settings client.
//!
//! - `GET /api/v1/embedding-profiles` → `{profiles, count}` (with `?action=`:
//!   `list-providers` → `{providers}`; `list-models[&provider=]` → `{provider,
//!   models}` / `Record<provider, models[]>`; `fetch-models` → the loud refusal).
//! - `POST /api/v1/embedding-profiles` → created + `apiKey` (201).
//! - `GET/PUT/DELETE /api/v1/embedding-profiles/{id}` → get / updated / `{message}`.
//! - `POST /api/v1/embedding-profiles/{id}?action=refit|reindex|reapply`.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::{json, Value};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap an `EmbeddingProfile` body to the raw route shape (v4's routes answer a
/// bare JSON body, not the successResponse envelope).
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::EmbeddingProfile(v) => (
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

/// v4's `await req.json()` with the empty-body tolerance the routes rely on. On a
/// malformed body returns the 400 arm directly (boxed to keep the `Ok` path
/// small — `clippy::result_large_err`).
fn parse_body(body: &axum::body::Bytes) -> Result<Value, Box<AxumResponse>> {
    if body.is_empty() {
        Ok(Value::Object(Default::default()))
    } else {
        serde_json::from_slice(body).map_err(|e| {
            Box::new(error_json(
                StatusCode::BAD_REQUEST,
                &format!("Invalid body: {e}"),
            ))
        })
    }
}

// ===========================================================================
// GET / POST /api/v1/embedding-profiles
// ===========================================================================

pub async fn collection_get(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let req = match query.get("action").map(String::as_str) {
        Some("list-providers") => CoreRequest::EmbeddingProfileListProviders,
        Some("list-models") => CoreRequest::EmbeddingProfileListModels {
            provider: query.get("provider").cloned(),
        },
        Some("fetch-models") => CoreRequest::EmbeddingProfileFetchModels {
            // v4 400s on a missing provider before the refusal, but the arm is a
            // loud refusal either way; carry the provider through if present.
            provider: query.get("provider").cloned().unwrap_or_default(),
            base_url: query.get("baseUrl").cloned(),
        },
        _ => CoreRequest::EmbeddingProfileList,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    // v4 `created(...)` → 201.
    match dispatch_core(
        &state,
        CoreRequest::EmbeddingProfileCreate { body: json_body },
    )
    .await
    {
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / PUT / DELETE / POST /api/v1/embedding-profiles/{id}
// ===========================================================================

pub async fn item_get(State(state): State<SharedState>, Path(id): Path<String>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::EmbeddingProfileGet { profile_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_put(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let req = CoreRequest::EmbeddingProfileUpdate {
        profile_id: id,
        body: json_body,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_delete(State(state): State<SharedState>, Path(id): Path<String>) -> AxumResponse {
    match dispatch_core(
        &state,
        CoreRequest::EmbeddingProfileDelete { profile_id: id },
    )
    .await
    {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let available = json!(["refit", "reindex", "reapply"]);
    let req = match query.get("action").map(String::as_str) {
        Some("refit") => CoreRequest::EmbeddingProfileRefit { profile_id: id },
        Some("reindex") => CoreRequest::EmbeddingProfileReindex {
            profile_id: id,
            // v4 reads `body.scope`; a present non-string value drops to its JSON
            // text so the handler's `Invalid scope: …` arm fires (an absent scope
            // defaults to 'all').
            scope: json_body.get("scope").map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            }),
        },
        Some("reapply") => CoreRequest::EmbeddingProfileReapply { profile_id: id },
        // v4 `withActionDispatch`: unknown action / missing action → 400 with the
        // dispatcher's exact sentence + availableActions.
        Some(other) => return (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/json")],
            json!({ "error": format!("Unknown action: {other}"), "availableActions": available })
                .to_string(),
        )
            .into_response(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/json")],
                json!({ "error": "Action parameter required", "availableActions": available })
                    .to_string(),
            )
                .into_response()
        }
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
