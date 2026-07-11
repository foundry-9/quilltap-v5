//! # quilltap-web — the axum HTTP transport (Phase-4 P4.2)
//!
//! The first-class no-auth local-web deployment (D1/D2): run the binary (or
//! the container), open a browser. Localhost-trust — the bind address is the
//! only knob (`127.0.0.1` bare, `0.0.0.0` in the container); anyone wanting
//! auth puts a proxy in front. v4's pepper-unlock readiness gate survives as
//! a non-auth concept: ready-gated dispatches answer 503 with the
//! `/setup` pointer while the vault is locked.
//!
//! Route surface (D3/D4/D5):
//!
//! - `POST /api/dispatch` — the one action route ([`dispatch`]).
//! - `GET /api/events` — the one scope-tagged SSE stream ([`events`]).
//! - `GET /health` — the v4 health vocabulary ([`health`]).
//! - the binary resource GETs ([`files_routes`]).
//! - the terminal REST + WebSocket surface ([`terminal_routes`]).
//! - static SPA serving with the index fallback ([`static_serve`]).
//!
//! The router is thin by decree — it marshals the core
//! `Request`/`Response`/`Event` contract; every decision lives behind
//! `CoreEngine::dispatch` (or, for the byte routes, the ported repo reads).

pub mod characters_routes;
pub mod dispatch;
pub mod events;
pub mod files_routes;
pub mod health;
pub mod multipart;
pub mod state;
pub mod static_serve;
pub mod terminal_routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;
use quilltap_core::api::BootError;
use quilltap_core::clock::now_unix_ms;
use quilltap_host::lock::{classify_lock_status, instance_lock_path, LockStatus};
use quilltap_host::{Host, HostConfig};
use serde_json::json;

pub use state::{SharedState, StartupStatus, WebState};

/// Build the full router over a shared state.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/api/dispatch", post(dispatch::dispatch))
        .route("/api/events", get(events::events))
        .route("/api/v1/files/proxy/{*key}", get(files_routes::files_proxy))
        .route("/api/v1/files/{id}", get(files_routes::files_get))
        .route(
            "/api/v1/mount-points/{id}/files/{*path}",
            get(files_routes::mount_file_get),
        )
        .route(
            "/api/v1/mount-points/{id}/blobs/{*path}",
            get(files_routes::mount_blob_get),
        )
        .route(
            "/api/v1/characters/{id}/photos",
            post(characters_routes::characters_photos_post),
        )
        .route(
            "/api/v1/characters/{id}",
            get(characters_routes::characters_get),
        )
        .route(
            "/api/v1/characters",
            post(characters_routes::characters_import_post),
        )
        .route(
            "/api/v1/terminals",
            post(terminal_routes::terminals_post).get(terminal_routes::terminals_get),
        )
        .route(
            "/api/v1/terminals/{id}",
            get(terminal_routes::terminal_get)
                .post(terminal_routes::terminal_post)
                .delete(terminal_routes::terminal_delete),
        )
        .route(
            "/api/v1/terminals/{id}/stream",
            get(terminal_routes::terminal_stream),
        )
        .route("/setup", get(static_serve::setup))
        .fallback(get(static_serve::spa_fallback))
        .with_state(state)
}

/// Boot the host and fold a failure into the served startup status
/// (lock-conflict → the `/health` 409 surface; anything else → 503). The
/// server always starts — a conflicted instance still answers `/health`.
pub fn boot_startup_status(config: HostConfig) -> StartupStatus {
    let base_dir = config.base_dir.clone();
    match Host::start(config) {
        Ok(host) => StartupStatus::Running(Box::new(host)),
        Err(e) => {
            let message = e.to_string();
            // Classify a lock conflict for the 409 body (read-only — the
            // classifier never modifies the lock file).
            let is_assemble = matches!(
                e,
                quilltap_host::host::HostError::Boot(BootError::Assemble(_))
            );
            if is_assemble {
                let lock_path = instance_lock_path(&base_dir);
                match classify_lock_status(&lock_path, now_unix_ms()) {
                    LockStatus::Active { reason } | LockStatus::Suspect { reason } => {
                        return StartupStatus::LockConflict {
                            message,
                            lock_conflict: json!({ "reason": reason }),
                        };
                    }
                    _ => {}
                }
            }
            StartupStatus::Failed { message }
        }
    }
}

/// Assemble the shared state around a startup status.
pub fn web_state(
    startup: StartupStatus,
    version: String,
    base_dir: PathBuf,
    spa_dir: Option<PathBuf>,
) -> SharedState {
    Arc::new(WebState {
        startup,
        started: Instant::now(),
        version,
        spa_dir,
        base_dir,
    })
}

/// Serve the router on `addr` until the process ends. Returns the bound
/// address (useful when `addr` carries port 0 — the tests bind ephemeral).
pub async fn serve(router: Router, addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await
}
