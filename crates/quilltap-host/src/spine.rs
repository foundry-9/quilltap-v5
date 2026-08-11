//! The production chat-send spine (P4.2) — the composition point the whole
//! round exists for: a [`ChatSendDriver`] that assembles
//! [`OrchestratorDeps`] from the P4.1 drivers and runs
//! `process_message` + `execute_turn_chain` per dispatch, with the stream
//! frames riding the engine's `Event` broadcast — plus the model-dependent
//! job-handler registrations (the thin payload-decode wrappers over the
//! differential-verified `handle_*` bodies).
//!
//! ## The reference construction
//!
//! The impl mirrors the tier-3 orchestrator differential's construction
//! (`crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs`) — the
//! REAL seams (`RealBuildContextSeams`, `RealAnswerConfirmation`,
//! `RealAsyncCompression`, the pricing-backed cost tracker, `RealCarinaQuery`,
//! the Prospero writer, `DangerContentRouter::new(db, DbApiKeys(db))`,
//! `ErasedAskCarina` over `TypedAskCarina`, `RealBrahmaConsole`) with ONLY the
//! model boundaries generic ([`ChatSpine`] takes any embedding / completion /
//! streaming provider + pricing fetch, so the M2 smoke runs canned providers
//! through the identical composition while production wires the
//! [`ProviderIo`](crate::providers::ProviderIo) drivers via
//! [`ProductionSpineFactory`]).
//!
//! ## The Send bridge
//!
//! `process_message`'s future is non-`Send` (the finalizer's carina-markup
//! callbacks are `&mut dyn` trait objects deep in the spine), while the
//! [`ChatSendDriver`] boxed future must be `Send`. Each dispatch therefore
//! runs the turn on its own dedicated thread + current-thread runtime and the
//! driver future awaits a oneshot — the composition shape the enclave turn
//! handler pinned (U4.4's dedicated-thread bridge); the `AUTONOMOUS_ROOM_TURN`
//! step-runner closure below uses the same bridge.
//!
//! ## Documented host-tier seams (flagged, not silently decided)
//!
//! - **Provider→key resolution** ([`DbProviderKeys`]): the streaming/completion
//!   seams carry only `(provider, base_url)` — v4 resolves the key by FOLLOWING
//!   the effective profile's `apiKeyId`. The host key source scans for the
//!   user's first active key for the provider
//!   (`api_key_service::find_active_api_key_for_provider`, the
//!   web-search/moderation style). Divergence is possible only when one user
//!   holds several keys for the same provider.
//! - **Chat-settings mapping** ([`orchestrator_chat_settings_from_value`]) and
//!   **timestamp config** ([`timestamp_config_from_value`]): NEW
//!   (differential-less) projections of the verified `chat_settings` net-read,
//!   written to the field-by-field Zod defaults documented on the structs; the
//!   P4.4/P4.5 settings verticals fold them into verified readers.
//! - **Answer-confirmation timeouts** ([`TimeoutConfirmation`]): v4 wraps the
//!   consistency check (25 s) and the re-affirmation (60 s) separately INSIDE
//!   the service; the host wrapper puts one 85 s ceiling around the whole
//!   runner future (the seam boundary is the whole runner), mapping a timeout
//!   to the ported could-not-verify shape.
//! - **`model_context_limit` / web-search pre-resolution**
//!   ([`ChatSpine::preresolve_provider_model`]): the ported spine takes both as
//!   inputs "resolved above the seam", so the driver runs the SAME
//!   deterministic participant→profile resolution `process_message` runs
//!   internally (same `random01`) and reads `(provider, model)` off it. The
//!   autonomous step's pre-resolve is best-effort (the step picks its own
//!   speaker) — a wrong guess only mis-sizes the context budget.
//! - **`MEMORY_HOUSEKEEPING`** ([`MemoryHousekeepingHandler`]): the v4 job
//!   handler body (settings cascade + character enumeration + the outcome-cache
//!   record) is glue over already-ported pieces; its end-to-end differential
//!   rides the P4.4 jobs vertical.

use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::{
    ChatCreateDriver, ChatCreateDriverRequest, ChatCreateFuture, ChatCreateResultDto,
    ChatErrorPayload, ChatSendDriver, ChatSendFuture, ChatSendRequest, ChatSendResultDto,
    CoreError, ErrorKind, Event, SINGLE_USER_ID,
};
use quilltap_core::chat_timestamp::{TimestampConfig, TimestampFormat, TimestampMode};
use quilltap_core::clock::now_unix_ms;
use quilltap_core::db::runtime::Db;
use quilltap_core::db::{
    background_jobs::BackgroundJob, chat_settings, chats_read, DbError, Writer,
};
use quilltap_core::enclave::cron;
use quilltap_core::enclave::lifecycle::LifecycleDeps;
use quilltap_core::enclave::step::{
    step as enclave_step, AutonomousRoomTurnHandler, AutonomousRoomTurnPayload, StepDeps,
    StepFuture, StepOutcome, TurnJobMeta,
};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::EmbeddingProvider;
use quilltap_core::model::stream::StreamingCompletionProvider;
use quilltap_core::model::streaming_provider::ProviderKeySource;
use quilltap_core::model_context::get_model_context_limit;
use quilltap_core::pascal::custom_tools::{LlmInvokeOptions, LlmInvokeResult, LlmInvoker};
use quilltap_core::pascal::llm_consult::{
    consult_timeout_reason, ConsultRunner, CustomToolConsultContext, CustomToolLlmInvoker,
    CONSULT_TIMEOUT_MS,
};
use quilltap_core::provider_manifest::{Capability, Registry};
use quilltap_core::services::answer_confirmation::AnswerConfirmationOutcome;
use quilltap_core::services::api_key_service;
use quilltap_core::services::brahma_console::RealBrahmaConsole;
use quilltap_core::services::build_context::RealBuildContextSeams;
use quilltap_core::services::carina_memory_extraction::{
    handle_carina_memory_extraction, CarinaMemoryExtractionPayload,
};
use quilltap_core::services::carina_query::{CarinaQueryDeps, RealCarinaQuery};
use quilltap_core::services::carina_runner::{
    CarinaRunError, PostProsperoCarinaError, ProsperoCarinaErrorArgs,
};
use quilltap_core::services::character_avatar_job::CharacterAvatarGenerationHandler;
use quilltap_core::services::chat_create::{
    handle_create, ChatCreateDeps, ChatCreateRequest, ChatCreateResult, HandleCreateError,
};
use quilltap_core::services::chat_events::{ChatEvent, EventSink};
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::context_summary_job::{handle_context_summary, ContextSummaryPayload};
use quilltap_core::services::cost_estimation::MessageCostEstimator;
use quilltap_core::services::creation_progress::{CreationProgressBus, CreationProgressEmitter};
use quilltap_core::services::dangerous_content::gatekeeper_job::{
    handle_chat_danger_classification, ChatDangerClassificationJob, RealDangerAnnouncer,
};
use quilltap_core::services::dangerous_content::moderation_wire::RealModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::{
    DangerContentRouter, DbApiKeys,
};
// === P4.6BL ===
use quilltap_core::services::embedding_generate_job::{
    handle_embedding_generate, EmbeddingGeneratePayload,
};
// === end P4.6BL ===
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::file_storage::{
    ProductionFileBytes, RealProjectImageUpload, StorageBackend,
};
use quilltap_core::services::housekeeping::{run_housekeeping, HousekeepingOptions};
use quilltap_core::services::housekeeping_outcome_cache::record_housekeeping_outcome;
use quilltap_core::services::job_runner::{JobFuture, JobHandler, JobOutcome};
// === P4.24 ===
use quilltap_core::services::llm_log_cleanup_job::{handle_llm_log_cleanup, LlmLogCleanupPayload};
// === end P4.24 ===
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::memory_extraction_job::{
    handle_memory_extraction, limits_from_value, MemoryExtractionPayload,
};
use quilltap_core::services::memory_processor::MemoryExtractionLimits;
use quilltap_core::services::message_finalizer::{
    AcOnAffirming, AnswerConfirmationRunner, CostEstimate, CostTrackArgs, CostTracker,
    FinalizerConfirmationRun, RealAnswerConfirmation, RealAsyncCompression,
};
use quilltap_core::services::native_tool_loop::RegistryToolCallDetector;
use quilltap_core::services::orchestrator::{
    self, build_pricing_context, ExecuteTurnChainOptions, OrchestratorChatSettings,
    OrchestratorDeps, OrchestratorSeams, ProcessClock, ProcessMessageInput, SendMessageOptions,
};
use quilltap_core::services::pricing_fetcher::{PricingContext, PricingFetch, PricingFetcher};
use quilltap_core::services::story_background_job::StoryBackgroundGenerationHandler;
use quilltap_core::services::title_update_job::TitleUpdateHandler;
use quilltap_core::services::turn_orchestrator::ChainConfig;
use quilltap_core::tools::ask_carina::{ErasedAskCarina, TypedAskCarina};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::generate_image::{
    execute_image_generation_tool, ErasedImageGeneration, ImageGenDeps, ImageGenerationRunner,
    ImageGenerationToolInput, ImageGenerationToolOutput, ImageToolExecutionContext,
    RealLanternNotification,
};
use quilltap_core::tools::rng::OsRandomBytes;
use quilltap_core::tools::self_inventory::SelfInventoryEnv;
use serde_json::Value;

use crate::env::production_self_inventory_env;
use crate::files_store::LocalStorageBackend;
use crate::image_codec::HostImageCodec;
use crate::providers::{LivePricingFetch, ProviderIo};
use crate::terminal::scrollback::PtyScrollbackSource;
use crate::terminal::TerminalManager;
use crate::wire::ReqwestWireTransport;

// ===========================================================================
// Provider→key resolution (documented seam — module header)
// ===========================================================================

/// A DB-backed [`ProviderKeySource`]: the single user's first active key for
/// the provider (the provider-scan resolver style).
#[derive(Clone)]
pub struct DbProviderKeys(pub Db);

impl ProviderKeySource for DbProviderKeys {
    fn key_for(&self, provider: &str) -> Option<String> {
        let provider = provider.to_string();
        self.0
            .read_main(move |conn| {
                api_key_service::find_active_api_key_for_provider(conn, SINGLE_USER_ID, &provider)
            })
            .ok()
            .flatten()
            .map(|k| k.key_value)
    }
}

/// A DB-backed [`SearchApiKeyLookup`](quilltap_core::tools::web_search::SearchApiKeyLookup)
/// (P4.42): the acting user's first active key for the search provider — v4's
/// `getAllApiKeys().find(provider === X && isActive)`, the exact analogue of
/// [`DbProviderKeys`] over the same `find_active_api_key_for_provider` resolver.
///
/// DELIBERATELY inert on this lane's runtime path: `serper_registered` stays
/// `false` (the plugin registry is the standing deferral), so
/// [`RealWebSearchProvider`](quilltap_core::tools::web_search::RealWebSearchProvider)
/// never consults it — the env-key fallback is the only live path. It is wired so
/// that when the plugin half lands, the api-key-row path is already correct.
#[derive(Clone)]
pub struct DbSearchApiKeys(pub Db);

impl quilltap_core::tools::web_search::SearchApiKeyLookup for DbSearchApiKeys {
    fn find_active_key(&self, provider: &str, user_id: &str) -> Option<String> {
        let provider = provider.to_string();
        let user_id = user_id.to_string();
        self.0
            .read_main(move |conn| {
                api_key_service::find_active_api_key_for_provider(conn, &user_id, &provider)
            })
            .ok()
            .flatten()
            .map(|k| k.key_value)
    }
}

// ===========================================================================
// The production non-streaming CompletionProvider
// ===========================================================================

/// The production [`CompletionProvider`] over the reqwest transport + the
/// host key source — the composition
/// [`execute_completion`](quilltap_core::model::completion_provider::execute_completion)
/// exists for. A local provider (no key on file) sends the empty key, matching
/// v4's `getApiKeyForCheapLLMSelection` local `''`.
pub struct WireCompletionProvider<K: ProviderKeySource> {
    transport: quilltap_core::model::transport::ReqwestTransport,
    keys: K,
    policy: quilltap_core::model::transport::TransportPolicy,
    user_agent: String,
    base_url_env: Option<String>,
}

impl<K: ProviderKeySource> WireCompletionProvider<K> {
    pub fn new(
        keys: K,
        policy: quilltap_core::model::transport::TransportPolicy,
        user_agent: String,
        base_url_env: Option<String>,
    ) -> Self {
        Self {
            transport: quilltap_core::model::transport::ReqwestTransport::new(),
            keys,
            policy,
            user_agent,
            base_url_env,
        }
    }
}

impl<K: ProviderKeySource> CompletionProvider for WireCompletionProvider<K> {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let api_key = self.keys.key_for(provider).unwrap_or_default();
        // P4.D42 (v4 `74ec93b5`): the caller's per-request budget becomes THIS
        // call's transport policy — a ceiling on one attempt, retries off. The
        // process-wide policy stands when the caller named none. This is the only
        // per-call path; the field never reaches a request body (v4 same).
        let policy = self.policy.with_request_budget(params.request_timeout_ms);
        async move {
            quilltap_core::model::completion_provider::execute_completion(
                &self.transport,
                provider,
                base_url,
                &api_key,
                params,
                &policy,
                &self.user_agent,
                self.base_url_env.as_deref(),
            )
            .await
        }
    }
}

/// The wire knobs the per-job provider constructions share (from
/// [`ProviderIo`]).
#[derive(Clone)]
pub struct WireConfig {
    pub policy: quilltap_core::model::transport::TransportPolicy,
    pub user_agent: String,
    pub base_url_env: Option<String>,
}

impl WireConfig {
    pub fn from_io(io: &ProviderIo) -> Self {
        Self {
            policy: io.policy(),
            user_agent: io.user_agent().to_string(),
            base_url_env: io.base_url_env().map(String::from),
        }
    }

    fn completion(&self, db: &Db) -> WireCompletionProvider<DbProviderKeys> {
        WireCompletionProvider::new(
            DbProviderKeys(db.clone()),
            self.policy,
            self.user_agent.clone(),
            self.base_url_env.clone(),
        )
    }
}

// ===========================================================================
// The chat-settings read (documented NEW mapping — module header)
// ===========================================================================

