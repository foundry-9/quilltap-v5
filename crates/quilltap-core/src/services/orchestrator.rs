//! The chat-message orchestrator (Phase-3 Unit-3 wave 3, final unit) — v4
//! `orchestrator.service.ts` (`processMessage`, lines ~247–1516) + the
//! model-calling chain driver `executeTurnChain`
//! (`turn-orchestrator.service.ts:284`), the spine that sequences the ported
//! wave-1..3 services into one full user-message → assistant-response cycle.
//!
//! ## The composition map
//!
//! `processMessage` is v4's send path. It routes through the already-ported
//! services, in v4's order:
//!
//! | v4 phase | ported service this routes through |
//! |---|---|
//! | resolve responding participant | [`super::participant_resolver::resolve_responding_participant`] |
//! | `turnStart` (chainDepth 0) + status | [`super::chat_events`] frames |
//! | resolve user identity | [`super::user_identity_resolver::resolve_user_identity`] |
//! | chat settings read | `chat_settings::find_..._by_user_id` (via a seam-provided value) |
//! | save the user message + link files | [`crate::db::chats_messages`] via [`Db::write`] |
//! | build context | [`super::build_context::build_context`] (with its [`BuildContextSeams`]) |
//! | primary stream (+ recovery/failover) | [`super::primary_stream::run_primary_stream`] |
//! | empty-response recovery | [`super::provider_failover::attempt_empty_response_recovery`] |
//! | finalize (clean/write/next-speaker/done/triggers) | [`super::message_finalizer::finalize_message_response`] |
//! | context-summary check (finalizer's deferral, closed HERE) | [`super::context_summary::check_and_generate_summary_if_needed`] |
//! | chain driver | [`execute_turn_chain`] over [`super::turn_orchestrator::should_chain_next`] |
//!
//! ## The injected seams (v4 subsystems not yet ported)
//!
//! Every subsystem `processMessage` touches that is NOT ported is an
//! [`OrchestratorSeams`] method whose v4 GATE condition is reproduced faithfully
//! (the carina-runner / build-context precedent — the gate lives here; the
//! subsystem body is a seam). The corpus keeps each inactive where v4 would
//! no-op, and the differential BANKS both the on- and off- gate paths through the
//! ordered event trace + DB dumps. The seamed subsystems and their gates:
//!
//! * **Attachment processing** (`loadAndProcessFiles`): gated on `fileIds`
//!   non-empty. The corpus keeps `fileIds` empty → no-op (no processing status,
//!   no attachment prefix, no file links). A `fileIds`-carrying turn is a
//!   documented deferral (the attachment subsystem is wave-4).
//! * **RNG auto-detect on the user message** (W4.1a): gated on
//!   `autoDetectRng ?? true`, `!continueMode`, and content. This seam is now
//!   CLOSED — the ported [`crate::rng_patterns`] detector +
//!   [`crate::tools::rng`] executor run inline, writing a TOOL message per
//!   detected pattern and appending it to the context (the byte source injected
//!   via [`OrchestratorDeps::rng_bytes`]).
//! * **Pending tool results / carina markup on the user message**: gated on
//!   `!continueMode && content`. The corpus keeps the user content free of carina
//!   markup and passes no pending tool results, so each detector returns "none"
//!   and writes nothing (the [`OrchestratorSeams`] `user_message_carina` method).
//!   ⚠ This seam has TWO call sites in v4, both deferred: the MAIN send path
//!   (here) AND the Bug-50 fair-rotation pause path
//!   ([`maybe_pause_for_user_seat_turn`], which in v4 fires the same
//!   `runCarinaMarkupQuery` after persisting the paused user message). The pause
//!   path omits it for the same reason and its corpus cases use non-`@Name`
//!   content — wiring it on only one path would be an asymmetric divergence.
//! * **Agent mode** (W4.4, real): the cascade resolver runs on the spine
//!   (`resolve_agent_mode_setting` over chat / project / character / global
//!   settings). The corpus's `agent_mode_on` chat opts in at the Chat level, so
//!   the agent-turn-count reset, the `submit_final_response` slate addition, and
//!   the agent-mode instruction injection all fire and are banked; every other
//!   chat resolves off.
//! * **Danger / courier**: the corpus keeps danger mode `DETECT_ONLY` (no
//!   reroute) and the transport non-courier — so the danger reroute and the
//!   courier short-circuit never fire. Their gates are reproduced (`is_courier`)
//!   and banked off.
//! * **Prospero cadence re-injection**: gated on
//!   `reinjectInterval > 0 && messageCount > 0 && messageCount % interval === 0`.
//!   The corpus keeps `messageCount` off a cadence boundary, so no whisper is
//!   posted. The gate is reproduced; the whisper post is a
//!   [`OrchestratorSeams`] no-op.
//! * **Tool build + native/text tool loops + pseudo-tool modes**: the corpus
//!   keeps `actualTools` empty (native mode, no tools), so the tool loops no-op
//!   (v4's loops early-return with no markers) and `toolMessages` stays empty. A
//!   non-empty tool slate is wave-4.
//! * **`request_full_context` bypass / `forceToolsOnNextMessage`**: their flag
//!   reads are reproduced; the corpus keeps the flags clear.
//!
//! ## `executeTurnChain`
//!
//! The chain *driver* composes the already-ported decision core
//! ([`super::turn_orchestrator::should_chain_next`] +
//! [`super::turn_orchestrator::persist_turn_participant_id`]) with a re-entry into
//! [`process_message`] per turn (v4's `processChainedMessage`), emitting the
//! `turnStart` / `turnComplete` / `chainComplete` frames. The depth-20 /
//! 300 000 ms wall-clock guards live in `should_chain_next` (clock injected);
//! the driver holds the loop, the empty-response stop, and the error stop
//! (which pauses the chat + persists a null next speaker).
//!
//! ## Injected impurities
//!
//! * **Wall clock** — the chain-start time + each `now_ms` for
//!   `should_chain_next`'s time guard, and the assistant-message `createdAt`. The
//!   caller injects them (the differential freezes the clock on both sides).
//! * **`Math.random()`** — threaded into participant resolution + the chain's
//!   `should_chain_next` selection. The caller passes `random01`; the corpus is
//!   shaped so the diffed pick is deterministic.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::cheap_llm::{
    get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile, CheapLlmSelection,
};
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, connection_profiles, DbError};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::{StreamMessage, StreamParams, StreamingCompletionProvider};
use crate::rng_patterns::detect_and_convert_rng_patterns;
use crate::services::build_context::{self, BuildContextInput, BuildContextSeams, BuiltContext};
use crate::services::carina_runner::{PostProsperoCarinaError, RunCarinaQuery};
use crate::services::chat_events::{
    ChainCompletePayload, ChatEvent, DonePayload, EventSink, StatusPayload, TurnCompletePayload,
    TurnStartPayload,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::llm_logging::LogContext;
use crate::services::message_context;
use crate::services::message_finalizer::{
    self, AnswerConfirmationRunner, AsyncCompressionTrigger, CostTracker, FinalizeOptions,
    FinalizerCharacter, FinalizerChat, FinalizerChatSettings, FinalizerCompression,
    FinalizerParticipant, FinalizerProfile, FinalizerStreaming, ParticipantCharacter,
    ProcessMessageResult,
};
use crate::services::native_tool_loop;
use crate::services::pricing_fetcher::{
    PricingContext, PricingFetch, PricingFetcher, PricingProfile,
};
use crate::services::primary_stream::{
    self, EffectiveProfile, PreservePartialOnError, ReasoningSegment, RunPrimaryStreamOptions,
    StreamingState,
};
use crate::services::provider_failover::{
    self, AttemptEmptyResponseRecoveryOptions, DangerSettings, DangerousContentRouter,
};
use crate::services::pseudo_tool::{
    build_native_tool_system_instructions, build_simple_json_system_instructions,
    build_text_block_system_instructions, check_resolved_tool_mode,
    determine_text_block_tool_options, SIMPLE_JSON_STOP,
};
use crate::services::text_tool_loop::{
    self, RunTextToolPassOptions, SimpleJsonStrategy, TextBlockStrategy, TextToolStrategy,
};
use crate::services::tool_build::{self, BuildToolsInput};
use crate::services::tool_call_threading::ThreadedMessage;
use crate::services::tool_execution::{
    create_tool_context, save_tool_messages, GeneratedImage, LoadedInterCharacterMemory,
    LoadedMemoriesContext, LoadedSemanticMemory, SaveToolMessagesResult, ToolExecutionContext,
    ToolMessage, ToolWhisperContext,
};
use crate::services::turn_orchestrator::{
    self, ChainConfig, ChainDecision, ChainGuards, ChainReason,
};
use crate::tools::ask_carina::ErasedAskCarina;
use crate::tools::executor::BuiltInToolRunner;
use crate::tools::pseudo_tool_support::{ResolvedToolMode, ToolMode};
use crate::tools::rng::{
    execute_rng_tool, format_rng_results, RandomBytes, RngToolContext, RngType,
};
use crate::tools::self_inventory::{ClientShell, SelfInventoryEnv};

/// The TOOL-message `content` JSON for an auto-detected RNG execution (v4's
/// `JSON.stringify({ tool, initiatedBy, success, result, prompt, arguments })`).
/// A typed struct in v4's field order so the stored bytes are byte-identical.
#[derive(serde::Serialize)]
struct RngToolMessageContent<'a> {
    tool: &'static str,
    #[serde(rename = "initiatedBy")]
    initiated_by: &'static str,
    success: bool,
    result: &'a str,
    prompt: &'a str,
    arguments: RngArguments,
}

/// The `arguments` object of an RNG TOOL message (v4
/// `{ type, rolls, modifier: pattern.modifier ?? 0 }`).
///
/// `modifier` is NOT optional here, unlike on the `RngToolCall` it comes from:
/// v4's `?? 0` runs at this site, so a coin-flip pattern — which carries no
/// modifier at all — still persists `modifier: 0` in its TOOL row.
#[derive(serde::Serialize)]
struct RngArguments {
    #[serde(rename = "type")]
    type_: RngType,
    rolls: u32,
    modifier: i64,
}

/// The `self_inventory` host environment the tool runner carries. In production
/// this is populated by the host (Quilltap version, runtime mode, client shell,
/// mount-index health, the registry model-info rows). W4.1g wires the tool SLATE;
/// the host-env wiring is the standing [`SelfInventoryEnv`] seam (a tracked
/// deferral). The corpus's native-tool-call cases invoke handlers that need none
/// of this (wardrobe / read_conversation / doc reads), so a placeholder suffices.
fn host_self_inventory_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Unknown,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

// ===========================================================================
// The injected seams (unported subsystems `processMessage` touches).
// ===========================================================================

/// The unported subsystems `processMessage` reaches. Each has a default
/// (no-op / "nothing to do") matching the tier-3 oracle mocks; the GATE that
/// decides whether the subsystem fires is reproduced in [`process_message`]
/// itself, per the carina-runner STOP-rule precedent.
#[allow(unused_variables)]
pub trait OrchestratorSeams {
    /// v4's chat-settings read (`repos.chatSettings.findByUserId(userId)`). The
    /// orchestrator threads the returned settings into the compression gate, the
    /// finalizer trigger gate, and the summary-check. `None` = no settings row.
    fn chat_settings(&self, user_id: &str) -> Option<OrchestratorChatSettings> {
        None
    }
}

/// A no-op seam bundle (nothing available). Used by self-tests; the differential
/// injects a recording bundle matching the oracle mocks.
pub struct NoopOrchestratorSeams;
impl OrchestratorSeams for NoopOrchestratorSeams {}

/// The chat-settings the orchestrator reads (v4 `chatSettings` subset the send
/// path consumes: the compression settings, the cheap-LLM presence, autoDetectRng,
/// the answer-confirmation global toggle, and the reinject interval).
#[derive(Clone, Debug, Default)]
pub struct OrchestratorChatSettings {
    /// Whether `cheapLLMSettings` is set — gates compression + memory extraction
    /// + the summary check.
    pub cheap_llm_settings_present: bool,
    /// `contextCompressionSettings.enabled` (default true when the block is
    /// absent).
    pub compression_enabled: bool,
    /// `projectContextReinjectInterval ?? 5`.
    pub project_context_reinject_interval: i64,
    /// `autoDetectRng ?? true`.
    pub auto_detect_rng: bool,
    /// `customTools ?? true` — the chat-level Pascal-custom-tools toggle.
    /// Withholding the `run_custom` roster context when this is false is HOW the
    /// setting turns the feature off (v4 `orchestrator.service.ts:888`).
    pub custom_tools: bool,
    /// `answerConfirmationSettings.enabled === true`.
    pub answer_confirmation_global_enabled: bool,
    /// `autonomousRoomSettings.destructiveToolPolicy ?? 'opt_in_per_room'` — the
    /// CEILING for the autonomous-room destructive-tool filter. Only consulted
    /// when `chat.chatType === 'autonomous'` (the corpus keeps chats
    /// non-autonomous, so this default is never read there).
    pub autonomous_destructive_policy: String,
    /// `agentModeSettings.defaultEnabled` — the GLOBAL level of the agent-mode
    /// cascade (v4 `resolveAgentModeSetting`). Zod default `false`. When the
    /// settings row is absent the orchestrator falls back to
    /// [`agent_mode::DEFAULT_AGENT_MODE_SETTINGS`] instead of this field.
    pub agent_mode_default_enabled: bool,
    /// `agentModeSettings.maxTurns` — the agent-mode iteration cap (Zod
    /// `.min(1).max(25).default(10)`). Not overridden by any cascade level.
    pub agent_mode_max_turns: i64,
    /// `dangerousContentSettings` — the GLOBAL danger settings sub-object (v4
    /// `chatSettings?.dangerousContentSettings`). The orchestrator resolves the
    /// EFFECTIVE settings from this + the chat's `conciergeOverride` / `chatType`
    /// via [`resolve_dangerous_content_settings`] (W4.2u). `None` = the settings
    /// row / sub-object is absent (the resolver then falls back to its default,
    /// mode `OFF`).
    pub danger_settings: Option<crate::db::chat_settings::DangerousContentSettings>,
    /// The `cheapLLMSettings.strategy` (v4 `chatSettings?.cheapLLMSettings ||
    /// DEFAULT_CHEAP_LLM_CONFIG`) — feeds the cheap-LLM selection the spine
    /// resolves for the compression / danger / proactive-recall paths (Round-3
    /// Group 8). Empty string → the DEFAULT config's `"PROVIDER_CHEAPEST"`.
    pub cheap_llm_strategy: String,
    /// `cheapLLMSettings.userDefinedProfileId` (`null` → `None`).
    pub cheap_llm_user_defined_profile_id: Option<String>,
    /// `cheapLLMSettings.defaultCheapProfileId` (`null` → `None`).
    pub cheap_llm_default_cheap_profile_id: Option<String>,
    /// `cheapLLMSettings.fallbackToLocal`. The v4 default is `true`, but the
    /// `Default` derive gives `false`; [`defaults_present`] supplies the real
    /// default when a settings row is synthesized.
    pub cheap_llm_fallback_to_local: bool,
}

impl OrchestratorChatSettings {
    /// v4's default when no `contextCompressionSettings` block is present
    /// (`{ enabled: true, …, projectContextReinjectInterval: 5 }`).
    pub fn defaults_present() -> Self {
        OrchestratorChatSettings {
            cheap_llm_settings_present: true,
            compression_enabled: true,
            project_context_reinject_interval: 5,
            auto_detect_rng: true,
            custom_tools: true,
            answer_confirmation_global_enabled: false,
            autonomous_destructive_policy: "opt_in_per_room".to_string(),
            agent_mode_default_enabled: false,
            agent_mode_max_turns: 10,
            danger_settings: None,
            // v4 `DEFAULT_CHEAP_LLM_CONFIG` = { PROVIDER_CHEAPEST, fallbackToLocal: true }.
            cheap_llm_strategy: "PROVIDER_CHEAPEST".to_string(),
            cheap_llm_user_defined_profile_id: None,
            cheap_llm_default_cheap_profile_id: None,
            cheap_llm_fallback_to_local: true,
        }
    }
}

// ===========================================================================
// Options
// ===========================================================================

/// v4 `pendingToolResultSchema` element — a user-initiated tool result the send
/// route pre-inserts as a TOOL message.
#[derive(Clone, Debug)]
pub struct PendingToolResult {
    pub tool: String,
    pub success: bool,
    pub result: String,
    pub prompt: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
}

/// v4 `SendMessageOptions` (the subset the ported spine consumes). The unported
/// flags (`suppressAutomaticImages`, `browserUserAgent`, …) are omitted.
#[derive(Clone, Debug, Default)]
pub struct SendMessageOptions {
    /// `continueMode === true` (a nudge — no new user message written).
    pub continue_mode: bool,
    /// The user's typed content (empty in continue mode).
    pub content: String,
    /// `respondingParticipantId` — an explicit responder override.
    pub responding_participant_id: Option<String>,
    /// `targetParticipantIds` — whisper targets (the first is the responder when
    /// no explicit responder is set).
    pub target_participant_ids: Option<Vec<String>>,
    /// `speakingAsParticipantId` — the user-controlled participant the human is
    /// "Speaking As".
    pub speaking_as_participant_id: Option<String>,
    /// `fileIds` — attachment ids (the corpus keeps this empty).
    pub file_ids: Vec<String>,
    /// `pendingToolResults` — user-initiated tool results pre-inserted as TOOL
    /// messages before the user message (v4 orchestrator.service.ts:601–624). Only
    /// applied on a normal (non-continue) send.
    pub pending_tool_results: Vec<PendingToolResult>,
    /// Autonomous-room flag (bypasses the all-LLM pause in the chain).
    pub never_pause_for_user: bool,
    /// Autonomous-room flag (chain enqueues the next turn as a separate job).
    pub single_turn: bool,
    /// Autonomous-room per-turn context cap (tokens) — clamps the model-derived
    /// budget. `None` = uncapped. (Threaded into `buildContext`'s budget cap.)
    pub autonomous_context_cap: Option<i64>,
    /// Nudge flag (v4 `options.nudge`): the human explicitly summoned this
    /// specific character to speak (Nudge button / queue). Distinct from an
    /// algorithm-picked chained turn. When `Some(true)`, the "nothing to add"
    /// skip option is withheld — you don't offer a pass to a voice the operator
    /// just called on.
    pub nudge: Option<bool>,
    /// How a chained turn's speaker was chosen (v4 `chainSelectionReason`):
    /// `queue` (popped from the manual turn queue — treated as summoned) or
    /// `algorithm` (weighted rotation — the skip option is offered). Threaded
    /// from the turn orchestrator into chained `process_message` calls; `None`
    /// on the initial (non-chained) turn.
    pub chain_selection_reason: Option<turn_orchestrator::ChainSelectionReason>,
}

/// The wall-clock + RNG values injected into one `process_message` call (v4's
/// `Date.now()` / `crypto.randomUUID()` seed points + `Math.random()`).
#[derive(Clone, Copy, Debug)]
pub struct ProcessClock {
    /// Milliseconds since epoch (v4 `Date.now()` — the timestamp base for
    /// buildContext + the message writes).
    pub now_ms: i64,
    /// The local UTC offset (minutes) buildContext's timestamp math reads.
    pub local_offset_minutes: i64,
    /// `Math.random()`'s value for the participant / next-speaker selection.
    pub random01: f64,
}

/// Everything a `process_message` call needs beyond the injected providers /
/// sinks / seams. The heavy pre-resolved buildContext inputs (roleplay template,
/// user character, participant characters) are computed inside from the DB reads;
/// the caller supplies the model context limit + the timestamp config the profile
/// carries.
pub struct ProcessMessageInput {
    pub chat_id: String,
    pub user_id: String,
    pub options: SendMessageOptions,
    pub clock: ProcessClock,
    /// The model's context-window limit (v4 `getModelContextLimit` on the
    /// effective profile — resolved above the seam from the registry).
    pub model_context_limit: i64,
    /// The optional timestamp config (v4 `chatSettings.defaultTimestampConfig`).
    pub timestamp_config: Option<crate::chat_timestamp::TimestampConfig>,
    /// The IANA timezone buildContext resolves (v4 `resolveTimezone`); `None` =
    /// system default (`UTC` in the corpus).
    pub timezone: Option<String>,
    /// The SERVER-LOCAL IANA zone (the v5 seam for v4's ambient process zone) —
    /// the memory distill's TODAY line and its deterministic day-reference scan
    /// resolve their calendar in it. ⚠ A DIFFERENT value from
    /// [`Self::timezone`]: that is the story/timestamp zone (`resolveTimezone`,
    /// a per-chat setting); this is where the server stands. `None` behaves like
    /// a UTC server (what the corpora pin).
    pub server_tz: Option<String>,
    /// Injected `provider.supportsWebSearch` — the provider-capability flag
    /// resolved above the seam. `useNativeWebSearch` ANDs it with the profile.
    pub provider_supports_web_search: bool,
    /// The [`LogContext`] threaded to the primary stream's terminal
    /// `CHAT_MESSAGE` `llm_logs` row (U4.4, spec decision #4 — the explicit
    /// replacement for v4's ambient `runWithAutonomousRunId`). Every
    /// request-path caller passes [`LogContext::none()`] (the `Default`); the
    /// autonomous turn handler passes the run's id so per-run token accounting
    /// can sum the rows by `autonomousRunId`.
    pub log_context: LogContext,
}

// ===========================================================================
// processMessage
// ===========================================================================

/// The bundle of injected model boundaries + sinks + seams threaded through the
/// whole spine (grouped so the re-entrant chain driver can pass one reference).
pub struct OrchestratorDeps<
    'a,
    EMB,
    CMP,
    STR,
    SNK,
    BCS,
    ORC,
    RTR,
    CONF,
    ACOMP,
    COST,
    CARQ,
    PROS,
    PF,
