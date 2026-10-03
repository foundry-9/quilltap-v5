//! `GET /health` — v4 `app/api/health/route.ts` semantics, collapsed to the
//! phases v5 has (no migrations / seeding / plugin phases):
//!
//! - **200** `{status:"healthy", version, timestamp, uptime, services:{json,
//!   fileStorage, structure}}` — the engine is ready (v4's JSON-store check maps
//!   to "the engine holds an open `Db`"; the file-storage check has no failure
//!   mode here — the local backend is a directory) and the boot's structural
//!   table check found nothing (`structure: {status:"healthy", message:"All
//!   structural tables verified"}`).
//! - **503** `{status:"degraded", …the same keys…}` — ready, but the boot's
//!   structural pass recorded damage (v4 `e5c6bd0c0`, bug 176):
//!   `structure: {status:"degraded", message:"<n> damaged table(s); …",
//!   problems}`, through v4's two maps (`getOverallStatus` /
//!   `getStatusCode`, `app/api/health/route.ts:144-159`). v4's own UI never
//!   reads that 503 (it branches on 409 alone), so a degraded v4 instance stays
//!   reachable; v5's SPA learns the same carve-out in `interpretHealth` (the
//!   P4.D248 ↔ P4.D247 shared contract). v4's `Structural health check
//!   unavailable` WARN has no v5 twin — the slot read cannot fail (NO-PORT).
//! - **423** `{status:"locked", dbKeyState, timestamp, uptime}` — the vault
//!   is locked (v4's locked mode, byte-shape faithful).
//! - **409** `{status:"lock-conflict", lockConflict, timestamp, uptime}` —
//!   boot hit the single-instance lock (the P4.1d startup-conflict handoff).
//! - **503** `{status:"unhealthy", timestamp, uptime, startupPhase:"failed",
//!   error}` — any other boot failure.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{QuilltapCore, Request, Response};
use quilltap_core::clock::now_iso;
use quilltap_core::db::table_shape::{structure_message, STRUCTURE_HEALTHY_MESSAGE};
use serde_json::{json, Value};

use crate::state::{SharedState, StartupStatus};

/// The transport-agnostic health core: (HTTP status, `GET /health` JSON body).
/// The HTTP route serializes both; the Tauri IPC `health` command carries the
/// numeric status alongside the body (`{status, body}`) so the SPA's
/// interpreter branches identically.
pub async fn health_parts(state: &SharedState) -> (StatusCode, Value) {
    let timestamp = now_iso();
    let uptime = state.uptime_secs();

    let host = match &state.startup {
        StartupStatus::Running(h) => h,
        StartupStatus::LockConflict {
            lock_conflict,
            message: _,
        } => {
            return (
                StatusCode::CONFLICT,
                json!({
                    "status": "lock-conflict",
                    "lockConflict": lock_conflict,
                    "timestamp": timestamp,
                    "uptime": uptime,
                }),
            );
        }
        StartupStatus::Failed { message } => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({
                    "status": "unhealthy",
                    "timestamp": timestamp,
                    "uptime": uptime,
                    "services": {},
                    "startupPhase": "failed",
                    "error": message,
                }),
            );
        }
    };

    match host.core().dispatch(Request::Health).await {
        Response::Health(h) if h.ready => {
            // v4 pushes `json` and `fileStorage` (both healthy here), then the
            // structural record (`checkStructuralHealth`, `route.ts:107-139`).
            let (structure, structure_status) = structure_service(&host.structural_problems());
            let overall = overall_status(&["healthy", "healthy", structure_status]);
            (
                status_code(overall),
                json!({
                    "status": overall,
                    // P4.9c, ADDITIVE and v5-only: v4's health body carries no
                    // version, but the engine has held `HealthDto.version` (the
                    // serving crate's `CARGO_PKG_VERSION`) since P4.0 and nothing
                    // could read it — no v5 code path could display its own
                    // version at all. The About screen is the first consumer; the
                    // Tauri `health` command already carries the same DTO, so both
                    // transports agree.
                    "version": h.version,
                    "timestamp": timestamp,
                    "uptime": uptime,
                    "services": {
                        "json": { "status": "healthy", "message": "Database engine is operational" },
                        "fileStorage": { "status": "healthy", "message": "Local file storage operational", "mode": "local" },
                        "structure": structure,
                    },
                }),
            )
        }
        Response::Health(h) => (
            StatusCode::LOCKED,
            json!({
                "status": "locked",
                // Carried on the locked arm too: the version is a property of
                // the SERVER, not of the vault's state, and a gate screen
                // reporting a version is useful when diagnosing a bad build.
                "version": h.version,
                "dbKeyState": h.pepper_state,
                "timestamp": timestamp,
                "uptime": uptime,
            }),
        ),
        other => (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "status": "unhealthy",
                "timestamp": timestamp,
                "uptime": uptime,
                "services": {},
                "error": format!("unexpected health response: {other:?}"),
            }),
        ),
    }
}

/// v4 `checkStructuralHealth` (`app/api/health/route.ts:107-139`): the service
/// object (key order `status, message, problems`) and its status.
fn structure_service(problems: &[String]) -> (Value, &'static str) {
    if problems.is_empty() {
        (
            json!({ "status": "healthy", "message": STRUCTURE_HEALTHY_MESSAGE }),
            "healthy",
        )
    } else {
        (
            json!({
                "status": "degraded",
                "message": structure_message(problems.len()),
                "problems": problems,
            }),
            "degraded",
        )
    }
}

/// v4 `getOverallStatus` (`route.ts:144-152`): unhealthy > degraded > healthy.
fn overall_status(statuses: &[&'static str]) -> &'static str {
    if statuses.contains(&"unhealthy") {
        "unhealthy"
    } else if statuses.contains(&"degraded") {
        "degraded"
    } else {
        "healthy"
    }
}

/// v4 `getStatusCode` (`route.ts:154-159`): `healthy → 200`, anything else 503.
fn status_code(status: &str) -> StatusCode {
    if status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

pub async fn health(State(state): State<SharedState>) -> AxumResponse {
    let (status, body) = health_parts(&state).await;
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}
