//! The **memory processor** — v4 `lib/memory/memory-processor.ts`
//! (`processTurnForMemory`), the model-dependent per-turn extraction service.
//! Runs once per chat turn: resolves the cheap-LLM selection once (so the
//! rendered-transcript prefix cache hits across the whole run), pre-resolves
//! per-character rate limits, then runs two extraction passes over the joined
//! turn transcript —
//!
//!   1. **SELF** — one call per allowed character, its own vantage-point
//!      fields preloaded as the ALREADY ESTABLISHED canon block.
//!   2. **OTHER** — one multi-subject call per observer, covering every other
//!      allowed character plus the user-controlled character; each subject's
//!      canon block prefers the observer's vault `Others/<subject>.md`, then
//!      the subject's identity, then description.
//!
//! Every candidate is written through the already-ported memory gate
//! ([`crate::services::memory_gate`]), so the DB effect funnels through the
//! verified append-or-reinforce path. The two model boundaries are generic
//! parameters: completions ([`CompletionProvider`]) for the extraction calls,
//! embeddings ([`EmbeddingProvider`]) for the gate.
//!
//! Also hosts `load_canon_for_observer_about_subject` + `read_vault_text_file`
//! (v4 `cheap-llm-tasks/canon.ts` / `vault-overlay/vault-readers.ts`) — the
//! OTHER-pass canon loader is the one stateful piece of the canon module, so
//! it lives with its only consumer; the pure renderers are in [`crate::canon`].
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - the per-call `logLLMCall` llm-logs write — see
//!     [`crate::services::cheap_llm_exec`].
//!   - the gate-side deferrals carry over unchanged
//!     (`maybeEnqueueHousekeeping`, `applyNamePresenceCheck`'s lookup branch,
//!     the 500 ms retry delay).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::canon::{
    load_canon_for_self, render_other_canon_block, render_self_canon_block, CanonSource,
    CanonSourceKind,
};
use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, DangerousContentSettings, UncensoredFallbackOptions,
};
use crate::clock;
use crate::context_budget::resolve_max_tokens;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::normalise_relative_path;
use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::jsnum::to_fixed;
use crate::memory_format::Pronouns;
use crate::memory_tasks::{
    build_other_extraction_messages, build_self_extraction_messages, parse_memory_candidate_array,
    parse_other_candidates_by_subject, resolve_max_memories, ExtractionClock, MemoryCandidate,
    OrientingContext, OtherSubjectInput, TurnTranscript,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::{
    CheapLlmTaskExecutor, CheapLlmTaskOptions, CheapLlmTaskResult,
};
use crate::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use crate::vault_overlay::sanitize_file_name;

const OTHERS_FOLDER: &str = "Others";

/// v4 `readVaultTextFile` (`vault-overlay/vault-readers.ts`): read a vault
/// document's content off the mount-index partition, `None` on NOT_FOUND —
/// and on any other failure too (v4 warns and returns null, never throws).
pub fn read_vault_text_file(db: &Db, mount_point_id: &str, path: &str) -> Option<String> {
    db.read_mount_index(|conn| Ok(read_vault_text_file_conn(conn, mount_point_id, path)))
        .ok()
        .flatten()
}

/// [`read_vault_text_file`] over a mount-index connection the caller already
/// holds (the wardrobe-instructions cascade probes up to four tiers inside one
/// read). Same contract: `None` on NOT_FOUND and on any other failure.
pub fn read_vault_text_file_conn(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    path: &str,
) -> Option<String> {
    let rel = normalise_relative_path(path).ok()?;
    DocMountDocumentsRepository::new(mount)
        .find_by_mount_point_and_path(mount_point_id, &rel)
        .ok()
        .flatten()
}

/// A subject of the OTHER pass, pre-resolution (v4's inline `Subject` type).
struct Subject {
    id: String,
    name: String,
    identity: Option<String>,
    description: Option<String>,
    is_user: bool,
}

/// v4 `loadCanonForObserverAboutSubject`: the observer's vault
/// `Others/<sanitized-subject-name>.md` first, then the subject's identity,
/// then description — never personality or manifesto (no observer sees another
/// character's interior or axiomatic core).
fn load_canon_for_observer_about_subject(
    db: &Db,
    observer_mount_point_id: Option<&str>,
    subject: &Subject,
) -> CanonSource {
    if let Some(mount_id) = observer_mount_point_id.filter(|s| !s.is_empty()) {
        let path = format!("{OTHERS_FOLDER}/{}.md", sanitize_file_name(&subject.name));
        if let Some(from_vault) = read_vault_text_file(db, mount_id, &path) {
            let trimmed = crate::jsstr::js_trim(&from_vault);
            if !trimmed.is_empty() {
                return CanonSource {
                    character_id: subject.id.clone(),
                    character_name: subject.name.clone(),
                    body: Some(trimmed.to_string()),
                    source: CanonSourceKind::Vault,
                };
            }
        }
    }

    if let Some(identity) = subject
        .identity
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty())
    {
        return CanonSource {
            character_id: subject.id.clone(),
            character_name: subject.name.clone(),
            body: Some(identity.to_string()),
            source: CanonSourceKind::Identity,
        };
    }

    if let Some(description) = subject
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty())
    {
        return CanonSource {
            character_id: subject.id.clone(),
            character_name: subject.name.clone(),
            body: Some(description.to_string()),
            source: CanonSourceKind::Description,
        };
    }

    CanonSource {
        character_id: subject.id.clone(),
        character_name: subject.name.clone(),
        body: None,
        source: CanonSourceKind::None,
    }
}

