//! The pass runner — port of v4 `lib/memory/extraction/run.ts` (`ac6348c5f`;
//! pin `83334b9a5`): one way a pass's model output becomes memories.
//!
//! v4's why, carried: a pass ([`ExtractionPass`]) says which cheap-LLM calls
//! it makes over a window and how to read each answer. [`run_pass`] does the
//! rest, the same for every pass: rate limit, call, parse (the pass's cap),
//! floor, candidate, event time, admission. In a dry run admission decides
//! and writes nothing, so the candidate list and the decisions are exactly
//! what live extraction would produce from the same model output (invariant
//! 10).
//!
//! Every candidate reaches the store ONLY through admission's
//! [`admit_candidate`] (C1-B); inside a job it carries the attempt's
//! [`crate::services::admission::AdmissionKeyScope`] (L-16) and admission
//! mints v4's `jobId:contentKey:n` key itself. One candidate's failed
//! admission costs that candidate alone (pre-refactor v5 aborted the whole
//! turn on a gate `DbError`).

use std::collections::HashMap;

use crate::admission::outcome::admission_inserted;
use crate::admission::types::{AdmissionAction, AdmissionOutcome};
use crate::db::fallback::error_text;
use crate::memory_candidate::{
    anchor_time_ms_from, candidate_from_draft, CandidatePass, CandidateProvenance,
    CandidateSubject, MemoryCandidate, MemorySource, ResolvedCandidateTime, WitnessedContext,
};
use crate::memory_extraction::parse::MemoryDraft;
use crate::memory_extraction::window::{first_valid_iso, TranscriptWindow};
use crate::memory_time::anchors::{resolve_episodic_anchors, EpisodicAnchorInput};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::model::embedding::EmbeddingProvider;
use crate::services::admission::{admit_candidate, AdmitOptions};
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::extraction::context::{CastMember, ExtractionContext};
use crate::services::extraction::passes::ExtractionPass;
use crate::services::extraction::rate_limit::{apply_importance_floor, RateLimitDecision};

/// The model / embedding / executor seams a unit of work runs through.
pub struct ExtractionDeps<'a, C, E> {
    pub completion: &'a C,
    pub embedding: &'a E,
    pub executor: &'a CheapLlmTaskExecutor,
}

/// v4 `ExtractionPassName`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractionPassName {
    SelfTurn,
    OtherTurn,
    OtherFold,
    EpisodeFold,
}

impl ExtractionPassName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfTurn => "self-turn",
            Self::OtherTurn => "other-turn",
            Self::OtherFold => "other-fold",
            Self::EpisodeFold => "episode-fold",
        }
    }
}

/// v4 `DraftBatch['floor']` — the pass floor before the rate limit's (the
/// hybrid mode's per-turn OTHER floor).
#[derive(Clone, Debug)]
pub struct BatchFloor {
    pub value: f64,
    pub reason: &'static str,
}

/// One parsed group of drafts and who they become memories for (v4
/// `DraftBatch`). A SELF draft is the holder's about themselves (`about`
/// `None`); an OTHER draft the observer's about one subject; an episode is
/// written for every holder. Holders / subject are cast `characterId`s.
#[derive(Clone, Debug)]
pub struct DraftBatch {
    pub holders: Vec<String>,
    pub about: Option<String>,
    pub drafts: Vec<MemoryDraft>,
    pub floor: Option<BatchFloor>,
    /// The message the memory cites, and its time (the row's `createdAt`).
    pub source_message_id: Option<String>,
    pub source_created_at: Option<String>,
    /// What a `when` phrase resolves against, and what `occurredAt` falls
    /// back to.
    pub reference_iso: Option<String>,
    pub fallback_iso: Option<String>,
}

/// How a call's answer is read into batches (v4's per-call `parse` closure,
/// made data so a call is plain and `Send`).
#[derive(Clone, Debug)]
pub enum CallParse {
    SelfDrafts {
        cap: usize,
        template: DraftBatch,
    },
    OtherBySubject {
        cap: usize,
        subject_ids: Vec<String>,
        template: DraftBatch,
    },
    Episodes {
        cap: usize,
        template: DraftBatch,
    },
}

