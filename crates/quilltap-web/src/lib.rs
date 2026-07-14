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
// === P4.6w: documents ===
pub mod qtap_target_route;
// === end P4.6w ===
pub mod state;
pub mod static_serve;
pub mod terminal_routes;
// === P4.6ak: text-replacements + get-background REST edges ===
pub mod text_replacements_routes;
// === end P4.6ak ===

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
        // P4.6ah: the general upload leg + the DELETE-associations REST route
        // (JSON verbs — list/move/promote/folders/maintenance — ride /api/dispatch).
        .route("/api/v1/files", post(files_routes::files_upload_post))
        .route(
            "/api/v1/files/{id}",
            get(files_routes::files_get).delete(files_routes::files_delete),
        )
        .route(
            "/api/v1/chats/{id}/files",
            post(files_routes::chat_files_post),
        )
        .route(
            "/api/v1/mount-points/{id}/files/{*path}",
            get(files_routes::mount_file_get).put(files_routes::mount_file_put),
        )
        .route(
            "/api/v1/mount-points/{id}/blobs/{*path}",
            get(files_routes::mount_blob_get),
        )
        // P4.6y multipart legs (JSON verbs ride /api/dispatch).
        .route(
            "/api/v1/mount-points/{id}",
            post(files_routes::mount_point_action_post),
        )
        .route(
            "/api/v1/mount-points/{id}/blobs",
            post(files_routes::mount_blobs_post),
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
        // === P4.6w: documents — the qtap-target byte route (D4) ===
        .route(
            "/api/v1/chats/{id}/qtap-target",
            get(qtap_target_route::qtap_target_get),
        )
        // === end P4.6w ===
        // === P4.6ak: text-replacements settings surface + chat get-background ===
        .route(
            "/api/v1/settings/text-replacements",
            get(text_replacements_routes::text_replacements_get)
                .post(text_replacements_routes::text_replacements_post),
        )
        .route(
            "/api/v1/settings/text-replacements/{id}",
            axum::routing::patch(text_replacements_routes::text_replacement_patch)
                .delete(text_replacements_routes::text_replacement_delete),
        )
        .route(
            "/api/v1/chats/{id}",
            get(text_replacements_routes::chat_get_background),
        )
        // === end P4.6ak ===
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