> where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider + Sync,
    STR: StreamingCompletionProvider,
    SNK: EventSink + Sync,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    PF: PricingFetch + Send + Sync,
{
    pub db: &'a Db,
    pub embedding: &'a EMB,
    pub completion: &'a CMP,
    pub streaming: &'a STR,
    pub executor: &'a CheapLlmTaskExecutor,
    /// The Carina engine the `ask_carina` TOOL dispatches to (v4's
    /// `executeToolCallWithContext` → the ask_carina handler → `runCarinaQuery`).
    /// The spine wires it into the per-turn [`BuiltInToolRunner`]; default
    /// [`ErasedAskCarina::not_available`] reproduces the prior loud fallback, so
    /// existing constructions stay inert. (The finalizer's `@Name:` markup runner
    /// is a SEPARATE seam — [`OrchestratorDeps::carina_query`].)
    pub ask_carina: &'a ErasedAskCarina,
    pub sink: &'a SNK,
    /// The pricing-cache fetcher backing `checkModelSupportsTools` (v4
    /// `getPricingCache` → the OpenRouter path; every other provider answers from
    /// the static fallback table). The fetch itself stays a seam — production wires
    /// the real HTTP fetch, the differential a never-called one.
    pub pricing: &'a PricingFetcher<PF>,
    pub build_context_seams: &'a BCS,
    pub orchestrator_seams: &'a ORC,
    /// The host byte store (v4 `fileStorageManager`) — the W4.4b attachment
    /// loader's legacy-`files` byte source.
    pub file_bytes: &'a dyn crate::services::chat_files::FileBytesStore,
    /// The image codec (Sharp) seam — the W4.4b attachment resize decision.
    pub image_transcoder: &'a dyn crate::files::image_processing::ImageTranscoder,
    pub danger_router: &'a RTR,
    /// The finalizer's answer-confirmation runner seam.
    pub confirmation: &'a mut CONF,
    /// The finalizer's async-compression trigger seam.
    pub compression: &'a mut ACOMP,
    /// The finalizer's cost-estimation seam.
    pub cost: &'a mut COST,
    /// The finalizer's + orchestrator's carina query seam.
    pub carina_query: &'a mut CARQ,
    /// The finalizer's Prospero-carina-error post seam.
    pub prospero: &'a mut PROS,
    /// The RNG byte source (v4's `crypto.randomBytes`) — shared by the orchestrator's
    /// user-message auto-detect AND the finalizer's assistant-response auto-detect
    /// (both draw the same committed stream sequentially). Production draws from the
    /// OS CSPRNG ([`crate::tools::rng::OsRandomBytes`]); the differential injects a
    /// fixed committed stream.
    pub rng_bytes: &'a mut dyn RandomBytes,
    /// The web-search boundary the per-turn `search_web` tool runs through (P4.42).
    /// The spine wires the host's `RealWebSearchProvider` here; `None` (the
    /// default for existing constructions — the enclave, canned differentials)
    /// keeps the not-configured boundary, so those turns refuse `search_web`
    /// exactly as before.
    pub web_search: Option<std::sync::Arc<dyn crate::tools::web_search::WebSearchProvider>>,
}

/// Build the [`PricingContext`] `checkModelSupportsTools` consults on the
/// OPENROUTER path (v4 `refreshPricingCache` reads the user's connection profiles
/// and resolves each profile's key via `findApiKeyByIdAndUserId`). The `api_keys`
/// map carries `apiKeyId → key_value` for every profile that names one (P4.1a —
/// the live [`PricingFetch`] can now authenticate the OpenRouter SDK path when
/// the host wires a real HTTP fetch; under the differential's never-called
/// pricing seam the populated map is inert). When the fetch returns nothing, an
/// OPENROUTER model falls through to v4's "default to native tools" — matching
/// v4's own missing-cache fallback. A read failure yields an empty context /
/// skips the key (fail-open to the static fallback table, v4's `catch`).
pub fn build_pricing_context(db: &Db, user_id: &str) -> PricingContext {
    let uid = user_id.to_string();
    let profiles = db
        .read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
        .unwrap_or_default();
    let pricing_profiles: Vec<PricingProfile> = profiles
        .iter()
        .map(|p| PricingProfile {
            provider: p
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            api_key_id: p
                .get("apiKeyId")
                .and_then(Value::as_str)
                .map(str::to_string),
            base_url: p.get("baseUrl").and_then(Value::as_str).map(str::to_string),
        })
        .collect();

    // v4 `getApiKeyForProvider`: `findApiKeyByIdAndUserId(profile.apiKeyId,
    // userId)` — resolved here for every key-naming profile so the fetcher's
    // provider-scan finds them; a missing/foreign-owned row resolves to no
    // entry (v4's `null`).
    let mut api_keys = HashMap::new();
    for key_id in pricing_profiles.iter().filter_map(|p| p.api_key_id.clone()) {
        if api_keys.contains_key(&key_id) {
            continue;
        }
        let uid = user_id.to_string();
        let kid = key_id.clone();
        if let Ok(Some(key)) =
            db.read_main(move |conn| crate::db::api_keys::find_by_id_and_user_id(conn, &kid, &uid))
        {
            api_keys.insert(key_id, key.key_value);
        }
    }

    PricingContext {
        profiles: pricing_profiles,
        api_keys,
    }
}

/// The identifiers [`turn_tool_context`] threads into the per-turn
/// [`ToolExecutionContext`].
pub(crate) struct TurnToolContextArgs<'a> {
    pub chat_id: &'a str,
    pub user_id: &'a str,
    pub character_id: &'a str,
    pub character_participant_id: &'a str,
    pub image_profile_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    /// The memory slate the LLM saw this turn (v4 passes
    /// `{ semantic, interCharacter, recap }` — dogfood finding #22). `None` only
    /// off the send path (an out-of-prompt tool invocation), which is what yields
    /// `self_inventory`'s `Unavailable` arm.
    pub loaded_memories: Option<&'a LoadedMemoriesContext>,
}

/// Convert the built context's debug memory bags into the [`LoadedMemoriesContext`]
/// `self_inventory` reads (dogfood finding #22 carry-out). v4 passes the debug bags
/// through unfiltered (`orchestrator.service.ts:1136–1151`:
/// `{ semantic: builtContext.debugMemories, interCharacter:
/// builtContext.debugInterCharacterMemories, recap: builtContext.debugMemoryRecap }`);
/// the `self_inventory` builder (`builders.ts:318–329`) then keeps only
/// `{summary, importance, score, effectiveWeight}` per semantic entry and
/// `{aboutCharacterName, summary, importance}` per inter-character entry, which is
/// exactly the [`LoadedSemanticMemory`] / [`LoadedInterCharacterMemory`] shape — so
/// the narrowing happens here. An empty slate still yields a PRESENT context (v4
/// always passes the object), which is `available: true` with empty arrays — never
/// the `Unavailable` arm (that is reserved for out-of-prompt tool invocations).
///
/// Takes the three `BuiltContext` fields directly (rather than the whole struct) so
/// the conversion is unit-testable without constructing a full built context.
fn loaded_memories_from_debug(
    semantic: &[Value],
    inter_character: Option<&[build_context::DebugInterCharacterOut]>,
    recap: Option<&str>,
) -> LoadedMemoriesContext {
    let semantic = semantic
        .iter()
        .map(|m| LoadedSemanticMemory {
            summary: m
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            importance: m.get("importance").and_then(Value::as_f64).unwrap_or(0.0),
            score: m.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            effective_weight: m
                .get("effectiveWeight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        })
        .collect();
    let inter_character = inter_character
        .map(|v| {
            v.iter()
                .map(|m| LoadedInterCharacterMemory {
                    about_character_name: m.about_character_name.clone(),
                    summary: m.summary.clone(),
                    importance: m.importance,
                })
                .collect()
        })
        .unwrap_or_default();
    LoadedMemoriesContext {
        semantic,
        inter_character,
        recap: recap.map(str::to_string),
    }
}

/// Build the per-turn tool context (v4 `orchestrator.service.ts:1136–1151`).
///
/// Extracted so both loops build it identically and so the *threading* is
/// testable: v4 passes the chat's real `imageProfileId` and `projectId` here, and
/// passing `None` instead silently disables every tool that gates on them —
/// `generate_image` ("Image generation is not enabled for this chat") and the
/// project-context tools `doc_list_files` / `doc_grep` / `project_info`
/// ("… requires a project context"). Dogfood finding #22 was exactly that: both
/// call sites hard-coded `None`, so those tools could never succeed in
/// production, while the tier-3 corpus — which drives the loops directly with its
/// own contexts — never saw the call sites at all.
///
/// `loaded_memories` is now threaded too (dogfood finding #22 carry-out): both
/// loops pass the `BuiltContext.debug_*` → [`LoadedMemoriesContext`] conversion, so
/// `self_inventory`'s loaded-memories section reports the real slate the LLM saw
/// instead of the `Unavailable` arm. Still unthreaded, deliberately (see the note
/// at `_image_profile_id`): `browser_user_agent` — v5's request path carries no
/// User-Agent at all.
pub(crate) fn turn_tool_context(args: TurnToolContextArgs<'_>) -> ToolExecutionContext {
    create_tool_context(
        args.chat_id,
        args.user_id,
        args.character_id,
        args.character_participant_id,
        args.image_profile_id.map(String::from),
        None,
        args.project_id.map(String::from),
        None,
        args.loaded_memories.cloned(),
    )
}

/// Fair-rotation pause for rooms where the human drives two or more seats (v4
/// `maybePauseForUserSeatTurn`, orchestrator.service.ts:1703–1853, Bug 50).
///
/// Called on a fresh, non-whisper user send before any responder is resolved.
/// Returns `Some(terminal ProcessMessageResult)` — so the caller returns
/// immediately and the turn chain does not run — when the rotation's next speaker
/// after this post is ANOTHER seat the human drives: it persists the user's
/// message and pauses for that seat. Returns `None` in every other case (single
/// user seat, or an LLM is genuinely up next), letting the normal generation path
/// proceed untouched.
///
/// ⚠ **Carina markup side effect deferred (loud seam).** v4's pause path also
/// fires the inline `@Name` / `@Name?` markup query (`runCarinaMarkupQuery`) as a
/// side effect, mirroring the MAIN path. v5's `process_message` has NEVER wired
/// the user-message markup pass — it is the standing [`OrchestratorSeams`]
/// `user_message_carina` deferral (see the seam note atop this module). Wiring it
/// only on the pause path would be an asymmetric divergence from the still-unwired
/// main path, so this port omits it too; the corpus's pause cases use non-`@Name`
/// content so the missing side effect cannot silently diverge a differential case.
#[allow(clippy::too_many_arguments)]
async fn maybe_pause_for_user_seat_turn<SNK>(
    db: &Db,
    sink: &SNK,
    chat_id: &str,
    chat: &Value,
    options: &SendMessageOptions,
    speaking_as: Option<&str>,
    now_ms: i64,
    random01: f64,
) -> Result<Option<ProcessMessageResult>, DbError>
where
    SNK: EventSink + Sync,
{
    use super::participant_resolver::{
        is_active_character, participants_of, to_filter_participant, to_speaker_participant,
    };
    use crate::db::chats_impersonation::read_impersonating;
    use crate::participant_filters::{find_active_user_participant, is_user_driven_seat};

    let participants = participants_of(chat);
    let impersonating = read_impersonating(chat);

    // Which seat is the human speaking as? (overlay-aware — an impersonated seat
    // counts, Bug 44.)
    let filter_parts: Vec<crate::participant_filters::ParticipantView> =
        participants.iter().map(to_filter_participant).collect();
    let Some(poster_seat) =
        find_active_user_participant(&filter_parts, speaking_as, Some(&impersonating))
    else {
        return Ok(None);
    };
    let poster_id = poster_seat.id.clone();

    // Only relevant when the human drives 2+ seats; otherwise the LLM answering a
    // human post is exactly right and this guard must not change anything.
    // NB: v4's `getActiveCharacterParticipants` is a misnomer (LLM seats only), so
    // filter the FULL roster here — a user-controlled seat AND the impersonated
    // overlay must both count.
    let active_chars: Vec<&Value> = participants
        .iter()
        .filter(|p| is_active_character(p))
        .collect();
    let user_driven_seat_count = active_chars
        .iter()
        .filter(|p| {
            is_user_driven_seat(
                p.get("id").and_then(Value::as_str).unwrap_or_default(),
                p.get("controlledBy")
                    .and_then(Value::as_str)
                    .unwrap_or("llm"),
                Some(&impersonating),
            )
        })
        .count();
    if user_driven_seat_count < 2 {
        return Ok(None);
    }

    // Talkativeness lives on the character record — build the weight map
    // (characterId → talkativeness), vault-overlaid like v4's `findById`.
    let mut characters_map: HashMap<String, crate::select_speaker::SpeakerCharacter> =
        HashMap::new();
    for p in &active_chars {
        let Some(cid) = p
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
        else {
            continue;
        };
        let cid_owned = cid.to_string();
        if let Some(ch) = db.read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::characters_read::find_by_id(main, mount, &cid_owned)
            })
        })? {
            // Insert unconditionally: the map now also carries the archived
            // flag (v4 `d553f72a`), which a talkativeness-gated insert drops.
            characters_map.insert(
                cid.to_string(),
                crate::select_speaker::SpeakerCharacter {
                    talkativeness: ch.get("talkativeness").and_then(Value::as_f64),
                    archived: crate::api::characters::is_archived(&ch),
                },
            );
        }
    }

    let speaker_parts: Vec<crate::select_speaker::SpeakerParticipant> =
        participants.iter().map(to_speaker_participant).collect();
    let next = crate::select_speaker::select_next_speaker_after_user_message(
        &speaker_parts,
        &characters_map,
        &poster_id,
        chat.get("spokenThisCycleParticipantIds")
            .and_then(Value::as_str),
        chat.get("turnQueue").and_then(Value::as_str),
        Some(&poster_id),
        random01,
        Some(&impersonating),
    );
    let next_seat = next.next_speaker_id.as_ref().and_then(|nid| {
        participants
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(nid.as_str()))
    });
    let next_is_user_driven = next_seat.is_some_and(|p| {
        is_user_driven_seat(
            p.get("id").and_then(Value::as_str).unwrap_or_default(),
            p.get("controlledBy")
                .and_then(Value::as_str)
                .unwrap_or("llm"),
            Some(&impersonating),
        )
    });

    // An LLM (or nobody) is up next — let the normal path generate its reply.
    let Some(next_seat) = next_seat.filter(|_| next_is_user_driven) else {
        return Ok(None);
    };
    let next_seat_id = next_seat
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // The floor after this post belongs to another seat the human drives. Persist
    // the message (which advances the persisted cycle via add_message) and pause.
    let user_message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::iso_from_unix_ms(now_ms);
    let mut msg = serde_json::Map::new();
    msg.insert("id".into(), json!(user_message_id));
    msg.insert("type".into(), json!("message"));
    msg.insert("role".into(), json!("USER"));
    msg.insert("content".into(), json!(options.content));
    msg.insert("createdAt".into(), json!(now));
    msg.insert("attachments".into(), json!(options.file_ids));
    msg.insert("participantId".into(), json!(poster_id));
    msg.insert("targetParticipantIds".into(), Value::Null);
    let write_chat_id = chat_id.to_string();
    let event: crate::db::chats_messages::ChatEventInput =
        serde_json::from_value(Value::Object(msg))
            .map_err(|e| DbError::Key(format!("paused user message marshal: {e}")))?;
    db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
        .await?;

    // Link file attachments (warn-on-fail, like v4; corpus keeps this empty).
    for fid in &options.file_ids {
        let fid = fid.clone();
        let mid = user_message_id.clone();
        if let Err(error) = db
            .write(move |w| w.main().files().add_link(&fid, &mid).map(|_| ()))
            .await
        {
            tracing::warn!(
                chat_id = %chat_id,
                error = %format!("{error:?}"),
                "[TurnFairness] Failed to link attachment on paused user turn",
            );
        }
    }

    // Persist the pending turn and tell the client whose floor it is (the client's
    // own recompute lands on the same seat; the frame settles its streaming state).
    super::turn_orchestrator::persist_turn_participant_id(db, chat_id, Some(&next_seat_id)).await?;
    sink.emit(ChatEvent::chain_complete(ChainCompletePayload {
        reason: "user_turn".to_string(),
        next_speaker_id: Some(next_seat_id),
        chain_depth: 0,
    }));

    Ok(Some(ProcessMessageResult {
        is_multi_character: true,
        has_content: false,
        message_id: user_message_id,
        user_participant_id: Some(poster_id),
        is_paused: chat.get("isPaused").and_then(Value::as_bool) == Some(true),
        scene_tracking_character_ids: None,
        skipped: false,
        skipped_participant_id: None,
    }))
}