impl CallParse {
    /// v4 `call.parse(raw)`.
    pub fn parse(&self, raw: &str) -> Vec<DraftBatch> {
        use crate::memory_extraction::parse::{
            parse_episode_drafts, parse_other_drafts_by_subject, parse_self_drafts,
        };
        match self {
            Self::SelfDrafts { cap, template } => vec![DraftBatch {
                drafts: parse_self_drafts(raw, *cap),
                ..template.clone()
            }],
            Self::OtherBySubject {
                cap,
                subject_ids,
                template,
            } => {
                let by_subject = parse_other_drafts_by_subject(raw, subject_ids, *cap);
                subject_ids
                    .iter()
                    .map(|id| DraftBatch {
                        about: Some(id.clone()),
                        drafts: by_subject
                            .iter()
                            .find(|(k, _)| k == id)
                            .map(|(_, d)| d.clone())
                            .unwrap_or_default(),
                        ..template.clone()
                    })
                    .collect()
            }
            Self::Episodes { cap, template } => parse_episode_drafts(raw, *cap)
                .into_iter()
                .map(|draft| DraftBatch {
                    drafts: vec![draft],
                    ..template.clone()
                })
                .collect(),
        }
    }
}

/// One cheap-LLM call a pass makes (v4 `PassCall`).
#[derive(Clone, Debug)]
pub struct PassCall {
    /// For logs and debug lines (`SELF memory for Amy`).
    pub label: String,
    /// The characters the call's memories are for; a skip for every one of
    /// them skips the call.
    pub holders: Vec<String>,
    pub messages: Vec<CompletionMessage>,
    /// Attribution for the LLM log.
    pub log_character_id: Option<String>,
    pub parse: CallParse,
}

/// One candidate a pass produced, and what admission did (or, in a dry run,
/// would do) with it (v4 `AdmittedCandidate`).
#[derive(Clone, Debug)]
pub struct AdmittedCandidate {
    pub candidate: MemoryCandidate,
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub holder_name: String,
    pub about_name: String,
    pub outcome: AdmissionOutcome,
}

/// v4 `PassResult['usage']`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// v4 `PassResult`.
#[derive(Clone, Debug)]
pub struct PassResult {
    pub pass: ExtractionPassName,
    pub calls: usize,
    /// Calls lost to a cheap-LLM timeout — work that never happened (bug 107).
    pub timed_out: usize,
    /// Calls that failed otherwise (a refusal, an unparseable answer).
    pub failed: usize,
    pub admitted: Vec<AdmittedCandidate>,
    pub created_ids: Vec<String>,
    pub reinforced_ids: Vec<String>,
    pub usage: PassUsage,
    pub debug_logs: Vec<String>,
    /// Set when a `defer_on_rate_limit` run made no call because one of its
    /// calls' holders were all at the hourly cap: the window was not read, and
    /// a fold must not move its cursor past it.
    pub rate_deferred: bool,
}

/// v4 `emptyPassResult(pass)`.
pub fn empty_pass_result(pass: ExtractionPassName) -> PassResult {
    PassResult {
        pass,
        calls: 0,
        timed_out: 0,
        failed: 0,
        admitted: Vec::new(),
        created_ids: Vec::new(),
        reinforced_ids: Vec::new(),
        usage: PassUsage::default(),
        debug_logs: Vec::new(),
        rate_deferred: false,
    }
}

/// Fold `next` into `into` (same pass or not) — v4 `mergePassResult`.
pub fn merge_pass_result(into: &mut PassResult, next: PassResult) {
    into.calls += next.calls;
    into.timed_out += next.timed_out;
    into.failed += next.failed;
    into.admitted.extend(next.admitted);
    into.created_ids.extend(next.created_ids);
    into.reinforced_ids.extend(next.reinforced_ids);
    into.usage.prompt_tokens += next.usage.prompt_tokens;
    into.usage.completion_tokens += next.usage.completion_tokens;
    into.usage.total_tokens += next.usage.total_tokens;
    into.debug_logs.extend(next.debug_logs);
    into.rate_deferred = into.rate_deferred || next.rate_deferred;
}

/// v4 `RunPassOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunPassOptions {
    /// Make no call at all when any call's holders are all at the rate
    /// limit's cap, and report `rate_deferred` (the fold passes: a skipped
    /// call there would otherwise pass for a read slice and its messages
    /// would never be read again). The turn passes leave this off — a capped
    /// holder simply forms nothing from that turn.
    pub defer_on_rate_limit: bool,
}

/// JS `String(number)` for a debug line's interpolated number.
fn js_num(n: f64) -> String {
    crate::pascal::js_value::number_to_string(n)
}

