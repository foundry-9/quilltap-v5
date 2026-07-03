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
//! * **Pending tool results / RNG auto-detect / carina markup on the user
//!   message**: gated on `!continueMode && content`. The corpus keeps the user
//!   content free of RNG patterns / carina markup and passes no pending tool
//!   results, so each detector returns "none" and writes nothing (the
//!   [`OrchestratorSeams`] `user_message_rng` / `user_message_carina` methods).
//! * **Agent mode / danger / courier**: the corpus keeps agent mode off, danger
//!   mode `DETECT_ONLY` (no reroute), and the transport non-courier — so the
//!   agent-turn-count reset, the danger reroute, and the courier short-circuit
//!   never fire. Their gates are reproduced (`agent_mode_enabled`,
//!   `is_courier`) and banked off.
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

use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};
use crate::model::completion::{CompletionMessage, CompletionProvider, CompletionRole};
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::{StreamParams, StreamingCompletionProvider};
use crate::services::build_context::{self, BuildContextInput, BuildContextSeams, BuiltContext};
use crate::services::carina_runner::{PostProsperoCarinaError, RunCarinaQuery};
use crate::services::chat_events::{
    ChainCompletePayload, ChatEvent, DonePayload, EventSink, StatusPayload, TurnCompletePayload,
    TurnStartPayload,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::message_context;
use crate::services::message_finalizer::{
    self, AnswerConfirmationRunner, AsyncCompressionTrigger, CostTracker, FinalizeOptions,
    FinalizerCharacter, FinalizerChat, FinalizerChatSettings, FinalizerCompression,
    FinalizerParticipant, FinalizerProfile, FinalizerStreaming, ParticipantCharacter,
    ProcessMessageResult,
};
use crate::services::primary_stream::{
    self, EffectiveProfile, PreservePartialOnError, ReasoningSegment, RunPrimaryStreamOptions,
    StreamingState,
};
use crate::services::provider_failover::{
    self, AttemptEmptyResponseRecoveryOptions, DangerSettings, DangerousContentRouter,
};
use crate::services::turn_orchestrator::{
    self, ChainConfig, ChainDecision, ChainGuards, ChainReason,
};

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

    /// Attachment processing (v4 `loadAndProcessFiles`). Fired only when
    /// `fileIds` is non-empty. Default: no attachments (empty result). The
    /// corpus keeps `fileIds` empty.
    fn process_files(&self, file_ids: &[String]) -> FileProcessingResult {
        FileProcessingResult::default()
    }

    /// v4's per-message RNG auto-detect on the USER message
    /// (`detectAndConvertRngPatterns`). Default: no patterns. Fired only when
    /// `autoDetectRng && content`.
    fn user_message_rng(&self, content: &str) -> bool {
        false
    }

    /// v4's Prospero cadence context re-injection whisper post. Fired only on a
    /// cadence boundary. Default: no-op (nothing posted).
    fn post_prospero_context(&self, chat_id: &str, character_participant_id: &str) {}
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
    /// `answerConfirmationSettings.enabled === true`.
    pub answer_confirmation_global_enabled: bool,
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
            answer_confirmation_global_enabled: false,
        }
    }
}

/// The result of attachment processing (v4 `loadAndProcessFiles` subset). The
/// corpus keeps everything empty.
#[derive(Clone, Debug, Default)]
pub struct FileProcessingResult {
    /// `(fileId)` links to write against the user message.
    pub attached_file_ids: Vec<String>,
    /// v4 `messageContentPrefix` — prepended to the user message content.
    pub message_content_prefix: Option<String>,
    /// v4 `attachmentsToSend` — the provider-supported attachments passed into
    /// `buildMessageContext` (merged onto the last user message). The corpus keeps
    /// this empty (the file subsystem is wave-4).
    pub attachments_to_send: Vec<Value>,
}