/// v4 `processMessage`. The main send-path spine. Composes the ported services,
/// reproducing every unported-subsystem gate and routing the subsystem body
/// through an [`OrchestratorSeams`] method. Returns the [`ProcessMessageResult`]
/// the chain driver reads.
#[allow(clippy::too_many_lines, clippy::type_complexity)]
pub async fn process_message<EMB, CMP, STR, SNK, BCS, ORC, RTR, CONF, ACOMP, COST, CARQ, PROS, PF>(
    deps: &mut OrchestratorDeps<
        '_,
        EMB,
        CMP,
        STR,
        SNK,
        BCS,
        ORC,
        RTR,
        CONF,
        ACOMP,
        COST,
        CARQ,
        PROS,
        PF,
    >,
    input: &ProcessMessageInput,
) -> Result<ProcessMessageResult, DbError>
where
    EMB: EmbeddingProvider + Sync,
    CMP: CompletionProvider + Sync,
    STR: StreamingCompletionProvider,
    SNK: EventSink + Sync,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    PF: PricingFetch + Send + Sync,
{
    let db = deps.db;
    let sink = deps.sink;
    let chat_id = input.chat_id.clone();
    let user_id = input.user_id.clone();
    let is_continue_mode = input.options.continue_mode;

    // --- Initial status (orchestrator.service.ts:257–261) ---
    sink.emit(ChatEvent::status(StatusPayload {
        stage: "initializing".into(),
        message: "Loading chat...".into(),
        tool_name: None,
        character_name: None,
        character_id: None,
    }));

    // --- Load chat (orchestrator.service.ts:263–267) ---
    let chat_id_owned = chat_id.clone();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &chat_id_owned))?
        .ok_or_else(|| DbError::Key("Chat not found".into()))?;

    // --- Resolve responding participant (orchestrator.service.ts:269–297) ---
    // respondingId = respondingParticipantId || targetParticipantIds[0].
    let responding_id = input.options.responding_participant_id.clone().or_else(|| {
        input
            .options
            .target_participant_ids
            .as_ref()
            .and_then(|t| t.first().cloned())
    });
    // speakingAs: the per-turn override wins over the persisted chat field.
    let speaking_as = input
        .options
        .speaking_as_participant_id
        .clone()
        .or_else(|| {
            chat.get("activeTypingParticipantId")
                .and_then(Value::as_str)
                .map(String::from)
        });

    // --- Fair-rotation guard for rooms where the human drives 2+ seats
    // (orchestrator.service.ts:294–326, Bug 50) ---
    // The first responder after a human post used to be picked from an LLM-ONLY
    // shortlist (participant-resolver), so a room with a single LLM had that LLM
    // answer EVERY human turn (Charlie→Kumar→Lorian→Kumar…). When the rotation's
    // next speaker after this post is ANOTHER seat the human drives, the floor is
    // theirs: persist the message and pause for that seat. A whisper, a nudge, an
    // explicit responding participant, or a continue-mode turn all bypass this
    // (they name their own speaker), as does any single-user-seat room.
    let multi_character_for_guard = {
        let filter_parts: Vec<crate::participant_filters::ParticipantView> =
            super::participant_resolver::participants_of(&chat)
                .iter()
                .map(super::participant_resolver::to_filter_participant)
                .collect();
        crate::participant_filters::is_multi_character_chat(&filter_parts)
    };
    if !is_continue_mode
        && input.options.responding_participant_id.is_none()
        && input
            .options
            .target_participant_ids
            .as_ref()
            .is_none_or(|t| t.is_empty())
        && input.options.nudge != Some(true)
        && !input.options.content.is_empty()
        && multi_character_for_guard
    {
        if let Some(pause_result) = maybe_pause_for_user_seat_turn(
            db,
            sink,
            &chat_id,
            &chat,
            &input.options,
            speaking_as.as_deref(),
            input.clock.now_ms,
            input.clock.random01,
        )
        .await?
        {
            return Ok(pause_result);
        }
    }

    let resolution = super::participant_resolver::resolve_responding_participant(
        db,
        &chat,
        &user_id,
        responding_id.as_deref(),
        is_continue_mode,
        speaking_as.as_deref(),
        input.clock.random01,
    )
    .await
    .map_err(|e| DbError::Key(format!("participant resolution failed: {e:?}")))?;

    let character_participant = resolution.character_participant.clone();
    let character = resolution.character.clone();
    let connection_profile = resolution.connection_profile.clone();
    // `image_profile_id` feeds the tool build (self_inventory / wardrobe /
    // image tools) AND the per-turn tool context, where it gates `generate_image`.
    //
    // `loadedMemories` is now threaded too (finding #22 carry-out — see
    // `turn_tool_context`, built once below from `built_context`). Still NOT threaded
    // (v4 passes it; orchestrator.service.ts:1143–1151): `browserUserAgent` — v5's
    // request path carries no User-Agent at all — recorded in the dogfood standing
    // notes as a genuine unported input, not a wiring slip.
    let _image_profile_id = resolution.image_profile_id.clone();
    let user_participant_id = resolution.user_participant_id.clone();
    let is_multi_character = resolution.is_multi_character;

    let character_id = json_str(&character, "id").unwrap_or_default();
    let character_name = json_str(&character, "name").unwrap_or_default();
    let character_participant_id = json_str(&character_participant, "id").unwrap_or_default();

    // --- turnStart (chainDepth 0) + status (orchestrator.service.ts:304–316) ---
    sink.emit(ChatEvent::turn_start(TurnStartPayload {
        participant_id: character_participant_id.clone(),
        character_name: character_name.clone(),
        chain_depth: 0,
    }));
    sink.emit(ChatEvent::status(StatusPayload {
        stage: "resolving".into(),
        message: format!("Setting up {character_name}..."),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

    // --- Nudge announcement (orchestrator.service.ts:329–345, v4 6a8a77aa) ---
    // The human explicitly summoned this voice via the Nudge button: the Host
    // announces the invitation so it persists in the transcript instead of a
    // client-only ephemeral note that vanishes on reload. Fires exactly once —
    // the `nudge` flag rides only on this initial summoned request, never on the
    // server-driven chained turns that follow. Best-effort by the writer
    // contract; surfaced live so the household sees it before the summoned reply
    // streams in.
    if is_continue_mode && input.options.nudge == Some(true) {
        let nudge_message = crate::services::host_notifications::post_host_nudge_announcement(
            db,
            crate::services::host_notifications::HostNudgeAnnouncement {
                chat_id: chat_id.clone(),
                character_name: character_name.clone(),
                participant_id: character_participant_id.clone(),
            },
        )
        .await;
        if let Some(posted) = &nudge_message {
            sink.emit(ChatEvent::host_announcement(posted.message.clone()));
        }
    }

    // --- Courier detect (orchestrator.service.ts:318–330) ---
    // The Courier (manual / clipboard transport) needs no API key and no plugin
    // call. Detected here so the tool build + streaming block can short-circuit
    // later. The actual dispatch runs AFTER `build_message_context` (v4 line 1028),
    // since it renders the assembled `formattedMessages`. `is_effective_courier`
    // is recomputed below the danger reroute (a reroute would swap to a non-courier
    // uncensored profile).
    let is_courier_transport =
        json_str(&connection_profile, "transport").as_deref() == Some("courier");

    // --- Resolve user identity (orchestrator.service.ts:332–342) ---
    let identity = super::user_identity_resolver::resolve_user_identity(
        db,
        &user_id,
        &chat,
        speaking_as.as_deref(),
    )
    .await?;
    let user_character = Some(crate::system_prompt::UserCharacter {
        name: identity.name.clone(),
        description: identity.description.clone(),
    });

    // --- Chat settings (orchestrator.service.ts:345) ---
    let chat_settings = deps.orchestrator_seams.chat_settings(&user_id);

    // --- Agent mode (orchestrator.service.ts:351–365, W4.4) ---
    // Resolve the effective agent-mode setting through the cascade
    // Global → Character → Project → Chat (v4 `resolveAgentModeSetting`). The
    // project level reads `project.defaultAgentModeEnabled` — a store-managed
    // (properties.json) field — so it needs the OVERLAID projects read (spanning
    // both DBs), matching v4's `repos.projects.findById(chat.projectId)`.
    let global_agent_mode = chat_settings
        .as_ref()
        .map(|s| crate::services::agent_mode::AgentModeSettings {
            max_turns: s.agent_mode_max_turns,
            default_enabled: s.agent_mode_default_enabled,
        })
        .unwrap_or(crate::services::agent_mode::DEFAULT_AGENT_MODE_SETTINGS);
    let project_agent_default: Option<bool> = match chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(project_id) => {
            let pid = project_id.to_string();
            db.read_main(|main| {
                db.read_mount_index(|mount| {
                    let repo = crate::db::projects::ProjectsRepository::new(main, mount);
                    Ok(repo
                        .find_by_id(&pid)
                        .map_err(|e| DbError::Key(format!("project read failed: {e:?}")))?
                        .and_then(|p| p.get("defaultAgentModeEnabled").and_then(Value::as_bool)))
                })
            })?
        }
        None => None,
    };
    let agent_mode = crate::services::agent_mode::resolve_agent_mode_setting(
        chat.get("agentModeEnabled").and_then(Value::as_bool),
        project_agent_default,
        character
            .get("defaultAgentModeEnabled")
            .and_then(Value::as_bool),
        global_agent_mode,
    );
    let agent_mode_enabled = agent_mode.enabled;

    // Reset agent turn count on a new user message (not continue mode). v4
    // `repos.chats.update(chatId, { agentTurnCount: 0 })` (no `updatedAt` bump).
    if !is_continue_mode && agent_mode_enabled {
        let write_chat_id = chat_id.clone();
        db.write(move |w| {
            w.main()
                .chats()
                .update(
                    &write_chat_id,
                    &crate::db::chats::ChatUpdate {
                        agent_turn_count: Some(0.0),
                        ..Default::default()
                    },
                )
                .map(|_| ())
        })
        .await?;
    }

    // --- Context compression setup (orchestrator.service.ts:371–418) ---
    // requestFullContextOnNextMessage bypass gate (corpus keeps it clear). The
    // ported `ChatUpdate` carries no setter for this column (no ported op writes
    // it), so a chat that HAD it set is a documented deferral; the corpus keeps
    // it clear, so the flag reset never fires and no write is missed.
    let bypass_compression = chat
        .get("requestFullContextOnNextMessage")
        .and_then(Value::as_bool)
        == Some(true);
    // The cheap-LLM selection (compression / danger / recall) is resolved above
    // the seam and threaded via the settings' `cheap_llm_settings_present` flag;
    // the corpus keeps a selection present whenever settings are present.
    let compression_enabled = chat_settings
        .as_ref()
        .map(|s| s.compression_enabled && s.cheap_llm_settings_present && !bypass_compression)
        .unwrap_or(false);

    // --- Danger state (orchestrator.service.ts:420–441 →
    //     danger-orchestrator.service.ts `resolveMessageDangerState`) ---
    // W4.2u: the real resolution replaces the wave-4 stub. Resolve the EFFECTIVE
    // danger settings from the global `dangerousContentSettings` sub-object + the
    // chat's `conciergeOverride`/`chatType` (off-duty / moderation-exempt collapse
    // to OFF). Then reproduce v4's FIRST branch of `resolveMessageDangerState`: a
    // permanently-dangerous chat (`isChatActiveDangerous`) whose mode is not OFF,
    // on a non-continue turn with content, synthesizes danger flags and — under
    // AUTO_ROUTE with a non-`isDangerousCompatible` profile — reroutes to an
    // uncensored provider via the REAL [`DangerousContentRouter`] BEFORE the
    // stream (mutating `effective_profile` / `effective_api_key`).
    //
    // The classify branch (`resolveMessageDangerState` L109 — the cheap-LLM /
    // moderation classification of the current user message) stays the injected
    // gatekeeper seam: its `classifying` status is outside the diffed status
    // vocabulary and, on a not-dangerous result, it writes no system event and
    // performs no reroute — so it is a behavioral no-op on the diffed tables /
    // trace. The gatekeeper JOB (the finalizer's danger-classification enqueue)
    // is the persistence path and stays reachable (its own OFF short-circuit is
    // now wired via `finalizer_danger_off`, below).
    let danger_resolved = resolve_dangerous_content_settings(
        chat_settings
            .as_ref()
            .and_then(|s| s.danger_settings.clone()),
        Some(&chat),
    );
    let danger_settings = danger_resolved.settings;
    let is_dangerous_chat = is_chat_active_dangerous(Some(&chat));

    // --- Cheap-LLM selection (orchestrator.service.ts:390–415; Round-3 Group 8) ---
    // v4 resolves ONE `cheapLLMSelection` here — the provider/model the compression,
    // danger-classification, and proactive-recall (recap/distill) paths all use.
    // `allProfiles = repos.connections.findByUserId(userId)`; `cheapLLMConfig =
    // chatSettings?.cheapLLMSettings || DEFAULT_CHEAP_LLM_CONFIG`. The registry
    // cheapest-model seam is injected `None` (no plugin → the legacy cheap-model
    // map, matching the `context_summary` / `build_context_tier3` precedent). This
    // closes the spine-plumbing deferral: the resolved selection now flows into
    // buildContext (the recap/distill feeders + the cached-compression window), the
    // proactive pre-compute distill (P4.19, `pre_compute::proactive_recall_task`,
    // wired below), and the finalizer's async-compression trigger — previously
    // `None`, which left those feeders inert in `process_message`.
    let available_cheap_profiles: Vec<CheapLlmProfile> = {
        let uid = user_id.clone();
        db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))
            .unwrap_or_default()
            .iter()
            .map(cheap_llm_profile_from_value)
            .collect()
    };
    let cheap_llm_selection: Option<CheapLlmSelection> = {
        let current = cheap_llm_profile_from_value(&connection_profile);
        let config = chat_settings
            .as_ref()
            .map(|s| CheapLlmConfig {
                strategy: if s.cheap_llm_strategy.is_empty() {
                    "PROVIDER_CHEAPEST".to_string()
                } else {
                    s.cheap_llm_strategy.clone()
                },
                user_defined_profile_id: s.cheap_llm_user_defined_profile_id.clone(),
                default_cheap_profile_id: s.cheap_llm_default_cheap_profile_id.clone(),
                fallback_to_local: s.cheap_llm_fallback_to_local,
            })
            // v4 `DEFAULT_CHEAP_LLM_CONFIG` when there is no settings row.
            .unwrap_or_else(|| CheapLlmConfig {
                strategy: "PROVIDER_CHEAPEST".to_string(),
                user_defined_profile_id: None,
                default_cheap_profile_id: None,
                fallback_to_local: true,
            });
        Some(get_cheap_llm_provider(
            &current,
            &config,
            &available_cheap_profiles,
            false,
            None,
        ))
    };

    let mut effective_profile = to_effective_profile(&connection_profile);
    let mut effective_api_key = resolution.api_key.clone().unwrap_or_default();
    let mut did_reroute = false;
    let mut content_was_flagged_dangerous = false;
    // The synthesized flags (v4 attaches them to the saved USER message below).
    let mut danger_flags: Option<Vec<Value>> = None;

    if is_dangerous_chat
        && danger_settings.mode != "OFF"
        && !is_continue_mode
        && !input.options.content.is_empty()
    {
        content_was_flagged_dangerous = true;
        // v4: `categories = chat.dangerCategories?.length ? chat.dangerCategories
        //      : ['unspecified']`; each → a flag { category, score:1, ... }.
        let categories: Vec<String> = chat
            .get("dangerCategories")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or_else(|| vec!["unspecified".to_string()]);
        let mut flags: Vec<Value> = categories
            .iter()
            .map(|cat| {
                json!({
                    "category": cat,
                    "score": 1.0,
                    "userOverridden": false,
                    "wasRerouted": false,
                })
            })
            .collect();

        let profile_is_dangerous_compatible = connection_profile
            .get("isDangerousCompatible")
            .and_then(Value::as_bool)
            == Some(true);
        if danger_settings.mode == "AUTO_ROUTE" && !profile_is_dangerous_compatible {
            let route = deps
                .danger_router
                .resolve(
                    &effective_profile,
                    &effective_api_key,
                    &DangerSettings {
                        mode: danger_settings.mode.clone(),
                        uncensored_text_profile_id: danger_settings
                            .uncensored_text_profile_id
                            .clone(),
                    },
                    &user_id,
                )
                .await;
            if route.rerouted {
                // v4 `markFlagsAsRerouted`: set `wasRerouted` + the rerouted
                // provider/model on every flag.
                for flag in &mut flags {
                    if let Some(obj) = flag.as_object_mut() {
                        obj.insert("wasRerouted".into(), json!(true));
                        obj.insert(
                            "reroutedProvider".into(),
                            json!(route.connection_profile.provider),
                        );
                        obj.insert(
                            "reroutedModel".into(),
                            json!(route.connection_profile.model_name),
                        );
                    }
                }
                effective_profile = route.connection_profile;
                effective_api_key = route.api_key;
                did_reroute = true;
            }
        }
        danger_flags = Some(flags);
    }

    // v4 `isEffectiveCourier = streamingState.effectiveProfile.transport === 'courier'`.
    // A danger reroute swaps the effective profile to a non-courier uncensored
    // provider, so a rerouted turn is NOT courier.
    let is_effective_courier = is_courier_transport && !did_reroute;

    let mut streaming_state = StreamingState {
        effective_profile: Some(effective_profile.clone()),
        effective_api_key: effective_api_key.clone(),
        ..Default::default()
    };

    // --- Roleplay template (orchestrator.service.ts:458) ---
    // chatSettings.defaultRoleplayTemplateId is a seam read; corpus keeps it
    // unset. The result's rendered `system_prompt` is the string buildContext
    // consumes (v4 threads the whole template through, but only its rendered body
    // reaches the context assembler).
    let roleplay_template = super::participant_resolver::get_roleplay_template(db, &chat, None)
        .await
        .map_err(|e| DbError::Key(format!("roleplay template resolution failed: {e:?}")))?
        .map(|r| r.system_prompt);

    // --- Multi-character participant data (orchestrator.service.ts:466–476) ---
    let participant_characters: HashMap<String, Value> = if is_multi_character {
        load_participant_characters(db, &chat, &character_id)?
    } else {
        HashMap::new()
    };

    // --- Existing messages (orchestrator.service.ts:479) ---
    let chat_id_owned = chat_id.clone();
    let mut existing_messages =
        db.read_main(move |c| chats_messages_read::get_messages(c, &chat_id_owned))?;

    // ========================================================================
    // "Nothing to add" turn-skipping — eligibility (orchestrator.service.ts:492)
    // ========================================================================
    // Decide once whether this character may be offered the pass option this
    // turn. Only meaningful in multi-character chats with an LLM-controlled
    // responder. `summoned` (nudge or queue-pop) withholds the option — you don't
    // offer a pass to a voice the operator explicitly called on. NOTE: when the
    // triple is computed, sentinel handling later runs even when `offer_skip` is
    // false — the branches differ (a sentinel without the offer routes to the
    // empty-response branch).
    let chat_participants = turn_orchestrator::participants_array(&chat);
    let turn_skip: Option<build_context::TurnSkip> =
        if crate::skip_signal::qualifies_for_turn_skipping(&chat_participants)
            && json_str(&character_participant, "controlledBy").as_deref() != Some("user")
        {
            let responding = crate::skip_signal::RespondingCharacter {
                id: character_id.clone(),
                name: character_name.clone(),
                aliases: json_str_array(&character, "aliases"),
            };
            let eligibility = crate::skip_signal::compute_skip_eligibility(
                &crate::skip_signal::ComputeSkipEligibilityOptions {
                    events: &existing_messages,
                    participants: &chat_participants,
                    responding_participant_id: &character_participant_id,
                    responding_character: &responding,
                    summoned: input.options.nudge == Some(true)
                        || input.options.chain_selection_reason
                            == Some(turn_orchestrator::ChainSelectionReason::Queue),
                    turn_skipping_enabled: chat.get("turnSkippingEnabled").and_then(Value::as_bool)
                        != Some(false),
                },
            );
            Some(build_context::TurnSkip {
                offer_skip: eligibility.offer_skip,
                recently_addressed: eligibility.recently_addressed,
                character_name: character_name.clone(),
            })
        } else {
            None
        };

    // --- Prospero cadence gate (orchestrator.service.ts:494–534) ---
    let reinject_interval = chat_settings
        .as_ref()
        .map(|s| s.project_context_reinject_interval)
        .unwrap_or(5);
    let message_count = existing_messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .count() as i64;
    let should_inject_context =
        reinject_interval > 0 && message_count > 0 && message_count % reinject_interval == 0;
    if should_inject_context {
        use crate::services::prospero_notifications as prospero;
        // v4 loads the project + general Prospero context (best-effort), posts the
        // public context announcement (pushing it into `existing_messages` so this
        // turn's context includes it), then posts the group-context whisper targeted
        // at the responding character (also pushed). Both are fail-soft.
        let project_id_opt = chat
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let (project_context, general_context) = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    let proj = project_id_opt
                        .as_deref()
                        .and_then(|pid| prospero::load_prospero_project_context(main, mount, pid));
                    let general = prospero::load_prospero_general_context(main, mount);
                    Ok((proj, general))
                })
            })
            .unwrap_or((None, None));
        if project_context.is_some() || general_context.is_some() {
            let posted = prospero::post_prospero_context_announcement(
                db,
                prospero::ProsperoContextAnnouncement {
                    chat_id: chat_id.clone(),
                    project: project_context,
                    general: general_context,
                },
            )
            .await;
            if let Some(msg) = posted {
                existing_messages.push(msg);
            }
        }
        if let Some(msg) = prospero::post_prospero_group_context_whisper(
            db,
            prospero::ProsperoGroupContextWhisper {
                chat_id: chat_id.clone(),
                target_participant_id: character_participant_id.clone(),
                character_id: character_id.clone(),
            },
        )
        .await
        {
            existing_messages.push(msg);
        }
    }

    // --- File attachment processing (orchestrator.service.ts:537–553) ---
    if !input.options.file_ids.is_empty() {
        sink.emit(ChatEvent::status(StatusPayload {
            stage: "processing_files".into(),
            message: "Processing attachments...".into(),
            tool_name: None,
            character_name: Some(character_name.clone()),
            character_id: Some(character_id.clone()),
        }));
    }
    // v4 `loadAndProcessFiles(chatId, fileIds, connectionProfile, userId)` — now
    // real (W4.4b). Uses the ORIGINAL responding `connection_profile` (not the
    // rerouted `effective_profile`), matching v4. Early-returns empty when there
    // are no file ids.
    let file_processing = {
        let process_files_deps = crate::services::chat_files::ProcessFilesDeps {
            db,
            bytes: deps.file_bytes,
            transcoder: deps.image_transcoder,
            completion: deps.completion,
            user_id: &user_id,
            now_ms: input.clock.now_ms,
        };
        crate::services::chat_files::load_and_process_files(
            &process_files_deps,
            &chat_id,
            &connection_profile,
            &input.options.file_ids,
        )
        .await
    };

    // --- Pending tool results → TOOL messages (orchestrator.service.ts:601–624) ---
    // Save each user-initiated pending tool result as a TOOL message BEFORE the
    // user message, and append to `existing_messages` so the model turn sees it.
    // The content JSON is byte-exact in v4's field order.
    if !is_continue_mode && !input.options.pending_tool_results.is_empty() {
        for tr in &input.options.pending_tool_results {
            let content_str = serde_json::to_string(&serde_json::json!({
                "tool": tr.tool,
                "initiatedBy": "user",
                "success": tr.success,
                "result": tr.result,
                "prompt": tr.prompt,
                "arguments": tr.arguments,
            }))
            .map_err(|e| DbError::Key(format!("pending tool result marshal: {e}")))?;
            let tool_id = uuid::Uuid::new_v4().to_string();
            let mut msg = serde_json::Map::new();
            msg.insert("id".into(), json!(tool_id));
            msg.insert("type".into(), json!("message"));
            msg.insert("role".into(), json!("TOOL"));
            msg.insert("content".into(), json!(content_str));
            msg.insert("createdAt".into(), json!(tr.created_at));
            msg.insert("attachments".into(), json!([]));
            let msg_value = Value::Object(msg);
            let write_chat_id = chat_id.clone();
            let event: crate::db::chats_messages::ChatEventInput =
                serde_json::from_value(msg_value.clone())
                    .map_err(|e| DbError::Key(format!("pending tool message marshal: {e}")))?;
            db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
                .await?;
            existing_messages.push(msg_value);
        }
    }

    // --- RNG auto-detect on the user message (orchestrator.service.ts:587–625) ---
    // Gated on `autoDetectRng ?? true`, `!continueMode`, and non-empty content.
    // Each detected pattern is executed and written as a TOOL message, then
    // appended to `existing_messages` so the model turn sees the results in
    // context (v4 appends to `existingMessages`, which was loaded above).
    let auto_detect_rng = chat_settings
        .as_ref()
        .map(|s| s.auto_detect_rng)
        .unwrap_or(true);
    if auto_detect_rng && !is_continue_mode && !input.options.content.is_empty() {
        let rng_calls = detect_and_convert_rng_patterns(&input.options.content);
        for call in &rng_calls {
            let rng_ctx = RngToolContext {
                user_id: user_id.clone(),
                chat_id: chat_id.clone(),
            };
            // v4 `{ type, rolls, modifier: pattern.modifier ?? 0 }` — the same
            // shape reaches the executor AND the persisted TOOL row below.
            let call_modifier = call.modifier.unwrap_or(0);
            let rng_input =
                json!({ "type": call.type_, "rolls": call.rolls, "modifier": call_modifier });
            let output = execute_rng_tool(db, &rng_input, &rng_ctx, deps.rng_bytes)?;
            let formatted = format_rng_results(&output);

            // The TOOL message content — byte-exact JSON in v4's field order.
            let content_str = serde_json::to_string(&RngToolMessageContent {
                tool: "rng",
                initiated_by: "auto-detect",
                success: output.success,
                result: &formatted,
                prompt: &call.match_text,
                arguments: RngArguments {
                    type_: call.type_,
                    rolls: call.rolls,
                    modifier: call_modifier,
                },
            })
            .map_err(|e| DbError::Key(format!("rng tool content marshal: {e}")))?;

            let tool_id = uuid::Uuid::new_v4().to_string();
            let now = crate::clock::iso_from_unix_ms(input.clock.now_ms);
            let mut msg = serde_json::Map::new();
            msg.insert("id".into(), json!(tool_id));
            msg.insert("type".into(), json!("message"));
            msg.insert("role".into(), json!("TOOL"));
            msg.insert("content".into(), json!(content_str));
            msg.insert("createdAt".into(), json!(now));
            msg.insert("attachments".into(), json!([]));
            let msg_value = Value::Object(msg);

            let write_chat_id = chat_id.clone();
            let event: crate::db::chats_messages::ChatEventInput =
                serde_json::from_value(msg_value.clone())
                    .map_err(|e| DbError::Key(format!("rng tool message marshal: {e}")))?;
            db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
                .await?;
            // Include in context building (existing_messages was loaded above).
            existing_messages.push(msg_value);
        }
    }

    // --- Save the user message (orchestrator.service.ts:627–685) ---
    let mut content = String::new();
    let mut user_message_id: Option<String> = None;
    if !is_continue_mode && !input.options.content.is_empty() {
        content = input.options.content.clone();
        let id = uuid::Uuid::new_v4().to_string();
        let now = crate::clock::iso_from_unix_ms(input.clock.now_ms);
        let attachments: Vec<String> = input.options.file_ids.clone();
        let mut msg = serde_json::Map::new();
        msg.insert("id".into(), json!(id));
        msg.insert("type".into(), json!("message"));
        msg.insert("role".into(), json!("USER"));
        msg.insert("content".into(), json!(content));
        msg.insert("createdAt".into(), json!(now));
        msg.insert("attachments".into(), json!(attachments));
        if let Some(pid) = &user_participant_id {
            msg.insert("participantId".into(), json!(pid));
        }
        if let Some(t) = &input.options.target_participant_ids {
            msg.insert("targetParticipantIds".into(), json!(t));
        }
        let write_chat_id = chat_id.clone();
        let msg_value = Value::Object(msg);
        let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(msg_value)
            .map_err(|e| DbError::Key(format!("user message marshal: {e}")))?;
        db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
            .await?;

        // Link file attachments (corpus keeps this empty).
        for fid in &file_processing.attached_file_ids {
            let fid = fid.clone();
            let mid = id.clone();
            db.write(move |w| w.main().files().add_link(&fid, &mid).map(|_| ()))
                .await?;
        }
        user_message_id = Some(id);
    }

    // --- Attach dangerFlags to the saved user message (orchestrator.service.ts:673–685) ---
    // v4 attaches the synthesized flags to the USER message via `updateMessage`
    // (best-effort — a failure is logged and swallowed). Reached only for an
    // actively-dangerous, non-continue turn with content (`danger_flags` is
    // `Some` iff the first-branch fired above).
    if let (Some(flags), Some(mid)) = (&danger_flags, &user_message_id) {
        if !flags.is_empty() {
            let chat_id_owned = chat_id.clone();
            let mid = mid.clone();
            let updates = json!({ "dangerFlags": flags });
            db.write(move |w| {
                w.main()
                    .chat_messages()
                    .update_message(&chat_id_owned, &mid, &updates)
                    .map(|_| ())
            })
            .await?;
        }
    }
    let _ = user_message_id;

    // Build the final user-message content for the context (prefix from
    // attachments, corpus keeps prefix absent).
    let final_user_message = if is_continue_mode {
        None
    } else {
        Some(match &file_processing.message_content_prefix {
            Some(prefix) => format!("{prefix}{content}"),
            None => content.clone(),
        })
    };

    // --- Tool build gate (orchestrator.service.ts:729–900) ---
    // `forceToolsOnNextMessage` — set by the chat-settings save
    // (`chat_admin::update_tool_settings`) when the operator changes the tool
    // roster, and consumed exactly once: this turn tells the model its tools
    // changed (the notice is built and measured in `turn_extras` below), then
    // clears the flag. Until P4.D82 the read was reproduced but the flag was
    // never consumed OR cleared, so a chat that had it set carried it forever.
    let tool_settings_changed =
        chat.get("forceToolsOnNextMessage").and_then(Value::as_bool) == Some(true);
    if tool_settings_changed {
        let write_chat_id = chat_id.clone();
        db.write(move |w| {
            w.main()
                .chats()
                .update(
                    &write_chat_id,
                    &crate::db::chats::ChatUpdate {
                        force_tools_on_next_message: Some(false),
                        ..Default::default()
                    },
                )
                .map(|_| ())
        })
        .await?;
    }

    // Character-permission flags (v4 orchestrator.service.ts:758–762).
    let help_tools_enabled = character
        .get("defaultHelpToolsEnabled")
        .and_then(Value::as_bool)
        == Some(true);
    // `canDressThemselves`/`canCreateOutfits` default ON (`!== false`).
    let can_dress_themselves =
        character.get("canDressThemselves").and_then(Value::as_bool) != Some(false);
    let can_create_outfits =
        character.get("canCreateOutfits").and_then(Value::as_bool) != Some(false);

    // Document editing enabled when the chat's project has linked doc stores
    // (orchestrator.service.ts:766–773). Reads the mount-index sibling DB; a read
    // error leaves it disabled (v4's empty catch).
    let document_editing_enabled = match chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(project_id) => {
            let pid = project_id.to_string();
            db.read_mount_index(move |c| {
                crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(c)
                    .find_by_project_id(&pid)
            })
            .map(|links| !links.is_empty())
            .unwrap_or(false)
        }
        None => false,
    };

    // System-transparency covenant: a non-transparent character has
    // `self_inventory` forced out of the slate (chat/project toggles can't
    // override the character covenant). v4 orchestrator.service.ts:779–782.
    let character_is_transparent =
        character.get("systemTransparency").and_then(Value::as_bool) == Some(true);
    let chat_disabled_tools: Vec<String> = json_str_array(&chat, "disabledTools");
    let effective_disabled_tools: Vec<String> = if character_is_transparent {
        chat_disabled_tools.clone()
    } else {
        let mut set: Vec<String> = chat_disabled_tools.clone();
        if !set.iter().any(|t| t == "self_inventory") {
            set.push("self_inventory".to_string());
        }
        set
    };
    let disabled_tool_groups: Vec<String> = json_str_array(&chat, "disabledToolGroups");

    // ask_carina offered when at least one Carina answerer (a character with
    // canBeCarina) exists, OR the acting character is transparent. Uses the
    // overlay-free raw read — `canBeCarina` is a DB column, not a vault field —
    // to keep this per-turn probe cheap. v4 orchestrator.service.ts:795–806;
    // the cheap-probe comment is load-bearing (carry it). Error-swallowed.
    let ask_carina_enabled = {
        let raw = db
            .read_main(crate::db::characters_read::find_all_raw)
            .unwrap_or_default();
        // An ARCHIVED answerer does not count (v4 `d553f72a`,
        // `orchestrator.service.ts:892`) — it cannot take a Carina turn, so
        // offering the tool on its strength alone would advertise a line
        // nothing can answer.
        let any_can_be_carina = raw.iter().any(|c| {
            !crate::api::characters::is_archived(c)
                && c.get("canBeCarina").and_then(Value::as_bool) == Some(true)
        });
        any_can_be_carina || character_is_transparent
    };

    // Build the tool slate (orchestrator.service.ts:807–834). The Courier exposes
    // NO tools and injects no tool instructions (the external LLM cannot reach
    // them) — v4 skips the whole tool-build step in that case.
    let use_native_web_search_profile = connection_profile
        .get("useNativeWebSearch")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let allow_tool_use = connection_profile
        .get("allowToolUse")
        .and_then(Value::as_bool);
    let allow_web_search = connection_profile
        .get("allowWebSearch")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (mut tools, model_supports_native_tools, use_native_web_search) = if is_effective_courier {
        (Vec::new(), false, false)
    } else {
        // v4 `buildTools` → `checkModelSupportsTools(effectiveProfile.provider,
        // effectiveProfile.modelName, userId)`: the async pricing-cache lookup,
        // sourced here from the real fetcher (only OPENROUTER consults the live
        // cache; every other provider answers from the static fallback table). The
        // fetch itself stays a seam. The Courier arm hardcodes `false` (v4 skips
        // the tool build there entirely).
        let pricing_ctx = build_pricing_context(db, &user_id);
        let model_supports = deps.pricing.check_model_supports_tools(
            &effective_profile.provider,
            &effective_profile.model_name,
            input.clock.now_ms,
            &pricing_ctx,
        );

        // Pascal's custom pseudo-tools (`run_custom`). The roster's perspective is
        // the RESPONDING character's, because a character-tier store shadows the
        // farther tiers — two characters in one room can hold different definitions
        // of the same tool name. `character_id` is REQUIRED (the group tier is keyed
        // on the responding character's own memberships and silently resolves to []
        // without it); `character_mount_point_id` is merely the fast path to their
        // vault. Withholding the context entirely is HOW the chat-level `customTools`
        // setting turns the feature off: `build_tools` then never lists a mount.
        let custom_tools_enabled = chat_settings
            .as_ref()
            .map(|s| s.custom_tools)
            .unwrap_or(true);
        let custom_tool_context = if custom_tools_enabled {
            let participant_character_ids: Vec<String> = chat
                .get("participants")
                .and_then(Value::as_array)
                .map(|ps| {
                    ps.iter()
                        .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                        .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Some(crate::pascal::roster::RosterContext {
                user_id: user_id.clone(),
                chat_id: chat_id.clone(),
                character_id: character
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                character_mount_point_id: character
                    .get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                character_ids: Some(participant_character_ids),
                project_id: chat
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                // The responding character's fact sheet, which availability
                // gates are answered against. Already hydrated by the
                // participant resolver, so this costs nothing.
                metadata: Some(
                    character
                        .get("metadata")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default(),
                ),
            })
        } else {
            None
        };

        let built = tool_build::build_tools(
            db,
            &user_id,
            &BuildToolsInput {
                provider: &effective_profile.provider,
                use_native_web_search: use_native_web_search_profile,
                allow_tool_use,
                allow_web_search,
                image_profile_id: _image_profile_id.as_deref(),
                // Image-provider constraint enrichment reads the provider registry
                // (W4.7) — injected `None` here (the base image tool). See tool_build.
                image_provider_constraints: None,
                project_id: chat
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty()),
                request_full_context: compression_enabled,
                disabled_tools: Some(&effective_disabled_tools),
                disabled_tool_groups: &disabled_tool_groups,
                agent_mode_enabled,
                is_multi_character,
                help_tools_enabled,
                can_dress_themselves,
                can_create_outfits,
                document_editing_enabled,
                ask_carina_enabled,
                // The Brahma Console (unported) is the only caller that flips these.
                include_workspace_tools: true,
                exclude_memory_search: false,
                sql_access: false,
                model_supports_native_tools: model_supports,
                provider_supports_web_search: input.provider_supports_web_search,
                custom_tool_context,
            },
        )?;
        (
            built.tools,
            built.model_supports_native_tools,
            built.use_native_web_search,
        )
    };

    // 4.6 Private Character Rooms — destructive-tool filter for autonomous rooms
    // (orchestrator.service.ts:836–861). The user `destructiveToolPolicy` is a
    // CEILING; the per-room `runDestructiveToolsAllowed === 1` (raw integer read,
    // not coerced) must be set. The corpus keeps chats non-autonomous → the
    // branch is reproduced but never fires.
    if chat.get("chatType").and_then(Value::as_str) == Some("autonomous") {
        let policy = chat_settings
            .as_ref()
            .map(|s| s.autonomous_destructive_policy.clone())
            .unwrap_or_else(|| "opt_in_per_room".to_string());
        let allowed_at_room = chat
            .get("runDestructiveToolsAllowed")
            .and_then(Value::as_i64)
            == Some(1);
        if !tool_build::destructive_allowed(&policy, allowed_at_room) {
            tool_build::filter_destructive_tools(&mut tools);
        }
    }

    // Resolve the pseudo-tool mode + the actual slate (orchestrator.service.ts:863–899).
    let profile_pseudo_tool_mode = connection_profile
        .get("pseudoToolMode")
        .and_then(Value::as_str)
        .and_then(ToolMode::from_str);
    // Courier sends no tools anyway; the mode value is unused (v4 line 864).
    let resolved_tool_mode = if is_effective_courier {
        ResolvedToolMode::Native
    } else {
        check_resolved_tool_mode(model_supports_native_tools, profile_pseudo_tool_mode)
    };
    let use_text_block_tools =
        !is_effective_courier && resolved_tool_mode != ResolvedToolMode::Native;
    // Under a pseudo-tool surface the native slate is suppressed (`actualTools = []`).
    let actual_tools: Vec<Value> = if use_text_block_tools {
        Vec::new()
    } else {
        tools
    };

    // Tool instructions — simple-json / text-block / native, mode-switched. The
    // Courier injects NO tool instructions (v4 line 872: `toolInstructions = undefined`).
    let tool_instructions: Option<String> = if is_effective_courier {
        None
    } else if resolved_tool_mode == ResolvedToolMode::SimpleJson {
        let opts = determine_text_block_tool_options(
            _image_profile_id.as_deref(),
            allow_web_search,
            is_multi_character,
            chat.get("projectId")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty()),
            Some(help_tools_enabled),
            Some(can_dress_themselves),
            Some(can_create_outfits),
        );
        Some(build_simple_json_system_instructions(&opts))
    } else if resolved_tool_mode == ResolvedToolMode::TextBlock {
        let opts = determine_text_block_tool_options(
            _image_profile_id.as_deref(),
            allow_web_search,
            is_multi_character,
            chat.get("projectId")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty()),
            Some(help_tools_enabled),
            Some(can_dress_themselves),
            Some(can_create_outfits),
        );
        Some(build_text_block_system_instructions(&opts))
    } else if !actual_tools.is_empty() {
        Some(build_native_tool_system_instructions())
    } else {
        None
    };

    // Provider stop sequences (simple-json only) — applied to the primary stream.
    let initial_stop_sequences: Vec<String> = if resolved_tool_mode == ResolvedToolMode::SimpleJson
    {
        SIMPLE_JSON_STOP.iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    };

    // The tool slate as the wire JSON array (`tools.length > 0 ? tools : undefined`
    // is applied by the stream provider). `actual_tools_value` is `None` when the
    // slate is empty so `params.tools` matches v4's `undefined`.
    let actual_tools_value: Option<Value> = if actual_tools.is_empty() {
        None
    } else {
        Some(Value::Array(actual_tools.clone()))
    };

    // --- Turn extras (turn-extras.ts, v4 `f933ba9c`) ---
    // Everything this turn will add to the payload *after* the context is built —
    // the tool schemas, plus the agent-mode and tool-change system messages
    // spliced in below. Built and measured HERE, before the context, so the
    // builder can hold room for them instead of packing history into space they
    // are about to occupy. The strings are reused at the splice sites; rebuilding
    // them there is how a reservation and the text it pays for drift apart.
    let turn_extras = crate::services::turn_extras::collect_turn_extras(
        crate::services::turn_extras::TurnExtrasOptions {
            tools: &actual_tools,
            agent_mode_enabled,
            agent_mode_max_turns: agent_mode.max_turns,
            tool_settings_changed,
            chars_per_token: crate::token_estimation::DEFAULT_CHARS_PER_TOKEN,
        },
    );

    // --- Async pre-compression cache check (pre-compute.service.ts compressionTask,
    //     W4.4a4) ---
    // When compression is enabled and not bypassed, consult the cache the finalizer
    // pre-computed so buildContext can skip synchronous compression. Count only
    // visible USER/ASSISTANT messages (the same domain `triggerAsyncCompression`
    // uses). Round-3 Group 8 threads the resolved `cheap_llm_selection` into
    // buildContext, so the cached-compression window is now live (it is still gated
    // on `compression_enabled`, which the corpus keeps false → the read no-ops here).
    let (cached_compression_result, cached_compression_message_count) =
        if compression_enabled && !bypass_compression {
            let raw: Vec<crate::chat_tasks::RawMessage> = existing_messages
                .iter()
                .map(|m| crate::chat_tasks::RawMessage {
                    type_: m.get("type").and_then(Value::as_str).map(str::to_string),
                    role: m.get("role").and_then(Value::as_str).map(str::to_string),
                    content: m.get("content").and_then(Value::as_str).map(str::to_string),
                })
                .collect();
            let actual_message_count =
                crate::chat_tasks::extract_visible_conversation(&raw).len() as i64;
            let participant_for_cache = if is_multi_character {
                Some(character_participant_id.clone())
            } else {
                None
            };
            let response = super::compression_cache::get_cached_compression(
                db,
                &chat_id,
                actual_message_count,
                participant_for_cache.as_deref(),
                None,
            )
            .await?;
            match response {
                Some(r) => (Some(r.result), Some(r.cached_message_count)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

    // --- Proactive memory recall (pre-compute.service.ts proactiveRecallTask,
    //     P4.19) ---
    // v4 runs this in the SAME `Promise.all` as the compression cache check above;
    // v5 runs them sequentially at this one spine site — the parallelism is a
    // latency optimization with no DB-observable effect (see `pre_compute.rs`). The
    // task distills the messages since this character last spoke into recall signals
    // and pre-searches the character's memories; a non-empty result rides
    // buildContext as the `'pre-searched'` dynamic head (suppressing the fallback
    // distill there), and the signals seed the retrospective cadence on both paths.
    //
    // Characters present in the room this turn: the responding character plus every
    // other character participant (`participant_characters` is keyed by characterId
    // and empty in single-character chats). Threaded into the proactive recall path
    // so memories ABOUT a present character get the participant-aware boost (item 4);
    // the dynamic-head fallback computes the same set itself (v4
    // orchestrator.service.ts:1013).
    let present_about_character_ids: Vec<String> = {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<String> = Vec::new();
        if !character_id.is_empty() && seen.insert(character_id.clone()) {
            out.push(character_id.clone());
        }
        for id in participant_characters.keys() {
            if !id.is_empty() && seen.insert(id.clone()) {
                out.push(id.clone());
            }
        }
        out
    };
    // v4 `dangerSettings` in the cheap-LLM subset the reroute reads.
    let danger_for_recall = crate::cheap_llm::DangerousContentSettings {
        mode: danger_settings.mode.clone(),
        uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
    };
    let (pre_searched_memories, recall_signals) = {
        // The two status frames (`recalling_keywords`, `recalling_memories`) v4 emits
        // from inside `proactiveRecallTask`; the closure stamps the character name/id.
        let mut emit_status = |stage: &str, message: String| {
            sink.emit(ChatEvent::status(StatusPayload {
                stage: stage.to_string(),
                message,
                tool_name: None,
                character_name: Some(character_name.clone()),
                character_id: Some(character_id.clone()),
            }));
        };
        let recall_input = crate::services::pre_compute::ProactiveRecallInput {
            chat: &chat,
            character_id: &character_id,
            character_name: &character_name,
            character_participant_id: &character_participant_id,
            present_about_character_ids: &present_about_character_ids,
            is_continue_mode,
            content: &content,
            existing_messages: &existing_messages,
            cheap_llm_selection: cheap_llm_selection.as_ref(),
            danger_settings: &danger_for_recall,
            available_profiles: &available_cheap_profiles,
            user_id: &user_id,
            chat_id: &chat_id,
            now_ms: input.clock.now_ms,
            server_tz: input.server_tz.as_deref(),
        };
        match crate::services::pre_compute::proactive_recall_task(
            db,
            deps.embedding,
            deps.completion,
            deps.executor,
            &recall_input,
            &mut emit_status,
        )
        .await
        {
            Some(outcome) => (outcome.memories, outcome.signals),
            None => (None, None),
        }
    };

    // --- Build context (orchestrator.service.ts:908–1010) ---
    sink.emit(ChatEvent::status(StatusPayload {
        stage: "gathering".into(),
        message: "Gathering memories and context...".into(),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

    let build_input = build_context_input(BuildContextArgs {
        user_id: &user_id,
        model_context_limit: input.model_context_limit,
        // Room held back for the tool schemas and the system messages spliced in
        // after the context is built (v4 `f933ba9c`).
        reserved_outgoing_tokens: Some(turn_extras.reserved_tokens),
        timestamp_config: input.timestamp_config.clone(),
        timezone: input.timezone.clone(),
        server_tz: input.server_tz.clone(),
        is_continue_mode,
        now_ms: input.clock.now_ms,
        local_offset_minutes: input.clock.local_offset_minutes,
        chat: &chat,
        character: &character,
        character_participant: &character_participant,
        connection_profile: &effective_profile_profile(&connection_profile),
        user_character,
        roleplay_template,
        is_multi_character,
        participant_characters: &participant_characters,
        existing_messages: &existing_messages,
        final_user_message: final_user_message.clone(),
        speaking_as: speaking_as.clone(),
        tool_instructions,
        compression_enabled,
        bypass_compression,
        cached_compression_result,
        cached_compression_message_count,
        cheap_llm_selection: cheap_llm_selection.clone(),
        // v4 `uncensoredFallbackOptions: (isChatActiveDangerous && dangerSettings &&
        // cheapLLMSelection) ? { dangerSettings, availableProfiles: allProfiles,
        // isDangerousChat: true } : undefined`. The corpus salons are not dangerous,
        // so this is `None` there; wired faithfully for the dangerous path.
        uncensored_fallback: if is_dangerous_chat && cheap_llm_selection.is_some() {
            Some(build_context::OwnedUncensoredFallback {
                danger_settings: crate::cheap_llm::DangerousContentSettings {
                    mode: danger_settings.mode.clone(),
                    uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
                },
                available_profiles: available_cheap_profiles.clone(),
            })
        } else {
            None
        },
        // U4.4: the autonomous per-turn context cap (v4 orchestrator.service.ts:984
        // passes `options.autonomousContextCap` straight through to buildContext).
        autonomous_context_cap: input.options.autonomous_context_cap,
        // "Nothing to add" turn-skipping — per-turn ephemeral instruction control
        // (v4 orchestrator.service.ts:1030 threads `turnSkip` through
        // `buildMessageContext` into buildContext).
        turn_skip: turn_skip.clone(),
        // Proactive memory recall results (v4 orchestrator.service.ts:1075-1076 pass
        // `preSearchedMemories` / `recallSignals` into `buildMessageContext`, P4.19).
        pre_searched_memories,
        recall_signals,
    });

    // v4 `buildMessageContext` (context-builder.service.ts): the wrapper that runs
    // the whisper pre-filters + `buildConversationMessages` above the buildContext
    // call, then post-processes the result (provider name attribution, the Lantern
    // image merge, the trailing-prefix injection, and the multi-character scene
    // block). The per-character opaque-anywhere test reads each present character's
    // `systemTransparency` (the responder from `character`, the rest from
    // `participant_characters`).
    let mut participant_transparency: HashMap<String, Option<bool>> = HashMap::new();
    if is_multi_character {
        if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
            for p in parts {
                if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                let Some(cid) = p
                    .get("characterId")
                    .and_then(Value::as_str)
                    .filter(|c| !c.is_empty())
                else {
                    continue;
                };
                let status = p.get("status").and_then(Value::as_str).unwrap_or("active");
                if status != "active" && status != "silent" {
                    continue;
                }
                let transp = if cid == character_id {
                    character.get("systemTransparency").and_then(Value::as_bool)
                } else {
                    participant_characters
                        .get(cid)
                        .and_then(|c| c.get("systemTransparency").and_then(Value::as_bool))
                };
                participant_transparency.insert(cid.to_string(), transp);
            }
        }
    }
    let empty_participants: Vec<Value> = Vec::new();
    let cp_created_at = json_str(&character_participant, "createdAt");
    // The EFFECTIVE connection profile's stored row. v4 passes
    // `streamingState.effectiveProfile` — the full object — to both
    // `buildMessageContext` (for `profileUsesNamePrefill`) and `profileParams`
    // (for `modelParams`); v5 carries the rerouted profile as the 4-field
    // [`EffectiveProfile`], so a rerouted turn re-reads the row the router
    // already loaded. The old hardcoded anchor test read
    // `effective_profile.provider`, so reading the ORIGINAL profile here would
    // be a regression on the danger-reroute path — measured: the tier-3's
    // `danger_live_reroute` case goes red.
    let effective_profile_row: Value = if did_reroute {
        match db.read_main(|c| crate::db::connection_profiles::find_by_id(c, &effective_profile.id))
        {
            Ok(Some(row)) => row,
            // v4 cannot lose the rerouted profile (it carries the full object
            // in `streamingState.effectiveProfile`); v5 re-reads the row, so a
            // failed read or a same-turn delete falls back to the ORIGINAL
            // profile's anchor + params. Rare, but it must be LOUD — a silent
            // fallback here is undiagnosable (the aa464abf unification
            // review's finding).
            other => {
                let outcome = match other {
                    Ok(_) => "row missing".to_string(),
                    Err(e) => format!("read failed: {e}"),
                };
                tracing::warn!(
                    target: "quilltap::orchestrator",
                    profile_id = %effective_profile.id,
                    %outcome,
                    "Rerouted profile row unavailable at context build; falling back to the original profile's anchor and parameters",
                );
                connection_profile.clone()
            }
        }
    } else {
        connection_profile.clone()
    };
    let mc_params = message_context::MessageContextParams {
        is_multi_character,
        provider: &effective_profile.provider,
        responding_character_name: &character_name,
        responding_participant_id: &character_participant_id,
        has_history_access: character_participant
            .get("hasHistoryAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        character_participant_created_at: cp_created_at.as_deref(),
        character_system_transparency: character.get("systemTransparency").and_then(Value::as_bool),
        // v4 `profileUsesNamePrefill(connectionProfile)` at the call site
        // (`23af7146`). The profile is the same object the old hardcoded
        // `connectionProfile.provider === 'ANTHROPIC'` test read.
        use_prefill: crate::services::multi_character_prefill::profile_uses_name_prefill_value(
            &effective_profile_row,
        ),
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&empty_participants),
        participant_transparency: &participant_transparency,
    };

    // The real Lantern K-seam loader (W4.4b) — closes
    // `MessageContextSeams::load_lantern_images`.
    let mc_seams = crate::services::chat_files::RealMessageContextSeams {
        deps: crate::services::chat_files::ProcessFilesDeps {
            db,
            bytes: deps.file_bytes,
            transcoder: deps.image_transcoder,
            completion: deps.completion,
            user_id: &user_id,
            now_ms: input.clock.now_ms,
        },
        connection_profile: &connection_profile,
    };
    let mc_result = message_context::build_message_context(
        db,
        deps.embedding,
        deps.completion,
        deps.executor,
        deps.build_context_seams,
        &mc_seams,
        &mc_params,
        build_input,
        &existing_messages,
        &file_processing.attachments_to_send,
    )
    .await
    .map_err(build_context_err_to_db)?;
    let built_context: BuiltContext = mc_result.built_context;

    // The wrapper's formatted messages feed the primary stream + the tool loops.
    // Extract them as a mutable local so the agent-mode instruction injection below
    // (v4 orchestrator.service.ts:1117–1142) mutates the exact array all three
    // downstream consumers read.
    let mut formatted_messages = mc_result.formatted_messages;

    // --- Tool Change Notification (orchestrator.service.ts:1224–1254) ---
    // When the operator changed the tool roster, tell the model once. The notice
    // is the string `collect_turn_extras` already measured (and reserved room
    // for); it goes in before the LAST message when that is the user's new
    // message, else at the end. v4's `findIndex` matches only a user message in
    // the final position, and its `> 0` guard means a conversation whose ONLY
    // message is that user turn takes the push arm.
    if let Some(notice) = turn_extras.tool_change_notice.clone() {
        let tool_change_message = message_context::FormattedMsg {
            role: "system".to_string(),
            content: notice,
            name: None,
            thought_signature: None,
            attachments: Some(Vec::new()),
        };
        // v4's predicate is `m.role === 'user' && i === arr.length - 1`, so the
        // index is the last position when a user message sits there, else -1.
        let last_user_index = formatted_messages
            .last()
            .filter(|m| m.role == "user")
            .map(|_| formatted_messages.len() - 1);
        match last_user_index {
            Some(idx) if idx > 0 => formatted_messages.insert(idx, tool_change_message),
            _ => formatted_messages.push(tool_change_message),
        }
        let tool_names = crate::services::turn_extras::extract_tool_names(&actual_tools);
        tracing::info!(
            chat_id = %chat_id,
            tool_count = tool_names.len(),
            tools = ?tool_names,
            "Injected tool change notification"
        );
    }

    // --- Agent Mode Instructions (orchestrator.service.ts:1113–1142, W4.4) ---
    // When agent mode is enabled, inject the agent-mode system-prompt block at the
    // first non-system position (or unshift / push per v4's index logic). This is
    // part of the provider request, so it changes the canned stream key — the
    // corpus's agent-mode cases bank the byte-exact injection.
    if let Some(instructions) = turn_extras.agent_mode_instructions.clone() {
        let agent_mode_message = message_context::FormattedMsg {
            role: "system".to_string(),
            // The string `collect_turn_extras` measured — not a rebuild of it.
            content: instructions,
            name: None,
            thought_signature: None,
            attachments: Some(Vec::new()),
        };
        let first_non_system = formatted_messages.iter().position(|m| m.role != "system");
        match first_non_system {
            Some(idx) if idx > 0 => formatted_messages.insert(idx, agent_mode_message),
            Some(_) => formatted_messages.insert(0, agent_mode_message),
            None => formatted_messages.push(agent_mode_message),
        }
    }

    // --- Pre-send payload check (orchestrator.service.ts:1314–1345, `f933ba9c`) ---
    // Measure the WHOLE payload, not just the message array: the tool schemas ride
    // alongside it and count against the same window. The ceiling is the budget's
    // own `safe_input_limit` — the number the builder packed to — so this check
    // cannot fire on a context that was filled exactly as instructed. Log-only in
    // v4 as here: the turn still goes out, and the provider decides.
    {
        let pairs: Vec<(String, String)> = formatted_messages
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let message_tokens = crate::token_estimation::count_messages_tokens(
            &pairs,
            crate::token_estimation::DEFAULT_CHARS_PER_TOKEN,
        );
        let estimated_input_tokens = message_tokens + turn_extras.tool_schema_tokens;
        let safe_input_limit = built_context.budget.safe_input_limit;
        if estimated_input_tokens > safe_input_limit {
            tracing::warn!(
                chat_id = %chat_id,
                character_id = %character_id,
                estimated_input_tokens,
                message_tokens,
                tool_schema_tokens = turn_extras.tool_schema_tokens,
                tool_count = actual_tools.len(),
                safe_input_limit,
                model_context_limit = built_context.budget.total_limit,
                response_reserve = built_context.budget.response_reserve,
                safety_margin = built_context.budget.safety_margin,
                reserved_outgoing_tokens = turn_extras.reserved_tokens,
                overage = estimated_input_tokens - safe_input_limit,
                "Context exceeds model safe input limit, payload may be rejected"
            );
        }
    }

    sink.emit(ChatEvent::status(StatusPayload {
        stage: "preparing".into(),
        message: format!("Preparing request for {character_name}..."),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

    // --- The Courier (manual / clipboard transport) short-circuit
    //     (orchestrator.service.ts:1022–1044) ---
    // Rendered AFTER `build_message_context` + the `preparing` status (v4's order:
    // the courier dispatch consumes `formatted_messages`). Render the assembled
    // request as Markdown, persist a placeholder assistant message, pause the chat,
    // and emit the SSE frames. Turn chaining halts on `isPaused = true` (the ported
    // `should_chain_next` already stops on paused). The agent-mode injection above is
    // inert for a courier chat (the corpus keeps agent mode off), matching v4 (which
    // returns before its agent-mode block).
    if is_effective_courier {
        return super::courier_transport::dispatch_courier_transport(
            db,
            sink,
            super::courier_transport::DispatchCourierOptions {
                chat_id: chat_id.clone(),
                chat: chat.clone(),
                character: character.clone(),
                character_participant: character_participant.clone(),
                user_participant_id: user_participant_id.clone(),
                is_multi_character,
                participant_characters: participant_characters.clone(),
                resolved_identity_name: identity.name.clone(),
                formatted_messages: formatted_messages.clone(),
                effective_provider: Some(effective_profile.provider.clone())
                    .filter(|s| !s.is_empty()),
                effective_model_name: Some(effective_profile.model_name.clone())
                    .filter(|s| !s.is_empty()),
                courier_delta_mode: connection_profile
                    .get("courierDeltaMode")
                    .and_then(Value::as_bool)
                    != Some(false),
                now_ms: input.clock.now_ms,
            },
        )
        .await;
    }

    // --- Primary stream (orchestrator.service.ts:1205–1255) ---
    let pre_generated_assistant_message_id = uuid::Uuid::new_v4().to_string();
    let character_aliases = json_str_array(&character, "aliases");
    let participant_status = json_str(&character_participant, "status");

    let mut preserve = PreservePartialOnError::new(
        chat_id.clone(),
        character_id.clone(),
        character_name.clone(),
        character_aliases.clone(),
        character_participant_id.clone(),
        participant_status.clone(),
        pre_generated_assistant_message_id.clone(),
    );

    let previous_response_id =
        primary_stream::find_previous_response_id(&effective_profile.provider, &existing_messages);

    // Build the stream params from the wrapper's formatted messages (v4 forwards
    // `formattedMessages`, the model params, no tools [corpus], no web search).
    // An assistant turn's persisted `thoughtSignature` rides through (the google
    // round-trip — v4 passes it on the message; P4.13 threads it to the wire),
    // and the last user message's `attachments` ride with it (v4
    // `streaming.service.ts` maps `attachments: m.attachments`; P4.21 — this
    // conversion reading `content` alone was dogfood #37's drop site 1).
    let stream_messages: Vec<StreamMessage> = formatted_messages
        .iter()
        .map(|m| match m.role.as_str() {
            "system" => StreamMessage::system(m.content.clone()),
            "assistant" => StreamMessage::Assistant {
                content: m.content.clone(),
                tool_calls: Vec::new(),
                reasoning_content: None,
                thought_signature: m.thought_signature.clone(),
                cache_control: None,
            },
            _ => StreamMessage::User {
                content: m.content.clone(),
                cache_control: None,
                attachments: m.attachments.clone().unwrap_or_default(),
            },
        })
        .collect();
    // v4 `orchestrator.service.ts:1034`:
    //   const modelParams = profileParams(streamingState.effectiveProfile) ?? {}
    // and `streaming.service.ts:395-400` reads the request's temperature /
    // maxTokens / topP off that bag and forwards the bag itself as
    // `profileParameters`.
    //
    // P4.D79 found v5 had NO twin of this line: the primary stream sent
    // `temperature` read from a top-level `connection_profile.temperature` key
    // that connection profiles do not have (so always `None`), and `maxTokens`
    // / `topP` / `profileParameters` all hard-`None` — every per-model setting
    // (DeepSeek reasoning off, an Ollama `num_ctx`, a profile's temperature)
    // was silently dropped on the Salon's main path. The tier-3 corpus could
    // not see it because every corpus profile left `parameters` at its `{}`
    // default; the P4.D79 corpus gives the Primary profile a real bag and the
    // oracle now records the whole `modelParams` at the wire.
    //
    // The bag comes from the EFFECTIVE profile (resolved above, beside the turn
    // anchor's own read of the same row), so a danger reroute uses the rerouted
    // profile's parameters.
    let model_params =
        crate::cheap_llm::profile_params_value(&effective_profile_row).unwrap_or_else(|| json!({}));
    // v4 forwards `actualTools` + `useNativeWebSearch` + `initialStopSequences`
    // into the primary stream. The stream provider maps `tools.length > 0 ? tools
    // : undefined` (here `actual_tools_value` is `None` for an empty slate).
    // P4.D83 (v4 `d89babc4`): the three sampling knobs come from the ONE
    // resolver. v4 read `modelParams.maxTokens` / `.topP` here — camelCase names
    // the profile editor never writes — so on this path a profile's Max Tokens
    // and Top P were dropped on every turn and the provider's own defaults
    // applied (on Ollama, `num_predict 4096` / `top_p 1`). v5 had the same bug,
    // and worse: it also reached regenerate-only agreement, so the original
    // reply and a regeneration of it disagreed.
    //
    // `maxTokens` keeps its typed seam: v4 forwards a fractional cell verbatim
    // where the typed i64 truncates it (1000.5 → 1000) — noted at the aa464abf
    // unification review, unchanged here.
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&model_params));
    let params = StreamParams {
        messages: stream_messages,
        model: effective_profile.model_name.clone(),
        temperature: sampling.temperature,
        max_tokens: sampling.max_tokens.map(|n| n as i64),
        top_p: sampling.top_p,
        tools: actual_tools_value.clone(),
        web_search_enabled: use_native_web_search,
        profile_parameters: Some(model_params.clone()),
        cache_key: None,
        previous_response_id,
        stop: initial_stop_sequences.clone(),
    };

    let is_paused = chat
        .get("isPaused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let primary = primary_stream::run_primary_stream(
        db,
        deps.streaming,
        sink,
        &mut preserve,
        RunPrimaryStreamOptions {
            chat_id: chat_id.clone(),
            user_id: user_id.clone(),
            chat: primary_stream::PrimaryStreamChat { is_paused },
            character_id: character_id.clone(),
            character_name: character_name.clone(),
            character_aliases: character_aliases.clone(),
            participant_id: character_participant_id.clone(),
            participant_status: participant_status.clone(),
            user_participant_id: user_participant_id.clone(),
            is_multi_character,
            params: params.clone(),
            attached_files: Vec::new(),
            original_message: Some(input.options.content.clone()),
            pre_generated_assistant_message_id: pre_generated_assistant_message_id.clone(),
            // U4.4: the run-id context the autonomous turn threads through (v4's
            // `runWithAutonomousRunId` scope covers the whole generation).
            log_context: input.log_context.clone(),
            state: &mut streaming_state,
        },
    )
    .await
    .map_err(|e| DbError::Key(format!("primary stream failed: {}", e.message)))?;

    if let Some(early) = primary.early_return {
        // Request-limit recovery handled the whole request.
        return Ok(ProcessMessageResult {
            is_multi_character: early.is_multi_character,
            has_content: early.has_content,
            message_id: early.message_id.unwrap_or_default(),
            user_participant_id: early.user_participant_id,
            is_paused: early.is_paused,
            scene_tracking_character_ids: None,
            skipped: false,
            skipped_participant_id: None,
        });
    }

    // --- Native tool loop (orchestrator.service.ts:1259–1280) ---
    // v4 runs `runNativeToolLoop` unconditionally after the primary stream; the
    // loop breaks immediately when the last raw response carries no native tool
    // calls. The real tool slate (`buildTools`, W4.1g) now flows into `params`,
    // and the tool runner is the real [`BuiltInToolRunner`] (dispatches the ported
    // handlers). Native detection is the injected W4.7 provider-parse seam
    // (`deps.tool_detector`; production wires [`native_tool_loop::NoToolCallDetector`]
    // → no calls until the provider manifest lands). The resolved `agent_mode`
    // (W4.4 cascade) sets the loop's iteration cap + `submit_final_response`
    // acceptance. The text tool passes (W4.1f) follow here.
    //
    // The runner's `self_inventory` host env + the `search`/`help_search`
    // embedding provider are host seams (the corpus's native-call cases call
    // handlers needing neither — wardrobe/read_conversation/doc reads); production
    // wires the real host env + embedding provider into the runner (a tracked
    // deferral, the standing SelfInventoryEnv seam).
    let mut tool_messages: Vec<ToolMessage> = Vec::new();
    let mut generated_image_paths: Vec<GeneratedImage> = Vec::new();

    // The per-turn built-in tool runner, with the injected Carina engine wired for
    // the `ask_carina` dispatch (v4's real `executeToolCallWithContext` routes it
    // there); `not_available` by default keeps a no-engine build's loud fallback.
    let mut tool_runner = BuiltInToolRunner::new(db.clone(), host_self_inventory_env())
        .with_ask_carina(deps.ask_carina.clone());
    // P4.42: the in-chat `search_web` runs through the host's web-search provider
    // when one is wired (SERPER_API_KEY set); `None` leaves the not-configured
    // boundary, so the tool refuses exactly as it did before this lane.
    if let Some(web_search) = &deps.web_search {
        tool_runner = tool_runner.with_web_search_provider(std::sync::Arc::clone(web_search));
    }
    // Native tool-call detection (v4 `detectToolCallsInResponse` → the provider
    // plugin's `parseToolCalls`) is the real registry-backed detector (W4.7c):
    // reshape/parse both key off the provider manifest, so a native call is parsed
    // out of the raw response per the effective provider's wire format.
    let tool_detector = native_tool_loop::RegistryToolCallDetector::built_in();

    // ONE shared pending-wardrobe-announcement set for the whole turn (v4's single
    // `toolContext.pendingWardrobeAnnouncements`): the wardrobe handlers record
    // affected character ids here across the native loop + text passes, and the
    // end-of-turn drain (below) enqueues one Aurora announcement per character.
    let pending_wardrobe: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

    let loop_messages: Vec<ThreadedMessage> = formatted_messages
        .iter()
        .map(|m| ThreadedMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            name: m.name.clone(),
            thought_signature: m.thought_signature.clone(),
            reasoning_content: None,
            tool_call_id: None,
            tool_calls: None,
            cache_control: None,
            // The last user message's attachments survive into the loop slate
            // (P4.21 drop site 2 — v4's loop re-sends the same messages array,
            // image payload intact, on every tool-loop iteration).
            attachments: m.attachments.clone(),
        })
        .collect();
    // finding #22 carry-out: the memory slate the LLM saw this turn, threaded into
    // BOTH loops' tool contexts so `self_inventory` reports the real loaded memories
    // (v4 passes the same object to `createToolContext`).
    let turn_loaded_memories = loaded_memories_from_debug(
        &built_context.debug_memories,
        built_context.debug_inter_character_memories.as_deref(),
        built_context.debug_memory_recap.as_deref(),
    );
    let mut loop_tool_context = turn_tool_context(TurnToolContextArgs {
        chat_id: &chat_id,
        user_id: &user_id,
        character_id: &character_id,
        character_participant_id: &character_participant_id,
        image_profile_id: _image_profile_id.as_deref(),
        project_id: json_str(&chat, "projectId").as_deref(),
        loaded_memories: Some(&turn_loaded_memories),
    });
    loop_tool_context.pending_wardrobe_announcements = pending_wardrobe.clone();
    native_tool_loop::run_native_tool_loop(
        db,
        deps.streaming,
        sink,
        &tool_runner,
        &tool_detector,
        &mut preserve,
        native_tool_loop::RunNativeToolLoopOptions {
            chat_id: chat_id.clone(),
            character_id: character_id.clone(),
            character_name: character_name.clone(),
            agent_mode,
            provider: effective_profile.provider.clone(),
            base_url: effective_profile.base_url.clone(),
            formatted_messages: loop_messages,
            base_params: params.clone(),
            tool_context: loop_tool_context,
            state: &mut streaming_state,
            tool_messages: &mut tool_messages,
            generated_image_paths: &mut generated_image_paths,
        },
    )
    .await
    .map_err(|e| DbError::Key(format!("native tool loop failed: {}", e.message)))?;

    // --- Text-tool passes (orchestrator.service.ts:1282–1378, W4.1f) ---
    // After the native loop, v4 runs the provider-text-markers pass (gated on the
    // provider plugin implementing all three hooks → here the injected strategy
    // seam being present) then EITHER the simple-json pass (when `resolvedToolMode
    // === 'simple-json'`) OR the text-block fall-through (runs for ALL providers).
    // The pass is driven by the real `resolvedToolMode` (flag region): simple-json
    // when resolved, else the text-block fall-through (all providers). `actualTools`
    // is empty under any pseudo-tool surface, so `continuationTools` follows v4's
    // `useTextBlockTools ? [] : actualTools`. Runs the real [`BuiltInToolRunner`].
    let make_text_messages = || -> Vec<ThreadedMessage> {
        formatted_messages
            .iter()
            .map(|m| ThreadedMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                name: m.name.clone(),
                thought_signature: m.thought_signature.clone(),
                reasoning_content: None,
                tool_call_id: None,
                tool_calls: None,
                cache_control: None,
                // The text passes re-send the same slate — attachments intact
                // (P4.21; v4's continuation calls reuse `formattedMessages`).
                attachments: m.attachments.clone(),
            })
            .collect()
    };
    let make_text_context = || {
        // Same identifiers as the native loop's context above — v4 threads ONE
        // `toolContext` into both loops.
        let mut c = turn_tool_context(TurnToolContextArgs {
            chat_id: &chat_id,
            user_id: &user_id,
            character_id: &character_id,
            character_participant_id: &character_participant_id,
            image_profile_id: _image_profile_id.as_deref(),
            project_id: json_str(&chat, "projectId").as_deref(),
            loaded_memories: Some(&turn_loaded_memories),
        });
        c.pending_wardrobe_announcements = pending_wardrobe.clone();
        c
    };

    // Phase 19: provider-native text tool markers — v4 runs the pass only when the
    // active provider plugin implements the detector/parser/stripper trio
    // (`provider_has_text_markers`; W4.7c). Every built-in provider does, so the
    // pass runs — but no-ops when the streamed prose carries no markers.
    if crate::model::tool_wire::provider_has_text_markers(
        crate::provider_manifest::Registry::built_in(),
        &effective_profile.provider,
    ) {
        let provider_strategy = text_tool_loop::ProviderTextMarkersStrategy::built_in(
            effective_profile.provider.clone(),
        );
        text_tool_loop::run_text_tool_pass(
            db,
            deps.streaming,
            sink,
            &tool_runner,
            &provider_strategy,
            &mut preserve,
            RunTextToolPassOptions {
                chat_id: chat_id.clone(),
                character_id: character_id.clone(),
                character_name: character_name.clone(),
                provider: effective_profile.provider.clone(),
                base_url: effective_profile.base_url.clone(),
                formatted_messages: make_text_messages(),
                base_params: params.clone(),
                // The provider pass re-offers the regular tool slate (v4
                // `continuationTools: actualTools`).
                continuation_tools: actual_tools_value.clone(),
                continuation_use_native_web_search: use_native_web_search,
                tool_context: make_text_context(),
                state: &mut streaming_state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut generated_image_paths,
            },
        )
        .await
        .map_err(|e| DbError::Key(format!("provider text-tool pass failed: {}", e.message)))?;
    }

    // Phase 20: text-format tool calls — simple-json when the resolved mode calls
    // for it, else the text-block fall-through. Under text-block mode the
    // continuation suppresses native tools + web search so the model can't re-emit
    // the markers it just had stripped (v4).
    {
        let simple = SimpleJsonStrategy;
        let text_block = TextBlockStrategy;
        let strategy: &dyn TextToolStrategy = match resolved_tool_mode {
            ResolvedToolMode::SimpleJson => &simple,
            _ => &text_block,
        };
        let continuation_tools = if use_text_block_tools {
            Some(Value::Array(Vec::new()))
        } else {
            actual_tools_value.clone()
        };
        text_tool_loop::run_text_tool_pass(
            db,
            deps.streaming,
            sink,
            &tool_runner,
            strategy,
            &mut preserve,
            RunTextToolPassOptions {
                chat_id: chat_id.clone(),
                character_id: character_id.clone(),
                character_name: character_name.clone(),
                provider: effective_profile.provider.clone(),
                base_url: effective_profile.base_url.clone(),
                formatted_messages: make_text_messages(),
                base_params: params.clone(),
                continuation_tools,
                continuation_use_native_web_search: use_native_web_search && !use_text_block_tools,
                tool_context: make_text_context(),
                state: &mut streaming_state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut generated_image_paths,
            },
        )
        .await
        .map_err(|e| DbError::Key(format!("text-block tool pass failed: {}", e.message)))?;
    }

    let tool_messages_len = tool_messages.len();

    // --- Empty-response recovery (orchestrator.service.ts:1380–1397) ---
    // W4.2u: the danger flags + settings are now the real resolution (above).
    let recovery_flags = provider_failover::attempt_empty_response_recovery(
        deps.streaming,
        sink,
        deps.danger_router,
        AttemptEmptyResponseRecoveryOptions {
            state: &mut streaming_state,
            tool_messages_length: tool_messages_len,
            content_was_flagged_dangerous,
            danger_settings: DangerSettings {
                mode: danger_settings.mode.clone(),
                uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
            },
            connection_profile: effective_profile.clone(),
            params: params.clone(),
            user_id: user_id.clone(),
            chat_id: chat_id.clone(),
            character_id: character_id.clone(),
            character_name: character_name.clone(),
        },
    )
    .await;

    // --- End-of-turn wardrobe drain (orchestrator.service.ts:1406) ---
    // Collapse any wardrobe edits this turn's characters made into one Aurora
    // announcement each, regardless of which terminal branch we exit through. The
    // handlers recorded affected character ids into the shared set; drain it before
    // finalize (v4 fires one `WARDROBE_OUTFIT_ANNOUNCEMENT` job per character).
    crate::services::aurora_notifications::flush_pending_wardrobe_announcements(
        db,
        &user_id,
        &chat_id,
        &pending_wardrobe,
    )
    .await;

    // ========================================================================
    // "Nothing to add" turn-skipping — sentinel handling
    //     (orchestrator.service.ts:1454–1498)
    // ========================================================================
    // Detect the pass sentinel on the raw response before the finalizer. Only
    // when eligibility was computed for this turn (multi-character, LLM responder).
    if let Some(ts) = &turn_skip {
        match resolve_sentinel_action(
            crate::skip_signal::detect_skip_sentinel(
                &streaming_state.full_response,
                Some(&character_name),
                Some(character_aliases.as_slice()),
            ),
            ts.offer_skip,
            !tool_messages.is_empty(),
        ) {
            SentinelAction::ClearResponse => {
                streaming_state.full_response = String::new();
            }
            SentinelAction::HandleSkip => {
                return handle_turn_skip(
                    db,
                    sink,
                    HandleTurnSkipParams {
                        chat_id: &chat_id,
                        character_name: &character_name,
                        character_participant_id: &character_participant_id,
                        is_multi_character,
                        user_participant_id: user_participant_id.clone(),
                        usage: streaming_state.usage,
                        cache_usage: streaming_state.cache_usage,
                        attachment_results: streaming_state.attachment_results.clone(),
                        provider: effective_profile.provider.clone(),
                        model_name: effective_profile.model_name.clone(),
                    },
                )
                .await;
            }
            SentinelAction::ReplaceWithCleaned(cleaned) => {
                streaming_state.full_response = cleaned;
            }
            SentinelAction::LeaveAsIs => {}
        }
    }

    // --- Terminal branches (orchestrator.service.ts:1408–1515) ---
    let has_response = !crate::jsstr::js_trim(&streaming_state.full_response).is_empty();

    if has_response {
        // Finalize the successful response.
        let profile = FinalizerProfile {
            id: effective_profile.id.clone(),
            provider: effective_profile.provider.clone(),
            model_name: effective_profile.model_name.clone(),
        };
        let finalizer_chat = to_finalizer_chat(&chat);
        let finalizer_character = FinalizerCharacter {
            id: character_id.clone(),
            name: character_name.clone(),
            aliases: character_aliases.clone(),
        };
        let finalizer_participant = FinalizerParticipant {
            id: character_participant_id.clone(),
            status: participant_status.clone(),
        };
        let finalizer_streaming = FinalizerStreaming {
            full_response: streaming_state.full_response.clone(),
            usage: streaming_state.usage,
            cache_usage: streaming_state.cache_usage,
            attachment_results: streaming_state.attachment_results.clone(),
            raw_response: streaming_state.raw_response.clone(),
            thought_signature: streaming_state.thought_signature.clone(),
            reasoning_content: streaming_state.reasoning_content.clone(),
            reasoning_segments: streaming_state.reasoning_segments.clone(),
        };
        // Round-3 Group 8: the resolved cheap-LLM selection now flows into the
        // finalizer's async-compression trigger (v4 `compression.cheapLLMSelection`).
        // The gate ALSO requires `compression_enabled`, which the corpus keeps false
        // (`contextCompressionSettings.enabled === false`), so the trigger stays a
        // no-op here — but the plumbing matches v4 (the trigger is exercised end to
        // end in `message_finalizer_tier3_equivalence`).
        let finalizer_compression = FinalizerCompression {
            compression_enabled,
            cheap_llm_selection: cheap_llm_selection.clone(),
            original_system_prompt: built_context.original_system_prompt.clone(),
            visible_conversation: Vec::new(),
            content: input.options.content.clone(),
            is_continue_mode,
            window_size: 10,
            compression_target_tokens: 0,
            danger_settings: Some(crate::cheap_llm::DangerousContentSettings {
                mode: danger_settings.mode.clone(),
                uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
            }),
            available_profiles: available_cheap_profiles.clone(),
            now_ms: input.clock.now_ms,
        };
        let finalizer_participant_characters: Vec<ParticipantCharacter> = participant_characters
            .values()
            .map(to_participant_character)
            .collect();
        let finalizer_chat_settings = chat_settings.as_ref().map(|s| FinalizerChatSettings {
            cheap_llm_settings_present: s.cheap_llm_settings_present,
            auto_detect_rng: Some(s.auto_detect_rng),
            answer_confirmation_global_enabled: s.answer_confirmation_global_enabled,
            // W4.2u: the resolved danger mode (off-duty / exempt / global-OFF all
            // collapse to `"OFF"`) gates the danger-classification enqueue.
            danger_mode_off: danger_settings.mode == "OFF",
        });

        let result = message_finalizer::finalize_message_response(
            db,
            sink,
            FinalizeOptions {
                chat_id: chat_id.clone(),
                user_id: user_id.clone(),
                chat: finalizer_chat,
                character: finalizer_character,
                character_participant: finalizer_participant,
                user_participant_id: user_participant_id.clone(),
                is_multi_character,
                // The tool loops (W4.1e/f) now produce a real slate; v4 forwards
                // `toolMessages` + `generatedImagePaths` into the finalizer.
                generated_image_paths,
                tool_messages,
                pre_generated_assistant_message_id: Some(
                    pre_generated_assistant_message_id.clone(),
                ),
                profile,
                streaming: finalizer_streaming,
                compression: finalizer_compression,
                participant_characters: finalizer_participant_characters,
                chat_settings: finalizer_chat_settings,
                // W4.2u: v4 `isChatActiveDangerous(chat)` — the real resolution.
                is_dangerous_chat,
                // W4.2u: the ORIGINAL connection profile id (v4 `connectionProfile.id`).
                // v4's finalizer enqueues memory-extraction + danger-classification
                // with this, NOT the rerouted `effectiveProfile.id`.
                connection_profile_id: json_str(&connection_profile, "id").unwrap_or_default(),
                // W4.3: the answer-confirmation inputs. The orchestrator corpus keeps
                // the feature OFF (the gate is never active in `process_message`), so
                // the runner is never invoked here — the empty inputs are inert. The
                // real cheap-LLM selection / connection-profile / danger-settings
                // plumbing for the spine is a tracked deferral (the same seam boundary
                // as the compression `cheapLLMSelection` the orchestrator seams); the
                // active path is proven by `answer_confirmation_tier3_equivalence`,
                // which drives the finalizer directly with the feature ON.
                confirmation: message_finalizer::FinalizerConfirmationInputs::default(),
            },
            deps.confirmation,
            deps.compression,
            deps.rng_bytes,
            deps.cost,
            deps.carina_query,
            deps.prospero,
        )
        .await?;

        // --- Context-summary check (the finalizer's DEFERRED invocation, closed
        //     HERE — v4 fires it inside the finalizer's background triggers, but
        //     the finalizer deferred it to the orchestrator so this differential
        //     is the one that pins the summary model boundary alongside the
        //     stream). Gated: not-autonomous + cheapLLMSettings present. ---
        let is_autonomous = chat.get("chatType").and_then(Value::as_str) == Some("autonomous");
        if let Some(settings) = &chat_settings {
            if !is_autonomous && settings.cheap_llm_settings_present {
                // v4's summary check runs on `connectionProfile` (the ORIGINAL),
                // not the rerouted `effectiveProfile` (W4.2u). Equal when no reroute.
                let original_profile = to_effective_profile(&connection_profile);
                // finding #27: the cheap-LLM selection uses the REAL stored
                // `cheapLLMSettings` + ALL the user's connection profiles (v4
                // `memory-trigger.service.ts:104-135`), not a hard-coded config over
                // the responding profile alone. The parsed fields already ride the
                // orchestrator settings (Round-3 Group 8, the compression selection),
                // and `available_cheap_profiles` is the same `findByUserId` load; reuse
                // both.
                let cheap_settings = cheap_llm_settings_from_orchestrator(settings);
                // v4's `generateContextSummary` resolves the user's danger settings
                // internally (only consulted on an active-dangerous chat); the ported
                // service takes them injected. Pass the already-resolved effective
                // settings so a dangerous room does the uncensored swap on both sides.
                let summary_danger = crate::cheap_llm::DangerousContentSettings {
                    mode: danger_settings.mode.clone(),
                    uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
                };
                run_summary_check(
                    deps,
                    &chat_id,
                    &user_id,
                    &original_profile,
                    &cheap_settings,
                    &available_cheap_profiles,
                    Some(&summary_danger),
                )
                .await?;
            }
        }

        // Keep `existing_messages` referenced (it fed buildContext + the previous
        // response id scan; nothing after this reads it).
        existing_messages.clear();
        Ok(result)
    } else if !tool_messages.is_empty() {
        // Tool-only terminal (orchestrator.service.ts:1443–1479): no assistant
        // prose, but tools executed → persist the TOOL rows, bump the chat's
        // `updatedAt`, and emit the done frame with `toolsExecuted: true`.
        // Corpus-dormant until the tool loops (W4.1e/f) fill `tool_messages` (v4's
        // inline block cannot be isolated for an end-to-end drive without them).
        // Its constituents are each differential-proven: `save_tool_messages`
        // byte-exact vs v4 (`tool_execution_tier2` + the finalizer direct-drive),
        // the `chats.update(updatedAt)` (`chats_tier2`), the done frame
        // (`chat_events` self-tests).
        let whisper = ToolWhisperContext {
            user_participant_id: user_participant_id.clone(),
            allow_cross_character_vault_reads: chat
                .get("allowCrossCharacterVaultReads")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        let save_result = persist_tools_only(
            db,
            &chat_id,
            &user_id,
            &character_id,
            &character_participant_id,
            whisper,
            tool_messages,
            generated_image_paths,
        )
        .await?;

        sink.emit(ChatEvent::done(DonePayload {
            message_id: save_result.first_tool_message_id.clone(),
            participant_id: Some(character_participant_id.clone()),
            usage: streaming_state.usage.map(to_done_usage),
            cache_usage: streaming_state.cache_usage.map(to_done_cache_usage),
            attachment_results: Some(
                streaming_state
                    .attachment_results
                    .clone()
                    .unwrap_or(Value::Null),
            ),
            tools_executed: true,
            provider: Some(effective_profile.provider.clone()),
            model_name: Some(effective_profile.model_name.clone()),
            ..Default::default()
        }));

        Ok(ProcessMessageResult {
            is_multi_character,
            has_content: true,
            message_id: save_result.first_tool_message_id.unwrap_or_default(),
            user_participant_id,
            is_paused,
            scene_tracking_character_ids: None,
            skipped: false,
            skipped_participant_id: None,
        })
    } else {
        // Empty response (no tool messages in the corpus). Emit the done frame.
        let empty_reason = provider_failover::get_empty_response_reason(
            recovery_flags.uncensored_retry_attempted,
            recovery_flags.same_provider_retry_attempted,
            content_was_flagged_dangerous,
        );
        sink.emit(ChatEvent::done(DonePayload {
            message_id: None,
            participant_id: Some(character_participant_id.clone()),
            usage: streaming_state.usage.map(to_done_usage),
            cache_usage: streaming_state.cache_usage.map(to_done_cache_usage),
            attachment_results: Some(
                streaming_state
                    .attachment_results
                    .clone()
                    .unwrap_or(Value::Null),
            ),
            tools_executed: false,
            empty_response: Some(true),
            empty_response_reason: Some(empty_reason),
            provider: Some(effective_profile.provider.clone()),
            model_name: Some(effective_profile.model_name.clone()),
            ..Default::default()
        }));
        Ok(ProcessMessageResult {
            is_multi_character,
            has_content: false,
            message_id: String::new(),
            user_participant_id,
            is_paused,
            scene_tracking_character_ids: None,
            skipped: false,
            skipped_participant_id: None,
        })
    }
}

/// What the sentinel handling does with the streamed response (the decision half
/// of v4 orchestrator.service.ts:1454–1498, isolated so the tools-ran precedence
/// is unit-testable without a full spine drive).
#[derive(Clone, Debug, PartialEq, Eq)]
enum SentinelAction {
    /// Clear `fullResponse` to `''` — either tools ran this turn (the tool-save
    /// branch must win; v4 logs a warning) or the sentinel arrived without the
    /// offer (routes to the empty-response branch, nothing persisted).
    ClearResponse,
    /// A bare sentinel with the offer standing → the skip path.
    HandleSkip,
    /// Sentinel line + trailing prose → keep the prose as a real reply.
    ReplaceWithCleaned(String),
    /// No sentinel — the response proceeds untouched.
    LeaveAsIs,
}

/// The sentinel-handling precedence (v4 b90cd1f5): tools-ran beats the skip,
/// the skip needs the offer, prose survives its sentinel line.
fn resolve_sentinel_action(
    detection: crate::skip_signal::DetectSkipResult,
    offer_skip: bool,
    tools_ran: bool,
) -> SentinelAction {
    match detection {
        crate::skip_signal::DetectSkipResult::Skip => {
            if tools_ran {
                SentinelAction::ClearResponse
            } else if offer_skip {
                SentinelAction::HandleSkip
            } else {
                SentinelAction::ClearResponse
            }
        }
        crate::skip_signal::DetectSkipResult::NoSkip {
            cleaned: Some(cleaned),
        } => SentinelAction::ReplaceWithCleaned(cleaned),
        crate::skip_signal::DetectSkipResult::NoSkip { cleaned: None } => SentinelAction::LeaveAsIs,
    }
}

/// The inputs [`handle_turn_skip`] needs (v4 `handleTurnSkip`'s params, minus the
/// repos/controller/encoder plumbing the v5 `Db` + sink replace).
struct HandleTurnSkipParams<'a> {
    chat_id: &'a str,
    character_name: &'a str,
    character_participant_id: &'a str,
    is_multi_character: bool,
    user_participant_id: Option<String>,
    usage: Option<crate::model::stream::StreamUsage>,
    cache_usage: Option<crate::model::stream::StreamCacheUsage>,
    attachment_results: Option<Value>,
    provider: String,
    model_name: String,
}

/// "Nothing to add" turn-skipping — the skip path (v4 `handleTurnSkip`).
///
/// Posts the Host turn-pass announcement, records the passing participant in the
/// persisted cycle set (so the rotation advances past them), surfaces the Host
/// bubble live (the `hostAnnouncement` frame), and emits a `done` event flagged
/// `skipped: true` so the client resets its streaming buffer without appending a
/// phantom bubble. No character reply is persisted.
async fn handle_turn_skip(
    db: &Db,
    sink: &(impl EventSink + Sync),
    params: HandleTurnSkipParams<'_>,
) -> Result<ProcessMessageResult, DbError> {
    // Post the Host announcement (errors swallowed by the writer contract).
    let host_message = crate::services::host_notifications::post_host_turn_pass_announcement(
        db,
        crate::services::host_notifications::HostTurnPassAnnouncement {
            chat_id: params.chat_id.to_string(),
            character_name: params.character_name.to_string(),
            participant_id: params.character_participant_id.to_string(),
            source: crate::services::host_notifications::TurnPassSource::Llm,
        },
    )
    .await;

    // Record the pass in the persisted cycle so the next speaker selection
    // advances past this character. Re-read for fresh participants/cycle state.
    // v4 wraps the whole re-read/update in a swallow-and-warn try/catch; the
    // update mints `updatedAt` UNCONDITIONALLY (unusual for this repo —
    // faithful: v4 always includes `updatedAt: new Date().toISOString()`).
    let mut is_paused = false;
    let chat_id_owned = params.chat_id.to_string();
    if let Ok(Some(fresh_chat)) = db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned)) {
        is_paused = fresh_chat.get("isPaused").and_then(Value::as_bool) == Some(true);
        let ts_participants: Vec<crate::turn_state::ParticipantView> =
            turn_orchestrator::participants_array(&fresh_chat)
                .iter()
                .map(turn_orchestrator::to_turnstate_participant)
                .collect();
        let spoken_json = fresh_chat
            .get("spokenThisCycleParticipantIds")
            .and_then(Value::as_str);
        let cycle_update = crate::turn_state::compute_spoken_this_cycle_after_skip(
            params.character_participant_id,
            &ts_participants,
            spoken_json,
        );
        let chat_id_owned = params.chat_id.to_string();
        let now = crate::clock::now_iso();
        let _ = db
            .write(move |w| {
                w.main()
                    .chats()
                    .update(
                        &chat_id_owned,
                        &crate::db::chats::ChatUpdate {
                            updated_at: Some(now),
                            spoken_this_cycle_participant_ids: cycle_update,
                            ..Default::default()
                        },
                    )
                    .map(|_| ())
            })
            .await;
    }

    // Surface the Host bubble live, then close the turn with the skipped flag.
    if let Some(posted) = &host_message {
        sink.emit(ChatEvent::host_announcement(posted.message.clone()));
    }
    sink.emit(ChatEvent::done(DonePayload {
        message_id: None,
        participant_id: Some(params.character_participant_id.to_string()),
        usage: params.usage.map(to_done_usage),
        cache_usage: params.cache_usage.map(to_done_cache_usage),
        attachment_results: Some(params.attachment_results.unwrap_or(Value::Null)),
        tools_executed: false,
        skipped: Some(true),
        skipped_participant_id: Some(params.character_participant_id.to_string()),
        provider: Some(params.provider),
        model_name: Some(params.model_name),
        ..Default::default()
    }));

    Ok(ProcessMessageResult {
        is_multi_character: params.is_multi_character,
        has_content: false,
        message_id: String::new(),
        user_participant_id: params.user_participant_id,
        is_paused,
        scene_tracking_character_ids: None,
        skipped: true,
        skipped_participant_id: Some(params.character_participant_id.to_string()),
    })
}

/// The tool-only terminal's DB effect (orchestrator.service.ts:1449–1460):
/// `saveToolMessages` (the TOOL rows + their image link/tag) then an explicit
/// `repos.chats.update(chatId, { updatedAt: now })` — a second `updatedAt` bump on
/// top of the per-message metadata side-effect. Isolated for clarity; its pieces
/// are differential-proven (see the branch comment) since `process_message` cannot
/// reach the branch until the tool loops land.
#[allow(clippy::too_many_arguments)]
async fn persist_tools_only(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    character_id: &str,
    participant_id: &str,
    whisper: ToolWhisperContext,
    tool_messages: Vec<ToolMessage>,
    generated_image_paths: Vec<GeneratedImage>,
) -> Result<SaveToolMessagesResult, DbError> {
    let chat_id = chat_id.to_string();
    let user_id = user_id.to_string();
    let character_id = character_id.to_string();
    let participant_id = participant_id.to_string();
    db.write(move |writers| {
        let w = writers.main();
        let result = save_tool_messages(
            w,
            &chat_id,
            &user_id,
            &tool_messages,
            &generated_image_paths,
            Some(&character_id),
            Some(&participant_id),
            Some(&whisper),
        )?;
        // v4: `await repos.chats.update(chatId, { updatedAt: new Date()... })`.
        let update = crate::db::chats::ChatUpdate {
            updated_at: Some(crate::clock::now_iso()),
            ..Default::default()
        };
        w.chats().update(&chat_id, &update)?;
        Ok(result)
    })
    .await
}

/// Build the summary service's [`context_summary::CheapLlmSettings`] from the
/// orchestrator's already-parsed chat settings. The `OrchestratorChatSettings`
/// fields are populated by the spine's settings reader (`PROVIDER_CHEAPEST` when a
/// stored object is absent); an empty `strategy` collapses to the same default the
/// compression selection uses above, so both cheap-LLM paths select identically for
/// the same stored settings — exactly as v4's single `chatSettings.cheapLLMSettings`
/// does.
fn cheap_llm_settings_from_orchestrator(
    settings: &OrchestratorChatSettings,
) -> crate::services::context_summary::CheapLlmSettings {
    crate::services::context_summary::CheapLlmSettings {
        strategy: if settings.cheap_llm_strategy.is_empty() {
            "PROVIDER_CHEAPEST".to_string()
        } else {
            settings.cheap_llm_strategy.clone()
        },
        user_defined_profile_id: settings.cheap_llm_user_defined_profile_id.clone(),
        default_cheap_profile_id: settings.cheap_llm_default_cheap_profile_id.clone(),
        fallback_to_local: settings.cheap_llm_fallback_to_local,
    }
}

/// Run the context-summary check (the finalizer's deferred invocation). v4's
/// `triggerContextSummaryCheck` (`memory-trigger.service.ts:104-135`) hands the
/// REAL stored `cheapLLMSettings` + ALL the user's connection profiles (a
/// `repos.connections.findByUserId`) to `checkAndGenerateSummaryIfNeeded`, so the
/// cheap-LLM selection can honour a configured `defaultCheapProfileId` /
/// `USER_DEFINED` / `isCheap` profile — the fix for dogfood finding #27, where the
/// summary previously always fell to `getCheapestModel(responding.provider)`. The
/// caller resolves `cheap_settings` / `available_profiles` / `danger` above the seam
/// (from the same reads the compression selection already performs). When the gate
/// fires a fold, `check_and_generate_summary_if_needed` writes the summary + the
/// title enqueue through the `Db` — the effect the differential banks.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
async fn run_summary_check<EMB, CMP, STR, SNK, BCS, ORC, RTR, CONF, ACOMP, COST, CARQ, PROS, PF>(
    deps: &OrchestratorDeps<
        '_,
        EMB,
        CMP,
        STR,
        SNK,
        BCS,
        ORC,
        RTR,
        CONF,
        ACOMP,
        COST,
        CARQ,
        PROS,
        PF,
    >,
    chat_id: &str,
    user_id: &str,
    profile: &EffectiveProfile,
    cheap_settings: &crate::services::context_summary::CheapLlmSettings,
    available_profiles: &[crate::cheap_llm::CheapLlmProfile],
    danger: Option<&crate::cheap_llm::DangerousContentSettings>,
) -> Result<(), DbError>
where
    EMB: EmbeddingProvider + Sync,
    CMP: CompletionProvider + Sync,
    STR: StreamingCompletionProvider,
    SNK: EventSink + Sync,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    PF: PricingFetch + Send + Sync,
{
    // The `current` connection profile is the responding profile (v4 passes
    // `connectionProfile` as the selection's basis); the SELECTION then runs over
    // `cheap_settings` + `available_profiles`, so a configured cheap profile wins.
    let cheap_profile = crate::cheap_llm::CheapLlmProfile {
        id: profile.id.clone(),
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        base_url: profile.base_url.clone(),
        is_cheap: false,
        is_dangerous_compatible: false,
        // ⚠ Pre-existing simplification, unchanged by P4.D79: this basis is
        // built from the 4-field [`EffectiveProfile`], so the profile's
        // `parameters` / `maxContext` / `maxTokens` are not available here. It
        // only matters on `getCheapLLMProvider`'s LAST fallback arm (where v4
        // would return `selectionFromProfile(currentProfile)` carrying the
        // profile's own parameters); every configured path selects from
        // `available_profiles`, which do carry them. Banked, not widened here —
        // see the P4.D79 lane record.
        parameters: None,
        max_tokens: None,
        max_context: None,
        model_class: None,
    };
    // The fold-time episode pass runs LIVE here (v4's in-loop check calls the
    // real `generateContextSummary`, `runFoldEpisodePass` included — the
    // orchestrator differential pins its cheap-LLM call). The other seam arms
    // stay no-ops per the oracle's mock set (see [`FoldEpisodePassSeams`]).
    let seams = super::context_summary::FoldEpisodePassSeams {
        db: deps.db,
        embedding: deps.embedding,
        completion: deps.completion,
        executor: deps.executor,
    };
    super::context_summary::check_and_generate_summary_if_needed_with_seams(
        deps.db,
        deps.completion,
        deps.executor,
        chat_id,
        &cheap_profile,
        cheap_settings,
        available_profiles,
        user_id,
        danger,
        None,
        true,
        &seams,
    )
    .await?;
    Ok(())
}

// ===========================================================================
// executeTurnChain
// ===========================================================================

/// The result the chain driver reads from a `process_message` re-entry — v4's
/// `ProcessMessageResult` (re-exported for clarity).
pub use crate::services::message_finalizer::ProcessMessageResult as ChainResult;

/// Options for [`execute_turn_chain`] (v4 `ExecuteTurnChainOptions`). The
/// `process_chained_message` closure is the re-entry into `process_message`.
pub struct ExecuteTurnChainOptions {
    pub chat_id: String,
    /// The initial `processMessage` result (whether to chain at all is decided
    /// from it).
    pub initial_result: ProcessMessageResult,
    pub initial_continue_mode: bool,
    /// Autonomous-room flag (bypasses the all-LLM pause).
    pub never_pause_for_user: bool,
    /// Autonomous-room flag (skip the chain loop — the caller enqueues each turn).
    pub single_turn: bool,
    /// The chain-start wall clock (v4 `Date.now()`), for the time guard.
    pub chain_start_time_ms: i64,
    /// The chain config (depth / time guards).
    pub config: ChainConfig,
}

/// v4 `executeTurnChain`. Continues multi-character turn chains inside one stream.
/// Composes the ported decision core (`should_chain_next` plus
/// `persist_turn_participant_id`) with the per-turn re-entry into
/// [`process_message`], emitting the `turnStart` / `turnComplete` / `chainComplete`
/// frames.
///
/// `make_chain_input(participant_id, now_ms) -> ProcessMessageInput` — v4's
/// `processChainedMessage` builder (a continue-mode re-entry with the resolved
/// next speaker); it returns an OWNED input so no borrow of `deps` escapes across
/// the re-entry (the chain calls `process_message(deps, &input)` internally). The
/// injected `now_ms` / `random01` (from `opts`) drive each step's time guard +
/// selection; the corpus freezes both.
#[allow(clippy::type_complexity)]
pub async fn execute_turn_chain<
    EMB,
    CMP,
    STR,
    SNK,
    BCS,
    ORC,
    RTR,
    CONF,
    ACOMP,
    COST,
    CARQ,
    PROS,
    PF,
    F,
>(
    deps: &mut OrchestratorDeps<
        '_,
        EMB,
        CMP,
        STR,
        SNK,
        BCS,
        ORC,
        RTR,
        CONF,
        ACOMP,
        COST,
        CARQ,
        PROS,
        PF,
    >,
    opts: ExecuteTurnChainOptions,
    now_ms: i64,
    random01: f64,
    mut make_chain_input: F,
) -> Result<(), DbError>
where
    EMB: EmbeddingProvider + Sync,
    CMP: CompletionProvider + Sync,
    STR: StreamingCompletionProvider,
    SNK: EventSink + Sync,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    PF: PricingFetch + Send + Sync,
    F: FnMut(String) -> ProcessMessageInput,
{
    let db = deps.db;
    let initial = &opts.initial_result;
    // v4: only chain for a multi-character chat that produced content and isn't
    // paused. A skipped initial turn ("nothing to add") is NOT terminal — it
    // advanced the rotation via a Host turn-pass record, so the chain must
    // continue to the next speaker. Only a genuinely empty (no content, no skip)
    // initial turn stops here.
    if !initial.is_multi_character
        || (!initial.has_content && !initial.skipped)
        || initial.is_paused
    {
        return Ok(());
    }
    // Single-turn callers enqueue the next turn themselves.
    if opts.single_turn {
        return Ok(());
    }

    let mut chain_depth: i64 = 0;
    // v4: userParticipantId ?? (continueMode ? null : '__user__').
    let effective_user_participant_id: Option<String> =
        initial.user_participant_id.clone().or_else(|| {
            if opts.initial_continue_mode {
                None
            } else {
                Some("__user__".to_string())
            }
        });
    let guards = ChainGuards {
        never_pause_for_user: opts.never_pause_for_user,
    };

    loop {
        let decision: ChainDecision = turn_orchestrator::should_chain_next(
            db,
            &opts.chat_id,
            effective_user_participant_id.as_deref(),
            chain_depth,
            now_ms,
            opts.chain_start_time_ms,
            &opts.config,
            guards,
            random01,
        )
        .await?;

        if !decision.chain || decision.participant_id.is_none() {
            let final_next_speaker = decision.participant_id.clone();
            // v4 persists the final next speaker (best-effort — errors logged).
            let _ = turn_orchestrator::persist_turn_participant_id(
                db,
                &opts.chat_id,
                final_next_speaker.as_deref(),
            )
            .await;
            deps.sink
                .emit(ChatEvent::chain_complete(ChainCompletePayload {
                    reason: chain_reason_str(decision.reason).to_string(),
                    next_speaker_id: final_next_speaker,
                    chain_depth,
                }));
            break;
        }

        chain_depth += 1;
        let participant_id = decision.participant_id.clone().unwrap();
        deps.sink.emit(ChatEvent::turn_start(TurnStartPayload {
            participant_id: participant_id.clone(),
            character_name: decision
                .character_name
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            chain_depth,
        }));

        // Re-enter processMessage for the chained turn (v4 `processChainedMessage`).
        // The decision's selection reason rides into the chained options (v4
        // `chainSelectionReason: decision.selectionReason`) so a queue-popped
        // (summoned) turn withholds the skip offer.
        let mut input = make_chain_input(participant_id.clone());
        input.options.chain_selection_reason = decision.selection_reason;
        let chain_step = process_message(deps, &input).await;
        match chain_step {
            Ok(chain_result) => {
                deps.sink
                    .emit(ChatEvent::turn_complete(TurnCompletePayload {
                        participant_id: participant_id.clone(),
                        message_id: chain_result.message_id.clone(),
                        chain_depth,
                        // v4 passes `chainResult.skipped === true` (always present
                        // on chained turns).
                        skipped: chain_result.skipped,
                    }));
                // A skipped turn advanced the rotation (Host turn-pass record) —
                // fall through to the next decideNextTurn iteration. Only a
                // genuinely empty response (no content, no skip) stops the chain.
                if !chain_result.has_content && !chain_result.skipped {
                    let _ = turn_orchestrator::persist_turn_participant_id(db, &opts.chat_id, None)
                        .await;
                    deps.sink
                        .emit(ChatEvent::chain_complete(ChainCompletePayload {
                            reason: "error".to_string(),
                            next_speaker_id: None,
                            chain_depth,
                        }));
                    break;
                }
            }
            Err(_chain_error) => {
                // v4: pause the chat + persist null + chainComplete{error}.
                let chat_id_owned = opts.chat_id.clone();
                db.write(move |w| {
                    w.main().chats().update(
                        &chat_id_owned,
                        &crate::db::chats::ChatUpdate {
                            is_paused: Some(true),
                            ..Default::default()
                        },
                    )
                })
                .await?;
                let _ =
                    turn_orchestrator::persist_turn_participant_id(db, &opts.chat_id, None).await;
                deps.sink
                    .emit(ChatEvent::chain_complete(ChainCompletePayload {
                        reason: "error".to_string(),
                        next_speaker_id: None,
                        chain_depth,
                    }));
                break;
            }
        }
    }
    Ok(())
}

/// The v4 chain-stop reason string for the `chainComplete` frame.
fn chain_reason_str(r: ChainReason) -> &'static str {
    r.as_str()
}

