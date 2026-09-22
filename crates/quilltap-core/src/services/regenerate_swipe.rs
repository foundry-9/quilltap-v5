//! Regenerate-swipe (port of v4
//! `lib/services/chat-message/regenerate-swipe.service.ts`).
//!
//! `regenerate_message_as_swipe` is the sibling entry point to
//! [`super::orchestrator::process_message`]: it generates an alternative
//! ("swipe") for an existing ASSISTANT message and persists it as a
//! properly-attributed variant of that message, grouped in place. It runs a
//! **single provider call with no tools** (no turn chain) — a swipe is one
//! alternative line — so it stays off the streaming/turn-chain path and the
//! live send path is untouched. It *is* READ as a stream (v4 `f564b0de3`): the
//! caller can hand in a [`SwipeProgressEmitter`] and watch the re-roll arrive
//! token by token. With no emitter the generation is identical, just silent.
//!
//! It composes the already-ported services: [`super::participant_resolver`]
//! (responder resolution + participant data + roleplay template),
//! [`super::user_identity_resolver`], [`super::message_context`] (the full
//! provider-ready context, continue-mode), the [`CompletionProvider`] seam (the
//! one generation), the swipe-group bookkeeping on `chat_messages`, and the
//! ported [`super::memory_service::delete_memories_by_source_message_with_vectors`]
//! cascade.
//!
//! ## The tracked deferral is CLOSED (v4 `f564b0de3`, P4.D207)
//!
//! This module used to say: *"the new swipe's `rawResponse` /
//! `reasoningContent` / `thoughtSignature` are set to `null` — the ported
//! `CompletionResponse` is the cheap-LLM subset (`content` + `usage`) and does
//! not carry the provider's raw payload / reasoning / thought signature;
//! forwarding them awaits the richer wire-decoded response."* v4 has now moved
//! the persist onto the CHUNKS, and says so itself: *every field the swipe
//! persists — usage, raw response, thought signature, reasoning — rides the
//! chunks, so reading the response as a stream costs the record nothing.*
//!
//! So the deferral is retired rather than worked around: the generation goes
//! through the STREAMING seam
//! ([`StreamingCompletionProvider`](crate::model::stream::StreamingCompletionProvider)),
//! whose [`StreamChunk`](crate::model::stream::StreamChunk) already carries all
//! six fields, and the swipe row takes them last-wins off the chunks. **That
//! moves a tier-2 comparand BY DESIGN** — three columns v5 wrote `null` now
//! carry real values, so `regenerate_swipe_tier3` is re-recorded at the target
//! pin rather than "fixed" back.
//!
//! Two other things ride the seam change, both improvements v4 already had:
//!
//! * **The stall watchdog is not optional on any `stream_message` consumer**
//!   (bug 141). An SDK timeout stops at the response headers, so a provider
//!   that answers and then goes quiet would hold this call — and with it the
//!   operator's disabled composer — open indefinitely. This is the TWELFTH
//!   wrap site (`stream_watchdog_wrap_census`), and the SECOND outside v4's
//!   one funnel: it takes the Salon's default 240 s/120 s budgets but v4's own
//!   `context: 'regenerate-swipe.service'`, which is a third class the census
//!   did not have (the greeting is the first, on its own 90 s/60 s).
//! * **Attachments now ride their own messages.** The old completion funnel
//!   narrowed each bag to four fields (dropping `url`) and re-anchored them by
//!   index; [`StreamMessage::User`](crate::model::stream::StreamMessage) carries
//!   the VERBATIM JSON bags per message, exactly as v4's
//!   `attachments: m.attachments` does. The named `url`-drop divergence this
//!   module recorded is therefore CLOSED, not carried.

use serde_json::{json, Map, Value};

use crate::api::types::Event;
use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::{
    StreamMessage, StreamParams, StreamUsage, StreamingCompletionProvider, ToolCallPayload,
};
use crate::model::stream_watchdog::{watch_stream, StallBudgets, StallWatchdogContext};
use crate::services::build_context::{BuildContextSeams, ConnectionProfileInput};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::memory_service::delete_memories_by_source_message_with_vectors;
use crate::services::message_context::{
    self, FormattedMsg, MessageContextParams, MessageContextSeams,
};
use crate::services::orchestrator::{
    build_context_input, effective_profile_profile, json_f64, json_str, BuildContextArgs,
};
use crate::weighted_random::DrawSource;

/// A step of a regeneration, reported live so the Salon can narrate it exactly
/// the way it narrates a first-time turn (v4 `RegenerateSwipeProgress`,
/// `regenerate-swipe.service.ts:51-56`).
///
/// `Delta` is a DELTA (append it); `Reasoning` is CUMULATIVE (replace it) — the
/// same contract the send path's stream frames use, so the client-side handling
/// is identical in both places. Never concatenate reasoning.
#[derive(Debug, Clone, PartialEq)]
pub enum RegenerateSwipeProgress {
    /// One of v4's FOUR status beats (`gathering` / `sending` / `regenerating`
    /// / `saving`). The swipe always carries both character fields.
    Status {
        stage: &'static str,
        message: String,
        character_name: String,
        character_id: String,
    },
    /// A content DELTA — append.
    Delta { content: String },
    /// The CUMULATIVE reasoning so far — replace.
    Reasoning { reasoning: String },
}

