//! The terminal REST + WebSocket surface (D5) — v4 `app/api/v1/terminals/*`
//! and `lib/terminal/ws.ts`, marshalling `quilltap_host::terminal::protocol`
//! verbatim. No session auth exists in v5 (D2) — the routes gate only on
//! readiness. The spawn route posts the session-opened Ariel announcement
//! (the P4.1c call-site handoff, closed here).

use std::sync::Arc;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::db::terminal_sessions::{self, TerminalSessionRow, TerminalSessionsRepository};
use quilltap_core::services::ariel_notifications::{
    post_ariel_session_opened_announcement, ArielSessionOpenedAnnouncement,
};
use quilltap_host::terminal::protocol::{PtySessionMeta, WsClientMessage, WsServerMessage};
use quilltap_host::terminal::{SpawnOptions, TerminalManager};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::SharedState;

fn error_json(status: StatusCode, message: &str) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        json!({ "error": message }).to_string(),
    )
        .into_response()
}

fn ok_json(status: StatusCode, body: Value) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// The ready manager + db, or the refusal.
fn manager_and_db(
    state: &SharedState,
) -> Result<(Arc<TerminalManager>, quilltap_core::db::runtime::Db), Box<AxumResponse>> {
    let Some(host) = state.host() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "server failed to start",
        )));
    };
    let (Some(manager), Some(db)) = (host.terminal_manager(), host.core().db()) else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The database is locked. Unlock it to continue.",
        )));
    };
    Ok((manager, db))
}

/// A DB `terminal_sessions` row as the wire meta (v4's row minus the
/// repository-managed `createdAt`/`updatedAt`).
fn row_to_meta_json(row: &TerminalSessionRow) -> Value {
    json!({
        "id": row.id,
        "chatId": row.chat_id,
        "label": row.label,
        "shell": row.shell,
        "cwd": row.cwd,
        "startedAt": row.started_at,
        "exitedAt": row.exited_at,
        "exitCode": row.exit_code,
        "transcriptPath": row.transcript_path,
    })
}

// ---------------------------------------------------------------------------
// POST /api/v1/terminals — spawn (+ the Ariel session-opened announcement)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SpawnBody {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

pub async fn terminals_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let (manager, db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let body: SpawnBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid request: {e}")),
    };
    let session = match manager
        .spawn(SpawnOptions {
            chat_id: body.chat_id.clone(),
            label: body.label.clone(),
            shell: body.shell,
            cwd: body.cwd,
            cols: body.cols,
            rows: body.rows,
            env: Vec::new(),
        })
        .await
    {
        Ok(meta) => meta,
        Err(_) => {
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to spawn terminal",
            )
        }
    };

    // The P4.1c handoff: the spawn route posts the session-opened Ariel
    // announcement (v4 terminals/route.ts POST).
    let _ = post_ariel_session_opened_announcement(
        &db,
        &ArielSessionOpenedAnnouncement {
            chat_id: body.chat_id,
            session_id: session.id.clone(),
            label: session.label.clone(),
            shell: session.shell.clone(),
            cwd: session.cwd.clone(),
        },
    )
    .await;

    ok_json(
        StatusCode::CREATED,
        json!({ "session": meta_json(&session) }),
    )
}

fn meta_json(meta: &PtySessionMeta) -> Value {
    serde_json::to_value(meta).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------
// GET /api/v1/terminals?chatId=
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "chatId")]
    chat_id: Option<String>,
}

pub async fn terminals_get(
    State(state): State<SharedState>,
    Query(query): Query<ListQuery>,
) -> AxumResponse {
    let (_manager, db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let Some(chat_id) = query.chat_id else {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Missing required query parameter: chatId",
        );
    };
    let rows = db.read_main(move |conn| terminal_sessions::find_by_chat_id(conn, &chat_id));
    match rows {
        Ok(rows) => ok_json(
            StatusCode::OK,
            json!({ "sessions": rows.iter().map(row_to_meta_json).collect::<Vec<_>>() }),
        ),
        Err(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list terminals",
        ),
    }
}

// ---------------------------------------------------------------------------
// GET / POST / DELETE /api/v1/terminals/{id}
// ---------------------------------------------------------------------------

