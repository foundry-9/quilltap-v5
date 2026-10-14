//! P4.D301 — bug 241's submission ledger, end to end: the real HTTP dispatch →
//! the production [`ChatSpine`] (`run_send`) → `process_message_with_ledger` →
//! the error frame on `/api/events`, the dispatch rejection's
//! `data.submission`, the DB rows, and the log lines.
//!
//! v4 (`93c5af6ad`, `orchestrator.service.ts:188-381/750-828/2178-2222`) keeps a
//! per-request `SubmissionLedger` that `processMessage` flips right after the
//! user row (and the staged TOOL rows) land; `handleStreamError` logs it and
//! spreads it onto the error frame as `userMessagePersisted` /
//! `pendingToolResultsPersisted`. v5 says the same on the frame AND on the
//! rejection (`CoreError.submission`), because v5's frame rides the global Event
//! channel and races the dispatch reply. The frame bytes themselves are pinned
//! against v4's REAL `encodeErrorEvent` by the harness's
//! `chat_error_frame_equivalence`; this file proves the ledger's VALUES at each
//! failure point through the production composition — the chat_send_smoke
//! recipe (the committed orchestrator corpus instance, canned providers).
//!
//! The capture is PROCESS-wide (the `memory_housekeeping_handler.rs` template):
//! the spine runs the turn on its own thread, which a thread-scoped rig cannot
//! see. Tests serialize on the capture lock.
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_send_submission_ledger

mod common;

use std::future::Future;
use std::sync::{Arc, Mutex, Once};

use futures_util::StreamExt;
use quilltap_core::api::Event;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{StreamChunkResult, StreamError, StreamParams};
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::{ChatCreateSpine, ChatSpine, SpineBundle, SpineFactory};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::{json, Value};

/// The fixture's `single_basic` chat (one LLM character + a user participant —
/// NOT multi-character, so the fair-rotation guard never pauses it).
const CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";
const STREAM_FAILURE: &str = "the stream fell over mid-reply";
/// What the spine's `e.to_string()` reads for [`STREAM_FAILURE`]: v5's primary
/// stream wraps the provider's text (pre-existing, outside bug 241's span —
/// both the frame's `details` and the rejection's `message` carry it).
const STREAM_FAILURE_DETAILS: &str = "primary stream failed: the stream fell over mid-reply";

// ---------------------------------------------------------------------------
// The process-global capture (every thread, while armed).
// ---------------------------------------------------------------------------

static LINES: Mutex<Option<Vec<String>>> = Mutex::new(None);
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct GlobalLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GlobalLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut guard = LINES.lock().unwrap_or_else(|p| p.into_inner());
        let Some(buf) = guard.as_mut() else {
            return;
        };
        let meta = event.metadata();
        let mut visitor = quilltap_core::test_support::FieldVisitor(format!(
            "{} {}",
            meta.level(),
            meta.target()
        ));
        event.record(&mut visitor);
        buf.push(visitor.0);
    }
}

fn install() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(GlobalLayer))
            .expect("this binary owns its global subscriber");
    });
}

async fn arm() -> tokio::sync::MutexGuard<'static, ()> {
    install();
    let serial = SERIAL.lock().await;
    *LINES.lock().unwrap() = Some(Vec::new());
    serial
}

fn disarm() -> Vec<String> {
    LINES.lock().unwrap().take().unwrap_or_default()
}

fn lines_with<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
    lines.iter().filter(|l| l.contains(needle)).collect()
}

fn only_line(lines: &[String], needle: &str) -> String {
    let hits = lines_with(lines, needle);
    assert_eq!(hits.len(), 1, "expected exactly one {needle:?}: {lines:#?}");
    hits[0].clone()
}

// ---------------------------------------------------------------------------
// Canned providers.
// ---------------------------------------------------------------------------

/// A streaming provider whose every call fails on its first read — AFTER the
/// orchestrator has saved whatever it saves before the model call.
struct FailingStream;
impl quilltap_core::model::stream::StreamingCompletionProvider for FailingStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.send(Err(StreamError::new(STREAM_FAILURE))).await;
            rx
        }
    }
}

