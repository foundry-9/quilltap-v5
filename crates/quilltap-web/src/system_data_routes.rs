//! P4.9G1 REST edges — the Data & System server surface's v4-URL parity routes.
//! Each edge dispatches the matching `Request` and unwraps the dispatch envelope
//! to v4's RAW route body (the `Response::System(Value)` carrier). The JSON verbs
//! also ride `POST /api/dispatch`; these edges give v4-URL parity for the
//! Settings "Data & System" tab.
//!
//! - `GET  /api/v1/system/tools?action=tasks-queue|job-concurrency|delete-data-preview`
//! - `POST /api/v1/system/tools?action=tasks-queue|job-concurrency|delete-data`
//! - `GET  /api/v1/system/jobs/{id}`
//! - `DELETE /api/v1/system/jobs/{id}`
//! - `POST /api/v1/system/jobs/{id}?action=pause|resume`
//!
//! The export/import/backup/restore edges (streaming NDJSON, multipart, byte
//! legs) land in later P4.9G1 units.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::Value;

use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a `Response::System(Value)` to the raw route body at `status`.
fn system_body(resp: CoreResponse, status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::System(v) => (
            status,
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

async fn dispatch_system(
    state: &SharedState,
    req: CoreRequest,
    status: StatusCode,
) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(resp) => system_body(resp, status),
        Err(r) => r,
    }
}

fn unknown_action(action: &str) -> AxumResponse {
    error_json(
        StatusCode::BAD_REQUEST,
        &format!("Unknown action: {action}"),
    )
}

// ── GET /api/v1/system/tools ─────────────────────────────────────────────────

pub async fn system_tools_get(
    State(state): State<SharedState>,
    Query(q): Query<HashMap<String, String>>,
) -> AxumResponse {
    let action = q.get("action").map(String::as_str).unwrap_or("");
    match action {
        "tasks-queue" => {
            dispatch_system(&state, CoreRequest::SystemTasksQueue, StatusCode::OK).await
        }
        "job-concurrency" => {
            dispatch_system(&state, CoreRequest::SystemJobConcurrencyGet, StatusCode::OK).await
        }
        "delete-data-preview" => {
            dispatch_system(&state, CoreRequest::SystemDeleteDataPreview, StatusCode::OK).await
        }
        "export-entities" => {
            let Some(entity_type) = q.get("type").filter(|s| !s.is_empty()).cloned() else {
                return error_json(StatusCode::BAD_REQUEST, "Missing type parameter");
            };
            dispatch_system(
                &state,
                CoreRequest::SystemExportEntities { entity_type },
                StatusCode::OK,
            )
            .await
        }
        "export-preview" => {
            let Some(entity_type) = q.get("type").filter(|s| !s.is_empty()).cloned() else {
                return error_json(StatusCode::BAD_REQUEST, "Missing required parameter: type");
            };
            let scope = q.get("scope").cloned();
            let selected_ids = q
                .get("selectedIds")
                .map(|s| {
                    s.split(',')
                        .filter(|p| !p.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let include_memories = q.get("includeMemories").map(String::as_str) == Some("true");
            dispatch_system(
                &state,
                CoreRequest::SystemExportPreview {
                    entity_type,
                    scope,
                    selected_ids,
                    include_memories,
                },
                StatusCode::OK,
            )
            .await
        }
        other => unknown_action(other),
    }
}

// ── POST /api/v1/system/tools ────────────────────────────────────────────────

pub async fn system_tools_post(
    State(state): State<SharedState>,
    Query(q): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let action = q.get("action").map(String::as_str).unwrap_or("");
    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match action {
        "tasks-queue" => {
            let Some(control) = parsed.get("action").and_then(Value::as_str) else {
                return error_json(
                    StatusCode::BAD_REQUEST,
                    "Invalid action. Must be \"start\" or \"stop\"",
                );
            };
            dispatch_system(
                &state,
                CoreRequest::SystemTasksQueueControl {
                    action: control.to_string(),
                },
                StatusCode::OK,
            )
            .await
        }
        "job-concurrency" => {
            // v4 body `{concurrency}`; the verb field is `maxConcurrentJobs`.
            let value = parsed.get("concurrency").and_then(Value::as_i64);
            let Some(value) = value else {
                return error_json(
                    StatusCode::BAD_REQUEST,
                    "Validation error: concurrency must be an integer between 1 and 32",
                );
            };
            dispatch_system(
                &state,
                CoreRequest::SystemJobConcurrencySet {
                    max_concurrent_jobs: value,
                },
                StatusCode::OK,
            )
            .await
        }
        "delete-data" => {
            let confirm = parsed
                .get("confirm")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            dispatch_system(
                &state,
                CoreRequest::SystemDeleteData { confirm },
                StatusCode::OK,
            )
            .await
        }
        other => unknown_action(other),
    }
}

// ── /api/v1/system/jobs/{id} ─────────────────────────────────────────────────

pub async fn system_job_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch_system(&state, CoreRequest::SystemJobGet { id }, StatusCode::OK).await
}

pub async fn system_job_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch_system(&state, CoreRequest::SystemJobDelete { id }, StatusCode::OK).await
}

pub async fn system_job_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> AxumResponse {
    let action = q.get("action").cloned().unwrap_or_default();
    dispatch_system(
        &state,
        CoreRequest::SystemJobControl { id, action },
        StatusCode::OK,
    )
    .await
}
