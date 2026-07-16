//! §1 — the `dispatch` + `health` commands (and the §2 `events_attach`
//! command). Thin wrappers over the shared `quilltap-web` transport cores;
//! the inner functions are public so the IPC contract suite can drive them
//! directly over a real booted `StartupStatus` (the tier-4 #2 mechanism —
//! see `tests/ipc_contract.rs`).

use quilltap_web::SharedState;
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime, State};

use crate::events_pump::{self, EventPump};

/// §1 `dispatch`: the `request` value is the same internally-tagged
/// `CoreRequest` JSON the HTTP body carries, verbatim. ALWAYS resolves with
/// the `Response` envelope serialized verbatim — including the typed error
/// envelopes: the boot-failure arm resolves the `dispatch.rs` 503 arm's
/// body, a malformed request resolves the 400 arm's body. IPC carries no
/// HTTP status; the envelope alone is authoritative.
pub async fn dispatch_inner(state: &SharedState, request: Value) -> Value {
    // A `Value` always has string keys, so to_vec cannot fail; the empty
    // fallback would parse to the BadRequest envelope regardless.
    let bytes = serde_json::to_vec(&request).unwrap_or_default();
    let (_status, body) = quilltap_web::dispatch::dispatch_body(state, &bytes).await;
    body
}

#[tauri::command]
pub async fn dispatch(state: State<'_, SharedState>, request: Value) -> Result<Value, String> {
    Ok(dispatch_inner(&state, request).await)
}

/// §1 `health`: `{status, body}` where `status` is exactly the HTTP status
/// `health.rs` would set (200 healthy / 423 locked / 409 lock-conflict /
/// 503 unhealthy) and `body` is the `GET /health` JSON verbatim, so the
/// SPA's interpreter branches identically under both transports.
pub async fn health_inner(state: &SharedState) -> Value {
    let (status, body) = quilltap_web::health::health_parts(state).await;
    json!({ "status": status.as_u16(), "body": body })
}

#[tauri::command]
pub async fn health(state: State<'_, SharedState>) -> Result<Value, String> {
    Ok(health_inner(&state).await)
}

/// §2 `events_attach`: (re)start the event forwarder for this webview
/// session — see [`events_pump::attach`] for the ordering rules.
#[tauri::command]
pub async fn events_attach<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    pump: State<'_, EventPump>,
) -> Result<(), String> {
    events_pump::attach(&app, &state, &pump)
}
