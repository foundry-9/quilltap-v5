//! The `buildContext` capstone (Phase-3 Unit-3 wave 3, batch 2) — v4
//! `lib/chat/context-manager.ts:477`, composing the ported context subsystem
//! (system prompt, message selection, compression, memory injection, knowledge
//! retrieval, scene state). Its whisper-posting side effects (core packet,
//! commonplace, mail) and the still-unported feeder subsystems are modeled as an
//! injected [`BuildContextSeams`] trait (default no-ops), exactly as the tier-3
//! oracle jest-mocks them — the STOP-rule precedent set by
//! [`crate::services::context_summary::ContextSummarySeams`] and the carina
//! runner.
//!
//! # Dependency map (ported vs seamed — one line each)
//!
//! **Ported (composed directly):**
//!   - [`crate::system_prompt`] — `build_system_prompt` (block 1),
//!     `build_other_participants_info`, `build_identity_reinforcement` (block 2).
//!   - [`crate::message_selector::select_recent_messages`] — the greedy tail fit.
//!   - [`crate::memory_injector`] — the scene-state / archive / dynamic-head /
//!     inter-character / summary formatters.
//!   - [`crate::context_budget`] — allocation + `calculate_max_available` +
//!     `should_summarize_conversation`; the ratio consts mirror v4's
//!     model-context-data (`CONTEXT_HISTORY_BUDGET_RATIO`/`MEMORY_BUDGET_RATIO`).
//!   - [`crate::context_compression`] sizing + [`super::compression`] service
//!     half (`apply_context_compression`, canned).
//!   - [`super::knowledge_injector::retrieve_knowledge_for_turn`].
//!   - [`super::memory_service::search_memories_semantic`].
//!   - [`crate::message_attribution`] — history-access / whisper / attribution.
//!   - [`crate::token_estimation`] — estimate / count / truncate.
//!   - [`crate::chat_timestamp`] — `should_inject_timestamp` /
//!     `calculate_current_timestamp`.
//!   - [`crate::chat_tasks::extract_visible_conversation`] /
//!     [`crate::chat_tasks::strip_tool_artifacts`].
//!   - [`crate::recall_history::append_recall_turn`] (feeds the recall-history
//!     persistence seam call).
//!   - [`crate::db::memories_read::find_by_character_about_characters`] (the
//!     inter-character importance half) over the [`crate::db::runtime::Db`]
//!     runtime; the semantic/knowledge reads go through the two services above.
//!   - the Commonplace-Book **content builders** (v4
//!     `buildCommonplaceLLMContext` / `buildCommonplacePersonaWhisper`,
//!     `lib/services/commonplace-notifications/writer.ts`) and the Host
//!     timestamp body (`buildTimestampContent`,
//!     `lib/services/host-notifications/writer.ts:617`) — pure string builders
//!     ported here (the *posts* are seams); plus [`SUMMARY_CONTENT_PREFIX`].
//!
//! **Seamed (injected [`BuildContextSeams`], evidence in each method's doc):**
//!   - `resolve_memory_recap` — v4 `generateMemoryRecap` (unported cheap-LLM +
//!     recap subsystem; `lib/memory/memory-recap.ts`).
//!   - `distill_memory_search` — v4 `extractMemorySearchKeywords` (unported
//!     cheap-LLM task, `lib/memory/cheap-llm-tasks`).
//!   - `resolve_mount_pool` — v4 `resolveTieredMountPool` (only its pure dedupe
//!     leaf is ported; the DB-reading group/vault/project resolver drags the
//!     mount-index tier walk).
//!   - `compute_frozen_archive` — v4 `getOrComputeFrozenArchive` (unported
//!     archive cache, `lib/memory/frozen-archive-cache.ts`).
//!   - `resolve_live_clothing` — v4's live-wardrobe outfit resolution
//!     (`resolveEquippedOutfitForCharacter`/`describeOutfit`, the wardrobe
//!     subsystem, wave 4).
//!   - `resolve_off_scene_announcement` — v4's Host off-scene-character
//!     introduction (`postHostOffSceneCharactersAnnouncement`, post-office).
//!   - `read_memory_recall_settings` — v4 `getMemoryRecallSettings`
//!     (instance-settings subsystem).
//!   - the post-office writers (`post_core_whisper` / `post_commonplace_whisper`
//!     / `post_suparna_mail` / `post_host_timestamp_announcement` /
//!     `persist_scene_cache` / `persist_recall_history`) — the whisper-posting
//!     side effects explicitly scoped out per the decomposition doc.
//!
//! # Result-object faithfulness
//!
//! The full [`BuiltContext`] (final message array, token accounting,
//! debug/metadata) is byte-faithful. v4 returns `undefined` for absent optional
//! fields (dropped by `JSON.stringify`), so those are `Option` here and the
//! differential compares the JSON-serialized shape.
//!
//! # Tracked deferrals (each verified a no-op for the corpus, closed later)
//!
//!   - **Phase-2 memory compression** (v4 `compressMemories`,
//!     `lib/memory/cheap-llm-tasks/compression-tasks.ts`): a distinct cheap-LLM
//!     task template not yet ported ([`super::compression`] ports only the
//!     conversation-history compressor). The corpus keeps memory tokens under
//!     `maxAvailable * MEMORY_BUDGET_RATIO`, so v4 takes the "within budget"
//!     else-arm on every op — see the inline note at the compression site.
//!   - The **off-scene Host introduction** (v4 context-manager.ts:583–691): the
//!     whole block — `characters.findByUserId` candidate pool, the
//!     `getMessages` scan corpus, `findMentionedCharacterIds`,
//!     `findIntroducedOffSceneCharacterIds`, and the announcement post — is the
//!     [`BuildContextSeams::resolve_off_scene_announcement`] seam (the post is
//!     post-office; the scan feeds nothing else). The oracle proves the corpus
//!     resolves to "no announcement" through v4's REAL scan (empty chat
//!     transcript → empty corpus → no match); only the *push* of a resolved
//!     announcement into the context messages is ported here. Port the scan
//!     composition with the post-office sub-unit.
//!   - The **core-whisper branch** (v4 context-manager.ts:1788–1860): config
//!     read (`chatSettings.findByUserId` → `resolveCoreWhisperConfig`), the
//!     ported [`crate::core_whisper::should_fire_core_whisper`] trigger, the
//!     `assembleCorePacket` vault read, the post + stale sweep — all behind
//!     [`BuildContextSeams::resolve_core_whisper_context`], because the packet
//!     assembly + post are the post-office/Aurora subsystem. The tier-1-verified
//!     trigger is composed when that sub-unit lands (add the scoped
//!     `chat_settings` `coreWhisper` read then).
//!   - The **scene-state prior-emission cache** read (v4 reads
//!     `chat.commonplaceSceneCache[targetKey]` to collapse unchanged sections):
//!     the prior lookup is pinned to `None` here (the corpus seeds no
//!     `sceneState`), paired with the seamed `persist_scene_cache` write. Both
//!     sides of the cache land with the Commonplace post-office sub-unit.
//!   - **`EVERY_N_MINUTES` gating**: v4 resolves
//!     `minutesSinceLastTimestampAnnouncement` by scanning `getMessages` for the
//!     last Host timestamp announcement; here it is a hoisted input
//!     (`minutes_since_last_timestamp_announcement`) — the scan lands with the
//!     Host post-office sub-unit. The corpus uses `EVERY_MESSAGE`/none.
//!   - **`autonomousContextCap`** (the enclave per-turn clamp) and the
//!     **cached-compression fallback window** (`cachedCompressionResult` /
//!     `cachedCompressionMessageCount`, the async pre-compression cache) are not
//!     wired as inputs yet — both are additive option plumbing on the same
//!     ported budget/window math, scoped to the `processMessage` spine unit.
//!   - The dynamic-head `debugMemories` **recall-adjustment diagnostics**
//!     (`recallMultiplier`/`recallFired`/`blendedBefore`/`blendedAfter`) are
//!     absent here because [`super::memory_service::search_memories_semantic`]
//!     defers the recallContext re-rank (its own tracked deferral); the
//!     differential strips those four fields from both sides. They never reach
//!     the rendered memory block.
//!   - The **memory-recap** and **keyword-distillation** paths run only through
//!     their seams (no canned content in the corpus → v4's mocked
//!     `generateMemoryRecap`/`extractMemorySearchKeywords` return nothing on the
//!     oracle side, matching the seam defaults).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::chat_tasks::{extract_visible_conversation, strip_tool_artifacts, RawMessage};
use crate::cheap_llm::UncensoredFallbackOptions;
use crate::context_budget::{
    get_recommended_context_allocation, should_summarize_conversation, MaxAvailable,
};
use crate::context_compression::{
    build_compressed_history_block, should_apply_budget_compression,
    split_messages_for_compression, CompressibleMessage, CompressionSettings,
};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_injector::{
    format_current_scene_state, format_dynamic_memory_head, format_frozen_memory_archive,
    format_inter_character_memories_for_context, DebugInterCharacterMemoryInfo, DebugMemoryInfo,
    InjectorMemory, InjectorResult, RecallAdjustment, SceneState, SceneStateEmissionEntry,
    DYNAMIC_HEAD_DEFAULT_SIZE, DYNAMIC_HEAD_TOKEN_BUDGET, RETRO_HEAD_SIZE, RETRO_HEAD_TOKEN_BUDGET,
};
use crate::message_attribution::{
    attribute_messages_for_character, filter_messages_by_history_access, filter_whisper_messages,
    find_user_participant_name, AttributionMessage, AttributionParticipant,
};
use crate::message_selector::{select_recent_messages, SelectableMessage};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::compression::{
    apply_context_compression, ContextCompressionOptions, ContextCompressionResult,
};
use crate::services::knowledge_injector::{retrieve_knowledge_for_turn, KnowledgeRetrievalParams};
use crate::services::memory_service::{search_memories_semantic, SemanticSearchOptions};
use crate::system_prompt::{
    build_identity_reinforcement, build_other_participants_info, build_system_prompt,
    BuildSystemPromptOptions, Character as SysChar, ChatParticipant as SysParticipant,
};
use crate::token_estimation::{
    count_messages_tokens, estimate_tokens, truncate_to_token_limit, DEFAULT_CHARS_PER_TOKEN,
};

/// v4 `CONTEXT_HISTORY_BUDGET_RATIO` (`lib/llm/model-context-data.ts:270`).
const CONTEXT_HISTORY_BUDGET_RATIO: f64 = 0.50;
/// v4 `MEMORY_BUDGET_RATIO` (`lib/llm/model-context-data.ts:273`).
const MEMORY_BUDGET_RATIO: f64 = 0.20;
/// v4 `INTER_CHAR_PER_CHARACTER_LIMIT`.
const INTER_CHAR_PER_CHARACTER_LIMIT: i64 = 5;
/// v4 `INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT`.
const INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT: usize = 5;
/// v4 `RECENT_WINDOW_QUERY_MAX_CHARS`.
const RECENT_WINDOW_QUERY_MAX_CHARS: usize = 600;
/// v4 `SUMMARY_CONTENT_PREFIX` — the head Librarian summary whisper marker used
/// to place the cache breakpoint (`lib/services/librarian-notifications/writer.ts:790`).
pub const SUMMARY_CONTENT_PREFIX: &str =
    "The Librarian deposits a précis of the conversation to date upon the table — file it for reference:";

// ---------------------------------------------------------------------------
// Result types (v4 BuiltContext / ContextMessage / ContextBudget).
// ---------------------------------------------------------------------------

/// Provider cache breakpoint marker (v4 `{ type: 'ephemeral' }`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: &'static str,
}

/// v4 `ContextMessage.metadata`.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMessageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_injected: Option<bool>,
}

/// A message ready for the LLM (v4 `ContextMessage`). Absent optionals are
/// dropped by `JSON.stringify` in v4 → `skip_serializing_if` here.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextMessage {
    pub role: &'static str,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ContextMessageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "cacheControl", skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// v4 `ContextBudget`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBudget {
    pub total_limit: i64,
    pub system_prompt_budget: i64,
    pub memory_budget: i64,
    pub knowledge_budget: i64,
    pub summary_budget: i64,
    /// v4 keeps this un-floored (`allocation.recentMessages` is an `f64`), but
    /// `JSON.stringify` renders an integer-valued float bare (`120000`, not
    /// `120000.0`) — so serialize via `js_number_to_json`.
    #[serde(serialize_with = "serialize_js_number")]
    pub recent_messages_budget: f64,
    pub response_reserve: i64,
}

/// Serialize an `f64` the way v4's `JSON.stringify` renders it (integer-valued →
/// bare integer). Mirrors [`crate::db::js_number_to_json`].
fn serialize_js_number<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*v).serialize(s)
}

/// v4 `BuiltContext.tokenUsage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub system_prompt: i64,
    pub memories: i64,
    pub knowledge: i64,
    pub summary: i64,
    pub recent_messages: i64,
    pub total: i64,
}

// v4 `BuiltContext.debugMemories[]` — the *declared* type is
// `{summary, importance, score, effectiveWeight}`, but at runtime v4 assigns the
// full `DebugMemoryInfo[]` (from the archive + dynamic-head formatters) straight
// through, so `JSON.stringify` emits every present field (`memoryId`/`rawWeight`/
// `recallMultiplier`/`recallFired`/`blendedBefore`/`blendedAfter`) and drops the
// `undefined` ones. Built as a JSON object with v4's key names + omissions in
// `debug_memory_json` (the field is a `Vec<Value>`).

/// v4 `BuiltContext.debugInterCharacterMemories[]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugInterCharacterOut {
    pub about_character_name: String,
    pub summary: String,
    #[serde(serialize_with = "serialize_js_number")]
    pub importance: f64,
}

/// v4 `BuiltContext.debugKnowledge[]` (v4 carries the raw `tier` key verbatim —
/// `character`/`group`/`project`/`global`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugKnowledgeOut {
    pub file_path: String,
    pub tier: String,
    #[serde(serialize_with = "serialize_js_number")]
    pub score: f64,
    pub inline: bool,
    pub token_count: i64,
}

/// v4 `BuiltContext.compressionDetails`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionDetailsOut {
    pub original_message_count: i64,
    pub compressed_message_count: i64,
    pub window_message_count: i64,
    pub original_history_tokens: i64,
    pub compressed_history_tokens: i64,
    pub original_system_prompt_tokens: i64,
    pub compressed_system_prompt_tokens: i64,
    pub total_savings: i64,
}

/// The full result of context building (v4 `BuiltContext`). Absent optionals are
/// dropped by `JSON.stringify` in v4.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltContext {
    pub messages: Vec<ContextMessage>,
    pub token_usage: TokenUsage,
    pub budget: ContextBudget,
    pub included_summary: bool,
    pub memories_included: usize,
    pub messages_included: usize,
    pub messages_truncated: bool,
    pub warnings: Vec<String>,
    pub debug_memories: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_inter_character_memories: Option<Vec<DebugInterCharacterOut>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_knowledge: Option<Vec<DebugKnowledgeOut>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_memory_recap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_summary: Option<String>,
    pub debug_system_prompt: String,
    pub original_system_prompt: String,
    pub compression_applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_details: Option<CompressionDetailsOut>,
}