/// Map the raw `chat_settings` row (the differential-verified
/// [`chat_settings::find_by_user_id`] net-read JSON) to the
/// [`OrchestratorChatSettings`] projection the spine consumes. Field defaults
/// follow the struct's documented Zod defaults.
pub fn orchestrator_chat_settings_from_value(row: &Value) -> OrchestratorChatSettings {
    let cheap = row.get("cheapLLMSettings").filter(|v| v.is_object());
    let compression = row
        .get("contextCompressionSettings")
        .filter(|v| v.is_object());
    let agent = row.get("agentModeSettings").filter(|v| v.is_object());
    let autonomous = row.get("autonomousRoomSettings").filter(|v| v.is_object());
    let s = |v: Option<&Value>, k: &str| -> Option<String> {
        v.and_then(|o| o.get(k))
            .and_then(Value::as_str)
            .map(String::from)
    };
    OrchestratorChatSettings {
        cheap_llm_settings_present: cheap.is_some(),
        // `contextCompressionSettings.enabled` (default true when absent).
        compression_enabled: compression
            .and_then(|c| c.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        project_context_reinject_interval: compression
            .and_then(|c| c.get("projectContextReinjectInterval"))
            .and_then(Value::as_f64)
            .map(|v| v as i64)
            .unwrap_or(5),
        auto_detect_rng: row
            .get("autoDetectRng")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        custom_tools: row
            .get("customTools")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        answer_confirmation_global_enabled: row
            .get("answerConfirmationSettings")
            .and_then(|v| v.get("enabled"))
            .and_then(Value::as_bool)
            == Some(true),
        autonomous_destructive_policy: s(autonomous, "destructiveToolPolicy")
            .unwrap_or_else(|| "opt_in_per_room".to_string()),
        agent_mode_default_enabled: agent
            .and_then(|a| a.get("defaultEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        agent_mode_max_turns: agent
            .and_then(|a| a.get("maxTurns"))
            .and_then(Value::as_f64)
            .map(|v| v as i64)
            .unwrap_or(10),
        danger_settings: row
            .get("dangerousContentSettings")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        cheap_llm_strategy: s(cheap, "strategy").unwrap_or_else(|| "PROVIDER_CHEAPEST".to_string()),
        cheap_llm_user_defined_profile_id: s(cheap, "userDefinedProfileId"),
        cheap_llm_default_cheap_profile_id: s(cheap, "defaultCheapProfileId"),
        // v4 DEFAULT_CHEAP_LLM_CONFIG.fallbackToLocal = true.
        cheap_llm_fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
    }
}

/// The production [`OrchestratorSeams`]: the `chat_settings` row read
/// (v4 `repos.chatSettings.findByUserId`), error-swallowed like v4's.
pub struct HostOrchestratorSeams {
    pub db: Db,
}

impl OrchestratorSeams for HostOrchestratorSeams {
    fn chat_settings(&self, user_id: &str) -> Option<OrchestratorChatSettings> {
        let uid = user_id.to_string();
        let row = self
            .db
            .read_main(move |conn| chat_settings::find_by_user_id(conn, &uid))
            .ok()??;
        Some(orchestrator_chat_settings_from_value(&row))
    }
}

/// v4 `chat.timestampConfig || chatSettings?.defaultTimestampConfig || null`,
/// with the Zod defaults materialized (`mode`→NONE, `format`→FRIENDLY,
/// `useFictionalTime`→false, `autoPrepend`→true, `intervalMinutes`→15).
pub fn timestamp_config_from_value(v: Option<&Value>) -> Option<TimestampConfig> {
    let obj = v.filter(|v| v.is_object())?;
    let mode = match obj.get("mode").and_then(Value::as_str).unwrap_or("NONE") {
        "START_ONLY" => TimestampMode::StartOnly,
        "EVERY_MESSAGE" => TimestampMode::EveryMessage,
        "EVERY_N_MINUTES" => TimestampMode::EveryNMinutes,
        _ => TimestampMode::None,
    };
    // v4 TimestampFormatEnum = ['ISO8601','FRIENDLY','DATE_ONLY','TIME_ONLY','CUSTOM'].
    let format = match obj
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("FRIENDLY")
    {
        "ISO8601" => TimestampFormat::Iso8601,
        "DATE_ONLY" => TimestampFormat::DateOnly,
        "TIME_ONLY" => TimestampFormat::TimeOnly,
        "CUSTOM" => TimestampFormat::Custom,
        _ => TimestampFormat::Friendly,
    };
    let s = |k: &str| obj.get(k).and_then(Value::as_str).map(String::from);
    Some(TimestampConfig {
        mode,
        format,
        custom_format: s("customFormat"),
        use_fictional_time: obj
            .get("useFictionalTime")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fictional_base_timestamp: s("fictionalBaseTimestamp"),
        fictional_base_real_time: s("fictionalBaseRealTime"),
        auto_prepend: obj
            .get("autoPrepend")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timezone: s("timezone"),
        interval_minutes: obj
            .get("intervalMinutes")
            .and_then(Value::as_f64)
            .map(|v| v as i64)
            .unwrap_or(15),
    })
}

// ===========================================================================
// Finalizer seam wrappers
// ===========================================================================

/// v4's answer-confirmation `withTimeout` family as a host wrapper: one
/// ceiling of `CONSISTENCY_CHECK_TIMEOUT_MS + REAFFIRMATION_TIMEOUT_MS`
/// (25 s + 60 s) around the whole runner future; a timeout maps to the ported
/// could-not-verify outcome (`confirmed: null` + notes), the same shape the
/// service's own failure path produces.
pub struct TimeoutConfirmation<R> {
    pub inner: R,
    pub ceiling: std::time::Duration,
}

impl<R> TimeoutConfirmation<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            ceiling: std::time::Duration::from_millis(25_000 + 60_000),
        }
    }
}

impl<R: AnswerConfirmationRunner + Sync> AnswerConfirmationRunner for TimeoutConfirmation<R> {
    async fn run<'a>(
        &'a self,
        opts: FinalizerConfirmationRun<'a>,
        on_affirming: &'a dyn AcOnAffirming,
    ) -> AnswerConfirmationOutcome {
        match tokio::time::timeout(self.ceiling, self.inner.run(opts, on_affirming)).await {
            Ok(outcome) => outcome,
            Err(_) => AnswerConfirmationOutcome {
                confirmed: None,
                revised: false,
                notes: Some("Answer confirmation timed out".to_string()),
                revised_content: None,
            },
        }
    }
}

/// v4's consult `withTimeout` (`lib/pascal/llm-consult.ts:159`) as a host
/// decorator around the WHOLE invoker — exactly where v4 puts it, at the
/// invoker boundary rather than inside `consult`. Shaped on
/// [`TimeoutConfirmation`]: one ceiling of `CONSULT_TIMEOUT_MS`, and an elapsed
/// timer maps to the ordinary fail-soft outcome
/// (`LlmInvokeResult::Failed { reason: consult_timeout_reason(…) }`) so a hung
/// provider becomes the author's `errorMessage`, never a wedged tool call. The
/// timer lives here because `quilltap-core` has no tokio timer driver in its
/// default build (the standing host-side-timers rule).
pub struct TimeoutConsult<I> {
    pub inner: I,
    pub ceiling: std::time::Duration,
}

impl<I> TimeoutConsult<I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            ceiling: std::time::Duration::from_millis(CONSULT_TIMEOUT_MS as u64),
        }
    }
}

impl<I: LlmInvoker> LlmInvoker for TimeoutConsult<I> {
    fn invoke<'a>(
        &'a self,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>> {
        Box::pin(async move {
            match tokio::time::timeout(self.ceiling, self.inner.invoke(prompt, options)).await {
                Ok(result) => result,
                Err(_) => LlmInvokeResult::Failed {
                    reason: consult_timeout_reason(CONSULT_TIMEOUT_MS),
                },
            }
        })
    }
}

/// The custom-tool consult runner (P4.6bd) — the host's live
/// `EngineAssembly.consult` seam, behind all three entrances (the `run_custom`
/// executor, the composer's `chatCustomToolRun`, the workbench `{live:true}`
/// bench). Mirrors [`HostImageGenerationRunner`]: it holds only the
/// [`WireConfig`] and rebuilds `wire.completion(db)` per consult, so a
/// key/profile change is live on the very next roll and the request's own
/// user/chat land on the `CUSTOM_TOOL_CONSULT` log row (the invoker constructs
/// its logging executor from the per-call context). Each consult is wrapped in
/// [`TimeoutConsult`] — v4's 60 s `withTimeout` at the invoker boundary.
pub struct HostConsultRunner {
    pub wire: WireConfig,
}

impl ConsultRunner for HostConsultRunner {
    fn consult<'a>(
        &'a self,
        db: &'a Db,
        context: CustomToolConsultContext,
        prompt: &'a str,
        options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>> {
        Box::pin(async move {
            let completion = self.wire.completion(db);
            let invoker = TimeoutConsult::new(CustomToolLlmInvoker::new(db, &completion, context));
            invoker.invoke(prompt, options).await
        })
    }
}

/// The in-chat announcement-preview runner (P4.9E2A, wired at that round's
/// unification) — the host's live `EngineAssembly.announcement_preview` seam
/// behind the Insert Announcement dialog's Generate button.
///
/// P4.9E2A shipped a ready-made `AnnouncementPreviewRunner` but had to leave the
/// seam `None`: it owned neither `spine.rs` nor the host's assembly. This is that
/// construction, with one deliberate difference from the shipped runner — it
/// rebuilds the **logging** `CheapLlmTaskExecutor` per call from the request's
/// own `user_id` + `chat_id`, the way [`HostConsultRunner`] rebuilds its
/// provider. v4 passes `userId`, `chatId` and `character.id` straight into
/// `executeCheapLLMTask` (`lib/services/announcer/character-voiced.ts:159-170`),
/// so the call lands on an `llm_logs` row in v4 and must land on one here; the
/// shipped runner's single assembly-time executor cannot carry per-request
/// identity. The differential's ROUTE cases use the shipped runner with a
/// non-logging executor because that fixture family has no llm-logs partition.
///
/// ⚠ LIVE means real money: one cheap-LLM call per press of Generate.
struct HostAnnouncementPreviewRunner<C, E> {
    db: Db,
    completion: Arc<C>,
    embedding: Arc<E>,
}

impl<C, E> quilltap_core::api::chat_post_office::AnnouncementPreviewDriver
    for HostAnnouncementPreviewRunner<C, E>
where
    C: quilltap_core::model::completion::CompletionProvider + Send + Sync,
    E: quilltap_core::model::embedding::EmbeddingProvider + Send + Sync,
{
    fn run(
        &self,
        input: quilltap_core::api::chat_post_office::CharacterVoicedRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        quilltap_core::api::chat_post_office::CharacterVoicedOutcome,
                        String,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: self.db.clone(),
                user_id: input.user_id.clone(),
                chat_id: Some(input.chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            let result = quilltap_core::services::announcer::character_voiced::
                generate_character_voiced_announcement(
                    &self.db,
                    &*self.completion,
                    &*self.embedding,
                    &executor,
                    &quilltap_core::services::announcer::character_voiced::
                        CharacterVoicedAnnouncementParams {
                        chat_id: &input.chat_id,
                        character: &input.character,
                        profile: &input.profile,
                        seed_markdown: &input.seed_markdown,
                        system_prompt_id: input.system_prompt_id.as_deref(),
                        user_id: &input.user_id,
                        audience_names: &input.audience_names,
                        now_ms: quilltap_core::clock::now_unix_ms() as f64,
                    },
                )
                .await;
            Ok(
                quilltap_core::api::chat_post_office::CharacterVoicedOutcome {
                    success: result.success,
                    proposed_markdown: result.proposed_markdown,
                    error: result.error,
                },
            )
        })
    }
}

/// P4.9E3A: the manual title-regeneration driver — the host's completion
/// provider plus a per-call LOGGING cheap executor, so the regeneration's
/// `llm_logs` row carries the request's own user + chat (the announcement-preview
/// arrangement).
///
/// ⚠ LIVE means real money: one cheap-LLM call per Regenerate Title.
struct HostRegenerateTitleRunner<C> {
    db: Db,
    user_id: String,
    completion: Arc<C>,
}

/// P4.9E3B: the out-of-create `llm_choose` outfit runner — the host's
/// completion provider plus a per-call LOGGING cheap executor keyed to the
/// request's own chat (the regenerate-title arrangement), delegating into the
/// core's `run_llm_choose_via_db` so the differential drives the exact
/// composition production uses.
///
/// ⚠ LIVE means real money: one cheap-LLM call per llm_choose pick
/// (add-participant or merge).
struct HostOutfitLlmChooseRunner<C> {
    db: Db,
    user_id: String,
    completion: Arc<C>,
}

impl<C> quilltap_core::services::outfit_selections::OutfitLlmChooseRunner
    for HostOutfitLlmChooseRunner<C>
where
    C: quilltap_core::model::completion::CompletionProvider + Send + Sync,
{
    fn choose(
        &self,
        req: quilltap_core::services::outfit_selections::OutfitLlmChooseRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<quilltap_core::wardrobe::Slots>> + Send + '_>,
    > {
        Box::pin(async move {
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: self.db.clone(),
                user_id: self.user_id.clone(),
                chat_id: Some(req.chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            quilltap_core::services::outfit_selections::run_llm_choose_via_db(
                &self.db,
                &*self.completion,
                &executor,
                &req.character_id,
                req.scenario_text.as_deref(),
                req.cheap_settings.as_ref(),
                &req.project_mount_point_ids,
            )
            .await
        })
    }
}

// === P4.9E4A: the vision-describe runner ===
/// The `attach-mount-file` describe seam: the host's completion provider plus
/// the host image codec (the pre-vision downsize) and the REAL `logLLMCall`
/// write, so an attach's `IMAGE_DESCRIPTION` row appears exactly as v4 logs it.
/// The core's [`generate_image_description`](quilltap_core::services::file_fallback::generate_image_description)
/// is the whole body — profile resolution, the refusal heuristics, and the
/// uncensored retry all run there, so the differential drives the exact
/// composition production uses.
///
/// ⚠ LIVE means real money: one vision-LLM call per attach of an image with no
/// cached description and no kept-image markdown.
struct HostImageDescribeRunner<C> {
    db: Db,
    user_id: String,
    completion: Arc<C>,
}

impl<C> quilltap_core::api::chat_media::ImageDescribeDriver for HostImageDescribeRunner<C>
where
    C: quilltap_core::model::completion::CompletionProvider + Send + Sync,
{
    fn describe<'a>(
        &'a self,
        file: quilltap_core::services::file_fallback::FallbackFile,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = quilltap_core::services::file_fallback::FallbackResult>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let transcoder = HostImageCodec;
            let deps = quilltap_core::services::file_fallback::FallbackDeps {
                db: &self.db,
                completion: &*self.completion,
                transcoder: &transcoder,
                user_id: &self.user_id,
                now_ms: now_unix_ms(),
            };
            quilltap_core::services::file_fallback::generate_image_description(&deps, &file).await
        })
    }
}
// === end P4.9E4A ===

