//! The canned swipe spine — shared by the two P4.D207 web families
//! (`message_swipe_stream_dispatch_wire` and `messages_swipe_sse_route`).
//!
//! ⚠ **`swipe_generate` must be `Some`**, unlike the `chat_send_smoke` factory
//! this is modelled on. With it `None` the engine's readiness gate answers
//! `"swipe generation not assembled"` (a 500) BEFORE
//! `api::salon::message_swipe_generate` runs at all — so the guards never
//! execute, no frame is ever published, and every assertion in either family
//! would be measuring that refusal instead of the wire. That is exactly what
//! the dispatch family's first run measured.

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

/// The two prose deltas the canned stream yields, so the wire sees the
/// `regenerating` beat fire ONCE and two `{"content": …}` frames follow.
///
/// ⚠ These are the SAME two strings v4's `salon-swipe-generate.test.ts` mocked
/// `streamMessage` yields. That is what makes `messages_swipe_sse_route` a real
/// byte diff against v4's recorded frames rather than a shape check: change one
/// and the other must change with it.
pub const SWIPE_DELTAS: [&str; 2] = ["A fresh retort, ", "reconsidered."];

/// A streaming provider that answers EVERY call with one fixed sequence. The
/// swipe's one generation is the only stream this venue opens.
pub struct AnyStream;
impl StreamingCompletionProvider for AnyStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            for d in SWIPE_DELTAS {
                let _ = tx.send(Ok(StreamChunk::content(d.to_string()))).await;
            }
            let _ = tx
                // ⚠ The SAME usage v4's `salon-swipe-generate.test.ts`
                // `COMPLETION.usage` carries, so the persisted row's three
                // token columns are part of the byte diff rather than a
                // difference the comparison has to exclude.
                .send(Ok(StreamChunk::done(Some(StreamUsage {
                    prompt_tokens: 40,
                    completion_tokens: 12,
                    total_tokens: 52,
                }))))
                .await;
            rx
        }
    }
}

/// The failure twin of [`AnyStream`]: the provider takes the request and then
/// errors before a single chunk. This is the ONE leg a canned success stream
/// cannot reach — v4's `route.ts:356-364` turns a throw INSIDE `start()` into
/// an `error` FRAME rather than a status, because the SSE headers are long
/// gone by then.
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

/// The text [`FailingStream`] fails with — asserted verbatim as the error
/// frame's `details`, which v4 fills from the raw `error.message`.
pub const FAILING_STREAM_MESSAGE: &str = "the understudy never came on";

/// `{}` for every cheap-LLM call (the ported parsers resolve it to
/// nothing-found and the paths degrade gracefully).
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

pub fn test_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: "0.0.0-swipe".to_string(),
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

/// The factory both families boot with — see the module header for why
/// `swipe_generate` is wired.
pub struct SwipeSpineFactory {
    pub base_dir: std::path::PathBuf,
    /// `true` swaps [`AnyStream`] for [`FailingStream`], so the generation
    /// throws where v4's would and the `error` frame is exercised.
    pub fail_stream: bool,
}

impl SpineFactory for SwipeSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        _terminal: Option<Arc<quilltap_host::terminal::TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
    ) -> SpineBundle {
        // `StreamingCompletionProvider` returns `impl Future`, so it is not
        // object-safe and the choice cannot be an `Arc<dyn …>` — the bundle is
        // built by a GENERIC helper instead, instantiated once per provider.
        if self.fail_stream {
            bundle(
                &self.base_dir,
                Arc::new(FailingStream),
                db,
                events,
                pepper,
                data_dir,
                bus,
            )
        } else {
            bundle(
                &self.base_dir,
                Arc::new(AnyStream),
                db,
                events,
                pepper,
                data_dir,
                bus,
            )
        }
    }
}

/// The bundle both families boot with, generic over the streaming provider.
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
            swipe_generate: Some(Arc::clone(&spine) as _),
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
        }
    }
}