/// v4 `describeLimit(member, rl)`.
fn describe_limit(member: &CastMember, rl: &RateLimitDecision) -> Option<String> {
    match rl {
        RateLimitDecision::Skip { recent_count, cap } => Some(format!(
            "[Memory] SKIPPED extraction for {} — rate limit reached ({}/{} memories in last hour)",
            member.name,
            js_num(*recent_count),
            js_num(*cap)
        )),
        RateLimitDecision::Throttle {
            floor,
            recent_count,
            cap,
        } => Some(format!(
            "[Memory] THROTTLING {} — {}/{}; floor {}",
            member.name,
            js_num(*recent_count),
            js_num(*cap),
            js_num(*floor)
        )),
        RateLimitDecision::Allow => None,
    }
}

/// Run one pass over one window: extract → cap → floor → admit (or decide
/// only, in a dry run) — v4 `runPass`. Never fails for a model failure; a
/// timeout is counted in `timed_out` so the caller can fail its job for a
/// retry.
pub async fn run_pass<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    pass: &mut ExtractionPass,
    ctx: &ExtractionContext,
    window: &TranscriptWindow,
    opts: RunPassOptions,
) -> PassResult
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let pass_name = pass.name();
    let mut result = empty_pass_result(pass_name);
    let calls = pass.calls(ctx, window);

    if opts.defer_on_rate_limit {
        for call in &calls {
            if call.holders.is_empty() {
                continue;
            }
            if call
                .holders
                .iter()
                .all(|h| ctx.rate_limit.decide(h).is_skip())
            {
                result.rate_deferred = true;
                result.debug_logs.push(format!(
                    "[Memory] {} deferred — every holder is at the hourly cap; the window is left unread",
                    call.label
                ));
                tracing::info!(
                    chatId = %ctx.chat_id,
                    pass = pass_name.as_str(),
                    call = %call.label,
                    "[Memory] Pass deferred by the rate limit; window left unread"
                );
                return result;
            }
        }
    }

    for call in &calls {
        // The rate limit gates each holder; a call none of its holders may use
        // is not made.
        let mut limits: HashMap<String, RateLimitDecision> = HashMap::new();
        for holder_id in &call.holders {
            let rl = ctx.rate_limit.decide(holder_id);
            limits.insert(holder_id.clone(), rl);
            if let Some(member) = ctx.cast_get(holder_id) {
                if let Some(line) = describe_limit(member, &rl) {
                    result.debug_logs.push(line);
                }
            }
        }
        if !call.holders.is_empty()
            && call
                .holders
                .iter()
                .all(|h| limits.get(h).is_some_and(RateLimitDecision::is_skip))
        {
            tracing::debug!(
                chatId = %ctx.chat_id,
                pass = pass_name.as_str(),
                call = %call.label,
                "[Memory] Call skipped: every holder is rate-limited"
            );
            continue;
        }

        result.calls += 1;
        let fallback = ctx.route.fallback_options();
        let parse = call.parse.clone();
        let answer = deps
            .executor
            .execute(
                deps.completion,
                &ctx.route.cheap_llm,
                call.messages.clone(),
                move |raw| parse.parse(raw),
                fallback.as_ref(),
                Some(ctx.route.max_tokens),
                call.log_character_id.as_deref(),
                Some(pass.task_type()),
                CheapLlmTaskOptions::default(),
            )
            .await;
        if let Some(usage) = &answer.usage {
            result.usage.prompt_tokens += usage.prompt_tokens;
            result.usage.completion_tokens += usage.completion_tokens;
            result.usage.total_tokens += usage.total_tokens;
        }
        if !answer.success {
            if answer.timed_out {
                result.timed_out += 1;
            } else {
                result.failed += 1;
            }
            // v4 `${answer.error}` — an absent error interpolates `undefined`.
            let error = answer.error.as_deref().unwrap_or("undefined");
            result.debug_logs.push(format!(
                "[Memory] {} {}: {error}",
                call.label,
                if answer.timed_out {
                    "was LOST to a timeout, not refused"
                } else {
                    "failed"
                }
            ));
            tracing::warn!(
                chatId = %ctx.chat_id,
                pass = pass_name.as_str(),
                call = %call.label,
                timedOut = answer.timed_out,
                error = %error,
                "[Memory] Extraction call failed"
            );
            continue;
        }

        for batch in answer.result.unwrap_or_default() {
            let admitted =
                admit_batch(deps, pass, ctx, &batch, &limits, &call.label, &mut result).await;
            if !admitted.is_empty() {
                pass.after_batch(ctx, window, &admitted).await;
            }
        }
    }

    tracing::debug!(
        chatId = %ctx.chat_id,
        pass = pass_name.as_str(),
        dryRun = ctx.dry_run,
        calls = result.calls,
        timedOut = result.timed_out,
        candidates = result.admitted.len(),
        created = result.created_ids.len(),
        reinforced = result.reinforced_ids.len(),
        "[Memory] Pass complete"
    );
    result
}

