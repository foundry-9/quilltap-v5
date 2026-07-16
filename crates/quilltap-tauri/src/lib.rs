//! # quilltap-tauri — the Tauri 2 desktop shell (Phase-4 P4.7a)
//!
//! The same `quilltap-host` engine and the same Angular SPA as the HTTP
//! deployment (D1), with the transport re-mapped from HTTP to Tauri IPC per
//! the shared P4.7 contract (§1–§4, reproduced verbatim in both work
//! orders):
//!
//! - **§1** the `dispatch` + `health` commands ([`commands`]) — thin
//!   wrappers over the reused `quilltap_web::dispatch::dispatch_body` /
//!   `health::health_parts` cores, so the envelope arms are the HTTP
//!   route's arms by construction.
//! - **§2** the global event stream — `events_attach` ([`commands`] +
//!   [`events_pump`]) re-emits engine `Event`s as `quilltap://event`;
//!   `RecvError::Lagged` becomes `quilltap://resync` (the SSE-reopen
//!   analogue).
//! - **§3** the `qtap` custom protocol ([`protocol`]) — the ONE origin
//!   (P4.7c, dogfood finding #12): the window itself loads
//!   `qtap://localhost/`, so the SPA's pages and assets are answered from
//!   the embedded `frontendDist` (index fallback included) and the server
//!   surface — the full `http::Request` — is delegated into the reused
//!   `quilltap_web::build_router` over the same booted state (one `Host`,
//!   one boot); responses buffered, CORS kept permissive for the `devUrl`
//!   dev loop. With page and API on one origin, every server-supplied
//!   RELATIVE URL (`avatarUrl`/`filepath`/`backgroundUrl` DTO fields,
//!   `/api/v1/files/…` links inside pre-rendered bodies) resolves through
//!   this protocol — the wire format stays v4-faithful and un-absolutized.
//! - **§4** the terminal stream over paired IPC ([`terminal_ipc`]) —
//!   `terminal_attach`/`terminal_send`/`terminal_detach` over
//!   `tauri::ipc::Channel`, attach semantics mirroring the frozen WS
//!   route; the terminal REST verbs ride §3 unchanged.
//!
//! Thin by decree: no business logic above the boundary; nothing here
//! touches repos/services except via `Host`/`CoreEngine` and the reused
//! router. The shell owns NO cadence — the `Host` does (boot it, don't wrap
//! it).
//!
//! ## Dev loop (documented, not turnkey — lane B owns the SPA side)
//!
//! Run against the Angular dev server by overlaying `devUrl` on the CLI:
//!
//! ```text
//! cd apps/web && npm start          # the Angular dev server on :4200
//! cd crates/quilltap-tauri && \
//!   cargo tauri dev --config '{"build":{"devUrl":"http://localhost:4200"}}' \
//!   -- -- --data-dir /path/to/instance
//! ```
//!
//! The production window loads `qtap://localhost/` (the `windows[].url` in
//! `tauri.conf.json`), served from the bundled `frontendDist`
//! (`apps/web/dist/quilltap/browser` — run `ng build` first for a real UI;
//! an empty dist is materialized by `build.rs` so plain `cargo build` stays
//! green on a checkout with no SPA build, and the qtap lookup then falls
//! through to the router's placeholder pages).
//!
//! **One canonical serving path (P4.7c tier 2):** the SPA ships ONCE — as
//! the embedded `frontendDist` assets, served exclusively through the
//! `qtap` protocol. The `tauri://localhost` asset origin is no longer
//! loaded by any window, and the reused router's own filesystem static
//! serving stays disabled in this shell (`web_state(…, spa_dir: None)`) —
//! do not hand it a dist directory or point the window back at
//! `tauri://localhost`; either would make the dist ship two ways and let
//! the two copies drift.

pub mod commands;
pub mod events_pump;
pub mod protocol;
pub mod terminal_ipc;

use std::path::PathBuf;
use std::sync::Arc;

struct Args {
    data_dir: Option<PathBuf>,
    instance: Option<String>,
}

/// Accept the HTTP binary's instance-selection flags (`--data-dir`,
/// `--instance`) for dev/fixture runs. Unknown args are ignored, not
/// refused — the platform hands the process args uninvited (macOS `-psn_…`,
/// `cargo tauri dev` separators), so the HTTP binary's strictness stays
/// with the HTTP binary.
fn parse_args() -> Args {
    let mut args = Args {
        data_dir: None,
        instance: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--data-dir" => args.data_dir = it.next().map(PathBuf::from),
            "--instance" => args.instance = it.next(),
            _ => {}
        }
    }
    args
}

/// Boot the engine and run the shell. The boot recipe is the HTTP binary's,
/// via the shared `quilltap-web` helpers: resolve the instance root
/// (`--data-dir` → `--instance` → `QUILLTAP_DATA_DIR` → platform default),
/// assemble the production `HostConfig`, and fold a boot failure into the
/// served `StartupStatus` (a conflicted/failed instance still answers
/// `health` — the SPA's setup/lock surfaces need it).
pub fn run() {
    let args = parse_args();
    let base_dir =
        match quilltap_web::resolve_instance_base_dir(args.data_dir, args.instance.as_deref()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("quilltap-tauri: {e}");
                std::process::exit(2);
            }
        };
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Boot inside the Tauri async runtime so the Host's cadence tasks (job
    // pump / stuck reset / enclave tick — all inside the Host) land on the
    // runtime that lives as long as the process.
    let startup = tauri::async_runtime::block_on(async {
        quilltap_web::boot_startup_status(quilltap_web::production_host_config(
            base_dir.clone(),
            version.clone(),
        ))
    });
    let state = quilltap_web::web_state(startup, version, base_dir, None);
    let router = quilltap_web::build_router(Arc::clone(&state));

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .manage(events_pump::EventPump::default())
        .manage(terminal_ipc::TerminalAttachments::default())
        .register_asynchronous_uri_scheme_protocol("qtap", move |ctx, request, responder| {
            let router = router.clone();
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(protocol::handle_qtap_request(&app, router, request).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::health,
            commands::events_attach,
            terminal_ipc::terminal_attach,
            terminal_ipc::terminal_send,
            terminal_ipc::terminal_detach,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Quilltap shell");
}