/// v4 `extractSelfMemoriesFromTurn`: build the SELF messages and run them
/// through the execution pipeline with the candidate-array parser. The
/// no-slice early return is `{ success: true, result: [], usage: undefined }`.
#[allow(clippy::too_many_arguments)]
pub async fn extract_self_memories_from_turn<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    transcript: &TurnTranscript,
    target_character_id: &str,
    canon_block: &str,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
    resolved_max_tokens: Option<f64>,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
    clock: Option<&ExtractionClock>,
) -> CheapLlmTaskResult<Vec<MemoryCandidate>> {
    let Some(messages) = build_self_extraction_messages(
        transcript,
        target_character_id,
        canon_block,
        resolved_max_tokens,
        in_autonomous_room,
        orienting,
        clock,
    ) else {
        return CheapLlmTaskResult::early(Vec::new());
    };

    executor
        .execute(
            completion,
            selection,
            messages,
            parse_memory_candidate_array,
            uncensored_fallback,
            resolved_max_tokens,
            Some(target_character_id),
            Some("memory-extraction-self"),
            CheapLlmTaskOptions::default(),
        )
        .await
}

/// v4 `extractOtherMemoriesFromTurn`: one multi-subject call per observer,
/// parsed back to per-subject buckets. The no-observer/no-subjects early
/// return carries an empty bucket per subject and no usage.
#[allow(clippy::too_many_arguments)]
pub async fn extract_other_memories_from_turn<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    transcript: &TurnTranscript,
    observer_character_id: &str,
    subjects: &[OtherSubjectInput],
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
    resolved_max_tokens: Option<f64>,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
    clock: Option<&ExtractionClock>,
) -> CheapLlmTaskResult<Vec<(String, Vec<MemoryCandidate>)>> {
    let Some(messages) = build_other_extraction_messages(
        transcript,
        observer_character_id,
        subjects,
        resolved_max_tokens,
        in_autonomous_room,
        orienting,
        clock,
    ) else {
        return CheapLlmTaskResult::early(
            subjects
                .iter()
                .map(|s| (s.id.clone(), Vec::new()))
                .collect(),
        );
    };

    let per_subject_cap = resolve_max_memories(resolved_max_tokens);
    executor
        .execute(
            completion,
            selection,
            messages,
            |content| parse_other_candidates_by_subject(content, subjects, per_subject_cap),
            uncensored_fallback,
            resolved_max_tokens,
            Some(observer_character_id),
            Some("memory-extraction-other"),
            CheapLlmTaskOptions::default(),
        )
        .await
}

/// v4 `MemoryExtractionLimits` (settings subset the rate limiter consumes).
#[derive(Clone, Debug)]
pub struct MemoryExtractionLimits {
    pub enabled: bool,
    pub max_per_hour: f64,
    pub soft_start_fraction: f64,
    pub soft_floor: f64,
}

/// v4 `CheapLLMSettings` subset (`toCheapLLMConfig` maps exactly these three —
/// the settings' `defaultCheapProfileId` deliberately does not reach the
/// extraction path).
#[derive(Clone, Debug, Default)]
pub struct CheapLlmSettings {
    pub strategy: String,
    pub user_defined_profile_id: Option<String>,
    pub fallback_to_local: bool,
}

fn to_cheap_llm_config(settings: &CheapLlmSettings) -> CheapLlmConfig {
    CheapLlmConfig {
        strategy: settings.strategy.clone(),
        // v4 `userDefinedProfileId || undefined` — empty string drops.
        user_defined_profile_id: settings
            .user_defined_profile_id
            .clone()
            .filter(|s| !s.is_empty()),
        default_cheap_profile_id: None,
        fallback_to_local: settings.fallback_to_local,
    }
}

