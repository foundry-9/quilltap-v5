//! `GET /health` — v4 `app/api/health/route.ts` semantics, collapsed to the
//! phases v5 has (no migrations / seeding / plugin phases):
//!
//! - **200** `{status:"healthy", timestamp, uptime, services:{json:{…}}}` —
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
use serde_json::json;

use crate::state::{SharedState, StartupStatus};

pub async fn health(State(state): State<SharedState>) -> AxumResponse {
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
                [("content-type", "application/json")],
                json!({
                    "status": "lock-conflict",
                    "lockConflict": lock_conflict,
                    "timestamp": timestamp,
                    "uptime": uptime,
                })
                .to_string(),
            )
                .into_response();
        }
        StartupStatus::Failed { message } => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("content-type", "application/json")],
                json!({
                    "status": "unhealthy",
                    "timestamp": timestamp,
                    "uptime": uptime,
                    "services": {},
                    "startupPhase": "failed",
                    "error": message,
                })
                .to_string(),
            )
                .into_response();
        }
    };

    match host.core().dispatch(Request::Health).await {
        Response::Health(h) if h.ready => (
            StatusCode::OK,
            [("content-type", "application/json")],
            json!({
                "status": "healthy",
                "timestamp": timestamp,
                "uptime": uptime,
                "services": {
                    "json": { "status": "healthy", "message": "Database engine is operational" },
                    "fileStorage": { "status": "healthy", "message": "Local file storage operational", "mode": "local" },
                },
            })
            .to_string(),
        )
            .into_response(),
        Response::Health(h) => (
            StatusCode::LOCKED,
            [("content-type", "application/json")],
            json!({
                "status": "locked",
                "dbKeyState": h.pepper_state,
                "timestamp": timestamp,
                "uptime": uptime,
            })
            .to_string(),
        )
            .into_response(),
        other => (
            StatusCode::SERVICE_UNAVAILABLE,
            [("content-type", "application/json")],
            json!({
                "status": "unhealthy",
                "timestamp": timestamp,
                "uptime": uptime,
                "services": {},
                "error": format!("unexpected health response: {other:?}"),
            })
            .to_string(),
        )
            .into_response(),
    }
}
