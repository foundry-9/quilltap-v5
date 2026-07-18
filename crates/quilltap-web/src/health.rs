//! `GET /health` — v4 `app/api/health/route.ts` semantics, collapsed to the
//! phases v5 has (no migrations / seeding / plugin phases):
//!
//! - **200** `{status:"healthy", version, timestamp, uptime, services:{json:{…}}}` —
//!   the engine is ready (v4's JSON-store check maps to "the engine holds an
//!   open `Db`"; the file-storage check has no failure mode here — the local
//!   backend is a directory).
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
        Response::Health(h) if h.ready => (
            StatusCode::OK,
            json!({
                "status": "healthy",
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
                },
            }),
        ),
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

pub async fn health(State(state): State<SharedState>) -> AxumResponse {
    let (status, body) = health_parts(&state).await;
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}