/// Per-turn memory extraction context (v4 `TurnMemoryExtractionContext`).
/// `participant_characters` holds the hydrated character records (the net JSON
/// the characters read overlay returns) keyed by characterId — the canon
/// loader reads `identity`/`description`/`manifesto`/`personality` and
/// `characterDocumentMountPointId` from them without fresh DB calls.
pub struct TurnMemoryExtractionContext {
    pub transcript: TurnTranscript,
    pub participant_characters: HashMap<String, Value>,
    pub chat_id: String,
    pub project_id: Option<String>,
    pub project_description: Option<String>,
    pub chat_context_summary: Option<String>,
    pub user_id: String,
    pub connection_profile: CheapLlmProfile,
    pub cheap_llm_settings: CheapLlmSettings,
    /// `None` mirrors v4's absent `availableProfiles` (which also disables the
    /// uncensored fallback); `Some(vec![])` is a present-but-empty list.
    pub available_profiles: Option<Vec<CheapLlmProfile>>,
    pub danger_settings: Option<DangerousContentSettings>,
    pub is_dangerous_chat: bool,
    pub memory_extraction_limits: Option<MemoryExtractionLimits>,
    /// Override the source-message timestamp on derived memories (batch
    /// re-extraction backfill).
    pub source_message_timestamp: Option<String>,
    /// Which clock the chat's story runs on (`chat.timelineMode`; `None` →
    /// `"realtime"`). Feeds the extraction CLOCK block and decides whether an
    /// unresolved / in-story `when` phrase is preserved as `narrativeTime`.
    pub timeline_mode: Option<String>,
    /// When true, run all passes but skip persistence — candidates are
    /// returned on the result instead.
    pub dry_run: bool,
    /// Autonomous-room provenance: prepends the user-absence clause and stamps
    /// `witnessedContext = 'autonomous_room'`.
    pub in_autonomous_room: bool,
    /// The plugin registry's cheapest-model answer for
    /// `connection_profile.provider` (the injected `getCheapModelConfig` seam;
    /// `None` = no plugin registered → v4's legacy fallback map).
    pub registry_cheapest_for_current: Option<String>,
}

/// Candidate captured during a dry run (v4 `ExtractedCandidate`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedCandidate {
    pub pass: String,
    pub character_id: String,
    pub character_name: String,
    pub about_character_id: String,
    pub about_character_name: String,
    pub source_message_id: Option<String>,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub importance: serde_json::Number,
    /// Episodic spine (dry-run visibility): declared kind + raw/resolved anchors.
    pub kind: String,
    pub when: Option<String>,
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub entities: Vec<String>,
}

/// Aggregate token usage (v4's `usage` totals — plain sums of the per-call
/// numbers).
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// v4 `TurnMemoryProcessingResult`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnMemoryProcessingResult {
    pub success: bool,
    pub memories_created_count: usize,
    pub memories_reinforced_count: usize,
    pub created_memory_ids: Vec<String>,
    pub reinforced_memory_ids: Vec<String>,
    pub usage: UsageTotals,
    pub source_message_id: Option<String>,
    pub debug_logs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_candidates: Option<Vec<ExtractedCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

enum RateLimitDecision {
    Allow,
    Throttle {
        floor: f64,
        recent_count: i64,
        cap: f64,
    },
    Skip {
        recent_count: i64,
        cap: f64,
    },
}

/// v4 `resolveExtractionRateLimit`: count the character's memories minted in
/// the last wall-clock hour; at/over the cap skip, past the soft-start
/// fraction throttle to the floor. A lookup failure allows (v4 warns and
/// falls through).
fn resolve_extraction_rate_limit(
    db: &Db,
    character_id: &str,
    limits: Option<&MemoryExtractionLimits>,
) -> RateLimitDecision {
    let Some(limits) = limits.filter(|l| l.enabled) else {
        return RateLimitDecision::Allow;
    };

    let one_hour_ago = clock::iso_from_unix_ms(clock::now_unix_ms() - 60 * 60 * 1000);
    let recent_count = match db
        .read_main(|conn| memories_read::count_created_since(conn, character_id, &one_hour_ago))
    {
        Ok(count) => count,
        // v4 logs a warning and allows extraction on a failed lookup.
        Err(_) => return RateLimitDecision::Allow,
    };

    if recent_count as f64 >= limits.max_per_hour {
        return RateLimitDecision::Skip {
            recent_count,
            cap: limits.max_per_hour,
        };
    }

    let soft_start = (limits.max_per_hour * limits.soft_start_fraction).floor();
    if recent_count as f64 >= soft_start {
        return RateLimitDecision::Throttle {
            floor: limits.soft_floor,
            recent_count,
            cap: limits.max_per_hour,
        };
    }

    RateLimitDecision::Allow
}

/// v4 `applyImportanceFloor`.
fn apply_importance_floor(candidates: Vec<MemoryCandidate>, floor: f64) -> Vec<MemoryCandidate> {
    candidates
        .into_iter()
        .filter(|c| c.importance_f64() >= floor)
        .collect()
}

