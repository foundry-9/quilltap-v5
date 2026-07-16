//! §4 — the terminal stream over paired IPC (tier 2):
//! `terminal_attach` / `terminal_send` / `terminal_detach` over
//! `tauri::ipc::Channel`, attach semantics mirroring the frozen WS route
//! (`quilltap-web/src/terminal_routes.rs`) exactly:
//!
//! - unknown session → the `exit {code:-1, signal:"session_not_found"}`
//!   frame on the channel (the WS sends it then closes 1000);
//! - a refused subscribe (an exited session) → the rejection carrying the
//!   WS close reason (`"Failed to subscribe"`);
//! - otherwise the manager's `subscribe` replays the ring buffer as one
//!   `output` frame followed by a `meta` frame, then live frames until the
//!   exit frame closes the stream — all forwarded verbatim
//!   (`WsServerMessage` JSON, the frozen union).
//!
//! `terminal_send` mirrors the WS client arm: `input`/`resize` act on the
//! manager, `ping` answers `pong` on the attached channel (the WS replies
//! on the same socket; the channel is the socket's pair), and a malformed
//! message is swallowed (v4 does a bare `JSON.parse` + type check inside a
//! catch — a failed serde parse plays the same role).
//!
//! The terminal REST verbs (`/api/v1/terminals*` list/get/spawn/kill) ride
//! the §3 protocol unchanged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use quilltap_host::terminal::protocol::{WsClientMessage, WsServerMessage};
use quilltap_host::terminal::TerminalManager;
use quilltap_web::SharedState;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::State;

struct Attachment {
    sub_id: u64,
    channel: Channel<WsServerMessage>,
    task: tauri::async_runtime::JoinHandle<()>,
}

/// Per-session attachments: one live attach per session (a re-attach
/// replaces the previous one — the one-WS-per-pane shape), keyed by
/// session id so `ping`'s `pong` finds its channel.
#[derive(Default)]
pub struct TerminalAttachments(Mutex<HashMap<String, Attachment>>);

/// The ready manager, or the WS-route refusal texts (`terminal_routes.rs`'s
/// `manager_and_db` arms — the HTTP statuses are transport-specific).
fn manager(state: &SharedState) -> Result<Arc<TerminalManager>, String> {
    let Some(host) = state.host() else {
        return Err("server failed to start".into());
    };
    host.terminal_manager()
        .ok_or_else(|| "The database is locked. Unlock it to continue.".into())
}

pub fn attach_inner(
    state: &SharedState,
    attachments: &TerminalAttachments,
    session_id: String,
    on_message: Channel<WsServerMessage>,
) -> Result<(), String> {
    let manager = manager(state)?;

    // v4 ws.ts: unknown session → the exit frame, then close 1000. The IPC
    // analogue: the frame on the channel, then resolve with no attachment.
    if !manager.contains(&session_id) {
        let _ = on_message.send(WsServerMessage::Exit {
            code: Some(-1),
            signal: Some("session_not_found".into()),
        });
        return Ok(());
    }
    let Some(mut sub) = manager.subscribe(&session_id) else {
        // The WS closes 1000 with this reason when subscribe refuses (the
        // session exited between contains() and here).
        return Err("Failed to subscribe".into());
    };

    // subscribe() already queued the ring-buffer `output` + `meta` replay;
    // the forwarder delivers them first — the same order the WS emits.
    let channel = on_message.clone();
    let sub_id = sub.id;
    let task = tauri::async_runtime::spawn(async move {
        while let Some(msg) = sub.rx.recv().await {
            if channel.send(msg).is_err() {
                break;
            }
            // After the exit frame the manager closes the sender; the next
            // recv returns None and the loop ends.
        }
    });

    let mut map = attachments.0.lock().expect("attachments lock");
    if let Some(previous) = map.insert(
        session_id.clone(),
        Attachment {
            sub_id,
            channel: on_message,
            task,
        },
    ) {
        previous.task.abort();
        manager.unsubscribe(&session_id, previous.sub_id);
    }
    Ok(())
}

pub fn send_inner(
    state: &SharedState,
    attachments: &TerminalAttachments,
    session_id: String,
    message: Value,
) -> Result<(), String> {
    let manager = manager(state)?;
    match serde_json::from_value::<WsClientMessage>(message) {
        Ok(WsClientMessage::Input { data }) => {
            // v4 ignores the write result (unknown session = no-op).
            let _ = manager.write(&session_id, &data);
        }
        Ok(WsClientMessage::Resize { cols, rows }) => {
            manager.resize(&session_id, cols, rows);
        }
        Ok(WsClientMessage::Ping) => {
            let map = attachments.0.lock().expect("attachments lock");
            if let Some(attachment) = map.get(&session_id) {
                let _ = attachment.channel.send(WsServerMessage::Pong);
            }
        }
        Err(_) => { /* swallowed, v4's catch */ }
    }
    Ok(())
}

pub fn detach_inner(
    state: &SharedState,
    attachments: &TerminalAttachments,
    session_id: String,
) -> Result<(), String> {
    let manager = manager(state)?;
    let removed = attachments
        .0
        .lock()
        .expect("attachments lock")
        .remove(&session_id);
    if let Some(attachment) = removed {
        attachment.task.abort();
        // The WS route's on-close: unsubscribe (which also drops an exited
        // session once no subscribers remain).
        manager.unsubscribe(&session_id, attachment.sub_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn terminal_attach(
    state: State<'_, SharedState>,
    attachments: State<'_, TerminalAttachments>,
    session_id: String,
    on_message: Channel<WsServerMessage>,
) -> Result<(), String> {
    attach_inner(&state, &attachments, session_id, on_message)
}

#[tauri::command]
pub async fn terminal_send(
    state: State<'_, SharedState>,
    attachments: State<'_, TerminalAttachments>,
    session_id: String,
    message: Value,
) -> Result<(), String> {
    send_inner(&state, &attachments, session_id, message)
}

#[tauri::command]
pub async fn terminal_detach(
    state: State<'_, SharedState>,
    attachments: State<'_, TerminalAttachments>,
    session_id: String,
) -> Result<(), String> {
    detach_inner(&state, &attachments, session_id)
}
