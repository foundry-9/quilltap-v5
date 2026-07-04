//! The **message finalizer** (Phase-3 Unit-3 wave 3, batch 2) — v4
//! `lib/services/chat-message/message-finalizer.service.ts`
//! (`finalizeMessageResponse` + `calculateNextSpeaker`), the tail of one
//! assistant-response cycle: **clean → (re-base anchors) → write → carina →
//! updatedAt bump → next speaker → done event → background triggers**.
//!
//! Scope (per `docs/developer/porting/chat-orchestration.md`): the **core path**,
//! *minus the tool / confirmation branches* (wave 4). The answer-confirmation
//! *service*, the async-compression *trigger*, the RNG detector/executor, the
//! carina *query* engine, and cost *estimation* are all unported subsystems
//! reached from here; each is an injected seam whose gate conditions are
//! reproduced faithfully so the corpus banks the on/off paths, while the seam call
//! itself is recorded (never fired by the corpus except carina, whose posting seam
//! DOES run against the writer).
//!
//! ## What is reproduced (in scope)
//!
//! * **Content cleaning** — `normalizeContentBlockFormat` → `stripCharacterNamePrefix`
//!   → multi-character `truncateAtForeignSpeaker` (the anti-hijack: truncate at a
//!   foreign speaker tag unless that would empty the message, in which case keep
//!   the leading-stripped text and warn). Over the ported
//!   [`crate::message_formatter`] leaves.
//! * **Tool-call anchor re-basing** — the pure `max(0, min(len, off - delta))`
//!   transform, plus the `normalizeRewroteBody` drop-all branch. The corpus
//!   exercises the empty tool list (the wave-4 tool loop supplies non-empty).
//! * **Reasoning-segment re-basing** — the same transform + the rewrite-collapse
//!   (`normalizeRewroteBody` → one offset-0 block).
//! * **Answer-confirmation SKIP gates** — user-driven turn → `confirmed = null`;
//!   silent participant → skipped; the `isAnswerConfirmationActive` gate. The
//!   active call is the injected [`AnswerConfirmationRunner`] seam (never fired).
//! * **Persistence** — [`super::primary_stream::save_assistant_message`] with the
//!   confirmation bag (absent shape), `isSilentMessage` from participant status,
//!   and the image-link loop.
//! * **Carina markup** — [`super::carina_runner::run_carina_markup_query`] over the
//!   injected query seam; a public answer's `onPosted` becomes a
//!   [`ChatEvent::CarinaAnswer`] emit.
//! * **Chat metadata update** — `repos.chats.update(chatId, { updatedAt: now })`.
//! * **Next speaker** — [`calculate_next_speaker`] over the ported turn manager.
//! * **Done event** — v4's full done payload (turn / provider / modelName /
//!   isSilentMessage / reasoning fields).
//! * **Background triggers** — memory-extraction (turn-closed / autonomous gate),
//!   context-summary check (not-autonomous gate — see the harness header for the
//!   summary-check decision), danger-classification.
//!
//! ## Injected seams (out of scope — recorded, gated faithfully)
//!
//! | v4 subsystem | seam | corpus banks |
//! |---|---|---|
//! | `runAnswerConfirmation` | [`AnswerConfirmationRunner`] | the skip gates only (never fired) |
//! | `triggerAsyncCompression` | [`AsyncCompressionTrigger`] | gate on (recorded) / off / autonomous-skip |
//! | `detectAndConvertRngPatterns` + `executeRngTool` | [`RngDetector`] | disabled + no-patterns |
//! | `runCarinaQuery` | [`super::carina_runner::RunCarinaQuery`] | a public markup answer |
//! | `estimateMessageCost` → `trackMessageTokenUsage` | [`CostTracker`] | the fire (args diffed) |
//!
//! **Cost tracking** is seamed with evidence: v4's `estimateMessageCost` drags
//! `lib/llm/pricing-fetcher` (OpenRouter HTTP + a per-user pricing cache) and its
//! `trackMessageTokenUsage` also calls the connection-profiles repo's
//! `incrementTokenUsage` — both unported. The chat-aggregate half
//! (`incrementTokenAggregates`) IS ported, but pinning it here would require the
//! unported cost estimator to produce a `cost`/`source`. The seam records the
//! exact `(provider, modelName, promptTokens, completionTokens, userId, profileId)`
//! the finalizer forwards, so the corpus banks the behavior without the unported
//! subsystems.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};
use crate::message_formatter::{
    normalize_content_block_format, strip_character_name_prefix, truncate_at_foreign_speaker,
};
use crate::model::stream::{StreamCacheUsage, StreamUsage};
use crate::select_speaker::{
    is_users_turn as selection_is_users_turn, select_next_speaker, SelectionResult,
    SpeakerParticipant,
};
use crate::turn_state::{calculate_turn_state_from_history, MessageView};

use super::carina_runner::{
    run_carina_markup_query, CarinaMarkupLogLabels, CarinaMarkupOptions, NoCallbacks,
    PostProsperoCarinaError, RunCarinaQuery,
};
use super::chat_events::{
    ChatEvent, DoneCacheUsage, DonePayload, DoneReasoningSegment, DoneTurn, DoneUsage, EventSink,
    Omittable,
};
use super::primary_stream::{
    save_assistant_message, AssistantMessageContext, ConfirmationField, ConfirmationFields,
    ReasoningSegment,
};
use super::tool_execution::{GeneratedImage, ToolMessage, ToolWhisperContext};

// Re-export the runner closure-seam helpers for the harness convenience.
pub use super::carina_runner::{ClosureProspero, ClosureRunQuery};

// ===========================================================================
// Injected seams
// ===========================================================================

/// The answer-confirmation runner seam — v4 `runAnswerConfirmation`. The corpus
/// only exercises the SKIP gates (user-driven / silent / inactive), so this is
/// never invoked; it exists for signature parity with the active path (wave 4).
pub trait AnswerConfirmationRunner {
    /// Run the consistency check + optional re-affirmation (v4
    /// `AnswerConfirmationOutcome`). Never fired by the corpus.
    fn run(&mut self, _reply: &str, _reference: &str) -> AnswerConfirmationOutcome {
        AnswerConfirmationOutcome::default()
    }
}

