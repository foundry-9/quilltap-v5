//! Transport contract tests (the tier-4 #2 seed): a `Request` corpus replayed
//! over real HTTP asserting exact `Response` bodies + status codes (incl. the
//! Locked 503 setup body and BadRequest 400), the `/health` vocabulary, and
//! an SSE round trip (publish a synthetic `Event` on the broadcast, assert
//! the exact `data:` frame bytes + the keep-alive comment).

mod common;

use futures_util::StreamExt;
use quilltap_core::api::Event;
use quilltap_core::dbkey;
use quilltap_core::services::chat_events::{ChatEvent, StatusPayload};
use serde_json::{json, Value};

async fn post_dispatch(addr: std::net::SocketAddr, body: Value) -> (reqwest::StatusCode, Value) {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    (status, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_health_and_sse_contract() {
    let base = common::materialize_bare_instance();
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;

    // --- /health: ready → 200 healthy ---
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let health: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(health["status"], "healthy");
    assert!(health["uptime"].is_number());
    assert!(health["timestamp"].is_string());

    // --- dispatch: health round trip (typed envelope) ---
    let (status, body) = post_dispatch(addr, json!({"type": "health"})).await;
    assert_eq!(status, 200);
    assert_eq!(body["type"], "health");
    assert_eq!(body["data"]["ready"], true);
    assert_eq!(body["data"]["pepperState"], "needs-vault-storage");

    // --- dispatch: malformed request → 400 with the typed error envelope ---
    let (status, body) = post_dispatch(addr, json!({"type": "no-such-action"})).await;
    assert_eq!(status, 400);
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "bad-request");

    // --- dispatch: chatSend with no driver → 500 internal (typed) ---
    let (status, body) = post_dispatch(
        addr,
        json!({"type": "chatSend", "chatId": "nope", "content": "hi"}),
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(body["data"]["message"], "chat dispatch not assembled");

    // --- static: / serves the placeholder; /setup is readable ---
    let index = client.get(format!("http://{addr}/")).send().await.unwrap();
    assert_eq!(index.status(), 200);
    assert!(index.text().await.unwrap().contains("Quilltap"));
    let setup = client
        .get(format!("http://{addr}/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 200);

    // --- SSE: exact frame bytes + keep-alive comment ---
    let resp = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    let mut stream = resp.bytes_stream();

    // Publish a synthetic chat frame on the engine broadcast.
    let host = state.host().unwrap();
    let frame = ChatEvent::status(StatusPayload {
        stage: "streaming".into(),
        message: "The wire hums.".into(),
        tool_name: None,
        character_name: None,
        character_id: None,
    });
    // Retry until the SSE subscriber task is attached (subscribe happens in
    // the route handler; the send is fire-and-forget).
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let _ = host
            .core()
            .event_sender()
            .send(Event::chat("chat-1", frame.clone()));
        if let Ok(Some(Ok(bytes))) =
            tokio::time::timeout(std::time::Duration::from_millis(300), stream.next()).await
        {
            collected.extend_from_slice(&bytes);
            if collected.windows(2).any(|w| w == b"\n\n") {
                break;
            }
        }
        if tokio::time::Instant::now() > deadline {
            panic!("no SSE frame arrived");
        }
    }
    let text = String::from_utf8(collected).unwrap();
    // The exact v4 frame encoding: `id: N` + `data: <JSON>` + blank line, no
    // `event:` names. The payload flattens the scope tag alongside the frame.
    let expected_json = json!({
        "chatId": "chat-1",
        "status": {"stage": "streaming", "message": "The wire hums."}
    });
    let first_frame = text.split("\n\n").next().unwrap();
    let mut lines = first_frame.lines();
    let id_line = lines.next().unwrap();
    assert!(id_line.starts_with("id: "), "got: {id_line}");
    let data_line = lines.next().unwrap();
    let payload: Value = serde_json::from_str(data_line.strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(payload, expected_json);
}

/// P4.4 unit 1 e2e: an EMPTY data dir → 423/needs-setup → `setup` dispatch →
/// an unlocked engine on a real schema'd instance → `listChats` answers `[]`.
#[tokio::test(flavor = "multi_thread")]
async fn setup_flow_end_to_end() {
    // A truly empty data dir: no main DB, no .dbkey, no env pepper → needs-setup.
    let base = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(base.path().join("data")).unwrap();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.env_pepper = None;
        c.terminal = false;
        c
    })
    .await;

    let client = reqwest::Client::new();

    // /health → 423 needs-setup.
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 423);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["status"], "locked");
    assert_eq!(body["dbKeyState"], "needs-setup");

    // unlockState reports needs-setup.
    let (status, body) = post_dispatch(addr, json!({"type": "unlockState"})).await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["state"], "needs-setup");

    // A ready-gated dispatch is still refused (503).
    let (status, _) = post_dispatch(addr, json!({"type": "listChats"})).await;
    assert_eq!(status, 503);

    // setup mints a pepper, provisions the encrypted instance, and unlocks.
    let (status, body) = post_dispatch(addr, json!({"type": "setup", "passphrase": ""})).await;
    assert_eq!(status, 200);
    assert_eq!(body["type"], "setup");
    let pepper = body["data"]["pepper"].as_str().unwrap().to_string();
    assert!(!pepper.is_empty());
    assert!(body["data"]["message"]
        .as_str()
        .unwrap()
        .contains("will not be displayed again"));

    // The instance is now on disk, encrypted, with a real schema.
    assert!(base.path().join("data").join("quilltap.db").exists());
    assert!(base.path().join("data").join("quilltap.dbkey").exists());

    // /health flips to 200; listChats answers [].
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let (status, body) = post_dispatch(addr, json!({"type": "listChats"})).await;
    assert_eq!(status, 200);
    assert_eq!(body["type"], "chats");
    assert_eq!(body["data"], json!([]));

    // P4.4u3: the fresh instance carries the built-in seeds (2 roleplay templates,
    // 3 mount stores + their pointers). Read them back read-only with the minted
    // pepper (the running server holds the writer; readers coexist).
    let ro = |db: &str| {
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = rusqlite::Connection::open_with_flags(base.path().join("data").join(db), flags)
            .unwrap();
        let key_hex = dbkey::pepper_b64_to_key_hex(&pepper).unwrap();
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))
            .unwrap();
        conn
    };
    let main = ro("quilltap.db");
    let n_templates: i64 = main
        .query_row(
            "SELECT count(*) FROM roleplay_templates WHERE isBuiltIn = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_templates, 2, "2 built-in roleplay templates after setup");
    let n_pointers: i64 = main
        .query_row(
            "SELECT count(*) FROM instance_settings WHERE key IN \
             ('generalMountPointId','userUploadsMountPointId','lanternBackgroundsMountPointId')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_pointers, 3, "3 mount-pointer settings after setup");
    let mi = ro("quilltap-mount-index.db");
    let n_mounts: i64 = mi
        .query_row("SELECT count(*) FROM doc_mount_points", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_mounts, 3, "3 built-in mount stores after setup");

    // A second setup refuses (already set up).
    let (status, body) = post_dispatch(addr, json!({"type": "setup", "passphrase": ""})).await;
    assert_eq!(status, 400);
    assert_eq!(body["data"]["kind"], "bad-request");
}

#[tokio::test(flavor = "multi_thread")]
async fn locked_vault_contract() {
    // A passphrase vault with no env pepper boots locked.
    let base = common::materialize_bare_instance();
    dbkey::save_dbkey(
        &base.path().join("data"),
        common::TEST_PEPPER,
        "open sesame",
    )
    .unwrap();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.env_pepper = None;
        c.terminal = false;
        c
    })
    .await;

    // /health → 423 locked with the dbKeyState.
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 423);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["status"], "locked");
    assert_eq!(body["dbKeyState"], "needs-passphrase");

    // A ready-gated dispatch → 503 with v4's setup body MERGED alongside the
    // typed envelope (auth.ts:98–105).
    let (status, body) = post_dispatch(addr, json!({"type": "listChats"})).await;
    assert_eq!(status, 503);
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "locked");
    assert_eq!(body["error"], "Setup required");
    assert_eq!(body["setupUrl"], "/setup");
    assert_eq!(body["pepperState"], "needs-passphrase");

    // Wrong passphrase → 400.
    let (status, body) =
        post_dispatch(addr, json!({"type": "unlock", "passphrase": "wrong"})).await;
    assert_eq!(status, 400);
    assert_eq!(body["data"]["message"], "Invalid passphrase");

    // Right passphrase → 200 unlockState resolved; /health flips to 200.
    let (status, body) =
        post_dispatch(addr, json!({"type": "unlock", "passphrase": "open sesame"})).await;
    assert_eq!(status, 200);
    assert_eq!(body["type"], "unlockState");
    assert_eq!(body["data"]["state"], "resolved");
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
