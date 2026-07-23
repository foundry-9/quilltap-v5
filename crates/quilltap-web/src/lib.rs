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

pub mod brahma_routes;
pub mod characters_routes;
pub mod custom_tools_routes;
pub mod dispatch;
pub mod events;
pub mod files_routes;
pub mod health;
// === P4.6ar: the llm-logs read surface + system image-aesthetics REST edges ===
pub mod llm_logs_routes;
// === end P4.6ar ===
pub mod multipart;
// === P4.9c: the user-profile + data-dir REST edges (lane C, append-only) ===
pub mod profile_routes;
// === P4.9a: the user photo gallery REST edges (lane A, append-only) ===
pub mod photos_routes;
// === end P4.9a ===
// === end P4.9c ===
// === P4.6w: documents ===
pub mod qtap_target_route;
// === end P4.6w ===
// === P4.10: where the Angular dist comes from ===
pub mod spa;
// === end P4.10 ===
pub mod state;
pub mod static_serve;
// === P4.6au: the home-dashboard REST edge ===
pub mod system_routes;
// === end P4.6au ===
pub mod terminal_routes;
// === P4.6ak: text-replacements + get-background REST edges ===
pub mod text_replacements_routes;
// === end P4.6ak ===
// === P4.9f1: the wardrobe REST edges (lane F1, append-only) ===
pub mod wardrobe_routes;
// === end P4.9f1 ===

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

/// Resolve the instance root for a host process: explicit `--data-dir` →
/// `--instance` (the launcher registry) → `QUILLTAP_DATA_DIR` → the platform
/// default (docker-aware). Shared by the HTTP binary and the Tauri shell so
/// every deployment resolves the SAME way.
pub fn resolve_instance_base_dir(
    data_dir: Option<PathBuf>,
    instance: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(dir) = data_dir {
        return Ok(dir);
    }
    if let Some(name) = instance {
        use quilltap_core::api::InstanceDirectory as _;
        let registry = quilltap_host::InstanceRegistry::at_default_location();
        let listed = registry.list().map_err(|e| e.to_string())?;
        let found = listed
            .instances
            .iter()
            .find(|i| i.name == name)
            .ok_or_else(|| format!("instance '{name}' is not registered"))?;
        return Ok(PathBuf::from(&found.path));
    }
    Ok(quilltap_host::paths::resolve_base_dir(None))
}

/// The production `HostConfig` every deployment shell boots with: the
/// production spine factory over the instance dir + the process version.
/// Shared by the HTTP binary and the Tauri shell — one boot recipe.
pub fn production_host_config(base_dir: PathBuf, version: String) -> HostConfig {
    let mut config = HostConfig::new(base_dir.clone());
    config.version = version.clone();
    let tz = config.tz.clone();
    config.spine = Some(Arc::new(quilltap_host::ProductionSpineFactory::new(
        base_dir, version, tz,
    )));
    config
}

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
            // P4.9f1 re-points the GET at the wardrobe fan-out (?action=outfit
            // served there; every other action delegates to the P4.6ak handler
            // untouched) and adds the POST action edge (equip |
            // regenerate-avatar).
            get(wardrobe_routes::chat_action_get).post(wardrobe_routes::chat_action_post),
        )
        // === end P4.6ak ===
        // === P4.9f1: the wardrobe REST edges (lane F1, append-only) ===
        .route(
            "/api/v1/wardrobe",
            get(wardrobe_routes::wardrobe_get).post(wardrobe_routes::wardrobe_post),
        )
        .route(
            "/api/v1/wardrobe/transfers",
            get(wardrobe_routes::wardrobe_transfers_get)
                .post(wardrobe_routes::wardrobe_transfers_post),
        )
        .route(
            "/api/v1/wardrobe/preview-avatar",
            post(wardrobe_routes::wardrobe_preview_avatar_post),
        )
        .route(
            "/api/v1/wardrobe/analyze-image",
            post(wardrobe_routes::wardrobe_analyze_image_post),
        )
        .route(
            "/api/v1/wardrobe/{itemId}",
            get(wardrobe_routes::wardrobe_item_get)
                .put(wardrobe_routes::wardrobe_item_put)
                .delete(wardrobe_routes::wardrobe_item_delete),
        )
        // === end P4.9f1 ===
        // === P4.6ay: Pascal's custom-tools route ===
        .route(
            "/api/v1/chats/{id}/custom-tools",
            get(custom_tools_routes::custom_tools_get).post(custom_tools_routes::custom_tools_post),
        )
        .route(
            "/api/v1/custom-tools",
            get(custom_tools_routes::workbench_get).post(custom_tools_routes::workbench_post),
        )
        // === end P4.6ay ===
        // === P4.6ar: the LLM-Inspector reads + the default-aesthetics editors ===
        .route("/api/v1/llm-logs", get(llm_logs_routes::llm_logs_get))
        .route(
            "/api/v1/llm-logs/{id}",
            get(llm_logs_routes::llm_log_get).delete(llm_logs_routes::llm_log_delete),
        )
        .route(
            "/api/v1/system/image-aesthetics",
            get(llm_logs_routes::system_image_aesthetics_get)
                .put(llm_logs_routes::system_image_aesthetics_put),
        )
        // === end P4.6ar ===
        // === P4.6au: the home dashboard ===
        .route("/api/v1/system/home", get(system_routes::system_home_get))
        // === end P4.6au ===
        // === P4.9I1A: the dedicated brahma-console CRUD + send surface ===
        .route(
            "/api/v1/brahma-console",
            get(brahma_routes::brahma_console_collection_get)
                .post(brahma_routes::brahma_console_collection_post),
        )
        .route(
            "/api/v1/brahma-console/{id}",
            get(brahma_routes::brahma_console_item_get)
                .patch(brahma_routes::brahma_console_item_patch)
                .delete(brahma_routes::brahma_console_item_delete),
        )
        .route(
            "/api/v1/brahma-console/{id}/messages",
            get(brahma_routes::brahma_console_messages_get)
                .post(brahma_routes::brahma_console_messages_post),
        )
        // === end P4.9I1A ===
        // === P4.9c: the user-profile + data-dir surface (lane C, append-only) ===
        .route(
            "/api/v1/user/profile",
            get(profile_routes::user_profile_get)
                .put(profile_routes::user_profile_put)
                .patch(profile_routes::user_profile_patch),
        )
        .route(
            "/api/v1/system/data-dir",
            get(profile_routes::system_data_dir_get).post(profile_routes::system_data_dir_post),
        )
        // === end P4.9c ===
        // === P4.9a: the user photo gallery (lane A, append-only) ===
        .route(
            "/api/v1/photos",
            get(photos_routes::photos_list).post(photos_routes::photos_save),
        )
        .route(
            "/api/v1/photos/{id}",
            get(photos_routes::photo_entry_get).delete(photos_routes::photo_entry_delete),
        )
        // P4.9a2: the image-info read the deep detail modals hang off; the
        // DELETE arm is the named loud refusal (v4's orphan-cleanup unported).
        .route(
            "/api/v1/images/{id}",
            get(photos_routes::image_info_get).delete(photos_routes::image_delete_not_available),
        )
        // === end P4.9a ===
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