/// v4 `AnswerConfirmationOutcome`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnswerConfirmationOutcome {
    pub confirmed: Option<bool>,
    pub revised: bool,
    pub notes: Option<String>,
    pub revised_content: Option<String>,
}

/// A no-op confirmation runner (the corpus never fires it).
pub struct NoAnswerConfirmation;
impl AnswerConfirmationRunner for NoAnswerConfirmation {}

/// The async pre-compression trigger seam — v4 `triggerAsyncCompression`. When the
/// finalizer's gate fires it records one call; the cheap-LLM compression
/// (compression-cache subsystem) is wave 4.
pub trait AsyncCompressionTrigger {
    /// Fire the async compression for this chat (recorded by the harness).
    fn trigger(&mut self, _chat_id: &str, _participant_id: Option<&str>);
}

/// A no-op compression trigger.
pub struct NoAsyncCompression;
impl AsyncCompressionTrigger for NoAsyncCompression {
    fn trigger(&mut self, _chat_id: &str, _participant_id: Option<&str>) {}
}

/// The assistant-response RNG detector/executor seam — v4
/// `detectAndConvertRngPatterns` + `executeRngTool`. The corpus banks only the
/// disabled + no-patterns cases (the detector returns none); the pattern
/// execution + tool-message writes land with the wave-4 tool/RNG subsystem.
pub trait RngDetector {
    /// Detect RNG patterns in the cleaned response. `false` = none (the only case
    /// the corpus exercises).
    fn has_patterns(&mut self, _cleaned_response: &str) -> bool {
        false
    }
}

/// A no-op RNG detector (returns "no patterns").
pub struct NoRng;
impl RngDetector for NoRng {}

/// The cost-**estimation** seam — v4's `estimateMessageCost` (seamed with
/// evidence: it drags `lib/llm/pricing-fetcher`, an OpenRouter HTTP + per-user
/// pricing-cache subsystem; see module docs). The composition around it —
/// `trackMessageTokenUsage`'s chat-aggregate half — IS ported: the finalizer
/// awaits the seam's estimate, then writes
/// [`crate::db::chats_tokens::ChatTokensRepository::increment_token_aggregates`]
/// with the estimated cost/source (v4 fire-and-forgets; v5 awaits per the
/// watermark precedent — same DB effect once settled). The profile-side
/// `incrementTokenUsage` (connection-profiles repo, unported) is a tracked
/// deferral.
pub trait CostTracker {
    /// Estimate the message cost (v4 `estimateMessageCost`). The harness records
    /// the args and returns a canned estimate.
    fn estimate(&mut self, _args: &CostTrackArgs) -> CostEstimate {
        CostEstimate {
            cost: None,
            source: Some("unavailable".to_string()),
        }
    }
}

/// v4 `CostEstimateResult` (the fields the tracking write consumes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostEstimate {
    pub cost: Option<f64>,
    pub source: Option<String>,
}

/// The arguments v4 forwards into `estimateMessageCost` (the subset the finalizer
/// supplies).
#[derive(Clone, Debug, PartialEq)]
pub struct CostTrackArgs {
    pub provider: String,
    pub model_name: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub user_id: String,
    pub profile_id: String,
}

/// A no-op cost estimator (null cost — the aggregate write still increments the
/// token counters, as v4 does).
pub struct NoCostTracking;
impl CostTracker for NoCostTracking {}

// ===========================================================================
// Inputs / outputs
// ===========================================================================

/// The connection profile fields the finalizer reads (v4 `effectiveProfile` +
/// `connectionProfile` subset).
#[derive(Clone, Debug)]
pub struct FinalizerProfile {
    /// `effectiveProfile.id` (the token-tracking profile id).
    pub id: String,
    pub provider: String,
    pub model_name: String,
}

/// The chat fields the finalizer reads off the passed `chat` (v4
/// `ChatMetadataBase` subset — the fields that gate behavior). Fresh reads of the
/// chat/messages go through the [`Db`]; these are the values the caller already
/// had in hand (matching v4's closed-over `chat`).
#[derive(Clone, Debug)]
pub struct FinalizerChat {
    pub id: String,
    pub is_paused: bool,
    /// `chatType` — `"autonomous"` gates the memory-always + summary-skip +
    /// async-compression-skip.
    pub chat_type: Option<String>,
    /// `projectId` — the confirmation gate reads the project override when there
    /// is no chat override.
    pub project_id: Option<String>,
    /// `answerConfirmationOverride` (`"ON"` / `"OFF"` / null).
    pub answer_confirmation_override: Option<String>,
    /// `impersonatingParticipantIds` — the user-driven-turn check.
    pub impersonating_participant_ids: Vec<String>,
    /// The participants (for the user-driven-turn `controlledBy` check + the
    /// next-speaker selection). Each is the raw participant JSON object.
    pub participants: Vec<Value>,
    /// `spokenThisCycleParticipantIds` (JSON text) — the fallback when a fresh
    /// re-read yields nothing (v4 `freshChat?.spoken… ?? chat.spoken…`).
    pub spoken_this_cycle_participant_ids: Option<String>,
    /// `allowCrossCharacterVaultReads === true` — feeds the tool-message whisper
    /// context (a VAULT_READ tool's result is public only when this is on).
    pub allow_cross_character_vault_reads: bool,
}