// ---------------------------------------------------------------------------
// Inputs (v4 BuildContextOptions), typed. Reads that v4 does inside buildContext
// via getRepositories() are hoisted to the caller (fixture reads) or the seams,
// keeping the composition a pure function of its inputs + the injected Db/seams.
// ---------------------------------------------------------------------------

/// A conversation message in the trimmed role/content view v4 passes as
/// `existingMessages` (plus the id / thoughtSignature / type it reads).
#[derive(Clone, Debug, Default)]
pub struct ExistingMessage {
    pub role: String,
    pub content: String,
    pub id: Option<String>,
    pub thought_signature: Option<String>,
    /// v4 reads `(msg as {type?}).type` for the visibility check; `None` = a
    /// plain message.
    pub message_type: Option<String>,
}

/// v4's `TimestampConfig` subset buildContext reads (mode / autoPrepend), plus
/// the fields `calculate_current_timestamp` needs. Kept as the ported
/// [`crate::chat_timestamp::TimestampConfig`].
pub use crate::chat_timestamp::TimestampConfig;

/// A responding-participant view (v4 `ChatParticipantBase` subset buildContext
/// reads).
#[derive(Clone, Debug)]
pub struct RespondingParticipant {
    pub id: String,
    pub selected_system_prompt_id: Option<String>,
}

/// A full participant view for the multi-character branches.
#[derive(Clone, Debug)]
pub struct FullParticipant {
    pub id: String,
    pub participant_type: String,
    pub character_id: Option<String>,
    pub controlled_by: String,
    pub status: String,
    pub created_at: Option<String>,
    pub has_history_access: bool,
}

/// Character data buildContext consumes (id / name / vault + the system-prompt
/// character subset).
#[derive(Clone, Debug)]
pub struct ContextCharacter {
    pub id: String,
    pub name: String,
    pub character_document_mount_point_id: Option<String>,
    /// The [`crate::system_prompt::Character`] subset for the prompt builder.
    pub sys: SysChar,
}

/// v4's `MessageWithParticipant` for the multi-character attribution path.
#[derive(Clone, Debug, Default)]
pub struct MessageWithParticipant {
    pub id: Option<String>,
    pub role: String,
    pub content: String,
    pub participant_id: Option<String>,
    pub thought_signature: Option<String>,
    pub created_at: Option<String>,
    pub target_participant_ids: Option<Vec<String>>,
    /// (participant_id, to_status) — for presence, unused here (attribution only).
    pub host_event: Option<(Option<String>, Option<String>)>,
}

/// v4 `ConnectionProfile` subset feeding `calculateMaxAvailable`.
#[derive(Clone, Debug, Default)]
pub struct ConnectionProfileInput {
    pub max_context: Option<i64>,
    pub max_tokens: Option<i64>,
    pub model_class: Option<String>,
}

/// v4 `ContextCompressionSettings` subset.
#[derive(Clone, Debug)]
pub struct ContextCompressionSettingsInput {
    pub enabled: bool,
    pub window_size: i64,
    /// v4's `contextCompressionSettings.contextCompressionThreshold` etc. drive
    /// `shouldApplyBudgetCompression`; kept as the ported settings shape.
    pub compression_threshold_ratio: f64,
    pub system_prompt_target_tokens: i64,
}

/// Chat metadata buildContext reads.
#[derive(Clone, Debug, Default)]
pub struct ContextChat {
    pub id: String,
    pub project_id: Option<String>,
    pub scenario_text: Option<String>,
    pub context_summary: Option<String>,
    pub compaction_generation: i64,
    /// The raw `summaryAnchorMessageIds` array.
    pub summary_anchor_message_ids: Vec<String>,
    /// The raw `commonplaceRecallHistory` JSON (for the recall-history persist).
    pub commonplace_recall_history: Value,
    /// The raw `commonplaceSceneCache` JSON object (per-recipient scene-emission
    /// cache) — read for the prior-emission `_unchanged_` compaction (W4.6a).
    pub commonplace_scene_cache: Option<Value>,
    /// The raw `sceneState` (JSON string or object) — parsed to [`SceneState`].
    pub scene_state: Option<Value>,
    /// The precompiled identity stack for the responding participant (v4
    /// `getCompiledIdentityStack`; resolved by the caller).
    pub precompiled_identity_stack: Option<String>,
    /// Which clock the chat's story runs on (`"realtime" | "narrative"`, v4
    /// `chat.timelineMode`, episodic spine). `None` → realtime.
    pub timeline_mode: Option<String>,
}

/// The full input bag (v4 `BuildContextOptions`, the subset in scope).
pub struct BuildContextInput {
    pub model_context_limit: i64,
    pub user_id: String,
    pub character: ContextCharacter,
    pub user_character: Option<crate::system_prompt::UserCharacter>,
    pub chat: ContextChat,
    pub existing_messages: Vec<ExistingMessage>,
    pub new_user_message: Option<String>,
    pub active_user_participant_id: Option<String>,
    pub roleplay_template: Option<String>,
    pub embedding_profile_id: Option<String>,
    pub skip_memories: bool,
    pub min_memory_importance: f64,
    // multi-character
    pub responding_participant: Option<RespondingParticipant>,
    pub all_participants: Option<Vec<FullParticipant>>,
    pub participant_characters: Option<HashMap<String, ContextCharacter>>,
    pub messages_with_participants: Option<Vec<MessageWithParticipant>>,
    // tool + timestamp
    pub tool_instructions: Option<String>,
    pub timestamp_config: Option<TimestampConfig>,
    pub is_initial_message: bool,
    pub timezone: Option<String>,
    // budget-driven compression
    pub connection_profile: Option<ConnectionProfileInput>,
    pub context_compression_settings: Option<ContextCompressionSettingsInput>,
    pub cheap_llm_selection: Option<crate::cheap_llm::CheapLlmSelection>,
    pub bypass_compression: bool,
    /// v4 `options.cachedCompressionResult` — the async pre-compression cache
    /// result. When present AND `compressionApplied`, phase-1 uses it instead of
    /// running the synchronous compression call (W4.4a4).
    pub cached_compression_result: Option<ContextCompressionResult>,
    /// v4 `options.cachedCompressionMessageCount` — the visible-message count the
    /// cache was computed at. Drives the dynamic effective-window size so no
    /// messages since the cache point are lost.
    pub cached_compression_message_count: Option<i64>,
    // memory recap / continue
    pub generate_memory_recap: bool,
    pub uncensored_fallback: Option<OwnedUncensoredFallback>,
    pub is_continue_mode: bool,
    // clock seams
    pub now_ms: i64,
    pub local_offset_minutes: i64,
    pub minutes_since_last_timestamp_announcement: Option<i64>,
    /// v4 `options.autonomousContextCap` (U4.4, the enclave per-turn clamp):
    /// clamp the model-derived `budgetInfo.maxAvailable` down to this turn's
    /// slice of the per-run token budget (`remaining / turns_left`, computed by
    /// the autonomous turn handler via `compute_autonomous_context_cap`). Only
    /// ever SHRINKS (guarded by `<`); `None` leaves behavior unchanged — every
    /// non-autonomous caller passes `None`, so pre-existing corpora are
    /// untouched.
    pub autonomous_context_cap: Option<i64>,
    /// "Nothing to add" turn-skipping — per-turn ephemeral instruction control
    /// (v4 `options.turnSkip`, b90cd1f5). When `offer_skip` is true, a Turn note
    /// is appended to (or pushed after) the outgoing messages inviting the
    /// character to pass with the `[NOTHING TO ADD]` sentinel;
    /// `recently_addressed` adds a caution to answer rather than pass. Never
    /// persisted. `None` / `offer_skip: false` → no note.
    pub turn_skip: Option<TurnSkip>,
    /// v4 `options.preSearchedMemories` — the proactive pre-compute distill's
    /// semantic-search results (P4.19, `pre_compute::proactive_recall_task`). When
    /// present AND non-empty, the two-pool memory retrieval takes the
    /// `'pre-searched'` path: these become the dynamic head (archive-overlap
    /// filtered) and the fallback distill block is skipped entirely
    /// (`context-manager.ts:1141-1145`). Every non-orchestrator caller passes
    /// `None`, leaving the fallback path unchanged.
    pub pre_searched_memories: Option<Vec<crate::services::memory_service::SemanticSearchResult>>,
    /// v4 `options.recallSignals` — the turn-level recall signals the proactive
    /// distillation emitted (retrospective / timeRange / entities / paraphrase).
    /// Seeds `turn_recall_signals` so the retrospective cadence (enlarged head +
    /// mini-recap) fires on BOTH the pre-searched and fallback paths without
    /// re-running the distillation (`context-manager.ts:1139`). `None` when the
    /// proactive path didn't run or its extraction failed.
    pub recall_signals: Option<crate::services::memory_recap::distill::DistilledSearch>,
}

/// v4's `turnSkip` option bag (`{ offerSkip, recentlyAddressed, characterName }`).
#[derive(Clone, Debug)]
pub struct TurnSkip {
    pub offer_skip: bool,
    pub recently_addressed: bool,
    pub character_name: String,
}

/// Build the ephemeral "you may pass this turn" Turn note (v4
/// `buildTurnSkipInstruction`). Not persisted — it rides only in the outgoing
/// LLM context for this single turn. The Host announcement text deliberately
/// never contains the sentinel, so history can't teach the phrase; this note is
/// the only place it appears.
pub fn build_turn_skip_instruction(character_name: &str, recently_addressed: bool) -> String {
    let base = "[Turn note from the Salon — not spoken by any character]\nYou are not obliged to speak this turn. If — and only if — you genuinely have\nnothing substantive to add to the conversation right now, reply with exactly\nthis single line and nothing else:\n\n[NOTHING TO ADD]\n\nThe floor will then pass to someone else and the scene continues without you\nthis turn. Do not use it to be coy or mysterious — a brief in-character\nremark is always better than an empty pass. If you have anything worth\nsaying, write your reply as normal and ignore this note entirely.";

    if !recently_addressed {
        return base.to_string();
    }

    format!(
        "{base}\n\nOne caution: {character_name} appears to have been addressed or mentioned since you last spoke. If someone has spoken to you and you have not yet answered them, you should answer rather than pass."
    )
}

/// Owned form of [`UncensoredFallbackOptions`] (the borrowing lives inside the
/// compression call).
#[derive(Clone, Debug)]
pub struct OwnedUncensoredFallback {
    pub danger_settings: crate::cheap_llm::DangerousContentSettings,
    pub available_profiles: Vec<crate::cheap_llm::CheapLlmProfile>,
}

// ---------------------------------------------------------------------------
// The seams for unported feeder / post-office subsystems.
// ---------------------------------------------------------------------------

/// The result of the seamed memory-recap generation (v4 `MemoryRecapResult`).
#[derive(Clone, Debug, Default)]
pub struct MemoryRecapResult {
    pub content: Option<String>,
}

/// The result of the seamed keyword distillation (v4
/// `extractMemorySearchKeywords`). `paraphrase` wins over `keywords` as the
/// embedding query; `context`/`temporal` steer the recall (both deferred at the
/// search leg — see `search_memories_semantic`'s recall deferral).
#[derive(Clone, Debug, Default)]
pub struct DistilledSearch {
    pub paraphrase: Option<String>,
    pub keywords: Vec<String>,
}

/// The tri-tier mount pool (v4 `resolveTieredMountPool` output subset).
#[derive(Clone, Debug, Default)]
pub struct MountPool {
    pub character_mount_point_id: Option<String>,
    pub group_mount_point_ids: Vec<String>,
    pub project_mount_point_ids: Vec<String>,
    pub global_mount_point_id: Option<String>,
}

/// Instance-wide recall settings (v4 `getMemoryRecallSettings`).
#[derive(Clone, Debug)]
pub struct MemoryRecallSettings {
    pub scope_policy: String,
    pub expand_related: bool,
}

impl Default for MemoryRecallSettings {
    fn default() -> Self {
        // v4 `DEFAULT_MEMORY_RECALL_SETTINGS` (`lib/instance-settings`).
        MemoryRecallSettings {
            scope_policy: "down-weight".to_string(),
            expand_related: false,
        }
    }
}

/// The injected **whisper-POSTING** half of the context feeders (W4.6b, wired live
/// in the Round-3 unification pass). W4.6a closed every READ/COMPUTE feeder in-line
/// (recap, distill, mount pool, frozen archive, live clothing, off-scene scan,
/// recall settings, core-whisper assembly, Suparṇā mail read,
/// scene-cache/recall-history persist); this trait is the set of personified-system
/// message POSTs. [`RealBuildContextSeams`] delegates each to its W4.6b writer;
/// [`NoopSeams`] keeps every method a no-op for read-only / off-path callers. The
/// methods are async (RPITIT `+ Send`), matching [`crate::services::context_summary::
/// ContextSummarySeams`].
pub trait BuildContextSeams {
    /// v4 `postHostOffSceneCharactersAnnouncement` — the Host's public off-scene
    /// introduction. Given the newcomer cards, post the intro (the writer builds the
    /// content + stamps `hostEvent.introducedCharacterIds`) and return the posted
    /// message's content, which THIS turn's LLM context includes (v4's
    /// `pendingOffSceneAnnouncement = { content: announcement.content }`). Default:
    /// no post → `None`.
    fn post_host_off_scene_announcement(
        &self,
        chat_id: &str,
        cards: &[crate::services::off_scene::OffSceneCharacterCard],
    ) -> impl std::future::Future<Output = Option<String>> + Send {
        let _ = (chat_id, cards);
        async { None }
    }

    /// v4 `postHostTimestampAnnouncement` — the Host's fictional-clock note.
    /// Default: no-op.
    fn post_host_timestamp_announcement(
        &self,
        chat_id: &str,
        formatted: &str,
    ) -> impl std::future::Future<Output = ()> + Send {
        let _ = (chat_id, formatted);
        async {}
    }

    /// v4 `postCoreWhisper` (+ the stale-whisper sweep) — Aurora's per-character
    /// Core re-offering. Default: no-op.
    fn post_core_whisper(
        &self,
        chat_id: &str,
        target_participant_id: &str,
        persona_content: &str,
        opaque_content: &str,
    ) -> impl std::future::Future<Output = ()> + Send {
        let _ = (
            chat_id,
            target_participant_id,
            persona_content,
            opaque_content,
        );
        async {}
    }

    /// v4 `postCommonplaceWhisper` (+ the stale-whisper sweep) — the consolidated
    /// Commonplace-Book recall whisper. Returns whether a whisper was durably posted
    /// (v4's `posted`), which gates the two persist writes (`commonplaceSceneCache` /
    /// `commonplaceRecallHistory`). Default: `false` (nothing posted → no persist).
    fn post_commonplace_whisper(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) -> impl std::future::Future<Output = bool> + Send {
        let _ = (chat_id, target_participant_id, content);
        async { false }
    }