/// JS template interpolation of a possibly-absent string: `${undefined}` is
/// the literal `undefined`.
fn js_interp(value: Option<&str>) -> &str {
    value.unwrap_or("undefined")
}

struct RunState {
    debug_logs: Vec<String>,
    created_memory_ids: Vec<String>,
    reinforced_memory_ids: Vec<String>,
    collected_candidates: Vec<ExtractedCandidate>,
    total_usage: UsageTotals,
}

impl RunState {
    fn add_usage(&mut self, usage: Option<crate::model::completion::CompletionUsage>) {
        if let Some(u) = usage {
            self.total_usage.prompt_tokens += u.prompt_tokens;
            self.total_usage.completion_tokens += u.completion_tokens;
            self.total_usage.total_tokens += u.total_tokens;
        }
    }
}

struct WriteArgs<'a> {
    character_id: &'a str,
    character_name: &'a str,
    about_character_id: &'a str,
    about_character_name: &'a str,
    pass: &'a str,
    candidate: &'a MemoryCandidate,
    pass_label: String,
    source_message_id: Option<String>,
    /// `createdAt` of the source message — the wall-clock anchor for `occurredAt`.
    source_message_created_at: Option<String>,
}

/// Resolve a candidate's episodic anchors against the source turn's clock (v4
/// `resolveCandidateAnchors`).
///
/// Stamping rules (episodic spine):
///  - Every memory gets `occurredAt` = the source message timestamp by default
///    (authoritative from the transcript — never asked of the model).
///  - A retold EVENT with a `when` phrase resolves that phrase server-side
///    against the same anchor; a successful resolution overrides the default.
///  - On fictional timelines the raw phrase is preserved as `narrativeTime`
///    (whether or not it also resolved to a wall-clock date).
fn resolve_candidate_anchors(
    candidate: &MemoryCandidate,
    anchor_iso: Option<&str>,
    timeline_mode: &str,
) -> (Option<String>, Option<String>) {
    let fallback = anchor_iso.map(str::to_string);
    let mut occurred_at = fallback.clone();
    // v4 `candidate.when && fallback` — both JS-truthy (an empty `when` never
    // survives the parser's coercion, and an empty anchor is falsy).
    if let (Some(when), Some(anchor)) = (
        candidate.when.as_deref().filter(|w| !w.is_empty()),
        fallback.as_deref().filter(|f| !f.is_empty()),
    ) {
        if let Some(resolved) = crate::episodic::resolve_when_phrase(Some(when), anchor) {
            occurred_at = Some(resolved);
        }
    }
    let narrative_time = match (timeline_mode, candidate.when.as_deref()) {
        ("narrative", Some(when)) if !when.is_empty() => Some(when.to_string()),
        _ => None,
    };
    (occurred_at, narrative_time)
}

