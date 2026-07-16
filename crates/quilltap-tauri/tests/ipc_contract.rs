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

    // --- §3: with NO embedded dist (noop assets), the placeholder page and
    // /setup fall through to the router over the qtap protocol — the same
    // degradation as the HTTP deployment with no `spa_dir` ---
    let app = mock_app();
    let router = build_router(Arc::clone(&state));
    let index = protocol::handle_qtap_request(
        app.handle(),
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
        app.handle(),
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
        app.handle(),
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
        app.handle(),
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

/// A valid 1×1 red PNG (69 bytes) — the `binary_routes.rs` staging.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

const SEEDED_IMG_ID: &str = "bbbbbbbb-2222-4222-8222-222222222222";

/// Seed one image into the library (the committed fixture builder never
/// creates a `files` table): a `files` row + its bytes under
/// `<base>/files/` — this lane's own staging, mirroring `binary_routes.rs`.
fn seed_image_file(base: &std::path::Path) {
    use quilltap_core::db::Writer;
    let files_root = base.join("files");
    std::fs::create_dir_all(files_root.join("test")).unwrap();
    std::fs::write(files_root.join("test/dot.png"), TINY_PNG).unwrap();

    let w =
        Writer::open_writable(&base.join("data").join("quilltap.db"), common::TEST_PEPPER).unwrap();
    w.connection()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS files (\
               id TEXT PRIMARY KEY, sha256 TEXT, originalFilename TEXT, mimeType TEXT, \
               size REAL, width REAL, height REAL, category TEXT, generationPrompt TEXT, \
               generationModel TEXT, generationRevisedPrompt TEXT, description TEXT, \
               storageKey TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
    w.connection()
        .execute(
            "INSERT INTO files (id, sha256, originalFilename, mimeType, size, width, height, category, storageKey, createdAt, updatedAt) \
             VALUES (?1, 'sha-png', 'dot.png', 'image/png', 69, 1, 1, 'IMAGE', 'test/dot.png', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            rusqlite::params![SEEDED_IMG_ID],
        )
        .unwrap();
}

/// --- §3 one-origin (P4.7c, dogfood finding #12): the SPA is served off
/// the qtap origin from the embedded `frontendDist`. A mock context with a
/// tiny in-memory dist stands in for the codegen-embedded assets (the real
/// resolver machinery — `AssetResolver` → `get_asset` with the SPA
/// fallback chain — runs for real); the server surface must stay the
/// router's, and a seeded library image round-trips its bytes through
/// `handle_qtap_request` so a server-relative `<img src="/api/v1/files/…">`
/// on a qtap page is proven end to end ---
#[tokio::test(flavor = "multi_thread")]
async fn one_origin_spa_serving_contract() {
    use std::borrow::Cow;
    use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};

    const INDEX_HTML: &str =
        "<!doctype html><html><head><title>Quilltap</title></head><body><app-root></app-root></body></html>";
    const MAIN_JS: &str = "console.log('one-origin');";

    struct SpaAssets;
    impl<R: tauri::Runtime> tauri::Assets<R> for SpaAssets {
        fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
            match key.as_ref() {
                "/index.html" => Some(Cow::Borrowed(INDEX_HTML.as_bytes())),
                "/main-CA2IZFBF.js" => Some(Cow::Borrowed(MAIN_JS.as_bytes())),
                _ => None,
            }
        }
        fn iter(&self) -> Box<AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(&self, _html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(SpaAssets))
        .expect("mock app with SPA assets");

    let base = common::materialize_fixture_instance();
    seed_image_file(base.path());
    let state = common::boot_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    });
    let router = build_router(Arc::clone(&state));

    let get = |path: &str| {
        http::Request::builder()
            .method("GET")
            .uri(format!("qtap://localhost{path}"))
            .body(Vec::new())
            .unwrap()
    };

    // GET / → the SPA index from the embedded dist.
    let resp = protocol::handle_qtap_request(app.handle(), router.clone(), get("/")).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body(), INDEX_HTML.as_bytes());
    assert!(resp
        .headers()
        .get("content-type")
        .is_some_and(|ct| ct.to_str().unwrap().contains("html")));

    // GET a hashed static asset → the asset bytes, script MIME.
    let resp =
        protocol::handle_qtap_request(app.handle(), router.clone(), get("/main-CA2IZFBF.js")).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body(), MAIN_JS.as_bytes());
    assert!(resp
        .headers()
        .get("content-type")
        .is_some_and(|ct| ct.to_str().unwrap().contains("javascript")));

    // A deep link (an Angular route, no such asset) → the index fallback:
    // a reload at qtap://localhost/salon lands back in the SPA router.
    let resp = protocol::handle_qtap_request(app.handle(), router.clone(), get("/salon")).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.body(), INDEX_HTML.as_bytes());

    // The server surface is NOT shadowed by the dist: /health still
    // reaches the delegated router.
    let resp = protocol::handle_qtap_request(app.handle(), router.clone(), get("/health")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = serde_json::from_slice(resp.body()).unwrap();
    assert_eq!(body["status"], "healthy");

    // The byte-route proof: an image seeded in the library (the
    // `binary_routes.rs` staging — the fixture builder never creates a
    // `files` table) fetched back through `handle_qtap_request`, the exact
    // path a server-relative <img src="/api/v1/files/…"> on a qtap page
    // takes (dogfood finding #12's broken avatar chip).
    let resp = protocol::handle_qtap_request(
        app.handle(),
        router,
        get(&format!("/api/v1/files/{SEEDED_IMG_ID}")),
    )
    .await;
    assert_eq!(resp.status(), 200, "byte route through qtap");
    assert_eq!(resp.body(), TINY_PNG);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
}