    /// v4's SECOND `postCommonplaceWhisper` call — the recall-on-reference
    /// mini-recap, posted as its own `retrospective-recall` whisper AFTER the
    /// consolidated sweep so it survives this turn (the next turn's consolidated
    /// sweep removes it — the kind is deliberately NOT sweep-exempt). No
    /// `opaqueContent`, no sweep of its own, and its return value is unused.
    /// Default: no-op.
    fn post_commonplace_retrospective_recall(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) -> impl std::future::Future<Output = ()> + Send {
        let _ = (chat_id, target_participant_id, content);
        async {}
    }

    /// v4 `postSuparnaMailWhisper` — Suparṇā's new-mail whisper. Given the unalerted
    /// letters, build the persona whisper ([`crate::services::suparna_notifications::
    /// build_suparna_mail_whisper`]) and post it targeted at the responding
    /// participant (multi-character) or public (single). Default: no-op.
    fn post_suparna_mail(
        &self,
        chat_id: &str,
        letters: &[crate::post_office::mailbox::DeliveredLetterSummary],
        target_participant_id: Option<&str>,
    ) -> impl std::future::Future<Output = ()> + Send {
        let _ = (chat_id, letters, target_participant_id);
        async {}
    }
}

/// The default no-op seam bundle (read-only / off-path callers).
pub struct NoopSeams;
impl BuildContextSeams for NoopSeams {}

/// The production seam bundle (Round-3 unification): each POST delegates to its
/// W4.6b personified-system writer, and the core-whisper / commonplace posts run
/// their stale-whisper sweeps (v4's `if (posted) { … sweep … }`).
pub struct RealBuildContextSeams<'a> {
    pub db: &'a Db,
}

impl BuildContextSeams for RealBuildContextSeams<'_> {
    async fn post_host_off_scene_announcement(
        &self,
        chat_id: &str,
        cards: &[crate::services::off_scene::OffSceneCharacterCard],
    ) -> Option<String> {
        let posted =
            crate::services::host_notifications::post_host_off_scene_characters_announcement(
                self.db,
                crate::services::host_notifications::HostOffSceneCharactersAnnouncement {
                    chat_id: chat_id.to_string(),
                    characters: cards.to_vec(),
                },
            )
            .await?;
        posted
            .message
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    async fn post_host_timestamp_announcement(&self, chat_id: &str, formatted: &str) {
        let _ = crate::services::host_notifications::post_host_timestamp_announcement(
            self.db,
            crate::services::host_notifications::HostTimestampAnnouncement {
                chat_id: chat_id.to_string(),
                formatted: formatted.to_string(),
            },
        )
        .await;
    }

    async fn post_core_whisper(
        &self,
        chat_id: &str,
        target_participant_id: &str,
        persona_content: &str,
        opaque_content: &str,
    ) {
        let posted = crate::services::aurora_notifications::post_core_whisper(
            self.db,
            chat_id,
            target_participant_id,
            persona_content,
            opaque_content,
        )
        .await;
        if let Some(msg) = posted {
            let posted_id = msg
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let target = target_participant_id.to_string();
            // v4: sweep prior aurora/core-whisper targeted at this participant,
            // except the just-posted one.
            sweep_stale_whispers(self.db, chat_id, &posted_id, move |m| {
                m.get("systemSender").and_then(Value::as_str) == Some("aurora")
                    && m.get("systemKind").and_then(Value::as_str) == Some("core-whisper")
                    && target_ids_include(m, Some(&target))
            })
            .await;
        }
    }

    async fn post_commonplace_whisper(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) -> bool {
        let params = crate::services::commonplace_notifications::PostCommonplaceParams {
            chat_id: chat_id.to_string(),
            target_participant_id: target_participant_id.map(str::to_string),
            content: content.to_string(),
            // v4 passes no `opaqueContent` (→ null) and kind `consolidated`.
            opaque_content: None,
            kind: crate::services::commonplace_notifications::CommonplaceWhisperKind::Consolidated,
        };
        match crate::services::commonplace_notifications::post_commonplace_whisper(self.db, params)
            .await
        {
            Some(posted) => {
                let target = target_participant_id.map(str::to_string);
                // v4: sweep prior commonplaceBook whispers (NOT relevant-conversations)
                // in the same target scope, except the just-posted one.
                sweep_stale_whispers(self.db, chat_id, &posted.id, move |m| {
                    m.get("systemSender").and_then(Value::as_str) == Some("commonplaceBook")
                        && m.get("systemKind").and_then(Value::as_str)
                            != Some("relevant-conversations")
                        && target_ids_include(m, target.as_deref())
                })
                .await;
                true
            }
            None => false,
        }
    }

    async fn post_commonplace_retrospective_recall(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) {
        // v4 passes no `opaqueContent` (→ null) and kind `retrospective-recall`,
        // and runs NO sweep here — the next consolidated post sweeps it.
        let _ = crate::services::commonplace_notifications::post_commonplace_whisper(
            self.db,
            crate::services::commonplace_notifications::PostCommonplaceParams {
                chat_id: chat_id.to_string(),
                target_participant_id: target_participant_id.map(str::to_string),
                content: content.to_string(),
                opaque_content: None,
                kind:
                    crate::services::commonplace_notifications::CommonplaceWhisperKind::RetrospectiveRecall,
            },
        )
        .await;
    }

    async fn post_suparna_mail(
        &self,
        chat_id: &str,
        letters: &[crate::post_office::mailbox::DeliveredLetterSummary],
        target_participant_id: Option<&str>,
    ) {
        let content = crate::services::suparna_notifications::build_suparna_mail_whisper(letters);
        let _ = crate::services::suparna_notifications::post_suparna_mail_whisper(
            self.db,
            crate::services::suparna_notifications::PostSuparnaMailWhisperParams {
                chat_id: chat_id.to_string(),
                target_participant_id: target_participant_id.map(str::to_string),
                content,
            },
        )
        .await;
    }
}

/// v4's target-scope filter for the whisper sweep: when `target` is `None`, the
/// stale whisper must be public (`targetParticipantIds` null/absent); when
/// `Some(id)`, its `targetParticipantIds` must be an array including `id`.
fn target_ids_include(message: &Value, target: Option<&str>) -> bool {
    let ids = message.get("targetParticipantIds");
    match target {
        None => matches!(ids, None | Some(Value::Null)),
        Some(t) => ids
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(t))),
    }
}

/// v4's stale-whisper sweep (shared by core-whisper + commonplace): re-read the
/// chat's messages, keep the `type:'message'` rows matching `predicate` (already
/// excluding the just-posted id), and delete them. Best-effort — a read/write
/// failure is swallowed (v4 logs + continues).
async fn sweep_stale_whispers<F: Fn(&Value) -> bool>(
    db: &Db,
    chat_id: &str,
    posted_id: &str,
    predicate: F,
) {
    let cid = chat_id.to_string();
    let messages =
        match db.read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid)) {
            Ok(m) => m,
            Err(_) => return,
        };
    let stale: Vec<String> = messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .filter(|m| m.get("id").and_then(Value::as_str) != Some(posted_id))
        .filter(|m| predicate(m))
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if stale.is_empty() {
        return;
    }
    let cid = chat_id.to_string();
    let _ = db
        .write(move |w| {
            w.main()
                .chat_messages()
                .delete_messages_by_ids(&cid, &stale)
        })
        .await;
}

/// Errors from [`build_context`]. v4 throws on an invalid timezone (aborting the
/// build) and surfaces DB failures; both are modeled here.
#[derive(Debug)]
pub enum BuildContextError {
    /// A DB read/write failed (propagated from [`Db`]).
    Db(DbError),
    /// The resolved IANA timezone was invalid (v4 throws).
    InvalidTimezone(String),
}

impl std::fmt::Display for BuildContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildContextError::Db(e) => write!(f, "db error: {e:?}"),
            BuildContextError::InvalidTimezone(tz) => write!(f, "invalid timezone: {tz}"),
        }
    }
}
impl std::error::Error for BuildContextError {}
impl From<DbError> for BuildContextError {
    fn from(e: DbError) -> Self {
        BuildContextError::Db(e)
    }
}

// ---------------------------------------------------------------------------
// buildRecentWindowQuery (v4 private).
// ---------------------------------------------------------------------------

/// v4 `buildRecentWindowQuery` — the tail of the conversation as prose, the
/// memory-search base query. Keeps the most-recent text on truncation (slice
/// from the end). The slice is by JS `String.length` (UTF-16 code units); a
/// 600-char cap on plain corpus text is ASCII-only, so a byte slice suffices for
/// the corpus, but we cap on code units to stay faithful.
fn build_recent_window_query(
    existing_messages: &[ExistingMessage],
    new_user_message: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    // Walk backward collecting up to 3 non-empty user/assistant turns.
    let mut i = existing_messages.len();
    while i > 0 && parts.len() < 3 {
        i -= 1;
        let m = &existing_messages[i];
        let role = m.role.to_lowercase();
        if role != "user" && role != "assistant" {
            continue;
        }
        let content = if role == "assistant" {
            strip_tool_artifacts(&m.content).unwrap_or_default()
        } else {
            m.content.clone()
        };
        let trimmed = crate::jsstr::js_trim(&content);
        if trimmed.is_empty() {
            continue;
        }
        parts.insert(0, trimmed.to_string());
    }
    if let Some(n) = new_user_message {
        let t = crate::jsstr::js_trim(n);
        if !t.is_empty() {
            parts.push(t.to_string());
        }
    }
    let joined = parts.join("\n");
    // UTF-16-faithful "slice from the end" cap.
    let units: Vec<u16> = joined.encode_utf16().collect();
    if units.len() > RECENT_WINDOW_QUERY_MAX_CHARS {
        let start = units.len() - RECENT_WINDOW_QUERY_MAX_CHARS;
        String::from_utf16_lossy(&units[start..])
    } else {
        joined
    }
}

// ---------------------------------------------------------------------------
// Small helpers to project JSON memory rows into InjectorResult / InjectorMemory.
// ---------------------------------------------------------------------------

