//! Stage 6 — assemble: one turn's recall, every pool, from one query (v4
//! `lib/memory/recall/assemble.ts`, `584d28164`).
//!
//! ```text
//! query (distil + embed, once) → head pool (search) → archive → recap → head → inter-character → conversations
//! ```
//!
//! v4's why, carried:
//! - **One embedding per turn.** The query is embedded once (the proactive
//!   pass hands its query in already embedded); the head, the inter-character
//!   relevance half, the recap's relevant-conversations list, the per-turn
//!   conversation list and (when the turn names no one) the retrospective
//!   mini-recap all reuse it. Only a retrospective turn embeds more: its
//!   probes (≤ 2), in the query stage, and a mini-recap query that adds the
//!   names the turn reaches for.
//! - **The inter-character relevance half is a filter of the turn's one
//!   search** by `aboutCharacterId` (decision 12), not a search per present
//!   character. It reads the search's ranked rows past the head's cut.
//! - **A row reaches the model in at most one line-rendered pool** (bug 224),
//!   in this order of precedence: archive → head → inter-character. The recap
//!   is a narrative, not lines: it is built without the archive's rows (which
//!   the archive already says verbatim), but it never takes a row from the
//!   head or the inter-character section — a lossy summary is no stand-in for
//!   the memory's own words on the turn it is most wanted. The archive keeps
//!   its composition and id sort; its size and token budget are fixed per
//!   compaction generation, so a retrospective turn's larger head never
//!   changes it — the head borrows from what the archive leaves.
//!
//! Reads only. What was delivered is `deliver`'s to record, after the request
//! body is built.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::cheap_llm::CheapLlmSelection;
use crate::cheap_llm::UncensoredFallbackOptions;
use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::memory_pools::{size_memory_pools, MemoryPoolSizing};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_tuning::ResolvedRecallTuning;
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::memory_recap::distill::{DistillMessage, ExtractionClock};
use crate::services::memory_recap::{
    generate_memory_recap, ramp_limit, render_relevant_conversations_block,
    search_vault_conversation_summaries, MemoryRecapRequest, READ_CONVERSATION_CALL_NOTE,
    RELEVANT_CONVERSATIONS_MAX, RELEVANT_CONVERSATIONS_MIN, RELEVANT_CONVERSATIONS_RAMP_MAX_TOKENS,
    RELEVANT_CONVERSATIONS_RAMP_MIN_TOKENS,
};
use crate::token_estimation::estimate_tokens;

use super::context::{build_turn_recall_context, TurnRecallContextInput, TurnRecallSettings};
use super::query::{build_recall_query, BuildRecallQueryInput, RecallDistill, RecallQuery};
use super::render::{
    chars_per_token, render_archive, render_head, render_inter_character,
    DebugInterCharacterMemoryInfo, DebugMemoryInfo, MemorySubjectContext, RenderHeadOptions,
};
use super::search::{search_recall, RecallSearchOptions};
use super::types::{
    memory_id, num, trace_stage, RecallCandidate, RecallPool, RecallStage, RecallTrace,
    RenderedLine,
};

const RECALL_LOG_TARGET: &str = "quilltap::recall";

/// Per-character caps on the inter-character section's two halves.
pub const INTER_CHAR_PER_CHARACTER_LIMIT: i64 = 5;
pub const INTER_CHAR_DIGEST_LIMIT: i64 = 2;
pub const INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT: usize = 5;

/// Cap on entries in the retrospective mini-recap conversation list.
pub const RETRO_MINI_RECAP_MAX_ENTRIES: usize = 5;

/// Wall-clock ceiling on the recap. Above the interactive cheap-LLM budget on
/// purpose: a recap merely slow in its embedding and its narrative call is
/// still working, and it is optional context.
pub const MEMORY_RECAP_PHASE_TIMEOUT_MS: u64 = 60_000;

/// A query the proactive pass already built and searched (v4 `PreRecall`).
#[derive(Clone, Debug, Default)]
pub struct PreRecall {
    pub query: RecallQuery,
    pub candidates: Vec<RecallCandidate>,
    /// The search's ranked rows before its cut (the inter-character relevance
    /// half reads these).
    pub ranked: Option<Vec<RecallCandidate>>,
    pub trace: RecallTrace,
}

/// The chat a plan reads (v4 `RecallPlan['chat']`).
#[derive(Clone, Copy, Debug)]
pub struct RecallChat<'a> {
    pub id: &'a str,
    pub project_id: Option<&'a str>,
    pub compaction_generation: i64,
    pub commonplace_recall_history: &'a Value,
    pub timeline_mode: Option<&'a str>,
}

/// The fallback distillation (v4 `RecallPlan['distill']`).
pub struct PlanDistill<'a> {
    pub messages: Vec<DistillMessage>,
    pub selection: &'a CheapLlmSelection,
    pub clock_iso: String,
    /// The host's calendar NAME — the memory calendar's last rung. The zone
    /// itself is resolved through `refresh_memory_time_zone` at the moment the
    /// distillation actually runs (bug 212: v4's `clock?.timeZone ?? await
    /// refreshMemoryTimeZone()` inside `extractMemorySearchKeywords`).
    pub host_zone_fallback: String,
}

