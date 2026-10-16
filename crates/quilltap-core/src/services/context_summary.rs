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
//!    on the *uncensored row* of the Concierge 2×2 (Flagged or the operator's
//!    own Uncensored — [`crate::services::dangerous_content::chat_override::should_use_uncensored_route`]),
//!    swap to the uncensored selection.
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
    CheapLlmProfile, CheapLlmSelection,
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
use crate::services::auto_title::{
    apply_auto_title, AutoTitleExtraPatch, AutoTitleOutcome, AutoTitleSource,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::commonplace_notifications::{
    refresh_relevant_conversations_on_fold, RefreshRelevantConversationsInput,
};
use crate::services::conversation_summary_vault_bridge::{
    compute_conversation_stats, write_conversation_summary_to_vaults, WriteConversationSummaryInput,
};
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::dangerous_content::resolver::ResolvedConciergePolicy;
use crate::services::queue_service;

use std::collections::HashMap;

pub mod prompt_text;
pub mod tasks;
// === P4.D110 (v4 `3c041e46`, bug 96): the tolerant title-verdict parser ===
pub mod title_verdict;
// === end P4.D110 ===

use tasks::{
    fold_chat_summary, generate_help_chat_title_from_summary, generate_title_from_summary, FoldTurn,
};

use crate::services::speaker_names::{resolve_speaker_names, speaker_label, SpeakerNames};

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
    /// The chat's resolved Concierge policy. v4 reads it from
    /// `chatSettings.findByUserId(userId)` + `resolveConciergeSettings(…, chat)`
    /// (`3b463d6b1`) only when the chat is active-dangerous; the resolution is a
    /// host seam (as in the memory processor), so the caller injects the
    /// already-resolved value (or `None` — then the uncensored swap is skipped).
    pub concierge_policy: Option<ResolvedConciergePolicy>,
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
    /// True when the fold was lost to a cheap-LLM timeout rather than answered
    /// (v4 `SummaryGenerationResult.timedOut`, `a1d88aa3a`, bug 107).
    /// Propagated from the task so the background handler can fail the job
    /// instead of reporting a clean finish over a summary that never ran.
    pub timed_out: bool,
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
            timed_out: false,
        }
    }

    /// The fold's own failure arm — v4 carries `timedOut: foldResult.timedOut`
    /// there and only there.
    fn fail_from_fold(error: &str, timed_out: bool) -> Self {
        Self {
            timed_out,
            ..Self::fail(error)
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
    /// v4 `context-summary.ts:552-574` — BOTH fold memory passes (the episode
    /// pass, then the fold-grain OTHER pass) on ONE extraction context. A seam
    /// rather than an inline call because the passes admit through admission
    /// and therefore need an [`EmbeddingProvider`], which the fold itself does
    /// not carry. Best-effort: never fails the fold.
    fn run_fold_memory_passes(
        &self,
        input: FoldMemoryPassesInput,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// What the fold hands its memory passes (v4 `context-summary.ts:552-574`):
/// the chat row the fold already holds, the transcript it already read (the
/// cursors are placed against it), the folded turns' RAW rows (the passes'
/// eligibility filter reads `type` / `systemSender` as v4's does), the unit's
/// connection profile, and the fold's fresh summary as the orienting context
/// (`chatContextSummary: newSummary`). The passes resolve their OWN cheap-LLM
/// route from the context (the default cheap profile honoured — L-1), never
/// the summary's selection.
#[derive(Clone, Debug)]
pub struct FoldMemoryPassesInput {
    pub chat_id: String,
    pub user_id: String,
    pub chat: Value,
    pub fold_transcript: Vec<Value>,
    pub window_messages: Vec<Value>,
    pub connection_profile: Value,
    pub chat_context_summary: String,
}

/// The LIVE fold hook every real seam shares: ONE `build_extraction_context`
/// (v4 `buildExtractionContext(chatId, userId, { chat, messages:
/// foldTranscript, connectionProfile, chatContextSummary: newSummary })`),
/// then `run_fold_pass('episode-fold')`, then `run_fold_pass('other-fold')`.
/// The passes never fail; a context build that does logs v4's ONE ERROR
/// `[Context Summary] Fold memory passes failed:` `{chatId}` + the error.
/// Not a backfill — the rate limit applies to the fold. `key_scope` is the
/// running job attempt's (L-16; `None` on the request path).
async fn run_fold_memory_passes_live<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    input: FoldMemoryPassesInput,
    host_zone_fallback: &str,
    key_scope: Option<crate::services::admission::AdmissionKeyScope>,
) where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    use crate::services::extraction::context::{
        build_extraction_context, BuildExtractionContextOptions,
    };
    use crate::services::extraction::fold::{run_fold_pass, RunFoldInput};
    let built = build_extraction_context(
        db,
        &input.chat_id,
        &input.user_id,
        BuildExtractionContextOptions {
            chat: Some(input.chat),
            messages: Some(input.fold_transcript),
            connection_profile: Some(input.connection_profile),
            chat_context_summary: Some(Some(input.chat_context_summary)),
            key_scope,
            host_zone_fallback: host_zone_fallback.to_string(),
            ..Default::default()
        },
    );
    let ctx = match built {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(
                chatId = %input.chat_id,
                error = %e.message(),
                "[Context Summary] Fold memory passes failed:"
            );
            return;
        }
    };
    let deps = crate::services::extraction::run::ExtractionDeps {
        completion,
        embedding,
        executor,
    };
    // Episodic spine: 0–2 dated episode records over the folded window, linked
    // to the window's per-turn fragments.
    run_fold_pass(
        &deps,
        &ctx,
        crate::services::extraction::fold::EPISODE_FOLD_PASS,
        RunFoldInput {
            window_messages: input.window_messages.clone(),
            ..Default::default()
        },
    )
    .await;
    // What each present character took away about the others across the
    // folded window, each thread stated once ('fold' and 'hybrid' modes).
    run_fold_pass(
        &deps,
        &ctx,
        crate::services::extraction::fold::OTHER_FOLD_PASS,
        RunFoldInput {
            window_messages: input.window_messages,
            ..Default::default()
        },
    )
    .await;
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
    async fn run_fold_memory_passes(&self, _input: FoldMemoryPassesInput) {}
}

