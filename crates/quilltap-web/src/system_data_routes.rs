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
//! - `GET|POST /api/v1/system/jobs` — the COLLECTION (P4.9G3; web-edge-only)
//! - `POST /api/v1/system/unlock?action=change-passphrase` (P4.9G3; the alias)
//!
//! The export/import/backup/restore edges (streaming NDJSON, multipart, byte
//! legs) land in the sibling P4.9G4 / P4.9G5 lanes.

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
/// Re-exported for the P4.9G5 backup edges, which answer the same envelope.
pub fn system_body_public(resp: CoreResponse, status: StatusCode) -> AxumResponse {
    system_body(resp, status)
}

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
    // P4.9G4: the import legs need the raw headers so they can re-drive the
    // multipart parser over the buffered body (v4 branches on `content-type`).
    headers: axum::http::HeaderMap,
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
        // ── P4.9G4 ──
        "export" => crate::qtap_routes::export_download(&state, &body).await,
        "import-preview" => {
            crate::qtap_routes::import_preview(&state, &headers, body.clone()).await
        }
        "import-execute" => crate::qtap_routes::import_execute_not_landed(),
        // ── end P4.9G4 ──
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
    Path(job_id): Path<String>,
) -> AxumResponse {
    dispatch_system(&state, CoreRequest::SystemJobGet { job_id }, StatusCode::OK).await
}

pub async fn system_job_delete(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
) -> AxumResponse {
    dispatch_system(
        &state,
        CoreRequest::SystemJobDelete { job_id },
        StatusCode::OK,
    )
    .await
}

pub async fn system_job_post(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> AxumResponse {
    let action = q.get("action").cloned().unwrap_or_default();
    dispatch_system(
        &state,
        CoreRequest::SystemJobControl { job_id, action },
        StatusCode::OK,
    )
    .await
}

// ── P4.9G3 ──────────────────────────────────────────────────────────────────
//
// `/api/v1/system/jobs` (the COLLECTION) and the change-passphrase alias are
// **web-edge-only** legs: the §1 wire surface (FROZEN this round) has no verb
// for the jobs collection, and the passphrase change already has one
// (`Request::ChangePassphrase`) that this alias simply re-exposes at v4's URL.
// The collection edge therefore reaches `api::system_data`'s free functions
// directly over `host.core().db()` — the established raw-edge pattern
// (`files_routes` / `terminal_routes`) — plus the host job-pump control.

use quilltap_core::api::system_data::{self, JobPumpControl, ProcessorStatus};
use quilltap_core::api::SINGLE_USER_ID;
use quilltap_core::db::runtime::Db;
use std::sync::Arc;

/// The Db + job pump, or the loud typed refusal (a locked engine / a read-only
/// embedder with no cadence assembled — the same refusal the dispatch
/// tasks-queue arms answer).
/// (The `Err` is boxed: an `AxumResponse` dwarfs the `Ok` tuple — clippy
/// `result_large_err`.)
fn db_and_pump(state: &SharedState) -> Result<(Db, Arc<dyn JobPumpControl>), Box<AxumResponse>> {
    let Some(host) = state.host() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is not running",
        )));
    };
    let Some(db) = host.core().db() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database is locked",
        )));
    };
    let Some(pump) = host.core().job_pump_control() else {
        return Err(Box::new(error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "job pump not assembled (job-pump-control seam deferral)",
        )));
    };
    Ok((db, pump))
}

/// v4 `GET /api/v1/system/jobs` (`system/jobs/route.ts:23`) — queue stats +
/// per-type active counts + the processor snapshot, with the optional
/// `includeJobs=true` (50 newest) and `chatId` (pending-for-chat) legs. v4 calls
/// `ensureProcessorRunning()` first; here that is the pump's idempotent `start`.
pub async fn system_jobs_collection_get(
    State(state): State<SharedState>,
    Query(q): Query<HashMap<String, String>>,
) -> AxumResponse {
    let (db, pump) = match db_and_pump(&state) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    pump.start();
    let include_jobs = q.get("includeJobs").map(String::as_str) == Some("true");
    let chat_id = q.get("chatId").filter(|s| !s.is_empty()).cloned();
    let status: ProcessorStatus = pump.status();
    system_body(
        system_data::jobs_list(
            &db,
            SINGLE_USER_ID,
            include_jobs,
            chat_id.as_deref(),
            &status,
        ),
        StatusCode::OK,
    )
}

/// v4 `POST /api/v1/system/jobs` (`route.ts:71`) — enqueue a job; **201** on
/// success. The type gate and the payload-must-be-an-object gate live in the
/// core fn (v4's `BackgroundJobTypeEnum.safeParse` + the explicit check).
pub async fn system_jobs_collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let (db, pump) = match db_and_pump(&state) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let job_type = parsed
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let payload = parsed.get("payload").cloned().unwrap_or(Value::Null);
    // v4: `typeof priority === 'number' ? priority : undefined`.
    let priority = parsed.get("priority").and_then(Value::as_f64);
    let max_attempts = parsed.get("maxAttempts").and_then(Value::as_f64);

    let resp = system_data::jobs_enqueue_now(
        &db,
        SINGLE_USER_ID,
        &job_type,
        &payload,
        priority,
        max_attempts,
    )
    .await;
    // v4 nudges the processor before enqueueing; either order is equivalent for
    // an idempotent start, and doing it after means a rejected body never wakes
    // the pump.
    if matches!(resp, CoreResponse::System(_)) {
        pump.start();
    }
    system_body(resp, StatusCode::CREATED)
}

/// v4 `POST /api/v1/system/unlock?action=change-passphrase`
/// (`system/unlock/route.ts:318`) — the REST alias over the existing
/// `Request::ChangePassphrase` verb (which already reproduces v4's messages and
/// status kinds). Body `{oldPassphrase, newPassphrase}` → `{success:true}`.
///
/// **Scope, named:** only this one action is aliased. v4's four sibling actions
/// (`setup` / `unlock` / `store` / `lock`) all have dispatch verbs the SPA uses;
/// they get no REST alias in this lane and answer `unknown_action` here.
pub async fn system_unlock_post(
    State(state): State<SharedState>,
    Query(q): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let action = q.get("action").map(String::as_str).unwrap_or("");
    if action != "change-passphrase" {
        return unknown_action(action);
    }
    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let str_field = |key: &str| {
        parsed
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let req = CoreRequest::ChangePassphrase {
        old_passphrase: str_field("oldPassphrase"),
        new_passphrase: str_field("newPassphrase"),
    };
    match dispatch_core(&state, req).await {
        Ok(CoreResponse::Ack(_)) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            "{\"success\":true}",
        )
            .into_response(),
        Ok(CoreResponse::Error(e)) => error_to_http(e),
        Ok(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
        Err(r) => r,
    }
}
// ── end P4.9G3 ──