impl<C> quilltap_core::services::chat_admin::RegenerateTitleDriver for HostRegenerateTitleRunner<C>
where
    C: quilltap_core::model::completion::CompletionProvider + Send + Sync,
{
    fn run<'a>(
        &'a self,
        chat_id: String,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = quilltap_core::api::types::Response> + Send + 'a>,
    > {
        Box::pin(async move {
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: self.db.clone(),
                user_id: self.user_id.clone(),
                chat_id: Some(chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            quilltap_core::services::chat_admin::chat_regenerate_title(
                &self.db,
                &self.user_id,
                &chat_id,
                &*self.completion,
                &executor,
                &quilltap_core::clock::now_iso(),
            )
            .await
        })
    }
}

/// The pricing-backed [`CostTracker`] (v4 `estimateMessageCost` — the W4.7e
/// cascade over the held fetcher + a per-call [`PricingContext`]).
pub struct PricingCostTracker<'a, PF: PricingFetch> {
    pub fetcher: &'a PricingFetcher<PF>,
    pub ctx: PricingContext,
}

impl<PF: PricingFetch> CostTracker for PricingCostTracker<'_, PF> {
    fn estimate(&mut self, args: &CostTrackArgs) -> CostEstimate {
        let result = self.fetcher.estimate_message_cost(
            Registry::built_in(),
            &args.provider,
            &args.model_name,
            args.prompt_tokens,
            args.completion_tokens,
            now_unix_ms(),
            &self.ctx,
        );
        CostEstimate {
            cost: result.cost,
            source: Some(result.source),
        }
    }
}

/// The production Prospero-carina-error post seam — bridges the sync trait to
/// the ported async writer
/// ([`post_prospero_carina_error`](quilltap_core::services::prospero_notifications::post_prospero_carina_error))
/// on a dedicated thread + current-thread runtime (a rare error path; the
/// sync trait cannot await on the spine's own runtime).
#[derive(Clone)]
pub struct ThreadedProspero {
    pub db: Db,
}

impl PostProsperoCarinaError for ThreadedProspero {
    fn post(&mut self, args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
        let db = self.db.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| CarinaRunError(format!("prospero runtime: {e}")))?;
            rt.block_on(async {
                quilltap_core::services::prospero_notifications::post_prospero_carina_error(
                    &db, args,
                )
                .await;
                Ok(())
            })
        });
        handle
            .join()
            .unwrap_or_else(|_| Err(CarinaRunError("prospero post thread panicked".into())))
    }
}

// ===========================================================================
// The event sink
// ===========================================================================

/// The [`EventSink`] adapter: wraps each [`ChatEvent`] in
/// `Event::chat(chat_id, frame)` and publishes on the engine broadcast. A
/// send with no subscribers is fine (fire-and-forget, per the sink contract).
pub struct BroadcastSink {
    pub chat_id: String,
    pub tx: tokio::sync::broadcast::Sender<Event>,
}

impl EventSink for BroadcastSink {
    fn emit(&self, event: ChatEvent) {
        let _ = self.tx.send(Event::chat(self.chat_id.clone(), event));
    }
}

// ===========================================================================
// The spine driver
// ===========================================================================

/// Everything one dispatch needs, cloneable into the dedicated send thread.
/// Generic over the model boundaries so the M2 smoke can run canned providers
/// through the identical composition.
pub struct ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    pub db: Db,
    pub events: tokio::sync::broadcast::Sender<Event>,
    pub embedding: Arc<EMB>,
    pub completion: Arc<CMP>,
    pub streaming: Arc<STR>,
    /// ONE per host process (the 24 h + 5 min negative caches are state).
    pub pricing: Arc<PricingFetcher<PF>>,
    /// IANA timezone (v4 uses the process zone).
    pub tz: String,
    pub env: SelfInventoryEnv,
    pub file_bytes: Arc<ProductionFileBytes>,
    pub image_transcoder: Arc<HostImageCodec>,
    /// The terminal scrollback source for `terminal_read` (P4.1c), when the
    /// host runs a terminal manager.
    pub scrollback: Option<Arc<PtyScrollbackSource>>,
    /// The custom-tool consult runner the `run_custom` tool consults through
    /// (P4.6bd; `None` for canned test spines — the tool then fails soft into
    /// the author's `errorMessage`, the pre-wire behavior).
    pub consult: Option<Arc<dyn ConsultRunner>>,
    /// The Serper web-search provider `search_web` runs through (P4.42; `None`
    /// for canned test spines + when `SERPER_API_KEY` is unset — the tool then
    /// answers v4's "not configured" error). ONE `Option` feeds both the in-chat
    /// turn (via [`OrchestratorDeps`]) and the carina/Brahma/Run-Tool engines
    /// (via [`Self::tool_runner`]).
    pub web_search: Option<Arc<dyn quilltap_core::tools::web_search::WebSearchProvider>>,
}