/// v4 `writeCandidate`: dry-run collection, or a write through the memory gate
/// with the per-outcome debug line.
async fn write_candidate<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    ctx: &TurnMemoryExtractionContext,
    state: &mut RunState,
    args: WriteArgs<'_>,
) -> Result<(), crate::db::DbError> {
    let candidate = args.candidate;
    let summary_interp = js_interp(candidate.summary.as_deref()).to_string();

    // Episodic spine: resolve the candidate's anchors against the source turn's
    // clock. v4's `??` chain — the slice's own message timestamp first, then the
    // batch-backfill override, then the turn timestamp, then the write clock.
    let timeline_mode = ctx.timeline_mode.as_deref().unwrap_or("realtime");
    let anchor_iso = args
        .source_message_created_at
        .clone()
        .or_else(|| ctx.source_message_timestamp.clone())
        .or_else(|| ctx.transcript.turn_timestamp.clone())
        .unwrap_or_else(clock::now_iso);
    let (occurred_at, narrative_time) =
        resolve_candidate_anchors(candidate, Some(&anchor_iso), timeline_mode);

    if ctx.dry_run {
        state.collected_candidates.push(ExtractedCandidate {
            pass: args.pass.to_string(),
            character_id: args.character_id.to_string(),
            character_name: args.character_name.to_string(),
            about_character_id: args.about_character_id.to_string(),
            about_character_name: args.about_character_name.to_string(),
            source_message_id: args.source_message_id.clone(),
            content: candidate.content.clone().unwrap_or_default(),
            summary: candidate.summary.clone().unwrap_or_default(),
            keywords: candidate.keywords.clone(),
            importance: candidate.importance.clone(),
            kind: candidate.kind.clone(),
            when: candidate.when.clone(),
            occurred_at: occurred_at.clone(),
            narrative_time: narrative_time.clone(),
            entities: candidate.entities.clone().unwrap_or_default(),
        });
        state.debug_logs.push(format!(
            "[Memory] DRY-RUN {} — would write: {summary_interp}",
            args.pass_label
        ));
        return Ok(());
    }

    // v4 `candidate.importance || 0.5` — `||`, so an explicit 0 defaults too.
    let importance = {
        let v = candidate.importance_f64();
        if v == 0.0 {
            0.5
        } else {
            v
        }
    };

    let outcome = create_memory_with_gate(
        db,
        embedding,
        &CreateMemoryOptions {
            character_id: args.character_id.to_string(),
            about_character_id: Some(args.about_character_id.to_string()),
            chat_id: Some(ctx.chat_id.clone()),
            project_id: ctx.project_id.clone(),
            content: candidate.content.clone().unwrap_or_default(),
            summary: candidate.summary.clone().unwrap_or_default(),
            keywords: candidate.keywords.clone(),
            importance: Some(importance),
            source: Some("AUTO".to_string()),
            source_message_id: args.source_message_id.clone(),
            source_message_timestamp: ctx.source_message_timestamp.clone(),
            tags: Vec::new(),
            // 4.6 Private Character Rooms: provenance for auto-extracted
            // memories — autonomous rooms get explicit attribution; ordinary
            // chats get 'user_present'.
            witnessed_context: Some(
                if ctx.in_autonomous_room {
                    "autonomous_room"
                } else {
                    "user_present"
                }
                .to_string(),
            ),
            // Episodic spine: event time + anchors, resolved above.
            occurred_at: occurred_at.clone(),
            narrative_time: narrative_time.clone(),
            entities: candidate.entities.clone().unwrap_or_default(),
            kind: Some(candidate.kind.clone()),
        },
        &MemoryServiceOptions {
            user_id: ctx.user_id.clone(),
            embedding_profile_id: None,
        },
    )
    .await?;

    match outcome.action {
        GateAction::SkipNearDuplicate => {
            let similarity = outcome
                .similarity
                .map(|s| to_fixed(s, 3))
                .unwrap_or_else(|| "n/a".to_string());
            let absorbed = outcome.memory_id.as_deref().unwrap_or("unknown");
            state.debug_logs.push(format!(
                "[Memory] SKIPPED near-duplicate {}: absorbed into {absorbed} (similarity {similarity}) — {summary_interp}",
                args.pass_label
            ));
        }
        GateAction::SkipEmbeddingFailed => {
            let reason = outcome.reason.as_deref().unwrap_or("unknown");
            state.debug_logs.push(format!(
                "[Memory] SKIPPED {} — embedding generation failed: {reason}",
                args.pass_label
            ));
        }
        GateAction::Reinforce => {
            if let Some(id) = &outcome.memory_id {
                state.reinforced_memory_ids.push(id.clone());
            }
            let count = outcome.reinforcement_count.unwrap_or(1.0);
            let id = outcome.memory_id.as_deref().unwrap_or("unknown");
            state.debug_logs.push(format!(
                "[Memory] REINFORCED {}: {id} (count {count}) — {summary_interp}",
                args.pass_label
            ));
        }
        GateAction::InsertRelated => {
            if let Some(id) = &outcome.memory_id {
                state.created_memory_ids.push(id.clone());
            }
            state.debug_logs.push(format!(
                "[Memory] Created {} (linked to {} related) — {summary_interp}",
                args.pass_label,
                outcome.related_memory_ids.len()
            ));
        }
        GateAction::Insert => {
            if let Some(id) = &outcome.memory_id {
                state.created_memory_ids.push(id.clone());
            }
            state.debug_logs.push(format!(
                "[Memory] Created {} — {summary_interp}",
                args.pass_label
            ));
        }
    }

    Ok(())
}

fn character_str_field<'a>(
    characters: &'a HashMap<String, Value>,
    character_id: &str,
    field: &str,
) -> Option<&'a str> {
    characters
        .get(character_id)
        .and_then(|c| c.get(field))
        .and_then(Value::as_str)
}