struct AnyCompletion;
impl CompletionProvider for AnyCompletion {
    #[allow(clippy::manual_async_fn)]
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        async move {
            Ok(CompletionResponse {
                content: "{}".to_string(),
                usage: None,
                finish_reason: None,
                attachment_results: None,
                cache_usage: None,
            })
        }
    }
}

struct NoPricingFetch;
impl PricingFetch for NoPricingFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        None
    }
    fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
        None
    }
    fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
        None
    }
}

fn test_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: "0.0.0-ledger".to_string(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// The production [`ChatSpine`] over the canned providers; the same spine also
/// backs the Brahma Console send (R-L1's three-key silence leg).
struct LedgerSpineFactory {
    base_dir: std::path::PathBuf,
}

impl SpineFactory for LedgerSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        _terminal: Option<Arc<quilltap_host::terminal::TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
    ) -> SpineBundle {
        let embedding = Arc::new(CannedEmbeddingProvider::new());
        let completion = Arc::new(AnyCompletion);
        let streaming = Arc::new(FailingStream);
        let spine = Arc::new(ChatSpine {
            db: db.clone(),
            events: events.clone(),
            embedding: Arc::clone(&embedding),
            completion: Arc::clone(&completion),
            streaming: Arc::clone(&streaming),
            pricing: Arc::new(PricingFetcher::new(NoPricingFetch)),
            tz: "UTC".to_string(),
            display_zone: quilltap_core::host_zone::TimeZone::UTC,
            env: test_env(),
            file_bytes: Arc::new(ProductionFileBytes {
                db: db.clone(),
                backend: Arc::new(LocalStorageBackend::new(self.base_dir.join("files"))),
                codec: Arc::new(HostImageCodec),
            }),
            image_transcoder: Arc::new(HostImageCodec),
            scrollback: None,
            consult: None,
            web_search: None,
            image_describe: None,
        });
        let chat_create = Arc::new(ChatCreateSpine {
            db: db.clone(),
            events: events.clone(),
            bus: Arc::clone(bus),
            pepper: pepper.to_string(),
            data_dir: data_dir.to_path_buf(),
            embedding,
            completion,
            streaming,
            tz: "UTC".to_string(),
            display_zone: quilltap_core::host_zone::TimeZone::UTC,
        });
        SpineBundle {
            chat_send: Arc::clone(&spine) as _,
            chat_create,
            swipe_generate: None,
            provider_actions: None,
            memory_embedding: None,
            courier_resolve: None,
            save_image_bytes: None,
            image_generation: None,
            consult: None,
            brahma_console_send: Some(Arc::clone(&spine) as _),
            help_chat_send: None,
            recall_replay: None,
            announcement_preview: None,
            in_scene_voice: None,
            operator_tool_runner: None,
            regenerate_title: None,
            outfit_llm_choose: None,
            image_describe: None,
            web_search: None,
            search_providers: Vec::new(),
            job_handlers: Vec::new(),
            generators_detail: None,
            generators_wizard: None,
            scenario_builder: None,
        }
    }
}

// ---------------------------------------------------------------------------
// The rig: serve, dispatch, collect the error frame, read the DB.
// ---------------------------------------------------------------------------

struct Outcome {
    status: u16,
    envelope: Value,
    /// The chat's error frame, re-serialized from the SSE line in its OWN key
    /// order (serde_json's `preserve_order`), with the envelope's `chatId` tag
    /// removed — i.e. v4's `encodeErrorEvent` body.
    frame_body: String,
    lines: Vec<String>,
    /// `(id, role, content, createdAt)` of every row the send added.
    new_rows: Vec<(String, String, String, String)>,
}

fn open_db(base: &std::path::Path) -> Db {
    let data = base.join("data");
    Db::open(
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: Some(data.join("quilltap-mount-index.db")),
            llm_logs: Some(data.join("quilltap-llm-logs.db")),
        },
        common::TEST_PEPPER,
    )
    .unwrap()
}

fn rows_of(db: &Db, chat_id: &str) -> Vec<(String, String, String, String)> {
    let cid = chat_id.to_string();
    db.read_main(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, createdAt FROM chat_messages \
                 WHERE chatId = ?1 ORDER BY rowid",
            )
            .unwrap();
        Ok(stmt
            .query_map(rusqlite::params![cid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect())
    })
    .unwrap()
}