/// --- §4 (tier 2): the terminal paired IPC over the fixture lineage the
/// WS test uses — spawn rides the §3 protocol (the REST verbs are §3
/// surface), attach replays ring-buffer `output` + `meta` (the manager's
/// subscribe semantics, same as the WS), ping → pong on the paired
/// channel, input → live output, malformed swallowed, unknown session →
/// the session_not_found exit frame ---
#[tokio::test(flavor = "multi_thread")]
async fn terminal_paired_ipc_contract() {
    use quilltap_tauri::terminal_ipc;

    let base = common::materialize_fixture_instance();
    // Terminal driver ON (the default) — the WS test's configuration.
    let state = common::boot_instance(base.path(), |c| c);
    let router = build_router(Arc::clone(&state));

    // The fixture chat (the Ariel announcement target, same as terminal_ws).
    const CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";

    // --- spawn rides the §3 protocol: POST /api/v1/terminals ---
    let app = mock_app();
    let resp = protocol::handle_qtap_request(
        app.handle(),
        router.clone(),
        http::Request::builder()
            .method("POST")
            .uri("qtap://localhost/api/v1/terminals")
            .header("content-type", "application/json")
            .body(
                json!({
                    "chatId": CHAT_ID,
                    "label": "ipc shell",
                    "shell": "/bin/sh",
                    "cols": 100,
                    "rows": 30,
                })
                .to_string()
                .into_bytes(),
            )
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let body: Value = serde_json::from_slice(resp.body()).unwrap();
    let session_id = body["session"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["session"]["chatId"], CHAT_ID);

    // --- attach: the channel carries WsServerMessage JSON verbatim ---
    let frames: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&frames);
    let channel = tauri::ipc::Channel::new(move |body| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            sink.lock()
                .expect("frame sink lock")
                .push(serde_json::from_str(&json).expect("frame JSON"));
        }
        Ok(())
    });
    let attachments = terminal_ipc::TerminalAttachments::default();
    terminal_ipc::attach_inner(&state, &attachments, session_id.clone(), channel).unwrap();

    // The attach replay: one `output` frame (the ring buffer) then `meta`
    // (the manager's subscribe semantics — the WS route emits the same).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        {
            let got = frames.lock().unwrap();
            if got.len() >= 2 {
                assert_eq!(got[0]["type"], "output");
                assert!(got[0]["data"].is_string());
                assert_eq!(got[1]["type"], "meta");
                assert_eq!(got[1]["meta"]["id"], session_id.as_str());
                assert_eq!(got[1]["meta"]["chatId"], CHAT_ID);
                break;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no attach replay: {:?}",
            frames.lock().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // --- ping → pong on the paired channel ---
    terminal_ipc::send_inner(
        &state,
        &attachments,
        session_id.clone(),
        json!({"type": "ping"}),
    )
    .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if frames.lock().unwrap().iter().any(|f| f["type"] == "pong") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no pong: {:?}",
            frames.lock().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // --- input → live output through the PTY ---
    terminal_ipc::send_inner(
        &state,
        &attachments,
        session_id.clone(),
        json!({"type": "input", "data": "echo qtap-ipc-proof\r"}),
    )
    .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if frames.lock().unwrap().iter().any(|f| {
            f["type"] == "output"
                && f["data"]
                    .as_str()
                    .is_some_and(|d| d.contains("qtap-ipc-proof"))
        }) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no echoed output: {:?}",
            frames.lock().unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // --- a malformed client message is swallowed (v4's catch) ---
    terminal_ipc::send_inner(
        &state,
        &attachments,
        session_id.clone(),
        json!({"type": "nope"}),
    )
    .unwrap();

    // --- unknown session → the session_not_found exit frame ---
    let frames2: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink2 = Arc::clone(&frames2);
    let channel2 = tauri::ipc::Channel::new(move |body| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            sink2
                .lock()
                .expect("frame sink lock")
                .push(serde_json::from_str(&json).expect("frame JSON"));
        }
        Ok(())
    });
    terminal_ipc::attach_inner(&state, &attachments, "no-such-session".into(), channel2).unwrap();
    {
        let got2 = frames2.lock().unwrap();
        assert_eq!(got2.len(), 1, "exactly the exit frame: {got2:?}");
        assert_eq!(
            got2[0],
            json!({"type": "exit", "code": -1, "signal": "session_not_found"})
        );
    }

    // --- detach, then kill via the §3 REST verb (the WS-close + DELETE
    // shape the SPA uses) ---
    terminal_ipc::detach_inner(&state, &attachments, session_id.clone()).unwrap();
    let resp = protocol::handle_qtap_request(
        app.handle(),
        router,
        http::Request::builder()
            .method("DELETE")
            .uri(format!("qtap://localhost/api/v1/terminals/{session_id}"))
            .body(Vec::new())
            .unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 200);
}