/// v4 `RecallPlan['recap']`.
pub struct PlanRecap<'a> {
    pub selection: &'a CheapLlmSelection,
    pub uncensored_fallback: Option<&'a UncensoredFallbackOptions<'a>>,
    pub max_context: Option<i64>,
    /// The recap's own relevance text, for a turn with no query.
    pub fallback_query: String,
}

/// v4 `RecallPlan['conversations']`.
pub struct PlanConversations<'a> {
    pub max_context: Option<i64>,
    /// UUIDs the standing fold whisper already lists for this target (read
    /// lazily, at most once — the caller's closure caches).
    pub standing_ids: &'a (dyn Fn() -> HashSet<String> + Sync),
}

/// The replay's knobs (v4 `RecallPlan['replay']`).
#[derive(Clone, Debug, Default)]
pub struct PlanReplay {
    pub exclude_ids: Option<HashSet<String>>,
    pub now_ms: Option<f64>,
    pub tuning: Option<ResolvedRecallTuning>,
    pub ring: Option<HashSet<String>>,
}

/// The instance recall settings a plan reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlanRecallSettings {
    pub scope_policy: crate::recall_tags::ScopePolicy,
    pub expand_related: bool,
    pub per_turn_conversation_summaries: bool,
}

/// v4 `RecallPlan`.
pub struct RecallPlan<'a> {
    pub user_id: &'a str,
    pub embedding_profile_id: Option<&'a str>,
    /// The connection's provider — the renderers' token rate (R-T).
    pub provider: Option<&'a str>,
    pub character_id: &'a str,
    pub character_name: &'a str,
    /// The character's vault (the conversation lists search it).
    pub character_mount_point_id: Option<&'a str>,
    pub chat: RecallChat<'a>,
    pub memory_budget: i64,
    pub min_importance: f64,
    /// No memory pools at all (the recap may still run).
    pub skip_memories: bool,
    /// Everyone present, the responder included (participant boost, R4).
    pub present: Vec<(String, String)>,
    /// The others the inter-character section covers. Empty → no section.
    pub inter_character_subjects: Vec<(String, String)>,
    /// Tokens the `## Current State` block takes.
    pub current_state_tokens: i64,
    pub recall_settings: PlanRecallSettings,
    /// The recent-window prose — the query when nothing better is distilled.
    pub recent_window: String,
    pub distill: Option<PlanDistill<'a>>,
    pub pre_recall: Option<PreRecall>,
    pub recap: Option<PlanRecap<'a>>,
    /// The conversation lists. `None` → none (the replay).
    pub conversations: Option<PlanConversations<'a>>,
    pub on_status_change: Option<StatusChange<'a>>,
    pub replay: Option<PlanReplay>,
    /// The turn's clock (v4 `Date.now()`; the replay pins its own).
    pub now_ms: f64,
}

/// The status callback a plan carries (v4 `onStatusChange`).
pub type StatusChange<'a> = &'a (dyn Fn(&str, &str) + Sync);

/// v4 `RecallBundle['path']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecallPath {
    #[default]
    Skipped,
    PreSearched,
    TwoPool,
}

impl RecallPath {
    pub fn as_str(self) -> &'static str {
        match self {
            RecallPath::Skipped => "skipped",
            RecallPath::PreSearched => "pre-searched",
            RecallPath::TwoPool => "two-pool",
        }
    }
}

/// v4 `RecallBundle['sections']`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecallSections {
    pub recap: Option<String>,
    /// Archive + head.
    pub relevant: Option<String>,
    pub inter_character: Option<String>,
    pub relevant_conversations: Option<String>,
    pub retrospective: Option<String>,
}

/// v4 `RecallBundle['tokens']`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecallTokens {
    pub recap: i64,
    pub relevant: i64,
    pub inter_character: i64,
}

/// v4 `RecallBundle['pools']` — rendered ids per pool. The recap's are the
/// rows its narrative read — not delivered in their own words, so never
/// stamped or deduped against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecallPools {
    pub archive: Vec<String>,
    pub head: Vec<String>,
    pub inter_character: Vec<String>,
    pub recap: Vec<String>,
}

impl RecallPools {
    /// v4's `{ archive, head, interCharacter, recap }` object.
    pub fn to_json(&self) -> Value {
        let ids = |v: &Vec<String>| Value::Array(v.iter().cloned().map(Value::String).collect());
        let mut m = Map::new();
        m.insert("archive".into(), ids(&self.archive));
        m.insert("head".into(), ids(&self.head));
        m.insert("interCharacter".into(), ids(&self.inter_character));
        m.insert("recap".into(), ids(&self.recap));
        Value::Object(m)
    }
}