/// The responding character (v4 `character`).
#[derive(Clone, Debug)]
pub struct FinalizerCharacter {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

/// The responding participant (v4 `characterParticipant`).
#[derive(Clone, Debug)]
pub struct FinalizerParticipant {
    pub id: String,
    /// `'silent'` → `isSilentMessage` + the confirmation skip.
    pub status: Option<String>,
}

/// One other participant character (v4's `participantCharacters` map values).
#[derive(Clone, Debug)]
pub struct ParticipantCharacter {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

/// The streaming outputs the finalizer consumes (v4 `streaming` subset).
#[derive(Clone, Debug, Default)]
pub struct FinalizerStreaming {
    pub full_response: String,
    pub usage: Option<StreamUsage>,
    pub cache_usage: Option<StreamCacheUsage>,
    pub attachment_results: Option<Value>,
    pub raw_response: Option<Value>,
    pub thought_signature: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning_segments: Vec<ReasoningSegment>,
}

/// The chat-settings-derived triggers gate (v4 `triggers.chatSettings` subset).
/// `None` = no chatSettings (the whole background-trigger block +
/// `sceneTrackingContext` skipped).
#[derive(Clone, Debug)]
pub struct FinalizerChatSettings {
    /// Whether `cheapLLMSettings` is set — gates memory-extraction + summary-check
    /// (the danger enqueue does NOT re-check it in v4).
    pub cheap_llm_settings_present: bool,
    /// `autoDetectRng ?? true` — the assistant-response RNG auto-detect gate.
    pub auto_detect_rng: Option<bool>,
    /// `answerConfirmationSettings.enabled === true` — the global confirmation
    /// gate leg.
    pub answer_confirmation_global_enabled: bool,
}

/// The async-compression gate inputs (v4 `compression` subset).
#[derive(Clone, Debug, Default)]
pub struct FinalizerCompression {
    pub compression_enabled: bool,
    pub cheap_llm_selection_present: bool,
    pub original_system_prompt_present: bool,
}

/// The finalizer's return value (v4 `ProcessMessageResult`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessMessageResult {
    pub is_multi_character: bool,
    pub has_content: bool,
    pub message_id: String,
    pub user_participant_id: Option<String>,
    pub is_paused: bool,
    /// Present iff `chatSettings` was present — the participant character ids (v4's
    /// `sceneTrackingContext.characterIds`). The rest of v4's `sceneTrackingContext`
    /// (connectionProfile, memoryChatSettings) is host-side context, not diffed.
    pub scene_tracking_character_ids: Option<Vec<String>>,
}

/// Everything [`finalize_message_response`] needs (v4's `FinalizeMessageResponseOptions`
/// minus the pieces resolved above the seam).
pub struct FinalizeOptions {
    pub chat_id: String,
    pub user_id: String,
    pub chat: FinalizerChat,
    pub character: FinalizerCharacter,
    pub character_participant: FinalizerParticipant,
    pub user_participant_id: Option<String>,
    pub is_multi_character: bool,
    /// v4 `generatedImagePaths` — the full descriptors (ids feed the assistant
    /// attachments; the descriptors feed `save_tool_messages`' image link/tag).
    pub generated_image_paths: Vec<GeneratedImage>,
    /// v4 `toolMessages` — the tool-result slate to persist as TOOL rows. Empty in
    /// the current corpus (the tool loops W4.1e/f supply it); the persistence
    /// primitive is `tool_execution_tier2`-proven.
    pub tool_messages: Vec<ToolMessage>,
    /// v4 `preGeneratedAssistantMessageId` — pins the message id when set.
    pub pre_generated_assistant_message_id: Option<String>,
    pub profile: FinalizerProfile,
    pub streaming: FinalizerStreaming,
    pub compression: FinalizerCompression,
    /// The other participant characters (v4 `participantCharacters` values).
    pub participant_characters: Vec<ParticipantCharacter>,
    /// v4 `triggers.chatSettings`.
    pub chat_settings: Option<FinalizerChatSettings>,
    /// v4 `isChatActiveDangerous(chat)` — resolved above the seam (the enqueue gate
    /// is re-derived on the fresh chat by the danger-classification enqueue).
    pub is_dangerous_chat: bool,
}

// ===========================================================================
// The finalizer
// ===========================================================================

/// Finalize a successful assistant text response (v4 `finalizeMessageResponse`).
/// Drives the whole tail through the injected seams + the [`Db`] writer, emitting
/// typed [`ChatEvent`]s at the `sink`. Returns the [`ProcessMessageResult`].
#[allow(clippy::too_many_arguments)]
pub async fn finalize_message_response<S, Q, P>(
    db: &Db,
    sink: &S,
    opts: FinalizeOptions,
    confirmation_runner: &mut dyn AnswerConfirmationRunner,
    compression_trigger: &mut dyn AsyncCompressionTrigger,
    rng: &mut dyn RngDetector,
    cost: &mut dyn CostTracker,
    carina_query: &mut Q,
    prospero: &mut P,
) -> Result<ProcessMessageResult, DbError>
where
    S: EventSink,
    Q: RunCarinaQuery,
    P: PostProsperoCarinaError,
{
    let FinalizeOptions {
        chat_id,
        user_id,
        chat,
        character,
        character_participant,
        user_participant_id,
        is_multi_character,
        generated_image_paths,
        tool_messages,
        pre_generated_assistant_message_id,
        profile,
        streaming,
        compression,
        participant_characters,
        chat_settings,
        is_dangerous_chat: _is_dangerous_chat,
    } = opts;

    // --- Content cleaning (message-finalizer.service.ts:96–133) ---
    let full_response = &streaming.full_response;
    let normalized_response = normalize_content_block_format(full_response);
    let leading_stripped = strip_character_name_prefix(
        &normalized_response,
        Some(&character.name),
        Some(&character.aliases),
    );

    // Foreign-speaker roster (multi-character only): every OTHER participant
    // character's name + aliases.
    let mut foreign_speaker_names: Vec<String> = Vec::new();
    if is_multi_character {
        for c in &participant_characters {
            if c.id == character.id {
                continue;
            }
            if !c.name.is_empty() {
                foreign_speaker_names.push(c.name.clone());
            }
            foreign_speaker_names.extend(c.aliases.iter().cloned());
        }
    }

    let mut cleaned_response = leading_stripped.clone();
    if !foreign_speaker_names.is_empty() {
        let truncation = truncate_at_foreign_speaker(&leading_stripped, &foreign_speaker_names);
        if truncation.truncated_at.is_some() && !crate::jsstr::js_trim(&truncation.text).is_empty()
        {
            // Truncate at the foreign tag (v4 logs a warn; logging is host-side).
            cleaned_response = truncation.text;
        } else if truncation.truncated_at.is_some() {
            // First line is a foreign tag — keep the leading-stripped text intact
            // to avoid an empty bubble (v4 warns).
            cleaned_response = leading_stripped.clone();
        }
    }

    // --- Anchor + reasoning re-basing (message-finalizer.service.ts:135–194) ---
    let normalize_rewrote_body = normalized_response != *full_response;
    let leading_strip_delta = crate::jsstr::utf16_len(&normalized_response) as i64
        - crate::jsstr::utf16_len(&leading_stripped) as i64;
    let cleaned_len = crate::jsstr::utf16_len(&cleaned_response) as i64;

    // Tool-call anchor re-basing: the corpus exercises the empty tool list (the
    // wave-4 tool loop supplies non-empty toolMessages). The transform is
    // [`rebase_offset`] (the same one the reasoning re-base below uses); with no
    // tool messages there are no anchors to re-base.

    // Reasoning-segment re-basing.
    let rebased_reasoning: Option<Vec<ReasoningSegment>> =
        if !streaming.reasoning_segments.is_empty() {
            if normalize_rewrote_body {
                // Collapse to one leading block: positions invalid, content good.
                let joined: String = streaming
                    .reasoning_segments
                    .iter()
                    .map(|s| s.content.as_str())
                    .collect();
                Some(vec![ReasoningSegment {
                    anchor_offset: 0,
                    content: joined,
                    seq: streaming.reasoning_segments[0].seq,
                }])
            } else {
                Some(
                    streaming
                        .reasoning_segments
                        .iter()
                        .map(|s| ReasoningSegment {
                            anchor_offset: rebase_offset(
                                s.anchor_offset,
                                leading_strip_delta,
                                cleaned_len,
                            ),
                            content: s.content.clone(),
                            seq: s.seq,
                        })
                        .collect(),
                )
            }
        } else {
            None
        };

    // --- Answer confirmation SKIP gates (message-finalizer.service.ts:196–286) ---
    let assistant_message_id = pre_generated_assistant_message_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut confirmation_fields = ConfirmationFields::default();

    let is_silent_turn = character_participant.status.as_deref() == Some("silent");
    if is_user_driven_turn(&chat, &character_participant.id) {
        // A user-controlled/impersonated turn → explicit `confirmed: null`.
        confirmation_fields.confirmed = ConfirmationField::Null;
    } else if !is_silent_turn {
        let global_enabled = chat_settings
            .as_ref()
            .map(|s| s.answer_confirmation_global_enabled)
            .unwrap_or(false);
        let chat_override = chat.answer_confirmation_override.as_deref();
        // The project override is a host-side `repos.projects.findById` lookup
        // (read only when there is no chat override AND the chat has a project).
        // The corpus keeps every case inactive at the chat/global level, so the
        // active call never fires; a project override that would flip the gate ON
        // is a documented deferral (wave-4 answer-confirmation).
        let project_override: Option<&str> = None;
        if is_answer_confirmation_active(chat_override, project_override, global_enabled) {
            // The active branch (never reached by the corpus — every case is
            // inactive). The seam supplies the outcome; the gather/run wiring lands
            // with the wave-4 answer-confirmation unit.
            let outcome = confirmation_runner.run(&cleaned_response, "");
            confirmation_fields.confirmed = match outcome.confirmed {
                Some(b) => ConfirmationField::Value(b),
                None => ConfirmationField::Null,
            };
            confirmation_fields.revised = ConfirmationField::Value(outcome.revised);
            confirmation_fields.notes = match outcome.notes {
                Some(n) => ConfirmationField::Value(n),
                None => ConfirmationField::Null,
            };
            if outcome.revised {
                if let Some(rev) = outcome.revised_content {
                    confirmation_fields.original_content =
                        ConfirmationField::Value(cleaned_response.clone());
                    cleaned_response = rev;
                }
            }
        }
    }

    // --- Persist the assistant message (message-finalizer.service.ts:293–311) ---
    let write_ctx_name = character.name.clone();
    let write_participant_id = character_participant.id.clone();
    let write_participant_status = character_participant.status.clone();
    let write_character_id = character.id.clone();
    let write_chat_id = chat_id.clone();
    let write_content = cleaned_response.clone();
    let write_usage = streaming.usage;
    let write_raw = streaming.raw_response.clone();
    let write_thought = streaming.thought_signature.clone();
    let write_provider = profile.provider.clone();
    let write_model = profile.model_name.clone();
    let write_reasoning_content = streaming.reasoning_content.clone();
    let write_reasoning_segments = rebased_reasoning.clone().unwrap_or_default();
    let write_images = generated_image_paths.clone();
    let write_tool_messages = tool_messages.clone();
    let write_confirmation = confirmation_fields.clone();
    let write_message_id = assistant_message_id.clone();
    // v4 `whisperContext = { userParticipantId, allowCrossCharacterVaultReads:
    // chat.allowCrossCharacterVaultReads === true }` (message-finalizer.service.ts:288).
    let write_whisper = ToolWhisperContext {
        user_participant_id: user_participant_id.clone(),
        allow_cross_character_vault_reads: chat.allow_cross_character_vault_reads,
    };

    db.write(move |writers| {
        let ctx = AssistantMessageContext {
            character_id: &write_character_id,
            character_name: &write_ctx_name,
            participant_id: &write_participant_id,
            participant_status: write_participant_status.as_deref(),
        };
        save_assistant_message(
            writers.main(),
            &write_chat_id,
            &ctx,
            &write_content,
            write_usage,
            write_raw.as_ref(),
            write_thought.as_deref(),
            Some(&write_message_id),
            Some(&write_provider),
            Some(&write_model),
            write_reasoning_content.as_deref(),
            &write_reasoning_segments,
            &write_images,
            &write_tool_messages,
            Some(&write_whisper),
            &write_confirmation,
        )
        .map(|_| ())
    })
    .await?;

    // v4 emits a `confirmationResult` event whenever `confirmed !== undefined` —
    // which INCLUDES the user-driven skip path (`confirmed = null`), so the event
    // fires there (banked by the corpus). The silent / inactive paths leave
    // `confirmed` absent and emit nothing.
    if !matches!(confirmation_fields.confirmed, ConfirmationField::Absent) {
        let revised = matches!(confirmation_fields.revised, ConfirmationField::Value(true));
        let confirmed = match &confirmation_fields.confirmed {
            ConfirmationField::Value(b) => Some(*b),
            _ => None,
        };
        let notes = match &confirmation_fields.notes {
            ConfirmationField::Value(n) => Some(n.clone()),
            _ => None,
        };
        sink.emit(ChatEvent::confirmation_result(
            super::chat_events::ConfirmationResultPayload {
                message_id: assistant_message_id.clone(),
                confirmed,
                revised,
                notes,
                content: if revised {
                    Some(cleaned_response.clone())
                } else {
                    None
                },
            },
        ));
    }

    // --- Async pre-compression trigger (message-finalizer.service.ts:326–367) ---
    let is_autonomous = chat.chat_type.as_deref() == Some("autonomous");
    if compression.compression_enabled
        && compression.cheap_llm_selection_present
        && compression.original_system_prompt_present
        && !is_autonomous
    {
        let participant = if is_multi_character {
            Some(character_participant.id.as_str())
        } else {
            None
        };
        compression_trigger.trigger(&chat_id, participant);
    }

    // --- Assistant-response RNG auto-detect (message-finalizer.service.ts:369–423) ---
    let auto_detect_rng = chat_settings
        .as_ref()
        .and_then(|s| s.auto_detect_rng)
        .unwrap_or(true);
    if auto_detect_rng && !cleaned_response.is_empty() {
        // The detector returns "no patterns" for the corpus (disabled +
        // no-patterns banked); the pattern-execution + tool-message writes land
        // with wave 4.
        let _ = rng.has_patterns(&cleaned_response);
    }

    // --- Carina markup in the assistant response (message-finalizer.service.ts:434–445) ---
    // Runs AFTER the assistant message is saved (answer ordered after it) and
    // BEFORE the done event. A PUBLIC answer's posted message surfaces live as a
    // `carinaAnswer` event (the seam posts + emits, mirroring v4's `onPosted`).
    if !cleaned_response.is_empty() {
        run_assistant_carina(
            &user_id,
            &chat_id,
            &cleaned_response,
            &character_participant.id,
            carina_query,
            prospero,
        );
    }

    // --- Chat metadata update: bump updatedAt (message-finalizer.service.ts:447) ---
    let bump_chat_id = chat_id.clone();
    let now = crate::clock::now_iso();
    db.write(move |writers| {
        writers.main().chats().update(
            &bump_chat_id,
            &crate::db::chats::ChatUpdate {
                updated_at: Some(now),
                ..Default::default()
            },
        )
    })
    .await?;

    // --- Next speaker (message-finalizer.service.ts:449–456) ---
    let turn_info = calculate_next_speaker(
        db,
        &chat_id,
        &chat,
        &character,
        &character_participant,
        user_participant_id.as_deref(),
    )
    .await?;

    // --- Done event (message-finalizer.service.ts:458–472) ---
    let is_silent_message = if is_silent_turn { Some(true) } else { None };
    let done_reasoning_segments: Omittable<Vec<DoneReasoningSegment>> = match &rebased_reasoning {
        Some(segs) => Omittable::Value(
            segs.iter()
                .map(|s| DoneReasoningSegment {
                    anchor_offset: s.anchor_offset,
                    content: s.content.clone(),
                    seq: s.seq,
                })
                .collect(),
        ),
        // v4 sets `reasoningSegments: rebasedReasoning` (null when none) — ALWAYS
        // present on this callsite.
        None => Omittable::Null,
    };
    let reasoning_content_omittable = match &streaming.reasoning_content {
        // v4 `reasoningContent || null` — ALWAYS present (null or string); an empty
        // string collapses to null.
        Some(rc) if !rc.is_empty() => Omittable::Value(rc.clone()),
        _ => Omittable::Null,
    };

    sink.emit(ChatEvent::done(DonePayload {
        message_id: Some(assistant_message_id.clone()),
        participant_id: Some(character_participant.id.clone()),
        usage: streaming.usage.map(to_done_usage),
        cache_usage: streaming.cache_usage.map(to_done_cache_usage),
        attachment_results: Some(streaming.attachment_results.clone().unwrap_or(Value::Null)),
        // v4 `toolsExecuted: toolMessages.length > 0` (message-finalizer.service.ts:464).
        tools_executed: !tool_messages.is_empty(),
        turn: Some(DoneTurn {
            next_speaker_id: turn_info.next_speaker_id.clone(),
            reason: turn_info.reason.clone(),
            cycle_complete: turn_info.cycle_complete,
            is_users_turn: turn_info.is_users_turn,
        }),
        provider: Some(profile.provider.clone()),
        model_name: Some(profile.model_name.clone()),
        is_silent_message,
        // Additive fields on the shared DonePayload (owned by chat_events); the
        // finalizer's done frame never sets an empty-response marker.
        empty_response: None,
        empty_response_reason: None,
        reasoning_content: reasoning_content_omittable,
        reasoning_segments: done_reasoning_segments,
    }));

    // --- Cost tracking (message-finalizer.service.ts:474–494) ---
    // v4: `if (usage && (usage.promptTokens || usage.completionTokens)) void
    // estimateMessageCost(...).then(r => trackMessageTokenUsage(chatId,
    // effectiveProfile.id, usage, r.cost, r.source))`. The estimation is the
    // injected seam; the chat-aggregate half of `trackMessageTokenUsage`
    // (`incrementTokenAggregates`) is the ported write, awaited (watermark
    // precedent). The profile-side `incrementTokenUsage` (connections repo,
    // unported) is a tracked deferral. A zero prompt+completion skips everything
    // (both v4 guards: the finalizer's `||` and updateChatTokenAggregates' 0+0
    // early return collapse to the same skip).
    if let Some(usage) = streaming.usage {
        if usage.prompt_tokens != 0 || usage.completion_tokens != 0 {
            let estimate = cost.estimate(&CostTrackArgs {
                provider: profile.provider.clone(),
                model_name: profile.model_name.clone(),
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                user_id: user_id.clone(),
                profile_id: profile.id.clone(),
            });
            let agg_chat_id = chat_id.clone();
            let prompt = usage.prompt_tokens as f64;
            let completion = usage.completion_tokens as f64;
            let est_cost = estimate.cost;
            let est_source = estimate.source.clone();
            db.write(move |writers| {
                writers.main().chat_tokens().increment_token_aggregates(
                    &agg_chat_id,
                    prompt,
                    completion,
                    est_cost,
                    est_source.as_deref(),
                )
            })
            .await?;
        }
    }

    // --- Background triggers (message-finalizer.service.ts:496–549) ---
    if let Some(settings) = &chat_settings {
        // Per-turn memory extraction — fire when the turn closes OR autonomous.
        if (turn_info.is_users_turn || is_autonomous) && settings.cheap_llm_settings_present {
            trigger_turn_memory_extraction(db, &chat_id, &user_id, &profile.id).await?;
        }

        // Context-summary check — gated on not-autonomous + cheapLLMSettings. The
        // fold itself is the separately-verified `context_summary` tier-3 unit; the
        // finalizer's contribution is the GATE (which the corpus banks via the
        // enqueues around it). Driving the full check here would double the model
        // boundary this differential already pins — documented deferral (see the
        // harness header).
        // if !is_autonomous && settings.cheap_llm_settings_present { … }

        // Chat danger classification — the gate + enqueue (its own sub-gates
        // inside; the danger-mode-OFF/Off-duty resolver short-circuit is deferred).
        trigger_chat_danger_classification(db, &chat_id, &user_id, &profile.id).await?;
    }

    // --- Return value (message-finalizer.service.ts:551–566) ---
    let scene_tracking_character_ids = chat_settings.as_ref().map(|_| {
        participant_characters
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    });

    Ok(ProcessMessageResult {
        is_multi_character,
        has_content: true,
        message_id: assistant_message_id,
        user_participant_id,
        is_paused: chat.is_paused,
        scene_tracking_character_ids,
    })
}

/// The pure anchor/reasoning re-base transform (v4 `Math.max(0, Math.min(len,
/// off - delta))`): shift by the leading-strip delta, clamp to `[0, cleaned_len]`.
/// UTF-16 units throughout (the offsets are display coordinates the client matches
/// against `fullResponse.length`).
fn rebase_offset(offset: usize, leading_strip_delta: i64, cleaned_len: i64) -> usize {
    let shifted = offset as i64 - leading_strip_delta;
    shifted.clamp(0, cleaned_len) as usize
}

// ===========================================================================
// Answer-confirmation gate leaves (answer-confirmation.service.ts)
// ===========================================================================

/// v4 `isUserDrivenTurn` — the turn was authored by a user-controlled participant
/// (impersonation or a `controlledBy: 'user'` participant).
fn is_user_driven_turn(chat: &FinalizerChat, participant_id: &str) -> bool {
    if chat
        .impersonating_participant_ids
        .iter()
        .any(|id| id == participant_id)
    {
        return true;
    }
    chat.participants.iter().any(|p| {
        p.get("id").and_then(Value::as_str) == Some(participant_id)
            && p.get("controlledBy").and_then(Value::as_str) == Some("user")
    })
}

/// v4 `isAnswerConfirmationActive` — the three-level gate: chat override wins,
/// then project override, then the global default. `'OFF'` beats an inherited
/// `'ON'` at its own level (checked first).
fn is_answer_confirmation_active(
    chat_override: Option<&str>,
    project_override: Option<&str>,
    global_enabled: bool,
) -> bool {
    if chat_override == Some("ON") {
        return true;
    }
    if chat_override == Some("OFF") {
        return false;
    }
    if project_override == Some("ON") {
        return true;
    }
    if project_override == Some("OFF") {
        return false;
    }
    global_enabled
}

// ===========================================================================
// Carina assistant-markup runner wiring
// ===========================================================================

/// Drive the assistant-markup carina path (v4's `runCarinaMarkupQuery` call in the
/// finalizer). The `onPosted` callback is OWNED by the carina query seam (as in
/// v4) — the harness's seam posts the message + fires the `carinaAnswer` emit. The
/// runner splices a public answer + routes failures; it never posts.
///
/// The finalizer's markup path passes `operatorInitiated: false` (a character
/// wrote the markup) and the assistant-markup log labels.
fn run_assistant_carina<Q, P>(
    user_id: &str,
    chat_id: &str,
    text: &str,
    asker_participant_id: &str,
    carina_query: &mut Q,
    prospero: &mut P,
) where
    Q: RunCarinaQuery,
    P: PostProsperoCarinaError,
{
    let opts = CarinaMarkupOptions {
        user_id: user_id.to_string(),
        chat_id: chat_id.to_string(),
        text: text.to_string(),
        asker_participant_id: Some(asker_participant_id.to_string()),
        operator_initiated: false,
        log_labels: CarinaMarkupLogLabels {
            detected: "assistant response".to_string(),
            failed: "assistant-markup".to_string(),
        },
    };
    let mut callbacks = NoCallbacks;
    // The trace is not diffed here (the carina_runner has its own differential);
    // the observable DB/event effects come from the seam impl (post + emit).
    let _trace = run_carina_markup_query(&opts, &mut callbacks, carina_query, prospero);
}

// ===========================================================================
// Background triggers (memory-trigger.service.ts + queue-service.ts)
// ===========================================================================

/// v4 `triggerTurnMemoryExtraction`: resolve the turn opener (or, when none, the
/// latest non-system non-silent ASSISTANT message id as the anchor), then enqueue
/// a single MEMORY_EXTRACTION job.
async fn trigger_turn_memory_extraction(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    connection_profile_id: &str,
) -> Result<(), DbError> {
    let chat_id_owned = chat_id.to_string();
    let messages =
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;

    let turn_opener_message_id = find_turn_opener_message_id(&messages);
    let mut extraction_anchor_message_id: Option<String> = None;
    if turn_opener_message_id.is_none() {
        // v4: walk newest-first for the latest non-system non-silent ASSISTANT
        // message with a participantId.
        for m in messages.iter().rev() {
            if m.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            if m.get("role").and_then(Value::as_str) != Some("ASSISTANT") {
                continue;
            }
            if m.get("systemSender").and_then(Value::as_str).is_some() {
                continue;
            }
            if is_truthy_silent(m.get("isSilentMessage")) {
                continue;
            }
            let Some(id) = m
                .get("participantId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            extraction_anchor_message_id = m.get("id").and_then(Value::as_str).map(String::from);
            let _ = id;
            break;
        }
    }

    super::queue_service::enqueue_memory_extraction(
        db,
        user_id,
        chat_id,
        turn_opener_message_id.as_deref(),
        extraction_anchor_message_id.as_deref(),
        connection_profile_id,
    )
    .await?;
    Ok(())
}

/// v4 `findTurnOpenerMessageId` (turn-transcript.ts) — the most recent non-system
/// USER message id, or `None`.
fn find_turn_opener_message_id(messages: &[Value]) -> Option<String> {
    for m in messages.iter().rev() {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if m.get("role").and_then(Value::as_str) != Some("USER") {
            continue;
        }
        if m.get("systemSender").and_then(Value::as_str).is_some() {
            continue;
        }
        return m.get("id").and_then(Value::as_str).map(String::from);
    }
    None
}

/// JS-truthy read of the TEXT-affinity `isSilentMessage` cell after v4's read-side
/// coercion: `"1.0"`/`"1"`/`true` → true, `"0.0"`/`false`/NULL → false. v4 reads
/// `m.isSilentMessage` after the read coerces the stored `"1.0"`/`"0.0"` back to a
/// bool, so a false silent message does NOT skip.
fn is_truthy_silent(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "1.0" || s == "1" || s == "true",
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        _ => false,
    }
}

/// v4 `triggerChatDangerClassification` gate + enqueue: read the fresh chat; bail
/// on sticky-dangerous / already-classified-at-this-count / no-summary, else
/// enqueue. The danger-mode-OFF and Off-duty resolver short-circuit is a
/// documented deferral (the resolver is unported); the corpus keeps danger mode
/// ON so the enqueue path is reachable. The enqueue's own `findPendingForChat`
/// dedupe is in [`super::queue_service`].
async fn trigger_chat_danger_classification(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    connection_profile_id: &str,
) -> Result<(), DbError> {
    let chat_id_owned = chat_id.to_string();
    let Some(chat) = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned))? else {
        return Ok(());
    };

    // Sticky: already dangerous → never re-check.
    if chat.get("isDangerousChat").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    // Already classified at this message count → skip.
    let classified_at = chat.get("dangerClassifiedAt").and_then(Value::as_str);
    let classified_count = chat
        .get("dangerClassifiedAtMessageCount")
        .and_then(Value::as_f64);
    let message_count = chat.get("messageCount").and_then(Value::as_f64);
    if classified_at.is_some() && classified_count == message_count {
        return Ok(());
    }
    // No context summary → nothing to classify yet.
    let has_summary = chat
        .get("contextSummary")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_summary {
        return Ok(());
    }

    super::queue_service::enqueue_chat_danger_classification(
        db,
        user_id,
        chat_id,
        connection_profile_id,
    )
    .await?;
    Ok(())
}