/// The in-loop summary-check seams (the orchestrator's `run_summary_check`):
/// the fold-time EPISODE pass runs LIVE (v4's in-loop
/// `checkAndGenerateSummaryIfNeeded` → `generateContextSummary` →
/// `runFoldEpisodePass` is un-mocked in the orchestrator-family oracle, so the
/// differential pins its cheap-LLM call), while the cross-subsystem arms
/// (Librarian re-post / vault mirror / relevant-conversations refresh / cost
/// events) stay no-ops — the orchestrator oracle mocks exactly those four, and
/// their real impls are separately verified by the context-summary family
/// through [`RealContextSummarySeams`]. Wiring THOSE live on the in-loop path
/// is a tracked deferral (it requires un-mocking the oracle arms and re-baking
/// the orchestrator corpus).
pub struct FoldEpisodePassSeams<'a, C: CompletionProvider, E: EmbeddingProvider> {
    pub db: &'a Db,
    pub embedding: &'a E,
    pub completion: &'a C,
    pub executor: &'a CheapLlmTaskExecutor,
    /// The memory calendar's host fallback — the host's display-zone NAME
    /// (`host_zone::zone_name`), as every other memory-calendar site threads it
    /// (v4 falls back to `QUILLTAP_TIMEZONE`, then the system zone). Passed by
    /// the four constructors — the orchestrator, the Courier, the enclave step
    /// and the help chat (P4.D296's HANDOFF (3), the `83334b9a5` unification).
    pub host_zone_fallback: &'a str,
}

