//! **The M2 smoke** (the P4.2 exit criterion, always-on in CI): a scripted
//! chat send against a fixture instance through the running axum transport —
//! real HTTP dispatch → the SSE trace → the DB state.
//!
//! The fixture is the committed v4-baked orchestrator corpus instance
//! (test-pepper-keyed; see `tests/common`). The spine bundle is constructed
//! with TEST providers through the IDENTICAL production composition
//! ([`ChatSpine`] is generic over the model boundaries only): the streaming
//! provider answers every call with a fixed content+done sequence, the
//! completion provider answers `{}` (the cheap-LLM parsers treat it as
//! nothing-found), and the embedding provider fails (the search legs degrade
//! per the ported fallbacks). Production swaps in the `ProviderIo` drivers
//! via `ProductionSpineFactory` — same type, different providers.

mod common;

use std::future::Future;
use std::sync::Arc;

use futures_util::StreamExt;
use quilltap_core::api::Event;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{StreamChunk, StreamChunkResult, StreamParams, StreamUsage};
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::{ChatCreateSpine, ChatSpine, SpineBundle, SpineFactory};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::{json, Value};

/// The fixture's `single_basic` chat (a single LLM character + a user
/// participant; the corpus drives it in continue mode).
const SMOKE_CHAT_ID: &str = "9fe3f87b-3833-46a9-bc1e-a88fc43dee6b";
const SMOKE_REPLY: &str = "Ahoy from the M2 smoke.";

// ---------------------------------------------------------------------------
// Test providers (the canned model boundaries)
// ---------------------------------------------------------------------------

/// A streaming provider that answers EVERY call with one fixed sequence —
/// the smoke asserts the transport plumbing, not prompt bytes (those are the
/// orchestrator differential's job).
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
                .send(Ok(StreamChunk::content(SMOKE_REPLY.to_string())))
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

/// A completion provider answering `{}` for every cheap-LLM call (the ported
/// parsers resolve it to nothing-found and the paths degrade gracefully).
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
        version: "0.0.0-smoke".to_string(),
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

/// The smoke's spine factory: the production [`ChatSpine`] over the canned
/// providers.
struct SmokeSpineFactory {
    base_dir: std::path::PathBuf,
}

impl SpineFactory for SmokeSpineFactory {
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
            job_handlers: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The smoke
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn m2_chat_send_end_to_end() {
    let base = common::materialize_fixture_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(SmokeSpineFactory { base_dir }));
        c
    })
    .await;

    let client = reqwest::Client::new();

    // Open the SSE stream FIRST so the turn's frames are observed live.
    let sse = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(sse.status(), 200);
    let mut sse_stream = sse.bytes_stream();
    // Give the SSE task a beat to subscribe before dispatching.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Dispatch the send (the corpus drives single_basic in continue mode —
    // a nudge; no new user message, the sole LLM character answers).
    let dispatch = client
        .post(format!("http://{addr}/api/dispatch"))
        .header("content-type", "application/json")
        .body(
            json!({
                "type": "chatSend",
                "chatId": SMOKE_CHAT_ID,
                "content": "",
                "continueMode": true,
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch.status(), 200, "dispatch failed");
    let body: Value = serde_json::from_str(&dispatch.text().await.unwrap()).unwrap();
    assert_eq!(body["type"], "chatSend", "unexpected envelope: {body}");
    let message_id = body["data"]["messageId"].as_str().unwrap().to_string();
    assert!(!message_id.is_empty());
    assert_eq!(body["data"]["hasContent"], true);

    // Collect SSE frames until the done frame for our message arrives.
    let mut buffer: Vec<u8> = Vec::new();
    let mut frames: Vec<Value> = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    'collect: loop {
        #[allow(clippy::single_match)]
        match tokio::time::timeout(std::time::Duration::from_secs(2), sse_stream.next()).await {
            Ok(Some(Ok(bytes))) => {
                buffer.extend_from_slice(&bytes);
                let text = String::from_utf8_lossy(&buffer).to_string();
                frames.clear();
                for frame in text.split("\n\n") {
                    for line in frame.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(v) = serde_json::from_str::<Value>(data) {
                                frames.push(v);
                            }
                        }
                    }
                }
                if frames.iter().any(|f| {
                    f.get("done").is_some()
                        && f.get("messageId").and_then(Value::as_str) == Some(&message_id)
                }) {
                    break 'collect;
                }
            }
            _ => {}
        }
        if tokio::time::Instant::now() > deadline {
            panic!("no done frame arrived; frames so far: {frames:#?}");
        }
    }

    // The trace: every frame chat-scoped; content + done present with the
    // streamed reply.
    assert!(frames
        .iter()
        .all(|f| f.get("chatId").and_then(Value::as_str) == Some(SMOKE_CHAT_ID)));
    assert!(
        frames
            .iter()
            .any(|f| f.get("content").and_then(Value::as_str) == Some(SMOKE_REPLY)),
        "no content frame: {frames:#?}"
    );
    let done = frames
        .iter()
        .find(|f| f.get("done").is_some())
        .expect("done frame");
    assert_eq!(done["messageId"], message_id.as_str());

    // The DB state: lock the engine (drops the writers), reopen, and assert
    // the assistant row landed with the streamed content.
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
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: Some(data.join("quilltap-mount-index.db")),
            llm_logs: Some(data.join("quilltap-llm-logs.db")),
        },
        common::TEST_PEPPER,
    )
    .unwrap();
    let mid = message_id.clone();
    let (content, role): (String, String) = db
        .read_main(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT content, role FROM chat_messages WHERE id = ?1",
                    rusqlite::params![mid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("assistant row present"))
        })
        .unwrap();
    assert_eq!(role, "ASSISTANT");
    assert_eq!(content, SMOKE_REPLY);

    // The chat's lastMessageAt/updatedAt were bumped by the turn.
    let chat_id = SMOKE_CHAT_ID.to_string();
    let (updated_at, last_message_at): (String, Option<String>) = db
        .read_main(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT updatedAt, lastMessageAt FROM chats WHERE id = ?1",
                    rusqlite::params![chat_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("chat row"))
        })
        .unwrap();
    assert!(last_message_at.is_some());
    // The fixture seeds 2020-era timestamps; the turn minted a fresh one.
    assert!(updated_at.as_str() > "2025", "updatedAt: {updated_at}");
}