/// Run all extraction passes for a single turn (v4 `processTurnForMemory`).
pub async fn process_turn_for_memory<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    ctx: &TurnMemoryExtractionContext,
) -> TurnMemoryProcessingResult {
    let mut state = RunState {
        debug_logs: Vec::new(),
        created_memory_ids: Vec::new(),
        reinforced_memory_ids: Vec::new(),
        collected_candidates: Vec::new(),
        total_usage: UsageTotals::default(),
    };
    let source_message_id = ctx.transcript.latest_assistant_message_id.clone();

    match run_passes(db, completion, embedding, executor, ctx, &mut state).await {
        Ok(()) => TurnMemoryProcessingResult {
            success: true,
            memories_created_count: state.created_memory_ids.len(),
            memories_reinforced_count: state.reinforced_memory_ids.len(),
            created_memory_ids: state.created_memory_ids,
            reinforced_memory_ids: state.reinforced_memory_ids,
            usage: state.total_usage,
            source_message_id,
            debug_logs: state.debug_logs,
            extracted_candidates: ctx.dry_run.then_some(state.collected_candidates),
            error: None,
        },
        Err(error) => TurnMemoryProcessingResult {
            success: false,
            memories_created_count: 0,
            memories_reinforced_count: 0,
            created_memory_ids: state.created_memory_ids,
            reinforced_memory_ids: state.reinforced_memory_ids,
            usage: state.total_usage,
            source_message_id,
            debug_logs: state.debug_logs,
            extracted_candidates: ctx.dry_run.then_some(state.collected_candidates),
            error: Some(error.to_string()),
        },
    }
}