impl<C: CompletionProvider + Sync, E: EmbeddingProvider + Sync> ContextSummarySeams
    for FoldEpisodePassSeams<'_, C, E>
{
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
    async fn run_fold_memory_passes(&self, input: FoldMemoryPassesInput) {
        // LIVE (the summary-check oracles do not mock the fold passes). The
        // request path is no job: no idempotency scope.
        run_fold_memory_passes_live(
            self.db,
            self.completion,
            self.embedding,
            self.executor,
            input,
            self.host_zone_fallback,
            None,
        )
        .await;
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
    /// The host's calendar NAME — the memory calendar's fallback zone.
    pub host_zone_fallback: String,
    /// The running `CONTEXT_SUMMARY` job's id: the fold passes' admissions
    /// carry one idempotency scope for the attempt (L-16 — v4 mints
    /// `jobId:contentKey:n` inside its job child). `None` outside a job.
    pub job_id: Option<String>,
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

    async fn run_fold_memory_passes(&self, input: FoldMemoryPassesInput) {
        run_fold_memory_passes_live(
            self.db,
            self.completion,
            self.embedding,
            self.executor,
            input,
            &self.host_zone_fallback,
            self.job_id
                .as_deref()
                .map(crate::services::admission::AdmissionKeyScope::new),
        )
        .await;
    }
}

/// The unit's connection profile as the row the extraction context takes (v4
/// passes the fold's `connectionProfile` object itself). Only the fields the
/// cheap-LLM route and the output budget read are carried.
fn profile_value_of(p: &CheapLlmProfile) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), Value::from(p.id.clone()));
    m.insert("provider".into(), Value::from(p.provider.clone()));
    m.insert("modelName".into(), Value::from(p.model_name.clone()));
    m.insert("isCheap".into(), Value::Bool(p.is_cheap));
    m.insert(
        "isDangerousCompatible".into(),
        Value::Bool(p.is_dangerous_compatible),
    );
    // An absent field stays ABSENT (a `null` would read back as a present
    // `parameters`).
    if let Some(v) = &p.base_url {
        m.insert("baseUrl".into(), Value::from(v.clone()));
    }
    if let Some(v) = &p.parameters {
        m.insert("parameters".into(), v.clone());
    }
    if let Some(v) = p.max_tokens {
        m.insert("maxTokens".into(), Value::from(v));
    }
    if let Some(v) = p.max_context {
        m.insert("maxContext".into(), Value::from(v));
    }
    if let Some(v) = &p.model_class {
        m.insert("modelClass".into(), Value::from(v.clone()));
    }
    Value::Object(m)
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