impl<EMB, CMP, STR, PF> ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn clone_state(&self) -> Self {
        ChatSpine {
            db: self.db.clone(),
            events: self.events.clone(),
            embedding: Arc::clone(&self.embedding),
            completion: Arc::clone(&self.completion),
            streaming: Arc::clone(&self.streaming),
            pricing: Arc::clone(&self.pricing),
            tz: self.tz.clone(),
            env: self.env.clone(),
            file_bytes: Arc::clone(&self.file_bytes),
            image_transcoder: Arc::clone(&self.image_transcoder),
            scrollback: self.scrollback.clone(),
            consult: self.consult.clone(),
            web_search: self.web_search.clone(),
        }
    }

    /// A tool runner for the carina / ask_carina / Brahma engines (each engine
    /// owns its own — the type-level cycle note from W4.11a).
    pub(crate) fn tool_runner(&self) -> BuiltInToolRunner {
        let mut runner = BuiltInToolRunner::new(self.db.clone(), self.env.clone());
        if let Some(sb) = &self.scrollback {
            runner = runner.with_scrollback(Arc::clone(sb) as _);
        }
        if let Some(consult) = &self.consult {
            runner = runner.with_consult(Arc::clone(consult));
        }
        // P4.42: the operator Run Tool modal + carina/ask_carina + the Brahma
        // Console all reach `search_web` through this one runner.
        if let Some(web_search) = &self.web_search {
            runner = runner.with_web_search_provider(Arc::clone(web_search));
        }
        runner
    }

    /// The local UTC offset (minutes) for the configured zone at `now_ms`,
    /// in JS `getTimezoneOffset()` convention (see [`js_local_offset_minutes`]).
    fn local_offset_minutes(&self, now_ms: i64) -> i64 {
        js_local_offset_minutes(&self.tz, now_ms)
    }

    /// Pre-resolve the effective connection profile's `(provider, model)` for
    /// the registry-sourced per-request inputs (module header). Deterministic
    /// given the same `random01` `process_message` receives.
    async fn preresolve_provider_model(
        &self,
        chat_id: &str,
        responding_participant_id: Option<&str>,
        target_participant_ids: Option<&[String]>,
        speaking_as_participant_id: Option<&str>,
        continue_mode: bool,
        random01: f64,
    ) -> Option<(String, String)> {
        let cid = chat_id.to_string();
        let chat = self
            .db
            .read_main(move |c| chats_read::find_by_id(c, &cid))
            .ok()??;
        let responding_id = responding_participant_id
            .map(String::from)
            .or_else(|| target_participant_ids.and_then(|t| t.first().cloned()));
        let speaking_as = speaking_as_participant_id.map(String::from).or_else(|| {
            chat.get("activeTypingParticipantId")
                .and_then(Value::as_str)
                .map(String::from)
        });
        let resolution =
            quilltap_core::services::participant_resolver::resolve_responding_participant(
                &self.db,
                &chat,
                SINGLE_USER_ID,
                responding_id.as_deref(),
                continue_mode,
                speaking_as.as_deref(),
                random01,
            )
            .await
            .ok()?;
        let provider = resolution
            .connection_profile
            .get("provider")
            .and_then(Value::as_str)?
            .to_string();
        let model = resolution
            .connection_profile
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Some((provider, model))
    }

    /// `(model_context_limit, provider_supports_web_search)` off the
    /// pre-resolved profile; registry defaults when resolution fails (the
    /// spine call surfaces the real error itself).
    fn registry_inputs(&self, resolved: Option<(String, String)>) -> (i64, bool) {
        match resolved {
            Some((provider, model)) => {
                let registry = Registry::built_in();
                let limit = get_model_context_limit(
                    &provider,
                    &model,
                    &self.env.model_info,
                    &self.env.fallback_pricing,
                    registry.default_context_window(&provider),
                );
                (
                    limit,
                    registry.supports_capability(&provider, Capability::WebSearch),
                )
            }
            None => (8192, false),
        }
    }

    /// v4 `chat.timestampConfig || chatSettings?.defaultTimestampConfig`.
    fn resolve_timestamp_config(&self, chat_id: &str) -> Option<TimestampConfig> {
        let cid = chat_id.to_string();
        let chat = self
            .db
            .read_main(move |c| chats_read::find_by_id(c, &cid))
            .ok()
            .flatten();
        let settings = self
            .db
            .read_main(|c| chat_settings::find_by_user_id(c, SINGLE_USER_ID))
            .ok()
            .flatten();
        timestamp_config_from_value(chat.as_ref().and_then(|c| c.get("timestampConfig"))).or_else(
            || {
                timestamp_config_from_value(
                    settings
                        .as_ref()
                        .and_then(|s| s.get("defaultTimestampConfig")),
                )
            },
        )
    }

    /// One swipe generation (the non-`Send` inner the dedicated thread runs):
    /// v4 `handleGenerateSwipe` → `regenerateMessageAsSwipe`. The route handler
    /// (`api::salon::message_swipe_generate`) already loaded the chat + passed the
    /// guards; here the spine resolves the same registry / timestamp / clock inputs
    /// `run_send` does and composes the ported service over its real providers +
    /// `RealBuildContextSeams`. The message-context K-loader is
    /// `NoopMessageContextSeams` — the same boundary `regenerate_swipe_tier3`
    /// proves against v4's REAL service (Lantern image loading into a regenerated
    /// context is a tracked deferral, shared with the core port).
    async fn run_swipe(
        self,
        req: quilltap_core::api::chat_send::SwipeGenerateRequest,
    ) -> Result<Value, CoreError> {
        use quilltap_core::services::message_context::NoopMessageContextSeams;
        use quilltap_core::services::regenerate_swipe::{
            regenerate_message_as_swipe, RegenError, RegenerateSwipeOptions,
        };

        let db = self.db.clone();
        let chat_id = req
            .chat
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let now_ms = now_unix_ms();
        let random01 = os_random01();

        // The responder in continue mode is the target message's own participant.
        let target_pid = req
            .target_message
            .get("participantId")
            .and_then(Value::as_str)
            .map(String::from);
        let resolved = self
            .preresolve_provider_model(
                &chat_id,
                target_pid.as_deref(),
                None,
                req.active_user_participant_id.as_deref(),
                true,
                random01,
            )
            .await;
        let (model_context_limit, _web) = self.registry_inputs(resolved);
        let timestamp_config = self.resolve_timestamp_config(&chat_id);

        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: req.user_id.clone(),
            chat_id: Some(chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let bc_seams = RealBuildContextSeams { db: &db };
        let mc_seams = NoopMessageContextSeams;

        let opts = RegenerateSwipeOptions {
            user_id: req.user_id.clone(),
            chat: req.chat.clone(),
            target_message: req.target_message.clone(),
            all_messages: req.all_messages.clone(),
            active_user_participant_id: req.active_user_participant_id.clone(),
            model_context_limit,
            timestamp_config,
            timezone: Some(self.tz.clone()),
            // The host's own zone — the memory distill's local-calendar seam.
            server_tz: Some(self.tz.clone()),
            now_ms,
            local_offset_minutes: self.local_offset_minutes(now_ms),
            random01,
        };

        regenerate_message_as_swipe(
            &db,
            &*self.embedding,
            &*self.completion,
            &executor,
            &bc_seams,
            &mc_seams,
            opts,
        )
        .await
        .map_err(|e| match e {
            RegenError::NotAssistant | RegenError::StaffMessage => CoreError {
                kind: ErrorKind::BadRequest,
                message: e.to_string(),
                pepper_state: None,
                code: None,
                associations: None,
                character_id: None,
                entity: None,
            },
            RegenError::Db(_) => CoreError {
                kind: ErrorKind::Internal,
                message: e.to_string(),
                pepper_state: None,
                code: None,
                associations: None,
                character_id: None,
                entity: None,
            },
        })
    }

    /// One full send turn (the non-`Send` inner the dedicated thread runs):
    /// v4 `handleSendMessage` — `processMessage`, then ALWAYS
    /// `executeTurnChain` (the chain's own guard decides whether it fires).
    async fn run_send(self, req: ChatSendRequest) -> Result<ChatSendResultDto, CoreError> {
        let db = self.db.clone();
        let chat_id = req.chat_id.clone();
        let sink = BroadcastSink {
            chat_id: chat_id.clone(),
            tx: self.events.clone(),
        };

        // Per-call clock + RNG (v4 Date.now() / Math.random()).
        let now_ms = now_unix_ms();
        let random01 = os_random01();
        let clock = ProcessClock {
            now_ms,
            local_offset_minutes: self.local_offset_minutes(now_ms),
            random01,
        };

        let resolved = self
            .preresolve_provider_model(
                &chat_id,
                req.responding_participant_id.as_deref(),
                req.target_participant_ids.as_deref(),
                req.speaking_as_participant_id.as_deref(),
                req.continue_mode,
                random01,
            )
            .await;
        let (model_context_limit, provider_supports_web_search) = self.registry_inputs(resolved);
        let timestamp_config = self.resolve_timestamp_config(&chat_id);

        // The per-call cheap-LLM executor with llm_logs logging (v4's
        // cheap-LLM logLLMCall context: chatId, no messageId).
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: SINGLE_USER_ID.to_string(),
            chat_id: Some(chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });

        // The REAL seams (the harness-mirrored construction; module header).
        let bc_seams = RealBuildContextSeams { db: &db };
        let orchestrator_seams = HostOrchestratorSeams { db: db.clone() };
        let router = DangerContentRouter::new(db.clone(), DbApiKeys(db.clone()));
        let mut confirmation = TimeoutConfirmation::new(RealAnswerConfirmation {
            completion: &*self.completion,
            executor: &executor,
        });
        let mut compression = RealAsyncCompression {
            db: &db,
            completion: &*self.completion,
            executor: &executor,
        };
        let mut cost = PricingCostTracker {
            fetcher: &self.pricing,
            ctx: build_pricing_context(&db, SINGLE_USER_ID),
        };
        let mut prospero = ThreadedProspero { db: db.clone() };
        // `model_supports_native_tools: true` on the carina/Brahma engines
        // matches v4's checkModelSupportsTools default for models absent from
        // the fallback table (the engines resolve their own profiles, which
        // the spine cannot know here).
        let brahma = RealBrahmaConsole::new(
            db.clone(),
            Arc::clone(&self.streaming),
            self.tool_runner(),
            RegistryToolCallDetector::built_in(),
            true,
        );
        let ask_carina = ErasedAskCarina::new(TypedAskCarina {
            db: db.clone(),
            embedding: Arc::clone(&self.embedding),
            streaming: Arc::clone(&self.streaming),
            tool_runner: self.tool_runner(),
            tool_detector: RegistryToolCallDetector::built_in(),
            brahma: RealBrahmaConsole::new(
                db.clone(),
                Arc::clone(&self.streaming),
                self.tool_runner(),
                RegistryToolCallDetector::built_in(),
                true,
            ),
            prospero: ThreadedProspero { db: db.clone() },
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
        });
        let carina_tool_runner = self.tool_runner();
        let carina_detector = RegistryToolCallDetector::built_in();
        let mut carina_query = RealCarinaQuery::new(CarinaQueryDeps {
            db: &db,
            embedding: &*self.embedding,
            streaming: &*self.streaming,
            tool_runner: &carina_tool_runner,
            tool_detector: &carina_detector,
            sink: &sink,
            brahma: &brahma,
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
        });
        let mut rng_bytes = OsRandomBytes;

        let mut deps = OrchestratorDeps {
            db: &db,
            embedding: &*self.embedding,
            completion: &*self.completion,
            streaming: &*self.streaming,
            executor: &executor,
            ask_carina: &ask_carina,
            sink: &sink,
            pricing: &self.pricing,
            build_context_seams: &bc_seams,
            orchestrator_seams: &orchestrator_seams,
            file_bytes: &*self.file_bytes,
            image_transcoder: &*self.image_transcoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina_query,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
            // P4.42: the in-chat turn's `search_web` provider (None until
            // SERPER_API_KEY is set).
            web_search: self.web_search.clone(),
        };

        let input = ProcessMessageInput {
            log_context: LogContext::none(),
            chat_id: chat_id.clone(),
            user_id: SINGLE_USER_ID.to_string(),
            options: SendMessageOptions {
                continue_mode: req.continue_mode,
                content: req.content.clone(),
                responding_participant_id: req.responding_participant_id.clone(),
                target_participant_ids: req.target_participant_ids.clone(),
                speaking_as_participant_id: req.speaking_as_participant_id.clone(),
                file_ids: req.file_ids.clone(),
                nudge: req.nudge,
                pending_tool_results: req
                    .pending_tool_results
                    .iter()
                    .map(|t| orchestrator::PendingToolResult {
                        tool: t.tool.clone(),
                        success: t.success,
                        result: t.result.clone(),
                        prompt: t.prompt.clone(),
                        arguments: t.arguments.clone(),
                        created_at: t.created_at.clone(),
                    })
                    .collect(),
                ..Default::default()
            },
            clock,
            model_context_limit,
            timestamp_config: timestamp_config.clone(),
            timezone: Some(self.tz.clone()),
            server_tz: Some(self.tz.clone()),
            provider_supports_web_search,
        };

        let initial = match orchestrator::process_message(&mut deps, &input).await {
            Ok(r) => r,
            Err(e) => {
                // v4 handleStreamError: the transport-shell error frame. The
                // frame reaches only a live SSE client; the server was otherwise
                // silent about a failed send — emit a server-side event beside it
                // so the console/log surface records it too (P4.18).
                tracing::error!(
                    target: "quilltap::chat",
                    chat_id = %chat_id,
                    error = %e,
                    "Chat send failed while streaming the initial turn",
                );
                let _ = self.events.send(Event::chat_error(
                    &chat_id,
                    ChatErrorPayload {
                        error: "Failed to generate response".to_string(),
                        error_type: "Error".to_string(),
                        details: e.to_string(),
                    },
                ));
                return Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: e.to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                });
            }
        };

        let dto = ChatSendResultDto {
            message_id: initial.message_id.clone(),
            has_content: initial.has_content,
            is_multi_character: initial.is_multi_character,
            is_paused: initial.is_paused,
            user_participant_id: initial.user_participant_id.clone(),
        };

        // v4's handleSendMessage ALWAYS drives executeTurnChain; the chain's
        // own guard decides whether a turn fires. Each chained turn re-enters
        // process_message in continue mode with a live wall clock.
        let chain_tz = self.tz.clone();
        let chain_tcfg = timestamp_config.clone();
        let chain_chat_id = chat_id.clone();
        let tz_name = self.tz.clone();
        let make_chain_input = move |pid: String| {
            let now = now_unix_ms();
            // `self` is out of reach inside this move closure, hence the free fn.
            let offset = js_local_offset_minutes(&tz_name, now);
            ProcessMessageInput {
                log_context: LogContext::none(),
                chat_id: chain_chat_id.clone(),
                user_id: SINGLE_USER_ID.to_string(),
                options: SendMessageOptions {
                    continue_mode: true,
                    content: String::new(),
                    responding_participant_id: Some(pid),
                    ..Default::default()
                },
                clock: ProcessClock {
                    now_ms: now,
                    local_offset_minutes: offset,
                    random01: os_random01(),
                },
                model_context_limit,
                timestamp_config: chain_tcfg.clone(),
                timezone: Some(chain_tz.clone()),
                server_tz: Some(chain_tz.clone()),
                provider_supports_web_search,
            }
        };
        // === P4.6BM ===
        // v4 gates its post-cycle conversation-render trigger on the INITIAL
        // result's `hasContent` (`orchestrator.service.ts:236`), which the
        // chain options take by value below — capture it first.
        let initial_had_content = initial.has_content;
        // === end P4.6BM ===
        let chain_result = orchestrator::execute_turn_chain(
            &mut deps,
            ExecuteTurnChainOptions {
                chat_id: chat_id.clone(),
                initial_result: initial,
                initial_continue_mode: req.continue_mode,
                never_pause_for_user: false,
                single_turn: false,
                chain_start_time_ms: now_ms,
                config: ChainConfig::default(),
            },
            now_ms,
            random01,
            make_chain_input,
        )
        .await;
        if let Err(e) = chain_result {
            // v4's chain errors reach the same stream shell; the initial turn
            // already succeeded, so surface the frame and keep the result (v4's
            // response stream closes after the frame). Log it server-side too
            // (P4.18) — the initial turn's success would otherwise hide a
            // silently-broken chain.
            tracing::error!(
                target: "quilltap::chat",
                chat_id = %chat_id,
                error = %e,
                "Turn chain failed after the initial turn",
            );
            let _ = self.events.send(Event::chat_error(
                &chat_id,
                ChatErrorPayload {
                    error: "Failed to generate response".to_string(),
                    error_type: "Error".to_string(),
                    details: e.to_string(),
                },
            ));
        }

        // === P4.6BM ===
        // v4's post-cycle Scriptorium trigger (`orchestrator.service.ts:236-247`)
        // — "runs on every turn with content", after the chain, swallowing every
        // failure. This is what keeps `renderedMarkdown` and the interchange
        // chunks current as a conversation grows; without it only the manual
        // button and the boot reconcile ever render. (v4's sibling scene-state
        // trigger stays unported — SCENE_STATE_TRACKING has no handler.)
        if initial_had_content {
            quilltap_core::services::conversation_render_job::trigger_conversation_render(
                &self.db,
                quilltap_core::api::SINGLE_USER_ID,
                &chat_id,
            )
            .await;
        }
        // === end P4.6BM ===

        Ok(dto)
    }

    /// One Brahma Console send turn (the orchestrator — `chat_send`'s sibling).
    /// A far simpler composition than [`Self::run_send`]: no participants / turn
    /// chain / carina / danger / confirmation / compression — just the streaming
    /// provider + the operator tool runner/detector + the pricing cost tracker,
    /// around the ported [`handle_brahma_console_message`]. Returns the send reply
    /// body; on failure emits v4's transport-shell error frame (`fatal_error`) and
    /// returns the `CoreError`.
    async fn run_brahma_send(
        self,
        req: quilltap_core::api::brahma::BrahmaConsoleSendRequest,
    ) -> Result<serde_json::Value, CoreError> {
        use quilltap_core::services::brahma_console::orchestrator::{
            handle_brahma_console_message, BrahmaConsoleSendOptions, BrahmaSendDeps,
        };

        let db = self.db.clone();
        let chat_id = req.chat_id.clone();
        let sink = BroadcastSink {
            chat_id: chat_id.clone(),
            tx: self.events.clone(),
        };
        let detector = RegistryToolCallDetector::built_in();
        let runner = self.tool_runner();
        let mut cost = PricingCostTracker {
            fetcher: &self.pricing,
            ctx: build_pricing_context(&db, &req.user_id),
        };
        let mut deps = BrahmaSendDeps {
            db: &db,
            streaming: &*self.streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            cost: &mut cost,
            // `model_supports_native_tools: true` matches v4's checkModelSupportsTools
            // default for models absent from the fallback table (the console resolves
            // its own per-chat profile, which the spine cannot know here) — the same
            // choice run_send makes for the carina/Brahma engines.
            model_supports_native_tools: true,
        };
        let opts = BrahmaConsoleSendOptions {
            content: req.content.clone(),
            file_ids: req.file_ids.clone(),
        };
        match handle_brahma_console_message(&mut deps, &sink, &req.user_id, &chat_id, &opts).await {
            Ok(result) => Ok(serde_json::json!({ "messageId": result.message_id })),
            Err(e) => {
                // v4 `encodeErrorEvent(message, 'fatal_error', '')`.
                tracing::error!(
                    target: "quilltap::chat",
                    chat_id = %chat_id,
                    error = %e.message,
                    "Brahma Console send failed",
                );
                let _ = self.events.send(Event::chat_error(
                    &chat_id,
                    ChatErrorPayload {
                        error: e.message.clone(),
                        error_type: "fatal_error".to_string(),
                        details: String::new(),
                    },
                ));
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: e.message,
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                })
            }
        }
    }

    /// One autonomous-room step (the `AUTONOMOUS_ROOM_TURN` runner body): the
    /// same deps construction as [`Self::run_send`] around the ported
    /// [`enclave_step`]. The step selects its own speaker, so the profile
    /// pre-resolve is best-effort (module header).
    async fn run_step(
        self,
        job_id: String,
        job_created_at: String,
        user_id: String,
        payload: AutonomousRoomTurnPayload,
    ) -> Result<StepOutcome, DbError> {
        let db = self.db.clone();
        let chat_id = payload.chat_id.clone().unwrap_or_default();
        let sink = BroadcastSink {
            chat_id: chat_id.clone(),
            tx: self.events.clone(),
        };

        let now_ms = now_unix_ms();
        let random01 = os_random01();
        let resolved = self
            .preresolve_provider_model(&chat_id, None, None, None, true, random01)
            .await;
        let (model_context_limit, provider_supports_web_search) = self.registry_inputs(resolved);
        let timestamp_config = self.resolve_timestamp_config(&chat_id);

        // The TURN's executor carries the run's LogContext via the step itself
        // (ProcessMessageInput.log_context); the cheap-LLM executor here is the
        // per-call logging one, and the FOLD executor is untagged (StepDeps doc).
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: user_id.clone(),
            chat_id: Some(chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let fold_executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: user_id.clone(),
            chat_id: Some(chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });

        let bc_seams = RealBuildContextSeams { db: &db };
        let orchestrator_seams = HostOrchestratorSeams { db: db.clone() };
        let router = DangerContentRouter::new(db.clone(), DbApiKeys(db.clone()));
        let mut confirmation = TimeoutConfirmation::new(RealAnswerConfirmation {
            completion: &*self.completion,
            executor: &executor,
        });
        let mut compression = RealAsyncCompression {
            db: &db,
            completion: &*self.completion,
            executor: &executor,
        };
        let mut cost = PricingCostTracker {
            fetcher: &self.pricing,
            ctx: build_pricing_context(&db, SINGLE_USER_ID),
        };
        let mut prospero = ThreadedProspero { db: db.clone() };
        let brahma = RealBrahmaConsole::new(
            db.clone(),
            Arc::clone(&self.streaming),
            self.tool_runner(),
            RegistryToolCallDetector::built_in(),
            true,
        );
        let ask_carina = ErasedAskCarina::new(TypedAskCarina {
            db: db.clone(),
            embedding: Arc::clone(&self.embedding),
            streaming: Arc::clone(&self.streaming),
            tool_runner: self.tool_runner(),
            tool_detector: RegistryToolCallDetector::built_in(),
            brahma: RealBrahmaConsole::new(
                db.clone(),
                Arc::clone(&self.streaming),
                self.tool_runner(),
                RegistryToolCallDetector::built_in(),
                true,
            ),
            prospero: ThreadedProspero { db: db.clone() },
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
        });
        let carina_tool_runner = self.tool_runner();
        let carina_detector = RegistryToolCallDetector::built_in();
        let mut carina_query = RealCarinaQuery::new(CarinaQueryDeps {
            db: &db,
            embedding: &*self.embedding,
            streaming: &*self.streaming,
            tool_runner: &carina_tool_runner,
            tool_detector: &carina_detector,
            sink: &sink,
            brahma: &brahma,
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
        });
        let mut rng_bytes = OsRandomBytes;

        let mut deps = OrchestratorDeps {
            db: &db,
            embedding: &*self.embedding,
            completion: &*self.completion,
            streaming: &*self.streaming,
            executor: &executor,
            ask_carina: &ask_carina,
            sink: &sink,
            pricing: &self.pricing,
            build_context_seams: &bc_seams,
            orchestrator_seams: &orchestrator_seams,
            file_bytes: &*self.file_bytes,
            image_transcoder: &*self.image_transcoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina_query,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
            // P4.42: the in-chat turn's `search_web` provider (None until
            // SERPER_API_KEY is set).
            web_search: self.web_search.clone(),
        };

        let now_fn = quilltap_core::enclave::announce::system_now_ms;
        let mint_fn = quilltap_core::enclave::announce::system_mint_uuid;
        let sdeps = StepDeps {
            now_ms: &now_fn,
            mint_uuid: &mint_fn,
            tz: &self.tz,
            random01,
            fold_executor: &fold_executor,
            model_context_limit,
            timestamp_config,
            timezone: Some(self.tz.clone()),
            local_offset_minutes: self.local_offset_minutes(now_ms),
            provider_supports_web_search,
        };
        let meta = TurnJobMeta {
            job_id: &job_id,
            job_created_at: &job_created_at,
            user_id: &user_id,
        };
        enclave_step(&mut deps, &sdeps, &meta, &payload).await
    }
}