pub async fn terminal_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    let (manager, db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    if let Some(meta) = manager.session_meta(&id) {
        let ring = manager.get_ring_buffer(&id);
        return ok_json(
            StatusCode::OK,
            json!({ "session": meta_json(&meta), "ringBuffer": ring }),
        );
    }
    let sid = id.clone();
    match db.read_main(move |conn| terminal_sessions::find_by_id(conn, &sid)) {
        Ok(Some(row)) => ok_json(
            StatusCode::OK,
            json!({ "session": row_to_meta_json(&row), "ringBuffer": Value::Null }),
        ),
        Ok(None) => error_json(StatusCode::NOT_FOUND, "Terminal session not found"),
        Err(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to get terminal session",
        ),
    }
}

#[derive(Deserialize)]
pub struct ActionQuery {
    action: Option<String>,
}

#[derive(Deserialize)]
struct WriteBody {
    data: String,
}

pub async fn terminal_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<ActionQuery>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let (manager, _db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    match query.action.as_deref() {
        // v4 kill/signal both deliver through ptyManager.kill; the P4.1c
        // manager's kill is SIGTERM (its documented signal — see the module
        // note there on interactive shells).
        Some("kill") | Some("signal") => {
            manager.kill(&id);
            ok_json(StatusCode::OK, json!({ "ok": true }))
        }
        Some("write") => {
            let body: WriteBody = match serde_json::from_slice(&body) {
                Ok(b) => b,
                Err(e) => {
                    return error_json(StatusCode::BAD_REQUEST, &format!("Invalid request: {e}"))
                }
            };
            if manager.write(&id, &body.data) {
                ok_json(StatusCode::OK, json!({ "ok": true }))
            } else {
                error_json(StatusCode::NOT_FOUND, "Terminal session not found")
            }
        }
        _ => error_json(
            StatusCode::BAD_REQUEST,
            "Missing or invalid action parameter",
        ),
    }
}

pub async fn terminal_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    let (manager, db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    if !manager.contains(&id) {
        return error_json(StatusCode::NOT_FOUND, "Terminal session not found");
    }
    // v4: kill (the exit sequence posts the close announcement + persists the
    // exit state), then delete the row.
    manager.kill(&id);
    let sid = id.clone();
    let deleted = db
        .write(move |writers| {
            let repo = TerminalSessionsRepository::new(writers.main().connection());
            repo.delete(&sid)
        })
        .await;
    match deleted {
        Ok(_) => ok_json(StatusCode::OK, json!({ "ok": true })),
        Err(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete terminal session",
        ),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/terminals/{id}/stream — the WebSocket
// ---------------------------------------------------------------------------

pub async fn terminal_stream(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> AxumResponse {
    let (manager, _db) = match manager_and_db(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    ws.on_upgrade(move |socket| handle_ws(socket, manager, id))
}

async fn handle_ws(mut socket: WebSocket, manager: Arc<TerminalManager>, id: String) {
    // v4 ws.ts: unknown session → send the exit frame, close 1000.
    if !manager.contains(&id) {
        let frame = serde_json::to_string(&WsServerMessage::Exit {
            code: Some(-1),
            signal: Some("session_not_found".into()),
        })
        .unwrap_or_default();
        let _ = socket.send(Message::Text(frame.into())).await;
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "Session not found".into(),
            })))
            .await;
        return;
    }
    let Some(mut sub) = manager.subscribe(&id) else {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "Failed to subscribe".into(),
            })))
            .await;
        return;
    };

    loop {
        tokio::select! {
            server_msg = sub.rx.recv() => match server_msg {
                Some(msg) => {
                    let text = match serde_json::to_string(&msg) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                    // After the exit frame the manager closes the channel; the
                    // next recv returns None and the loop ends.
                }
                None => break,
            },
            client_msg = socket.recv() => match client_msg {
                Some(Ok(Message::Text(text))) => {
                    // v4 does a bare JSON.parse + type check; a malformed
                    // message is swallowed (the route's catch).
                    match serde_json::from_str::<WsClientMessage>(&text) {
                        Ok(WsClientMessage::Input { data }) => {
                            let _ = manager.write(&id, &data);
                        }
                        Ok(WsClientMessage::Resize { cols, rows }) => {
                            manager.resize(&id, cols, rows);
                        }
                        Ok(WsClientMessage::Ping) => {
                            let pong = serde_json::to_string(&WsServerMessage::Pong)
                                .unwrap_or_default();
                            if socket.send(Message::Text(pong.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => { /* swallowed, v4's catch */ }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => { /* binary/ping/pong frames ignored */ }
                Some(Err(_)) => break,
            },
        }
    }
    manager.unsubscribe(&id, sub.id);
}
