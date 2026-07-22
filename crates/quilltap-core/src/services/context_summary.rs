//! The context-summary **service half** (Phase-3 Unit-3 wave 3) — v4
//! `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
//! `checkAndGenerateSummaryIfNeeded` (`lib/chat/context-summary.ts`): the fold
//! write + title enqueue, over the ported cadence leaves
//! ([`crate::context_summary`]) and the cheap-LLM executor
//! ([`crate::services::cheap_llm_exec`]).
//!
//! ## Shape of the fold write ([`generate_context_summary`])
//!
//! 1. read the chat; resolve the cheap-LLM selection (priority order via
//!    [`crate::cheap_llm::get_cheap_llm_provider`]); if the chat is
//!    *active-dangerous* (`isDangerousChat && conciergeOverride != 'OFF'`), swap
//!    to the uncensored selection.
//! 2. read all messages, partition into turns ([`crate::context_summary::partition_messages_into_turns`]),
//!    pick the fold range (T-hard: turns 1..currentTurn-TAIL; regular: the next
//!    FOLD_TURN_BATCH after the prior fold), fold via the cheap LLM.
//! 3. bump `compactionGeneration`, advance `lastSummaryTurn`, recompute
//!    `summaryAnchorMessageIds`, write the new `contextSummary` (+ `lastFullRebuildTurn`
//!    on a hard rebuild); append a `context-summary` chat event.
//! 4. **sweep** prior-generation Librarian summary whispers (this unit's one
//!    in-scope side effect — [`sweep_prior_summary_whispers`]).
//! 5. (deferred seams) post the fresh Librarian whisper, mirror into vaults,
//!    refresh relevant conversations, emit the cost event.
//! 6. generate + write a title (literary, or practical for help-like chats).
//!
//! ## `generateContextSummaryAsync` — the v5 shape
//!
//! v4 exposes a fire-and-forget wrapper (`generateContextSummaryAsync`) whose
//! whole reason for existing is a Node forked-job-child hazard: a promise that
//! settles after the write-buffer flush drops its writes silently. **The v5
//! runtime has no such hazard** — every mutation goes through the writer channel
//! ([`crate::db::runtime::Db::write`]), which guarantees delivery regardless of
//! when the caller's future is polled. So there is no separate `_async` port:
//! the caller either `.await`s [`generate_context_summary`] or `tokio::spawn`s
//! it; both reach the same committed state. [`check_and_generate_summary_if_needed`]
//! therefore always awaits the fold (v4's `awaitFold` branch), and the flag is
//! dropped — see its docs.
//!
//! ## The cross-subsystem seams ([`ContextSummarySeams`])
//!
//! Per the decomposition doc these belong to the post-office / vault / cost
//! subsystems; they are best-effort in v4 (a failure never fails a fold) and are
//! modeled here as the [`ContextSummarySeams`] trait. [`NoopSeams`] is the
//! default no-op (matching the oracle mocks the spine callers keep);
//! [`RealContextSummarySeams`] (W4.6b + Round-3 Group 7) closes ALL of them:
//!
//!   - `postLibrarianSummaryAnnouncement` — the fresh whisper post — **CLOSED**
//!     (the *sweep* of prior whispers is ported inline; the re-post rides the
//!     ported [`crate::services::librarian_notifications`] writer).
//!   - `estimateMessageCost` + `createContextSummaryEvent` /
//!     `createTitleGenerationEvent` — the cost/system-event emit — **CLOSED**
//!     (through the ported [`crate::services::cost_events`] writers; the estimated
//!     cost stays host-resolved → `None`).
//!   - `writeConversationSummaryToVaults` + `computeConversationStats` — the
//!     Commonplace-Book vault mirror — **CLOSED** (Round-3 Group 7): the fold
//!     builds the input from `compute_conversation_stats` + the participant
//!     character ids and calls the ported bridge writer.
//!   - `refreshRelevantConversationsOnFold` — the relevant-past-conversations
//!     re-search + whisper — **CLOSED** (Round-3 Group 7): fired AFTER the mirror
//!     (so the search reads the fresh corpus), through the ported writer, over the
//!     seam's injected embedding provider.
//!
//! Also inherited from [`crate::services::cheap_llm_exec`]: the per-call
//! `logLLMCall` llm-logs write and host-side API-key acquisition.

use serde_json::{json, Value};

use crate::chat_predicates::is_help_like_chat_type;
use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, DangerousContentSettings,
};
use crate::clock::now_iso;
use crate::context_summary::{
    calculate_interchange_count, evaluate_summarization_gate, partition_messages_into_turns,
    should_check_title_at_interchange, FoldedTurn, InterchangeMessage, PartitionInputMessage,
    SummarizationGateDecision, FOLD_TAIL_FLOOR, FOLD_TURN_BATCH,
};
use crate::db::chats::ChatUpdate;
use crate::db::chats_messages::{ChatEventInput, ContextSummaryInput};
use crate::db::chats_messages_read;
use crate::db::chats_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::commonplace_notifications::{
    refresh_relevant_conversations_on_fold, RefreshRelevantConversationsInput,
};
use crate::services::conversation_summary_vault_bridge::{
    compute_conversation_stats, write_conversation_summary_to_vaults, WriteConversationSummaryInput,
};
use crate::services::fold_episode_pass::{
    run_fold_episode_pass, FoldWindowMessage, RunFoldEpisodePassInput,
};
use crate::services::queue_service;