// ===========================================================================
// buildContext input assembly
// ===========================================================================

pub(crate) struct BuildContextArgs<'a> {
    /// The user id (v4 `input.user_id`).
    pub user_id: &'a str,
    /// The model's context-window limit (v4 `input.model_context_limit`).
    pub model_context_limit: i64,
    /// The optional timestamp config (v4 `input.timestamp_config`).
    pub timestamp_config: Option<crate::chat_timestamp::TimestampConfig>,
    /// The resolved IANA timezone (v4 `input.timezone`).
    pub timezone: Option<String>,
    /// The SERVER-LOCAL IANA zone (v4's ambient process zone) — see
    /// [`ProcessMessageInput::server_tz`]; NOT the story zone above.
    pub server_tz: Option<String>,
    /// `continueMode === true` (v4 `input.options.continue_mode`).
    pub is_continue_mode: bool,
    /// The wall-clock base (v4 `input.clock.now_ms`).
    pub now_ms: i64,
    /// The local UTC offset in minutes (v4 `input.clock.local_offset_minutes`).
    pub local_offset_minutes: i64,
    pub chat: &'a Value,
    pub character: &'a Value,
    pub character_participant: &'a Value,
    pub connection_profile: &'a build_context::ConnectionProfileInput,
    pub user_character: Option<crate::system_prompt::UserCharacter>,
    pub roleplay_template: Option<String>,
    pub is_multi_character: bool,
    pub participant_characters: &'a HashMap<String, Value>,
    pub existing_messages: &'a [Value],
    pub final_user_message: Option<String>,
    pub speaking_as: Option<String>,
    pub tool_instructions: Option<String>,
    pub compression_enabled: bool,
    pub bypass_compression: bool,
    /// The async pre-compression cache result (v4 `cachedCompressionResponse?.result`),
    /// computed by [`super::compression_cache::get_cached_compression`] before
    /// buildContext (W4.4a4).
    pub cached_compression_result: Option<crate::services::compression::ContextCompressionResult>,
    /// The cache's visible-message count (v4 `cachedCompressionResponse?.cachedMessageCount`).
    pub cached_compression_message_count: Option<i64>,
    /// The resolved cheap-LLM selection (v4 `cheapLLMSelection`; Round-3 Group 8).
    /// Threaded into buildContext so the recap/distill feeders + the cached-
    /// compression window activate. `None` only when no selection could be resolved.
    pub cheap_llm_selection: Option<CheapLlmSelection>,
    /// The uncensored memory-recap fallback (v4 `uncensoredFallbackOptions`) — set
    /// only for an actively-dangerous chat with a resolved selection.
    pub uncensored_fallback: Option<build_context::OwnedUncensoredFallback>,
    /// The autonomous-room per-turn context cap (v4 `options.autonomousContextCap`,
    /// U4.4): clamps the model-derived `maxAvailable` down to this turn's slice of
    /// the per-run token budget. `None` (every non-autonomous caller) leaves the
    /// budget untouched.
    pub autonomous_context_cap: Option<i64>,
    /// The tokens this turn will add to the payload after the context is built
    /// (v4 `options.reservedOutgoingTokens`, `f933ba9c`) — the tool schemas plus
    /// the system messages the orchestrator splices in. `None` leaves the message
    /// budget untouched.
    pub reserved_outgoing_tokens: Option<i64>,
    /// "Nothing to add" turn-skipping — the per-turn ephemeral instruction
    /// control (v4 `turnSkip`, b90cd1f5). `None` for the sibling entry points
    /// (regenerate-swipe) that never offer the pass.
    pub turn_skip: Option<build_context::TurnSkip>,
    /// The proactive pre-compute distill's semantic-search results (v4
    /// `preSearchedMemories`, P4.19). When non-empty, buildContext takes the
    /// `'pre-searched'` memory path and skips its own fallback distill. `None` for
    /// the sibling entry points (regenerate-swipe) that don't run the proactive task.
    pub pre_searched_memories: Option<Vec<crate::services::memory_service::SemanticSearchResult>>,
    /// The proactive pre-compute distill's turn-level recall signals (v4
    /// `recallSignals`, P4.19), seeding the retrospective cadence on both memory
    /// paths. `None` for the sibling entry points.
    pub recall_signals: Option<crate::services::memory_recap::distill::DistilledSearch>,
}

