//! **The P4.4u2b chat-create smoke** (always-on in CI): a scripted
//! `POST /api/dispatch` `chatCreate` against the committed fixture instance
//! through the running axum transport — real HTTP dispatch → the created chat
//! → the Green-Room `creationProgress` replay on a late `/api/events`
//! subscriber.
//!
//! Hermetic: the create spine runs the IDENTICAL production composition
//! ([`ChatCreateSpine`] is generic over the model boundaries only) but over the
//! smoke's canned providers — the streaming provider answers the auto-greeting
//! with a fixed reply, the completion provider answers `{}` (the outfit
//! `default` path never calls it anyway), and the embedding provider fails (the
//! first-message-context search legs degrade per the ported fallbacks). No
//! network.

mod common;

use std::future::Future;
use std::sync::Arc;

use futures_util::StreamExt;
use quilltap_core::api::Event;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{StreamChunk, StreamChunkResult, StreamParams, StreamUsage};
use quilltap_core::services::creation_progress::CreationProgressBus;
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::{ChatCreateSpine, ChatSpine, SpineBundle, SpineFactory};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::{json, Value};

/// The fixture's `Friday` LLM character (controlledBy `llm`, no vault
/// `firstMessage` → the auto-greeting runs; the canned stream answers it).
const FRIDAY_CHARACTER_ID: &str = "969a21df-73ad-465f-8f6c-2bc52d90ddc6";
/// The fixture's ANTHROPIC connection profile (apiKeyId `None` → the greeting
/// path proceeds with an empty key, straight to the canned stream — no network).
const ANTHROPIC_PROFILE_ID: &str = "a1cb6066-2fcc-4006-a7db-ddb2fc6e2221";
const PROGRESS_ID: &str = "green-room-e2e";
const GREETING_REPLY: &str = "Ahoy — the players are ready.";

// ---------------------------------------------------------------------------
// Test providers (the canned model boundaries)
// ---------------------------------------------------------------------------

struct AnyStream;
impl quilltap_core::model::stream::StreamingCompletionProvider for AnyStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            let _ = tx
                .send(Ok(StreamChunk::content(GREETING_REPLY.to_string())))
                .await;
            let _ = tx
                .send(Ok(StreamChunk::done(Some(StreamUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }))))
                .await;
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
        version: "0.0.0-create".to_string(),
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

/// The create smoke's factory: the production [`ChatSpine`] + [`ChatCreateSpine`]
/// over the canned providers (Arcs shared, mirroring `ProductionSpineFactory`).
struct CreateSpineFactory {
    base_dir: std::path::PathBuf,
}