// ===========================================================================
// The chat-create driver (P4.4u2b)
// ===========================================================================

/// Map a [`HandleCreateError`] to the transport [`CoreError`] (the same shape
/// the engine's `ChatCreate` arm returns).
fn map_create_error(e: HandleCreateError) -> CoreError {
    let kind = match &e {
        HandleCreateError::NotFound(_) => ErrorKind::NotFound,
        HandleCreateError::BadRequest(_) => ErrorKind::BadRequest,
        HandleCreateError::Db(_) => ErrorKind::Internal,
    };
    CoreError {
        kind,
        message: e.to_string(),
        pepper_state: None,
        code: None,
        associations: None,
        character_id: None,
        entity: None,
    }
}

/// The production [`ChatCreateDriver`]: assembles [`ChatCreateDeps`] and runs
/// [`handle_create`] per dispatch, sharing the [`ChatSpine`] provider Arcs.
///
/// Unlike [`ChatSpine`], the create spine opens its OWN writable
/// [`Writer`]s (main + mount-index + optional llm-logs) per create: the outfit
/// sub-unit (`apply_outfit_selections`) holds writable `&Connection`s across an
/// LLM await, which the sync single-writer [`Db::write`](Db) closure cannot host.
/// A `busy_timeout` guards the rare overlap with the engine `Db`'s writer thread
/// (they are used sequentially within one create). Green-Room frames ride the
/// engine broadcast AND the shared [`CreationProgressBus`] (replay-on-subscribe).
pub struct ChatCreateSpine<EMB, CMP, STR>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
{
    pub db: Db,
    pub events: tokio::sync::broadcast::Sender<Event>,
    pub bus: Arc<CreationProgressBus>,
    /// The instance pepper (opens the per-create writable partitions).
    pub pepper: String,
    /// The instance `<base>/data` dir (where the partitions live).
    pub data_dir: PathBuf,
    pub embedding: Arc<EMB>,
    pub completion: Arc<CMP>,
    pub streaming: Arc<STR>,
    pub tz: String,
}

impl<EMB, CMP, STR> ChatCreateSpine<EMB, CMP, STR>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
{
    fn clone_state(&self) -> Self {
        ChatCreateSpine {
            db: self.db.clone(),
            events: self.events.clone(),
            bus: Arc::clone(&self.bus),
            pepper: self.pepper.clone(),
            data_dir: self.data_dir.clone(),
            embedding: Arc::clone(&self.embedding),
            completion: Arc::clone(&self.completion),
            streaming: Arc::clone(&self.streaming),
            tz: self.tz.clone(),
        }
    }

    /// One full chat-creation flow (the non-`Send` inner the dedicated thread
    /// runs).
    async fn run_create(
        self,
        req: ChatCreateDriverRequest,
    ) -> Result<ChatCreateResultDto, CoreError> {
        let request: ChatCreateRequest =
            serde_json::from_value(req.raw).map_err(|e| CoreError {
                kind: ErrorKind::BadRequest,
                message: format!("invalid chatCreate request: {e}"),
                pepper_state: None,
                code: None,
                associations: None,
                character_id: None,
                entity: None,
            })?;

        // Open the OWN writable partitions (module note). `busy_timeout` guards
        // the rare overlap with the engine Db's writer thread.
        let busy = std::time::Duration::from_millis(5000);
        let open = |name: &str| -> Result<Writer, CoreError> {
            let w =
                Writer::open_writable(&self.data_dir.join(name), &self.pepper).map_err(|e| {
                    CoreError {
                        kind: ErrorKind::Internal,
                        message: format!("open {name}: {e}"),
                        pepper_state: None,
                        code: None,
                        associations: None,
                        character_id: None,
                        entity: None,
                    }
                })?;
            w.connection().busy_timeout(busy).map_err(|e| CoreError {
                kind: ErrorKind::Internal,
                message: format!("busy_timeout {name}: {e}"),
                pepper_state: None,
                code: None,
                associations: None,
                character_id: None,
                entity: None,
            })?;
            Ok(w)
        };
        let main_writer = open("quilltap.db")?;
        let mount_writer = open("quilltap-mount-index.db")?;
        let llm_writer = if self.data_dir.join("quilltap-llm-logs.db").exists() {
            Some(open("quilltap-llm-logs.db")?)
        } else {
            None
        };

        let db = self.db.clone();
        let now_ms = now_unix_ms();
        let random01 = os_random01();

        // The outfit `llm_choose` executor (per-call llm-logs logging).
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: SINGLE_USER_ID.to_string(),
            chat_id: None,
            message_id: None,
            ctx: LogContext::none(),
        });
        let api_keys = DbApiKeys(db.clone());

        // The lifecycle seam (the enclave-step construction): system clock +
        // UUID mint + the cron next-run evaluation.
        let now_fn = quilltap_core::enclave::announce::system_now_ms;
        let mint_fn = quilltap_core::enclave::announce::system_mint_uuid;
        let tz_for_cron = self.tz.clone();
        let cron_seam = move |expr: &str, anchor: i64| -> Result<Option<i64>, String> {
            cron::try_next_occurrence(expr, anchor, &tz_for_cron)
        };
        let lifecycle = LifecycleDeps {
            now_ms: &now_fn,
            mint_uuid: &mint_fn,
            next_occurrence: &cron_seam,
        };

        let deps = ChatCreateDeps {
            embedding: &*self.embedding,
            completion: &*self.completion,
            streaming: &*self.streaming,
            executor: &executor,
            api_keys: &api_keys,
            tz: self.tz.clone(),
            now_ms,
            random01,
            lifecycle: &lifecycle,
            greeting_log: true,
        };

        let emitter = CreationProgressEmitter::from_id(
            request.progress_id.as_deref(),
            Arc::clone(&self.bus),
            self.events.clone(),
        );

        let ChatCreateResult {
            mut chat,
            participants,
        } = handle_create(
            &db,
            main_writer.connection(),
            mount_writer.connection(),
            llm_writer.as_ref().map(|w| w.connection()),
            &deps,
            &request,
            &emitter,
        )
        .await
        .map_err(map_create_error)?;

        // v4 201 body: chat.participants := the enriched summaries.
        if let Value::Object(map) = &mut chat {
            map.insert(
                "participants".into(),
                serde_json::to_value(participants).unwrap_or(Value::Null),
            );
        }
        Ok(ChatCreateResultDto { chat })
    }
}

impl<EMB, CMP, STR> ChatCreateDriver for ChatCreateSpine<EMB, CMP, STR>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
{
    fn create(&self, req: ChatCreateDriverRequest) -> ChatCreateFuture<'_> {
        let state = self.clone_state();
        Box::pin(async move {
            // The Send bridge (the `ChatSpine::send` pattern): `handle_create`'s
            // future is non-`Send`, so it runs on its own thread + current-thread
            // runtime while the driver future awaits a oneshot.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(state.run_create(req)),
                    Err(e) => Err(CoreError {
                        kind: ErrorKind::Internal,
                        message: format!("chat create runtime: {e}"),
                        pepper_state: None,
                        code: None,
                        associations: None,
                        character_id: None,
                        entity: None,
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "chat create thread panicked".to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                })
            })
        })
    }
}

/// The local UTC offset (minutes) for `tz_name` at `now_ms`, in **JS
/// `getTimezoneOffset()` convention: positive = west of UTC** (Chicago in
/// summer is +300). jiff's `to_offset` is east-positive (Chicago −300), so
/// the sign flips here. Every core consumer of `local_offset_minutes`
/// (`chat_timestamp`'s zone-less arm) documents and SUBTRACTS the JS
/// convention — feeding it the raw jiff sign renders a no-timezone chat at
/// the mirrored offset on any non-UTC host (found at the 5cc76688-round
/// unification; invisible to the differentials, which all pin TZ=UTC where
/// both conventions read 0).
fn js_local_offset_minutes(tz_name: &str, now_ms: i64) -> i64 {
    use jiff::Timestamp;
    let tz = jiff::tz::TimeZone::get(tz_name).unwrap_or(jiff::tz::TimeZone::UTC);
    Timestamp::from_millisecond(now_ms)
        .map(|ts| -(tz.to_offset(ts).seconds() as i64) / 60)
        .unwrap_or(0)
}

/// `Math.random()` off the OS CSPRNG (via the ported `RandomBytes` source).
fn os_random01() -> f64 {
    use quilltap_core::tools::rng::RandomBytes as _;
    let bytes = OsRandomBytes.random_bytes(8);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    (u64::from_le_bytes(arr) >> 11) as f64 / (1u64 << 53) as f64
}

impl<EMB, CMP, STR, PF> ChatSendDriver for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn send(&self, req: ChatSendRequest) -> ChatSendFuture<'_> {
        let state = self.clone_state();
        Box::pin(async move {
            // The Send bridge (module header): the turn runs on its own thread
            // + current-thread runtime; the driver future awaits a oneshot.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(state.run_send(req)),
                    Err(e) => Err(CoreError {
                        kind: ErrorKind::Internal,
                        message: format!("spine runtime: {e}"),
                        pepper_state: None,
                        code: None,
                        associations: None,
                        character_id: None,
                        entity: None,
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "chat send thread panicked".to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                })
            })
        })
    }
}

impl<EMB, CMP, STR, PF> quilltap_core::api::brahma::BrahmaConsoleSendDriver
    for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn send(
        &self,
        req: quilltap_core::api::brahma::BrahmaConsoleSendRequest,
    ) -> quilltap_core::api::brahma::BrahmaConsoleSendFuture<'_> {
        let state = self.clone_state();
        Box::pin(async move {
            // The Send bridge (module header): the turn runs on its own thread +
            // current-thread runtime; the driver future awaits a oneshot.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(state.run_brahma_send(req)),
                    Err(e) => Err(CoreError {
                        kind: ErrorKind::Internal,
                        message: format!("spine runtime: {e}"),
                        pepper_state: None,
                        code: None,
                        associations: None,
                        character_id: None,
                        entity: None,
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "brahma send thread panicked".to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                })
            })
        })
    }
}

impl<EMB, CMP, STR, PF> quilltap_core::api::chat_send::SwipeGenerateDriver
    for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn generate_swipe(
        &self,
        req: quilltap_core::api::chat_send::SwipeGenerateRequest,
    ) -> quilltap_core::api::chat_send::SwipeGenerateFuture<'_> {
        let state = self.clone_state();
        Box::pin(async move {
            // The Send bridge (module header): `regenerate_message_as_swipe`'s
            // future is non-`Send` (the same buildContext composition), so it runs
            // on its own thread + current-thread runtime.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(state.run_swipe(req)),
                    Err(e) => Err(CoreError {
                        kind: ErrorKind::Internal,
                        message: format!("spine runtime: {e}"),
                        pepper_state: None,
                        code: None,
                        associations: None,
                        character_id: None,
                        entity: None,
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "swipe generation thread panicked".to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                })
            })
        })
    }
}

impl<EMB, CMP, STR, PF> quilltap_core::api::chat_media::CourierResolveDriver
    for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn resolve_external_turn(
        &self,
        chat_id: String,
        message_id: String,
        reply_content: String,
    ) -> quilltap_core::api::chat_media::CourierResolveFuture<'_> {
        let state = self.clone_state();
        Box::pin(async move {
            // The dedicated-thread bridge (the chat-send/swipe idiom): the resolve
            // settle re-enters the cheap-LLM triggers, whose futures are non-`Send`.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(async {
                        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                            db: state.db.clone(),
                            user_id: SINGLE_USER_ID.to_string(),
                            chat_id: Some(chat_id.clone()),
                            message_id: None,
                            ctx: LogContext::none(),
                        });
                        quilltap_core::api::chat_media::message_resolve_external_turn(
                            &state.db,
                            &*state.completion,
                            &*state.embedding,
                            &executor,
                            SINGLE_USER_ID,
                            &chat_id,
                            &message_id,
                            &reply_content,
                            now_unix_ms(),
                        )
                        .await
                    }),
                    Err(e) => quilltap_core::api::Response::error(
                        ErrorKind::Internal,
                        format!("spine runtime: {e}"),
                    ),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                quilltap_core::api::Response::error(
                    ErrorKind::Internal,
                    "courier resolve thread panicked".to_string(),
                )
            })
        })
    }
}

impl<EMB, CMP, STR, PF> quilltap_core::api::recall_replay::RecallReplayDriver
    for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn run(
        &self,
        input: quilltap_core::services::recall_replay::RunRecallReplayInput,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send + '_>,
    > {
        let state = self.clone_state();
        Box::pin(async move {
            // The dedicated-thread bridge (the courier-resolve idiom): the
            // replay's distill re-enters the cheap-LLM executor, whose futures
            // are non-`Send`.
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(async {
                        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                            db: state.db.clone(),
                            user_id: SINGLE_USER_ID.to_string(),
                            chat_id: Some(input.chat_id.clone()),
                            message_id: None,
                            ctx: LogContext::none(),
                        });
                        quilltap_core::services::recall_replay::run_recall_replay(
                            &state.db,
                            &*state.completion,
                            &executor,
                            &*state.embedding,
                            &input,
                            // The host's own zone: the dispatch layer that built
                            // `input` has none to give (see the fn's doc).
                            Some(&state.tz),
                        )
                        .await
                    }),
                    Err(e) => Err(format!("spine runtime: {e}")),
                };
                let _ = tx.send(result);
            });
            rx.await
                .unwrap_or_else(|_| Err("recall replay thread panicked".to_string()))
        })
    }
}