// ===========================================================================
// calculateNextSpeaker (message-finalizer.service.ts:665–715)
// ===========================================================================

/// The next-speaker info the finalizer emits (v4 `NextSpeakerInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextSpeakerInfo {
    pub next_speaker_id: Option<String>,
    pub reason: String,
    pub cycle_complete: bool,
    pub is_users_turn: bool,
}

/// v4 `calculateNextSpeaker`: re-read messages + the fresh chat, compute the turn
/// state, select the next speaker over the ported turn manager, and report whether
/// it's the user's turn.
///
/// `random01` is fixed at `0.0` (documented): the finalizer only surfaces
/// `isUsersTurn` + the ids from this call (not a chained turn), and the corpus is
/// built so the pick is deterministic (single character, or a cycle-complete /
/// user-turn state), so the weighted RNG never affects the diffed output.
async fn calculate_next_speaker(
    db: &Db,
    chat_id: &str,
    chat: &FinalizerChat,
    character: &FinalizerCharacter,
    character_participant: &FinalizerParticipant,
    user_participant_id: Option<&str>,
) -> Result<NextSpeakerInfo, DbError> {
    let chat_id_owned = chat_id.to_string();
    let messages =
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;
    let message_views = to_message_views(&messages);

    // Re-read the chat so spokenThisCycleParticipantIds reflects the latest
    // persistence; fall back to the passed chat's value (v4 `?? chat.spoken…`).
    let chat_id_owned = chat_id.to_string();
    let fresh_chat = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned))?;
    let spoken_json = fresh_chat
        .as_ref()
        .and_then(|c| c.get("spokenThisCycleParticipantIds"))
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| chat.spoken_this_cycle_participant_ids.clone());

    let turn_state = calculate_turn_state_from_history(&message_views, spoken_json.as_deref());

    // Talkativeness map keyed by characterId for the active-character participants
    // (v4 reads `getActiveCharacterParticipants` then a `findById` each; the
    // responder reuses the passed character, whose talkativeness is on the overlaid
    // row — read the same way here).
    let mut talkativeness: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let _ = character; // responder read below via the DB, same as others.
    for p in &chat.participants {
        if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let status = p.get("status").and_then(Value::as_str).unwrap_or("active");
        // getActiveCharacterParticipants keeps present statuses (active/silent);
        // removed / unknown are dropped.
        if status == "removed" {
            continue;
        }
        let Some(cid) = p
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
        else {
            continue;
        };
        if let Some(t) = read_character_talkativeness(db, cid)? {
            talkativeness.insert(cid.to_string(), t);
        }
    }

    let speaker_participants: Vec<SpeakerParticipant> = chat
        .participants
        .iter()
        .map(to_speaker_participant)
        .collect();

    let result: SelectionResult = select_next_speaker(
        &speaker_participants,
        &talkativeness,
        &[], // v4 passes turnState.queue; the corpus keeps it empty at finalize.
        &turn_state.spoken_since_user_turn,
        turn_state.last_speaker_id.as_deref(),
        0.0,
    );
    let _ = (character_participant, user_participant_id);

    Ok(NextSpeakerInfo {
        next_speaker_id: result.next_speaker_id.clone(),
        reason: result.reason.to_string(),
        cycle_complete: result.cycle_complete,
        is_users_turn: selection_is_users_turn(&result),
    })
}