/// Convert a connection-profile net-read `Value` into a [`CheapLlmProfile`] (v4's
/// `ConnectionProfile` → `CheapLLMProfile` shape). Used to resolve the cheap-LLM
/// selection (Round-3 Group 8).
pub(crate) fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
    CheapLlmProfile {
        id: json_str(v, "id").unwrap_or_default(),
        provider: json_str(v, "provider").unwrap_or_default(),
        model_name: json_str(v, "modelName").unwrap_or_default(),
        base_url: json_str(v, "baseUrl"),
        is_cheap: v.get("isCheap").and_then(Value::as_bool) == Some(true),
        is_dangerous_compatible: v.get("isDangerousCompatible").and_then(Value::as_bool)
            == Some(true),
        parameters: v.get("parameters").cloned(),
        max_tokens: json_f64(v, "maxTokens"),
        max_context: json_f64(v, "maxContext"),
        model_class: json_str(v, "modelClass"),
    }
}

/// Assemble the [`BuildContextInput`] from the resolved orchestrator state (v4's
/// inline `buildMessageContext` argument object). Single-character corpus:
/// participant/multi-character fields are `None`, `skip_memories` false,
/// `min_memory_importance` 0.
pub(crate) fn build_context_input(args: BuildContextArgs<'_>) -> BuildContextInput {
    let character = to_context_character(args.character);

    let chat = build_context::ContextChat {
        id: json_str(args.chat, "id").unwrap_or_default(),
        project_id: json_str(args.chat, "projectId"),
        scenario_text: json_str(args.chat, "scenarioText"),
        context_summary: json_str(args.chat, "contextSummary"),
        compaction_generation: json_f64(args.chat, "compactionGeneration").unwrap_or(0.0) as i64,
        summary_anchor_message_ids: json_str_array(args.chat, "summaryAnchorMessageIds"),
        commonplace_recall_history: args
            .chat
            .get("commonplaceRecallHistory")
            .cloned()
            .unwrap_or(Value::Null),
        commonplace_scene_cache: args.chat.get("commonplaceSceneCache").cloned(),
        scene_state: args.chat.get("sceneState").cloned(),
        precompiled_identity_stack: None,
        timeline_mode: json_str(args.chat, "timelineMode"),
        impersonating_participant_ids: json_str_array(args.chat, "impersonatingParticipantIds"),
    };

    // v4 `buildMessageContext` passes `respondingParticipant` in BOTH single- and
    // multi-character chats (Phase H: the system-prompt compiler cache can hit on
    // single-char chats too), but the participant list / character map /
    // attribution messages ONLY when multi-character.
    let responding_participant = Some(build_context::RespondingParticipant {
        id: json_str(args.character_participant, "id").unwrap_or_default(),
        selected_system_prompt_id: json_str(args.character_participant, "selectedSystemPromptId"),
    });
    // The all-participants list + per-character map are message-independent (they
    // feed buildContext's attribution/system-prompt). The `existing_messages` /
    // `messages_with_participants` / `is_initial_message` / `generate_memory_recap`
    // fields are placeholders here: `message_context::build_message_context` fills
    // them from the whisper-filtered conversation (v4 runs the pre-filters +
    // `buildConversationMessages` inside the wrapper, above the buildContext call).
    let (_rp_unused, all_participants, participant_characters, _mwp_unused) =
        if args.is_multi_character {
            multi_character_fields(
                args.chat,
                args.character_participant,
                args.character,
                args.participant_characters,
                args.existing_messages,
            )
        } else {
            (None, None, None, None)
        };
    let _ = (_rp_unused, _mwp_unused);

    BuildContextInput {
        model_context_limit: args.model_context_limit,
        reserved_outgoing_tokens: args.reserved_outgoing_tokens,
        user_id: args.user_id.to_string(),
        character,
        user_character: args.user_character,
        chat,
        existing_messages: Vec::new(),
        new_user_message: args.final_user_message,
        active_user_participant_id: args.speaking_as,
        roleplay_template: args.roleplay_template.clone(),
        embedding_profile_id: None,
        skip_memories: false,
        // v4 `buildMessageContext` passes `minMemoryImportance: 0.5`.
        min_memory_importance: 0.5,
        responding_participant,
        all_participants,
        participant_characters,
        messages_with_participants: None,
        tool_instructions: args.tool_instructions,
        timestamp_config: args.timestamp_config.clone(),
        is_initial_message: false,
        timezone: args.timezone.clone(),
        server_tz: args.server_tz.clone(),
        connection_profile: Some(args.connection_profile.clone()),
        context_compression_settings: if args.compression_enabled {
            Some(build_context::ContextCompressionSettingsInput {
                enabled: true,
                window_size: 10,
                compression_threshold_ratio: 0.8,
                system_prompt_target_tokens: 1500,
            })
        } else {
            None
        },
        cheap_llm_selection: args.cheap_llm_selection.clone(),
        bypass_compression: args.bypass_compression,
        cached_compression_result: args.cached_compression_result,
        cached_compression_message_count: args.cached_compression_message_count,
        generate_memory_recap: false,
        uncensored_fallback: args.uncensored_fallback.clone(),
        is_continue_mode: args.is_continue_mode,
        now_ms: args.now_ms,
        local_offset_minutes: args.local_offset_minutes,
        minutes_since_last_timestamp_announcement: None,
        autonomous_context_cap: args.autonomous_context_cap,
        turn_skip: args.turn_skip,
        pre_searched_memories: args.pre_searched_memories,
        recall_signals: args.recall_signals,
    }
}