// ===========================================================================
// Options
// ===========================================================================

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
    /// Autonomous-room flag (bypasses the all-LLM pause in the chain).
    pub never_pause_for_user: bool,
    /// Autonomous-room flag (chain enqueues the next turn as a separate job).
    pub single_turn: bool,
    /// Autonomous-room per-turn context cap (tokens) — clamps the model-derived
    /// budget. `None` = uncapped. (Threaded into `buildContext`'s budget cap.)
    pub autonomous_context_cap: Option<i64>,
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
    RNG,
    COST,
    CARQ,
    PROS,
> where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    STR: StreamingCompletionProvider,
    SNK: EventSink,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    RNG: message_finalizer::RngDetector,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
{
    pub db: &'a Db,
    pub embedding: &'a EMB,
    pub completion: &'a CMP,
    pub streaming: &'a STR,
    pub executor: &'a CheapLlmTaskExecutor,
    pub sink: &'a SNK,
    pub build_context_seams: &'a BCS,
    pub orchestrator_seams: &'a ORC,
    pub danger_router: &'a RTR,
    /// The finalizer's answer-confirmation runner seam.
    pub confirmation: &'a mut CONF,
    /// The finalizer's async-compression trigger seam.
    pub compression: &'a mut ACOMP,
    /// The finalizer's assistant-response RNG detector seam.
    pub finalizer_rng: &'a mut RNG,
    /// The finalizer's cost-estimation seam.
    pub cost: &'a mut COST,
    /// The finalizer's + orchestrator's carina query seam.
    pub carina_query: &'a mut CARQ,
    /// The finalizer's Prospero-carina-error post seam.
    pub prospero: &'a mut PROS,
}