/// The `AUTONOMOUS_ROOM_TURN` step-runner closure over a shared spine — the
/// dedicated-thread bridge the enclave E2E pinned (the step future is
/// non-`Send`; `StepFuture` must be).
/// Constrain a closure to the HRTB step-runner shape (closure lifetime
/// inference needs the nudge).
fn constrain_step_runner<F>(f: F) -> F
where
    F: for<'a> Fn(&'a Db, &'a BackgroundJob, AutonomousRoomTurnPayload) -> StepFuture<'a>,
{
    f
}

pub fn autonomous_turn_handler<EMB, CMP, STR, PF>(
    spine: Arc<ChatSpine<EMB, CMP, STR, PF>>,
) -> AutonomousRoomTurnHandler<
    impl for<'a> Fn(&'a Db, &'a BackgroundJob, AutonomousRoomTurnPayload) -> StepFuture<'a>
        + Send
        + Sync,
>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    AutonomousRoomTurnHandler {
        run_step: constrain_step_runner(
            move |_db: &Db, job: &BackgroundJob, payload: AutonomousRoomTurnPayload| {
                let state = spine.clone_state();
                let job_id = job.id.clone();
                let job_created_at = job.created_at.clone();
                let user_id = job.user_id.clone();
                Box::pin(async move {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    std::thread::spawn(move || {
                        let result = match tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            Ok(rt) => rt.block_on(state.run_step(
                                job_id,
                                job_created_at,
                                user_id,
                                payload,
                            )),
                            Err(e) => Err(DbError::Key(format!("step runtime: {e}"))),
                        };
                        let _ = tx.send(result);
                    });
                    rx.await
                        .unwrap_or_else(|_| Err(DbError::Key("step thread panicked".into())))
                }) as StepFuture<'_>
            },
        ),
    }
}

// ===========================================================================
// The model-dependent job handlers (thin payload-decode wrappers)
// ===========================================================================

/// `MEMORY_HOUSEKEEPING` — v4 `handleMemoryHousekeeping`
/// (`lib/background-jobs/handlers/memory-housekeeping.ts`): the auto-settings
/// gate, the explicit-vs-all character resolution, the payload-over-settings
/// option cascade, and the outcome-cache record — glue over the ported
/// [`run_housekeeping`] + settings/outcome readers (module header).
pub struct MemoryHousekeepingHandler;

impl JobHandler for MemoryHousekeepingHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let uid = job.user_id.clone();
            let auto = db
                .read_main(move |conn| {
                    chat_settings::find_auto_housekeeping_settings_by_user_id(conn, &uid)
                })
                .unwrap_or(None);
            let auto_enabled = auto
                .as_ref()
                .and_then(|a| a.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let reason = payload.get("reason").and_then(Value::as_str);
            // v4: automatic triggers bail when auto-housekeeping is off;
            // manual / reason-less jobs still run.
            if !auto_enabled {
                if let Some(r) = reason {
                    if r != "manual" {
                        return JobOutcome::Completed(None);
                    }
                }
            }

            // Explicit characterId wins; otherwise sweep every character.
            let mut targets: Vec<String> = Vec::new();
            if let Some(cid) = payload.get("characterId").and_then(Value::as_str) {
                targets.push(cid.to_string());
            } else {
                let uid = job.user_id.clone();
                let chars = db.read_main(move |main| {
                    db.read_mount_index(|mount| {
                        quilltap_core::db::characters_read::find_by_user_id(main, mount, &uid)
                    })
                });
                match chars {
                    Ok(rows) => targets.extend(
                        rows.iter()
                            .filter_map(|c| c.get("id").and_then(Value::as_str))
                            .map(String::from),
                    ),
                    Err(e) => {
                        // v4 logs and RETURNS (a completed no-op).
                        let _ = e;
                        return JobOutcome::Completed(None);
                    }
                }
            }

            // Payload overrides win over user settings, which win over the
            // housekeeping defaults (absent = None → the service default).
            let g = |v: Option<&Value>, k: &str| v.and_then(|o| o.get(k)).cloned();
            let merge_threshold = payload
                .get("mergeThreshold")
                .and_then(Value::as_f64)
                .or_else(|| g(auto.as_ref(), "autoMergeSimilarThreshold").and_then(|v| v.as_f64()));
            let merge_similar = payload
                .get("mergeSimilar")
                .and_then(Value::as_bool)
                .or_else(|| g(auto.as_ref(), "mergeSimilar").and_then(|v| v.as_bool()));
            let dry_run = payload
                .get("dryRun")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            for character_id in &targets {
                let per_char_cap = payload
                    .get("maxMemories")
                    .and_then(Value::as_f64)
                    .or_else(|| {
                        auto.as_ref()
                            .and_then(|a| a.get("perCharacterCapOverrides"))
                            .and_then(|o| o.get(character_id.as_str()))
                            .and_then(Value::as_f64)
                    })
                    .or_else(|| g(auto.as_ref(), "perCharacterCap").and_then(|v| v.as_f64()));
                let options = HousekeepingOptions {
                    max_memories: per_char_cap.map(|v| v as usize),
                    merge_threshold,
                    merge_similar,
                    dry_run: Some(dry_run),
                    ..Default::default()
                };
                // v4 catches per-character errors and continues the sweep.
                if let Ok(result) = run_housekeeping(db, character_id, &options).await {
                    if !dry_run {
                        record_housekeeping_outcome(
                            character_id,
                            result.deleted as f64,
                            result.total_before as f64,
                            result.cap_used as f64,
                        );
                    }
                }
            }
            JobOutcome::Completed(None)
        })
    }
}

/// `CHAT_DANGER_CLASSIFICATION` — a payload decode around the
/// differential-verified
/// [`handle_chat_danger_classification`] with the real moderation/completion
/// providers + the live Concierge announcer.
pub struct ChatDangerClassificationHandler {
    pub wire: WireConfig,
}

impl JobHandler for ChatDangerClassificationHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let Some(chat_id) = payload.get("chatId").and_then(Value::as_str) else {
                return JobOutcome::Failed(
                    "CHAT_DANGER_CLASSIFICATION payload missing chatId".into(),
                );
            };
            let connection_profile_id = payload
                .get("connectionProfileId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let cjob = ChatDangerClassificationJob {
                id: job.id.clone(),
                user_id: job.user_id.clone(),
                chat_id: chat_id.to_string(),
                connection_profile_id: connection_profile_id.to_string(),
            };
            let moderation = RealModerationProvider::new(
                db.clone(),
                DbApiKeys(db.clone()),
                ReqwestWireTransport::new(),
            );
            let completion = self.wire.completion(db);
            let announcer = RealDangerAnnouncer { db };
            match handle_chat_danger_classification(db, &moderation, &completion, &announcer, &cjob)
                .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e.to_string()),
            }
        })
    }
}

/// The pricing-backed [`MessageCostEstimator`] — the same cascade the finalizer's
/// cost tracker uses. The one host impl of the seam, shared by every consumer of
/// v4's `estimateMessageCost`: the TITLE_GENERATION system event's
/// `estimatedCostUSD` and the carina handler's MEMORY_EXTRACTION event.
pub struct PricingMessageCost<PF: PricingFetch + Send + Sync> {
    pub fetcher: Arc<PricingFetcher<PF>>,
    pub db: Db,
}

impl<PF: PricingFetch + Send + Sync> MessageCostEstimator for PricingMessageCost<PF> {
    fn estimate(
        &self,
        provider: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        user_id: &str,
    ) -> impl std::future::Future<Output = Option<f64>> + Send {
        let ctx = build_pricing_context(&self.db, user_id);
        let result = self.fetcher.estimate_message_cost(
            Registry::built_in(),
            provider,
            model,
            prompt_tokens,
            completion_tokens,
            now_unix_ms(),
            &ctx,
        );
        async move { result.cost }
    }
}

/// `TITLE_UPDATE` (P4.6ao) — a payload decode around the differential-verified
/// [`handle_title_update`](quilltap_core::services::title_update_job::handle_title_update).
/// Rebuilt per job so the cheap-LLM executor's log context carries this job's
/// user/chat (the [`StoryBackgroundJobHandler`] precedent). `context_summary`
/// enqueues these at every title checkpoint; before this registration they died
/// on the runner's loud fallback, which also meant the automatic
/// story-background trigger never fired.
pub struct TitleUpdateJobHandler<PF: PricingFetch + Send + Sync + 'static> {
    pub wire: WireConfig,
    pub pricing: Arc<PricingFetcher<PF>>,
}

impl<PF: PricingFetch + Send + Sync + 'static> JobHandler for TitleUpdateJobHandler<PF> {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let chat_id = payload
                .get("chatId")
                .and_then(Value::as_str)
                .map(String::from);
            let inner = TitleUpdateHandler {
                completion: self.wire.completion(db),
                executor: CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                    db: db.clone(),
                    user_id: job.user_id.clone(),
                    chat_id,
                    message_id: None,
                    ctx: LogContext::none(),
                }),
                now_ms: now_unix_ms(),
                cost: PricingMessageCost {
                    fetcher: Arc::clone(&self.pricing),
                    db: db.clone(),
                },
            };
            inner.handle(db, job).await
        })
    }
}

/// `CARINA_MEMORY_EXTRACTION` — a payload decode around the
/// differential-verified [`handle_carina_memory_extraction`].
/// `memory_extraction_limits: None` keeps the service defaults (the
/// instance-settings reader is a tracked deferral there).
pub struct CarinaMemoryExtractionHandler<PF: PricingFetch + Send + Sync + 'static> {
    pub wire: WireConfig,
    pub pricing: Arc<PricingFetcher<PF>>,
}

impl<PF: PricingFetch + Send + Sync + 'static> JobHandler for CarinaMemoryExtractionHandler<PF> {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let g = |k: &str| {
                payload
                    .get(k)
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_default()
            };
            let cpayload = CarinaMemoryExtractionPayload {
                chat_id: g("chatId"),
                carina_message_id: g("carinaMessageId"),
                answerer_id: g("answererId"),
                connection_profile_id: g("connectionProfileId"),
            };
            let completion = self.wire.completion(db);
            let embedding = ApiEmbeddingProvider::new(db.clone(), ReqwestWireTransport::new());
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: job.user_id.clone(),
                chat_id: Some(cpayload.chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            let cost = PricingMessageCost {
                fetcher: Arc::clone(&self.pricing),
                db: db.clone(),
            };
            match handle_carina_memory_extraction(
                db,
                &completion,
                &embedding,
                &executor,
                &cost,
                &job.user_id,
                &cpayload,
                // v4 `getMemoryExtractionLimits()` — the instance-settings read
                // (P4.6bj closed the reader deferral; a read failure keeps the
                // service defaults, as before).
                read_memory_extraction_limits(db),
            )
            .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e.to_string()),
            }
        })
    }
}

/// v4 `getMemoryExtractionLimits()` — the instance-settings read + parse the
/// extraction handlers resolve above the core seam. `None` on a read failure
/// (the service falls back to its defaults).
fn read_memory_extraction_limits(db: &Db) -> Option<MemoryExtractionLimits> {
    db.read_main(quilltap_core::db::instance_settings::get_memory_extraction_limits)
        .ok()
        .map(|v| limits_from_value(&v))
}

/// `MEMORY_EXTRACTION` (P4.6bj) — a payload decode around the
/// differential-verified
/// [`handle_memory_extraction`](quilltap_core::services::memory_extraction_job::handle_memory_extraction).
/// The finalizer enqueues one per closed turn; before this registration the
/// whole episodic-campaign extraction pipeline was verified but DORMANT (every
/// job died on the runner's loud fallback).
pub struct MemoryExtractionJobHandler<PF: PricingFetch + Send + Sync + 'static> {
    pub wire: WireConfig,
    pub pricing: Arc<PricingFetcher<PF>>,
}

impl<PF: PricingFetch + Send + Sync + 'static> JobHandler for MemoryExtractionJobHandler<PF> {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = MemoryExtractionPayload::decode(&payload);
            let completion = self.wire.completion(db);
            let embedding = ApiEmbeddingProvider::new(db.clone(), ReqwestWireTransport::new());
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: job.user_id.clone(),
                chat_id: Some(decoded.chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            let cost = PricingMessageCost {
                fetcher: Arc::clone(&self.pricing),
                db: db.clone(),
            };
            match handle_memory_extraction(
                db,
                &completion,
                &embedding,
                &executor,
                &cost,
                &job.user_id,
                &decoded,
                read_memory_extraction_limits(db),
            )
            .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e),
            }
        })
    }
}

/// `CONTEXT_SUMMARY` (P4.6bj) — a payload decode around the
/// differential-verified
/// [`handle_context_summary`](quilltap_core::services::context_summary_job::handle_context_summary).
/// The scheduled danger scan enqueues these; the handler folds with
/// `RealContextSummarySeams` (Librarian re-post / vault mirror / refresh /
/// cost events / episode pass all live) and chains the priority −2 danger
/// classification.
pub struct ContextSummaryJobHandler {
    pub wire: WireConfig,
}

impl JobHandler for ContextSummaryJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = ContextSummaryPayload::decode(&payload);
            let completion = self.wire.completion(db);
            let embedding = ApiEmbeddingProvider::new(db.clone(), ReqwestWireTransport::new());
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: job.user_id.clone(),
                chat_id: Some(decoded.chat_id.clone()),
                message_id: None,
                ctx: LogContext::none(),
            });
            match handle_context_summary(
                db,
                &completion,
                &embedding,
                &executor,
                &job.user_id,
                &decoded,
            )
            .await
            {
                Ok(_) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e),
            }
        })
    }
}

// === P4.6BL ===
/// `EMBEDDING_GENERATE` — a payload decode around the differential-verified
/// [`handle_embedding_generate`](quilltap_core::services::embedding_generate_job::handle_embedding_generate),
/// over the same API-path embedding provider the spine embeds with (the seam
/// P4.6s wired live). Before this registration BOTH enqueue families were live
/// with no handler — every job retried three times and died (dogfood finding
/// #35: 2,088 DEAD rows on the Friday copy, and every chunk/memory written
/// since v5 took over unembedded).
pub struct EmbeddingGenerateJobHandler;

impl JobHandler for EmbeddingGenerateJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = EmbeddingGeneratePayload::from_json(&payload);
            let embedding = ApiEmbeddingProvider::new(db.clone(), ReqwestWireTransport::new());
            match handle_embedding_generate(db, &embedding, &job.user_id, &decoded).await {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e),
            }
        })
    }
}
// === end P4.6BL ===

