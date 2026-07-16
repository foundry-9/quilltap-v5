//! The IPC transport contract suite (tier 4 #2): the corpus
//! `quilltap-web/tests/contract.rs` proves over real HTTP, replayed through
//! the Tauri IPC surface, asserting identical `Response` bodies and an
//! ordered, identical `Event` trace. Where a corpus case is HTTP-specific
//! (status codes on /api/dispatch), the §1 IPC mapping is asserted instead —
//! IPC carries no status, the envelope alone is authoritative; the `health`
//! command carries the numeric status explicitly (`{status, body}`).
//!
//! Mechanism (the order's allowed fallback, recorded in the lane close):
//! the command cores are driven directly over a real booted
//! `StartupStatus` (`dispatch_inner`/`health_inner`/`events_pump::attach`/
//! `protocol::handle_qtap_request`), with a `tauri::test::mock_builder` app
//! supplying the real event system (emit → listener) for the §2 trace. One
//! case additionally goes through `tauri::test::get_ipc_response` to pin
//! the command-name + argument-key wiring (`dispatch`, key `request`).

mod common;

use std::sync::{Arc, Mutex};

use quilltap_core::api::Event;
use quilltap_core::clock::now_unix_ms;
use quilltap_core::dbkey;
use quilltap_core::services::chat_events::{ChatEvent, StatusPayload};
use quilltap_core::services::creation_progress::CreationProgressFrame;
use quilltap_tauri::{commands, events_pump, protocol};
use quilltap_web::{build_router, StartupStatus};
use serde_json::{json, Value};

/// A mock app for the real event system (emit → listener). The §2 pump and
/// the §4 channels do not need a webview — only an `AppHandle`.
fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app")
}

async fn dispatch(state: &quilltap_web::SharedState, body: Value) -> Value {
    commands::dispatch_inner(state, body).await
}