/// The four optional multi-character fields buildContext reads (v4 sets them only
/// for a multi-character chat).
type MultiCharacterFields = (
    Option<build_context::RespondingParticipant>,
    Option<Vec<build_context::FullParticipant>>,
    Option<HashMap<String, build_context::ContextCharacter>>,
    Option<Vec<build_context::MessageWithParticipant>>,
);

/// Assemble the multi-character buildContext fields (v4 sets these for a
/// multi-character chat): the responding participant, the full participant list,
/// the per-character map (INCLUDING the responder — the map keys every character
/// the attribution needs), and the attribution messages
/// (`messagesWithParticipants`, each `type:'message'` event with its
/// participant/target info).
fn multi_character_fields(
    chat: &Value,
    character_participant: &Value,
    responder_character: &Value,
    participant_characters: &HashMap<String, Value>,
    existing_messages: &[Value],
) -> MultiCharacterFields {
    let responding = build_context::RespondingParticipant {
        id: json_str(character_participant, "id").unwrap_or_default(),
        // v4 reads `characterParticipant.selectedSystemPromptId`.
        selected_system_prompt_id: json_str(character_participant, "selectedSystemPromptId"),
    };
    let participants: Vec<build_context::FullParticipant> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(to_full_participant).collect())
        .unwrap_or_default();

    // The per-character map: every OTHER participant character (from
    // `participant_characters`) PLUS the responder itself (v4's map is keyed by
    // characterId and includes the responder's own overlaid row).
    let mut chars: HashMap<String, build_context::ContextCharacter> = participant_characters
        .iter()
        .map(|(k, v)| (k.clone(), to_context_character(v)))
        .collect();
    if let Some(rid) = json_str(responder_character, "id") {
        chars
            .entry(rid)
            .or_insert_with(|| to_context_character(responder_character));
    }

    // The attribution messages: every `type:'message'` event with the fields the
    // per-character attribution reads.
    let mwp: Vec<build_context::MessageWithParticipant> = existing_messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .map(|m| build_context::MessageWithParticipant {
            id: json_str(m, "id"),
            role: json_str(m, "role").unwrap_or_default(),
            content: json_str(m, "content").unwrap_or_default(),
            participant_id: json_str(m, "participantId"),
            thought_signature: json_str(m, "thoughtSignature"),
            created_at: json_str(m, "createdAt"),
            target_participant_ids: m.get("targetParticipantIds").and_then(Value::as_array).map(
                |a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                },
            ),
            host_event: None,
        })
        .collect();

    (Some(responding), Some(participants), Some(chars), Some(mwp))
}

