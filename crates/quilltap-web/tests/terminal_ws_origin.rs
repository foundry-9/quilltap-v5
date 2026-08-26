//! P4.D124 — the terminal WebSocket's same-origin gate, on the wire.
//!
//! `terminal_ws_origin_equivalence` pins the accept/refuse TABLE against v4's
//! real `authenticateUpgrade`. What this pins is the plumbing that differential
//! cannot see: that the header actually reaches the gate, that the gate fires
//! where v4's does (AFTER the session-exists check, so an unknown session still
//! gets its `session_not_found` frame rather than a policy close), and that a
//! refusal closes with **1008** — v4 completes the upgrade and then closes,
//! which is the observable shape reproduced here.
//!
//! Run:
//!   cargo test -p quilltap-web --test terminal_ws_origin

mod common;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

const CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";

/// Connect with an explicit `Origin` header (or none), returning the first
/// frame the server sends.
async fn first_frame(
    addr: &std::net::SocketAddr,
    session_id: &str,
    origin: Option<&str>,
) -> Message {
    let mut request = format!("ws://{addr}/api/v1/terminals/{session_id}/stream")
        .into_client_request()
        .unwrap();
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
    }
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
        .await
        .expect("a frame within 5s")
        .expect("a frame")
        .expect("no ws error")
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_ws_same_origin_gate() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://{addr}/api/v1/terminals"))
        .header("content-type", "application/json")
        .body(
            json!({ "chatId": CHAT_ID, "label": "origin gate", "shell": "/bin/sh",
                    "cols": 80, "rows": 24 })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let session_id = body["session"]["id"].as_str().unwrap().to_string();

    // --- CROSS-ORIGIN: refused with 1008, before any PTY data ---
    match first_frame(&addr, &session_id, Some("http://evil.example")).await {
        Message::Close(Some(frame)) => {
            assert_eq!(u16::from(frame.code), 1008, "policy violation");
            assert_eq!(frame.reason.to_string(), "Unauthorized");
        }
        other => panic!("expected a 1008 close, got {other:?}"),
    }

    // A different PORT on the same host is cross-origin too.
    match first_frame(&addr, &session_id, Some("http://127.0.0.1:65000")).await {
        Message::Close(Some(frame)) => assert_eq!(u16::from(frame.code), 1008),
        other => panic!("expected a 1008 close, got {other:?}"),
    }

    // --- NO Origin (a non-browser client) is accepted: the first frame is the
    //     session meta, not a close ---
    match first_frame(&addr, &session_id, None).await {
        Message::Text(text) => {
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(v["type"], "close", "an accepted upgrade streams");
        }
        other => panic!("expected a data frame, got {other:?}"),
    }

    // --- SAME origin is accepted ---
    let same = format!("http://{addr}");
    match first_frame(&addr, &session_id, Some(&same)).await {
        Message::Text(_) => {}
        other => panic!("expected a data frame for a same-origin upgrade, got {other:?}"),
    }

    // --- ORDER: v4 answers `session_not_found` BEFORE the origin gate, so an
    //     unknown session cross-origin still gets the exit frame + 1000 ---
    match first_frame(
        &addr,
        "00000000-0000-4000-8000-000000000000",
        Some("http://evil.example"),
    )
    .await
    {
        Message::Text(text) => assert_eq!(
            text.as_str(),
            r#"{"type":"exit","code":-1,"signal":"session_not_found"}"#,
            "the session check runs first, as it does in v4"
        ),
        other => panic!("expected the exit frame, got {other:?}"),
    }
}