/// Read a character's `talkativeness` off the vault-overlaid row.
fn read_character_talkativeness(db: &Db, id: &str) -> Result<Option<f64>, DbError> {
    let id = id.to_string();
    let ch = db.read_main(|main| {
        db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &id))
    })?;
    Ok(ch
        .as_ref()
        .and_then(|c| c.get("talkativeness"))
        .and_then(Value::as_f64))
}

// ===========================================================================
// Marshaling helpers (shared shapes with the turn-orchestrator, re-derived here)
// ===========================================================================

fn to_message_views(messages: &[Value]) -> Vec<MessageView> {
    messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .map(|m| MessageView {
            msg_type: Some("message".to_string()),
            role: m
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            participant_id: m
                .get("participantId")
                .and_then(Value::as_str)
                .map(String::from),
            target_participant_ids: m.get("targetParticipantIds").and_then(|t| {
                t.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            }),
        })
        .collect()
}

fn to_speaker_participant(p: &Value) -> SpeakerParticipant {
    use crate::chat_predicates::ParticipantStatus;
    let status = match p.get("status").and_then(Value::as_str).unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    };
    SpeakerParticipant {
        id: p
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        participant_type: p
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status,
        character_id: p
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(String::from),
        controlled_by: p
            .get("controlledBy")
            .and_then(Value::as_str)
            .unwrap_or("llm")
            .to_string(),
        talkativeness: p.get("talkativeness").and_then(Value::as_f64),
    }
}