impl RegenerateSwipeProgress {
    /// v4's SSE payload object for this step, VERBATIM — the bytes
    /// `app/api/v1/messages/[id]/route.ts` puts on the wire through
    /// `encodeStatusEvent` / `encodeContentChunk` / `encodeReasoningChunk`.
    ///
    /// ⚠ **The `status` object carries `kind` too**, and that is measured, not
    /// assumed: the route passes the WHOLE progress event to
    /// `encodeStatusEvent(encoder, event)`, which does
    /// `JSON.stringify({ status })` — so v4's `{kind, stage, message,
    /// characterName, characterId}` literal reaches the client with its `kind`
    /// intact and in first position. (`encodeStatusEvent`'s TypeScript
    /// parameter type does not declare `kind`, but `event` is a variable, not
    /// an object literal, so TS's excess-property check never applies and
    /// nothing strips it at runtime. The P4.D207 order's §S.2 paragraph omits
    /// `kind`; the hunks win — §R.4.)
    pub fn to_v4_frame(&self) -> Value {
        match self {
            RegenerateSwipeProgress::Status {
                stage,
                message,
                character_name,
                character_id,
            } => json!({
                "status": {
                    "kind": "status",
                    "stage": stage,
                    "message": message,
                    "characterName": character_name,
                    "characterId": character_id,
                }
            }),
            RegenerateSwipeProgress::Delta { content } => json!({ "content": content }),
            RegenerateSwipeProgress::Reasoning { reasoning } => json!({ "reasoning": reasoning }),
        }
    }
}

/// v4's optional `onProgress` callback, as a per-call emitter onto the engine's
/// Event channel (the
/// [`GeneratorProgressEmitter`](crate::services::generator_progress) precedent,
/// same shape and the same empty-string rule).
///
/// Inert when the caller asked for no narration (`stream` absent/false), so the
/// service never branches on whether anyone is watching — which is v4's own
/// invariant: *with no callback the generation is identical, just silent.*
///
/// Frames ride [`Event::swipe_progress`] scope-tagged by the TARGET message id.
/// Unlike [`crate::services::creation_progress`] there is no replay buffer and
/// none is needed: the client dispatches `messageSwipe { stream: true }` and is
/// already subscribed, and the REST SSE edge subscribes before it polls the
/// dispatch future at all.
#[derive(Clone, Default)]
pub struct SwipeProgressEmitter {
    inner: Option<SwipeProgressActive>,
}

#[derive(Clone)]
struct SwipeProgressActive {
    /// v4 has no such id (its frames ride the request's own HTTP response);
    /// v5's ONE Event channel needs a scope tag, and §S.2 fixes it as the
    /// swiped message's id.
    progress_id: String,
    events: tokio::sync::broadcast::Sender<Event>,
}

impl std::fmt::Debug for SwipeProgressEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwipeProgressEmitter")
            .field(
                "progress_id",
                &self.inner.as_ref().map(|a| a.progress_id.as_str()),
            )
            .finish()
    }
}

impl SwipeProgressEmitter {
    /// The no-op emitter — nobody asked for narration.
    pub fn inert() -> Self {
        SwipeProgressEmitter { inner: None }
    }

    /// An active emitter tagged with the target message id.
    pub fn active(
        progress_id: impl Into<String>,
        events: tokio::sync::broadcast::Sender<Event>,
    ) -> Self {
        SwipeProgressEmitter {
            inner: Some(SwipeProgressActive {
                progress_id: progress_id.into(),
                events,
            }),
        }
    }

    /// Build from the `stream` flag: `false` is inert, and so is an empty
    /// target id (the `GeneratorProgressEmitter::from_id` emptiness rule).
    pub fn from_flag(
        stream: bool,
        progress_id: &str,
        events: tokio::sync::broadcast::Sender<Event>,
    ) -> Self {
        if stream && !progress_id.is_empty() {
            Self::active(progress_id, events)
        } else {
            Self::inert()
        }
    }

    /// Whether anyone is being narrated to.
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }

    /// Publish one v4 frame VERBATIM. Best-effort: a lagging or absent
    /// subscriber is fine (a frame is a narration, never the outcome — the
    /// dispatch response still carries the persisted row).
    pub fn emit_frame(&self, frame: Value) {
        let Some(active) = &self.inner else {
            return;
        };
        let _ = active
            .events
            .send(Event::swipe_progress(active.progress_id.clone(), frame));
    }

    /// One [`RegenerateSwipeProgress`] step (v4's `onProgress(event)`).
    pub fn emit(&self, event: RegenerateSwipeProgress) {
        if self.inner.is_none() {
            return;
        }
        self.emit_frame(event.to_v4_frame());
    }

    /// v4's terminal `{done: true, message: newSwipe}`, built INLINE by the
    /// route (`route.ts:347-350`) rather than by the service — so v5 emits it
    /// from [`crate::api::salon::message_swipe_generate`], v4's own placement.
    pub fn emit_done(&self, message: &Value) {
        self.emit_frame(json!({ "done": true, "message": message }));
    }

    /// v4's `encodeErrorEvent(encoder, 'Failed to generate alternative
    /// response', 'regenerate_failed', <message>)` (`route.ts:356-364`) — a
    /// failure AFTER the stream opened, because the headers are long gone.
    pub fn emit_error(&self, details: &str) {
        self.emit_frame(json!({
            "error": "Failed to generate alternative response",
            "errorType": "regenerate_failed",
            "details": details,
        }));
    }

    /// The four status beats, each with v4's sentence and both character
    /// fields. Spelled here so the bytes have ONE home.
    fn status(&self, stage: &'static str, message: String, name: &str, id: &str) {
        self.emit(RegenerateSwipeProgress::Status {
            stage,
            message,
            character_name: name.to_string(),
            character_id: id.to_string(),
        });
    }
}