fn to_full_participant(p: &Value) -> build_context::FullParticipant {
    build_context::FullParticipant {
        id: json_str(p, "id").unwrap_or_default(),
        participant_type: json_str(p, "type").unwrap_or_default(),
        character_id: json_str(p, "characterId").filter(|c| !c.is_empty()),
        controlled_by: json_str(p, "controlledBy").unwrap_or_else(|| "llm".into()),
        status: json_str(p, "status").unwrap_or_else(|| "active".into()),
        created_at: json_str(p, "createdAt"),
        has_history_access: p
            .get("hasHistoryAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

// ===========================================================================
// Marshaling helpers.
// ===========================================================================

pub(crate) fn json_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}
pub(crate) fn json_f64(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
pub(crate) fn json_str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn to_effective_profile(profile: &Value) -> EffectiveProfile {
    EffectiveProfile {
        id: json_str(profile, "id").unwrap_or_default(),
        provider: json_str(profile, "provider").unwrap_or_default(),
        model_name: json_str(profile, "modelName").unwrap_or_default(),
        base_url: json_str(profile, "baseUrl"),
    }
}

pub(crate) fn effective_profile_profile(profile: &Value) -> build_context::ConnectionProfileInput {
    build_context::ConnectionProfileInput {
        max_context: json_f64(profile, "maxContext").map(|f| f as i64),
        max_tokens: json_f64(profile, "maxTokens").map(|f| f as i64),
        model_class: json_str(profile, "modelClass"),
    }
}

fn to_context_character(c: &Value) -> build_context::ContextCharacter {
    build_context::ContextCharacter {
        id: json_str(c, "id").unwrap_or_default(),
        name: json_str(c, "name").unwrap_or_default(),
        character_document_mount_point_id: json_str(c, "characterDocumentMountPointId"),
        sys: to_sys_char(c),
    }
}

/// Build the [`crate::system_prompt::Character`] subset the prompt builder reads
/// off the vault-overlaid character row. The corpus characters carry no pronouns
/// / scenarios / systemPrompts, so those typed fields default (the same subset
/// the build_context differential builds).
fn to_sys_char(c: &Value) -> crate::system_prompt::Character {
    crate::system_prompt::Character {
        name: json_str(c, "name").unwrap_or_default(),
        title: json_str(c, "title"),
        identity: json_str(c, "identity"),
        description: json_str(c, "description"),
        manifesto: json_str(c, "manifesto"),
        personality: json_str(c, "personality"),
        aliases: json_str_array(c, "aliases"),
        ..Default::default()
    }
}

fn to_finalizer_chat(chat: &Value) -> FinalizerChat {
    FinalizerChat {
        id: json_str(chat, "id").unwrap_or_default(),
        is_paused: chat
            .get("isPaused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        chat_type: json_str(chat, "chatType"),
        project_id: json_str(chat, "projectId"),
        answer_confirmation_override: json_str(chat, "answerConfirmationOverride"),
        // W4.3: the project override is a host-side `repos.projects.findById` read,
        // resolved above the seam. The orchestrator corpus keeps the feature OFF
        // (the gate is never active), so the project override is never consulted;
        // the answer-confirmation differential drives the finalizer directly and
        // supplies this when a project override would flip the gate ON.
        answer_confirmation_project_override: None,
        impersonating_participant_ids: json_str_array(chat, "impersonatingParticipantIds"),
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        spoken_this_cycle_participant_ids: json_str(chat, "spokenThisCycleParticipantIds"),
        allow_cross_character_vault_reads: chat
            .get("allowCrossCharacterVaultReads")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn to_participant_character(c: &Value) -> ParticipantCharacter {
    ParticipantCharacter {
        id: json_str(c, "id").unwrap_or_default(),
        name: json_str(c, "name").unwrap_or_default(),
        aliases: json_str_array(c, "aliases"),
    }
}

/// Read the participant characters map (v4 `loadAllParticipantData`) — every
/// active CHARACTER participant's vault-overlaid character except the responder.
pub(crate) fn load_participant_characters(
    db: &Db,
    chat: &Value,
    responder_id: &str,
) -> Result<HashMap<String, Value>, DbError> {
    let mut map = HashMap::new();
    if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
        for p in participants {
            if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                continue;
            }
            let Some(cid) = p
                .get("characterId")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
            else {
                continue;
            };
            if cid == responder_id {
                continue;
            }
            let cid_owned = cid.to_string();
            if let Some(ch) = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &cid_owned)
                })
            })? {
                map.insert(cid.to_string(), ch);
            }
        }
    }
    Ok(map)
}