impl SpineFactory for CreateSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        _terminal: Option<Arc<quilltap_host::terminal::TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> SpineBundle {
        let embedding = Arc::new(CannedEmbeddingProvider::new());
        let completion = Arc::new(AnyCompletion);
        let streaming = Arc::new(AnyStream);
        let spine = Arc::new(ChatSpine {
            db: db.clone(),
            events: events.clone(),
            embedding: Arc::clone(&embedding),
            completion: Arc::clone(&completion),
            streaming: Arc::clone(&streaming),
            pricing: Arc::new(PricingFetcher::new(NoPricingFetch)),
            tz: "UTC".to_string(),
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
        });
        SpineBundle {
            chat_send: spine,
            chat_create,
            swipe_generate: None,
            provider_actions: None,
            memory_embedding: None,
            courier_resolve: None,
            save_image_bytes: None,
            image_generation: None,
            consult: None,
            brahma_console_send: None,
            recall_replay: None,
            announcement_preview: None,
            // P4.9E3A: canned test factory — no tool runner, so `run-tool`
            // answers the loud not-assembled refusal.
            operator_tool_runner: None,
            regenerate_title: None,
            outfit_llm_choose: None,
            // P4.9E4A: no vision-describe runner in the smoke assembly — the
            // attach ladder then resolves to '' (v4's own any-failure arm).
            image_describe: None,
            web_search: None,
            job_handlers: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The smoke
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn chat_create_end_to_end() {
    let base = common::materialize_fixture_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(CreateSpineFactory { base_dir }));
        c
    })
    .await;

    let client = reqwest::Client::new();

    // Dispatch the create: a single LLM character (Friday) + its ANTHROPIC
    // profile, default outfits (mode omitted), a Green-Room progressId.
    let dispatch = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(
            json!({
                "type": "chatCreate",
                "title": "Green Room E2E",
                "progressId": PROGRESS_ID,
                "participants": [{
                    "type": "CHARACTER",
                    "characterId": FRIDAY_CHARACTER_ID,
                    "connectionProfileId": ANTHROPIC_PROFILE_ID,
                }],
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch.status(), 200, "create dispatch failed");
    let body: Value = serde_json::from_str(&dispatch.text().await.unwrap()).unwrap();
    assert_eq!(body["type"], "chatCreate", "unexpected envelope: {body}");
    let chat = &body["data"]["chat"];
    let chat_id = chat["id"].as_str().expect("chat.id present").to_string();
    assert!(!chat_id.is_empty());
    // The 201 body's participants are the enriched summaries (array present).
    assert!(
        chat["participants"].is_array(),
        "chat.participants missing: {chat}"
    );
    assert_eq!(
        chat["participants"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        1,
        "expected one enriched participant: {chat}"
    );

    // The created chat is now listed (the row landed in the DB).
    let list = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(json!({"type": "listChats"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body: Value = serde_json::from_str(&list.text().await.unwrap()).unwrap();
    let listed = list_body["data"]
        .as_array()
        .expect("listChats data array")
        .iter()
        .any(|c| c["id"].as_str() == Some(chat_id.as_str()));
    assert!(listed, "created chat not in listChats: {list_body}");

    // Open a LATE /api/events subscriber (AFTER the create) — the Green-Room
    // bus replays the buffered backlog for our progressId.
    let sse = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(sse.status(), 200);
    let mut sse_stream = sse.bytes_stream();

    let mut buffer: Vec<u8> = Vec::new();
    let mut frames: Vec<Value> = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut saw_status = false;
    let mut saw_done = false;
    'collect: loop {
        #[allow(clippy::single_match)]
        match tokio::time::timeout(std::time::Duration::from_secs(2), sse_stream.next()).await {
            Ok(Some(Ok(bytes))) => {
                buffer.extend_from_slice(&bytes);
                let text = String::from_utf8_lossy(&buffer).to_string();
                frames.clear();
                for block in text.split("\n\n") {
                    for line in block.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(v) = serde_json::from_str::<Value>(data) {
                                frames.push(v);
                            }
                        }
                    }
                }
                for f in &frames {
                    if f["progressId"].as_str() != Some(PROGRESS_ID) {
                        continue;
                    }
                    match f["kind"].as_str() {
                        Some("status") => saw_status = true,
                        Some("done") => saw_done = true,
                        _ => {}
                    }
                }
                if saw_status && saw_done {
                    break 'collect;
                }
            }
            _ => {}
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Green-Room replay incomplete; frames so far: {frames:#?}");
        }
    }
    assert!(saw_status, "no creationProgress status frame replayed");
    assert!(saw_done, "no terminal creationProgress done frame replayed");

    // Confirm the chat row + its opening ASSISTANT message landed in the DB.
    let lock = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(json!({"type": "lock"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(lock.status(), 200);
    drop(state);

    let data = base.path().join("data");
    let db = Db::open(
        quilltap_core::db::runtime::DbPaths {
            main: data.join("quilltap.db"),
            mount_index: Some(data.join("quilltap-mount-index.db")),
            llm_logs: Some(data.join("quilltap-llm-logs.db")),
        },
        common::TEST_PEPPER,
    )
    .unwrap();
    let cid = chat_id.clone();
    // `messageCount` is a REAL-affinity column (bound f64; see `db::chats`).
    let (title, message_count): (String, f64) = db
        .read_main(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT title, messageCount FROM chats WHERE id = ?1",
                    rusqlite::params![cid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("created chat row present"))
        })
        .unwrap();
    assert_eq!(title, "Green Room E2E");
    // The auto-greeting ASSISTANT message was written (visible count ≥ 1).
    assert!(
        message_count >= 1.0,
        "no opening messages: count={message_count}"
    );
}