/// v4 `RecallBundle`.
#[derive(Clone, Debug, Default)]
pub struct RecallBundle {
    pub query: Option<RecallQuery>,
    pub path: RecallPath,
    /// The search's result: the head draws on it.
    pub pool: Vec<RecallCandidate>,
    /// The same search's ranked rows before its cut to the head's pull.
    pub relevance_pool: Vec<RecallCandidate>,
    pub sections: RecallSections,
    pub tokens: RecallTokens,
    /// What each line-rendered pool wrote, for `deliver`, in assignment order
    /// (v4's `lines` object property order).
    pub lines: Vec<(RecallPool, Vec<RenderedLine>)>,
    pub pools: RecallPools,
    pub retro_signature: Option<String>,
    pub debug_memories: Vec<DebugMemoryInfo>,
    pub debug_inter_character_memories: Vec<DebugInterCharacterMemoryInfo>,
    pub memories_included: usize,
    pub inter_character_included: usize,
    pub trace: RecallTrace,
    pub warnings: Vec<String>,
}

/// v4 `emptyRecallBundle()`.
pub fn empty_recall_bundle() -> RecallBundle {
    RecallBundle::default()
}

/// v4's `withTimeout(generateMemoryRecap(req), MEMORY_RECAP_PHASE_TIMEOUT_MS,
/// "Memory recap exceeded its 60000ms phase budget")` with the assemble
/// stage's catch folded in: a fired ceiling pushes `Failed to generate memory
/// recap: <message>` into the warnings and logs ERROR `[Recall] Memory recap
/// generation failed {characterId}` (v4's error object rides as the third
/// argument; its message is the `error` field here), and the recap is dropped
/// — `None`. `generate_memory_recap` itself never fails (it degrades to an
/// empty result), so the ceiling is the catch's one reachable arm.
///
/// Extracted because a fired ceiling is wall-clock behaviour no NDJSON corpus
/// can observe (the canned providers never stall) — the proof is unit-tier.
pub(crate) async fn recap_within_phase_budget(
    work: impl std::future::Future<Output = crate::services::memory_recap::MemoryRecapResult>,
    character_id: &str,
    warnings: &mut Vec<String>,
) -> Option<crate::services::memory_recap::MemoryRecapResult> {
    match tokio::time::timeout(
        std::time::Duration::from_millis(MEMORY_RECAP_PHASE_TIMEOUT_MS),
        work,
    )
    .await
    {
        Ok(recap) => Some(recap),
        Err(_) => {
            let message =
                format!("Memory recap exceeded its {MEMORY_RECAP_PHASE_TIMEOUT_MS}ms phase budget");
            warnings.push(format!("Failed to generate memory recap: {message}"));
            tracing::error!(
                target: RECALL_LOG_TARGET,
                characterId = %character_id,
                error = %message,
                "[Recall] Memory recap generation failed"
            );
            None
        }
    }
}

/// v4's `errorText` — the error's message.
fn error_text(e: &DbError) -> String {
    e.to_string()
}

fn status(plan: &RecallPlan<'_>, stage: &str, message: &str) {
    if let Some(cb) = plan.on_status_change {
        cb(stage, message);
    }
}