/// v4 `processMessage`. The main send-path spine. Composes the ported services,
/// reproducing every unported-subsystem gate and routing the subsystem body
/// through an [`OrchestratorSeams`] method. Returns the [`ProcessMessageResult`]
/// the chain driver reads.
#[allow(clippy::too_many_lines, clippy::type_complexity)]
pub async fn process_message<
    EMB,
    CMP,
    STR,
    SNK,
    BCS,
    ORC,
    RTR,
    CONF,
    ACOMP,
    RNG,
    COST,
    CARQ,
    PROS,
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
        RNG,
        COST,
        CARQ,
        PROS,
    >,
    input: &ProcessMessageInput,
) -> Result<ProcessMessageResult, DbError>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    STR: StreamingCompletionProvider,
    SNK: EventSink,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    RNG: message_finalizer::RngDetector,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
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
    // image tools), a wave-4 seam kept inactive in the corpus.
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

    // --- Courier short-circuit gate (orchestrator.service.ts:318–330) ---
    // The corpus keeps the transport non-courier; the gate is reproduced so a
    // courier profile would be caught (its dispatch is a wave-4 seam — a courier
    // turn is a documented deferral).
    let is_courier = json_str(&connection_profile, "transport").as_deref() == Some("courier");
    if is_courier {
        // A courier turn is out of scope (the dispatch service is unported). The
        // corpus never sends one; erroring keeps the deferral explicit rather
        // than silently mis-handling.
        return Err(DbError::Key(
            "courier transport not ported (orchestrator deferral)".into(),
        ));
    }

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

    // --- Agent mode (orchestrator.service.ts:351–365) ---
    // Agent mode is a wave-4 seam; the corpus keeps it OFF (no per-message
    // reset). The gate (`!continueMode && agentMode.enabled`) is reproduced but
    // never fires.
    let agent_mode_enabled = false;
    if !is_continue_mode && agent_mode_enabled {
        // (unreached in the corpus — the reset write would go here)
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

    // --- Danger state (orchestrator.service.ts:424–441) ---
    // The danger-classification + reroute resolver is a wave-4 seam. The corpus
    // keeps danger mode non-AUTO so no reroute happens; the effective profile is
    // the resolved connection profile.
    let effective_profile = to_effective_profile(&connection_profile);

    let mut streaming_state = StreamingState {
        effective_profile: Some(effective_profile.clone()),
        effective_api_key: resolution.api_key.clone().unwrap_or_default(),
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
        // The whisper post is a seam (post-office subsystem); the gate is here.
        deps.orchestrator_seams
            .post_prospero_context(&chat_id, &character_participant_id);
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
    let file_processing = deps
        .orchestrator_seams
        .process_files(&input.options.file_ids);

    // --- RNG auto-detect on the user message (orchestrator.service.ts:587–625) ---
    // Gated on autoDetectRng && !continueMode && content. The corpus keeps the
    // content pattern-free → no tool messages written.
    let auto_detect_rng = chat_settings
        .as_ref()
        .map(|s| s.auto_detect_rng)
        .unwrap_or(true);
    if auto_detect_rng && !is_continue_mode && !input.options.content.is_empty() {
        let _ = deps
            .orchestrator_seams
            .user_message_rng(&input.options.content);
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
    // forceToolsOnNextMessage flag (corpus keeps it clear). Same as the
    // full-context bypass: no ported `ChatUpdate` setter, corpus keeps it clear,
    // so the reset never fires (documented deferral for a tool-settings-changed
    // turn — that path is wave-4 anyway).
    let _force_tools = chat.get("forceToolsOnNextMessage").and_then(Value::as_bool) == Some(true);
    // The tool build + pseudo-tool mode resolution is a wave-4 seam. The corpus
    // keeps the tool slate EMPTY (native mode, no tools), so `actualTools` is []
    // and `toolInstructions` is None — the tool loops downstream no-op.
    let tool_instructions: Option<String> = None;

    // --- Build context (orchestrator.service.ts:908–1010) ---
    sink.emit(ChatEvent::status(StatusPayload {
        stage: "gathering".into(),
        message: "Gathering memories and context...".into(),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

    let build_input = build_context_input(BuildContextArgs {
        input,
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
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&empty_participants),
        participant_transparency: &participant_transparency,
    };

    let mc_result = message_context::build_message_context(
        db,
        deps.embedding,
        deps.completion,
        deps.executor,
        deps.build_context_seams,
        &message_context::NoopMessageContextSeams,
        &mc_params,
        build_input,
        &existing_messages,
        &file_processing.attachments_to_send,
    )
    .await
    .map_err(build_context_err_to_db)?;
    let built_context: BuiltContext = mc_result.built_context;

    sink.emit(ChatEvent::status(StatusPayload {
        stage: "preparing".into(),
        message: format!("Preparing request for {character_name}..."),
        tool_name: None,
        character_name: Some(character_name.clone()),
        character_id: Some(character_id.clone()),
    }));

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
    let stream_messages: Vec<CompletionMessage> = mc_result
        .formatted_messages
        .iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect();
    let params = StreamParams {
        messages: stream_messages,
        model: effective_profile.model_name.clone(),
        temperature: json_f64(&connection_profile, "temperature"),
        max_tokens: None,
        top_p: None,
        tools: None,
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id,
        stop: Vec::new(),
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
        });
    }

    // --- Tool loops (orchestrator.service.ts:1259–1378) ---
    // No tools this turn (corpus) → the native + text tool loops no-op. A
    // non-empty tool slate is the wave-4 tool subsystem.
    let tool_messages_len = 0usize;

    // --- Empty-response recovery (orchestrator.service.ts:1380–1397) ---
    let content_was_flagged_dangerous = false;
    let recovery_flags = provider_failover::attempt_empty_response_recovery(
        deps.streaming,
        sink,
        deps.danger_router,
        AttemptEmptyResponseRecoveryOptions {
            state: &mut streaming_state,
            tool_messages_length: tool_messages_len,
            content_was_flagged_dangerous,
            danger_settings: DangerSettings {
                mode: "DETECT_ONLY".into(),
                uncensored_text_profile_id: None,
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
        let finalizer_compression = FinalizerCompression {
            compression_enabled,
            cheap_llm_selection_present: chat_settings
                .as_ref()
                .map(|s| s.cheap_llm_settings_present)
                .unwrap_or(false),
            original_system_prompt_present: !built_context.original_system_prompt.is_empty(),
        };
        let finalizer_participant_characters: Vec<ParticipantCharacter> = participant_characters
            .values()
            .map(to_participant_character)
            .collect();
        let finalizer_chat_settings = chat_settings.as_ref().map(|s| FinalizerChatSettings {
            cheap_llm_settings_present: s.cheap_llm_settings_present,
            auto_detect_rng: Some(s.auto_detect_rng),
            answer_confirmation_global_enabled: s.answer_confirmation_global_enabled,
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
                generated_image_ids: Vec::new(),
                pre_generated_assistant_message_id: Some(
                    pre_generated_assistant_message_id.clone(),
                ),
                profile,
                streaming: finalizer_streaming,
                compression: finalizer_compression,
                participant_characters: finalizer_participant_characters,
                chat_settings: finalizer_chat_settings,
                is_dangerous_chat: false,
            },
            deps.confirmation,
            deps.compression,
            deps.finalizer_rng,
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
                run_summary_check(deps, &chat_id, &user_id, &effective_profile).await?;
            }
        }

        // Keep `existing_messages` referenced (it fed buildContext + the previous
        // response id scan; nothing after this reads it).
        existing_messages.clear();
        Ok(result)
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
        })
    }
}

/// Run the context-summary check (the finalizer's deferred invocation). The
/// cheap-LLM profile / settings / available-profiles are resolved above the seam;
/// the corpus keeps them consistent with the compression selection. When the gate
/// fires a fold, `check_and_generate_summary_if_needed` writes the summary + the
/// title enqueue through the `Db` — the effect the differential banks.
#[allow(clippy::type_complexity)]
async fn run_summary_check<EMB, CMP, STR, SNK, BCS, ORC, RTR, CONF, ACOMP, RNG, COST, CARQ, PROS>(
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
        RNG,
        COST,
        CARQ,
        PROS,
    >,
    chat_id: &str,
    user_id: &str,
    profile: &EffectiveProfile,
) -> Result<(), DbError>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    STR: StreamingCompletionProvider,
    SNK: EventSink,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    RNG: message_finalizer::RngDetector,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
{
    // The cheap-LLM profile the summary uses IS the effective profile in the
    // corpus (single-profile chats); the settings are a fixed low-window config.
    let cheap_profile = crate::cheap_llm::CheapLlmProfile {
        id: profile.id.clone(),
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        base_url: profile.base_url.clone(),
        is_cheap: false,
        is_dangerous_compatible: false,
        parameters: None,
        max_tokens: None,
        model_class: None,
    };
    let cheap_settings = super::context_summary::CheapLlmSettings {
        strategy: "AUTO".to_string(),
        user_defined_profile_id: None,
        default_cheap_profile_id: None,
        fallback_to_local: false,
    };
    super::context_summary::check_and_generate_summary_if_needed(
        deps.db,
        deps.completion,
        deps.executor,
        chat_id,
        &cheap_profile,
        &cheap_settings,
        std::slice::from_ref(&cheap_profile),
        user_id,
        None,
        None,
        true,
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
    RNG,
    COST,
    CARQ,
    PROS,
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
        RNG,
        COST,
        CARQ,
        PROS,
    >,
    opts: ExecuteTurnChainOptions,
    now_ms: i64,
    random01: f64,
    mut make_chain_input: F,
) -> Result<(), DbError>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    STR: StreamingCompletionProvider,
    SNK: EventSink,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    RNG: message_finalizer::RngDetector,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    F: FnMut(String) -> ProcessMessageInput,
{
    let db = deps.db;
    let initial = &opts.initial_result;
    // v4: only chain for a multi-character chat that produced content and isn't
    // paused.
    if !initial.is_multi_character || !initial.has_content || initial.is_paused {
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
        let input = make_chain_input(participant_id.clone());
        let chain_step = process_message(deps, &input).await;
        match chain_step {
            Ok(chain_result) => {
                deps.sink
                    .emit(ChatEvent::turn_complete(TurnCompletePayload {
                        participant_id: participant_id.clone(),
                        message_id: chain_result.message_id.clone(),
                        chain_depth,
                    }));
                if !chain_result.has_content {
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

struct BuildContextArgs<'a> {
    input: &'a ProcessMessageInput,
    chat: &'a Value,
    character: &'a Value,
    character_participant: &'a Value,
    connection_profile: &'a build_context::ConnectionProfileInput,
    user_character: Option<crate::system_prompt::UserCharacter>,
    roleplay_template: Option<String>,
    is_multi_character: bool,
    participant_characters: &'a HashMap<String, Value>,
    existing_messages: &'a [Value],
    final_user_message: Option<String>,
    speaking_as: Option<String>,
    tool_instructions: Option<String>,
    compression_enabled: bool,
    bypass_compression: bool,
}

/// Assemble the [`BuildContextInput`] from the resolved orchestrator state (v4's
/// inline `buildMessageContext` argument object). Single-character corpus:
/// participant/multi-character fields are `None`, `skip_memories` false,
/// `min_memory_importance` 0.
fn build_context_input(args: BuildContextArgs<'_>) -> BuildContextInput {
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
        scene_state: args.chat.get("sceneState").cloned(),
        precompiled_identity_stack: None,
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
        model_context_limit: args.input.model_context_limit,
        user_id: args.input.user_id.clone(),
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
        timestamp_config: args.input.timestamp_config.clone(),
        is_initial_message: false,
        timezone: args.input.timezone.clone(),
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
        cheap_llm_selection: None,
        bypass_compression: args.bypass_compression,
        generate_memory_recap: false,
        uncensored_fallback: None,
        is_continue_mode: args.input.options.continue_mode,
        now_ms: args.input.clock.now_ms,
        local_offset_minutes: args.input.clock.local_offset_minutes,
        minutes_since_last_timestamp_announcement: None,
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

fn json_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}
fn json_f64(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
fn json_str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn to_effective_profile(profile: &Value) -> EffectiveProfile {
    EffectiveProfile {
        id: json_str(profile, "id").unwrap_or_default(),
        provider: json_str(profile, "provider").unwrap_or_default(),
        model_name: json_str(profile, "modelName").unwrap_or_default(),
        base_url: json_str(profile, "baseUrl"),
    }
}

fn effective_profile_profile(profile: &Value) -> build_context::ConnectionProfileInput {
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
        impersonating_participant_ids: json_str_array(chat, "impersonatingParticipantIds"),
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        spoken_this_cycle_participant_ids: json_str(chat, "spokenThisCycleParticipantIds"),
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
fn load_participant_characters(
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

fn build_context_err_to_db(e: build_context::BuildContextError) -> DbError {
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
        fn run(
            &mut self,
            _o: crate::services::carina_runner::RunCarinaQueryOptions,
        ) -> Result<
            crate::services::carina_runner::CarinaResult,
            crate::services::carina_runner::CarinaRunError,
        > {
            Err(crate::services::carina_runner::CarinaRunError(
                "no carina".into(),
            ))
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
            let mut rng = message_finalizer::NoRng;
            let mut cost = message_finalizer::NoCostTracking;
            let mut carina = NoChainCarina;
            let mut prospero = crate::services::carina_runner::ClosureProspero(prospero_fn);
            let mut deps = OrchestratorDeps {
                db: &db,
                embedding: &embedding,
                completion: &completion,
                streaming: &streaming,
                executor: &executor,
                sink: &sink,
                build_context_seams: &bc,
                orchestrator_seams: &orc,
                danger_router: &router,
                confirmation: &mut confirmation,
                compression: &mut compression,
                finalizer_rng: &mut rng,
                cost: &mut cost,
                carina_query: &mut carina,
                prospero: &mut prospero,
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

    /// A canned embedding provider + settings default construct cleanly (smoke —
    /// the full spine is verified by the tier-3 differential).
    #[test]
    fn deps_smoke() {
        let _emb = CannedEmbeddingProvider::new();
        let settings = OrchestratorChatSettings::defaults_present();
        assert_eq!(settings.project_context_reinject_interval, 5);
    }
}
