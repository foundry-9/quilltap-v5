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

// === P4.9G5 ===
pub mod backup_routes;
// === end P4.9G5 ===
pub mod brahma_routes;
pub mod characters_routes;
pub mod custom_tools_routes;
pub mod dispatch;
pub mod embedding_profiles_routes;
pub mod events;
pub mod files_routes;
pub mod health;
// === P4.6ar: the llm-logs read surface + system image-aesthetics REST edges ===
pub mod llm_logs_routes;
// === P4.9P: the global-search REST edge ===
pub mod ui_search_routes;
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
// === P4.9G1: the Data & System server surface REST edges ===
pub mod system_data_routes;
// === end P4.9G1 ===
pub mod terminal_routes;
// === P4.6ak: text-replacements + get-background REST edges ===
pub mod text_replacements_routes;
// === end P4.6ak ===
// === P4.9f1: the wardrobe REST edges (lane F1, append-only) ===
pub mod tools_routes;
pub mod wardrobe_routes;
// ── P4.9G4 ──
pub mod qtap_routes;
// ── end P4.9G4 ──
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

/// Initialize the process-global log surface (P4.18): a `tracing-subscriber`
/// fmt subscriber writing to **stderr**, env-filtered by `RUST_LOG` (default
/// `info` when unset — the analog of v4's `LOG_LEVEL` default INFO).
///
/// This restores v4-parity of *operability*: v4 is NOT silent — it logs
/// structured JSON to the console (and/or rotated files) at every one of the
/// swallow sites v5 ported the logic of. v5 was silent from P4.2 to here, which
/// made findings #23/#26 cost hours of invisible failure arms.
///
/// Call once per process, first thing in each bin's entrypoint — never in
/// `Host::start` (it runs per-assembly and in tests; the subscriber is
/// process-global while the *events* belong in host/core). Idempotent by
/// `try_init`: a second call, or a test harness that already installed a
/// subscriber, is a harmless no-op rather than a panic. Writing to stderr keeps
/// the banner/user output on stdout clean.
///
/// Log records are operator output, not data — **no differential applies** (a
/// first for this port; the fidelity obligation stays on the DB/wire/UI
/// surfaces).
pub fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let directive = tracing_filter_directive(std::env::var("RUST_LOG").ok().as_deref());
    // Lossy parse (env_logger-style): an invalid directive keeps its valid
    // parts rather than panicking the boot.
    let filter = EnvFilter::new(directive);
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// The effective `EnvFilter` directive: `RUST_LOG` when set and non-blank, else
/// the `info` default (v4's `LOG_LEVEL` default INFO). Pulled out pure so a
/// test can pin the default/respect behavior without mutating the process
/// environment (env mutation races the parallel test threads).
fn tracing_filter_directive(rust_log: Option<&str>) -> String {
    match rust_log {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => "info".to_string(),
    }
}

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