use std::collections::HashMap;

pub mod prompt_text;
pub mod tasks;

use tasks::{
    fold_chat_summary, generate_help_chat_title_from_summary, generate_title_from_summary,
    ChatMessage,
};

/// v4 `SUMMARY_CONTENT_PREFIX` (`lib/services/librarian-notifications/writer.ts`):
/// the legacy-whisper content prefix the sweep matches on when a whisper carries
/// no `summaryAnchor`.
pub const SUMMARY_CONTENT_PREFIX: &str =
    "The Librarian deposits a précis of the conversation to date upon the table — file it for reference:";

/// v4 `CheapLLMSettings` subset the fold consumes (the three keys
/// `toCheapLLMConfig` maps — the settings' `defaultCheapProfileId` deliberately
/// does not reach this path).
#[derive(Clone, Debug, Default)]
pub struct CheapLlmSettings {
    pub strategy: String,
    pub user_defined_profile_id: Option<String>,
    pub default_cheap_profile_id: Option<String>,
    pub fallback_to_local: bool,
}

/// v4 `GenerateSummaryOptions`.
pub struct GenerateSummaryOptions {
    pub user_id: String,
    pub chat_id: String,
    /// The current connection profile (the cheap-LLM selection's basis).
    pub connection_profile: CheapLlmProfile,
    pub cheap_llm_settings: CheapLlmSettings,
    pub available_profiles: Vec<CheapLlmProfile>,
    /// Force an unconditional T-hard rebuild (v4 `forceRegenerate`).
    pub force_regenerate: bool,
    /// The resolved dangerous-content settings for the user. v4 reads these from
    /// `chatSettings.findByUserId(userId)` + `resolveDangerousContentSettings`
    /// only when the chat is active-dangerous; the resolution is a host seam
    /// (as in the memory processor), so the caller injects the already-resolved
    /// value (or `None` — then the uncensored swap is skipped).
    pub danger_settings: Option<DangerousContentSettings>,
    /// Registry cheapest-model answer for the connection profile's provider (the
    /// injected `getCheapModelConfig` seam; `None` = no plugin → v4 legacy map).
    pub registry_cheapest_for_current: Option<String>,
    /// The current connection profile's `maxContext` (v4
    /// `connectionProfile.maxContext ?? null`) — scales the relevant-conversations
    /// refresh list size on the fold's cross-subsystem seam (Round-3 Group 7). The
    /// `CheapLlmProfile` subset carries no `maxContext`, so the caller supplies it
    /// separately; `None` uses the refresh's minimum list size.
    pub connection_max_context: Option<i64>,
}

/// v4 `SummaryGenerationResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryGenerationResult {
    pub success: bool,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub was_generated: bool,
    pub usage: Option<Usage>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

impl SummaryGenerationResult {
    fn fail(error: &str) -> Self {
        Self {
            success: false,
            summary: None,
            error: Some(error.to_string()),
            was_generated: false,
            usage: None,
        }
    }
}

