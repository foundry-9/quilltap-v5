//! Terminal surface integration (the P4.1c test lineage, over the HTTP/WS
//! routes): spawn via REST (the Ariel session-opened announcement lands),
//! attach the WebSocket (replay + meta), input → output, ping → pong, resize,
//! exit frame on shell exit, and the unknown-session close semantics
//! (v4 `lib/terminal/ws.ts`).
//!
//! Run:
//!   cargo test -p quilltap-web --test terminal_ws

mod common;

use futures_util::{SinkExt, StreamExt};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test(flavor = "multi_thread")]
async fn terminal_rest_and_ws_round_trip() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();

    // A fixture chat for the Ariel announcement target.
    const CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";

    // --- spawn (POST /api/v1/terminals) ---
    let resp = client
        .post(format!("http://{addr}/api/v1/terminals"))
        .header("content-type", "application/json")
        .body(
            json!({
                "chatId": CHAT_ID,
                "label": "smoke shell",
                "shell": "/bin/sh",
                "cols": 100,
                "rows": 30,
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let session_id = body["session"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["session"]["chatId"], CHAT_ID);
    assert_eq!(body["session"]["shell"], "/bin/sh");

    // --- list (GET ?chatId=) ---
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals?chatId={CHAT_ID}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert!(body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == session_id.as_str()));

    // Missing chatId → 400 (v4 badRequest).
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // --- item GET: live session carries the ring buffer ---
    let resp = client
        .get(format!("http://{addr}/api/v1/terminals/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["session"]["id"], session_id.as_str());
    assert!(body["ringBuffer"].is_string());

    // Unknown id → 404 (v4 notFound('Terminal session')).
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/terminals/00000000-0000-4000-8000-000000000000"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["error"], "Terminal session not found");

    // --- WebSocket attach ---
    let (mut ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/v1/terminals/{session_id}/stream"
    ))
    .await
    .expect("ws connect");

    // The attach replay ends with the meta frame (an output frame precedes it
    // when the ring buffer already has bytes).
    let mut saw_meta = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !saw_meta {
        assert!(tokio::time::Instant::now() < deadline, "no meta frame");
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await
        {
            let v: Value = serde_json::from_str(&text).unwrap();
            if v["type"] == "meta" {
                assert_eq!(v["meta"]["id"], session_id.as_str());
                saw_meta = true;
            }
        }
    }

    // --- ping → pong (verbatim protocol bytes) ---
    ws.send(Message::Text(r#"{"type":"ping"}"#.into()))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        assert!(tokio::time::Instant::now() < deadline, "no pong");
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await
        {
            if text == r#"{"type":"pong"}"# {
                break;
            }
        }
    }

    // --- resize (fire-and-forget) + input → output round trip ---
    ws.send(Message::Text(
        r#"{"type":"resize","cols":120,"rows":40}"#.into(),
    ))
    .await
    .unwrap();
    ws.send(Message::Text(
        json!({"type": "input", "data": "echo qt_ws_$((20+22))\r"}).to_string(),
    ))
    .await
    .unwrap();
    let mut echoed = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    while !echoed.contains("qt_ws_42") {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no echoed output; got: {echoed:?}"
        );
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await
        {
            let v: Value = serde_json::from_str(&text).unwrap();
            if v["type"] == "output" {
                echoed.push_str(v["data"].as_str().unwrap_or_default());
            }
        }
    }

    // --- shell exit → the exit frame ---
    ws.send(Message::Text(
        json!({"type": "input", "data": "exit\r"}).to_string(),
    ))
    .await
    .unwrap();
    let mut exit_frame: Option<Value> = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    while exit_frame.is_none() {
        assert!(tokio::time::Instant::now() < deadline, "no exit frame");
        match tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let v: Value = serde_json::from_str(&text).unwrap();
                if v["type"] == "exit" {
                    exit_frame = Some(v);
                }
            }
            Ok(None) => break,
            _ => {}
        }
    }
    let exit = exit_frame.expect("exit frame");
    assert_eq!(exit["code"], 0);
    let _ = ws.close(None).await;

    // --- unknown-session WS: the exit frame then close 1000 (v4 ws.ts) ---
    let (mut ws2, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/v1/terminals/00000000-0000-4000-8000-000000000000/stream"
    ))
    .await
    .unwrap();
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), ws2.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match first {
        Message::Text(text) => {
            assert_eq!(
                text.as_str(),
                r#"{"type":"exit","code":-1,"signal":"session_not_found"}"#
            );
        }
        other => panic!("expected the exit frame, got {other:?}"),
    }
    let second = tokio::time::timeout(std::time::Duration::from_secs(5), ws2.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match second {
        Message::Close(Some(frame)) => {
            assert_eq!(u16::from(frame.code), 1000);
            assert_eq!(frame.reason.to_string(), "Session not found");
        }
        other => panic!("expected close 1000, got {other:?}"),
    }

    // --- the Ariel session-opened announcement landed (the P4.1c handoff) ---
    // Lock the engine (drop the writers), reopen, read the row.
    let lock = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(json!({"type": "lock"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(lock.status(), 200);

    let data = base.path().join("data");
    let db = Db::open(
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: Some(data.join("quilltap-mount-index.db")),
            llm_logs: Some(data.join("quilltap-llm-logs.db")),
        },
        common::TEST_PEPPER,
    )
    .unwrap();
    let sid = session_id.clone();
    let announcement: Option<String> = db
        .read_main(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT content FROM chat_messages WHERE chatId = ?1 \
                     AND systemSender = 'ariel' AND systemKind = 'session-opened' \
                     AND content LIKE '%' || ?2 || '%' LIMIT 1",
                    rusqlite::params![CHAT_ID, sid],
                    |row| row.get(0),
                )
                .ok())
        })
        .unwrap();
    let content = announcement.expect("the Ariel session-opened row");
    assert!(content.contains("Ariel has opened a terminal"));
    assert!(content.contains("smoke shell"));
}