// === P4.24 ===
/// `LLM_LOG_CLEANUP` — a payload decode around the differential-verified
/// [`handle_llm_log_cleanup`](quilltap_core::services::llm_log_cleanup_job::handle_llm_log_cleanup),
/// with the host's wall clock and configured zone supplied per job (core reads
/// no ambient clock, and the retention cutoff is LOCAL calendar-day arithmetic —
/// see `db::llm_logs::llm_log_retention_cutoff_iso`).
///
/// This was the LAST type in `KNOWN_JOB_TYPES` without a handler. The enqueuer
/// runs on the daily cadence AND immediately at boot, so until this registration
/// every start-up minted a job that burned three attempts against the
/// "recognized but not yet available" arm and died — while the retention window
/// on the real instance was quietly being maintained by v4 (dogfood finding
/// #40).
pub struct LlmLogCleanupJobHandler {
    pub tz: String,
}

impl JobHandler for LlmLogCleanupJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let decoded = LlmLogCleanupPayload::from_json(&payload);
            let now_ms = quilltap_core::clock::now_unix_ms();
            // v4 reads `job.userId`, the row's own value — not the payload's.
            match handle_llm_log_cleanup(db, &job.user_id, &decoded, now_ms, &self.tz).await {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e),
            }
        })
    }
}
// === end P4.24 ===

/// `CHARACTER_AVATAR_GENERATION` — constructs the core 7-generic handler per
/// job (the core handler pins `now_ms` at construction; production wants the
/// wall clock at job time).
pub struct AvatarJobHandler {
    pub wire: WireConfig,
}

impl JobHandler for AvatarJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let inner = CharacterAvatarGenerationHandler {
                image_provider: quilltap_core::model::image_dialects::RealImageProvider::new(
                    ReqwestWireTransport::new(),
                ),
                completion: self.wire.completion(db),
                moderation: RealModerationProvider::new(
                    db.clone(),
                    DbApiKeys(db.clone()),
                    ReqwestWireTransport::new(),
                ),
                api_keys: DbApiKeys(db.clone()),
                transcoder: HostImageCodec,
                upload: RealProjectImageUpload {
                    db: db.clone(),
                    codec: Arc::new(HostImageCodec),
                },
                now_ms: now_unix_ms(),
                orientation_data_for: quilltap_core::image_gen_data::orientation_data_for,
            };
            inner.handle(db, job).await
        })
    }
}

/// `STORY_BACKGROUND_GENERATION` — same shape as the avatar wrapper, plus the
/// scene-task executor.
pub struct StoryBackgroundJobHandler {
    pub wire: WireConfig,
}

impl JobHandler for StoryBackgroundJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let chat_id = payload
                .get("chatId")
                .and_then(Value::as_str)
                .map(String::from);
            let inner = StoryBackgroundGenerationHandler {
                image_provider: quilltap_core::model::image_dialects::RealImageProvider::new(
                    ReqwestWireTransport::new(),
                ),
                completion: self.wire.completion(db),
                moderation: RealModerationProvider::new(
                    db.clone(),
                    DbApiKeys(db.clone()),
                    ReqwestWireTransport::new(),
                ),
                api_keys: DbApiKeys(db.clone()),
                transcoder: HostImageCodec,
                upload: RealProjectImageUpload {
                    db: db.clone(),
                    codec: Arc::new(HostImageCodec),
                },
                executor: CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                    db: db.clone(),
                    user_id: job.user_id.clone(),
                    chat_id,
                    message_id: None,
                    ctx: LogContext::none(),
                }),
                now_ms: now_unix_ms(),
                orientation_data_for: quilltap_core::image_gen_data::orientation_data_for,
            };
            inner.handle(db, job).await
        })
    }
}

/// The `imageProfileGenerate` dispatch runner (P4.6ai) — the host's live
/// `EngineAssembly.image_generation` seam. Mirrors the avatar/story JOB handlers:
/// it rebuilds the W4.9a [`ImageGenDeps`] per run so `now_ms` is the wall clock at
/// request time AND the cheap-LLM executor's log context carries the request's
/// user/chat (the same reason [`AvatarJobHandler`] reconstructs per job). The
/// `Real*Provider`s are the W4.7f wire seams; the Lantern character-image
/// notification posts LIVE.
pub struct HostImageGenerationRunner {
    pub wire: WireConfig,
}

impl ImageGenerationRunner for HostImageGenerationRunner {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        input: &'a ImageGenerationToolInput,
        ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>
    {
        Box::pin(async move {
            let image_provider = quilltap_core::model::image_dialects::RealImageProvider::new(
                ReqwestWireTransport::new(),
            );
            let completion = self.wire.completion(db);
            let moderation = RealModerationProvider::new(
                db.clone(),
                DbApiKeys(db.clone()),
                ReqwestWireTransport::new(),
            );
            let api_keys = DbApiKeys(db.clone());
            let transcoder = HostImageCodec;
            let lantern = RealLanternNotification { db };
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: ctx.user_id.clone(),
                chat_id: ctx.chat_id.clone(),
                message_id: None,
                ctx: LogContext::none(),
            });
            let orientation_fn = quilltap_core::image_gen_data::orientation_data_for;
            let deps = ImageGenDeps {
                image_provider: &image_provider,
                completion: &completion,
                moderation: &moderation,
                api_keys: &api_keys,
                transcoder: &transcoder,
                lantern: &lantern,
                executor: &executor,
                now_ms: now_unix_ms(),
                orientation_data_for: &orientation_fn,
            };
            execute_image_generation_tool(db, &deps, input, ctx).await
        })
    }
}

// ===========================================================================
// The spine factory (the assembler's per-assembly construction seam)
// ===========================================================================

/// One assembly's spine products.
pub struct SpineBundle {
    pub chat_send: Arc<dyn ChatSendDriver>,
    /// The chat-creation driver (P4.4u2b).
    pub chat_create: Arc<dyn ChatCreateDriver>,
    /// The swipe-generate model driver (P4.6c; `None` for canned test
    /// factories that predate it).
    pub swipe_generate: Option<Arc<dyn quilltap_core::api::chat_send::SwipeGenerateDriver>>,
    /// The provider wire-actions driver (P4.6d; `None` for canned test
    /// factories).
    pub provider_actions: Option<Arc<dyn quilltap_core::api::ProviderActionsDriver>>,
    /// The memory embedding provider the `MemoryCreate`/`MemorySearch` dispatch
    /// arms run LIVE over (P4.6s, wired at the P4.6stu unification; `None` for
    /// canned test factories — the arms answer "not assembled").
    pub memory_embedding: Option<quilltap_core::model::embedding::ErasedEmbeddingProvider>,
    /// The courier-resolve driver (P4.6ab, the unification wire; `None` for
    /// canned test factories — the arm answers the loud not-assembled refusal).
    pub courier_resolve: Option<Arc<dyn quilltap_core::api::chat_media::CourierResolveDriver>>,
    /// The save-image bytes seam (P4.6ab, the unification wire; `None` → the
    /// `messageSaveImage` arm falls back to `NotConfiguredBytes`).
    pub save_image_bytes:
        Option<Arc<dyn quilltap_core::photos::save_image_to_album::FileBytesStore>>,
    /// The `imageProfileGenerate` runner (P4.6ai, the unification wire; `None` for
    /// canned test factories — the arm answers the loud not-assembled refusal).
    pub image_generation: Option<ErasedImageGeneration>,
    /// The custom-tool consult runner (P4.6bd; `None` for canned test factories —
    /// the composer/bench arms answer the loud not-assembled error).
    pub consult: Option<Arc<dyn ConsultRunner>>,
    /// The Brahma Console send driver (P4.9I1A — the orchestrator; `None` for
    /// canned test factories — the arm answers "not assembled").
    pub brahma_console_send: Option<Arc<dyn quilltap_core::api::brahma::BrahmaConsoleSendDriver>>,
    /// The recall-replay runner (P4.d13 — episodic recall §3; `None` for canned
    /// test factories — the arm answers the loud not-assembled error).
    pub recall_replay: Option<Arc<dyn quilltap_core::api::recall_replay::RecallReplayDriver>>,
    /// The in-chat announcement-preview runner (P4.9E2A, wired at the round's
    /// unification; `None` for canned test factories — the
    /// `ChatAnnouncementPreview` arm answers the loud not-assembled refusal).
    pub announcement_preview:
        Option<Arc<dyn quilltap_core::api::chat_post_office::AnnouncementPreviewDriver>>,
    /// The operator run-tool runner (P4.9E3A) — the same `BuiltInToolRunner` the
    /// carina / ask_carina / Brahma engines get, so a tool invoked from the Run
    /// Tool modal behaves exactly as it does mid-turn. `None` for canned test
    /// factories — the `ChatRunTool` arm then answers the loud not-assembled
    /// refusal AFTER v4's deny-list and chat arms.
    pub operator_tool_runner:
        Option<Arc<dyn quilltap_core::services::chat_run_tool::OperatorToolRunner>>,
    /// The manual title-regeneration driver (P4.9E3A). `None` for canned test
    /// factories — the arm answers the loud not-assembled refusal.
    /// ⚠ LIVE: one cheap-LLM call per Regenerate Title.
    pub regenerate_title:
        Option<Arc<dyn quilltap_core::services::chat_admin::RegenerateTitleDriver>>,
    /// The out-of-create llm_choose outfit runner (P4.9E3B). `None` for canned
    /// test factories — both call sites then fall back to the default outfit
    /// (v4's own failure shape). ⚠ LIVE: one cheap-LLM call per pick.
    pub outfit_llm_choose:
        Option<Arc<dyn quilltap_core::services::outfit_selections::OutfitLlmChooseRunner>>,
    /// The `attach-mount-file` vision-describe runner (P4.9E4A). `None` for
    /// canned test factories — the describe ladder then resolves to `''` and
    /// the attach still succeeds (v4's own any-failure arm).
    /// ⚠ LIVE: one vision-LLM call per attach of an undescribed image.
    pub image_describe: Option<Arc<dyn quilltap_core::api::chat_media::ImageDescribeDriver>>,
    /// The Serper web-search provider (P4.42) — the SAME `Option` held by the
    /// bundle's `ChatSpine`. The host places it in the `EngineAssembly` so the
    /// tools inventory's `web_search_configured` derives from the same source the
    /// runner uses. `None` for canned test factories + when `SERPER_API_KEY` is
    /// unset → `search_web` refuses (v4's unconfigured arm) and the inventory
    /// advertises it unavailable.
    pub web_search: Option<Arc<dyn quilltap_core::tools::web_search::WebSearchProvider>>,
    pub job_handlers: Vec<(String, Box<dyn JobHandler>)>,
}

/// Builds the chat-send + chat-create drivers + the model-dependent job
/// handlers for one assembly (a fresh `Db` per unlock). Production is
/// [`ProductionSpineFactory`]; the M2 smoke registers a canned-provider
/// factory. `pepper` + `data_dir` let the create driver open its own writable
/// partitions; `bus` is the shared Green-Room replay buffer.
pub trait SpineFactory: Send + Sync {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        terminal: Option<Arc<TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> SpineBundle;
}

/// The production factory over the [`ProviderIo`] drivers.
pub struct ProductionSpineFactory {
    pub io: ProviderIo,
    pub version: String,
    pub tz: String,
    /// The instance root; the disk file store lives at `<base>/files`
    /// (v4 `getFilesDir()`), the docs dir feeds `self_inventory`.
    pub base_dir: PathBuf,
    pub docs_dir: Option<PathBuf>,
    /// ONE per host process — the pricing caches are state.
    pub pricing: Arc<PricingFetcher<LivePricingFetch>>,
}

impl ProductionSpineFactory {
    pub fn new(base_dir: PathBuf, version: String, tz: String) -> Self {
        let io = ProviderIo::new(&version);
        let pricing = Arc::new(io.pricing_fetcher());
        Self {
            io,
            version,
            tz,
            base_dir,
            docs_dir: None,
            pricing,
        }
    }
}

