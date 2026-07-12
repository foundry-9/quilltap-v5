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
use quilltap_core::provider_manifest::{Capability, Registry};
use quilltap_core::services::answer_confirmation::AnswerConfirmationOutcome;
use quilltap_core::services::api_key_service;
use quilltap_core::services::brahma_console::RealBrahmaConsole;
use quilltap_core::services::build_context::RealBuildContextSeams;
use quilltap_core::services::carina_memory_extraction::{
    handle_carina_memory_extraction, CarinaCostEstimator, CarinaMemoryExtractionPayload,
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
use quilltap_core::services::creation_progress::{CreationProgressBus, CreationProgressEmitter};
use quilltap_core::services::dangerous_content::gatekeeper_job::{
    handle_chat_danger_classification, ChatDangerClassificationJob, RealDangerAnnouncer,
};
use quilltap_core::services::dangerous_content::moderation_wire::RealModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::{
    DangerContentRouter, DbApiKeys,
};
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::file_storage::{
    ProductionFileBytes, RealProjectImageUpload, StorageBackend,
};
use quilltap_core::services::housekeeping::{run_housekeeping, HousekeepingOptions};
use quilltap_core::services::housekeeping_outcome_cache::record_housekeeping_outcome;
use quilltap_core::services::job_runner::{JobFuture, JobHandler, JobOutcome};
use quilltap_core::services::llm_logging::LogContext;
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
use quilltap_core::services::turn_orchestrator::ChainConfig;
use quilltap_core::tools::ask_carina::{ErasedAskCarina, TypedAskCarina};
use quilltap_core::tools::executor::BuiltInToolRunner;
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
        async move {
            quilltap_core::model::completion_provider::execute_completion(
                &self.transport,
                provider,
                base_url,
                &api_key,
                params,
                &self.policy,
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
        }
    }

    /// A tool runner for the carina / ask_carina / Brahma engines (each engine
    /// owns its own — the type-level cycle note from W4.11a).
    fn tool_runner(&self) -> BuiltInToolRunner {
        let mut runner = BuiltInToolRunner::new(self.db.clone(), self.env.clone());
        if let Some(sb) = &self.scrollback {
            runner = runner.with_scrollback(Arc::clone(sb) as _);
        }
        runner
    }

    /// The local UTC offset (minutes) for the configured zone at `now_ms`.
    fn local_offset_minutes(&self, now_ms: i64) -> i64 {
        use jiff::Timestamp;
        let tz = jiff::tz::TimeZone::get(&self.tz).unwrap_or(jiff::tz::TimeZone::UTC);
        Timestamp::from_millisecond(now_ms)
            .map(|ts| tz.to_offset(ts).seconds() as i64 / 60)
            .unwrap_or(0)
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
            },
            RegenError::Db(_) => CoreError {
                kind: ErrorKind::Internal,
                message: e.to_string(),
                pepper_state: None,
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
            provider_supports_web_search,
        };

        let initial = match orchestrator::process_message(&mut deps, &input).await {
            Ok(r) => r,
            Err(e) => {
                // v4 handleStreamError: the transport-shell error frame.
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
            let offset = {
                use jiff::Timestamp;
                let tz = jiff::tz::TimeZone::get(&tz_name).unwrap_or(jiff::tz::TimeZone::UTC);
                Timestamp::from_millisecond(now)
                    .map(|ts| tz.to_offset(ts).seconds() as i64 / 60)
                    .unwrap_or(0)
            };
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
                provider_supports_web_search,
            }
        };
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
            // response stream closes after the frame).
            let _ = self.events.send(Event::chat_error(
                &chat_id,
                ChatErrorPayload {
                    error: "Failed to generate response".to_string(),
                    error_type: "Error".to_string(),
                    details: e.to_string(),
                },
            ));
        }

        Ok(dto)
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
                    }
                })?;
            w.connection().busy_timeout(busy).map_err(|e| CoreError {
                kind: ErrorKind::Internal,
                message: format!("busy_timeout {name}: {e}"),
                pepper_state: None,
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
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "chat create thread panicked".to_string(),
                    pepper_state: None,
                })
            })
        })
    }
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
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "chat send thread panicked".to_string(),
                    pepper_state: None,
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
                    }),
                };
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: "swipe generation thread panicked".to_string(),
                    pepper_state: None,
                })
            })
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

/// The pricing-backed [`CarinaCostEstimator`] (the same cascade the finalizer's
/// cost tracker uses).
pub struct PricingCarinaCost<PF: PricingFetch + Send + Sync> {
    pub fetcher: Arc<PricingFetcher<PF>>,
    pub db: Db,
}

impl<PF: PricingFetch + Send + Sync> CarinaCostEstimator for PricingCarinaCost<PF> {
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
            let cost = PricingCarinaCost {
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
                None,
            )
            .await
            {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e.to_string()),
            }
        })
    }
}

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
                "CHARACTER_AVATAR_GENERATION".to_string(),
                Box::new(AvatarJobHandler { wire: wire.clone() }),
            ),
            (
                "STORY_BACKGROUND_GENERATION".to_string(),
                Box::new(StoryBackgroundJobHandler { wire: wire.clone() }),
            ),
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
            swipe_generate: Some(spine),
            chat_create,
            provider_actions: Some(provider_actions),
            memory_embedding: Some(memory_embedding),
            job_handlers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