async fn run_passes<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    ctx: &TurnMemoryExtractionContext,
    state: &mut RunState,
) -> Result<(), crate::db::DbError> {
    let source_message_id = ctx.transcript.latest_assistant_message_id.clone();

    if ctx.transcript.character_slices.is_empty() {
        state
            .debug_logs
            .push("[Memory] Turn has no character contributions — nothing to extract".to_string());
        return Ok(());
    }

    // Resolve the cheap LLM selection ONCE for the whole turn — every pass
    // reuses the same provider so the cacheable transcript prefix hits.
    let config = to_cheap_llm_config(&ctx.cheap_llm_settings);
    let empty_profiles: Vec<CheapLlmProfile> = Vec::new();
    let profiles: &[CheapLlmProfile] = ctx.available_profiles.as_deref().unwrap_or(&empty_profiles);
    let selection = get_cheap_llm_provider(
        &ctx.connection_profile,
        &config,
        profiles,
        false,
        ctx.registry_cheapest_for_current.as_deref(),
    );
    let selection = resolve_uncensored_cheap_llm_selection(
        selection,
        ctx.is_dangerous_chat,
        ctx.danger_settings.as_ref(),
        profiles,
    );

    let uncensored_fallback: Option<UncensoredFallbackOptions<'_>> =
        match (&ctx.danger_settings, &ctx.available_profiles) {
            (Some(danger), Some(available)) => Some(UncensoredFallbackOptions {
                danger_settings: danger,
                available_profiles: available,
                is_dangerous_chat: None,
            }),
            _ => None,
        };

    let cheap_max_tokens = resolve_max_tokens(
        ctx.connection_profile.max_tokens.map(|v| v as i64),
        ctx.connection_profile.model_class.as_deref(),
    ) as f64;

    // Non-canonical orienting context, identical across every call this turn.
    let orienting = OrientingContext {
        project_description: ctx.project_description.clone(),
        chat_context_summary: ctx.chat_context_summary.clone(),
    };

    // Episodic spine: give the extractor a clock, anchored to the turn's own
    // message timestamps (batch re-extraction passes its historical override).
    let clock = ExtractionClock {
        now_iso: ctx
            .source_message_timestamp
            .clone()
            .or_else(|| ctx.transcript.turn_timestamp.clone())
            .unwrap_or_else(clock::now_iso),
        timeline_mode: ctx
            .timeline_mode
            .clone()
            .unwrap_or_else(|| "realtime".to_string()),
        // v4 builds the clock with only `nowIso` + `timelineMode`; the
        // in-story line rides the fold pass's own clock.
        narrative_now: None,
    };

    // Pre-resolve per-character rate limits.
    let mut rate_limits: HashMap<String, RateLimitDecision> = HashMap::new();
    for slice in &ctx.transcript.character_slices {
        rate_limits.insert(
            slice.character_id.clone(),
            resolve_extraction_rate_limit(
                db,
                &slice.character_id,
                ctx.memory_extraction_limits.as_ref(),
            ),
        );
    }

    let allowed_slices: Vec<&crate::memory_tasks::TurnCharacterSlice> = ctx
        .transcript
        .character_slices
        .iter()
        .filter(|s| {
            let rl = &rate_limits[&s.character_id];
            match rl {
                RateLimitDecision::Skip { recent_count, cap } => {
                    state.debug_logs.push(format!(
                        "[Memory] SKIPPED extraction for {} — rate limit reached ({recent_count}/{cap} memories in last hour)",
                        s.character_name
                    ));
                    false
                }
                RateLimitDecision::Throttle {
                    floor,
                    recent_count,
                    cap,
                } => {
                    state.debug_logs.push(format!(
                        "[Memory] THROTTLING {} — {recent_count}/{cap}; floor {floor}",
                        s.character_name
                    ));
                    true
                }
                RateLimitDecision::Allow => true,
            }
        })
        .collect();

    // ---------------------------------------------------------------------
    // Build the subject set for the OTHER pass: every allowed character plus
    // the user-controlled character (if any). De-dupe by character ID — the
    // slice loop wins; the explicit user block only fires as a fallback when
    // the user character is present but silent.
    // ---------------------------------------------------------------------
    let mut subjects: Vec<Subject> = Vec::new();
    let mut seen_subject_ids: HashSet<String> = HashSet::new();
    for slice in &allowed_slices {
        if !seen_subject_ids.insert(slice.character_id.clone()) {
            continue;
        }
        subjects.push(Subject {
            id: slice.character_id.clone(),
            name: slice.character_name.clone(),
            identity: character_str_field(
                &ctx.participant_characters,
                &slice.character_id,
                "identity",
            )
            .map(str::to_string),
            description: character_str_field(
                &ctx.participant_characters,
                &slice.character_id,
                "description",
            )
            .map(str::to_string),
            is_user: slice.is_user_controlled,
        });
    }
    if let (Some(user_id), Some(user_name)) = (
        ctx.transcript
            .user_character_id
            .as_deref()
            .filter(|s| !s.is_empty()),
        ctx.transcript
            .user_character_name
            .as_deref()
            .filter(|s| !s.is_empty()),
    ) {
        if seen_subject_ids.contains(user_id) {
            state.debug_logs.push(format!(
                "[Memory] Skipped duplicate user subject for {user_name} — already present as a turn slice"
            ));
        } else {
            seen_subject_ids.insert(user_id.to_string());
            subjects.push(Subject {
                id: user_id.to_string(),
                name: user_name.to_string(),
                identity: character_str_field(&ctx.participant_characters, user_id, "identity")
                    .map(str::to_string),
                description: character_str_field(
                    &ctx.participant_characters,
                    user_id,
                    "description",
                )
                .map(str::to_string),
                is_user: true,
            });
        }
    }

    // ---------------------------------------------------------------------
    // Pass 1: SELF memories (one call per allowed character; canon = own
    // identity fields, no vault lookup).
    // ---------------------------------------------------------------------
    for slice in &allowed_slices {
        let rl = &rate_limits[&slice.character_id];
        let self_canon_block = render_self_canon_block(&load_canon_for_self(
            &slice.character_id,
            &slice.character_name,
            character_str_field(
                &ctx.participant_characters,
                &slice.character_id,
                "manifesto",
            ),
            character_str_field(
                &ctx.participant_characters,
                &slice.character_id,
                "personality",
            ),
            character_str_field(
                &ctx.participant_characters,
                &slice.character_id,
                "description",
            ),
            character_str_field(&ctx.participant_characters, &slice.character_id, "identity"),
        ));

        let self_result = extract_self_memories_from_turn(
            executor,
            completion,
            &ctx.transcript,
            &slice.character_id,
            &self_canon_block,
            &selection,
            uncensored_fallback.as_ref(),
            Some(cheap_max_tokens),
            ctx.in_autonomous_room,
            Some(&orienting),
            Some(&clock),
        )
        .await;

        state.add_usage(self_result.usage);

        if self_result.success {
            let raw_candidates = self_result.result.unwrap_or_default();
            let raw_len = raw_candidates.len();
            let candidates = match rl {
                RateLimitDecision::Throttle { floor, .. } => {
                    apply_importance_floor(raw_candidates, *floor)
                }
                _ => raw_candidates,
            };
            if let RateLimitDecision::Throttle { floor, .. } = rl {
                if candidates.len() < raw_len {
                    state.debug_logs.push(format!(
                        "[Memory] Throttle dropped {} SELF candidate(s) for {} below importance {floor}",
                        raw_len - candidates.len(),
                        slice.character_name
                    ));
                }
            }
            state.debug_logs.push(format!(
                "[Memory] SELF for {}{}: {} candidate(s)",
                slice.character_name,
                if slice.is_user_controlled {
                    " (user-controlled)"
                } else {
                    ""
                },
                candidates.len()
            ));
            for candidate in &candidates {
                write_candidate(
                    db,
                    embedding,
                    ctx,
                    state,
                    WriteArgs {
                        character_id: &slice.character_id,
                        character_name: &slice.character_name,
                        about_character_id: &slice.character_id,
                        about_character_name: &slice.character_name,
                        pass: "SELF",
                        candidate,
                        pass_label: format!("SELF memory for {}", slice.character_name),
                        source_message_id: slice
                            .contributing_message_ids
                            .last()
                            .cloned()
                            .or_else(|| source_message_id.clone()),
                        source_message_created_at: slice.last_message_created_at.clone(),
                    },
                )
                .await?;
            }
        } else {
            state.debug_logs.push(format!(
                "[Memory] SELF extraction failed for {}: {}",
                slice.character_name,
                js_interp(self_result.error.as_deref())
            ));
        }
    }

    // ---------------------------------------------------------------------
    // Pass 2: OTHER memories (one multi-subject call per observer). Each
    // subject's canon block comes from the observer's vault
    // `Others/<subject>.md` first, then the subject's identity property; the
    // canon source is preserved per subject for the debug attribution.
    // ---------------------------------------------------------------------
    for observer in &allowed_slices {
        let rl = &rate_limits[&observer.character_id];
        let observer_mount = character_str_field(
            &ctx.participant_characters,
            &observer.character_id,
            "characterDocumentMountPointId",
        );

        let observer_subjects: Vec<&Subject> = subjects
            .iter()
            .filter(|s| s.id != observer.character_id)
            .collect();
        if observer_subjects.is_empty() {
            continue;
        }

        // Resolve every subject's canon block + pronouns up front so the
        // single LLM call is assembled in one shot.
        struct ResolvedSubject {
            input: OtherSubjectInput,
            canon_source: &'static str,
            subject_name: String,
        }
        let mut resolved_subjects: Vec<ResolvedSubject> = Vec::new();
        for subject in &observer_subjects {
            let canon = load_canon_for_observer_about_subject(db, observer_mount, subject);
            let subject_canon_block = render_other_canon_block(&canon);
            let pronouns: Option<Pronouns> = if subject.is_user {
                ctx.transcript.user_character_pronouns.clone()
            } else {
                ctx.transcript
                    .character_slices
                    .iter()
                    .find(|s| s.character_id == subject.id)
                    .and_then(|s| s.character_pronouns.clone())
            };
            resolved_subjects.push(ResolvedSubject {
                input: OtherSubjectInput {
                    id: subject.id.clone(),
                    name: subject.name.clone(),
                    pronouns,
                    is_user: subject.is_user,
                    canon_block: subject_canon_block,
                },
                canon_source: canon.source.as_str(),
                subject_name: subject.name.clone(),
            });
        }
        let subject_inputs: Vec<OtherSubjectInput> =
            resolved_subjects.iter().map(|r| r.input.clone()).collect();

        let other_result = extract_other_memories_from_turn(
            executor,
            completion,
            &ctx.transcript,
            &observer.character_id,
            &subject_inputs,
            &selection,
            uncensored_fallback.as_ref(),
            Some(cheap_max_tokens),
            ctx.in_autonomous_room,
            Some(&orienting),
            Some(&clock),
        )
        .await;

        state.add_usage(other_result.usage);

        if !other_result.success {
            state.debug_logs.push(format!(
                "[Memory] OTHER extraction failed ({} → {} subject(s)): {}",
                observer.character_name,
                resolved_subjects.len(),
                js_interp(other_result.error.as_deref())
            ));
            continue;
        }

        let candidates_by_subject = other_result.result.unwrap_or_default();
        for subject in &resolved_subjects {
            let raw_candidates = candidates_by_subject
                .iter()
                .find(|(id, _)| *id == subject.input.id)
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            let raw_len = raw_candidates.len();
            let candidates = match rl {
                RateLimitDecision::Throttle { floor, .. } => {
                    apply_importance_floor(raw_candidates, *floor)
                }
                _ => raw_candidates,
            };
            if let RateLimitDecision::Throttle { floor, .. } = rl {
                if candidates.len() < raw_len {
                    state.debug_logs.push(format!(
                        "[Memory] Throttle dropped {} OTHER candidate(s) for {} about {} below importance {floor}",
                        raw_len - candidates.len(),
                        observer.character_name,
                        subject.subject_name
                    ));
                }
            }
            state.debug_logs.push(format!(
                "[Memory] OTHER observer={}{} subject={} canon={}: {} candidate(s)",
                observer.character_name,
                if observer.is_user_controlled {
                    " (user-controlled)"
                } else {
                    ""
                },
                subject.subject_name,
                subject.canon_source,
                candidates.len()
            ));
            for candidate in &candidates {
                write_candidate(
                    db,
                    embedding,
                    ctx,
                    state,
                    WriteArgs {
                        character_id: &observer.character_id,
                        character_name: &observer.character_name,
                        about_character_id: &subject.input.id,
                        about_character_name: &subject.subject_name,
                        pass: "OTHER",
                        candidate,
                        pass_label: format!(
                            "OTHER memory {} about {}",
                            observer.character_name, subject.subject_name
                        ),
                        source_message_id: observer
                            .contributing_message_ids
                            .last()
                            .cloned()
                            .or_else(|| source_message_id.clone()),
                        source_message_created_at: observer.last_message_created_at.clone(),
                    },
                )
                .await?;
            }
        }
    }

    Ok(())
}