/// Materialize the corpus instance, let `prepare` adjust it (keys, chat type),
/// serve it, open `/api/events`, dispatch `request`, collect the chat's error
/// frame, lock, and read back the rows the send added.
async fn run_failing_send(
    prepare: impl FnOnce(&std::path::Path),
    chat_id: &str,
    request: Value,
) -> Outcome {
    let base = common::materialize_fixture_instance();
    prepare(base.path());
    let before: Vec<String> = {
        let db = open_db(base.path());
        rows_of(&db, chat_id).into_iter().map(|r| r.0).collect()
    };
    let base_dir = base.path().to_path_buf();
    let (addr, state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(LedgerSpineFactory { base_dir }));
        // A served-host test otherwise dies exit 1 after 60 s (memory note
        // `served-host-heartbeat-and-message-field`).
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let client = reqwest::Client::new();
    let sse = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(sse.status(), 200);
    let mut sse_stream = sse.bytes_stream();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let dispatch = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap();
    let status = dispatch.status().as_u16();
    let envelope: Value = serde_json::from_str(&dispatch.text().await.unwrap()).unwrap();

    // The frame was sent BEFORE the rejection returned (the spine's order), so
    // it is already on the stream; read until it shows.
    let mut buffer: Vec<u8> = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let frame_body = loop {
        if let Ok(Some(Ok(bytes))) =
            tokio::time::timeout(std::time::Duration::from_secs(2), sse_stream.next()).await
        {
            buffer.extend_from_slice(&bytes);
        }
        let text = String::from_utf8_lossy(&buffer).to_string();
        let found = text.split("\n\n").flat_map(str::lines).find_map(|line| {
            let data = line.strip_prefix("data: ")?;
            let v: Value = serde_json::from_str(data).ok()?;
            (v.get("errorType").is_some()
                && v.get("chatId").and_then(Value::as_str) == Some(chat_id))
            .then_some(v)
        });
        if let Some(mut v) = found {
            v.as_object_mut().unwrap().shift_remove("chatId");
            break serde_json::to_string(&v).unwrap();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no error frame arrived; stream so far: {text}"
        );
    };

    let lock = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(json!({"type": "lock"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(lock.status(), 200);
    drop(state);
    let lines = disarm();

    let db = open_db(base.path());
    let new_rows = rows_of(&db, chat_id)
        .into_iter()
        .filter(|r| !before.contains(&r.0))
        .collect();
    drop(base);
    Outcome {
        status,
        envelope,
        frame_body,
        lines,
        new_rows,
    }
}

fn send(content: &str) -> Value {
    json!({ "type": "chatSend", "chatId": CHAT_ID, "content": content })
}

fn five_key_frame(details: &str, user: bool, tools: bool) -> String {
    serde_json::to_string(&json!({
        "error": "Failed to generate response",
        "errorType": "Error",
        "details": details,
        "userMessagePersisted": user,
        "pendingToolResultsPersisted": tools,
    }))
    .unwrap()
}

/// The ERROR line's two flags (R-L4: P4.18's text, v4's two fields after
/// `chat_id`) and v4's `[Orchestrator] Emitting error frame` DEBUG, in v4's
/// order: the ERROR first, then the DEBUG.
fn assert_shell_lines(lines: &[String], error_text: &str, user: bool, tools: bool) {
    let error_line = only_line(lines, error_text);
    assert!(
        error_line.starts_with("ERROR quilltap::chat"),
        "{error_line}"
    );
    assert!(
        error_line.contains(&format!(
            "chat_id={CHAT_ID} userMessagePersisted={user} pendingToolResultsPersisted={tools}"
        )),
        "{error_line}"
    );
    let debug_line = only_line(lines, "[Orchestrator] Emitting error frame");
    assert!(debug_line.starts_with("DEBUG"), "{debug_line}");
    assert!(
        debug_line.ends_with(&format!(
            "[Orchestrator] Emitting error frame chatId={CHAT_ID} errorType=Error \
             userMessagePersisted={user} pendingToolResultsPersisted={tools}"
        )),
        "{debug_line}"
    );
    let at = |n: &str| lines.iter().position(|l| l.contains(n)).unwrap();
    assert!(
        at(error_text) < at("[Orchestrator] Emitting error frame"),
        "v4's order"
    );
}

fn assert_rejection(o: &Outcome, message: &str, user: bool, tools: bool) {
    assert_eq!(o.status, 500, "{}", o.envelope);
    assert_eq!(o.envelope["type"], "error", "{}", o.envelope);
    assert_eq!(o.envelope["data"]["message"], message, "{}", o.envelope);
    assert_eq!(
        serde_json::to_string(&o.envelope["data"]["submission"]).unwrap(),
        format!(r#"{{"userMessagePersisted":{user},"pendingToolResultsPersisted":{tools}}}"#),
        "{}",
        o.envelope
    );
}

// ---------------------------------------------------------------------------
// The rows.
// ---------------------------------------------------------------------------

/// A requires-key provider with no key: refused BEFORE any write → `{false,
/// false}`, no USER / TOOL row, and NO flip DEBUG (the silence leg).
#[tokio::test(flavor = "multi_thread")]
async fn a_send_refused_before_any_write_says_nothing_was_saved() {
    let _serial = arm().await;
    let o = run_failing_send(|_| {}, CHAT_ID, send("Hello before the key check")).await;
    let refusal = "No API key configured for this connection profile";
    assert_rejection(&o, refusal, false, false);
    assert_eq!(o.frame_body, five_key_frame(refusal, false, false));
    assert!(o.new_rows.is_empty(), "nothing written: {:?}", o.new_rows);
    assert!(lines_with(&o.lines, "[Orchestrator] User message persisted").is_empty());
    assert!(lines_with(&o.lines, "[Orchestrator] Pending tool results persisted").is_empty());
    assert_shell_lines(
        &o.lines,
        "Chat send failed while streaming the initial turn",
        false,
        false,
    );
}

/// A stream that fails AFTER the user row landed → `{true, false}`, the USER
/// row present, v4's `User message persisted` DEBUG carrying the row's id, and
/// no TOOL-rows DEBUG.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_failure_after_the_user_row_says_the_message_was_saved() {
    let _serial = arm().await;
    let o = run_failing_send(
        common::bind_profile_keys,
        CHAT_ID,
        send("Hello after the key check"),
    )
    .await;
    assert_rejection(&o, STREAM_FAILURE_DETAILS, true, false);
    assert_eq!(
        o.frame_body,
        five_key_frame(STREAM_FAILURE_DETAILS, true, false)
    );
    let users: Vec<_> = o.new_rows.iter().filter(|r| r.1 == "USER").collect();
    assert_eq!(users.len(), 1, "{:?}", o.new_rows);
    assert_eq!(users[0].2, "Hello after the key check");
    assert!(o.new_rows.iter().all(|r| r.1 != "TOOL"), "{:?}", o.new_rows);
    let flip = only_line(&o.lines, "[Orchestrator] User message persisted chatId=");
    assert!(flip.starts_with("DEBUG"), "{flip}");
    assert!(
        flip.contains(&format!("chatId={CHAT_ID} userMessageId=")),
        "{flip}"
    );
    let id = flip.rsplit("userMessageId=").next().unwrap().trim();
    assert_eq!(id, users[0].0, "the DEBUG names the saved row");
    assert!(lines_with(&o.lines, "[Orchestrator] Pending tool results persisted").is_empty());
    assert!(lines_with(&o.lines, "(fair-rotation pause)").is_empty());
    assert_shell_lines(
        &o.lines,
        "Chat send failed while streaming the initial turn",
        true,
        false,
    );
}

/// The same with two staged tool results → `{true, true}`: both TOOL rows
/// (each carrying its chip's `createdAt` verbatim — the SPA's R-L5 fallback
/// keys on it), the USER row, and the `count: 2` DEBUG, once.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_failure_after_staged_tool_rows_says_both_were_saved() {
    let _serial = arm().await;
    let chip = |n: u32| {
        json!({
            "tool": "rng",
            "success": true,
            "result": format!("Rolled {n}"),
            "prompt": format!("roll {n}"),
            "arguments": {"type": "dice", "count": n},
            "createdAt": format!("2026-10-14T12:00:0{n}.000Z"),
        })
    };
    let mut req = send("Hello with two rolls");
    req["pendingToolResults"] = json!([chip(1), chip(2)]);
    let o = run_failing_send(common::bind_profile_keys, CHAT_ID, req).await;
    assert_rejection(&o, STREAM_FAILURE_DETAILS, true, true);
    assert_eq!(
        o.frame_body,
        five_key_frame(STREAM_FAILURE_DETAILS, true, true)
    );
    let tools: Vec<_> = o.new_rows.iter().filter(|r| r.1 == "TOOL").collect();
    assert_eq!(tools.len(), 2, "{:?}", o.new_rows);
    assert_eq!(tools[0].3, "2026-10-14T12:00:01.000Z");
    assert_eq!(tools[1].3, "2026-10-14T12:00:02.000Z");
    assert_eq!(
        o.new_rows.iter().filter(|r| r.1 == "USER").count(),
        1,
        "{:?}",
        o.new_rows
    );
    let flip = only_line(&o.lines, "[Orchestrator] Pending tool results persisted");
    assert!(flip.starts_with("DEBUG"), "{flip}");
    assert!(
        flip.ends_with(&format!(
            "[Orchestrator] Pending tool results persisted chatId={CHAT_ID} count=2"
        )),
        "{flip}"
    );
    // The TOOL rows land (and flip) before the user row (v4's order).
    let at = |n: &str| o.lines.iter().position(|l| l.contains(n)).unwrap();
    assert!(
        at("[Orchestrator] Pending tool results persisted")
            < at("[Orchestrator] User message persisted chatId=")
    );
    assert_shell_lines(
        &o.lines,
        "Chat send failed while streaming the initial turn",
        true,
        true,
    );
}

/// A continue-mode send writes no user row and no TOOL rows, so its failure
/// says `{false, false}` with no flip DEBUG — pinning R-L2's claim that the
/// chained turns (every one continue-mode) never flip a ledger they are not
/// handed.
#[tokio::test(flavor = "multi_thread")]
async fn a_continue_mode_failure_never_flips_the_ledger() {
    let _serial = arm().await;
    let o = run_failing_send(
        common::bind_profile_keys,
        CHAT_ID,
        json!({ "type": "chatSend", "chatId": CHAT_ID, "content": "", "continueMode": true }),
    )
    .await;
    assert_rejection(&o, STREAM_FAILURE_DETAILS, false, false);
    assert_eq!(
        o.frame_body,
        five_key_frame(STREAM_FAILURE_DETAILS, false, false)
    );
    assert!(
        o.new_rows.iter().all(|r| r.1 != "USER" && r.1 != "TOOL"),
        "{:?}",
        o.new_rows
    );
    assert!(lines_with(&o.lines, "[Orchestrator] User message persisted").is_empty());
    assert!(lines_with(&o.lines, "[Orchestrator] Pending tool results persisted").is_empty());
    assert_shell_lines(
        &o.lines,
        "Chat send failed while streaming the initial turn",
        false,
        false,
    );
}

/// R-L1's silence leg: the Brahma Console's failure frame stays v4's THREE-key
/// `encodeErrorEvent(message, 'fatal_error', '')` (v4 passes no extras there),
/// its rejection carries no `submission`, and no ledger line is logged.
#[tokio::test(flavor = "multi_thread")]
async fn the_brahma_console_failure_frame_stays_three_keys() {
    let _serial = arm().await;
    let o = run_failing_send(
        |base| {
            common::bind_profile_keys(base);
            let w = quilltap_core::db::Writer::open_writable(
                &base.join("data").join("quilltap.db"),
                common::TEST_PEPPER,
            )
            .unwrap();
            w.connection()
                .execute(
                    "UPDATE chats SET chatType = 'brahma' WHERE id = ?1",
                    rusqlite::params![CHAT_ID],
                )
                .unwrap();
        },
        CHAT_ID,
        json!({ "type": "brahmaConsoleSend", "chatId": CHAT_ID, "content": "Hello, Brahma" }),
    )
    .await;
    assert_eq!(o.envelope["type"], "error", "{}", o.envelope);
    assert!(
        o.envelope["data"].get("submission").is_none(),
        "{}",
        o.envelope
    );
    let frame: Value = serde_json::from_str(&o.frame_body).unwrap();
    let keys: Vec<&str> = frame
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["error", "errorType", "details"], "{}", o.frame_body);
    assert_eq!(frame["errorType"], "fatal_error");
    assert!(lines_with(&o.lines, "[Orchestrator] Emitting error frame").is_empty());
    assert!(
        lines_with(&o.lines, "Persisted=").is_empty(),
        "{:#?}",
        o.lines
    );
}

/// Tier 2 (item 15): the fair-rotation pause. The corpus instance carries no
/// room the human drives two seats of, so its two-LLM-seat room (`c89ee722`)
/// has BOTH seats turned user-controlled: the poster is the first, and the only
/// other seat — the next speaker, with no random draw to make — is a user seat.
/// A plain send then hits the fairness guard: the pause saves the user's message and
/// returns `Ok` — v4 `:376-381`'s flip + DEBUG at the CALLER. The pause cannot
/// be followed by a failure in the same request (it returns, and the chain's
/// guard stops on `is_paused`), so no consumer ever reads this ledger value;
/// the row pins v4's DEBUG (the flip's twin, one statement apart) and the
/// saved row, and that the turn neither reached the user-row site nor logged
/// its DEBUG.
#[tokio::test(flavor = "multi_thread")]
async fn the_fair_rotation_pause_logs_its_user_row_flip() {
    const ROOM: &str = "c89ee722-b555-40db-9e02-bbd0e82ee56b";
    let _serial = arm().await;
    let base = common::materialize_fixture_instance();
    common::bind_profile_keys(base.path());
    {
        let w = quilltap_core::db::Writer::open_writable(
            &base.path().join("data").join("quilltap.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        let parts: String = w
            .connection()
            .query_row(
                "SELECT participants FROM chats WHERE id = ?1",
                rusqlite::params![ROOM],
                |r| r.get(0),
            )
            .unwrap();
        let mut parts: Value = serde_json::from_str(&parts).unwrap();
        let seats = parts.as_array_mut().unwrap();
        let mut flipped = 0;
        for seat in seats.iter_mut() {
            if seat["controlledBy"] == "llm" {
                seat["controlledBy"] = json!("user");
                flipped += 1;
            }
        }
        assert_eq!(flipped, 2, "the corpus's two-LLM-seat room: {parts}");
        w.connection()
            .execute(
                "UPDATE chats SET participants = ?2 WHERE id = ?1",
                rusqlite::params![ROOM, parts.to_string()],
            )
            .unwrap();
    }
    let before: Vec<String> = rows_of(&open_db(base.path()), ROOM)
        .into_iter()
        .map(|r| r.0)
        .collect();
    let base_dir = base.path().to_path_buf();
    let (addr, state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(LedgerSpineFactory { base_dir }));
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let client = reqwest::Client::new();
    let dispatch = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(
            json!({ "type": "chatSend", "chatId": ROOM, "content": "Charlie waits a turn." })
                .to_string(),
        )
        .send()
        .await
        .unwrap();
    let status = dispatch.status().as_u16();
    let envelope: Value = serde_json::from_str(&dispatch.text().await.unwrap()).unwrap();
    let lock = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(json!({"type": "lock"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(lock.status(), 200);
    drop(state);
    let lines = disarm();

    assert_eq!(status, 200, "{envelope}");
    // The guard's result: no reply (`hasContent: false`); `isPaused` mirrors
    // the CHAT row, not the guard, so it is not the signal.
    assert_eq!(
        envelope["data"]["hasContent"], false,
        "the guard paused: {envelope}"
    );
    let new_rows: Vec<_> = rows_of(&open_db(base.path()), ROOM)
        .into_iter()
        .filter(|r| !before.contains(&r.0))
        .collect();
    let users: Vec<_> = new_rows.iter().filter(|r| r.1 == "USER").collect();
    assert_eq!(users.len(), 1, "{new_rows:?}");
    assert_eq!(users[0].2, "Charlie waits a turn.");
    assert!(
        new_rows.iter().all(|r| r.1 != "ASSISTANT"),
        "no reply: {new_rows:?}"
    );
    let flip = only_line(&lines, "(fair-rotation pause)");
    assert!(flip.starts_with("DEBUG"), "{flip}");
    assert!(
        flip.ends_with(&format!(
            "[Orchestrator] User message persisted (fair-rotation pause) chatId={ROOM}"
        )),
        "{flip}"
    );
    assert!(lines_with(&lines, "[Orchestrator] User message persisted chatId=").is_empty());
    assert!(lines_with(&lines, "[Orchestrator] Emitting error frame").is_empty());
}