/// The request-body ceiling for every route (dogfood finding #36).
///
/// axum applies a **2 MB** `DefaultBodyLimit` unless told otherwise, and that
/// limit is enforced by the body extractors — so `Multipart::from_request`
/// fails before any handler runs and the caller sees a flat
/// `400 "Invalid multipart body"`. That silently shadowed every ported
/// application-level cap: `MAX_CHAT_FILE_SIZE` (10 MB,
/// `services/chat_files.rs`) and the 10 MB image cap
/// (`services/file_storage.rs`) can only decide an upload the transport lets
/// through, so a 3 MB photo v4 accepts was refused at the edge with the wrong
/// error and the wrong status.
///
/// v4 imposes no per-route limit of its own — its route handlers stream
/// `request.formData()`. It states TWO ceilings in `next.config.js`, and only
/// one of them governs this surface (dogfood finding #63 — the #36 fix read the
/// wrong one):
///
/// - `experimental.serverActions.bodySizeLimit: '100mb'` applies to **Server
///   Actions**, which no ported route is;
/// - `experimental.proxyClientMaxBodySize: '10gb'` is the one on the request
///   path, and v4's own comment beside it names this exact surface: *"allow
///   large import/export and backup files … Default is 10MB which truncates
///   .qtap import files with memories … Bumped to 10GB so the streaming NDJSON
///   .qtap imports (which can run multi-GB once full memory sets are included)
///   aren't rejected at the proxy layer."*
///
/// So 100 MB was never v4's ceiling here, and a real Friday characters export
/// (791 MB, mostly vault blobs) was refused at the edge with a bare
/// `413 "Failed to buffer the request body: length limit exceeded"`. Matching
/// v4's stated 10 GB keeps the ported caps authoritative (they fire far below
/// it, with v4's own messages) while leaving a hard backstop, which matters
/// because the no-auth HTTP/Docker deployment (D1/D12) is first-class.
///
/// ⚠ Honest limit: v5 buffers the whole body and then parses it, where v4's
/// import streams the NDJSON line by line ("only one record's worth of bytes …
/// in a V8 string at a time"). Raising the ceiling makes the large import
/// *reachable*, not cheap — a multi-GB archive will cost multi-GB of RSS until
/// the import path streams. Recorded as a standing note, not fixed here.
const MAX_REQUEST_BODY_BYTES: usize = 10 * 1024 * 1024 * 1024;

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
        // === P4.D50: the instance-wide Taboo list ===
        .route(
            "/api/v1/settings/taboo",
            get(text_replacements_routes::taboo_settings_get)
                .put(text_replacements_routes::taboo_settings_put),
        )
        // === end P4.D50 ===
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
        // === P4.9H2A: embedding-profiles management REST edges ===
        .route(
            "/api/v1/embedding-profiles",
            get(embedding_profiles_routes::collection_get)
                .post(embedding_profiles_routes::collection_post),
        )
        .route(
            "/api/v1/embedding-profiles/{id}",
            get(embedding_profiles_routes::item_get)
                .put(embedding_profiles_routes::item_put)
                .delete(embedding_profiles_routes::item_delete)
                .post(embedding_profiles_routes::item_post),
        )
        // === end P4.9H2A ===
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
        // P4.9E3B: the tool inventory (a §1-contract REST edge).
        .route("/api/v1/tools", get(tools_routes::tools_get))
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
        // === P4.9P: the global-search endpoint ===
        .route("/api/v1/ui/search", get(ui_search_routes::ui_search_get))
        // === end P4.9P ===
        // === P4.6au: the home dashboard ===
        .route("/api/v1/system/home", get(system_routes::system_home_get))
        // === P4.9G1: the Data & System server surface ===
        .route(
            "/api/v1/system/tools",
            get(system_data_routes::system_tools_get).post(system_data_routes::system_tools_post),
        )
        .route(
            "/api/v1/system/jobs/{id}",
            get(system_data_routes::system_job_get)
                .delete(system_data_routes::system_job_delete)
                .post(system_data_routes::system_job_post),
        )
        // === end P4.9G1 ===
        // === P4.43: the conversation-summaries regeneration edge ===
        .route(
            "/api/v1/system/conversation-summaries",
            get(system_data_routes::system_conversation_summaries_get)
                .post(system_data_routes::system_conversation_summaries_post),
        )
        // === end P4.43 ===
        // === P4.9G3: the jobs COLLECTION edge + the change-passphrase alias ===
        .route(
            "/api/v1/system/jobs",
            get(system_data_routes::system_jobs_collection_get)
                .post(system_data_routes::system_jobs_collection_post),
        )
        .route(
            "/api/v1/system/unlock",
            axum::routing::post(system_data_routes::system_unlock_post),
        )
        // === end P4.9G3 ===
        // === P4.9G5: the byte-level backup legs ===
        .route(
            "/api/v1/system/backup",
            post(backup_routes::system_backup_post),
        )
        .route(
            "/api/v1/system/backup/{id}",
            get(backup_routes::system_backup_download),
        )
        .route(
            "/api/v1/system/restore",
            post(backup_routes::system_restore_post),
        )
        // === end P4.9G5 ===
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
        // P4.18 (unit 4): the request-log analog of v4's `logRequest`. `tower-http`'s
        // `TraceLayer` emits request/response events at DEBUG and failures at ERROR,
        // so the default `info` filter stays quiet and `RUST_LOG=tower_http=debug`
        // (or `RUST_LOG=debug`) surfaces the per-request line on demand — never
        // drowning the default, never a per-token span on the hot path.
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Must sit outside the routes it governs: the body extractors read this
        // limit, so raising it here is what lets the ported per-surface caps be
        // the ones that answer. See [`MAX_REQUEST_BODY_BYTES`].
        .layer(axum::extract::DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
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

#[cfg(test)]
mod tests {
    use super::*;

    // === P4.18 unit 5: the init-helper guards. Log output is operator output,
    // not data — no differential applies; these are plain unit tests. ===

    /// The filter directive defaults to `info` (v4's `LOG_LEVEL` default INFO)
    /// when `RUST_LOG` is unset or blank.
    #[test]
    fn filter_directive_defaults_to_info() {
        assert_eq!(tracing_filter_directive(None), "info");
        assert_eq!(tracing_filter_directive(Some("")), "info");
        assert_eq!(tracing_filter_directive(Some("   ")), "info");
    }

    /// A set `RUST_LOG` is honored verbatim (including per-target directives).
    #[test]
    fn filter_directive_respects_rust_log() {
        assert_eq!(tracing_filter_directive(Some("debug")), "debug");
        assert_eq!(
            tracing_filter_directive(Some("quilltap::jobs=trace,tower_http=debug,info")),
            "quilltap::jobs=trace,tower_http=debug,info"
        );
    }

    /// `init_tracing` is idempotent: `try_init` fails silently on a second
    /// install, so repeated calls (bins share a process with tests) never panic.
    #[test]
    fn init_tracing_is_idempotent() {
        init_tracing();
        init_tracing();
    }
}
