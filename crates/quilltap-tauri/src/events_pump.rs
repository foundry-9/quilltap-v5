//! §2 — the global event stream over Tauri events.
//!
//! `events_attach` (re)starts the forwarder: subscribe to the core
//! broadcast FIRST, then emit the Green-Room `active_snapshot` backlog as
//! `quilltap://event` frames in order, then forward live frames — the
//! `quilltap_web::events` ordering rule, taken through the same
//! `subscribe_with_backlog` helper so the two transports cannot drift. The
//! payload is the `Event` JSON exactly as the SSE `data:` payload (the same
//! serde serialization; no `id:`/framing wrapper — that wrapper is
//! transport-specific).
//!
//! On broadcast lag (`RecvError::Lagged`, drop-oldest) the shell emits
//! `quilltap://resync` (payload ignored) and keeps forwarding — the IPC
//! analogue of the SSE reopen signal (D3 best-effort; no replay buffer,
//! same as HTTP).

use std::sync::Mutex;

use quilltap_web::SharedState;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast::error::RecvError;

/// The Tauri event carrying each engine `Event` (§2).
pub const EVENT_CHANNEL: &str = "quilltap://event";
/// The lag signal (§2): the SPA bumps its `resyncCounter` on this.
pub const RESYNC_CHANNEL: &str = "quilltap://resync";

/// The forwarder slot. A second attach (a webview reload) replaces — aborts
/// — the previous forwarder, making `events_attach` idempotent: one
/// forwarder per webview session, never two interleaving.
#[derive(Default)]
pub struct EventPump(Mutex<Option<tauri::async_runtime::JoinHandle<()>>>);

pub fn attach<R: Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    pump: &EventPump,
) -> Result<(), String> {
    // Hold the slot for the whole attach so the abort-then-subscribe-then-
    // spawn sequence is atomic against a racing re-attach.
    let mut slot = pump.0.lock().expect("event pump lock");
    if let Some(previous) = slot.take() {
        previous.abort();
    }

    let Some(host) = state.host() else {
        // The SSE route's 503 arm ("server failed to start"), as a rejection.
        return Err("server failed to start".into());
    };
    let (mut rx, backlog) = quilltap_web::events::subscribe_with_backlog(host);

    let app = app.clone();
    *slot = Some(tauri::async_runtime::spawn(async move {
        // The Green-Room backlog first (D6), then live frames — the same
        // order the SSE stream delivers.
        for ev in backlog {
            if app.emit(EVENT_CHANNEL, &ev).is_err() {
                return;
            }
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if app.emit(EVENT_CHANNEL, &ev).is_err() {
                        break;
                    }
                }
                // Lagged: events were dropped (drop-oldest). Signal the
                // client to resync and keep forwarding.
                Err(RecvError::Lagged(_)) => {
                    let _ = app.emit(RESYNC_CHANNEL, ());
                }
                Err(RecvError::Closed) => break,
            }
        }
    }));
    Ok(())
}