fn json_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
fn json_f64(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}
fn json_ms(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_str)
        .and_then(crate::clock::iso_to_ms)
        .map(|ms| ms as f64)
}
fn json_str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Build an [`InjectorMemory`] from a net memory JSON row (the shape the read
/// repos return). `graph_degree` = `relatedMemoryIds.length`.
fn injector_memory_from_json(v: &Value) -> InjectorMemory {
    InjectorMemory {
        id: json_str(v, "id").unwrap_or_default(),
        content: json_str(v, "content"),
        summary: json_str(v, "summary").unwrap_or_default(),
        importance: json_f64(v, "importance").unwrap_or(0.0),
        keywords: json_str_array(v, "keywords"),
        about_character_id: json_str(v, "aboutCharacterId"),
        reinforced_importance: json_f64(v, "reinforcedImportance"),
        created_at_ms: json_ms(v, "createdAt").unwrap_or(0.0),
        last_reinforced_at_ms: json_ms(v, "lastReinforcedAt"),
        last_accessed_at_ms: json_ms(v, "lastAccessedAt"),
        reinforcement_count: json_f64(v, "reinforcementCount").map(|n| n as u64),
        graph_degree: json_str_array(v, "relatedMemoryIds").len(),
        // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
        kind_episodic: json_str(v, "kind").as_deref() == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(v.get("occurredAt").and_then(Value::as_str)),
        narrative_time: json_str(v, "narrativeTime"),
    }
}

/// Project a [`SemanticSearchResult`](crate::services::memory_service::SemanticSearchResult)
/// into an [`InjectorResult`] the formatters read.
fn injector_result_from_search(
    r: &crate::services::memory_service::SemanticSearchResult,
) -> InjectorResult {
    InjectorResult {
        memory: injector_memory_from_json(&r.memory),
        score: r.score,
        effective_weight: Some(r.effective_weight),
        raw_weight: Some(r.raw_weight),
        recall_adjustment: r.recall_adjustment.as_ref().map(|a| RecallAdjustment {
            multiplier: Some(a.multiplier),
            fired: a.fired.clone(),
            blended_before: Some(a.blended_before),
            blended_after: Some(a.blended_after),
        }),
    }
}

// ---------------------------------------------------------------------------
// Ported feeder helpers (W4.6a) — the READ/COMPUTE half of the former seams.
// ---------------------------------------------------------------------------

/// v4 `resolveTieredMountPool` for buildContext's call (no ownership gate, no
/// participant tier). Reads across both DBs; degrades gracefully.
fn resolve_mount_pool(db: &Db, input: &BuildContextInput) -> MountPool {
    use crate::db::tiered_mount_pool::{
        resolve_tiered_mount_pool, TierContext, TierResolveOptions,
    };
    let ctx = TierContext {
        user_id: Some(input.user_id.clone()),
        character_id: if input.character.id.is_empty() {
            None
        } else {
            Some(input.character.id.clone())
        },
        character_mount_point_id: input.character.character_document_mount_point_id.clone(),
        character_ids: None,
        project_id: input.chat.project_id.clone(),
    };
    let opts = TierResolveOptions {
        require_ownership: false,
        include_participants: false,
    };
    let resolved = db
        .read_main(|main| {
            db.read_mount_index(|mount| Ok(resolve_tiered_mount_pool(main, mount, &ctx, &opts)))
        })
        .unwrap_or_default();
    MountPool {
        character_mount_point_id: resolved.character_mount_point_id,
        group_mount_point_ids: resolved.group_mount_point_ids,
        project_mount_point_ids: resolved.project_mount_point_ids,
        global_mount_point_id: resolved.global_mount_point_id,
    }
}

/// v4 `getMemoryRecallSettings` (`lib/instance-settings`) — reads the per-instance
/// Commonplace-Book recall settings, defaulting to `down-weight` / no-expand when
/// the setting is unwritten. Consumed by the per-turn recall context (P4.d13
/// closed the search-leg re-rank deferral).
pub(crate) fn read_memory_recall_settings(db: &Db) -> MemoryRecallSettings {
    match db.read_main(crate::db::instance_settings::get_memory_recall_settings) {
        Ok((scope_policy, expand_related)) => MemoryRecallSettings {
            scope_policy,
            expand_related,
        },
        Err(_) => MemoryRecallSettings::default(),
    }
}

/// v4 `chats.update({ commonplaceSceneCache })` (W4.6a): persist the per-target
/// scene-state emission cache as `{...prior, [cacheTargetKey]: slice}`, where the
/// slice is `characterId → SceneStateEmissionEntry` (insertion order). Awaited (v4
/// awaits); a write failure is swallowed (benign — next turn re-emits full).
async fn persist_scene_cache(
    db: &Db,
    input: &BuildContextInput,
    cache_target_key: &str,
    emitted: &[(String, SceneStateEmissionEntry)],
) {
    use serde_json::{Map, Value};
    // Build the slice object in insertion order (preserve_order keeps it).
    let mut slice = Map::new();
    for (cid, entry) in emitted {
        let mut o = Map::new();
        o.insert(
            "actionHash".to_string(),
            Value::String(entry.action_hash.clone()),
        );
        o.insert(
            "clothingHash".to_string(),
            Value::String(entry.clothing_hash.clone()),
        );
        o.insert(
            "emittedAt".to_string(),
            Value::String(entry.emitted_at.clone()),
        );
        slice.insert(cid.clone(), Value::Object(o));
    }
    // `{...(priorCache ?? {}), [cacheTargetKey]: slice}`.
    let mut next = match input
        .chat
        .commonplace_scene_cache
        .as_ref()
        .and_then(Value::as_object)
    {
        Some(prior) => prior.clone(),
        None => Map::new(),
    };
    next.insert(cache_target_key.to_string(), Value::Object(slice));
    let next_value = Value::Object(next);

    let chat_id = input.chat.id.clone();
    let _ = db
        .write(move |writers| {
            let repo = crate::db::chats::ChatsRepository::new(writers.main().connection());
            repo.update(
                &chat_id,
                &crate::db::chats::ChatUpdate {
                    commonplace_scene_cache: Some(next_value),
                    ..Default::default()
                },
            )
            .map(|_| ())
        })
        .await;
}

/// v4 `chats.update({ commonplaceRecallHistory })` (W4.6a): persist the recall
/// ring buffer. Awaited; a write failure is swallowed (one un-penalized turn).
async fn persist_recall_history(db: &Db, chat_id: &str, next_history: Value) {
    let chat_id = chat_id.to_string();
    let _ = db
        .write(move |writers| {
            let repo = crate::db::chats::ChatsRepository::new(writers.main().connection());
            repo.update(
                &chat_id,
                &crate::db::chats::ChatUpdate {
                    commonplace_recall_history: Some(next_history),
                    ..Default::default()
                },
            )
            .map(|_| ())
        })
        .await;
}

/// v4's per-character live-clothing override (W4.6a). For each scene character
/// with a currently-equipped wardrobe whose hash differs from the scene-cached
/// `clothingHash`, resolve the concise title-only outfit description. Reads the
/// raw scene JSON to recover each character's `clothingHash`. Fails soft per
/// character (a failure leaves the cached line standing).
fn resolve_live_clothing(
    db: &Db,
    input: &BuildContextInput,
    parsed_scene: Option<&crate::memory_injector::SceneState>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(scene) = parsed_scene else {
        return map;
    };
    if scene.characters.is_empty() {
        return map;
    }
    // Recover each character's stored `clothingHash` from the raw scene JSON.
    let cached_hashes = scene_clothing_hashes(input.chat.scene_state.as_ref());
    // The project mount pool ids (v4 `getTurnMountPool().projectMountPointIds`).
    let project_mount_point_ids: Vec<String> = {
        let pid = input.chat.project_id.clone();
        db.read_mount_index(move |mount| {
            Ok(match &pid {
                Some(p) => {
                    crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(mount)
                        .find_by_project_id(p)
                        .unwrap_or_default()
                }
                None => Vec::new(),
            })
        })
        .unwrap_or_default()
    };

    for c in &scene.characters {
        if c.character_id.is_empty() {
            continue;
        }
        let cid = c.character_id.clone();
        let chat_id = input.chat.id.clone();
        // Read equipped outfit for the character.
        let equipped: Option<Value> = db
            .read_main(move |main| {
                crate::db::chats_outfits::ChatOutfitsRepository::new(main)
                    .get_equipped_outfit_for_character(&chat_id, &cid)
            })
            .unwrap_or(None);
        let slots = crate::wardrobe::Slots::from_value(equipped.as_ref());
        if !crate::wardrobe::has_equipped_items(Some(&slots)) {
            continue;
        }
        let live_hash = crate::wardrobe::hash_equipped_slots(Some(&slots));
        // Wardrobe unchanged since the cached summary was derived — keep it.
        let cached = cached_hashes.get(&c.character_id).cloned().flatten();
        if Some(&live_hash) == cached.as_ref() {
            continue;
        }
        // Resolve + describe (title-only, imagePrompt-preferring).
        let cid2 = c.character_id.clone();
        let pmp = project_mount_point_ids.clone();
        let desc = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    let docs =
                        crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
                    let values =
                        crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values(
                            main, &docs, &cid2, &slots, &pmp,
                        )?;
                    Ok(crate::wardrobe::describe_outfit(
                        &crate::wardrobe::OutfitSlotValues {
                            top: crate::wardrobe::decorate_outfit_items_title_only(&values.top),
                            bottom: crate::wardrobe::decorate_outfit_items_title_only(
                                &values.bottom,
                            ),
                            footwear: crate::wardrobe::decorate_outfit_items_title_only(
                                &values.footwear,
                            ),
                            accessories: crate::wardrobe::decorate_outfit_items_title_only(
                                &values.accessories,
                            ),
                        },
                    ))
                })
            })
            .unwrap_or_default();
        if !desc.is_empty() {
            map.insert(c.character_id.clone(), desc);
        }
    }
    map
}

/// Recover each scene character's stored `clothingHash` from the raw scene JSON
/// (the parsed [`crate::memory_injector::SceneState`] drops it). Value is
/// `Some(hash)` when present, else `None`.
fn scene_clothing_hashes(raw: Option<&Value>) -> HashMap<String, Option<String>> {
    let mut out = HashMap::new();
    let Some(raw) = raw else { return out };
    let candidate: Value = match raw {
        Value::String(s) => match serde_json::from_str(s) {
            Ok(v) => v,
            Err(_) => return out,
        },
        other => other.clone(),
    };
    if let Some(chars) = candidate.get("characters").and_then(Value::as_array) {
        for c in chars {
            if let Some(cid) = c.get("characterId").and_then(Value::as_str) {
                let hash = c
                    .get("clothingHash")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                out.insert(cid.to_string(), hash);
            }
        }
    }
    out
}

/// Read the per-recipient prior scene-emission cache (v4
/// `chat.commonplaceSceneCache?.[cacheTargetKey]`) as a characterId → entry map.
fn read_prior_scene_emission(
    input: &BuildContextInput,
    cache_target_key: &str,
) -> HashMap<String, crate::memory_injector::SceneStateEmissionEntry> {
    let mut out = HashMap::new();
    let Some(cache) = input.chat.commonplace_scene_cache.as_ref() else {
        return out;
    };
    let Some(target) = cache.get(cache_target_key).and_then(Value::as_object) else {
        return out;
    };
    for (cid, entry) in target {
        let Some(o) = entry.as_object() else { continue };
        let action_hash = o
            .get("actionHash")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let clothing_hash = o
            .get("clothingHash")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let emitted_at = o
            .get("emittedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        out.insert(
            cid.clone(),
            crate::memory_injector::SceneStateEmissionEntry {
                action_hash,
                clothing_hash,
                emitted_at,
            },
        );
    }
    out
}

/// v4's Core-whisper trigger + READ/ASSEMBLY (W4.6a). Resolves the config
/// (chat → character → global), runs [`crate::core_whisper::should_fire_core_whisper`]
/// over the chat's events, and — on a fire — assembles the packet + returns its
/// LLM-context contribution. Fires the W4.6b whisper POST via the recorded seam.
/// Any error → `""` (v4 wraps the whole block error-only).
async fn resolve_core_whisper_llm_context<S: BuildContextSeams>(
    db: &Db,
    input: &BuildContextInput,
    responding_participant_id: &str,
    seams: &S,
) -> String {
    use crate::services::core_whisper as cw;

    // Global core-whisper settings (user chat settings' `coreWhisper` sub-object).
    let uid = input.user_id.clone();
    let global = match db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid)) {
        Ok(Some(settings)) => match settings.get("coreWhisper").and_then(Value::as_object) {
            Some(g) => cw::GlobalCoreWhisperSettings {
                enabled: g.get("enabled").and_then(Value::as_bool),
                interval: g.get("interval").and_then(Value::as_i64),
                silence_threshold: g.get("silenceThreshold").and_then(Value::as_i64),
                packet_token_budget: g.get("packetTokenBudget").and_then(Value::as_i64),
                fire_on_context_transition: g
                    .get("fireOnContextTransition")
                    .and_then(Value::as_bool),
            },
            None => cw::GlobalCoreWhisperSettings::default(),
        },
        // v4 passes `userChatSettings?.coreWhisper ?? null` → all defaults.
        _ => cw::GlobalCoreWhisperSettings::default(),
    };

    // Per-chat + per-character overrides.
    let chat_id = input.chat.id.clone();
    let (chat_enabled, chat_interval) = db
        .read_main(move |c| crate::db::chats_read::find_core_whisper_overrides(c, &chat_id))
        .ok()
        .flatten()
        .unwrap_or((None, None));
    let char_id = input.character.id.clone();
    let character_enabled = db
        .read_main(move |c| crate::db::characters_read::find_core_whisper_enabled(c, &char_id))
        .ok()
        .flatten()
        .flatten();

    let cfg =
        cw::resolve_core_whisper_config(chat_enabled, chat_interval, character_enabled, &global);
    if !cfg.enabled {
        return String::new();
    }

    // Trigger over the chat's events.
    let cid = input.chat.id.clone();
    let events_raw =
        match db.read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid)) {
            Ok(e) => e,
            Err(_) => return String::new(),
        };
    let events: Vec<crate::core_whisper::WhisperEvent> = events_raw
        .iter()
        .map(|m| crate::core_whisper::WhisperEvent {
            event_type: m
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            role: m.get("role").and_then(Value::as_str).map(str::to_string),
            participant_id: m
                .get("participantId")
                .and_then(Value::as_str)
                .map(str::to_string),
            content: m.get("content").and_then(Value::as_str).map(str::to_string),
            system_sender: m
                .get("systemSender")
                .and_then(Value::as_str)
                .map(str::to_string),
            system_kind: m
                .get("systemKind")
                .and_then(Value::as_str)
                .map(str::to_string),
            is_silent_message: m.get("isSilentMessage").and_then(Value::as_bool),
            target_participant_ids: m.get("targetParticipantIds").and_then(Value::as_array).map(
                |a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                },
            ),
        })
        .collect();

    let decision = crate::core_whisper::should_fire_core_whisper(
        crate::core_whisper::ShouldFireCoreWhisperOptions {
            events: &events,
            responding_participant_id,
            is_continue: input.is_continue_mode,
            is_nudge: false,
            interval: cfg.interval,
            silence_threshold: cfg.silence_threshold,
            fire_on_context_transition: cfg.fire_on_context_transition,
        },
    );
    if !decision.fire {
        return String::new();
    }

    // Assemble the packet; None → no whisper.
    let Some(packet) = cw::assemble_core_packet(db, &input.character.id, cfg.packet_token_budget)
    else {
        return String::new();
    };
    let persona = cw::build_core_whisper_content(&packet);
    let opaque = cw::build_core_whisper_opaque_content(&packet);
    let llm_context = cw::build_core_whisper_llm_context(&packet);
    // The W4.6b Aurora POST + stale sweep fire via the seam.
    seams
        .post_core_whisper(&input.chat.id, responding_participant_id, &persona, &opaque)
        .await;
    llm_context
}

/// Cap on entries in the retrospective mini-recap conversation list (v4
/// `RETRO_MINI_RECAP_MAX_ENTRIES`, context-manager.ts:33).
const RETRO_MINI_RECAP_MAX_ENTRIES: usize = 5;

/// v4's UUID-in-backticks scanner for the fold whisper's conversation list
/// (`/\`([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\`/gi`).
static FOLD_WHISPER_UUID_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?i)`([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})`")
        .expect("fold-whisper uuid regex")
});

/// Recall-on-reference (fourth cadence, **part 2**) — v4 `buildContext`
/// context-manager.ts:1944–2030.
///
/// On a retrospective turn, build the scoped mini-recap: a dated, drillable
/// Relevant Past Conversations list from the character's vault summaries, scoped
/// by the turn's `timeRange`/`entities` and closed by the `read_conversation`
/// call note. Returns `Some((content, signature))` only when at least one
/// conversation survives the fold-whisper dedup — the signature is what the
/// caller persists into the recall-history spam-guard ring.
///
/// Spam guard: a block with the same `timeRange`/entity signature emitted within
/// the last [`crate::recall_history::RETRO_SIGNATURE_TURNS`] emissions is skipped
/// outright (v4 logs at debug and returns nothing).
///
/// Best-effort throughout — v4 wraps the whole block in a warn-only try/catch, so
/// every fallible step here degrades in place (`search_vault_conversation_-
/// summaries` already returns `[]` on any failure; the dedup read has its own
/// swallowing catch, since "a duplicate UUID listing is harmless").
async fn resolve_retrospective_mini_recap<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    input: &BuildContextInput,
    signals: &crate::services::memory_recap::distill::DistilledSearch,
    memory_search_query: &str,
    is_multi_character: bool,
) -> Option<(String, String)> {
    // The signature: the window (or `∅`) plus the lowercased entities in JS
    // default-`sort()` order (code-unit lexicographic, which Rust's `str: Ord`
    // reproduces across the BMP), joined by `|`.
    let mut entity_keys: Vec<String> = signals.entities.iter().map(|e| e.to_lowercase()).collect();
    entity_keys.sort();
    let mut signature_parts: Vec<String> = Vec::with_capacity(entity_keys.len() + 1);
    signature_parts.push(match &signals.time_range {
        Some(tr) => format!("{}→{}", tr.from, tr.to),
        None => "∅".to_string(),
    });
    signature_parts.extend(entity_keys);
    let signature = signature_parts.join("|");

    let recent_signatures =
        crate::recall_history::parse_retro_signatures(&input.chat.commonplace_recall_history);
    if recent_signatures.iter().any(|s| s == &signature) {
        // v4 logs `[CommonplaceWhisper] Retrospective mini-recap suppressed
        // (recent same-signature block)` and emits nothing.
        return None;
    }

    // v4 `signals.paraphrase ?? memorySearchQuery ?? ''` — a NULLISH fallback, so
    // an empty-string paraphrase (were one to arrive) is kept as-is and only the
    // absent case falls back to the recent-window query. The entity join is a
    // second part; both are dropped when whitespace-only, and the survivors join
    // with an em dash.
    let query_parts: Vec<String> = [
        signals
            .paraphrase
            .clone()
            .unwrap_or_else(|| memory_search_query.to_string()),
        signals.entities.join(", "),
    ]
    .into_iter()
    .filter(|part| !crate::jsstr::js_trim(part).is_empty())
    .collect();
    let query = query_parts.join(" — ");
    if crate::jsstr::js_trim(&query).is_empty() {
        return None;
    }

    let matches = crate::services::memory_recap::search_vault_conversation_summaries(
        db,
        embedding,
        &input.character.id,
        input.character.character_document_mount_point_id.as_deref(),
        &query,
        &input.user_id,
        input.embedding_profile_id.as_deref(),
        RETRO_MINI_RECAP_MAX_ENTRIES,
        Some(&input.chat.id),
        // The round-2 `time_range` gets its first production caller here.
        signals.time_range.as_ref(),
    )
    .await;

    // Dedup against the standing on-fold `relevant-conversations` whisper for the
    // same target — no point listing the same UUID twice. Best-effort: a read
    // failure just means a possible duplicate.
    let fold_whisper_ids: HashSet<String> = {
        let chat_id = input.chat.id.clone();
        let target_id: Option<&str> = if is_multi_character {
            input.responding_participant.as_ref().map(|p| p.id.as_str())
        } else {
            None
        };
        let messages = db
            .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &chat_id))
            .unwrap_or_default();
        let mut ids = HashSet::new();
        for m in &messages {
            if m.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            if m.get("systemSender").and_then(Value::as_str) != Some("commonplaceBook")
                || m.get("systemKind").and_then(Value::as_str) != Some("relevant-conversations")
            {
                continue;
            }
            if !target_ids_include(m, target_id) {
                continue;
            }
            let content = m.get("content").and_then(Value::as_str).unwrap_or("");
            for cap in FOLD_WHISPER_UUID_RE.captures_iter(content) {
                ids.insert(cap[1].to_string());
            }
        }
        ids
    };

    let mut filtered: Vec<_> = matches
        .into_iter()
        .filter(|m| !fold_whisper_ids.contains(&m.conversation_id))
        .collect();
    filtered.truncate(RETRO_MINI_RECAP_MAX_ENTRIES);
    if filtered.is_empty() {
        return None;
    }
    let content = format!(
        "{}\n\n{}",
        crate::services::memory_recap::render_relevant_conversations_block(&filtered),
        crate::services::memory_recap::READ_CONVERSATION_CALL_NOTE
    );
    Some((content, signature))
}

