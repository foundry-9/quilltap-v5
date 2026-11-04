//! P4.D355 (v4 `85c002822`, bug 268) — both terminal routes end a session
//! with `terminate`: SIGHUP, then SIGKILL after the 3 s grace. A shell that
//! IGNORES SIGHUP (bash ignores SIGTERM, the old signal) is the vector: `POST
//! ?action=kill` answers `{ok:true}` at once and the session's `exitedAt`
//! lands after the grace with the escalation WARN; `DELETE` likewise, and the
//! row is gone. Also pins the route's `[Terminals API] Killing session` DEBUG
//! (Tier 2 item 20 — a P4.2-era absence).
//!
//! Its own binary: the escalation WARN fires on a detached THREAD, so the
//! capture is a process-global subscriber this binary installs first.
//!
//! Run:
//!   cargo test -p quilltap-web --test terminal_terminate_routes

#![cfg(unix)]

mod common;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

const CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";

fn lines() -> &'static Arc<Mutex<Vec<String>>> {
    use tracing_subscriber::layer::SubscriberExt;
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(quilltap_core::test_support::CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn lines_with(needle: &str, session_id: &str) -> Vec<String> {
    lines()
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(needle) && l.contains(&format!("sessionId={session_id}")))
        .cloned()
        .collect()
}

async fn spawn_hup_ignoring(client: &reqwest::Client, addr: std::net::SocketAddr) -> String {
    let resp = client
        .post(format!("http://{addr}/api/v1/terminals"))
        .header("content-type", "application/json")
        .body(json!({"chatId": CHAT_ID, "shell": "/bin/sh"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let id = body["session"]["id"].as_str().unwrap().to_string();
    let resp = client
        .post(format!("http://{addr}/api/v1/terminals/{id}?action=write"))
        .header("content-type", "application/json")
        .body(json!({"data": "exec sh -c 'trap \"\" HUP; exec sleep 30'\n"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(Duration::from_millis(600)).await;
    id
}

async fn exited_at(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    id: &str,
) -> Option<Value> {
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals/{id}"))
        .send()
        .await
        .unwrap();
    if resp.status() != 200 {
        return None;
    }
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    Some(body["session"]["exitedAt"].clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn kill_and_delete_end_a_shell_that_ignores_sighup() {
    lines();
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();

    // --- POST ?action=kill ---
    let id = spawn_hup_ignoring(&client, addr).await;
    let t0 = std::time::Instant::now();
    let resp = client
        .post(format!("http://{addr}/api/v1/terminals/{id}?action=kill"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), r#"{"ok":true}"#);
    assert!(
        t0.elapsed() < Duration::from_secs(1),
        "the route answers at once"
    );
    assert_eq!(
        lines_with("[Terminals API] Killing session", &id),
        [format!(
            "DEBUG quilltap::terminal [Terminals API] Killing session module=terminals-api sessionId={id}"
        )]
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        exited_at(&client, addr, &id).await,
        Some(Value::Null),
        "alive through the SIGHUP"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while exited_at(&client, addr, &id).await == Some(Value::Null) {
        assert!(tokio::time::Instant::now() < deadline, "never escalated");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        lines_with("[PTY] Session ignored SIGHUP; sending SIGKILL", &id),
        [format!(
            "WARN quilltap::terminal [PTY] Session ignored SIGHUP; sending SIGKILL module=pty-manager \
             sessionId={id} graceMs=3000"
        )]
    );

    // --- DELETE /terminals/{id} ---
    let id = spawn_hup_ignoring(&client, addr).await;
    let resp = client
        .delete(format!("http://{addr}/api/v1/terminals/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while lines_with("[PTY] Session ignored SIGHUP; sending SIGKILL", &id).is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "DELETE never escalated"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals?chatId={CHAT_ID}"))
        .send()
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert!(
        !body["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == id.as_str() && s["exitedAt"].is_null()),
        "the deleted session is not live: {body}"
    );
}

async fn spawn_shell(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    chat_id: &str,
) -> String {
    let resp = client
        .post(format!("http://{addr}/api/v1/terminals"))
        .header("content-type", "application/json")
        .body(json!({"chatId": chat_id, "shell": "/bin/sh"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    body["session"]["id"].as_str().unwrap().to_string()
}

async fn sessions_of(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    chat_id: &str,
) -> Vec<Value> {
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals?chatId={chat_id}"))
        .send()
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    body["sessions"].as_array().cloned().unwrap_or_default()
}

/// P4.D355 (contract C5 + round ruling R-O, v4 `85c002822` bug 267): deleting
/// a chat with a LIVE terminal — through the engine's `chatDelete` verb AND
/// through the REST `DELETE /api/v1/chats/{id}` edge — ends the shell
/// (`[PTY] Terminated sessions of deleted chat {chatId, count}`), drops it from
/// the manager, and leaves no `terminal_sessions` row for the chat.
#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_chat_ends_its_live_terminals_on_both_doors() {
    lines();
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();

    for (door, chat) in [
        ("dispatch", "c860cf74-128f-4a81-9a5c-6c2275f24302"),
        ("rest", "c89ee722-b555-40db-9e02-bbd0e82ee56b"),
    ] {
        let id = spawn_shell(&client, addr, chat).await;
        assert_eq!(
            sessions_of(&client, addr, chat).await.len(),
            1,
            "[{door}] the plant"
        );
        let resp = match door {
            "dispatch" => client
                .post(format!("http://{addr}/api/dispatch"))
                .header("content-type", "application/json")
                .body(json!({"type": "chatDelete", "chatId": chat}).to_string())
                .send()
                .await
                .unwrap(),
            _ => client
                .delete(format!("http://{addr}/api/v1/chats/{chat}"))
                .send()
                .await
                .unwrap(),
        };
        assert_eq!(
            resp.status(),
            200,
            "[{door}] {}",
            resp.text().await.unwrap()
        );
        let info: Vec<String> = lines()
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.contains("[PTY] Terminated sessions of deleted chat") && l.contains(chat))
            .cloned()
            .collect();
        assert_eq!(
            info,
            [format!(
                "INFO quilltap::terminal [PTY] Terminated sessions of deleted chat module=pty-manager \
                 chatId={chat} count=1"
            )],
            "[{door}]"
        );
        // Gone from the manager, and (R-O) no row for the chat.
        let resp = client
            .get(format!("http://{addr}/api/v1/terminals/{id}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            404,
            "[{door}] the session left the manager and the table"
        );
        assert!(
            sessions_of(&client, addr, chat).await.is_empty(),
            "[{door}]"
        );
    }
}