/// JS truthiness for a `serde_json::Value` (v4's `if (chunk.rawResponse)`):
/// `''`, `0`, `false`, `null` and an absent key are falsy; every other value —
/// **including an empty object**, which is what v4's own canned stream sends —
/// is truthy.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(true),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// The inputs `regenerate_message_as_swipe` consumes (v4 `RegenerateSwipeOptions`
/// + the injected wall-clock / model-limit the differential freezes).
pub struct RegenerateSwipeOptions {
    /// The user id.
    pub user_id: String,
    /// The chat the message lives in (already loaded by the caller — the
    /// slim-row-read + overlaid `Value`).
    pub chat: Value,
    /// The ASSISTANT message being regenerated (a `chat_messages` event `Value`).
    pub target_message: Value,
    /// All events in the chat (already loaded by the caller).
    pub all_messages: Vec<Value>,
    /// The user-controlled participant the human is "Speaking As" (optional).
    pub active_user_participant_id: Option<String>,
    /// The model's context-window limit (v4 `getModelContextLimit`; resolved above
    /// the seam).
    pub model_context_limit: i64,
    /// The optional timestamp config (v4 `chatSettings.defaultTimestampConfig`).
    pub timestamp_config: Option<crate::chat_timestamp::TimestampConfig>,
    /// The resolved IANA timezone (v4 `resolveTimezone`).
    pub timezone: Option<String>,
    /// The SERVER-LOCAL IANA zone (v4's ambient process zone) — the memory
    /// distill's TODAY line + day-reference scan resolve their calendar in it.
    /// ⚠ NOT [`Self::timezone`], the story/timestamp zone above.
    pub server_tz: Option<String>,
    /// The wall clock (v4 `Date.now()` — the buildContext timestamp base + the
    /// swipe id/… mint points; the swipe's own `createdAt` is the target's).
    pub now_ms: i64,
    /// The local UTC offset (minutes) buildContext's timestamp math reads.
    pub local_offset_minutes: i64,
    /// `Math.random()`'s value for the weighted responder fallback (only read when
    /// the target message carries no participant id — a legacy single-char row).
    pub random01: DrawSource,
    /// v4 `onProgress?` — live narration for a caller showing the re-roll as it
    /// happens. [`SwipeProgressEmitter::inert()`] (the `Default`) is v4's
    /// absent callback: *with no callback the generation is identical, just
    /// silent.*
    pub progress: SwipeProgressEmitter,
}

/// Error from a regenerate-swipe (v4 throws for a non-regenerable target).
#[derive(Debug)]
pub enum RegenError {
    /// The target is not an ASSISTANT message (v4 "Only assistant messages can be
    /// regenerated").
    NotAssistant,
    /// The target is a Staff/system-authored message (v4 "Staff and system
    /// messages cannot be regenerated").
    StaffMessage,
    /// A DB / resolution / stream error surfaced.
    Db(DbError),
    /// The generation itself failed — a provider error, or the stall watchdog
    /// abandoning a silent stream (bug 141). Carries the provider/watchdog
    /// message VERBATIM, because that is what v4 puts on the wire: the JSON leg
    /// answers `serverError(error.message)` and the SSE leg's `error` frame
    /// carries `details: error.message` (`route.ts:356-364,405`). v5 used to
    /// wrap this as `"swipe generation failed: {…}"` inside a
    /// `DbError::Internal`, which no v4 byte matches — a pre-existing
    /// divergence, closed here because the error frame made it visible.
    Generation(String),
}

impl std::fmt::Display for RegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegenError::NotAssistant => write!(f, "Only assistant messages can be regenerated"),
            RegenError::StaffMessage => {
                write!(f, "Staff and system messages cannot be regenerated")
            }
            RegenError::Db(e) => write!(f, "regenerate-swipe failed: {e:?}"),
            // v4's raw `error.message`, unadorned — see the variant's doc.
            RegenError::Generation(m) => write!(f, "{m}"),
        }
    }
}
impl From<DbError> for RegenError {
    fn from(e: DbError) -> Self {
        RegenError::Db(e)
    }
}