/// v4 `turnsToChatMessages(turns, names)` over the fold range: flatten each
/// turn's messages to `{ speaker, role: role.toLowerCase(), content, createdAt }`
/// — key order `speaker, role, content, createdAt`. Every line carries a speaker
/// label resolved from the chat's seats — a bare `USER:` / `ASSISTANT:`
/// transcript is what let a model invent a name for the character nobody
/// happened to address (bug 161, `e7821606f`). The message dates feed the fold
/// summary's Timeline section. The ported [`FoldedTurn`] carries only message
/// ids, so the row is reconstructed from the full message list via
/// `rows_by_id` — the same rows the turn was partitioned from (and the row
/// carries the `participantId` the label is resolved from).
fn turns_to_chat_messages(
    turns: &[FoldedTurn],
    rows_by_id: &HashMap<&str, &Value>,
    names: &SpeakerNames,
) -> Vec<FoldTurn> {
    let mut result = Vec::new();
    for t in turns {
        for id in &t.ids {
            if let Some(m) = rows_by_id.get(id.as_str()) {
                let role = m.get("role").and_then(Value::as_str).unwrap_or("");
                result.push(FoldTurn {
                    speaker: speaker_label(
                        m.get("participantId").and_then(Value::as_str),
                        role,
                        names,
                    ),
                    role: role.to_lowercase(),
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

/// v4's `[Context Summary] Resolved speaker names for fold` debug line
/// (`e7821606f`) — so the next unresolvable seat is visible in the log before
/// it is visible in a summary. `seatCount` counts EVERY participant and
/// `unresolvedParticipantIds` is every seat absent from the map, so a seat
/// with no `characterId` is counted unresolved.
///
/// `unresolvedParticipantIdsJson`, not `unresolvedParticipantIds`: v4 hands
/// winston a raw `string[]`; `tracing` has no structured-value channel, so the
/// callsite serializes and the file layer's `…Json` convention re-parses it
/// back into an array under the unsuffixed name (P4.91's `fileIdsJson`).
fn log_resolved_speaker_names(chat_id: &str, participants: &[Value], names: &SpeakerNames) {
    let unresolved: Vec<&str> = participants
        .iter()
        .map(|p| p.get("id").and_then(Value::as_str).unwrap_or_default())
        .filter(|id| !names.has(id))
        .collect();
    let unresolved_json = serde_json::to_string(&unresolved).unwrap_or_else(|_| "[]".into());
    tracing::debug!(
        chatId = chat_id,
        seatCount = participants.len(),
        resolvedCount = names.len(),
        unresolvedParticipantIdsJson = unresolved_json.as_str(),
        "[Context Summary] Resolved speaker names for fold"
    );
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
    let selection = if should_use_uncensored_route(Some(&chat)) {
        if let Some(policy) = options.concierge_policy.as_ref() {
            tracing::debug!(
                chat_id = %chat_id,
                concierge_source = policy.source.as_str(),
                route_direct = policy.route_direct,
                "[Context Summary] Unmoderated chat; resolving uncensored cheap LLM"
            );
        }
        resolve_uncensored_cheap_llm_selection(
            selection,
            true,
            options.concierge_policy.as_ref(),
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
    // v4 resolves + logs BEFORE the empty-turns return, from the chat row it
    // already holds — every participant, removed and silent seats included.
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let speaker_names = db
        .read_main(|conn| Ok(resolve_speaker_names(conn, &participants)))
        .unwrap_or_default();
    log_resolved_speaker_names(&chat_id, &participants, &speaker_names);
    let new_turns_content = turns_to_chat_messages(turns_to_fold, &rows_by_id, &speaker_names);

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
        return Ok(SummaryGenerationResult::fail_from_fold(
            fold_result
                .error
                .as_deref()
                .unwrap_or("Failed to fold summary"),
            fold_result.timed_out,
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

    // Both fold memory passes — the episode pass and the fold-grain OTHER pass
    // — run on one extraction context: the chat, its cast, the project, the
    // Concierge-resolved cheap LLM and its uncensored fallback (bug 213), all
    // read once. Each reads from its own cursor through the end of the folded
    // window, so a hard rebuild's turns-1..N window only reads what that pass
    // has not (bug 207). The transcript this fold already read places the
    // cursors. Best-effort: a failed memory pass never fails the fold. Help
    // chats fold too — v4 has no help-chat gate here.
    seams
        .run_fold_memory_passes(FoldMemoryPassesInput {
            chat_id: chat_id.clone(),
            user_id: options.user_id.clone(),
            chat: chat.clone(),
            fold_transcript: all_messages
                .iter()
                .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
                .cloned()
                .collect(),
            window_messages: turns_to_fold
                .iter()
                .flat_map(|t| t.ids.iter())
                .filter_map(|id| rows_by_id.get(id.as_str()).map(|m| (*m).clone()))
                .collect(),
            connection_profile: profile_value_of(&options.connection_profile),
            chat_context_summary: new_summary.clone(),
        })
        .await;

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

    // v4's `result` — what every remaining exit returns: the fold succeeded
    // whatever happens to the title.
    let result = SummaryGenerationResult {
        success: true,
        summary: Some(new_summary.clone()),
        error: None,
        was_generated: true,
        usage,
        timed_out: false,
    };

    // v4 `00c290c9a` (bug 164): a hand-renamed chat keeps its title, so don't
    // pay for one. Off the START snapshot — the chokepoint re-checks after the
    // call in case the user renames mid-flight. No title LLM call, no
    // TITLE_GENERATION log row, no title cost event.
    if chat.get("isManuallyRenamed").and_then(Value::as_bool) == Some(true) {
        tracing::debug!(
            chatId = chat_id.as_str(),
            "[Context Summary] Chat renamed by hand; skipping fold title"
        );
        return Ok(result);
    }

    // Title generation (literary, or practical for help-like chats).
    let title_result = if is_help_like_chat_type(chat_type.as_deref()) {
        generate_help_chat_title_from_summary(executor, completion, &new_summary, &selection).await
    } else {
        generate_title_from_summary(executor, completion, &new_summary, &selection).await
    };

    // v4's `if (titleResult.success && titleResult.result)` — JS-truthy, so an
    // empty title takes the warn arm.
    match title_result
        .result
        .clone()
        .filter(|t| title_result.success && !t.is_empty())
    {
        Some(title) => {
            // v4 `:590-632` is one try/catch: a failure anywhere from the
            // settings read through the chokepoint lands in the catch's ERROR
            // and skips the title cost event — the fold still succeeds. (Before
            // this port v5 `?`-propagated a title-write error and FAILED the
            // whole fold; that pre-existing divergence closes here.)
            match apply_fold_title(db, &options.user_id, &chat_id, &title).await {
                Ok(outcome) => {
                    // v4 `:605` — one rendered sentence, now WITH `{ outcome }`
                    // (every outcome, not only `applied`).
                    tracing::info!(
                        outcome = outcome.as_str(),
                        "[Context Summary] Generated title for chat {}: {}",
                        chat_id,
                        title
                    );
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
                Err(e) => {
                    // v4 `:631` — `logger.error(msg, {}, err)`: the error rides
                    // as v5's `error` field.
                    tracing::error!(
                        error = %e,
                        "[Context Summary] Error generating title for chat {}:",
                        chat_id
                    );
                }
            }
        }
        None => {
            // v4 `:628` — `${titleResult.error}` renders `undefined` when absent.
            tracing::warn!(
                "[Context Summary] Failed to generate title for chat {}: {}",
                chat_id,
                title_result.error.as_deref().unwrap_or("undefined")
            );
        }
    }

    Ok(result)
}

/// The fold's title write (v4 `context-summary.ts:597-604`): its OWN
/// chat-settings read (the fold's options carry none; `null` → no background),
/// then the auto-title chokepoint with `source: 'summary-fold'` and no extra
/// patch — so a refused title writes nothing at all.
async fn apply_fold_title(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    title: &str,
) -> Result<AutoTitleOutcome, DbError> {
    let uid = user_id.to_string();
    let chat_settings =
        db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))?;
    apply_auto_title(
        db,
        user_id,
        chat_id,
        title,
        chat_settings.as_ref(),
        AutoTitleExtraPatch::default(),
        false,
        AutoTitleSource::SummaryFold,
        &now_iso(),
    )
    .await
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

    // P4.163: v4's `deleteMessagesByIds` is a FALLBACK `safeQuery(…, 0)` — a
    // failed delete logs `Failed to delete messages from chat {chatId, count}`
    // and answers 0, so `context-summary.ts:490` logs its sweep INFO with
    // `removed: 0` and posts the fresh whisper; its catch is unreachable (v5
    // had propagated the error into the caller's silent swallow).
    let (chat_id_owned, count) = (chat_id.to_string(), prior_ids.len());
    let outcome = db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .delete_messages_by_ids(&chat_id_owned, &prior_ids)
        })
        .await;
    Ok(crate::db::fallback::messages_deleted_or_zero(
        chat_id,
        count,
        || outcome,
    ))
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
    concierge_policy: Option<&ResolvedConciergePolicy>,
    registry_cheapest_for_current: Option<&str>,
    await_fold: bool,
) -> Result<Option<CheckOutcome>, DbError> {
    check_and_generate_summary_if_needed_with_seams(
        db,
        completion,
        executor,
        chat_id,
        connection_profile,
        cheap_llm_settings,
        available_profiles,
        user_id,
        concierge_policy,
        registry_cheapest_for_current,
        await_fold,
        &NoopSeams,
    )
    .await
}

/// [`check_and_generate_summary_if_needed`] with the fold's cross-subsystem
/// side effects supplied by `seams`. The orchestrator's in-loop check passes
/// [`FoldEpisodePassSeams`] so the fold-time episode pass runs live (v4's
/// in-loop check calls the real `generateContextSummary`, episode pass
/// included); the bare variant keeps [`NoopSeams`] for callers whose
/// differential mocks the whole seam set.
#[allow(clippy::too_many_arguments)]
pub async fn check_and_generate_summary_if_needed_with_seams<
    C: CompletionProvider,
    S: ContextSummarySeams,
>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    connection_profile: &CheapLlmProfile,
    cheap_llm_settings: &CheapLlmSettings,
    available_profiles: &[CheapLlmProfile],
    user_id: &str,
    concierge_policy: Option<&ResolvedConciergePolicy>,
    registry_cheapest_for_current: Option<&str>,
    _await_fold: bool,
    seams: &S,
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
            concierge_policy: concierge_policy.cloned(),
            registry_cheapest_for_current: registry_cheapest_for_current.map(str::to_string),
            // The gate path's mirror/refresh arms are no-ops in every current
            // seams impl, so the refresh list size is irrelevant — `None`.
            connection_max_context: None,
        };
        // The v5 runtime always awaits the fold (see module docs).
        summary_result = Some(
            generate_context_summary_with_seams(db, completion, executor, &options, seams).await,
        );
    }

    Ok(Some(CheckOutcome {
        current_interchange,
        enqueued_title_job_id,
        decision,
        summary_result,
    }))
}