// ---------------------------------------------------------------------------
// The capstone.
// ---------------------------------------------------------------------------

/// v4 `buildContext`. Composes the ported context subsystem into a byte-faithful
/// [`BuiltContext`]; the remaining seam is the W4.6b whisper-POSTING half
/// ([`BuildContextSeams`]). Generic over the embedding + completion providers.
#[allow(clippy::too_many_lines)]
pub async fn build_context<E, C, S>(
    db: &Db,
    embedding: &E,
    completion: &C,
    executor: &crate::services::cheap_llm_exec::CheapLlmTaskExecutor,
    seams: &S,
    input: &BuildContextInput,
) -> Result<BuiltContext, BuildContextError>
where
    E: EmbeddingProvider,
    C: CompletionProvider,
    S: BuildContextSeams,
{
    let cpt = DEFAULT_CHARS_PER_TOKEN;
    let now_ms_f = input.now_ms as f64;
    let mut warnings: Vec<String> = Vec::new();

    // Budget.
    let allocation = get_recommended_context_allocation(input.model_context_limit);
    let budget = ContextBudget {
        total_limit: allocation.total_limit,
        system_prompt_budget: allocation.system_prompt,
        memory_budget: allocation.memories,
        knowledge_budget: allocation.knowledge,
        summary_budget: allocation.conversation_summary,
        recent_messages_budget: allocation.recent_messages,
        response_reserve: allocation.response_reserve,
    };

    let is_multi_character = input.responding_participant.is_some()
        && input.all_participants.as_ref().is_some_and(|p| p.len() > 1)
        && input.participant_characters.is_some()
        && input.messages_with_participants.is_some();

    // 1. Other-participants info (multi-character).
    if is_multi_character {
        if let (Some(rp), Some(all), Some(pc)) = (
            &input.responding_participant,
            &input.all_participants,
            &input.participant_characters,
        ) {
            let sys_participants: Vec<SysParticipant> = all
                .iter()
                .map(|p| SysParticipant {
                    id: p.id.clone(),
                    is_character: p.participant_type == "CHARACTER",
                    character_id: p.character_id.clone(),
                    status: parse_sys_status(&p.status),
                })
                .collect();
            let sys_chars: HashMap<String, SysChar> =
                pc.iter().map(|(k, v)| (k.clone(), v.sys.clone())).collect();
            let _ = build_other_participants_info(&rp.id, &sys_participants, &sys_chars);
        }
    }

    let selected_system_prompt_id = input
        .responding_participant
        .as_ref()
        .and_then(|p| p.selected_system_prompt_id.clone());

    // Off-scene announcement — v4's SCAN + content build (W4.6a). The Host POST
    // is W4.6b (the recorded `post_host_off_scene_announcement` seam); here we
    // compute the content this turn's LLM context needs.
    let pending_off_scene_announcement = {
        let participants: Vec<crate::services::off_scene::OffSceneParticipant> = input
            .all_participants
            .as_ref()
            .map(|all| {
                all.iter()
                    .map(|p| crate::services::off_scene::OffSceneParticipant {
                        participant_type: p.participant_type.clone(),
                        character_id: p.character_id.clone(),
                        controlled_by: p.controlled_by.clone(),
                        status: p.status.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let newcomers = crate::services::off_scene::scan_off_scene_newcomers(
            db,
            &input.chat.id,
            &input.user_id,
            &input.character.id,
            input.user_character.as_ref().map(|uc| uc.name.as_str()),
            &participants,
        );
        // The W4.6b Host POST builds + persists the announcement from the newcomer
        // cards and returns its content; v4 surfaces THAT content to this turn's LLM
        // context (`pendingOffSceneAnnouncement = { content: announcement.content }`).
        match newcomers {
            Some(cards) if !cards.is_empty() => {
                seams
                    .post_host_off_scene_announcement(&input.chat.id, &cards)
                    .await
            }
            _ => None,
        }
    };

    // System prompt (block 1).
    let system_prompt = build_system_prompt(&BuildSystemPromptOptions {
        character: &input.character.sys,
        user_character: input.user_character.as_ref(),
        roleplay_template: input.roleplay_template.as_deref(),
        tool_instructions: input.tool_instructions.as_deref(),
        selected_system_prompt_id: selected_system_prompt_id.as_deref(),
        timestamp_config: input.timestamp_config.as_ref(),
        is_initial_message: input.is_initial_message,
        timezone: input.timezone.as_deref(),
        scenario_text: input.chat.scenario_text.as_deref(),
        precompiled_identity_stack: input.chat.precompiled_identity_stack.as_deref(),
        now_ms: input.now_ms,
        local_offset_minutes: input.local_offset_minutes,
    })
    .map_err(|e| BuildContextError::InvalidTimezone(e.0))?;
    let system_prompt_tokens = estimate_tokens(&system_prompt, cpt);

    // Truncate to budget if needed.
    let mut final_system_prompt = system_prompt.clone();
    if system_prompt_tokens > budget.system_prompt_budget {
        warnings.push(format!(
            "System prompt ({system_prompt_tokens} tokens) exceeds budget ({}). Truncating.",
            budget.system_prompt_budget
        ));
        final_system_prompt =
            truncate_to_token_limit(&system_prompt, budget.system_prompt_budget, cpt, "...");
    }
    let final_system_prompt_tokens = estimate_tokens(&final_system_prompt, cpt);

    // ---------------------------------------------------------------------
    // Context compression (budget-driven).
    // ---------------------------------------------------------------------
    let mut budget_info: Option<MaxAvailable> = input.connection_profile.as_ref().map(|p| {
        crate::context_budget::calculate_max_available(
            input.model_context_limit,
            p.max_context,
            p.max_tokens,
            p.model_class.as_deref(),
        )
    });

    // Autonomous-room pacing (v4 context-manager.ts:760–767, U4.4): clamp the
    // model-derived context budget down to this turn's slice of the per-run
    // token budget. This shrinks the whole pie — the compression trigger, the
    // history fold, and the memory budget all derive from `maxAvailable` — so
    // the room spreads its run across multiple turns rather than letting a
    // model with a huge context window spend most of the per-run budget on one
    // turn. Only ever shrinks (guarded by `<`); `None` leaves behavior
    // unchanged.
    if let (Some(bi), Some(cap)) = (budget_info.as_mut(), input.autonomous_context_cap) {
        if cap < bi.max_available {
            bi.max_available = cap;
        }
    }

    // Estimate total conversation tokens.
    let existing_raw: Vec<RawMessage> = input
        .existing_messages
        .iter()
        .map(|m| RawMessage {
            type_: m.message_type.clone(),
            role: Some(m.role.clone()),
            content: Some(m.content.clone()),
        })
        .collect();
    let visible_conversation = extract_visible_conversation(&existing_raw);
    let vis_pairs: Vec<(String, String)> = visible_conversation
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();
    let conversation_tokens = count_messages_tokens(&vis_pairs, cpt);

    let total_estimated_tokens =
        final_system_prompt_tokens + conversation_tokens + budget.memory_budget;

    let compression_enabled = input.context_compression_settings.is_some()
        && input.cheap_llm_selection.is_some()
        && budget_info.is_some()
        && input
            .context_compression_settings
            .as_ref()
            .is_some_and(|s| {
                should_apply_budget_compression(
                    total_estimated_tokens,
                    budget_info.unwrap().max_available,
                    &CompressionSettings {
                        enabled: s.enabled,
                        window_size: s.window_size,
                    },
                    input.bypass_compression,
                )
            });

    let mut compression_result: Option<ContextCompressionResult> = None;
    let mut use_compressed_context = false;

    if compression_enabled {
        let settings = input.context_compression_settings.as_ref().unwrap();
        let selection = input.cheap_llm_selection.as_ref().unwrap();
        let max_available = budget_info.unwrap().max_available;
        let context_history_budget =
            (max_available as f64 * CONTEXT_HISTORY_BUDGET_RATIO).floor() as i64;

        // Phase 1: compress older history if it exceeds the history budget.
        let vis_compressible: Vec<CompressibleMessage> = visible_conversation
            .iter()
            .map(|m| CompressibleMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();
        let split = split_messages_for_compression(&vis_compressible, settings.window_size);
        let compressible_pairs: Vec<(String, String)> = split
            .messages_to_compress
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let compressible_tokens = count_messages_tokens(&compressible_pairs, cpt);

        if compressible_tokens > context_history_budget {
            // Check for a cached compression result first (async pre-compression).
            // A cache hit with `compressionApplied` is used verbatim — the
            // synchronous compression call is SKIPPED (no canned key consumed).
            let cached_hit = input
                .cached_compression_result
                .as_ref()
                .filter(|c| c.compression_applied)
                .cloned();
            if let Some(cached) = cached_hit {
                use_compressed_context = true;
                for w in &cached.warnings {
                    warnings.push(format!("[Compression] {w}"));
                }
                compression_result = Some(cached);
            } else {
                let user_name = input
                    .user_character
                    .as_ref()
                    .map(|u| u.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "User".to_string());
                let uncensored =
                    input
                        .uncensored_fallback
                        .as_ref()
                        .map(|u| UncensoredFallbackOptions {
                            danger_settings: &u.danger_settings,
                            available_profiles: &u.available_profiles,
                            is_dangerous_chat: None,
                        });
                let result = apply_context_compression(
                    completion,
                    executor,
                    &vis_compressible,
                    &final_system_prompt,
                    &ContextCompressionOptions {
                        window_size: settings.window_size,
                        compression_target_tokens: context_history_budget,
                        selection: selection.clone(),
                        character_name: input.character.name.clone(),
                        user_name,
                        uncensored_fallback: uncensored,
                    },
                )
                .await;
                use_compressed_context = result.compression_applied;
                for w in &result.warnings {
                    warnings.push(format!("[Compression] {w}"));
                }
                compression_result = Some(result);
            }
        }
    }

    // Compressed-history block (system block 3) + effective-messages window.
    let mut compressed_history_block: Option<String> = None;
    let mut effective_messages: Vec<ExistingMessage> = input.existing_messages.clone();

    if use_compressed_context {
        if let Some(cr) = &compression_result {
            compressed_history_block =
                build_compressed_history_block(cr.compressed_history.as_deref());

            let visible_messages = extract_visible_conversation(&existing_raw);
            let standard_window_size = input
                .context_compression_settings
                .as_ref()
                .map(|s| s.window_size)
                .unwrap_or(5);
            // When using a fallback cache (computed for fewer visible messages than
            // we have now), include ALL messages since the compression point — not
            // just the standard window — so nothing is lost (v4 dynamic window).
            let effective_window_size = match input.cached_compression_message_count {
                Some(cached_count) if cached_count < visible_messages.len() as i64 => {
                    let messages_since_cache = visible_messages.len() as i64 - cached_count;
                    standard_window_size + messages_since_cache
                }
                _ => standard_window_size,
            };
            let vis_compressible: Vec<CompressibleMessage> = visible_messages
                .iter()
                .map(|m| CompressibleMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect();
            let split = split_messages_for_compression(&vis_compressible, effective_window_size);
            let window_count = split.window_messages.len();

            // Walk backwards to find the last N visible messages in existing_messages.
            let mut found = 0usize;
            let mut window_start = input.existing_messages.len();
            let mut i = input.existing_messages.len();
            while i > 0 && found < window_count {
                i -= 1;
                let msg = &input.existing_messages[i];
                let role = msg.role.to_uppercase();
                let is_visible =
                    msg.message_type.is_none() || msg.message_type.as_deref() == Some("message");
                if is_visible && (role == "USER" || role == "ASSISTANT") {
                    found += 1;
                    window_start = i;
                }
            }
            effective_messages = input.existing_messages[window_start..].to_vec();
        }
    }

    let compressed_history_block_tokens = compressed_history_block
        .as_deref()
        .map(|b| estimate_tokens(b, cpt))
        .unwrap_or(0);
    let effective_system_prompt_tokens =
        final_system_prompt_tokens + compressed_history_block_tokens;

    // ---------------------------------------------------------------------
    // 1b. Memory recap (seam).
    // ---------------------------------------------------------------------
    let mut memory_recap_content = String::new();
    let mut memory_recap_tokens = 0i64;
    if let (true, false, Some(selection)) = (
        input.generate_memory_recap,
        input.character.id.is_empty(),
        input.cheap_llm_selection.as_ref(),
    ) {
        let recap_relevance_query = input
            .new_user_message
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                input
                    .existing_messages
                    .last()
                    .map(|m| m.content.clone())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| input.chat.scenario_text.clone().filter(|s| !s.is_empty()))
            .or_else(|| input.chat.context_summary.clone().filter(|s| !s.is_empty()))
            .unwrap_or_default();
        // v4 `generateMemoryRecap` (W4.6a). Borrow the owned uncensored fallback
        // (the danger-chat fallback for the summarization call).
        let uncensored = input
            .uncensored_fallback
            .as_ref()
            .map(|u| UncensoredFallbackOptions {
                danger_settings: &u.danger_settings,
                available_profiles: &u.available_profiles,
                is_dangerous_chat: None,
            });
        let params = crate::services::memory_recap::MemoryRecapParams {
            character_id: &input.character.id,
            character_name: &input.character.name,
            character_mount_point_id: input.character.character_document_mount_point_id.as_deref(),
            selection,
            user_id: &input.user_id,
            chat_id: Some(input.chat.id.as_str()),
            uncensored_fallback: uncensored.as_ref(),
            max_context: input
                .connection_profile
                .as_ref()
                .and_then(|p| p.max_context),
            relevance_query: &recap_relevance_query,
            embedding_profile_id: input.embedding_profile_id.as_deref(),
            now_ms: now_ms_f,
        };
        let recap = crate::services::memory_recap::generate_memory_recap(
            db, embedding, completion, executor, &params,
        )
        .await;
        if !recap.content.is_empty() {
            memory_recap_tokens = estimate_tokens(&recap.content, cpt);
            memory_recap_content = recap.content;
        }
    }

    // ---------------------------------------------------------------------
    // 2. Memory retrieval (two-pool).
    // ---------------------------------------------------------------------
    let mut memory_content = String::new();
    let mut memory_tokens = 0i64;
    let mut memories_included = 0usize;
    let mut debug_memories: Vec<DebugMemoryInfo> = Vec::new();

    let memory_search_query =
        build_recent_window_query(&input.existing_messages, input.new_user_message.as_deref());
    let mut whispered_memory_ids: Vec<String> = Vec::new();
    // Turn-level recall signals (retrospective / timeRange / entities). The
    // proactive pre-compute path hands them in via `input.recall_signals` (v4
    // `turnRecallSignals = options.recallSignals ?? null`, context-manager.ts:1139);
    // the fallback distillation below fills them when the proactive path didn't run.
    // Consumed by the retrospective head sizing there AND by the mini-recap after
    // the whisper assembles (v4 declares it at function scope for exactly that
    // reason, context-manager.ts:1118).
    let mut turn_recall_signals: Option<crate::services::memory_recap::distill::DistilledSearch> =
        input.recall_signals.clone();

    if !input.skip_memories && !input.character.id.is_empty() {
        let compaction_gen = input.chat.compaction_generation;

        // v4 `getOrComputeFrozenArchive` (W4.6a) — the effective-weight-ranked
        // top-25, byte-stable within a compactionGeneration (process-cached).
        // Budgets are decided AFTER the search below (recall-on-reference: the
        // retrospective head sizing needs the turn signals), so only the fetch
        // happens here; the archive formats once the budgets are known.
        let frozen_archive: Vec<InjectorMemory> =
            crate::services::frozen_archive::get_or_compute_frozen_archive(
                db,
                &input.character.id,
                compaction_gen,
                None,
                now_ms_f,
            )
            .unwrap_or_default()
            .iter()
            .map(injector_memory_from_json)
            .collect();
        let archive_ids: HashSet<String> = frozen_archive.iter().map(|m| m.id.clone()).collect();

        let mut dynamic_head_results: Vec<InjectorResult> = Vec::new();

        if let Some(pre_searched) = input
            .pre_searched_memories
            .as_ref()
            .filter(|p| !p.is_empty())
        {
            // v4 `context-manager.ts:1141-1145` — the `'pre-searched'` memory path.
            // The proactive pre-compute distill (P4.19) already keyword-distilled
            // and semantic-searched this turn; use its results as the dynamic head
            // (archive-overlap filtered), and skip the fallback distill block
            // entirely. `turn_recall_signals` is already seeded from
            // `input.recall_signals` above, so the retrospective head sizing +
            // mini-recap fire identically on this path.
            dynamic_head_results = pre_searched
                .iter()
                .filter(|r| json_str(&r.memory, "id").is_none_or(|id| !archive_ids.contains(&id)))
                .map(injector_result_from_search)
                .collect();
        } else if !memory_search_query.is_empty() {
            // Query distillation (v4 `extractMemorySearchKeywords`, W4.6a). Falls
            // back to the raw recent-window query.
            let mut distilled_query = memory_search_query.clone();
            let mut turn_context: Option<crate::recall_tags::ContextTag> = None;
            let mut turn_temporal: Option<crate::recall_tags::TemporalTag> = None;
            if let Some(selection) = &input.cheap_llm_selection {
                // v4 `recentForDistill`: existingMessages.slice(-12), role
                // lowercased, assistant slice tool-stripped, non-empty; then push
                // newUserMessage.
                let start = input.existing_messages.len().saturating_sub(12);
                let mut recent: Vec<crate::services::memory_recap::distill::DistillMessage> = input
                    .existing_messages[start..]
                    .iter()
                    .map(|m| {
                        let role = m.role.to_lowercase();
                        let content = if role == "assistant" {
                            strip_tool_artifacts(&m.content).unwrap_or_default()
                        } else {
                            m.content.clone()
                        };
                        crate::services::memory_recap::distill::DistillMessage { role, content }
                    })
                    .filter(|m| crate::jsstr::utf16_len(&m.content) > 0)
                    .collect();
                // v4 `if (newUserMessage) recentForDistill.push(...)` — a truthy
                // (non-empty) string only.
                if let Some(num) = input.new_user_message.as_ref().filter(|s| !s.is_empty()) {
                    recent.push(crate::services::memory_recap::distill::DistillMessage {
                        role: "user".to_string(),
                        content: num.clone(),
                    });
                }
                // v4 passes `{ nowIso: new Date().toISOString(), timelineMode:
                // chat.timelineMode ?? 'realtime' }` — the injected now_ms seam
                // stands in for the wall clock.
                let clock = crate::services::memory_recap::distill::ExtractionClock {
                    now_iso: crate::clock::iso_from_unix_ms(now_ms_f as i64),
                    timeline_mode: input
                        .chat
                        .timeline_mode
                        .clone()
                        .unwrap_or_else(|| "realtime".to_string()),
                };
                if let Some(d) = crate::services::memory_recap::distill::distill_memory_search(
                    executor,
                    completion,
                    &recent,
                    &input.character.name,
                    selection,
                    &input.character.id,
                    Some(&clock),
                )
                .await
                {
                    if let Some(p) = d.paraphrase.clone().filter(|p| !p.is_empty()) {
                        distilled_query = p;
                    } else if !d.keywords.is_empty() {
                        distilled_query = d.keywords.join(" ");
                    }
                    turn_context = d.context;
                    turn_temporal = d.temporal;
                    turn_recall_signals = Some(d);
                }
            }

            // Characters present in the room this turn (responding character +
            // every other character participant) for the participant-aware boost
            // (item 4).
            let mut present_about_character_ids: Vec<String> = Vec::new();
            {
                let mut seen: HashSet<String> = HashSet::new();
                if !input.character.id.is_empty() && seen.insert(input.character.id.clone()) {
                    present_about_character_ids.push(input.character.id.clone());
                }
                if let Some(pc) = &input.participant_characters {
                    for id in pc.keys() {
                        if !id.is_empty() && seen.insert(id.clone()) {
                            present_about_character_ids.push(id.clone());
                        }
                    }
                }
            }

            // Read the instance-wide recall settings and assemble the full
            // per-turn recall context so the dynamic head reads the targeting
            // tags back (see recall_tags). chat.projectId is the rename-proof
            // comparand for scope: narrow gating.
            let recall_settings = read_memory_recall_settings(db);
            let fallback_retro = turn_recall_signals
                .as_ref()
                .is_some_and(|s| s.retrospective);
            let recall_context = crate::services::memory_service::RecallContextInput {
                current_project_id: input.chat.project_id.clone(),
                scope_policy: if recall_settings.scope_policy == "exclude" {
                    crate::recall_tags::ScopePolicy::Exclude
                } else {
                    crate::recall_tags::ScopePolicy::DownWeight
                },
                present_about_character_ids,
                turn_context,
                turn_temporal,
                turn_retrospective: fallback_retro,
                expand_related: recall_settings.expand_related,
                recently_whispered_ids: Some(crate::recall_history::recently_whispered_id_set(
                    &input.chat.commonplace_recall_history,
                )),
                // Fresh-event boost (mirrors the proactive path): recent events
                // keep their footing against evergreen memories whatever the
                // retrospective classifier decided; the chat id is the echo
                // guard. (v4 passes `Date.now()`; the injected seam stands in.)
                current_chat_id: Some(input.chat.id.clone()),
                now_ms: Some(now_ms_f),
            };
            // Retrospective multi-probe (mirrors the proactive path in
            // `pre_compute::proactive_recall_task`, now ported — P4.19).
            let mut extra_probes: Vec<String> = Vec::new();
            if fallback_retro {
                if let Some(signals) = &turn_recall_signals {
                    let entity_probe =
                        crate::jsstr::js_trim(&signals.entities.join(" ")).to_string();
                    if !entity_probe.is_empty() {
                        extra_probes.push(entity_probe);
                    }
                    if let (Some(p), Some(tr)) = (&signals.paraphrase, &signals.time_range) {
                        extra_probes.push(format!(
                            "{p} (around {} to {})",
                            crate::jsstr::utf16_truncate(&tr.from, 10),
                            crate::jsstr::utf16_truncate(&tr.to, 10)
                        ));
                    }
                }
            }
            let results = search_memories_semantic(
                db,
                embedding,
                &input.character.id,
                &distilled_query,
                &SemanticSearchOptions {
                    user_id: input.user_id.clone(),
                    embedding_profile_id: input.embedding_profile_id.clone(),
                    // Pull a few more than the head size so the archive-overlap
                    // filter still leaves enough candidates to fill the head.
                    // Retrospective turns run the enlarged head, so pull
                    // proportionally more.
                    limit: Some(
                        (if fallback_retro {
                            RETRO_HEAD_SIZE
                        } else {
                            DYNAMIC_HEAD_DEFAULT_SIZE
                        }) * 3,
                    ),
                    min_importance: Some(input.min_memory_importance),
                    recall_context: Some(recall_context),
                    // v4 passes `turnRecallSignals?.entities` on EVERY turn.
                    entity_anchors: turn_recall_signals
                        .as_ref()
                        .map(|s| s.entities.clone())
                        .unwrap_or_default(),
                    occurred_within: if fallback_retro {
                        turn_recall_signals
                            .as_ref()
                            .and_then(|s| s.time_range.clone())
                    } else {
                        None
                    },
                    extra_probes,
                    now_ms: now_ms_f,
                    ..Default::default()
                },
            )
            .await?;
            dynamic_head_results = results
                .iter()
                .filter(|r| json_str(&r.memory, "id").is_none_or(|id| !archive_ids.contains(&id)))
                .map(injector_result_from_search)
                .collect();
        }

        // Recall-on-reference (fourth cadence, part 1): a retrospective turn
        // gets an enlarged head — the turn that needs rich recall is exactly
        // the turn that otherwise gets the smallest window. Budgets decided
        // here, AFTER the signals are known, so the archive takes what's left.
        let is_retrospective_turn = turn_recall_signals
            .as_ref()
            .is_some_and(|s| s.retrospective);
        let dynamic_head_budget = (if is_retrospective_turn {
            RETRO_HEAD_TOKEN_BUDGET
        } else {
            DYNAMIC_HEAD_TOKEN_BUDGET
        })
        .min(budget.memory_budget);
        let archive_budget = (budget.memory_budget - dynamic_head_budget).max(0);
        let archive_formatted = format_frozen_memory_archive(&frozen_archive, archive_budget);

        let head_formatted = format_dynamic_memory_head(
            &dynamic_head_results,
            Some(dynamic_head_budget),
            Some(if is_retrospective_turn {
                RETRO_HEAD_SIZE
            } else {
                DYNAMIC_HEAD_DEFAULT_SIZE
            }),
            now_ms_f,
        );

        whispered_memory_ids = head_formatted
            .debug_memories
            .iter()
            .filter_map(|d| d.memory_id.clone())
            .filter(|id| !id.is_empty())
            .collect();

        let mut sections: Vec<String> = Vec::new();
        if !archive_formatted.content.is_empty() {
            sections.push(archive_formatted.content.clone());
        }
        if !head_formatted.content.is_empty() {
            sections.push(head_formatted.content.clone());
        }
        memory_content = sections.join("\n\n");
        memory_tokens = archive_formatted.token_count + head_formatted.token_count;
        memories_included = archive_formatted.memories_used + head_formatted.memories_used;
        debug_memories = archive_formatted
            .debug_memories
            .into_iter()
            .chain(head_formatted.debug_memories)
            .collect();
    }

    // ---------------------------------------------------------------------
    // 2a-bis. Scene state.
    // ---------------------------------------------------------------------
    let cache_target_key: String = if is_multi_character {
        input
            .responding_participant
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "__public__".to_string())
    } else {
        "__public__".to_string()
    };

    let (current_state_content, current_state_tokens, emitted_scene_state) = {
        let parsed_scene = parse_scene_state(input.chat.scene_state.as_ref());
        let scene_time: Option<String> = compute_scene_time(input);
        // Live-clothing override — v4's per-character equipped-wardrobe resolution
        // (W4.6a, closed with existing wardrobe code + the small hash/decorate
        // leaves). Only overrides when the character's wardrobe changed since the
        // cached `clothingHash`.
        let live_clothing_map: HashMap<String, String> =
            resolve_live_clothing(db, input, parsed_scene.as_ref());
        // Prior-emission cache: the per-recipient `commonplaceSceneCache[targetKey]`
        // (v4 `priorEmissionByCharacter`), read from the chat's stored cache.
        let prior_emission_map = read_prior_scene_emission(input, &cache_target_key);
        let live_lookup = |cid: &str| live_clothing_map.get(cid).cloned();
        let prior_lookup = |cid: &str| prior_emission_map.get(cid).cloned();
        let now_iso = crate::clock::iso_from_unix_ms(input.now_ms);
        let formatted = format_current_scene_state(
            parsed_scene.as_ref(),
            scene_time.as_deref(),
            &now_iso,
            &live_lookup,
            &prior_lookup,
        );
        (
            formatted.content,
            formatted.token_count,
            formatted.emitted_by_character,
        )
    };

    // ---------------------------------------------------------------------
    // 2b. Inter-character memories (multi-character).
    // ---------------------------------------------------------------------
    let mut inter_character_memory_content = String::new();
    let mut inter_character_memory_tokens = 0i64;
    let mut inter_character_memories_included = 0usize;
    let mut debug_inter_character_memories: Vec<DebugInterCharacterMemoryInfo> = Vec::new();

    if !input.skip_memories && is_multi_character && !input.character.id.is_empty() {
        let (Some(pc), Some(all)) = (&input.participant_characters, &input.all_participants) else {
            unreachable!("is_multi_character guarantees these are Some");
        };
        let mut other_character_ids: Vec<String> = Vec::new();
        let mut other_character_names: HashMap<String, String> = HashMap::new();
        for p in all {
            if p.participant_type == "CHARACTER" {
                if let Some(cid) = &p.character_id {
                    if cid != &input.character.id {
                        if let Some(oc) = pc.get(cid) {
                            other_character_ids.push(oc.id.clone());
                            other_character_names.insert(oc.id.clone(), oc.name.clone());
                        }
                    }
                }
            }
        }

        let inter_character_budget =
            ((budget.memory_budget - memory_tokens - current_state_tokens) as f64 / 2.0).floor()
                as i64;

        if !other_character_ids.is_empty() && inter_character_budget > 0 {
            let ids = other_character_ids.clone();
            let cid = input.character.id.clone();
            let importance_rows = db.read_main(move |conn| {
                crate::db::memories_read::find_by_character_about_characters(
                    conn,
                    &cid,
                    &ids,
                    INTER_CHAR_PER_CHARACTER_LIMIT,
                )
            })?;
            let importance_memories: Vec<InjectorMemory> = importance_rows
                .iter()
                .map(injector_memory_from_json)
                .collect();

            // Relevance half — semantic search per other character.
            let mut relevance_results: Vec<InjectorResult> = Vec::new();
            if !memory_search_query.is_empty() {
                for other_id in &other_character_ids {
                    let r = search_memories_semantic(
                        db,
                        embedding,
                        &input.character.id,
                        &memory_search_query,
                        &SemanticSearchOptions {
                            user_id: input.user_id.clone(),
                            embedding_profile_id: input.embedding_profile_id.clone(),
                            limit: Some(INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT),
                            min_importance: Some(input.min_memory_importance),
                            about_character_id: Some(other_id.clone()),
                            now_ms: now_ms_f,
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap_or_default();
                    relevance_results.extend(r.iter().map(injector_result_from_search));
                }
            }

            if !importance_memories.is_empty() || !relevance_results.is_empty() {
                let names_lookup = |id: &str| other_character_names.get(id).cloned();
                let formatted = format_inter_character_memories_for_context(
                    &importance_memories,
                    &names_lookup,
                    inter_character_budget,
                    &relevance_results,
                    now_ms_f,
                );
                // (arg order matches the ported signature: importance, names,
                // max_tokens, relevance, now_ms.)
                inter_character_memory_content = formatted.content;
                inter_character_memory_tokens = formatted.token_count;
                inter_character_memories_included = formatted.memories_used;
                debug_inter_character_memories = formatted.debug_memories;
            }
        }
    }

    // ---------------------------------------------------------------------
    // 2c. Knowledge retrieval.
    // ---------------------------------------------------------------------
    let mut knowledge_content = String::new();
    let mut knowledge_tokens = 0i64;
    let mut debug_knowledge: Vec<DebugKnowledgeOut> = Vec::new();

    if !input.skip_memories
        && !input.character.id.is_empty()
        && !memory_search_query.is_empty()
        && budget.knowledge_budget > 0
    {
        // v4 `resolveTieredMountPool` — the real DB-reading tier walk (W4.6a,
        // closed with the ported [`crate::db::tiered_mount_pool`]). No ownership
        // gate / participant tier here (v4's buildContext call passes neither).
        let pool = resolve_mount_pool(db, input);
        if pool.character_mount_point_id.is_some()
            || !pool.group_mount_point_ids.is_empty()
            || !pool.project_mount_point_ids.is_empty()
            || pool.global_mount_point_id.is_some()
        {
            let result = retrieve_knowledge_for_turn(
                db,
                embedding,
                &KnowledgeRetrievalParams {
                    character_id: input.character.id.clone(),
                    user_id: input.user_id.clone(),
                    embedding_profile_id: input.embedding_profile_id.clone(),
                    query: memory_search_query.clone(),
                    character_mount_point_id: pool.character_mount_point_id,
                    group_mount_point_ids: pool.group_mount_point_ids,
                    project_mount_point_ids: pool.project_mount_point_ids,
                    global_mount_point_id: pool.global_mount_point_id,
                    budget_tokens: budget.knowledge_budget,
                    chars_per_token: cpt,
                    ..Default::default()
                },
            )
            .await?;
            knowledge_content = result.content;
            knowledge_tokens = result.token_count;
            debug_knowledge = result
                .debug
                .into_iter()
                .map(|d| DebugKnowledgeOut {
                    file_path: d.file_path,
                    tier: d.tier.debug_key().to_string(),
                    score: d.score,
                    inline: d.inline,
                    token_count: d.token_count,
                })
                .collect();
        }
    }

    // ---------------------------------------------------------------------
    // Phase 2: memory compression (budget-driven) — TRACKED DEFERRAL.
    //
    // v4 compresses the assembled memory / inter-character blocks via
    // `compressMemories` (`lib/memory/cheap-llm-tasks/compression-tasks.ts`)
    // when the compression path is active AND the memory total exceeds
    // `maxAvailable * MEMORY_BUDGET_RATIO`. That task is a distinct cheap-LLM
    // template not yet ported (`services::compression` ports only the
    // conversation-history compressor). The corpus keeps memory tokens under
    // the ratio, so this branch is a verified no-op both sides (the `else`
    // "within budget" log arm). Close with the `compressMemories` task port.
    //
    // P4.d15 (2026-07-22): the PORT TARGET has moved to v4 `8bf3cb5f`. The
    // episodic overhaul edited `MEMORY_COMPRESSION_PROMPT` in place — KEEP gains
    // "Dates attached to events — keep the date with the event it dates", and the
    // two DROP lines ("Exact dates/times when relative timing is sufficient" /
    // "Details about concluded events with no ongoing impact") are deleted, so the
    // compressor must now PRESERVE dates rather than shed them. That flip is
    // prompt text inside the unported task; there is nothing to change here until
    // the task itself lands, at which point the prompt must be transcribed from
    // `8bf3cb5f`, not from the older text.
    //
    // MEMORY_BUDGET_RATIO is referenced here to keep the constant live.
    let _memory_budget_ratio = MEMORY_BUDGET_RATIO;

    // 3. Summary rides as a Librarian whisper → no dedicated section.
    let summary_tokens = 0i64;

    // 4. Remaining budget for messages.
    let used_tokens = effective_system_prompt_tokens
        + memory_recap_tokens
        + memory_tokens
        + inter_character_memory_tokens
        + summary_tokens;
    let remaining_budget = budget.total_limit - used_tokens - budget.response_reserve;

    // 5. Prepare messages.
    let mut messages_to_process: Vec<SelectableMessage> = if is_multi_character {
        let rp = input.responding_participant.as_ref().unwrap();
        let all = input.all_participants.as_ref().unwrap();
        let pc = input.participant_characters.as_ref().unwrap();
        let mwp = input.messages_with_participants.as_ref().unwrap();

        // History-access filter.
        let responding = all.iter().find(|p| p.id == rp.id);
        let has_history = responding.map(|p| p.has_history_access).unwrap_or(true);
        let join_ms = responding
            .and_then(|p| p.created_at.as_deref())
            .and_then(crate::clock::iso_to_ms)
            .map(|ms| ms as f64)
            .unwrap_or(0.0);
        let hist_msgs: Vec<crate::message_attribution::HistoryMessage> = mwp
            .iter()
            .map(|m| crate::message_attribution::HistoryMessage {
                created_at_ms: m
                    .created_at
                    .as_deref()
                    .and_then(crate::clock::iso_to_ms)
                    .map(|ms| ms as f64),
            })
            .collect();
        let hist_mask = filter_messages_by_history_access(&hist_msgs, has_history, join_ms);

        // Whisper filter.
        let attr_msgs: Vec<AttributionMessage> = mwp
            .iter()
            .map(|m| AttributionMessage {
                id: m.id.clone(),
                role: m.role.clone(),
                content: m.content.clone(),
                participant_id: m.participant_id.clone(),
                thought_signature: m.thought_signature.clone(),
                created_at: m.created_at.clone(),
                target_participant_ids: m.target_participant_ids.clone(),
                host_event: None,
            })
            .collect();
        let filtered: Vec<AttributionMessage> = attr_msgs
            .iter()
            .zip(&hist_mask)
            .filter_map(|(m, keep)| if *keep { Some(m.clone()) } else { None })
            .collect();
        let whisper_mask = filter_whisper_messages(&filtered, &rp.id);
        let whisper_filtered: Vec<AttributionMessage> = filtered
            .iter()
            .zip(&whisper_mask)
            .filter_map(|(m, keep)| if *keep { Some(m.clone()) } else { None })
            .collect();

        // Attribution.
        let attr_participants: Vec<AttributionParticipant> = all
            .iter()
            .map(|p| AttributionParticipant {
                id: p.id.clone(),
                participant_type: p.participant_type.clone(),
                character_id: p.character_id.clone(),
                controlled_by: p.controlled_by.clone(),
                status: parse_attr_status(&p.status),
            })
            .collect();
        let name_map: HashMap<String, String> = pc
            .iter()
            .map(|(k, v)| (k.clone(), v.name.clone()))
            .collect();
        let attributed = attribute_messages_for_character(
            &whisper_filtered,
            &rp.id,
            &name_map,
            &attr_participants,
        );
        attributed
            .into_iter()
            .map(|m| SelectableMessage {
                role: m.role.to_string(),
                content: m.content,
                id: m.id,
                name: m.name,
                participant_id: m.participant_id,
                thought_signature: m.thought_signature,
            })
            .collect()
    } else {
        effective_messages
            .iter()
            .map(|m| SelectableMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                id: m.id.clone(),
                name: None,
                participant_id: None,
                thought_signature: m.thought_signature.clone(),
            })
            .collect()
    };

    // Drop summary-anchored messages.
    let summary_anchor_ids: HashSet<String> = input
        .chat
        .summary_anchor_message_ids
        .iter()
        .cloned()
        .collect();
    if !summary_anchor_ids.is_empty() {
        messages_to_process.retain(|m| {
            m.id.as_deref()
                .is_none_or(|id| !summary_anchor_ids.contains(id))
        });
    }

    // 6. Select recent messages.
    let recent_budget = remaining_budget.min(budget.recent_messages_budget.floor() as i64);
    let selection = select_recent_messages(&messages_to_process, recent_budget, cpt);
    let selected_messages = selection.messages;
    let messages_tokens = selection.token_count;
    let truncated = selection.truncated;

    if truncated {
        let all_pairs: Vec<(String, String)> = messages_to_process
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let total_message_tokens = count_messages_tokens(&all_pairs, cpt);
        if should_summarize_conversation(
            messages_to_process.len() as i64,
            total_message_tokens,
            budget.total_limit,
        ) {
            warnings.push(
                "Conversation is getting long. Consider generating a summary for better context management."
                    .to_string(),
            );
        }
    }

    // 7. New user message tokens.
    let new_user_message_tokens = input
        .new_user_message
        .as_deref()
        .map(|m| estimate_tokens(m, cpt) + 4)
        .unwrap_or(0);

    // 8. Assemble.
    let mut context_messages: Vec<ContextMessage> = Vec::new();

    // Block 1.
    context_messages.push(ContextMessage {
        role: "system",
        content: final_system_prompt.clone(),
        metadata: Some(ContextMessageMetadata {
            is_injected: Some(true),
            ..Default::default()
        }),
        thought_signature: None,
        name: None,
        cache_control: None,
    });
    // Block 2.
    let identity_reminder = build_identity_reinforcement(&input.character.name);
    context_messages.push(ContextMessage {
        role: "system",
        content: identity_reminder,
        metadata: Some(ContextMessageMetadata {
            is_injected: Some(true),
            ..Default::default()
        }),
        thought_signature: None,
        name: None,
        cache_control: None,
    });
    // Block 3.
    if let Some(block) = &compressed_history_block {
        context_messages.push(ContextMessage {
            role: "system",
            content: block.clone(),
            metadata: Some(ContextMessageMetadata {
                is_injected: Some(true),
                ..Default::default()
            }),
            thought_signature: None,
            name: None,
            cache_control: None,
        });
    }

    // Selected conversation messages, with the summary-head cache breakpoint.
    let mut summary_breakpoint_placed = false;
    for msg in &selected_messages {
        let is_summary_head =
            !summary_breakpoint_placed && msg.content.starts_with(SUMMARY_CONTENT_PREFIX);
        let role: &'static str = if msg.role.to_lowercase() == "assistant" {
            "assistant"
        } else {
            "user"
        };
        context_messages.push(ContextMessage {
            role,
            content: msg.content.clone(),
            metadata: None,
            thought_signature: msg.thought_signature.clone(),
            name: msg.name.clone(),
            cache_control: if is_summary_head {
                Some(CacheControl { kind: "ephemeral" })
            } else {
                None
            },
        });
        if is_summary_head {
            summary_breakpoint_placed = true;
        }
    }

    // Off-scene announcement push (role=user).
    if let Some(content) = &pending_off_scene_announcement {
        context_messages.push(ContextMessage {
            role: "user",
            content: content.clone(),
            metadata: None,
            thought_signature: None,
            name: None,
            cache_control: None,
        });
    }

    // Timestamp whisper push (+ the post-office post seam).
    if should_inject_timestamp_now(input) {
        let cfg = input.timestamp_config.as_ref().unwrap();
        let ts = crate::chat_timestamp::calculate_current_timestamp(
            cfg,
            input.timezone.as_deref(),
            input.now_ms,
            input.local_offset_minutes,
        )
        .map_err(|e| BuildContextError::InvalidTimezone(e.0))?;
        seams
            .post_host_timestamp_announcement(&input.chat.id, &ts.formatted)
            .await;
        context_messages.push(ContextMessage {
            role: "user",
            content: build_timestamp_content(&ts.formatted),
            metadata: None,
            thought_signature: None,
            name: None,
            cache_control: None,
        });
    }

    // Core whisper (before commonplace) — v4's trigger + READ/ASSEMBLY (W4.6a);
    // its LLM-context contribution folds into the new user message below. The
    // whisper POST + stale sweep are W4.6b (the recorded seam).
    let core_whisper_llm_context = match &input.responding_participant {
        Some(rp) if !input.is_continue_mode => {
            resolve_core_whisper_llm_context(db, input, &rp.id, seams).await
        }
        _ => String::new(),
    };

    // Recall-on-reference (fourth cadence, part 2): on a retrospective turn, build
    // the scoped mini-recap — a dated, drillable Relevant Past Conversations list
    // from the vault summaries, scoped by the turn's timeRange/entities and closed
    // by the read_conversation call note. Spam guard: skip when a block with the
    // same signature was emitted within the last few turns (piggybacking the
    // recall-history ring buffer), and dedup conversation IDs against the current
    // on-fold relevant-conversations whisper. Best-effort — never blocks the turn.
    let (retrospective_recall_content, emitted_retro_signature) = match &turn_recall_signals {
        Some(signals) if signals.retrospective && !input.character.id.is_empty() => {
            match resolve_retrospective_mini_recap(
                db,
                embedding,
                input,
                signals,
                &memory_search_query,
                is_multi_character,
            )
            .await
            {
                Some((content, signature)) => (content, Some(signature)),
                None => (String::new(), None),
            }
        }
        _ => (String::new(), None),
    };

    // Commonplace whisper post + scene-cache + recall-history persistence (seams).
    // Reuse the canonical builders from `commonplace_notifications` (Group 5 dedup);
    // the per-turn consolidated whisper leaves `relevant_conversations` empty (only
    // the fold-refresh whisper sets it), so the output is identical to the removed
    // private copies. The retrospective mini-recap rides the LLM recall text here
    // but is posted as its OWN `retrospective-recall` whisper below — NEVER inside
    // the consolidated body.
    let cmpb_parts = crate::services::commonplace_notifications::CommonplaceParts {
        current_state: opt(&current_state_content),
        recap: opt(&memory_recap_content),
        relevant: opt(&memory_content),
        inter_char: opt(&inter_character_memory_content),
        knowledge: opt(&knowledge_content),
        relevant_conversations: None,
        retrospective_recall: None,
    };
    let persona_whisper =
        crate::services::commonplace_notifications::build_commonplace_persona_whisper(&cmpb_parts);
    let llm_recall_text = crate::services::commonplace_notifications::build_commonplace_llm_context(
        &crate::services::commonplace_notifications::CommonplaceParts {
            retrospective_recall: opt(&retrospective_recall_content),
            ..cmpb_parts.clone()
        },
    );

    if !persona_whisper.is_empty() {
        let persona = &persona_whisper;
        let target = if is_multi_character {
            input.responding_participant.as_ref().map(|p| p.id.as_str())
        } else {
            None
        };
        // The Commonplace whisper POST is W4.6b (the recorded seam). The two
        // persist writes are W4.6a — gated on `posted`, exactly as v4's
        // `if (posted)`. The seam returns whether a whisper was durably posted.
        let posted = seams
            .post_commonplace_whisper(&input.chat.id, target, persona)
            .await;
        if posted {
            // v4 `chats.update({ commonplaceSceneCache: {...prior, [key]: slice} })`.
            if !emitted_scene_state.is_empty() {
                persist_scene_cache(db, input, &cache_target_key, &emitted_scene_state).await;
            }
            // v4 `chats.update({ commonplaceRecallHistory: appendRecallTurn(...) })`,
            // plus the retrospective mini-recap signature (the fourth cadence's spam
            // guard) — ONE write, gated on either half having something to record.
            if !whispered_memory_ids.is_empty() || emitted_retro_signature.is_some() {
                let mut next = crate::recall_history::append_recall_turn(
                    &input.chat.commonplace_recall_history,
                    &whispered_memory_ids,
                );
                if let Some(sig) = &emitted_retro_signature {
                    next = crate::recall_history::append_retro_signature(&next, sig);
                }
                // v4 persists the RecallHistory OBJECT (`{ turns, retroSignatures? }`),
                // never the bare turns array — the parse side requires the wrapper.
                persist_recall_history(db, &input.chat.id, next.to_value()).await;
            }
        }
    }

    // Recall-on-reference: post the mini-recap as its OWN whisper (kind
    // `retrospective-recall`) so the salon shows the dated conversation list
    // distinctly. Posted AFTER the consolidated sweep snapshot so it survives this
    // turn; the next turn's sweep removes it (turn-specific by design — unlike
    // `relevant-conversations`, which the sweep exempts). Note this sits OUTSIDE
    // the `if (personaWhisper)` block: a turn with no ordinary recall still gets
    // its mini-recap.
    if !retrospective_recall_content.is_empty() {
        let target = if is_multi_character {
            input.responding_participant.as_ref().map(|p| p.id.as_str())
        } else {
            None
        };
        let content = crate::services::commonplace_notifications::build_commonplace_persona_whisper(
            &crate::services::commonplace_notifications::CommonplaceParts {
                retrospective_recall: Some(retrospective_recall_content.clone()),
                ..Default::default()
            },
        );
        seams
            .post_commonplace_retrospective_recall(&input.chat.id, target, &content)
            .await;
    }

    // Suparṇā mail — v4's READ half (W4.6a): collect unalerted mail, build the
    // LLM context, flip each `alerted` flag. The whisper POST is W4.6b (fired via
    // the recorded seam when there is unalerted mail).
    let suparna_mail_llm_context =
        match input.character.character_document_mount_point_id.as_deref() {
            Some(vault) => {
                let (ctx, unalerted) =
                    crate::services::suparna_mail::resolve_suparna_mail_context(db, vault).await;
                if !unalerted.is_empty() {
                    // v4 targets the responding participant in multi-character chats,
                    // public otherwise; the whisper body is built from the letters.
                    let target = if is_multi_character {
                        input.responding_participant.as_ref().map(|p| p.id.as_str())
                    } else {
                        None
                    };
                    seams
                        .post_suparna_mail(&input.chat.id, &unalerted, target)
                        .await;
                }
                ctx
            }
            None => String::new(),
        };

    // "Nothing to add" turn-skipping: build the ephemeral Turn note when the
    // orchestrator has decided this character may pass. Injected as a trailing
    // context section on the new user message when there is one, or as its own
    // trailing user message on chained/continue turns (no newUserMessage). Never
    // persisted.
    let turn_skip_instruction = match &input.turn_skip {
        Some(ts) if ts.offer_skip => {
            build_turn_skip_instruction(&ts.character_name, ts.recently_addressed)
        }
        _ => String::new(),
    };

    // New user message with trailing recall / core / mail context.
    let mut messages_included = selected_messages.len();
    if let Some(new_user_message) = &input.new_user_message {
        let new_user_msg_name = if is_multi_character {
            let all = input.all_participants.as_ref().unwrap();
            let pc = input.participant_characters.as_ref().unwrap();
            let attr_participants: Vec<AttributionParticipant> = all
                .iter()
                .map(|p| AttributionParticipant {
                    id: p.id.clone(),
                    participant_type: p.participant_type.clone(),
                    character_id: p.character_id.clone(),
                    controlled_by: p.controlled_by.clone(),
                    status: parse_attr_status(&p.status),
                })
                .collect();
            let name_map: HashMap<String, String> = pc
                .iter()
                .map(|(k, v)| (k.clone(), v.name.clone()))
                .collect();
            find_user_participant_name(
                &attr_participants,
                &name_map,
                input.active_user_participant_id.as_deref(),
            )
        } else {
            None
        };

        let mut trailing: Vec<String> = Vec::new();
        if !core_whisper_llm_context.is_empty() {
            trailing.push(core_whisper_llm_context.clone());
        }
        if !llm_recall_text.is_empty() {
            trailing.push(llm_recall_text.clone());
        }
        if !suparna_mail_llm_context.is_empty() {
            trailing.push(suparna_mail_llm_context.clone());
        }
        if !turn_skip_instruction.is_empty() {
            trailing.push(turn_skip_instruction.clone());
        }
        let composed = if trailing.is_empty() {
            new_user_message.clone()
        } else {
            format!(
                "{new_user_message}\n\n---\n\n{}",
                trailing.join("\n\n---\n\n")
            )
        };
        context_messages.push(ContextMessage {
            role: "user",
            content: composed,
            metadata: None,
            thought_signature: None,
            name: new_user_msg_name,
            cache_control: None,
        });
        messages_included += 1;
    } else if !turn_skip_instruction.is_empty() {
        // Chained / continue turns carry no new user message, so the note can't
        // ride as a trailing section above. Push it as its own trailing user
        // message (same off-scene/timestamp pattern) so the model sees it this
        // turn. Anthropic 4.6+ rejects role=assistant tails, so 'user' is required.
        context_messages.push(ContextMessage {
            role: "user",
            content: turn_skip_instruction,
            metadata: None,
            thought_signature: None,
            name: None,
            cache_control: None,
        });
    }

    // Token accounting.
    let total_memory_tokens = memory_recap_tokens + memory_tokens + inter_character_memory_tokens;
    let total_used = effective_system_prompt_tokens
        + total_memory_tokens
        + knowledge_tokens
        + summary_tokens
        + messages_tokens
        + new_user_message_tokens;
    let total_memories_included = memories_included + inter_character_memories_included;

    let debug_system_prompt = match &compressed_history_block {
        Some(block) => format!("{final_system_prompt}\n\n{block}"),
        None => final_system_prompt.clone(),
    };

    Ok(BuiltContext {
        messages: context_messages,
        token_usage: TokenUsage {
            system_prompt: effective_system_prompt_tokens,
            memories: total_memory_tokens,
            knowledge: knowledge_tokens,
            summary: summary_tokens,
            recent_messages: messages_tokens + new_user_message_tokens,
            total: total_used,
        },
        budget,
        included_summary: summary_tokens > 0,
        memories_included: total_memories_included,
        messages_included,
        messages_truncated: truncated,
        warnings,
        debug_memories: debug_memories.iter().map(debug_memory_json).collect(),
        debug_inter_character_memories: if debug_inter_character_memories.is_empty() {
            None
        } else {
            Some(
                debug_inter_character_memories
                    .into_iter()
                    .map(|d| DebugInterCharacterOut {
                        about_character_name: d.about_character_name,
                        summary: d.summary,
                        importance: d.importance,
                    })
                    .collect(),
            )
        },
        debug_knowledge: if debug_knowledge.is_empty() {
            None
        } else {
            Some(debug_knowledge)
        },
        debug_memory_recap: opt(&memory_recap_content),
        debug_summary: input.chat.context_summary.clone().filter(|s| !s.is_empty()),
        debug_system_prompt,
        original_system_prompt: final_system_prompt,
        compression_applied: use_compressed_context,
        compression_details: compression_result
            .as_ref()
            .and_then(|r| r.compression_details.as_ref())
            .map(|d| CompressionDetailsOut {
                original_message_count: d.original_message_count,
                compressed_message_count: d.compressed_message_count,
                window_message_count: d.window_message_count,
                original_history_tokens: d.original_history_tokens,
                compressed_history_tokens: d.compressed_history_tokens,
                original_system_prompt_tokens: d.original_system_prompt_tokens,
                compressed_system_prompt_tokens: d.compressed_system_prompt_tokens,
                total_savings: d.total_savings,
            }),
    })
}

// ---------------------------------------------------------------------------
// Small helpers.
// ---------------------------------------------------------------------------

fn opt(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Serialize a [`DebugMemoryInfo`] to v4's runtime `JSON.stringify` shape: keys
/// in insertion order (`memoryId` first when present, then summary / importance /
/// score / effectiveWeight, then the dynamic-head extras), each `None` dropped
/// (JS `undefined`); the float cells rendered via `js_number_to_json` so an
/// integer-valued weight/score emits bare (`1`, not `1.0`).
fn debug_memory_json(d: &DebugMemoryInfo) -> Value {
    let num = crate::db::js_number_to_json;
    let mut m = serde_json::Map::new();
    if let Some(id) = &d.memory_id {
        m.insert("memoryId".into(), Value::from(id.clone()));
    }
    m.insert("summary".into(), Value::from(d.summary.clone()));
    m.insert("importance".into(), num(d.importance));
    m.insert("score".into(), num(d.score));
    m.insert("effectiveWeight".into(), num(d.effective_weight));
    if let Some(rw) = d.raw_weight {
        m.insert("rawWeight".into(), num(rw));
    }
    if let Some(rm) = d.recall_multiplier {
        m.insert("recallMultiplier".into(), num(rm));
    }
    if let Some(rf) = &d.recall_fired {
        m.insert(
            "recallFired".into(),
            Value::Array(rf.iter().map(|s| Value::from(s.clone())).collect()),
        );
    }
    if let Some(bb) = d.blended_before {
        m.insert("blendedBefore".into(), num(bb));
    }
    if let Some(ba) = d.blended_after {
        m.insert("blendedAfter".into(), num(ba));
    }
    Value::Object(m)
}

fn parse_sys_status(s: &str) -> Option<crate::system_prompt::ParticipantStatus> {
    use crate::system_prompt::ParticipantStatus as P;
    match s {
        "active" => Some(P::Active),
        "silent" => Some(P::Silent),
        "absent" => Some(P::Absent),
        "removed" => Some(P::Removed),
        _ => None,
    }
}

fn parse_attr_status(s: &str) -> crate::chat_predicates::ParticipantStatus {
    use crate::chat_predicates::ParticipantStatus as P;
    match s {
        "silent" => P::Silent,
        "absent" => P::Absent,
        "removed" => P::Removed,
        _ => P::Active,
    }
}

/// Parse the chat's raw `sceneState` (JSON string or object) → [`SceneState`],
/// reproducing v4's `SceneStateSchema.safeParse` fallthrough (a bad shape → None).
fn parse_scene_state(raw: Option<&Value>) -> Option<SceneState> {
    let raw = raw?;
    if raw.is_null() {
        return None;
    }
    let candidate: Value = match raw {
        Value::String(s) => serde_json::from_str(s).ok()?,
        other => other.clone(),
    };
    let obj = candidate.as_object()?;
    let location = obj
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let chars = obj.get("characters").and_then(Value::as_array)?;
    let mut characters = Vec::new();
    for c in chars {
        let co = c.as_object()?;
        characters.push(crate::memory_injector::SceneStateCharacter {
            character_id: co
                .get("characterId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            character_name: co
                .get("characterName")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            action: co
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            clothing: co
                .get("clothing")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Some(SceneState {
        location,
        characters,
    })
}

/// v4's scene-time gate: `autoPrepend && shouldInjectTimestamp(...)` → the
/// formatted current timestamp.
fn compute_scene_time(input: &BuildContextInput) -> Option<String> {
    let cfg = input.timestamp_config.as_ref()?;
    if !cfg.auto_prepend {
        return None;
    }
    if !crate::chat_timestamp::should_inject_timestamp(
        Some(cfg),
        input.is_initial_message,
        input.minutes_since_last_timestamp_announcement,
    ) {
        return None;
    }
    crate::chat_timestamp::calculate_current_timestamp(
        cfg,
        input.timezone.as_deref(),
        input.now_ms,
        input.local_offset_minutes,
    )
    .ok()
    .map(|t| t.formatted)
}

/// The assembly-site timestamp gate (same predicate as [`compute_scene_time`]).
fn should_inject_timestamp_now(input: &BuildContextInput) -> bool {
    let Some(cfg) = input.timestamp_config.as_ref() else {
        return false;
    };
    cfg.auto_prepend
        && crate::chat_timestamp::should_inject_timestamp(
            Some(cfg),
            input.is_initial_message,
            input.minutes_since_last_timestamp_announcement,
        )
}

/// v4 `buildTimestampContent` (host-notifications writer): the timestamp whisper
/// body pushed into the LLM context.
fn build_timestamp_content(formatted: &str) -> String {
    format!("The Host marks the time as {formatted}.")
}

// Commonplace-Book whisper assembly (`CommonplaceParts` +
// `build_commonplace_persona_whisper` / `build_commonplace_llm_context`) is reused
// from `crate::services::commonplace_notifications` (Group 5 dedup) — the private
// copies that used to live here were byte-identical for the per-turn consolidated
// whisper (which never sets `relevant_conversations`).