/// --- the contract.rs `dispatch_health_and_sse_contract` mirror ---
#[tokio::test(flavor = "multi_thread")]
async fn dispatch_health_and_event_contract() {
    let base = common::materialize_bare_instance();
    let state = common::boot_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    });

    // --- health command: ready → {status: 200, body: the /health JSON} ---
    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 200);
    assert_eq!(health["body"]["status"], "healthy");
    assert!(health["body"]["uptime"].is_number());
    assert!(health["body"]["timestamp"].is_string());

    // --- dispatch: health round trip (typed envelope; no status in IPC) ---
    let body = dispatch(&state, json!({"type": "health"})).await;
    assert_eq!(body["type"], "health");
    assert_eq!(body["data"]["ready"], true);
    assert_eq!(body["data"]["pepperState"], "needs-vault-storage");

    // --- dispatch: malformed request → the typed BadRequest envelope
    // (HTTP's 400 is transport-specific; the envelope is the contract) ---
    let body = dispatch(&state, json!({"type": "no-such-action"})).await;
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "bad-request");

    // --- dispatch: chatSend with no driver → the internal envelope
    // (HTTP's 500 is transport-specific) ---
    let body = dispatch(
        &state,
        json!({"type": "chatSend", "chatId": "nope", "content": "hi"}),
    )
    .await;
    assert_eq!(body["data"]["message"], "chat dispatch not assembled");

    // --- §3: the placeholder page and /setup ride the qtap protocol ---
    let router = build_router(Arc::clone(&state));
    let index = protocol::handle_qtap_request(
        router.clone(),
        http::Request::builder()
            .method("GET")
            .uri("qtap://localhost/")
            .body(Vec::new())
            .unwrap(),
    )
    .await;
    assert_eq!(index.status(), 200);
    assert!(String::from_utf8_lossy(index.body()).contains("Quilltap"));
    let setup = protocol::handle_qtap_request(
        router.clone(),
        http::Request::builder()
            .method("GET")
            .uri("qtap://localhost/setup")
            .body(Vec::new())
            .unwrap(),
    )
    .await;
    assert_eq!(setup.status(), 200);

    // --- §3: GET /health through the protocol — the delegated router
    // answers, and every response carries permissive CORS ---
    let resp = protocol::handle_qtap_request(
        router.clone(),
        http::Request::builder()
            .method("GET")
            .uri("qtap://localhost/health")
            .header("origin", "tauri://localhost")
            .body(Vec::new())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    let body: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(body["status"], "healthy");

    // --- §3: CORS preflight answered without touching the router ---
    let preflight = protocol::handle_qtap_request(
        router.clone(),
        http::Request::builder()
            .method("OPTIONS")
            .uri("qtap://localhost/api/dispatch")
            .header("origin", "tauri://localhost")
            .header("access-control-request-method", "POST")
            .body(Vec::new())
            .unwrap(),
    )
    .await;
    assert_eq!(preflight.status(), 204);
    assert_eq!(
        preflight
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "*"
    );
    assert!(preflight
        .headers()
        .contains_key("access-control-allow-methods"));

    // --- §2: the ordered event trace — Green-Room backlog BEFORE live
    // frames, payloads byte-identical to the SSE `data:` payload ---
    let app = mock_app();
    let handle = app.handle();
    let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    {
        use tauri::Listener;
        let sink = Arc::clone(&collected);
        handle.listen(events_pump::EVENT_CHANNEL, move |event| {
            sink.lock()
                .expect("collector lock")
                .push(event.payload().to_string());
        });
    }

    // Seed a Green-Room backlog frame BEFORE attaching (the D6 replay).
    let host = state.host().unwrap();
    let frame = CreationProgressFrame::Status {
        message: "Warming the parlour lamps…".into(),
        ts: 1,
    };
    host.core()
        .creation_progress_bus()
        .publish("prog-1", &frame, now_unix_ms());
    let expected_backlog =
        serde_json::to_string(&Event::creation_progress("prog-1", frame)).unwrap();

    let pump = events_pump::EventPump::default();
    events_pump::attach(handle, &state, &pump).unwrap();

    // The subscription exists as soon as attach returns (subscribe happens
    // inline, BEFORE the snapshot — the ordering rule), so this live send
    // cannot be missed.
    let live = Event::chat(
        "chat-1",
        ChatEvent::status(StatusPayload {
            stage: "streaming".into(),
            message: "The wire hums.".into(),
            tool_name: None,
            character_name: None,
            character_id: None,
        }),
    );
    let expected_live = serde_json::to_string(&live).unwrap();
    host.core().event_sender().send(live).unwrap();

    // The forwarder emits on the Tauri async runtime; poll for both frames.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        {
            let got = collected.lock().expect("collector lock");
            if got.len() >= 2 {
                // Backlog first, live second — and both payloads are exactly
                // the SSE `data:` JSON (same serde serialization).
                assert_eq!(got[0], expected_backlog);
                assert_eq!(got[1], expected_live);
                break;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no event frames arrived: {:?}",
            collected.lock().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    // The live payload matches the corpus' expected frame JSON, field-for-field.
    let payload: Value = serde_json::from_str(&collected.lock().unwrap()[1]).unwrap();
    assert_eq!(
        payload,
        json!({
            "chatId": "chat-1",
            "status": {"stage": "streaming", "message": "The wire hums."}
        })
    );

    // --- a second attach replaces the forwarder (webview reload): the
    // still-active Green-Room backlog replays FIRST (the D6 rule — an SSE
    // reopen replays it the same way), then live frames, with no
    // duplicated forwarder doubling anything ---
    let before = collected.lock().unwrap().len();
    events_pump::attach(handle, &state, &pump).unwrap();
    let live2 = Event::chat(
        "chat-2",
        ChatEvent::status(StatusPayload {
            stage: "streaming".into(),
            message: "Still one wire.".into(),
            tool_name: None,
            character_name: None,
            character_id: None,
        }),
    );
    let expected_live2 = serde_json::to_string(&live2).unwrap();
    host.core().event_sender().send(live2).unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let n = collected.lock().unwrap().len();
        if n >= before + 2 {
            // Let a straggling duplicate show itself, then assert exactly
            // backlog-replay + one live frame arrived, in order.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let got = collected.lock().unwrap();
            assert_eq!(got.len(), before + 2);
            assert_eq!(got[before], expected_backlog);
            assert_eq!(got[before + 1], expected_live2);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no frames after re-attach: {:?}",
            collected.lock().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// --- the contract.rs `setup_flow_end_to_end` mirror (the transport-visible
/// arc; the seed-row assertions are engine behavior already proven over
/// HTTP and are not re-proven here) ---
#[tokio::test(flavor = "multi_thread")]
async fn setup_flow_contract() {
    // A truly empty data dir: no main DB, no .dbkey, no env pepper.
    let base = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(base.path().join("data")).unwrap();
    let state = common::boot_instance(base.path(), |mut c| {
        c.env_pepper = None;
        c.terminal = false;
        c
    });

    // health → {status: 423, body: locked/needs-setup}.
    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 423);
    assert_eq!(health["body"]["status"], "locked");
    assert_eq!(health["body"]["dbKeyState"], "needs-setup");

    // unlockState reports needs-setup.
    let body = dispatch(&state, json!({"type": "unlockState"})).await;
    assert_eq!(body["data"]["state"], "needs-setup");

    // A ready-gated dispatch is still refused — the Locked envelope with
    // v4's setup body merged alongside (HTTP's 503 is transport-specific).
    let body = dispatch(&state, json!({"type": "listChats"})).await;
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "locked");
    assert_eq!(body["error"], "Setup required");
    assert_eq!(body["setupUrl"], "/setup");

    // setup mints a pepper, provisions, and unlocks.
    let body = dispatch(&state, json!({"type": "setup", "passphrase": ""})).await;
    assert_eq!(body["type"], "setup");
    let pepper = body["data"]["pepper"].as_str().unwrap();
    assert!(!pepper.is_empty());
    assert!(base.path().join("data").join("quilltap.db").exists());
    assert!(base.path().join("data").join("quilltap.dbkey").exists());

    // health flips to 200; listChats answers [].
    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 200);
    let body = dispatch(&state, json!({"type": "listChats"})).await;
    assert_eq!(body["type"], "chats");
    assert_eq!(body["data"], json!([]));

    // A second setup refuses (already set up).
    let body = dispatch(&state, json!({"type": "setup", "passphrase": ""})).await;
    assert_eq!(body["data"]["kind"], "bad-request");
}