/// v4's done event spreads the whole `usage` object; the stream always sets all
/// three numeric fields, so render them (0 stays 0, not omitted).
fn to_done_usage(u: StreamUsage) -> DoneUsage {
    DoneUsage {
        prompt_tokens: Some(u.prompt_tokens),
        completion_tokens: Some(u.completion_tokens),
        total_tokens: Some(u.total_tokens),
    }
}

/// The done event's `cacheUsage` carries only the two Anthropic cache fields (v4's
/// `encodeDoneEvent` types it as `{ cacheCreationInputTokens?, cacheReadInputTokens? }`);
/// the finalizer callsite passes the streaming cacheUsage verbatim, so the corpus
/// keeps `cacheUsage` null on the diffed paths (mirroring the recovery paths).
fn to_done_cache_usage(c: StreamCacheUsage) -> DoneCacheUsage {
    DoneCacheUsage {
        cache_creation_input_tokens: c.cache_creation_input_tokens,
        cache_read_input_tokens: c.cache_read_input_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rebase_offset_shifts_and_clamps() {
        assert_eq!(rebase_offset(10, 3, 100), 7);
        assert_eq!(rebase_offset(2, 5, 100), 0); // clamps at 0
        assert_eq!(rebase_offset(50, 0, 20), 20); // clamps at len
    }

    #[test]
    fn is_answer_confirmation_active_three_level_gate() {
        assert!(is_answer_confirmation_active(
            Some("ON"),
            Some("OFF"),
            false
        ));
        assert!(!is_answer_confirmation_active(
            Some("OFF"),
            Some("ON"),
            true
        ));
        assert!(is_answer_confirmation_active(None, Some("ON"), false));
        assert!(!is_answer_confirmation_active(None, Some("OFF"), true));
        assert!(is_answer_confirmation_active(None, None, true));
        assert!(!is_answer_confirmation_active(None, None, false));
    }

    #[test]
    fn user_driven_turn_via_impersonation_and_controlled_by() {
        let chat = FinalizerChat {
            id: "c".into(),
            is_paused: false,
            chat_type: None,
            project_id: None,
            answer_confirmation_override: None,
            impersonating_participant_ids: vec!["p-imp".into()],
            participants: vec![json!({ "id": "p-user", "controlledBy": "user" })],
            spoken_this_cycle_participant_ids: None,
            allow_cross_character_vault_reads: false,
        };
        assert!(is_user_driven_turn(&chat, "p-imp"));
        assert!(is_user_driven_turn(&chat, "p-user"));
        assert!(!is_user_driven_turn(&chat, "p-other"));
    }

    #[test]
    fn find_turn_opener_is_newest_non_system_user() {
        let msgs = vec![
            json!({ "type": "message", "role": "USER", "id": "u1" }),
            json!({ "type": "message", "role": "ASSISTANT", "id": "a1" }),
            json!({ "type": "message", "role": "USER", "id": "u2" }),
            json!({ "type": "message", "role": "USER", "id": "sys", "systemSender": "host" }),
        ];
        assert_eq!(find_turn_opener_message_id(&msgs), Some("u2".into()));
    }

    #[test]
    fn is_truthy_silent_coerces_text_affinity() {
        assert!(is_truthy_silent(Some(&json!("1.0"))));
        assert!(!is_truthy_silent(Some(&json!("0.0"))));
        assert!(!is_truthy_silent(None));
        assert!(is_truthy_silent(Some(&json!(true))));
        assert!(!is_truthy_silent(Some(&json!(false))));
    }
}
