//! The canned Scenario Builder spine — shared by the P4.D217 web families
//! (`scenario_builder_dispatch_wire` and the REST edge's disconnect test).
//!
//! ⚠ **`scenario_builder` must be `Some`** (the `swipe_spine` lesson): with it
//! `None` the build verb answers the named not-assembled refusal AFTER every
//! v4 refusal, so a canned run would never reach the wire.
//!
//! Three canned streams, one per leg the wire must see:
//! - [`SceneStream`] answers every call with a plain-text scene (the loop's
//!   plain-text break → the `done` frame);
//! - [`FailingStream`] fails before a chunk (the "detained" error frame);
//! - [`SlowStream`] yields a chunk every 100 ms for ~20 s, so an abort or a
//!   client disconnect lands MID-STREAM (the loop checks the token per chunk).

#![allow(dead_code)] // each family uses the parts it needs

use std::future::Future;
use std::sync::Arc;

use quilltap_core::api::types::Event;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::{ChatCreateSpine, ChatSpine, SpineBundle, SpineFactory};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::Value;

/// The scene [`SceneStream`] answers (padded, so the service's trim shows).
pub const SCENE: &str = "  Lamplight on the wet quay; the tide is turning.  ";

/// The text [`FailingStream`] fails with — the "detained" frame's `details`.
pub const FAILING_STREAM_MESSAGE: &str = "the Host's cab never came";

/// Which canned stream a spine is built over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Canned {
    Scene,
    Failing,
    Slow,
}

pub struct SceneStream;
impl StreamingCompletionProvider for SceneStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            let _ = tx.send(Ok(StreamChunk::content(SCENE.to_string()))).await;
            let _ = tx
                .send(Ok(StreamChunk::done(Some(StreamUsage {
                    prompt_tokens: 30,
                    completion_tokens: 9,
                    total_tokens: 39,
                }))))
                .await;
            rx
        }
    }
}

pub struct FailingStream;
impl StreamingCompletionProvider for FailingStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.send(Err(StreamError::new(FAILING_STREAM_MESSAGE))).await;
            rx
        }
    }
}

pub struct SlowStream;
impl StreamingCompletionProvider for SlowStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                for i in 0..200 {
                    if tx
                        .send(Ok(StreamChunk::content(format!("beat {i} "))))
                        .await
                        .is_err()
                    {
                        return; // the loop broke on the abort and dropped us
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                let _ = tx.send(Ok(StreamChunk::done(None))).await;
            });
            rx
        }
    }
}

pub struct AnyCompletion;
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

pub struct NoPricingFetch;
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
        version: "0.0.0-scenario-builder".to_string(),
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

/// The factory the P4.D217 web families boot with.
pub struct ScenarioBuilderSpineFactory {
    pub base_dir: std::path::PathBuf,
    pub canned: Canned,
}

impl SpineFactory for ScenarioBuilderSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        _terminal: Option<Arc<quilltap_host::terminal::TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
    ) -> SpineBundle {
        // `StreamingCompletionProvider` is not object-safe, so one generic
        // helper per provider (the `swipe_spine` shape).
        match self.canned {
            Canned::Scene => bundle(
                &self.base_dir,
                Arc::new(SceneStream),
                db,
                events,
                pepper,
                data_dir,
                bus,
            ),
            Canned::Failing => bundle(
                &self.base_dir,
                Arc::new(FailingStream),
                db,
                events,
                pepper,
                data_dir,
                bus,
            ),
            Canned::Slow => bundle(
                &self.base_dir,
                Arc::new(SlowStream),
                db,
                events,
                pepper,
                data_dir,
                bus,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn bundle<S>(
    base_dir: &std::path::Path,
    streaming: Arc<S>,
    db: &Db,
    events: &tokio::sync::broadcast::Sender<Event>,
    pepper: &str,
    data_dir: &std::path::Path,
    bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
) -> SpineBundle
where
    S: StreamingCompletionProvider + Send + Sync + 'static,
{
    let embedding = Arc::new(CannedEmbeddingProvider::new());
    let completion = Arc::new(AnyCompletion);
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
            backend: Arc::new(LocalStorageBackend::new(base_dir.join("files"))),
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
        brahma_console_send: None,
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
        scenario_builder: Some(Arc::clone(&spine) as _),
    }
}