/// --- the contract.rs `locked_vault_contract` mirror ---
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
    let state = common::boot_instance(base.path(), |mut c| {
        c.env_pepper = None;
        c.terminal = false;
        c
    });

    // health → {status: 423, body: locked/needs-passphrase}.
    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 423);
    assert_eq!(health["body"]["status"], "locked");
    assert_eq!(health["body"]["dbKeyState"], "needs-passphrase");

    // A ready-gated dispatch → the Locked envelope with v4's setup body
    // MERGED alongside the typed envelope (auth.ts:98–105).
    let body = dispatch(&state, json!({"type": "listChats"})).await;
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "locked");
    assert_eq!(body["error"], "Setup required");
    assert_eq!(body["setupUrl"], "/setup");
    assert_eq!(body["pepperState"], "needs-passphrase");

    // Wrong passphrase → the BadRequest envelope.
    let body = dispatch(&state, json!({"type": "unlock", "passphrase": "wrong"})).await;
    assert_eq!(body["data"]["message"], "Invalid passphrase");

    // Right passphrase → unlockState resolved; health flips to 200.
    let body = dispatch(
        &state,
        json!({"type": "unlock", "passphrase": "open sesame"}),
    )
    .await;
    assert_eq!(body["type"], "unlockState");
    assert_eq!(body["data"]["state"], "resolved");
    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 200);
}

/// --- the §1 boot-failure arm: a Failed startup resolves the dispatch.rs
/// 503 arm's BODY (never rejects), and health carries the 503 vocabulary ---
#[tokio::test(flavor = "multi_thread")]
async fn boot_failure_contract() {
    let state = quilltap_web::web_state(
        StartupStatus::Failed {
            message: "the boiler burst".into(),
        },
        "0.0.0-test".into(),
        std::env::temp_dir(),
        None,
    );

    let body = commands::dispatch_inner(&state, json!({"type": "health"})).await;
    assert_eq!(body["type"], "error");
    assert_eq!(body["data"]["kind"], "internal");
    assert_eq!(body["data"]["message"], "the boiler burst");

    let health = commands::health_inner(&state).await;
    assert_eq!(health["status"], 503);
    assert_eq!(health["body"]["status"], "unhealthy");
    assert_eq!(health["body"]["startupPhase"], "failed");
    assert_eq!(health["body"]["error"], "the boiler burst");

    // events_attach mirrors the SSE route's refusal.
    let app = mock_app();
    let pump = events_pump::EventPump::default();
    let err = events_pump::attach(app.handle(), &state, &pump).unwrap_err();
    assert_eq!(err, "server failed to start");
}

/// --- the command-layer wiring proof: `dispatch` by NAME with argument key
/// `request` through the real invoke pipeline (`get_ipc_response`), so the
/// direct-drive cases above are known to sit behind the right wiring ---
#[tokio::test(flavor = "multi_thread")]
async fn invoke_wiring_contract() {
    let base = common::materialize_bare_instance();
    let state = common::boot_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    });

    let app = tauri::test::mock_builder()
        .manage(Arc::clone(&state))
        .manage(events_pump::EventPump::default())
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::health,
            commands::events_attach,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview");

    // The LOCAL webview origin (platform-specific, per Tauri 2): app
    // commands from a local origin need no capability entry; a remote
    // origin would hit the ACL.
    let local_origin = if cfg!(any(windows, target_os = "android")) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "dispatch".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: local_origin.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(json!({"request": {"type": "health"}})),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .expect("dispatch resolves");
    let body: Value = response.deserialize().unwrap();
    assert_eq!(body["type"], "health");
    assert_eq!(body["data"]["ready"], true);

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "health".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: local_origin.parse().unwrap(),
            body: tauri::ipc::InvokeBody::default(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .expect("health resolves");
    let body: Value = response.deserialize().unwrap();
    assert_eq!(body["status"], 200);
    assert_eq!(body["body"]["status"], "healthy");
}