fn to_done_usage(u: crate::model::stream::StreamUsage) -> crate::services::chat_events::DoneUsage {
    crate::services::chat_events::DoneUsage {
        prompt_tokens: Some(u.prompt_tokens),
        completion_tokens: Some(u.completion_tokens),
        total_tokens: Some(u.total_tokens),
    }
}
fn to_done_cache_usage(
    c: crate::model::stream::StreamCacheUsage,
) -> crate::services::chat_events::DoneCacheUsage {
    crate::services::chat_events::DoneCacheUsage {
        cache_creation_input_tokens: c.cache_creation_input_tokens,
        cache_read_input_tokens: c.cache_read_input_tokens,
    }
}

pub(crate) fn build_context_err_to_db(e: build_context::BuildContextError) -> DbError {
    match e {
        build_context::BuildContextError::Db(d) => d,
        build_context::BuildContextError::InvalidTimezone(tz) => {
            DbError::Key(format!("invalid timezone: {tz}"))
        }
    }
}

// The reasoning-segment type is re-exported so callers that build a
// StreamingState with segments can name it via this module.
pub use crate::services::primary_stream::ReasoningSegment as OrchestratorReasoningSegment;
#[allow(unused_imports)]
use ReasoningSegment as _ReasoningSegment;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::db::Writer;
    use crate::model::embedding::CannedEmbeddingProvider;
    use crate::services::chat_events::RecordingSink;

    const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A fresh encrypted main-only `Db` (the chain-guard tests never read it —
    /// they short-circuit before any DB access).
    fn test_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("main.db");
        {
            let _w = Writer::open_writable(&path, TEST_PEPPER).expect("writable open");
        }
        let db = Db::open(DbPaths::main_only(path), TEST_PEPPER).expect("open db");
        (dir, db)
    }

    // Minimal seam bundle for the chain-guard self-tests. The guard cases
    // short-circuit before any provider / seam is touched.
    struct NoRouter;
    impl DangerousContentRouter for NoRouter {
        fn resolve(
            &self,
            p: &EffectiveProfile,
            k: &str,
            _s: &crate::services::provider_failover::DangerSettings,
            _u: &str,
        ) -> impl std::future::Future<Output = crate::services::provider_failover::RouteResult> + Send
        {
            let profile = p.clone();
            let key = k.to_string();
            async move {
                crate::services::provider_failover::RouteResult {
                    rerouted: false,
                    connection_profile: profile,
                    api_key: key,
                }
            }
        }
    }

    struct NoChainCarina;
    impl RunCarinaQuery for NoChainCarina {
        #[allow(clippy::manual_async_fn)]
        fn run(
            &mut self,
            _o: crate::services::carina_runner::RunCarinaQueryOptions,
        ) -> impl std::future::Future<
            Output = Result<
                crate::services::carina_runner::CarinaResult,
                crate::services::carina_runner::CarinaRunError,
            >,
        > + Send {
            async {
                Err(crate::services::carina_runner::CarinaRunError(
                    "no carina".into(),
                ))
            }
        }
    }

    fn chain_input(_pid: String) -> ProcessMessageInput {
        // Unreachable in the guard self-tests (the guard short-circuits first).
        ProcessMessageInput {
            chat_id: "chat-x".into(),
            user_id: "u".into(),
            options: SendMessageOptions::default(),
            clock: ProcessClock {
                now_ms: 0,
                local_offset_minutes: 0,
                random01: 0.0,
            },
            model_context_limit: 1000,
            timestamp_config: None,
            timezone: None,
            server_tz: None,
            provider_supports_web_search: false,
            log_context: LogContext::none(),
        }
    }

    /// A never-called pricing fetch for the guard self-tests (they short-circuit
    /// before the tool build).
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

    /// The chain driver returns immediately for a non-multi-character / no-content
    /// / paused initial result, and for `single_turn` (v4's `executeTurnChain`
    /// guards). None of these touch a provider, so the chained re-entry
    /// (`chain_input`) is never built.
    #[tokio::test]
    async fn chain_skips_on_guard_and_single_turn() {
        let (_dir, db) = test_db();
        let sink = RecordingSink::new();
        let embedding = CannedEmbeddingProvider::new();
        let completion = crate::model::completion::CannedCompletionProvider::new();
        let streaming = crate::model::stream::CannedStreamingProvider::new();
        let executor = CheapLlmTaskExecutor::new();
        let bc = build_context::NoopSeams;
        let orc = NoopOrchestratorSeams;
        let router = NoRouter;
        let pricing = PricingFetcher::new(NoPricingFetch);
        let prospero_fn: fn(
            crate::services::carina_runner::ProsperoCarinaErrorArgs,
        ) -> Result<(), crate::services::carina_runner::CarinaRunError> = |_a| Ok(());

        // (is_multi, has_content, is_paused, single_turn) → all must skip.
        let cases = [
            (false, true, false, false),
            (true, false, false, false),
            (true, true, true, false),
            (true, true, false, true),
        ];
        for (is_multi, has_content, is_paused, single_turn) in cases {
            let mut confirmation = message_finalizer::NoAnswerConfirmation;
            let mut compression = message_finalizer::NoAsyncCompression;
            let mut cost = message_finalizer::NoCostTracking;
            let mut carina = NoChainCarina;
            let mut prospero = crate::services::carina_runner::ClosureProspero(prospero_fn);
            let mut rng_bytes = crate::tools::rng::FixedBytes::new(vec![]);
            let ask_carina = ErasedAskCarina::not_available();
            let mut deps = OrchestratorDeps {
                db: &db,
                embedding: &embedding,
                completion: &completion,
                streaming: &streaming,
                executor: &executor,
                ask_carina: &ask_carina,
                sink: &sink,
                pricing: &pricing,
                build_context_seams: &bc,
                orchestrator_seams: &orc,
                file_bytes: &crate::services::chat_files::NotConfiguredBytes,
                image_transcoder: &crate::files::image_processing::NotConfiguredTranscoder,
                danger_router: &router,
                confirmation: &mut confirmation,
                compression: &mut compression,
                cost: &mut cost,
                carina_query: &mut carina,
                prospero: &mut prospero,
                rng_bytes: &mut rng_bytes,
                web_search: None,
            };
            execute_turn_chain(
                &mut deps,
                ExecuteTurnChainOptions {
                    chat_id: "chat-x".into(),
                    initial_result: ProcessMessageResult {
                        is_multi_character: is_multi,
                        has_content,
                        message_id: "m".into(),
                        user_participant_id: None,
                        is_paused,
                        scene_tracking_character_ids: None,
                        skipped: false,
                        skipped_participant_id: None,
                    },
                    initial_continue_mode: false,
                    never_pause_for_user: false,
                    single_turn,
                    chain_start_time_ms: 0,
                    config: ChainConfig::default(),
                },
                0,
                0.0,
                chain_input,
            )
            .await
            .expect("chain");
        }
        // No frames emitted (every case skipped).
        assert!(sink.events().is_empty());
    }

    /// The marshaling helpers pull the fields the spine consumes.
    #[test]
    fn marshaling_helpers() {
        let ch = json!({
            "id": "c1", "name": "Friday", "aliases": ["Fri"],
            "identity": "public", "description": "desc"
        });
        let cc = to_context_character(&ch);
        assert_eq!(cc.id, "c1");
        assert_eq!(cc.name, "Friday");
        assert_eq!(cc.sys.aliases, vec!["Fri".to_string()]);
        assert_eq!(cc.sys.identity.as_deref(), Some("public"));

        let prof =
            json!({ "id": "p1", "provider": "ANTHROPIC", "modelName": "claude", "baseUrl": null });
        let ep = to_effective_profile(&prof);
        assert_eq!(ep.provider, "ANTHROPIC");
        assert_eq!(ep.model_name, "claude");
        assert_eq!(ep.base_url, None);
    }

    /// The sentinel-handling precedence (v4 orchestrator.service.ts:1454–1498):
    /// tools-ran clears the bare sentinel (the tool-save branch must win) even
    /// when the offer stands; the offer gates the skip path; a sentinel without
    /// the offer clears to the empty-response branch; prose survives its
    /// sentinel line. (The corpus banks the skip / cleaned / withheld paths
    /// end-to-end; the tools-ran rule is pinned here — a corpus case would need
    /// a live tool slate.)
    #[test]
    fn sentinel_action_precedence() {
        use crate::skip_signal::DetectSkipResult as D;
        // Tools ran → clear, regardless of the offer.
        assert_eq!(
            resolve_sentinel_action(D::Skip, true, true),
            SentinelAction::ClearResponse
        );
        assert_eq!(
            resolve_sentinel_action(D::Skip, false, true),
            SentinelAction::ClearResponse
        );
        // Offer stands, no tools → the skip path.
        assert_eq!(
            resolve_sentinel_action(D::Skip, true, false),
            SentinelAction::HandleSkip
        );
        // Sentinel without the offer → the empty-response branch.
        assert_eq!(
            resolve_sentinel_action(D::Skip, false, false),
            SentinelAction::ClearResponse
        );
        // Sentinel + prose → the cleaned prose is a real reply (offer irrelevant).
        assert_eq!(
            resolve_sentinel_action(
                D::NoSkip {
                    cleaned: Some("the prose".into())
                },
                true,
                true,
            ),
            SentinelAction::ReplaceWithCleaned("the prose".into())
        );
        // No sentinel → untouched.
        assert_eq!(
            resolve_sentinel_action(D::NoSkip { cleaned: None }, true, false),
            SentinelAction::LeaveAsIs
        );
    }

    /// A canned embedding provider + settings default construct cleanly (smoke —
    /// the full spine is verified by the tier-3 differential).
    #[test]
    fn deps_smoke() {
        let _emb = CannedEmbeddingProvider::new();
        let settings = OrchestratorChatSettings::defaults_present();
        assert_eq!(settings.project_context_reinject_interval, 5);
    }

    // --- turn_tool_context (dogfood finding #22) -------------------------------

    fn ctx_args<'a>(
        image_profile_id: Option<&'a str>,
        project_id: Option<&'a str>,
    ) -> TurnToolContextArgs<'a> {
        TurnToolContextArgs {
            chat_id: "chat-1",
            user_id: "user-1",
            character_id: "char-1",
            character_participant_id: "part-1",
            image_profile_id,
            project_id,
            loaded_memories: None,
        }
    }

    /// The regression: a chat that HAS a project and an image profile must carry
    /// both into the tool context, or `doc_list_files` / `doc_grep` /
    /// `project_info` answer "requires a project context" and `generate_image`
    /// answers "not enabled for this chat" — the finding-#22 symptom.
    #[test]
    fn turn_tool_context_threads_project_and_image_profile() {
        let ctx = turn_tool_context(ctx_args(Some("img-profile-1"), Some("project-7")));
        assert_eq!(ctx.project_id.as_deref(), Some("project-7"));
        assert_eq!(ctx.image_profile_id.as_deref(), Some("img-profile-1"));
        // The identifiers v4 has always threaded stay correct.
        assert_eq!(ctx.chat_id, "chat-1");
        assert_eq!(ctx.user_id, "user-1");
        assert_eq!(ctx.character_id.as_deref(), Some("char-1"));
        assert_eq!(ctx.calling_participant_id.as_deref(), Some("part-1"));
    }

    /// The guard: a chat with NO project must still yield `None`, so the
    /// project-context refusal keeps firing where v4 fires it. Threading the ids
    /// must not turn the guard into a lie.
    #[test]
    fn turn_tool_context_leaves_a_projectless_chat_unset() {
        let ctx = turn_tool_context(ctx_args(None, None));
        assert!(ctx.project_id.is_none());
        assert!(ctx.image_profile_id.is_none());
    }

    /// v4's `projectId || undefined` / `imageProfileId || undefined`: an empty
    /// string is falsy and collapses to absent, NOT to `Some("")` (which would
    /// pass the `is_some()` guards and then fail deeper on a lookup).
    #[test]
    fn turn_tool_context_collapses_empty_strings_to_absent() {
        let ctx = turn_tool_context(ctx_args(Some(""), Some("")));
        assert!(ctx.project_id.is_none());
        assert!(ctx.image_profile_id.is_none());
    }

    /// Both loops must see the same context — v4 builds ONE `toolContext` and
    /// threads it into the native loop and the text passes alike.
    #[test]
    fn turn_tool_context_is_identical_for_both_loops() {
        let native = turn_tool_context(ctx_args(Some("img-1"), Some("proj-1")));
        let text = turn_tool_context(ctx_args(Some("img-1"), Some("proj-1")));
        assert_eq!(native.project_id, text.project_id);
        assert_eq!(native.image_profile_id, text.image_profile_id);
        assert_eq!(native.chat_id, text.chat_id);
        assert_eq!(native.calling_participant_id, text.calling_participant_id);
    }

    // --- finding #22 carry-out: the loadedMemories threading -------------------

    /// The `BuiltContext.debug_*` bags convert into the [`LoadedMemoriesContext`]
    /// shape `self_inventory` reads — the semantic narrowing keeps only the four
    /// consumed keys; inter-character + recap carry through.
    #[test]
    fn loaded_memories_conversion_narrows_to_the_consumed_shape() {
        let semantic = vec![serde_json::json!({
            "summary": "winter ledger",
            "importance": 0.8,
            "score": 0.5,
            "effectiveWeight": 0.4,
            // extra keys v4 carries through JSON.stringify but the builder drops:
            "memoryId": "m-1",
            "recallFired": true,
        })];
        let inter = vec![build_context::DebugInterCharacterOut {
            about_character_name: "Jeeves".to_string(),
            summary: "shared the umbrella".to_string(),
            importance: 0.6,
        }];
        let loaded = loaded_memories_from_debug(
            &semantic,
            Some(&inter),
            Some("mid-way through the ledgers"),
        );
        assert_eq!(loaded.semantic.len(), 1);
        assert_eq!(loaded.semantic[0].summary, "winter ledger");
        assert!((loaded.semantic[0].importance - 0.8).abs() < 1e-12);
        assert!((loaded.semantic[0].score - 0.5).abs() < 1e-12);
        assert!((loaded.semantic[0].effective_weight - 0.4).abs() < 1e-12);
        assert_eq!(loaded.inter_character.len(), 1);
        assert_eq!(loaded.inter_character[0].about_character_name, "Jeeves");
        assert_eq!(loaded.recap.as_deref(), Some("mid-way through the ledgers"));
    }

    /// An empty built context still yields a PRESENT (not `None`) loaded-memories
    /// context — v4 always passes the object, so `self_inventory` reports
    /// `available: true` with empty arrays, never the `Unavailable` arm.
    #[test]
    fn loaded_memories_conversion_empty_is_present_not_unavailable() {
        let loaded = loaded_memories_from_debug(&[], None, None);
        assert!(loaded.semantic.is_empty());
        assert!(loaded.inter_character.is_empty());
        assert_eq!(loaded.recap, None);
    }

    /// The converted slate threads into the tool context both loops receive.
    #[test]
    fn turn_tool_context_threads_loaded_memories() {
        let loaded = LoadedMemoriesContext {
            semantic: vec![LoadedSemanticMemory {
                summary: "a memory".to_string(),
                importance: 0.7,
                score: 0.3,
                effective_weight: 0.2,
            }],
            inter_character: Vec::new(),
            recap: Some("recap".to_string()),
        };
        let mut args = ctx_args(None, None);
        args.loaded_memories = Some(&loaded);
        let ctx = turn_tool_context(args);
        let threaded = ctx.loaded_memories.expect("loaded memories threaded");
        assert_eq!(threaded.semantic.len(), 1);
        assert_eq!(threaded.recap.as_deref(), Some("recap"));
        // Absent (the out-of-prompt path) stays None → the Unavailable arm.
        let absent = turn_tool_context(ctx_args(None, None));
        assert!(absent.loaded_memories.is_none());
    }

    // --- finding #27: the summary check's cheap-LLM settings threading ---------

    /// A configured `defaultCheapProfileId` (and the other stored fields) reach the
    /// summary check's [`CheapLlmSettings`] — the field carried past the presence
    /// bool. With this the cheap-LLM selection can honour the configured profile
    /// instead of always falling to `getCheapestModel(responding.provider)`.
    #[test]
    fn summary_cheap_settings_carry_the_configured_default_profile() {
        let mut s = OrchestratorChatSettings::defaults_present();
        s.cheap_llm_strategy = "USER_DEFINED".to_string();
        s.cheap_llm_user_defined_profile_id = Some("cp-user-defined".to_string());
        s.cheap_llm_default_cheap_profile_id = Some("cp-default-cheap".to_string());
        s.cheap_llm_fallback_to_local = true;
        let cheap = cheap_llm_settings_from_orchestrator(&s);
        assert_eq!(cheap.strategy, "USER_DEFINED");
        assert_eq!(
            cheap.user_defined_profile_id.as_deref(),
            Some("cp-user-defined")
        );
        assert_eq!(
            cheap.default_cheap_profile_id.as_deref(),
            Some("cp-default-cheap")
        );
        assert!(cheap.fallback_to_local);
    }

    /// An empty `strategy` (the `OrchestratorChatSettings::default()` shape, when no
    /// stored object supplied one) collapses to `PROVIDER_CHEAPEST` — the real Zod
    /// default — matching the compression selection's empty-guard. Never "AUTO"
    /// (not a valid `CheapLLMStrategyEnum` member).
    #[test]
    fn summary_cheap_settings_default_strategy_is_provider_cheapest() {
        let s = OrchestratorChatSettings {
            cheap_llm_strategy: String::new(),
            ..OrchestratorChatSettings::default()
        };
        let cheap = cheap_llm_settings_from_orchestrator(&s);
        assert_eq!(cheap.strategy, "PROVIDER_CHEAPEST");
        assert_eq!(cheap.user_defined_profile_id, None);
        assert_eq!(cheap.default_cheap_profile_id, None);
        assert!(!cheap.fallback_to_local);
    }
}