/// v4 `admitBatch`.
async fn admit_batch<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    pass: &ExtractionPass,
    ctx: &ExtractionContext,
    batch: &DraftBatch,
    limits: &HashMap<String, RateLimitDecision>,
    call_label: &str,
    result: &mut PassResult,
) -> Vec<AdmittedCandidate>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let about = batch.about.as_deref().and_then(|id| ctx.cast_get(id));
    let about_name = about.map(|a| a.name.as_str());
    // v4 `aboutName ? ` about ${aboutName}` : ''` — truthy.
    let about_suffix = about_name
        .filter(|n| !n.is_empty())
        .map(|n| format!(" about {n}"))
        .unwrap_or_default();
    let mut drafts = batch.drafts.clone();
    if let Some(floor) = &batch.floor {
        let floored = apply_importance_floor(&drafts, floor.value);
        if floored.len() < drafts.len() {
            result.debug_logs.push(format!(
                "[Memory] {} dropped {} candidate(s) ({call_label}{about_suffix}) below importance {}",
                floor.reason,
                drafts.len() - floored.len(),
                js_num(floor.value)
            ));
        }
        drafts = floored;
    }

    let mut admitted = Vec::new();
    for holder_id in &batch.holders {
        let Some(holder) = ctx.cast_get(holder_id) else {
            continue;
        };
        let rl = limits
            .get(holder_id)
            .copied()
            .unwrap_or(RateLimitDecision::Allow);
        let kept = match rl {
            RateLimitDecision::Skip { .. } => continue,
            RateLimitDecision::Throttle { floor, .. } => {
                let kept = apply_importance_floor(&drafts, floor);
                if kept.len() < drafts.len() {
                    result.debug_logs.push(format!(
                        "[Memory] Throttle dropped {} candidate(s) for {}{about_suffix} below importance {}",
                        drafts.len() - kept.len(),
                        holder.name,
                        js_num(floor)
                    ));
                }
                kept
            }
            RateLimitDecision::Allow => drafts.clone(),
        };
        let label = match about {
            Some(a) => format!("{call_label} — {} about {}", holder.name, a.name),
            None if batch.holders.len() > 1 => format!("{call_label} — for {}", holder.name),
            None => call_label.to_string(),
        };
        result
            .debug_logs
            .push(format!("[Memory] {label}: {} candidate(s)", kept.len()));
        for draft in &kept {
            if let Some(one) =
                admit_draft(deps, pass, ctx, batch, holder, about, draft, &label, result).await
            {
                admitted.push(one);
            }
        }
    }
    admitted
}