/// v4 `regenerateMessageAsSwipe`. Generate a fresh response for an existing
/// ASSISTANT message and persist it as a swipe variant. Returns the new swipe
/// message event.
#[allow(clippy::too_many_arguments)]
pub async fn regenerate_message_as_swipe<EMB, CMP, STR, BCS, MCS>(
    db: &Db,
    embedding: &EMB,
    completion: &CMP,
    streaming: &STR,
    executor: &CheapLlmTaskExecutor,
    bc_seams: &BCS,
    mc_seams: &MCS,
    opts: RegenerateSwipeOptions,
) -> Result<Value, RegenError>
where
    EMB: EmbeddingProvider,
    // `completion` stays: the context build's own cheap-LLM feeders (recall
    // extraction, the distill) go through the COMPLETION half on both sides.
    // Only the one visible generation moved to the stream.
    CMP: CompletionProvider,
    STR: StreamingCompletionProvider,
    BCS: BuildContextSeams,
    MCS: MessageContextSeams,
{
    let RegenerateSwipeOptions {
        user_id,
        chat,
        target_message,
        all_messages,
        active_user_participant_id,
        model_context_limit,
        timestamp_config,
        timezone,
        server_tz,
        now_ms,
        local_offset_minutes,
        random01,
        progress,
    } = opts;

    // --- Guards (v4 lines 60–67) ---
    if json_str(&target_message, "role").as_deref() != Some("ASSISTANT") {
        return Err(RegenError::NotAssistant);
    }
    // Staff/system-authored messages (Lantern, Host, Prospero, …) are not
    // character turns — they have no responder to regenerate from.
    if target_message
        .get("systemSender")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err(RegenError::StaffMessage);
    }

    let chat_id = json_str(&chat, "id").unwrap_or_default();
    let target_message_id = json_str(&target_message, "id").unwrap_or_default();

    // --- Resolve the responder from the message's own participant (v4 68–86) ---
    // A null participant (legacy/single-character message) falls back to weighted
    // first-responder selection; a present one pins the responder (continueMode =
    // true → "throw if not found").
    let requested_participant_id = json_str(&target_message, "participantId");
    let resolution = super::participant_resolver::resolve_responding_participant(
        db,
        &chat,
        &user_id,
        requested_participant_id.as_deref(),
        requested_participant_id.is_some(),
        None,
        &random01,
    )
    .await
    .map_err(|e| DbError::Internal(format!("participant resolution failed: {e:?}")))?;

    let character_participant = resolution.character_participant.clone();
    let character = resolution.character.clone();
    let connection_profile = resolution.connection_profile.clone();
    let is_multi_character = resolution.is_multi_character;

    let character_id = json_str(&character, "id").unwrap_or_default();
    let character_name = json_str(&character, "name").unwrap_or_default();
    let character_participant_id = json_str(&character_participant, "id").unwrap_or_default();

    // --- User identity (honors "Speaking As") + per-char map + template (v4 88–98) ---
    let speaking_as_id = active_user_participant_id.clone().or_else(|| {
        chat.get("activeTypingParticipantId")
            .and_then(Value::as_str)
            .map(String::from)
    });
    let identity = super::user_identity_resolver::resolve_user_identity(
        db,
        &user_id,
        &chat,
        speaking_as_id.as_deref(),
    )
    .await?;
    let user_character = Some(crate::system_prompt::UserCharacter {
        name: identity.name.clone(),
        description: identity.description.clone(),
    });

    let participant_characters =
        super::participant_resolver::load_all_participant_data(db, &chat, &character)?;

    let uid = user_id.clone();
    let chat_settings =
        db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))?;
    let default_roleplay_template_id = chat_settings
        .as_ref()
        .and_then(|s| s.get("defaultRoleplayTemplateId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let roleplay_template = super::participant_resolver::get_roleplay_template(
        db,
        &chat,
        default_roleplay_template_id.as_deref(),
    )
    .await
    .map_err(|e| DbError::Internal(format!("roleplay template resolution failed: {e:?}")))?
    .map(|r| r.system_prompt);

    // --- Context = everything strictly BEFORE the target (v4 100–106) ---
    // Sibling swipes share the original's timestamp, so a strict `<` also drops
    // them.
    let target_time =
        crate::clock::iso_to_ms(&json_str(&target_message, "createdAt").unwrap_or_default())
            .unwrap_or(i64::MAX);
    let previous_messages: Vec<Value> = all_messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && json_str(m, "createdAt")
                    .and_then(|c| crate::clock::iso_to_ms(&c))
                    .map(|ms| ms < target_time)
                    .unwrap_or(false)
        })
        .cloned()
        .collect();

    // === P4.D205 OUT-OF-MANDATE — P4.D207 preserves ===
    // The swipe's inform re-apply handle (v4 `regenerate-swipe.service.ts:
    // 146-155`). A swipe re-rolls the line this message was, so its generation
    // must see exactly the informs that generation saw: the target plus every
    // id in its swipe group, deduplicated, first-seen order (v4's
    // `[...new Set([...])]`). Pending rows are deliberately excluded — it would
    // be surprising for a brand-new passage to land in a re-roll of an old line
    // and be spent there — and NOTHING here consumes: `build_inform_block`
    // returns no row ids for a regeneration, and this path never calls
    // `mark_consumed`.
    //
    // P4.D207 owns this file and rewrites the generation below as a watched
    // stream; this handle must survive that rewrite. `chat_informs_swipe_handle`
    // in the harness is the source census that fails if it does not.
    let existing_swipe_group_id =
        json_str(&target_message, "swipeGroupId").filter(|s| !s.is_empty());
    let regeneration_of_message_ids: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let target_id = json_str(&target_message, "id").unwrap_or_default();
        if seen.insert(target_id.clone()) {
            out.push(target_id);
        }
        if let Some(group) = &existing_swipe_group_id {
            for m in &all_messages {
                if m.get("type").and_then(Value::as_str) != Some("message") {
                    continue;
                }
                if json_str(m, "swipeGroupId").as_deref() != Some(group.as_str()) {
                    continue;
                }
                if let Some(id) = json_str(m, "id") {
                    if seen.insert(id.clone()) {
                        out.push(id);
                    }
                }
            }
        }
        out
    };
    // === end P4.D205 OUT-OF-MANDATE ===

    // --- Beat 1 of 4: `gathering` (v4 `regenerate-swipe.service.ts:156-162`) ---
    // v4 emits this after the inform re-apply ids are computed and BEFORE
    // `buildMessageContext`. Everything between `previous_messages` and the call
    // below is pure computation on v5's side, so this is v4's position exactly.
    progress.status(
        "gathering",
        format!("Regenerating — gathering {character_name}'s memories and context..."),
        &character_name,
        &character_id,
    );

    // --- Build the full provider-ready context (continue mode, no new user msg) ---
    let cp_input: ConnectionProfileInput = effective_profile_profile(&connection_profile);
    let build_input = build_context_input(BuildContextArgs {
        user_id: &user_id,
        model_context_limit,
        // === P4.D205 OUT-OF-MANDATE — P4.D207 preserves ===
        regeneration_of_message_ids: Some(regeneration_of_message_ids),
        // === end P4.D205 OUT-OF-MANDATE ===
        // v4's `f933ba9c` reserves the turn extras on the orchestrator path only;
        // the regenerate/swipe path builds no tool schemas and splices nothing,
        // so it reserves nothing here either.
        reserved_outgoing_tokens: None,
        timestamp_config: timestamp_config.clone(),
        timezone: timezone.clone(),
        server_tz: server_tz.clone(),
        is_continue_mode: true,
        now_ms,
        local_offset_minutes,
        chat: &chat,
        character: &character,
        character_participant: &character_participant,
        connection_profile: &cp_input,
        user_character,
        roleplay_template,
        is_multi_character,
        participant_characters: &participant_characters,
        existing_messages: &previous_messages,
        // continue mode → no new user message.
        final_user_message: None,
        speaking_as: speaking_as_id.clone(),
        tool_instructions: None,
        compression_enabled: false,
        bypass_compression: false,
        // Regenerate-swipe rebuilds context fresh; no async pre-compression cache.
        cached_compression_result: None,
        cached_compression_message_count: None,
        // Regenerate-swipe does not resolve a cheap-LLM selection (its rebuild is a
        // single continue-mode generation; the recap/distill feeders stay inert, as
        // in its differential).
        cheap_llm_selection: None,
        uncensored_fallback: None,
        // Not an autonomous turn — no per-turn context cap (v4's regenerate path
        // never carries `autonomousContextCap`).
        autonomous_context_cap: None,
        // Regenerate-as-swipe never offers the "nothing to add" pass (v4 passes
        // no `turnSkip`).
        turn_skip: None,
        // Regenerate-swipe does not run the proactive pre-compute distill (P4.19);
        // its rebuild falls through to buildContext's own fallback path, as before.
        pre_searched_memories: None,
        recall_signals: None,
        pre_searched_query_embedding: None,
    });

    // The per-character opaque-anywhere transparency map (v4's wrapper reads each
    // present character's `systemTransparency`; the responder from `character`, the
    // rest from `participant_characters`).
    let mut participant_transparency: std::collections::HashMap<String, Option<bool>> =
        std::collections::HashMap::new();
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
    let cp_created_at = json_str(&character_participant, "createdAt");
    let empty_participants: Vec<Value> = Vec::new();
    let mc_params = MessageContextParams {
        is_multi_character,
        provider: &json_str(&connection_profile, "provider").unwrap_or_default(),
        responding_character_name: &character_name,
        responding_participant_id: &character_participant_id,
        has_history_access: character_participant
            .get("hasHistoryAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        character_participant_created_at: cp_created_at.as_deref(),
        character_system_transparency: character.get("systemTransparency").and_then(Value::as_bool),
        // v4 `23af7146` — the anchor route lives inside `buildMessageContext`,
        // so the swipe path resolves it from the same profile the send path
        // does — including the thinking answer (`97d2fcb5`, bug 85), which
        // only matters for a profile that never chose.
        use_prefill: crate::services::multi_character_prefill::profile_uses_name_prefill_value(
            &connection_profile,
            crate::services::thinking_turn::profile_runs_thinking_turn(
                crate::provider_manifest::Registry::built_in(),
                connection_profile.get("provider").and_then(Value::as_str),
                connection_profile.get("modelName").and_then(Value::as_str),
                connection_profile.get("parameters"),
            ),
        ),
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&empty_participants),
        participant_transparency: &participant_transparency,
    };

    let no_attachments: Vec<Value> = Vec::new();
    let mc_result = message_context::build_message_context(
        db,
        embedding,
        completion,
        executor,
        bc_seams,
        mc_seams,
        &mc_params,
        build_input,
        &previous_messages,
        &no_attachments,
    )
    .await
    .map_err(|e| DbError::Internal(format!("buildMessageContext failed: {e:?}")))?;

    // --- The single generation, READ AS A STREAM (v4 `f564b0de3`, :193-262) ---
    // v4 `d9c5a1c7`: `const params = profileParams(connectionProfile) ?? {}`,
    // which REPLACED `(connectionProfile.parameters || {})`. Two behaviour
    // changes ride the conversion, both v4's: a non-object `parameters` cell
    // (a number, a string) now collapses to `{}` instead of being forwarded
    // verbatim, and an Ollama profile's Max Context is injected as `num_ctx`.
    let params_value =
        crate::cheap_llm::profile_params_value(&connection_profile).unwrap_or_else(|| json!({}));
    // v4 maps each formatted message into the stream request with
    // `{role: lowercased, content, name, attachments, toolCallId, toolCalls}`
    // (`regenerate-swipe.service.ts:216-223`) — the SAME map
    // `streaming.service.ts:370-379` applies on the send path. This is a copy of
    // the orchestrator's port of that line, so the two entrances cannot drift.
    //
    // Two shape notes, both pre-existing and both v4-wide rather than
    // swipe-specific: `name` has no `StreamMessage` home for a user/assistant
    // message (no v5 request builder reads one), and a continue-mode swipe
    // builds no tool schemas, so `toolCallId`/`toolCalls` are always absent
    // here. `attachments` DO ride their own message now, verbatim — see the
    // module header's second bullet.
    let messages: Vec<StreamMessage> = mc_result
        .formatted_messages
        .iter()
        .map(|m: &FormattedMsg| match m.role.as_str() {
            "system" => StreamMessage::system(m.content.clone()),
            "assistant" => StreamMessage::Assistant {
                content: m.content.clone(),
                tool_calls: Vec::<ToolCallPayload>::new(),
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
    // P4.D83 (v4 `d89babc4`): `...resolveSamplingParams(params)` replaced three
    // hand-rolled casts here. Two changes ride the conversion, both v4's: `top_p`
    // is now read at all (it never was on this path), and a knob the profile
    // stores under the camelCase spelling is honoured. The absent-`max_tokens`
    // arm also stops lying — v5 collapsed it to `0`, which reached the wire as a
    // literal `max_tokens: 0` where v4 leaves the key to the provider's default.
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&params_value));
    let provider = json_str(&connection_profile, "provider").unwrap_or_default();
    let base_url = json_str(&connection_profile, "baseUrl");
    let model = json_str(&connection_profile, "modelName").unwrap_or_default();

    // --- Beat 2 of 4: `sending` (v4 :195-201) ---
    // v4 emits this BEFORE `createLLMProvider`, i.e. before the stream opens.
    progress.status(
        "sending",
        format!("Regenerating — sending to {character_name}..."),
        &character_name,
        &character_id,
    );

    let params = StreamParams {
        messages,
        model: model.clone(),
        temperature: sampling.temperature,
        max_tokens: sampling.max_tokens.map(|f| f as i64),
        top_p: sampling.top_p,
        // A swipe is one alternative line: no tools, no native web search, no
        // Responses-API chaining, no stop sequences. v4's request object carries
        // none of those keys (:213-231).
        tools: None,
        web_search_enabled: false,
        profile_parameters: Some(params_value),
        // The P4.95 per-character prompt-cache key — v4 `cacheKey: character.id`.
        cache_key: Some(character_id.clone()),
        previous_response_id: None,
        stop: Vec::new(),
        // A visible regeneration, not cheap-LLM work: v4 sets no
        // `requestTimeoutMs` here (and sets none on any streaming call).
        request_timeout_ms: None,
    };

    let rx = streaming
        .stream_message(&provider, base_url.as_deref(), &params)
        .await;
    // The stall watchdog is NOT optional on any `stream_message` consumer (bug
    // 141) — v4 says so in this very function. The budgets are the Salon's
    // defaults (240 s / 120 s); the `context` is v4's own
    // `'regenerate-swipe.service'`, NOT the funnel's `'streaming.service'`,
    // because v4's swipe passes its own `logContext` (:233-241). That pairing —
    // default budgets, own context — is a third class in
    // `stream_watchdog_wrap_census`; the greeting is the first (its own 90/60).
    let mut rx = watch_stream(
        rx,
        StallBudgets::default(),
        StallWatchdogContext {
            provider: &provider,
            model_name: &model,
            context: "regenerate-swipe.service",
            // v4's five `logContext` fields, all present on this call.
            user_id: Some(&user_id),
            chat_id: Some(&chat_id),
            character_id: Some(&character_id),
            message_id: Some(&target_message_id),
        },
    );

    let mut content = String::new();
    let mut usage: Option<StreamUsage> = None;
    let mut raw_response: Option<Value> = None;
    let mut reasoning_content: Option<String> = None;
    let mut thought_signature: Option<String> = None;
    let mut announced_streaming = false;
    let mut stream_error: Option<String> = None;

    while let Some(item) = rx.recv().await {
        match item {
            Ok(chunk) => {
                // v4 `if (chunk.content)` — JS truthiness, so an empty delta
                // neither announces nor appends nor reports.
                if !chunk.content.is_empty() {
                    // --- Beat 3 of 4: `regenerating`, ONCE (v4 :246-253) ---
                    // Guarded by `announcedStreaming`, and fired on the FIRST
                    // chunk carrying content — not on every content chunk, and
                    // not on a reasoning-only or usage-only chunk.
                    if !announced_streaming {
                        announced_streaming = true;
                        progress.status(
                            "regenerating",
                            format!("Regenerating {character_name}'s reply..."),
                            &character_name,
                            &character_id,
                        );
                    }
                    content.push_str(&chunk.content);
                    progress.emit(RegenerateSwipeProgress::Delta {
                        content: chunk.content.clone(),
                    });
                }
                // Reasoning arrives CUMULATIVELY — keep the latest, never
                // concatenate (v4 :254-257). v4's `if (chunk.reasoningContent)`
                // is JS-truthy, so an empty string does NOT clear what came
                // before.
                if let Some(r) = chunk.reasoning_content.as_deref().filter(|r| !r.is_empty()) {
                    reasoning_content = Some(r.to_string());
                    progress.emit(RegenerateSwipeProgress::Reasoning {
                        reasoning: r.to_string(),
                    });
                }
                // The three record fields, last-wins (v4 :258-260). Each is a
                // JS-truthy test, which matters for `rawResponse`: v4's own
                // canned stream sends `{}`, and an EMPTY OBJECT IS TRUTHY, so it
                // is stored.
                if let Some(u) = chunk.usage {
                    usage = Some(u);
                }
                if js_truthy(chunk.raw_response.as_ref()) {
                    raw_response = chunk.raw_response.clone();
                }
                if let Some(ts) = chunk.thought_signature.as_deref().filter(|s| !s.is_empty()) {
                    thought_signature = Some(ts.to_string());
                }
            }
            Err(e) => {
                // v4's `for await` THROWS here, which unwinds the whole function
                // — so the swipe is all-or-nothing: no partial row is persisted,
                // no swipe-group write happens, and the memory cascade never
                // runs. A stall (bug 141) arrives as exactly this, carrying the
                // watchdog's own sentence.
                stream_error = Some(e.message);
                break;
            }
        }
    }
    if let Some(message) = stream_error {
        return Err(RegenError::Generation(message));
    }

    // --- Beat 4 of 4: `saving` (v4 :267-273) ---
    // After the loop, before any persistence. The one beat whose sentence names
    // no character.
    progress.status(
        "saving",
        "Regenerating — filing the new line...".to_string(),
        &character_name,
        &character_id,
    );

    // --- Persist the grouping (v4 155–194) ---
    let target_swipe_group_id = json_str(&target_message, "swipeGroupId");
    let swipe_group_id = match &target_swipe_group_id {
        Some(g) if !g.is_empty() => g.clone(),
        _ => format!("swipe-{target_message_id}"),
    };
    // The original anchors the group at index 0; on the first regeneration its
    // swipeGroupId must be written back (else the variant renders as a stray,
    // mis-attributed message).
    if target_swipe_group_id.as_deref().unwrap_or("").is_empty() {
        let write_chat_id = chat_id.clone();
        let write_target_id = target_message_id.clone();
        let updates = json!({ "swipeGroupId": swipe_group_id, "swipeIndex": 0 });
        db.write(move |w| {
            w.main()
                .chat_messages()
                .update_message(&write_chat_id, &write_target_id, &updates)
                .map(|_| ())
        })
        .await?;
    }

    let new_swipe_index = all_messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && (json_str(m, "swipeGroupId").as_deref() == Some(swipe_group_id.as_str())
                    || json_str(m, "id").as_deref() == Some(target_message_id.as_str()))
        })
        .map(|m| json_f64(m, "swipeIndex").unwrap_or(0.0) as i64)
        .max()
        .unwrap_or(0)
        + 1;

    let new_swipe_id = uuid::Uuid::new_v4().to_string();
    let mut swipe = Map::new();
    swipe.insert("type".into(), json!("message"));
    swipe.insert("id".into(), json!(new_swipe_id));
    swipe.insert("role".into(), json!("ASSISTANT"));
    // The accumulated deltas (v4's `content` local), not a response field.
    swipe.insert("content".into(), json!(content));
    // Attribute to the same participant that authored the original (the fix for
    // the regenerated-message-shows-the-wrong-character bug).
    swipe.insert("participantId".into(), json!(character_participant_id));
    swipe.insert("swipeGroupId".into(), json!(swipe_group_id));
    swipe.insert("swipeIndex".into(), json!(new_swipe_index));
    // All six fields ride the CHUNKS now (v4 `f564b0de3`, :304-311): the usage
    // triple off the last chunk that carried a `usage`, and the three below
    // last-wins off their own JS-truthy tests. `?? null` on each, so a stream
    // that carried none writes NULL exactly as before.
    swipe.insert(
        "tokenCount".into(),
        usage
            .as_ref()
            .map_or(Value::Null, |u| json!(u.total_tokens)),
    );
    swipe.insert(
        "promptTokens".into(),
        usage
            .as_ref()
            .map_or(Value::Null, |u| json!(u.prompt_tokens)),
    );
    swipe.insert(
        "completionTokens".into(),
        usage
            .as_ref()
            .map_or(Value::Null, |u| json!(u.completion_tokens)),
    );
    // v4 stores the LAST chunk's `rawResponse` VERBATIM — not a synthesized
    // value, and not the first chunk's.
    swipe.insert("rawResponse".into(), raw_response.unwrap_or(Value::Null));
    swipe.insert(
        "reasoningContent".into(),
        reasoning_content.map_or(Value::Null, Value::from),
    );
    swipe.insert(
        "thoughtSignature".into(),
        thought_signature.map_or(Value::Null, Value::from),
    );
    swipe.insert("provider".into(), json!(provider));
    swipe.insert(
        "modelName".into(),
        json!(json_str(&connection_profile, "modelName")),
    );
    swipe.insert("attachments".into(), json!([]));
    // Keep the original's timestamp so the group stays in place in the transcript.
    swipe.insert(
        "createdAt".into(),
        json!(json_str(&target_message, "createdAt")),
    );
    let new_swipe = Value::Object(swipe);

    let write_chat_id = chat_id.clone();
    let event: crate::db::chats_messages::ChatEventInput =
        serde_json::from_value(new_swipe.clone())
            .map_err(|e| DbError::Internal(format!("swipe message marshal: {e}")))?;
    db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
        .await?;
    // v4 calls `repos.chats.update(chat.id, {})` (an empty patch — a no-op SET; the
    // metadata bump already happened inside `addMessage`).
    let write_chat_id = chat_id.clone();
    db.write(move |w| {
        w.main()
            .chats()
            .update(&write_chat_id, &crate::db::chats::ChatUpdate::default())
            .map(|_| ())
    })
    .await?;

    // --- Memory cascade (v4 196–212): the replaced variant may have seeded
    // memories. Error-swallowed (v4 logs a warn, keeps the swipe). ---
    let cascade_action = chat_settings
        .as_ref()
        .and_then(|s| s.get("memoryCascadePreferences"))
        .and_then(|m| m.get("onSwipeRegenerate"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("DELETE_MEMORIES")
        .to_string();
    if cascade_action != "KEEP_MEMORIES" {
        let cascade: Result<(), DbError> = async {
            let tid = target_message_id.clone();
            let count =
                db.read_main(move |c| memories_read::count_by_source_message_id(c, &tid))?;
            if count > 0 {
                delete_memories_by_source_message_with_vectors(db, &target_message_id).await?;
            }
            Ok(())
        }
        .await;
        // v4 swallows + logs a warn; the swipe is kept regardless.
        let _ = cascade;
    }

    Ok(new_swipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::EventPayload;

    fn channel() -> (
        tokio::sync::broadcast::Sender<Event>,
        tokio::sync::broadcast::Receiver<Event>,
    ) {
        tokio::sync::broadcast::channel(64)
    }

    /// **The `status` frame carries `kind`, and that is the measured shape.**
    ///
    /// v4's route passes the WHOLE progress event to `encodeStatusEvent(encoder,
    /// event)`, which does `JSON.stringify({ status })` — so the service's
    /// `{kind, stage, message, characterName, characterId}` literal reaches the
    /// client with `kind` first and intact. The P4.D207 order's §S.2 paragraph
    /// spells the frame WITHOUT `kind`; the hunks win (§R.4), and this is the
    /// pin that says so. Dropping `"kind": "status"` from `to_v4_frame` reddens
    /// this, and so does re-ordering any of the five keys.
    #[test]
    fn the_status_frame_is_v4s_bytes_including_kind() {
        let f = RegenerateSwipeProgress::Status {
            stage: "gathering",
            message: "Regenerating — gathering Bertie's memories and context...".to_string(),
            character_name: "Bertie".to_string(),
            character_id: "bb-1".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&f.to_v4_frame()).unwrap(),
            r#"{"status":{"kind":"status","stage":"gathering","message":"Regenerating — gathering Bertie's memories and context...","characterName":"Bertie","characterId":"bb-1"}}"#
        );
    }

    /// The two content frames, v4's `encodeContentChunk` / `encodeReasoningChunk`
    /// exactly — one key each, and NOT wrapped in anything.
    #[test]
    fn the_delta_and_reasoning_frames_are_single_key_objects() {
        assert_eq!(
            serde_json::to_string(
                &RegenerateSwipeProgress::Delta {
                    content: "the new ".to_string()
                }
                .to_v4_frame()
            )
            .unwrap(),
            r#"{"content":"the new "}"#
        );
        assert_eq!(
            serde_json::to_string(
                &RegenerateSwipeProgress::Reasoning {
                    reasoning: "thinking about it".to_string()
                }
                .to_v4_frame()
            )
            .unwrap(),
            r#"{"reasoning":"thinking about it"}"#
        );
    }

    /// The two TERMINAL frames the route owns (`route.ts:347-350` and
    /// `:356-364`), byte-exact including v4's two fixed sentences.
    #[test]
    fn the_terminal_frames_are_v4s_bytes() {
        let (tx, mut rx) = channel();
        let e = SwipeProgressEmitter::active("msg-1", tx);
        e.emit_done(&json!({ "id": "swipe-1", "content": "the new line" }));
        e.emit_error("provider went quiet");
        let done = rx.try_recv().expect("a done frame");
        let err = rx.try_recv().expect("an error frame");
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"progressId":"msg-1","type":"swipeProgress","frame":{"done":true,"message":{"id":"swipe-1","content":"the new line"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&err).unwrap(),
            r#"{"progressId":"msg-1","type":"swipeProgress","frame":{"error":"Failed to generate alternative response","errorType":"regenerate_failed","details":"provider went quiet"}}"#
        );
    }

    /// The envelope: `progressId` is the TARGET message id (§S.2), and the
    /// payload is recognisable as its own family.
    #[test]
    fn a_frame_is_scope_tagged_by_the_target_message_id() {
        let (tx, mut rx) = channel();
        SwipeProgressEmitter::active("target-msg", tx).emit(RegenerateSwipeProgress::Delta {
            content: "x".to_string(),
        });
        let ev = rx.try_recv().expect("a frame");
        assert_eq!(ev.progress_id.as_deref(), Some("target-msg"));
        assert!(ev.chat_id.is_none() && ev.room_id.is_none());
        assert!(matches!(ev.payload, EventPayload::SwipeProgress(_)));
    }

    /// v4's own invariant: *with no callback the generation is identical, just
    /// silent.* An inert emitter publishes NOTHING — not a beat, not a delta,
    /// not the terminal frames. Dropping either `let Some(active) = … else {
    /// return }` guard reddens this.
    #[test]
    fn an_inert_emitter_publishes_nothing_at_all() {
        let (tx, mut rx) = channel();
        // The three ways to end up inert: the flag off, an empty target id, and
        // the `Default`.
        for e in [
            SwipeProgressEmitter::from_flag(false, "msg-1", tx.clone()),
            SwipeProgressEmitter::from_flag(true, "", tx.clone()),
            SwipeProgressEmitter::default(),
        ] {
            assert!(!e.is_active());
            e.status("gathering", "hi".to_string(), "Bertie", "bb-1");
            e.emit(RegenerateSwipeProgress::Delta {
                content: "x".to_string(),
            });
            e.emit(RegenerateSwipeProgress::Reasoning {
                reasoning: "r".to_string(),
            });
            e.emit_done(&json!({}));
            e.emit_error("boom");
        }
        assert!(
            rx.try_recv().is_err(),
            "an inert emitter published a frame — v4's absent `onProgress` emits nothing"
        );
        // …and the positive control, so the test cannot pass by a dead channel.
        SwipeProgressEmitter::from_flag(true, "msg-1", tx).emit_error("boom");
        assert!(
            rx.try_recv().is_ok(),
            "the active control published nothing"
        );
    }

    /// The four beats' sentences, byte-exact, in v4's order — the strings the
    /// Salon's status strip renders. `saving` is the one beat whose sentence
    /// names no character.
    #[test]
    fn the_four_beats_are_v4s_sentences_in_v4s_order() {
        let (tx, mut rx) = channel();
        let e = SwipeProgressEmitter::active("m", tx);
        let name = "Bertie";
        e.status(
            "gathering",
            format!("Regenerating — gathering {name}'s memories and context..."),
            name,
            "bb-1",
        );
        e.status(
            "sending",
            format!("Regenerating — sending to {name}..."),
            name,
            "bb-1",
        );
        e.status(
            "regenerating",
            format!("Regenerating {name}'s reply..."),
            name,
            "bb-1",
        );
        e.status(
            "saving",
            "Regenerating — filing the new line...".to_string(),
            name,
            "bb-1",
        );
        let mut got = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let EventPayload::SwipeProgress(p) = &ev.payload {
                let s = p.frame.get("status").expect("a status frame");
                got.push((
                    s.get("stage").and_then(Value::as_str).unwrap().to_string(),
                    s.get("message")
                        .and_then(Value::as_str)
                        .unwrap()
                        .to_string(),
                ));
            }
        }
        assert_eq!(
            got,
            vec![
                (
                    "gathering".to_string(),
                    "Regenerating — gathering Bertie's memories and context...".to_string()
                ),
                (
                    "sending".to_string(),
                    "Regenerating — sending to Bertie...".to_string()
                ),
                (
                    "regenerating".to_string(),
                    "Regenerating Bertie's reply...".to_string()
                ),
                (
                    "saving".to_string(),
                    "Regenerating — filing the new line...".to_string()
                ),
            ]
        );
    }

    /// v4's `if (chunk.rawResponse)` is a JS-truthy test, and its own canned
    /// stream sends `{}` — **an empty object is TRUTHY**, so it IS stored. The
    /// `null` / `false` / `0` / `""` arms are what make `?? null` reachable.
    #[test]
    fn raw_response_uses_js_truthiness_so_an_empty_object_is_kept() {
        assert!(js_truthy(Some(&json!({}))));
        assert!(js_truthy(Some(&json!([]))));
        assert!(js_truthy(Some(&json!({"id": "x"}))));
        assert!(!js_truthy(Some(&Value::Null)));
        assert!(!js_truthy(None));
        assert!(!js_truthy(Some(&json!(false))));
        assert!(!js_truthy(Some(&json!(0))));
        assert!(!js_truthy(Some(&json!(""))));
        assert!(js_truthy(Some(&json!("x"))));
    }

    /// `RegenError::Generation`'s `Display` is v4's RAW `error.message` — the
    /// bytes the SSE `error` frame's `details` and the JSON leg's 500 both
    /// carry. A wrapper like `"swipe generation failed: {…}"` (which v5 had)
    /// matches no v4 byte; this pins that it is gone.
    #[test]
    fn a_generation_failure_renders_v4s_raw_message() {
        let e = RegenError::Generation(
            "Provider stream went quiet for 120000ms after 2 chunk(s)".to_string(),
        );
        assert_eq!(
            e.to_string(),
            "Provider stream went quiet for 120000ms after 2 chunk(s)"
        );
        // The two refusal sentences are unchanged.
        assert_eq!(
            RegenError::NotAssistant.to_string(),
            "Only assistant messages can be regenerated"
        );
        assert_eq!(
            RegenError::StaffMessage.to_string(),
            "Staff and system messages cannot be regenerated"
        );
    }
}