impl SpineFactory for ProductionSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        terminal: Option<Arc<TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> SpineBundle {
        let wire = WireConfig::from_io(&self.io);
        // The provider Arcs are SHARED between the send + create drivers.
        let streaming = Arc::new(self.io.streaming_provider(DbProviderKeys(db.clone())));
        let completion = Arc::new(wire.completion(db));
        let embedding = Arc::new(ApiEmbeddingProvider::new(
            db.clone(),
            self.io.wire_transport(),
        ));
        let env = production_self_inventory_env(&self.version, self.docs_dir.as_deref(), db);
        let backend: Arc<dyn StorageBackend> =
            Arc::new(LocalStorageBackend::new(self.base_dir.join("files")));
        let file_bytes = Arc::new(ProductionFileBytes {
            db: db.clone(),
            backend,
            codec: Arc::new(HostImageCodec),
        });
        let scrollback = terminal.map(|m| Arc::new(PtyScrollbackSource::new(m, db.clone())));
        // P4.6bd: ONE consult runner per assembly, shared by the spine's tool
        // runners (the `run_custom` entrance) and the dispatch arms (composer +
        // workbench bench). It holds only the wire config — the provider and
        // the logging executor are rebuilt per consult.
        let consult: Arc<dyn ConsultRunner> = Arc::new(HostConsultRunner { wire: wire.clone() });
        // P4.9E2A (the unification wire): the announcement rewriter shares the
        // send/create drivers' provider Arcs; cloned here because `completion`
        // and `embedding` move into `chat_create` below.
        let announcement_completion = Arc::clone(&completion);
        let title_completion = Arc::clone(&completion);
        let outfit_completion = Arc::clone(&completion);
        // P4.9E4A: the attach-mount-file describe shares the same provider Arc.
        let describe_completion = Arc::clone(&completion);
        let announcement_embedding = Arc::clone(&embedding);
        // P4.42: build the Serper web-search provider iff SERPER_API_KEY is set —
        // the SINGLE source of truth. `serper_registered = false` (the plugin
        // registry is the standing deferral), so the env key is the only live
        // path; the `DbSearchApiKeys` lookup is wired inert for the plugin half.
        // This one `Option` feeds BOTH the runner (ChatSpine below) AND the tools
        // inventory bool (via the SpineBundle → EngineAssembly), so advertised and
        // executed can never disagree.
        let web_search: Option<Arc<dyn quilltap_core::tools::web_search::WebSearchProvider>> =
            std::env::var("SERPER_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .map(|env_key| {
                    Arc::new(self.io.web_search_provider(
                        DbSearchApiKeys(db.clone()),
                        false,
                        Some(env_key),
                    ))
                        as Arc<dyn quilltap_core::tools::web_search::WebSearchProvider>
                });
        let spine = Arc::new(ChatSpine {
            db: db.clone(),
            events: events.clone(),
            embedding: Arc::clone(&embedding),
            completion: Arc::clone(&completion),
            streaming: Arc::clone(&streaming),
            pricing: Arc::clone(&self.pricing),
            tz: self.tz.clone(),
            env,
            file_bytes,
            image_transcoder: Arc::new(HostImageCodec),
            scrollback,
            consult: Some(Arc::clone(&consult)),
            web_search: web_search.clone(),
        });
        let chat_create: Arc<dyn ChatCreateDriver> = Arc::new(ChatCreateSpine {
            db: db.clone(),
            events: events.clone(),
            bus: Arc::clone(bus),
            pepper: pepper.to_string(),
            data_dir: data_dir.to_path_buf(),
            embedding,
            completion,
            streaming,
            tz: self.tz.clone(),
        });

        let job_handlers: Vec<(String, Box<dyn JobHandler>)> = vec![
            (
                "AUTONOMOUS_ROOM_TURN".to_string(),
                Box::new(autonomous_turn_handler(Arc::clone(&spine))),
            ),
            (
                "MEMORY_HOUSEKEEPING".to_string(),
                Box::new(MemoryHousekeepingHandler),
            ),
            (
                "CHAT_DANGER_CLASSIFICATION".to_string(),
                Box::new(ChatDangerClassificationHandler { wire: wire.clone() }),
            ),
            (
                "CARINA_MEMORY_EXTRACTION".to_string(),
                Box::new(CarinaMemoryExtractionHandler {
                    wire: wire.clone(),
                    pricing: Arc::clone(&self.pricing),
                }),
            ),
            (
                "MEMORY_EXTRACTION".to_string(),
                Box::new(MemoryExtractionJobHandler {
                    wire: wire.clone(),
                    pricing: Arc::clone(&self.pricing),
                }),
            ),
            (
                "CONTEXT_SUMMARY".to_string(),
                Box::new(ContextSummaryJobHandler { wire: wire.clone() }),
            ),
            (
                "CHARACTER_AVATAR_GENERATION".to_string(),
                Box::new(AvatarJobHandler { wire: wire.clone() }),
            ),
            (
                "STORY_BACKGROUND_GENERATION".to_string(),
                Box::new(StoryBackgroundJobHandler { wire: wire.clone() }),
            ),
            (
                "TITLE_UPDATE".to_string(),
                Box::new(TitleUpdateJobHandler {
                    wire: wire.clone(),
                    pricing: Arc::clone(&self.pricing),
                }),
            ),
            // === P4.6BL ===
            (
                "EMBEDDING_GENERATE".to_string(),
                Box::new(EmbeddingGenerateJobHandler),
            ),
            // === end P4.6BL ===
            // === P4.24 ===
            (
                "LLM_LOG_CLEANUP".to_string(),
                Box::new(LlmLogCleanupJobHandler {
                    tz: self.tz.clone(),
                }),
            ),
            // === end P4.24 ===
        ];

        // The provider wire-actions driver (the P4.6 unification wire): the
        // live validate/models probes over the blocking wire transport + the
        // shared completion path.
        let provider_actions: Arc<dyn quilltap_core::api::ProviderActionsDriver> =
            Arc::new(quilltap_core::api::RealProviderActions {
                db: db.clone(),
                transport: self.io.sync_wire_transport(),
                completion: wire.completion(db),
                user_agent: self.io.user_agent().to_string(),
                base_url_env: self.io.base_url_env().map(str::to_string),
            });
        // The memory-embedding provider for the dispatch arms (P4.6s): the same
        // API-path provider the spine embeds with, resolved per call against the
        // default embedding profile (the BUILTIN TF-IDF path needs no wire IO).
        let memory_embedding = quilltap_core::model::embedding::ErasedEmbeddingProvider::new(
            ApiEmbeddingProvider::new(db.clone(), self.io.wire_transport()),
        );
        SpineBundle {
            chat_send: Arc::clone(&spine) as Arc<dyn ChatSendDriver>,
            swipe_generate: Some(Arc::clone(&spine) as _),
            // P4.6ab (the unification wire): the same spine backs the courier
            // resolve (completion + cheap executor for the settle's triggers);
            // the production byte store backs save-image.
            courier_resolve: Some(Arc::clone(&spine) as _),
            // P4.d13: the recall-replay runner — LIVE on the same spine (the
            // distill costs one cheap-LLM call per replay).
            recall_replay: Some(Arc::clone(&spine) as _),
            save_image_bytes: Some(Arc::clone(&spine.file_bytes) as _),
            // P4.6ai: the imageProfileGenerate un-refusal seam, wired LIVE over the
            // W4.7f Real*Providers (the runner rebuilds ImageGenDeps per request).
            image_generation: Some(ErasedImageGeneration::new(HostImageGenerationRunner {
                wire: wire.clone(),
            })),
            // P4.6bd: the consult seam, wired LIVE — the composer + workbench
            // bench entrances consult for real spend from here on.
            consult: Some(consult),
            // P4.9I1A: the Brahma Console orchestrator — the same spine backs it
            // (streaming + tool runner + pricing), on real spend.
            brahma_console_send: Some(Arc::clone(&spine) as _),
            // P4.9E2A (the unification wire): the in-character announcement
            // rewriter, LIVE. The lane shipped the runner and its seam but owned
            // neither `spine.rs` nor the host's assembly, so it left the seam
            // `None`; this is the one construction its record specified, over the
            // same completion + embedding providers and the same logging cheap
            // executor the rest of the spine uses. ⚠ Once wired the rewrite costs
            // real money — one cheap-LLM call per Generate in the Insert
            // Announcement dialog.
            announcement_preview: Some(Arc::new(HostAnnouncementPreviewRunner {
                db: db.clone(),
                completion: announcement_completion,
                embedding: announcement_embedding,
            })),
            // P4.9E3A: the operator run-tool seam, LIVE — the same built-in tool
            // runner the in-turn engines use, so a tool run from the Run Tool
            // modal behaves exactly as it does mid-turn (scrollback + consult
            // included).
            operator_tool_runner: Some(Arc::new(
                quilltap_core::services::chat_run_tool::ErasedToolRunner(spine.tool_runner()),
            )),
            // P4.9E3A: the manual title regeneration, LIVE — ⚠ one cheap-LLM
            // call per Regenerate Title.
            regenerate_title: Some(Arc::new(HostRegenerateTitleRunner {
                db: db.clone(),
                user_id: SINGLE_USER_ID.to_string(),
                completion: title_completion,
            })),
            // P4.9E3B: the out-of-create llm_choose pick, LIVE — ⚠ one
            // cheap-LLM call per pick (add-participant / merge).
            outfit_llm_choose: Some(Arc::new(HostOutfitLlmChooseRunner {
                db: db.clone(),
                user_id: SINGLE_USER_ID.to_string(),
                completion: outfit_completion,
            })),
            // P4.9E4A: the attach-mount-file vision describe, LIVE — ⚠ one
            // vision-LLM call per attach of an image that has neither a cached
            // blob description nor kept-image markdown.
            image_describe: Some(Arc::new(HostImageDescribeRunner {
                db: db.clone(),
                user_id: SINGLE_USER_ID.to_string(),
                completion: describe_completion,
            })),
            chat_create,
            provider_actions: Some(provider_actions),
            memory_embedding: Some(memory_embedding),
            // P4.42: the same provider the spine's runner holds — the host derives
            // the tools-inventory bool from `is_some()`.
            web_search,
            job_handlers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An invoker that never resolves — the hung-provider shape the timeout
    /// decorator exists for.
    struct PendingInvoker;
    impl LlmInvoker for PendingInvoker {
        fn invoke<'a>(
            &'a self,
            _prompt: &'a str,
            _options: LlmInvokeOptions,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>>
        {
            Box::pin(std::future::pending())
        }
    }

    /// An invoker that answers immediately (the pass-through side).
    struct EchoInvoker;
    impl LlmInvoker for EchoInvoker {
        fn invoke<'a>(
            &'a self,
            prompt: &'a str,
            _options: LlmInvokeOptions,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>>
        {
            let out = LlmInvokeResult::Answered {
                output: prompt.to_string(),
                provider: Some("echo".into()),
                model: Some("echo-1".into()),
            };
            Box::pin(async move { out })
        }
    }

    /// The elapsed branch maps to EXACTLY the ported reason string — the
    /// message has no v4 export to diff against (it stays pinned by
    /// `llm_consult`'s in-crate test), so the decorator carries its own pin.
    #[test]
    fn timeout_consult_elapsed_maps_to_the_ported_reason() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        // A short test ceiling; the REASON always names the production
        // CONSULT_TIMEOUT_MS, exactly as v4's withTimeout message does.
        let decorated = TimeoutConsult {
            inner: PendingInvoker,
            ceiling: std::time::Duration::from_millis(5),
        };
        let out = rt.block_on(decorated.invoke(
            "the author's prompt",
            LlmInvokeOptions {
                max_output_chars: 8000,
            },
        ));
        assert_eq!(
            out,
            LlmInvokeResult::Failed {
                reason: "the consult timed out after 60s".to_string()
            }
        );
    }

    #[test]
    fn timeout_consult_passes_an_answer_through() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let decorated = TimeoutConsult::new(EchoInvoker);
        assert_eq!(decorated.ceiling.as_millis() as i64, CONSULT_TIMEOUT_MS);
        let out = rt.block_on(decorated.invoke(
            "pass me through",
            LlmInvokeOptions {
                max_output_chars: 8000,
            },
        ));
        assert_eq!(
            out,
            LlmInvokeResult::Answered {
                output: "pass me through".to_string(),
                provider: Some("echo".into()),
                model: Some("echo-1".into()),
            }
        );
    }

    #[test]
    fn chat_settings_mapping_defaults() {
        // An empty settings row: every documented default materializes.
        let row = json!({});
        let s = orchestrator_chat_settings_from_value(&row);
        assert!(!s.cheap_llm_settings_present);
        assert!(s.compression_enabled);
        assert_eq!(s.project_context_reinject_interval, 5);
        assert!(s.auto_detect_rng);
        assert!(!s.answer_confirmation_global_enabled);
        assert_eq!(s.autonomous_destructive_policy, "opt_in_per_room");
        assert!(!s.agent_mode_default_enabled);
        assert_eq!(s.agent_mode_max_turns, 10);
        assert!(s.danger_settings.is_none());
        assert_eq!(s.cheap_llm_strategy, "PROVIDER_CHEAPEST");
        assert!(s.cheap_llm_fallback_to_local);
    }

    #[test]
    fn chat_settings_mapping_populated() {
        let row = json!({
            "cheapLLMSettings": {
                "strategy": "USER_DEFINED",
                "userDefinedProfileId": "p1",
                "fallbackToLocal": false
            },
            "contextCompressionSettings": {
                "enabled": false,
                "projectContextReinjectInterval": 9
            },
            "autoDetectRng": false,
            "answerConfirmationSettings": { "enabled": true },
            "agentModeSettings": { "maxTurns": 15, "defaultEnabled": true },
            "autonomousRoomSettings": { "destructiveToolPolicy": "always_allow" },
            "dangerousContentSettings": {
                "mode": "AUTO_ROUTE",
                "threshold": 0.7,
                "scanTextChat": true,
                "scanImagePrompts": true,
                "scanImageGeneration": false,
                "displayMode": "SHOW",
                "showWarningBadges": true
            }
        });
        let s = orchestrator_chat_settings_from_value(&row);
        assert!(s.cheap_llm_settings_present);
        assert!(!s.compression_enabled);
        assert_eq!(s.project_context_reinject_interval, 9);
        assert!(!s.auto_detect_rng);
        assert!(s.answer_confirmation_global_enabled);
        assert_eq!(s.autonomous_destructive_policy, "always_allow");
        assert!(s.agent_mode_default_enabled);
        assert_eq!(s.agent_mode_max_turns, 15);
        assert_eq!(s.danger_settings.as_ref().unwrap().mode, "AUTO_ROUTE");
        assert_eq!(s.cheap_llm_strategy, "USER_DEFINED");
        assert_eq!(s.cheap_llm_user_defined_profile_id.as_deref(), Some("p1"));
        assert!(!s.cheap_llm_fallback_to_local);
    }

    #[test]
    fn timestamp_config_defaults_materialize() {
        assert!(timestamp_config_from_value(None).is_none());
        assert!(timestamp_config_from_value(Some(&Value::Null)).is_none());
        let cfg = timestamp_config_from_value(Some(&json!({"mode": "EVERY_N_MINUTES"}))).unwrap();
        assert_eq!(cfg.mode, TimestampMode::EveryNMinutes);
        assert_eq!(cfg.format, TimestampFormat::Friendly);
        assert!(cfg.auto_prepend);
        assert_eq!(cfg.interval_minutes, 15);
        assert!(!cfg.use_fictional_time);
    }

    #[test]
    fn random01_in_unit_interval() {
        for _ in 0..64 {
            let r = os_random01();
            assert!((0.0..1.0).contains(&r), "out of range: {r}");
        }
    }

    /// The JS `getTimezoneOffset()` sign convention: positive = WEST of UTC.
    /// Pinned at absolute instants so the assertions hold on any host zone.
    /// (jiff's raw `to_offset` is east-positive; the flip is the point.)
    #[test]
    fn local_offset_minutes_is_js_west_positive() {
        // 2026-07-28T12:00:00Z — Chicago on CDT (UTC-5): JS offset +300.
        let summer_noon_utc = 1_785_240_000_000;
        assert_eq!(
            js_local_offset_minutes("America/Chicago", summer_noon_utc),
            300
        );
        // Same instant in Tokyo (UTC+9, no DST): JS offset -540.
        assert_eq!(js_local_offset_minutes("Asia/Tokyo", summer_noon_utc), -540);
        // 2026-01-15T12:00:00Z — Chicago on CST (UTC-6): JS offset +360.
        let winter_noon_utc = 1_768_478_400_000;
        assert_eq!(
            js_local_offset_minutes("America/Chicago", winter_noon_utc),
            360
        );
        // UTC and an unresolvable zone (falls back to UTC) are both 0.
        assert_eq!(js_local_offset_minutes("UTC", summer_noon_utc), 0);
        assert_eq!(js_local_offset_minutes("Not/AZone", summer_noon_utc), 0);
    }
}