/// v4 `assembleRecall`.
pub async fn assemble_recall<E, C>(
    db: &Db,
    embedding: &E,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    plan: RecallPlan<'_>,
) -> RecallBundle
where
    E: EmbeddingProvider,
    C: CompletionProvider,
{
    let mut bundle = empty_recall_bundle();
    let character_id = plan.character_id;
    let replay = plan.replay.clone().unwrap_or_default();
    let now_ms = replay.now_ms.unwrap_or(plan.now_ms);
    let exclude_ids = replay.exclude_ids.clone();
    let mut present_ids: Vec<String> = Vec::new();
    for (id, _) in &plan.present {
        if !id.is_empty() && !present_ids.contains(id) {
            present_ids.push(id.clone());
        }
    }
    let ordinary_sizing = size_memory_pools(plan.memory_budget as f64, false);
    let retro_sizing = size_memory_pools(plan.memory_budget as f64, true);

    // 1. The query — handed in by the proactive pass, or built here (once).
    if !plan.skip_memories {
        // The proactive pass's result stands only when it found something: an
        // empty or failed proactive search gets the fallback's own distil and
        // search, as before (review of PR 95). R-L8: its signals are then
        // discarded and distilled again — ported as v4 has it.
        let pre = plan
            .pre_recall
            .as_ref()
            .filter(|p| !p.query.text.is_empty() && !p.candidates.is_empty());
        if let Some(pre) = pre {
            bundle.query = Some(pre.query.clone());
            bundle.pool = pre.candidates.clone();
            bundle.relevance_pool = pre.ranked.clone().unwrap_or_else(|| pre.candidates.clone());
            bundle.path = RecallPath::PreSearched;
            bundle.trace.extend(pre.trace.iter().cloned());
        } else if !plan.recent_window.is_empty() {
            bundle.path = RecallPath::TwoPool;
            // The distiller runs only on a non-blank window (v4's
            // `!signals && input.distill && input.text.trim()`); the memory
            // calendar's zone is resolved there and then, as v4 resolves it.
            let will_distill = !crate::jsstr::js_trim(&plan.recent_window).is_empty();
            let distill = plan
                .distill
                .as_ref()
                .filter(|_| will_distill)
                .map(|d| RecallDistill {
                    completion,
                    executor,
                    messages: d.messages.clone(),
                    character_name: plan.character_name,
                    selection: d.selection,
                    character_id,
                    clock: Some(ExtractionClock {
                        now_iso: d.clock_iso.clone(),
                        timeline_mode: plan.chat.timeline_mode.unwrap_or("realtime").to_string(),
                        time_zone: Some(
                            crate::services::memory_time_zone::refresh_memory_time_zone(
                                db,
                                &d.host_zone_fallback,
                                now_ms,
                            ),
                        ),
                    }),
                    // The operator is waiting: tight budget, no retry (bug 115).
                    latency: CheapLlmTaskOptions::interactive(),
                });
            let query = build_recall_query(
                embedding,
                BuildRecallQueryInput {
                    user_id: plan.user_id,
                    embedding_profile_id: plan.embedding_profile_id,
                    text: &plan.recent_window,
                    signals: None,
                    distill,
                    embedding_memo: None,
                    skip_embedding: false,
                    probes: None,
                },
                &mut bundle.trace,
            )
            .await;
            let mut ctx = build_turn_recall_context(TurnRecallContextInput {
                chat_id: plan.chat.id,
                chat_project_id: plan.chat.project_id,
                commonplace_recall_history: plan.chat.commonplace_recall_history,
                recall_settings: TurnRecallSettings {
                    scope_policy: plan.recall_settings.scope_policy,
                    expand_related: plan.recall_settings.expand_related,
                },
                turn_context: query.signals.as_ref().and_then(|s| s.context),
                turn_temporal: query.signals.as_ref().and_then(|s| s.temporal),
                turn_retrospective: query.retrospective,
                present_about_character_ids: present_ids.clone(),
                present_participant_names: plan
                    .present
                    .iter()
                    .map(|(_, n)| n.clone())
                    .filter(|n| !n.is_empty())
                    .collect(),
                now_ms,
                tuning: replay.tuning.clone(),
            });
            if let Some(ring) = &replay.ring {
                ctx.recently_whispered_ids = Some(ring.clone());
            }
            let sizing = if query.retrospective {
                retro_sizing
            } else {
                ordinary_sizing
            };
            let result = search_recall(
                db,
                character_id,
                &query,
                &RecallSearchOptions {
                    // A few more than the head, so the archive overlap still
                    // leaves enough.
                    limit: Some(sizing.head_entries * 3),
                    min_importance: Some(plan.min_importance),
                    recall_context: Some(ctx),
                    exclude_ids: exclude_ids.clone(),
                    now_ms,
                    ..Default::default()
                },
                &mut bundle.trace,
                Some("head"),
            )
            .await;
            bundle.query = Some(query);
            match result {
                Ok(r) => {
                    bundle.pool = r.candidates;
                    bundle.relevance_pool = r.ranked;
                }
                Err(e) => bundle
                    .warnings
                    .push(format!("Failed to retrieve memories: {}", error_text(&e))),
            }
        }
    }
    let query = bundle.query.clone();
    let retrospective = query.as_ref().is_some_and(|q| q.retrospective);
    let turn_sizing: MemoryPoolSizing = if retrospective {
        retro_sizing
    } else {
        ordinary_sizing
    };

    // 2. The frozen archive, rendered under the ordinary archive budget.
    let mut archive_ids: HashSet<String> = HashSet::new();
    let mut subject = MemorySubjectContext {
        self_character_id: character_id.to_string(),
        character_names: HashMap::new(),
    };
    let mut archive_tokens = 0i64;
    let mut archive_content = String::new();
    if !plan.skip_memories {
        let others: Vec<String> = present_ids
            .iter()
            .filter(|id| id.as_str() != character_id)
            .cloned()
            .collect();
        match crate::services::frozen_archive::get_or_compute_frozen_archive(
            db,
            character_id,
            plan.chat.id,
            plan.chat.compaction_generation,
            Some(ordinary_sizing.archive_size),
            &others,
            now_ms,
            exclude_ids.as_ref(),
        ) {
            Ok(archive_memories) => {
                archive_ids = archive_memories
                    .iter()
                    .map(|m| memory_id(m).to_string())
                    .collect();
                // Every self-facing line names a subject that is not the
                // character (bug 122) — ONE lookup over archive + pool.
                subject = crate::services::memory_subject::build_memory_subject_context(
                    db,
                    character_id,
                    archive_memories
                        .iter()
                        .chain(bundle.pool.iter().map(|c| &c.memory))
                        .map(|m| {
                            m.get("aboutCharacterId")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .collect::<Vec<_>>(),
                );
                // Always the ordinary budget: a retrospective turn's larger head
                // never trims the archive, so its bytes hold for the whole
                // generation.
                let archive = render_archive(
                    &archive_memories,
                    ordinary_sizing.archive_token_budget,
                    plan.provider,
                    &subject,
                );
                archive_content = archive.content;
                archive_tokens = archive.token_count;
                bundle.pools.archive = archive.lines.iter().map(|l| l.memory_id.clone()).collect();
                let rendered = archive.lines.len();
                bundle.lines.push((RecallPool::Archive, archive.lines));
                bundle.debug_memories.extend(archive.debug_memories);
                let mut detail = Map::new();
                detail.insert("size".into(), num(ordinary_sizing.archive_size as f64));
                detail.insert("held".into(), num(archive_memories.len() as f64));
                detail.insert("rendered".into(), num(rendered as f64));
                detail.insert(
                    "cutByBudget".into(),
                    num(archive_memories.len() as f64 - rendered as f64),
                );
                detail.insert(
                    "tokenBudget".into(),
                    num(ordinary_sizing.archive_token_budget as f64),
                );
                trace_stage(
                    &mut bundle.trace,
                    RecallStage::Assemble,
                    detail,
                    Some("archive"),
                );
            }
            Err(e) => bundle
                .warnings
                .push(format!("Failed to retrieve memories: {}", error_text(&e))),
        }
    }

    // 3. The recap (chat start / character join): a narrative of what the
    // archive does not already say, its conversation list on the turn's vector.
    if let Some(recap_plan) = &plan.recap {
        status(
            &plan,
            "generating_recap",
            &format!("Recalling {}'s memories...", plan.character_name),
        );
        let turn_embedding = query
            .as_ref()
            .and_then(|q| q.embedding.as_ref())
            .filter(|_| query.as_ref().is_some_and(|q| !q.text.is_empty()));
        let use_turn_query = turn_embedding.is_some();
        let relevance_query = if use_turn_query {
            query.as_ref().map(|q| q.text.clone()).unwrap_or_default()
        } else {
            recap_plan.fallback_query.clone()
        };
        let exclude: HashSet<String> = bundle.pools.archive.iter().cloned().collect();
        let request = MemoryRecapRequest {
            character_id,
            character_name: plan.character_name,
            character_mount_point_id: plan.character_mount_point_id,
            selection: recap_plan.selection,
            user_id: plan.user_id,
            chat_id: Some(plan.chat.id),
            uncensored_fallback: recap_plan.uncensored_fallback,
            max_context: recap_plan.max_context,
            relevance_query: &relevance_query,
            relevance_embedding: turn_embedding.map(|e| e.embedding.as_slice()),
            embedding_profile_id: plan.embedding_profile_id,
            exclude_ids: Some(&exclude),
            now_ms,
        };
        if let Some(recap) = recap_within_phase_budget(
            generate_memory_recap(db, embedding, completion, executor, &request),
            character_id,
            &mut bundle.warnings,
        )
        .await
        {
            if !recap.content.is_empty() {
                bundle.tokens.recap =
                    estimate_tokens(&recap.content, chars_per_token(plan.provider));
                bundle.sections.recap = Some(recap.content);
                bundle.pools.recap = recap.memory_ids.clone();
            }
            let mut detail = Map::new();
            detail.insert("rows".into(), num(recap.memory_ids.len() as f64));
            detail.insert("reusedQuery".into(), Value::Bool(use_turn_query));
            detail.insert(
                "excludedAsArchive".into(),
                num(bundle.pools.archive.len() as f64),
            );
            trace_stage(
                &mut bundle.trace,
                RecallStage::Assemble,
                detail,
                Some("recap"),
            );
        }
    }
    status(&plan, "assembling_context", "Assembling context...");

    // 4. The head: the pool past the archive. The recap's rows stay eligible —
    // its narrative may have left out exactly the line this turn needs.
    let mut head_tokens = 0i64;
    let mut head_content = String::new();
    // A retrospective head borrows what the archive left of the memory budget
    // (never less than the ordinary head).
    let head_token_budget = if retrospective {
        retro_sizing.head_token_budget.min(
            ordinary_sizing
                .head_token_budget
                .max(plan.memory_budget - archive_tokens),
        )
    } else {
        ordinary_sizing.head_token_budget
    };
    if !plan.skip_memories && !bundle.pool.is_empty() {
        let head_candidates: Vec<RecallCandidate> = bundle
            .pool
            .iter()
            .filter(|c| !archive_ids.contains(c.id()))
            .cloned()
            .collect();
        let head = render_head(
            &head_candidates,
            plan.provider,
            &subject,
            RenderHeadOptions {
                max_tokens: head_token_budget,
                max_entries: turn_sizing.head_entries,
                now_ms,
            },
        );
        head_content = head.content;
        head_tokens = head.token_count;
        bundle.pools.head = head.lines.iter().map(|l| l.memory_id.clone()).collect();
        let rendered = head.lines.len();
        bundle.lines.push((RecallPool::Head, head.lines));
        bundle.debug_memories.extend(head.debug_memories);
        let removed = bundle
            .pool
            .iter()
            .filter(|c| archive_ids.contains(c.id()))
            .count();
        let mut detail = Map::new();
        detail.insert("pool".into(), num(bundle.pool.len() as f64));
        detail.insert("removedAsArchive".into(), num(removed as f64));
        detail.insert("rendered".into(), num(rendered as f64));
        detail.insert(
            "cutByBudget".into(),
            num(head_candidates.len().min(turn_sizing.head_entries) as f64 - rendered as f64),
        );
        detail.insert("entries".into(), num(turn_sizing.head_entries as f64));
        detail.insert("tokenBudget".into(), num(head_token_budget as f64));
        detail.insert("retrospective".into(), Value::Bool(retrospective));
        trace_stage(
            &mut bundle.trace,
            RecallStage::Assemble,
            detail,
            Some("head"),
        );
    }
    let relevant: Vec<&str> = [archive_content.as_str(), head_content.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    bundle.sections.relevant = (!relevant.is_empty()).then(|| relevant.join("\n\n"));
    bundle.tokens.relevant = archive_tokens + head_tokens;
    bundle.memories_included = bundle.pools.archive.len() + bundle.pools.head.len();

    // 5. Inter-character: about the others present. Both halves skip anything
    // an earlier pool already delivers.
    if !plan.skip_memories && !plan.inter_character_subjects.is_empty() {
        if let Err(e) =
            assemble_inter_character(db, &plan, &mut bundle, now_ms, exclude_ids.as_ref())
        {
            bundle.warnings.push(format!(
                "Failed to retrieve inter-character memories: {}",
                error_text(&e)
            ));
        }
    }

    // 6. The conversation lists, on the turn's vector.
    if let (Some(conversations), Some(query)) = (&plan.conversations, &query) {
        assemble_conversations(db, embedding, &plan, conversations, &mut bundle, query).await;
    }

    bundle
}

fn assemble_inter_character(
    db: &Db,
    plan: &RecallPlan<'_>,
    bundle: &mut RecallBundle,
    now_ms: f64,
    exclude_ids: Option<&HashSet<String>>,
) -> Result<(), DbError> {
    let character_id = plan.character_id;
    let subjects = &plan.inter_character_subjects;
    // `Math.floor((memoryBudget − relevant − currentState) / 2)`.
    let budget = ((plan.memory_budget - bundle.tokens.relevant - plan.current_state_tokens) as f64
        / 2.0)
        .floor() as i64;
    if budget <= 0 {
        return Ok(());
    }
    let other_ids: Vec<String> = subjects.iter().map(|(id, _)| id.clone()).collect();
    let delivered: HashSet<String> = bundle
        .pools
        .archive
        .iter()
        .chain(bundle.pools.head.iter())
        .cloned()
        .collect();

    // Importance half: hot digests about each, then the partition query.
    let mut digests: Vec<Value> = Vec::new();
    for id in &other_ids {
        let cid = character_id.to_string();
        let about = id.clone();
        digests.extend(db.read_main(move |conn| {
            memories_read::find_hot_digests(
                conn,
                &cid,
                memories_read::DigestSubject::About(about),
                INTER_CHAR_DIGEST_LIMIT,
            )
        })?);
    }
    let partition = {
        let cid = character_id.to_string();
        let others = other_ids.clone();
        db.read_main(move |conn| {
            memories_read::find_by_character_about_characters(
                conn,
                &cid,
                &others,
                INTER_CHAR_PER_CHARACTER_LIMIT,
            )
        })?
    };
    let mut seen: HashSet<String> = delivered.clone();
    let mut skipped = 0usize;
    let mut importance: Vec<Value> = Vec::new();
    for m in digests.into_iter().chain(partition) {
        let id = memory_id(&m).to_string();
        if exclude_ids.is_some_and(|e| e.contains(&id)) {
            continue;
        }
        if delivered.contains(&id) {
            skipped += 1;
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        importance.push(m);
    }

    // Relevance half: the turn's pool, filtered to rows about each other
    // present.
    let mut per_character: HashMap<String, usize> = HashMap::new();
    let mut relevance: Vec<RecallCandidate> = Vec::new();
    for c in &bundle.relevance_pool {
        let Some(about) = c
            .memory
            .get("aboutCharacterId")
            .and_then(Value::as_str)
            .filter(|a| !a.is_empty())
        else {
            continue;
        };
        if !other_ids.iter().any(|o| o == about) {
            continue;
        }
        if delivered.contains(c.id()) {
            skipped += 1;
            continue;
        }
        let taken = per_character.get(about).copied().unwrap_or(0);
        if taken >= INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT {
            continue;
        }
        per_character.insert(about.to_string(), taken + 1);
        relevance.push(c.clone());
    }

    let names: HashMap<String, String> = subjects.iter().cloned().collect();
    let section = render_inter_character(
        &importance,
        &names,
        budget,
        plan.provider,
        &relevance,
        now_ms,
    );
    bundle.sections.inter_character =
        (!section.content.is_empty()).then(|| section.content.clone());
    bundle.tokens.inter_character = section.token_count;
    bundle.pools.inter_character = section.lines.iter().map(|l| l.memory_id.clone()).collect();
    bundle.inter_character_included = section.memories_used;
    let rendered = section.lines.len();
    bundle
        .lines
        .push((RecallPool::InterCharacter, section.lines));
    bundle.debug_inter_character_memories = section.debug_memories;
    let mut detail = Map::new();
    detail.insert("subjects".into(), num(other_ids.len() as f64));
    detail.insert("importance".into(), num(importance.len() as f64));
    detail.insert("relevance".into(), num(relevance.len() as f64));
    detail.insert("skippedAsDelivered".into(), num(skipped as f64));
    detail.insert("rendered".into(), num(rendered as f64));
    detail.insert("tokenBudget".into(), num(budget as f64));
    trace_stage(
        &mut bundle.trace,
        RecallStage::Assemble,
        detail,
        Some("interCharacter"),
    );
    Ok(())
}

/// JS `Array.prototype.sort()`'s default order (UTF-16 code units).
fn js_default_sort(items: &mut [String]) {
    items.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

async fn assemble_conversations<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    plan: &RecallPlan<'_>,
    conversations: &PlanConversations<'_>,
    bundle: &mut RecallBundle,
    query: &RecallQuery,
) {
    let turn_embedding: Option<&[f32]> = query.embedding.as_ref().map(|e| e.embedding.as_slice());
    let character_id = plan.character_id;
    let mut listed: HashSet<String> = HashSet::new();

    // Per-turn list (instance setting, off by default). The recap already
    // lists the shelf on the turn it runs. It rides the turn's vector or sits
    // the turn out — never an embedding of its own.
    if let Some(vector) = turn_embedding {
        if plan.recall_settings.per_turn_conversation_summaries && bundle.sections.recap.is_none() {
            let matches = search_vault_conversation_summaries(
                db,
                embedding,
                character_id,
                plan.character_mount_point_id,
                &query.text,
                plan.user_id,
                plan.embedding_profile_id,
                ramp_limit(
                    conversations.max_context,
                    RELEVANT_CONVERSATIONS_MIN,
                    RELEVANT_CONVERSATIONS_MAX,
                    RELEVANT_CONVERSATIONS_RAMP_MIN_TOKENS,
                    RELEVANT_CONVERSATIONS_RAMP_MAX_TOKENS,
                )
                .max(0) as usize,
                Some(plan.chat.id),
                query.window.as_ref(),
                Some(vector),
            )
            .await;
            let standing = (conversations.standing_ids)();
            let filtered: Vec<_> = matches
                .iter()
                .filter(|m| !standing.contains(&m.conversation_id))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                for m in &filtered {
                    listed.insert(m.conversation_id.clone());
                }
                bundle.sections.relevant_conversations = Some(format!(
                    "{}\n\n{READ_CONVERSATION_CALL_NOTE}",
                    render_relevant_conversations_block(&filtered)
                ));
            }
            let mut detail = Map::new();
            detail.insert("matched".into(), num(matches.len() as f64));
            detail.insert("listed".into(), num(filtered.len() as f64));
            trace_stage(
                &mut bundle.trace,
                RecallStage::Assemble,
                detail,
                Some("conversations"),
            );
        }
    }

    // Retrospective mini-recap: a dated, drillable list scoped by the turn's
    // window, skipped when the same window/entity signature went out recently.
    if query.retrospective {
        let entities: Vec<String> = query
            .signals
            .as_ref()
            .map(|s| s.entities.clone())
            .unwrap_or_default();
        let mut keys: Vec<String> = entities.iter().map(|e| e.to_lowercase()).collect();
        js_default_sort(&mut keys);
        let mut parts: Vec<String> = vec![match &query.window {
            Some(w) => format!("{}→{}", w.from, w.to),
            None => "∅".to_string(),
        }];
        parts.extend(keys);
        let signature = parts.join("|");
        if crate::recall_history::parse_retro_signatures(plan.chat.commonplace_recall_history)
            .contains(&signature)
        {
            let mut detail = Map::new();
            detail.insert("suppressed".into(), Value::Bool(true));
            detail.insert("signature".into(), Value::String(signature));
            trace_stage(
                &mut bundle.trace,
                RecallStage::Assemble,
                detail,
                Some("retrospective"),
            );
            return;
        }
        // The mini-recap asks about the paraphrase AND the names it reaches
        // for, so "remember Lighthouse Point?" finds the conversation that names
        // the place (review of PR 95). With no entities that is the turn's
        // query and rides its vector; otherwise it is this retrospective turn's
        // one extra embedding, beside its probes.
        let named: Vec<String> = entities
            .iter()
            .filter(|e| !crate::jsstr::js_trim(e).is_empty())
            .cloned()
            .collect();
        let mini_parts: Vec<String> = [query.text.clone(), named.join(", ")]
            .into_iter()
            .filter(|p| !crate::jsstr::js_trim(p).is_empty())
            .collect();
        let mini_query = mini_parts.join(" — ");
        let reuse = mini_query == query.text && turn_embedding.is_some();
        let matches = search_vault_conversation_summaries(
            db,
            embedding,
            character_id,
            plan.character_mount_point_id,
            &mini_query,
            plan.user_id,
            plan.embedding_profile_id,
            RETRO_MINI_RECAP_MAX_ENTRIES,
            Some(plan.chat.id),
            query.window.as_ref(),
            if reuse { turn_embedding } else { None },
        )
        .await;
        let standing = (conversations.standing_ids)();
        let filtered: Vec<_> = matches
            .iter()
            .filter(|m| {
                !standing.contains(&m.conversation_id) && !listed.contains(&m.conversation_id)
            })
            .take(RETRO_MINI_RECAP_MAX_ENTRIES)
            .cloned()
            .collect();
        if !filtered.is_empty() {
            bundle.sections.retrospective = Some(format!(
                "{}\n\n{READ_CONVERSATION_CALL_NOTE}",
                render_relevant_conversations_block(&filtered)
            ));
            bundle.retro_signature = Some(signature.clone());
        }
        let mut detail = Map::new();
        detail.insert("matched".into(), num(matches.len() as f64));
        detail.insert("listed".into(), num(filtered.len() as f64));
        detail.insert("signature".into(), Value::String(signature));
        detail.insert("reusedQuery".into(), Value::Bool(reuse));
        trace_stage(
            &mut bundle.trace,
            RecallStage::Assemble,
            detail,
            Some("retrospective"),
        );
    }
}

#[cfg(test)]
mod phase_ceiling_tests {
    //! P4.D42 unit 3 — the memory-recap phase ceiling (v4 `74ec93b5`), moved
    //! here with the recap call (v4 Phase E: `assembleRecall`'s recap stage).
    //!
    //! Wall-clock behaviour no NDJSON corpus can observe (P4.15): the canned
    //! providers never stall. Paused-clock unit pins instead.

    use super::*;
    use crate::services::memory_recap::MemoryRecapResult;
    use crate::test_support::CaptureLayer;

    /// v4's literal, spelled out rather than read back from the constant, so
    /// moving the bound has to be a deliberate edit here too.
    #[test]
    fn the_phase_ceiling_is_v4s_sixty_seconds() {
        assert_eq!(MEMORY_RECAP_PHASE_TIMEOUT_MS, 60_000);
        // Deliberately ABOVE the per-leg cheap-LLM budget: a recap that is
        // merely slow in two places is still doing useful work. A const block,
        // so inverting the ordering fails to COMPILE rather than merely failing
        // a test. The comparison is against the **interactive** budget (v4
        // `a1d88aa3a`, bug 107): the recap declares itself interactive
        // precisely so this phase ceiling stays above its legs.
        const {
            assert!(
                MEMORY_RECAP_PHASE_TIMEOUT_MS
                    > crate::services::cheap_llm_exec::CHEAP_LLM_TASK_TIMEOUT_INTERACTIVE_MS
            )
        };
        // …and the background budget is ABOVE it, which is exactly why the
        // recap must not take that class.
        const {
            assert!(
                crate::services::cheap_llm_exec::CHEAP_LLM_TASK_TIMEOUT_MS
                    > MEMORY_RECAP_PHASE_TIMEOUT_MS
            )
        };
    }

    fn landed(content: &str) -> MemoryRecapResult {
        MemoryRecapResult {
            content: content.to_string(),
            ..Default::default()
        }
    }

    /// A recap that never finishes is abandoned at the ceiling — dropped, the
    /// turn keeps its remaining context and moves on — and v4's catch pushes
    /// its prefixed message into the context warnings.
    #[tokio::test(start_paused = true)]
    async fn a_stalled_recap_is_dropped_at_the_ceiling() {
        let started = tokio::time::Instant::now();
        let mut warnings = Vec::new();
        let recap =
            recap_within_phase_budget(std::future::pending(), "char-1", &mut warnings).await;
        assert!(recap.is_none());
        assert_eq!(started.elapsed().as_millis() as u64, 60_000);
        assert_eq!(
            warnings,
            vec![
                "Failed to generate memory recap: Memory recap exceeded its 60000ms phase budget"
                    .to_string()
            ]
        );
    }

    /// The other half: a recap that is merely slow — well past the per-leg
    /// budget — still lands, whole. A ceiling that truncated healthy work
    /// would be worse than no ceiling.
    #[tokio::test(start_paused = true)]
    async fn a_slow_but_finishing_recap_is_not_truncated() {
        let work = async {
            tokio::time::sleep(std::time::Duration::from_millis(55_000)).await;
            landed("## What You Remember\n…")
        };
        let mut warnings = Vec::new();
        let recap = recap_within_phase_budget(work, "char-1", &mut warnings)
            .await
            .expect("a landed recap");
        assert_eq!(recap.content, "## What You Remember\n…");
        assert!(warnings.is_empty(), "a landed recap must push no warning");
    }

    /// A fired ceiling narrates itself at v4's own level and words
    /// (`logger.error('[Recall] Memory recap generation failed',
    /// { characterId }, error)`).
    #[tokio::test(start_paused = true)]
    async fn a_fired_ceiling_logs_error_with_its_context() {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            recap_within_phase_budget(std::future::pending(), "char-7", &mut Vec::new()).await;
        }
        let captured = logs.lock().unwrap().join("\n");
        for expected in [
            "ERROR quilltap::recall",
            "[Recall] Memory recap generation failed",
            "characterId=char-7",
            "Memory recap exceeded its 60000ms phase budget",
        ] {
            assert!(
                captured.contains(expected),
                "the ceiling's line is missing {expected}; captured:\n{captured}"
            );
        }
    }

    /// A healthy recap logs nothing — the line means "this fired", not "this ran".
    #[tokio::test(start_paused = true)]
    async fn a_healthy_recap_says_nothing() {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            recap_within_phase_budget(async { landed("") }, "char-1", &mut Vec::new()).await;
        }
        assert!(logs.lock().unwrap().is_empty());
    }
}