/// v4 `admitDraft` — THE extraction → admission seam.
#[allow(clippy::too_many_arguments)]
async fn admit_draft<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    pass: &ExtractionPass,
    ctx: &ExtractionContext,
    batch: &DraftBatch,
    holder: &CastMember,
    about: Option<&CastMember>,
    draft: &MemoryDraft,
    label: &str,
    result: &mut PassResult,
) -> Option<AdmittedCandidate>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let reference = batch.reference_iso.as_deref();
    let fallback = batch.fallback_iso.as_deref();
    let anchor_time_ms =
        anchor_time_ms_from(&[reference, fallback], crate::clock::now_unix_ms() as f64);
    let anchor_iso = first_valid_iso(&[reference, fallback])
        .unwrap_or_else(|| crate::clock::iso_from_unix_ms(anchor_time_ms as i64));
    let fallback_iso = first_valid_iso(&[fallback]).unwrap_or_else(|| anchor_iso.clone());
    let anchors = resolve_episodic_anchors(&EpisodicAnchorInput {
        when: draft.when.as_deref(),
        reference_iso: Some(&anchor_iso),
        fallback_iso: Some(&fallback_iso),
        timeline_mode: &ctx.clock.timeline_mode,
        narrative_time: draft.narrative_time.as_deref(),
        time_zone: &ctx.clock.time_zone,
    });

    let candidate = candidate_from_draft(
        &draft.to_view(),
        CandidateSubject {
            holder_id: holder.character_id.clone(),
            about_id: about.map(|a| a.character_id.clone()),
        },
        CandidateProvenance {
            source: MemorySource::Auto,
            witnessed_context: Some(if ctx.in_autonomous_room {
                WitnessedContext::AutonomousRoom
            } else {
                WitnessedContext::UserPresent
            }),
            chat_id: Some(ctx.chat_id.clone()),
            project_id: ctx.project_id.clone(),
            source_message_id: batch.source_message_id.clone(),
            anchor_time_ms,
            pass: pass.candidate_pass(),
        },
    );
    let Some(candidate) = candidate else {
        result.debug_logs.push(format!(
            "[Memory] {label} — skipped a draft with no usable text"
        ));
        return None;
    };

    let created_at = first_valid_iso(&[batch.source_created_at.as_deref()]);
    let resolved = ResolvedCandidateTime {
        occurred_at: anchors.occurred_at.clone(),
        narrative_time: anchors.narrative_time.clone(),
        source_message_timestamp: created_at,
    };
    // One candidate's failed admission costs that candidate alone, never the
    // holder's other candidates, the batch's other holders or the rest of the
    // pass.
    let outcome = {
        let mut book_guard = match (&ctx.dry_run_book, ctx.dry_run) {
            (Some(book), true) => Some(book.lock().await),
            _ => None,
        };
        admit_candidate(
            &ctx.db,
            deps.embedding,
            &candidate,
            &resolved,
            AdmitOptions {
                user_id: &ctx.user_id,
                dry_run: ctx.dry_run,
                backfill: ctx.backfill,
                dry_run_book: book_guard.as_deref_mut(),
                key_scope: if ctx.dry_run {
                    None
                } else {
                    ctx.key_scope.as_ref()
                },
                host_zone_fallback: &ctx.host_zone_fallback,
            },
        )
        .await
    };
    let outcome = match outcome {
        Ok(o) => o,
        Err(error) => {
            let message = error_text(&error);
            tracing::warn!(
                chatId = %ctx.chat_id,
                pass = pass.name().as_str(),
                holderId = %holder.character_id,
                error = %message,
                "[Memory] Admission failed for one candidate; continuing"
            );
            result.debug_logs.push(format!(
                "[Memory] {label} — admission failed ({message}): {}",
                candidate.summary
            ));
            return None;
        }
    };

    let memory_id = outcome
        .memory
        .as_ref()
        .and_then(|m| m.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    if admission_inserted(&outcome.action) {
        ctx.rate_limit.record(&holder.character_id, 1.0);
        if let (Some(id), false) = (&memory_id, ctx.dry_run) {
            result.created_ids.push(id.clone());
        }
    } else if let (Some(id), false) = (&memory_id, ctx.dry_run) {
        let reinforced = match &outcome.action {
            AdmissionAction::Reinforce { .. } => true,
            AdmissionAction::PromoteAndReinforce { with_footnotes, .. } => *with_footnotes,
            _ => false,
        };
        // A near-duplicate absorption counts on its row but adds no detail;
        // the debug line reports it.
        if reinforced {
            result.reinforced_ids.push(id.clone());
        }
    }
    result.debug_logs.push(if ctx.dry_run {
        format!(
            "[Memory] DRY-RUN {label} — would {}: {}",
            outcome.action.kind().as_str(),
            candidate.summary
        )
    } else {
        crate::admission::describe::describe_admission(&outcome, label, &candidate.summary)
    });

    let admitted = AdmittedCandidate {
        candidate,
        occurred_at: anchors.occurred_at,
        narrative_time: anchors.narrative_time,
        holder_name: holder.name.clone(),
        about_name: about
            .map(|a| a.name.clone())
            .unwrap_or_else(|| holder.name.clone()),
        outcome,
    };
    result.admitted.push(admitted.clone());
    Some(admitted)
}

/// The candidate provenance a pass carries (v4 `pass.candidatePass`).
pub(crate) fn candidate_pass_of(name: ExtractionPassName) -> CandidatePass {
    match name {
        ExtractionPassName::SelfTurn => CandidatePass::SelfTurn,
        ExtractionPassName::OtherTurn => CandidatePass::OtherTurn,
        ExtractionPassName::OtherFold => CandidatePass::OtherFold,
        ExtractionPassName::EpisodeFold => CandidatePass::EpisodeFold,
    }
}