/// The cross-subsystem side effects the fold fires (post-office / vault / cost).
/// Every method is best-effort in v4 (a failure never fails the fold), so the
/// [`NoopSeams`] no-op implementation matches the oracle's jest mocks
/// byte-for-byte in diffed state. [`RealContextSummarySeams`] (W4.6b) closes the
/// Librarian re-post + the CONTEXT_SUMMARY / TITLE_GENERATION cost events; the
/// vault-mirror + relevant-conversations refresh remain no-ops there (documented
/// handoffs — they need vault fixtures + embedding).
///
/// Async (the real writers await the single-writer channel); modeled on the
/// `EmbeddingProvider` / `DangerAnnouncer` RPITIT precedent so the futures are
/// `Send` without boxing. `provider`/`model` on the cost events are the resolved
/// cheap-LLM selection's provider/model (v4 `cheapLLM.provider`/`.modelName`);
/// the estimated cost is host-resolved (v4 `estimateMessageCost`), so it is not
/// threaded here (`None` — the differential drives the same null).
pub trait ContextSummarySeams {
    /// `postLibrarianSummaryAnnouncement` — re-post the fresh whisper. The
    /// *sweep* of prior whispers is NOT here (it is ported inline).
    fn post_librarian_summary(
        &self,
        chat_id: &str,
        summary: &str,
        new_generation: f64,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// `writeConversationSummaryToVaults` (the caller supplies the input built from
    /// `computeConversationStats` + the participant character ids).
    fn mirror_summary_to_vaults(
        &self,
        input: WriteConversationSummaryInput,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// `refreshRelevantConversationsOnFold`.
    fn refresh_relevant_conversations(
        &self,
        input: RefreshRelevantConversationsInput,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// `estimateMessageCost` + `createContextSummaryEvent`.
    fn emit_summary_cost_event(
        &self,
        chat_id: &str,
        usage: Usage,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// `estimateMessageCost` + `createTitleGenerationEvent`.
    fn emit_title_cost_event(
        &self,
        chat_id: &str,
        usage: Usage,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// `runFoldEpisodePass` — the fold-time episode consolidation (episodic
    /// spine). A seam rather than an inline call because the pass writes through
    /// the memory gate and therefore needs an [`EmbeddingProvider`], which the
    /// fold itself does not carry.
    fn run_fold_episode_pass(
        &self,
        input: RunFoldEpisodePassInput,
        selection: CheapLlmSelection,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// The default no-op seams (matches the oracle's mocks).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopSeams;
impl ContextSummarySeams for NoopSeams {
    async fn post_librarian_summary(&self, _chat_id: &str, _summary: &str, _new_generation: f64) {}
    async fn mirror_summary_to_vaults(&self, _input: WriteConversationSummaryInput) {}
    async fn refresh_relevant_conversations(&self, _input: RefreshRelevantConversationsInput) {}
    async fn emit_summary_cost_event(
        &self,
        _chat_id: &str,
        _usage: Usage,
        _provider: Option<&str>,
        _model: Option<&str>,
    ) {
    }
    async fn emit_title_cost_event(
        &self,
        _chat_id: &str,
        _usage: Usage,
        _provider: Option<&str>,
        _model: Option<&str>,
    ) {
    }
    async fn run_fold_episode_pass(
        &self,
        _input: RunFoldEpisodePassInput,
        _selection: CheapLlmSelection,
    ) {
    }
}

/// The real context-summary side effects (v4 `generateContextSummary`'s
/// post-office + vault + cost blocks; W4.6b + Round-3 Group 7): the Librarian
/// summary re-post (through the ported [`crate::services::librarian_notifications`]
/// writer), the CONTEXT_SUMMARY / TITLE_GENERATION cost events (through the ported
/// [`crate::services::cost_events`] writers), the Commonplace-Book vault mirror
/// (`writeConversationSummaryToVaults`), and the relevant-conversations refresh
/// (`refreshRelevantConversationsOnFold`). The ordering `mirror_summary_to_vaults`
/// → `refresh_relevant_conversations` is deliberate: the refresh's semantic search
/// must read the fresh corpus, not race it. The refresh needs an embedding
/// provider, so the struct is generic over `E`. Each write is best-effort (the
/// writers swallow their own failure).
pub struct RealContextSummarySeams<'a, C: CompletionProvider, E: EmbeddingProvider> {
    pub db: &'a Db,
    pub embedding: &'a E,
    /// The completion provider + executor the fold-episode pass drives (the
    /// same cheap-LLM path the fold itself used).
    pub completion: &'a C,
    pub executor: &'a CheapLlmTaskExecutor,
}

impl<C: CompletionProvider + Sync, E: EmbeddingProvider + Sync> ContextSummarySeams
    for RealContextSummarySeams<'_, C, E>
{
    async fn post_librarian_summary(&self, chat_id: &str, summary: &str, new_generation: f64) {
        use crate::services::librarian_notifications as ln;
        // v4 posts with `targetParticipantIds: null` and
        // `summaryAnchor: { compactionGeneration: newGeneration }`.
        ln::post_librarian_summary_announcement(
            self.db,
            &ln::LibrarianSummaryAnnouncement {
                chat_id: chat_id.to_string(),
                summary: summary.to_string(),
                target_participant_ids: None,
                summary_anchor: Some(new_generation as i64),
            },
        )
        .await;
    }

    async fn mirror_summary_to_vaults(&self, input: WriteConversationSummaryInput) {
        // v4 `writeConversationSummaryToVaults`: mirror the fresh summary into every
        // participant character's vault. Best-effort (per-character try/catch inside).
        write_conversation_summary_to_vaults(self.db, &input).await;
    }

    async fn refresh_relevant_conversations(&self, input: RefreshRelevantConversationsInput) {
        // v4 `refreshRelevantConversationsOnFold`: re-run the relevant-past-
        // conversations search against the fresh fold summary and post a refreshed
        // Commonplace whisper to each present character. Best-effort (never throws).
        refresh_relevant_conversations_on_fold(self.db, self.embedding, input).await;
    }

    async fn emit_summary_cost_event(
        &self,
        chat_id: &str,
        usage: Usage,
        provider: Option<&str>,
        model: Option<&str>,
    ) {
        use crate::services::cost_events as ce;
        ce::create_context_summary_event(
            self.db,
            chat_id,
            Some(ce::TokenUsage {
                prompt_tokens: Some(usage.prompt_tokens as f64),
                completion_tokens: Some(usage.completion_tokens as f64),
                total_tokens: Some(usage.total_tokens as f64),
            }),
            provider.map(str::to_string),
            model.map(str::to_string),
            // Host-resolved (`estimateMessageCost`); not threaded → NULL.
            None,
        )
        .await;
    }

    async fn emit_title_cost_event(
        &self,
        chat_id: &str,
        usage: Usage,
        provider: Option<&str>,
        model: Option<&str>,
    ) {
        use crate::services::cost_events as ce;
        ce::create_title_generation_event(
            self.db,
            chat_id,
            Some(ce::TokenUsage {
                prompt_tokens: Some(usage.prompt_tokens as f64),
                completion_tokens: Some(usage.completion_tokens as f64),
                total_tokens: Some(usage.total_tokens as f64),
            }),
            provider.map(str::to_string),
            model.map(str::to_string),
            None,
        )
        .await;
    }

    async fn run_fold_episode_pass(
        &self,
        input: RunFoldEpisodePassInput,
        selection: CheapLlmSelection,
    ) {
        // v4 `runFoldEpisodePass`: over the just-folded window, ask the cheap
        // LLM for 0–2 consolidated episode records and write them as dated
        // `kind: 'episodic'` memories linked to the window's per-turn
        // fragments. Best-effort — the pass swallows its own failures.
        let _ = run_fold_episode_pass(
            self.db,
            self.completion,
            self.embedding,
            self.executor,
            &selection,
            &input,
        )
        .await;
    }
}

fn to_cheap_llm_config(settings: &CheapLlmSettings) -> CheapLlmConfig {
    CheapLlmConfig {
        strategy: settings.strategy.clone(),
        // v4 `userDefinedProfileId ?? undefined` — but note `??` keeps `''`
        // whereas `getCheapLLMProvider`'s own guard drops empty strings, so the
        // net effect matches the empty-string filter there.
        user_defined_profile_id: settings.user_defined_profile_id.clone(),
        default_cheap_profile_id: settings.default_cheap_profile_id.clone(),
        fallback_to_local: settings.fallback_to_local,
    }
}

/// v4 `isChatActiveDangerous`: `isDangerousChat === true && conciergeOverride !== 'OFF'`.
fn is_chat_active_dangerous(chat: &Value) -> bool {
    let flagged = chat
        .get("isDangerousChat")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let off_duty = chat.get("conciergeOverride").and_then(Value::as_str) == Some("OFF");
    flagged && !off_duty
}

/// v4 `turnsToChatMessages` over the fold range: flatten each turn's messages to
/// `{ role: role.toLowerCase(), content, createdAt }` — the message dates feed
/// the fold summary's Timeline section. The ported [`FoldedTurn`] carries only
/// message ids, so the row is reconstructed from the full message list via
/// `rows_by_id` — the same rows the turn was partitioned from.
fn turns_to_chat_messages(
    turns: &[FoldedTurn],
    rows_by_id: &HashMap<&str, &Value>,
) -> Vec<ChatMessage> {
    let mut result = Vec::new();
    for t in turns {
        for id in &t.ids {
            if let Some(m) = rows_by_id.get(id.as_str()) {
                result.push(ChatMessage {
                    role: m
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase(),
                    content: m
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    // v4 `m.createdAt ?? null`.
                    created_at: m
                        .get("createdAt")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        }
    }
    result
}

/// v4 `PartitionInputMessage` view of a `getMessages` row.
fn to_partition_input(m: &Value) -> PartitionInputMessage {
    PartitionInputMessage {
        id: m
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        role: m.get("role").and_then(Value::as_str).map(str::to_string),
        message_type: m.get("type").and_then(Value::as_str).map(str::to_string),
        system_sender: m
            .get("systemSender")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4 `InterchangeMessage` view of a `getMessages` row.
fn to_interchange_input(m: &Value) -> InterchangeMessage {
    InterchangeMessage {
        role: m.get("role").and_then(Value::as_str).map(str::to_string),
        message_type: m.get("type").and_then(Value::as_str).map(str::to_string),
        system_sender: m
            .get("systemSender")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4 `generateContextSummary` (with the default no-op seams). Convenience over
/// [`generate_context_summary_with_seams`].
pub async fn generate_context_summary<C: CompletionProvider>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    options: &GenerateSummaryOptions,
) -> SummaryGenerationResult {
    generate_context_summary_with_seams(db, completion, executor, options, &NoopSeams).await
}

/// v4 `generateContextSummary` — the rolling-window fold write + title, with the
/// cross-subsystem side effects supplied by `seams`.
pub async fn generate_context_summary_with_seams<C: CompletionProvider, S: ContextSummarySeams>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    options: &GenerateSummaryOptions,
    seams: &S,
) -> SummaryGenerationResult {
    match generate_inner(db, completion, executor, options, seams).await {
        Ok(result) => result,
        // v4's outer catch: `{ success: false, error: message, wasGenerated: false }`.
        Err(e) => SummaryGenerationResult::fail(&e.to_string()),
    }
}

async fn generate_inner<C: CompletionProvider, S: ContextSummarySeams>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    options: &GenerateSummaryOptions,
    seams: &S,
) -> Result<SummaryGenerationResult, DbError> {
    let chat_id = options.chat_id.clone();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &chat_id))?;
    let Some(chat) = chat else {
        return Ok(SummaryGenerationResult::fail("Chat not found"));
    };
    let chat_type = chat
        .get("chatType")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Resolve the cheap-LLM selection (priority order), then the uncensored swap
    // for active-dangerous chats.
    let config = to_cheap_llm_config(&options.cheap_llm_settings);
    let selection: CheapLlmSelection = get_cheap_llm_provider(
        &options.connection_profile,
        &config,
        &options.available_profiles,
        false,
        options.registry_cheapest_for_current.as_deref(),
    );
    // v4 returns early when the selection is falsy; `get_cheap_llm_provider`
    // always yields a selection (its priority-5 fallback), matching v4's own
    // behavior on the differential (a valid current profile is always present).
    let selection = if is_chat_active_dangerous(&chat) {
        resolve_uncensored_cheap_llm_selection(
            selection,
            true,
            options.danger_settings.as_ref(),
            &options.available_profiles,
        )
    } else {
        selection
    };

    // Read all messages, partition into turns. Keep an id → (role, content) map
    // so the fold range can be reconstructed into `{role, content}` (the ported
    // `FoldedTurn` carries only ids).
    let all_messages = db.read_main(|conn| chats_messages_read::get_messages(conn, &chat_id))?;
    // The fold render + the fold-episode pass both need the whole message row
    // (v4 hands the pass `turnsToFold.flatMap(t => t.messages)`, and the fold
    // render now carries each turn's `createdAt`). The ported `FoldedTurn`
    // carries only ids, so keep a by-id view of the rows themselves.
    let rows_by_id: HashMap<&str, &Value> = all_messages
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(|id| (id, m)))
        .collect();
    let partition_inputs: Vec<PartitionInputMessage> =
        all_messages.iter().map(to_partition_input).collect();
    let all_turns = partition_messages_into_turns(&partition_inputs, chat_type.as_deref());
    let current_turn = all_turns.len() as i64;
    if current_turn == 0 {
        return Ok(SummaryGenerationResult::fail("No messages to summarize"));
    }

    let last_folded_turn = chat
        .get("lastSummaryTurn")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as i64;

    // Fold range. T-hard absorbs everything up to the tail floor; the regular
    // path takes the next FOLD_TURN_BATCH turns.
    let is_hard_rebuild = options.force_regenerate;
    let (fold_from_turn, fold_through_turn) = if is_hard_rebuild {
        (1, (current_turn - FOLD_TAIL_FLOOR).max(1))
    } else {
        (
            last_folded_turn + 1,
            (last_folded_turn + FOLD_TURN_BATCH).min(current_turn - FOLD_TAIL_FLOOR),
        )
    };

    if fold_through_turn < fold_from_turn {
        return Ok(SummaryGenerationResult::fail("Not enough turns to fold"));
    }

    // `slice(foldFromTurn - 1, foldThroughTurn)`.
    let lo = (fold_from_turn - 1).max(0) as usize;
    let hi = fold_through_turn.max(0) as usize;
    let turns_to_fold = &all_turns[lo.min(all_turns.len())..hi.min(all_turns.len())];
    let new_turns_content = turns_to_chat_messages(turns_to_fold, &rows_by_id);

    if new_turns_content.is_empty() {
        return Ok(SummaryGenerationResult::fail("No content in turns to fold"));
    }

    // v4 `isHardRebuild ? null : chat.contextSummary ?? null`.
    let prior_summary: Option<String> = if is_hard_rebuild {
        None
    } else {
        chat.get("contextSummary")
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let fold_result = fold_chat_summary(
        executor,
        completion,
        prior_summary.as_deref(),
        &new_turns_content,
        &selection,
    )
    .await;

    let (Some(new_summary), true) = (fold_result.result.clone(), fold_result.success) else {
        return Ok(SummaryGenerationResult::fail(
            fold_result
                .error
                .as_deref()
                .unwrap_or("Failed to fold summary"),
        ));
    };

    let usage = fold_result.usage.map(|u| Usage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });

    let new_generation = chat
        .get("compactionGeneration")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        + 1.0;
    let new_last_folded_turn = fold_through_turn;

    // Anchor every conversation message in turns 1..newLastFoldedTurn.
    let anchor_hi = new_last_folded_turn.max(0) as usize;
    let summary_anchor_message_ids: Vec<String> = all_turns[..anchor_hi.min(all_turns.len())]
        .iter()
        .flat_map(|t| t.ids.iter().cloned())
        .collect();

    // Write the fold result to the chat row.
    {
        let summary = new_summary.clone();
        let anchors = summary_anchor_message_ids.clone();
        let chat_id = chat_id.clone();
        let now = now_iso();
        db.write(move |writers| {
            let patch = ChatUpdate {
                context_summary: Some(Some(summary)),
                compaction_generation: Some(new_generation),
                last_summary_turn: Some(new_last_folded_turn as f64),
                summary_anchor_message_ids: Some(anchors),
                last_full_rebuild_turn: if is_hard_rebuild {
                    Some(current_turn as f64)
                } else {
                    None
                },
                updated_at: Some(now),
                ..Default::default()
            };
            writers.main().chats().update(&chat_id, &patch)?;
            Ok(())
        })
        .await?;
    }

    // Append the context-summary chat event.
    {
        let input = ChatEventInput::ContextSummary(ContextSummaryInput {
            id: uuid::Uuid::new_v4().to_string(),
            context: new_summary.clone(),
            created_at: now_iso(),
        });
        let chat_id = chat_id.clone();
        db.write(move |writers| {
            writers
                .main()
                .chat_messages()
                .add_message(&chat_id, &input)?;
            Ok(())
        })
        .await?;
    }

    // Sweep prior-generation Librarian summary whispers (this unit's in-scope
    // side effect), then fire the deferred post-office seam. v4 wraps this in a
    // try/catch that only logs — a sweep/post failure never fails the fold.
    if let Err(_e) = sweep_prior_summary_whispers(db, &chat_id, new_generation).await {
        // best-effort — swallowed (v4 logs and continues).
    }
    seams
        .post_librarian_summary(&chat_id, &new_summary, new_generation)
        .await;

    // Cross-subsystem vault seams (best-effort; Round-3 Group 7 wired them live).
    // v4 builds these two inputs here and calls them IN THIS ORDER — the mirror
    // writes the fresh summary into every participant vault FIRST, so the refresh's
    // semantic search reads the fresh corpus rather than racing it.
    {
        // participantCharacterIds = Array.from(new Set(chat.participants.map(p => p.characterId))).
        let mut participant_character_ids: Vec<String> = Vec::new();
        if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
            for p in parts {
                if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                    let cid = cid.to_string();
                    if !participant_character_ids.contains(&cid) {
                        participant_character_ids.push(cid);
                    }
                }
            }
        }
        let stats = compute_conversation_stats(&all_messages);
        let chat_title = chat
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        seams
            .mirror_summary_to_vaults(WriteConversationSummaryInput {
                chat_id: chat_id.clone(),
                chat_title,
                summary: new_summary.clone(),
                summary_generation: new_generation as i64,
                participant_character_ids,
                message_count: stats.message_count,
                first_message_at: stats.first_message_at,
                last_message_at: stats.last_message_at,
                updated_at: now_iso(),
            })
            .await;
    }
    seams
        .refresh_relevant_conversations(RefreshRelevantConversationsInput {
            chat_id: chat_id.clone(),
            summary: new_summary.clone(),
            user_id: options.user_id.clone(),
            // v4 passes `embeddingProfileId: undefined`.
            embedding_profile_id: None,
            max_context: options.connection_max_context,
        })
        .await;

    // Episodic spine: fold-time episode pass. Over the just-folded window, ask
    // the cheap LLM for 0–2 consolidated episode records and write them as
    // dated `kind: 'episodic'` memories linked to the window's per-turn
    // fragments. Piggybacks the fold cadence — no new trigger. Best-effort:
    // a failed episode pass never fails the fold.
    {
        let window_messages: Vec<FoldWindowMessage> = turns_to_fold
            .iter()
            .flat_map(|t| t.ids.iter())
            .filter_map(|id| rows_by_id.get(id.as_str()).copied())
            .map(|m| FoldWindowMessage {
                id: m
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                role: m
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                content: m.get("content").and_then(Value::as_str).map(str::to_string),
                participant_id: m
                    .get("participantId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                created_at: m
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
            .collect();
        seams
            .run_fold_episode_pass(
                RunFoldEpisodePassInput {
                    chat_id: chat_id.clone(),
                    user_id: options.user_id.clone(),
                    window_messages,
                    // v4 `chat.timelineMode ?? 'realtime'`.
                    timeline_mode: chat
                        .get("timelineMode")
                        .and_then(Value::as_str)
                        .unwrap_or("realtime")
                        .to_string(),
                    project_id: chat
                        .get("projectId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    in_autonomous_room: chat_type.as_deref() == Some("autonomous"),
                },
                selection.clone(),
            )
            .await;
    }

    if let Some(u) = usage {
        if u.prompt_tokens > 0 || u.completion_tokens > 0 {
            // v4 `createContextSummaryEvent(chatId, usage, cheapLLM.provider,
            // cheapLLM.modelName, cost)` — provider/model = the resolved selection.
            seams
                .emit_summary_cost_event(
                    &chat_id,
                    u,
                    Some(selection.provider.as_str()),
                    Some(selection.model_name.as_str()),
                )
                .await;
        }
    }

    // Title generation (literary, or practical for help-like chats).
    let title_result = if is_help_like_chat_type(chat_type.as_deref()) {
        generate_help_chat_title_from_summary(executor, completion, &new_summary, &selection).await
    } else {
        generate_title_from_summary(executor, completion, &new_summary, &selection).await
    };

    if title_result.success {
        if let Some(title) = title_result.result.clone().filter(|t| !t.is_empty()) {
            let write_chat_id = chat_id.clone();
            let now = now_iso();
            db.write(move |writers| {
                let patch = ChatUpdate {
                    title: Some(title),
                    updated_at: Some(now),
                    ..Default::default()
                };
                writers.main().chats().update(&write_chat_id, &patch)?;
                Ok(())
            })
            .await?;

            if let Some(u) = title_result.usage {
                let tu = Usage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                };
                if tu.prompt_tokens > 0 || tu.completion_tokens > 0 {
                    seams
                        .emit_title_cost_event(
                            &chat_id,
                            tu,
                            Some(selection.provider.as_str()),
                            Some(selection.model_name.as_str()),
                        )
                        .await;
                }
            }
        }
    }
    // v4: a title failure only logs; the fold result still returns success.

    Ok(SummaryGenerationResult {
        success: true,
        summary: Some(new_summary),
        error: None,
        was_generated: true,
        usage,
    })
}

/// v4 lines ~435-453: sweep prior Librarian summary whispers from older
/// generations. A whisper qualifies when `systemSender == 'librarian'` AND
/// either (a) it carries a `summaryAnchor` whose `compactionGeneration` is
/// `< new_generation`, or (b) it carries no anchor but its content starts with
/// [`SUMMARY_CONTENT_PREFIX`] (legacy unanchored whispers). Whispers from the new
/// generation are left untouched. Re-reads messages first (the freshly-appended
/// context-summary event is not a message and never matches).
async fn sweep_prior_summary_whispers(
    db: &Db,
    chat_id: &str,
    new_generation: f64,
) -> Result<i64, DbError> {
    let chat_id_owned = chat_id.to_string();
    let refreshed = db.read_main(|conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;

    let prior_ids: Vec<String> = refreshed
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .filter(|m| {
            if m.get("systemSender").and_then(Value::as_str) != Some("librarian") {
                return false;
            }
            match m.get("summaryAnchor") {
                Some(anchor) if !anchor.is_null() => {
                    // v4: `m.summaryAnchor.compactionGeneration < newGeneration`.
                    anchor
                        .get("compactionGeneration")
                        .and_then(Value::as_f64)
                        .map(|g| g < new_generation)
                        .unwrap_or(false)
                }
                _ => {
                    // Legacy unanchored: content starts with the summary prefix.
                    m.get("content")
                        .and_then(Value::as_str)
                        .map(|c| c.starts_with(SUMMARY_CONTENT_PREFIX))
                        .unwrap_or(false)
                }
            }
        })
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    if prior_ids.is_empty() {
        return Ok(0);
    }

    let chat_id_owned = chat_id.to_string();
    let removed = db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .delete_messages_by_ids(&chat_id_owned, &prior_ids)
        })
        .await?;
    Ok(removed)
}

/// v4 `invalidateContextSummaryIfMessageCovered`: when a conversation message is
/// edited/deleted, clear the running summary if any changed id was in the anchor
/// set — resetting `lastSummaryTurn` / `lastFullRebuildTurn` and bumping
/// `compactionGeneration`. Returns `true` iff the summary was invalidated.
pub async fn invalidate_context_summary_if_message_covered(
    db: &Db,
    chat_id: &str,
    message_ids: &[String],
) -> bool {
    if message_ids.is_empty() {
        return false;
    }
    // v4's catch logs and returns false.
    invalidate_inner(db, chat_id, message_ids)
        .await
        .unwrap_or(false)
}

async fn invalidate_inner(db: &Db, chat_id: &str, message_ids: &[String]) -> Result<bool, DbError> {
    let chat_id_owned = chat_id.to_string();
    let Some(chat) = db.read_main(|conn| chats_read::find_by_id(conn, &chat_id_owned))? else {
        return Ok(false);
    };
    // `!chat.contextSummary` — absent OR empty string is falsy.
    let has_summary = chat
        .get("contextSummary")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_summary {
        return Ok(false);
    }

    // `chat.summaryAnchorMessageIds ?? []` — the read always materializes this
    // `.default([])` column.
    let covered: Vec<&str> = chat
        .get("summaryAnchorMessageIds")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if covered.is_empty() {
        return Ok(false);
    }

    let intersects = message_ids.iter().any(|id| covered.iter().any(|c| c == id));
    if !intersects {
        return Ok(false);
    }

    let new_generation = chat
        .get("compactionGeneration")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        + 1.0;
    let chat_id_owned = chat_id.to_string();
    let now = now_iso();
    db.write(move |writers| {
        let patch = ChatUpdate {
            context_summary: Some(None),
            summary_anchor_message_ids: Some(Vec::new()),
            compaction_generation: Some(new_generation),
            last_summary_turn: Some(0.0),
            last_full_rebuild_turn: Some(0.0),
            updated_at: Some(now),
            ..Default::default()
        };
        writers.main().chats().update(&chat_id_owned, &patch)?;
        Ok(())
    })
    .await?;
    Ok(true)
}

/// Result of a title-checkpoint check inside
/// [`check_and_generate_summary_if_needed`], surfaced for the differential's
/// per-op trace (v4 has no return value — it side-effects the enqueue + the
/// fold).
#[derive(Clone, Debug, PartialEq)]
pub struct CheckOutcome {
    pub current_interchange: i64,
    /// The title-update job id enqueued at this checkpoint, if the checkpoint
    /// fired. Absent → no checkpoint.
    pub enqueued_title_job_id: Option<String>,
    /// The gate decision (`skip` / `fold` / `hard`).
    pub decision: SummarizationGateDecision,
    /// The fold result when the gate fired (`decision != skip`).
    pub summary_result: Option<SummaryGenerationResult>,
}

/// v4 `checkAndGenerateSummaryIfNeeded`: recompute the interchange count, enqueue
/// a TITLE_UPDATE job when a title checkpoint is crossed, then run the
/// summarization gate and — when it fires — fold.
///
/// `connection_profile_id` is v4's `connectionProfile.id` (the title-update job
/// payload field). `await_fold` is v4's `options.awaitFold`; the v5 runtime
/// always awaits regardless (see module docs), so the flag is accepted for
/// signature parity but does not change behavior — the fold is always awaited.
#[allow(clippy::too_many_arguments)]
pub async fn check_and_generate_summary_if_needed<C: CompletionProvider>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    connection_profile: &CheapLlmProfile,
    cheap_llm_settings: &CheapLlmSettings,
    available_profiles: &[CheapLlmProfile],
    user_id: &str,
    danger_settings: Option<&DangerousContentSettings>,
    registry_cheapest_for_current: Option<&str>,
    _await_fold: bool,
) -> Result<Option<CheckOutcome>, DbError> {
    let chat_id_owned = chat_id.to_string();
    let Some(chat) = db.read_main(|conn| chats_read::find_by_id(conn, &chat_id_owned))? else {
        return Ok(None);
    };
    let chat_type = chat
        .get("chatType")
        .and_then(Value::as_str)
        .map(str::to_string);

    let all_messages =
        db.read_main(|conn| chats_messages_read::get_messages(conn, &chat_id_owned))?;
    let interchange_inputs: Vec<InterchangeMessage> =
        all_messages.iter().map(to_interchange_input).collect();
    let current_interchange =
        calculate_interchange_count(&interchange_inputs, chat_type.as_deref());

    let last_checked = chat
        .get("lastRenameCheckInterchange")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as i64;

    let mut enqueued_title_job_id = None;
    if should_check_title_at_interchange(current_interchange, last_checked, chat_type.as_deref()) {
        // Enqueue a TITLE_UPDATE job rather than running the cheap-LLM call
        // inline (v4's dedupe folds repeat firings at the same checkpoint).
        let payload = json!({
            "chatId": chat_id,
            "connectionProfileId": connection_profile.id,
            "currentInterchange": current_interchange,
        });
        // v4 wraps the enqueue in a try/catch that only logs on failure.
        if let Ok(id) = queue_service::enqueue_title_update(db, user_id, chat_id, payload).await {
            enqueued_title_job_id = Some(id);
        }
    }

    let decision = evaluate_summarization_gate(
        current_interchange,
        chat.get("lastSummaryTurn")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as i64,
        chat.get("lastFullRebuildTurn")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as i64,
    );

    let mut summary_result = None;
    if decision != SummarizationGateDecision::Skip {
        let options = GenerateSummaryOptions {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
            connection_profile: connection_profile.clone(),
            cheap_llm_settings: cheap_llm_settings.clone(),
            available_profiles: available_profiles.to_vec(),
            force_regenerate: decision == SummarizationGateDecision::Hard,
            danger_settings: danger_settings.cloned(),
            registry_cheapest_for_current: registry_cheapest_for_current.map(str::to_string),
            // The gate path folds with `NoopSeams` (the mirror/refresh never run
            // here), so the refresh list size is irrelevant — `None`.
            connection_max_context: None,
        };
        // The v5 runtime always awaits the fold (see module docs).
        summary_result = Some(generate_context_summary(db, completion, executor, &options).await);
    }

    Ok(Some(CheckOutcome {
        current_interchange,
        enqueued_title_job_id,
        decision,
        summary_result,
    }))
}
