//! The `MEMORY_EXTRACTION` job handler — port of v4
//! `lib/background-jobs/handlers/memory-extraction.ts` (`ac6348c5f`, memory
//! refactor Phase D; bug 245's `extractAfterMessageId`; pin `83334b9a5`), on
//! the ONE extraction pipeline (`crate::services::extraction`).
//!
//! A payload's grain ([`MemoryExtractionPayload::pass_of`]) picks the branch:
//!  - `turn` (the default) runs the turn passes over the turn the opener /
//!    anchor locate ([`run_turn_extraction`]), past the chat's `turn` cursor;
//!  - any other grain is a fold pass's idle catch-up over the chat's backlog
//!    past that pass's cursor ([`run_fold_catchup`]) — enqueued by the daily
//!    sweep, a regenerate, or a catch-up job that left slices unread (it
//!    enqueues its own continuation, keyed on the cursor it advanced to).
//!
//! ## Failure shape (v4's exactly)
//!
//! A missing connection profile THROWS (`Err` → the job FAILS and the
//! runner's backoff retries); a pass lost to a cheap-LLM timeout throws v4's
//! `CheapLLMTaskLostError` text; a missing chat and a turn with no new line
//! are logged skips (`Ok(())` → COMPLETED). The chat-settings row is
//! OPTIONAL (pre-refactor v5 threw `Chat settings not found for user`).
//!
//! ## Model boundary (tier-3 seams)
//!
//! [`CompletionProvider`] + [`EmbeddingProvider`] via the ported
//! [`CheapLlmTaskExecutor`]; the cost figure behind the MEMORY_EXTRACTION
//! system event rides [`MessageCostEstimator`]. The extraction limits and
//! mode are the context's (the keystone's settings façade) — the host no
//! longer reads them (L-13).

use serde_json::Value;

use crate::db::connection_profiles;
use crate::db::fallback::error_text;
use crate::db::runtime::Db;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::admission::AdmissionKeyScope;
use crate::services::cheap_llm_exec::{cheap_llm_task_lost_message, CheapLlmTaskExecutor};
use crate::services::cost_estimation::MessageCostEstimator;
use crate::services::cost_events::{create_memory_extraction_event, TokenUsage};
use crate::services::extraction::catchup::{run_fold_catchup, RunFoldCatchupOptions};
use crate::services::extraction::context::{
    build_extraction_context, BuildExtractionContextOptions, ExtractionContext,
    ExtractionContextError,
};
use crate::services::extraction::run::{ExtractionDeps, PassResult};
use crate::services::extraction::turn::{run_turn_extraction, TurnLocator};
use crate::services::queue_service::{
    enqueue_memory_extraction_with, MemoryExtractionEnqueue, MemoryExtractionPass,
};

/// A payload's grain (work order P4.D296 L-4): v4's `memoryExtractionPassOf`
/// lets any TRUTHY `pass` through unvalidated, so an unknown one is carried
/// raw — it takes the EPISODE branch (v4 `catchup.ts` tests only
/// `'other-fold'`), labels every line with its own string, and reads /
/// writes its own `<chatId>:<pass>` cursor row (`UNKNOWN_PASS_CURSOR_ROW`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassOf {
    Known(MemoryExtractionPass),
    Unknown(String),
}

impl PassOf {
    /// The pass string the lines, the labels and the cursor carry.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(p) => p.as_str(),
            Self::Unknown(s) => s,
        }
    }

    pub fn is_turn(&self) -> bool {
        matches!(self, Self::Known(MemoryExtractionPass::Turn))
    }
}

/// The `MEMORY_EXTRACTION` job payload (v4 `MemoryExtractionPayload`). The
/// raw JSON is kept: the grain is decided over it exactly as v4's
/// `memoryExtractionPassOf` reads it.
#[derive(Clone, Debug, Default)]
pub struct MemoryExtractionPayload {
    pub raw: Value,
    pub chat_id: String,
    /// `None` for greeting / continue turns with no fresh user input.
    pub turn_opener_message_id: Option<String>,
    /// The autonomous-turn terminal anchor (see the transcript builder).
    pub extraction_anchor_message_id: Option<String>,
    /// Bug 245 — a regenerate's bound for an autonomous room.
    pub extract_after_message_id: Option<String>,
    pub connection_profile_id: String,
    /// Catch-ups: the cursor the job was enqueued at (a dedupe term only).
    pub cursor: Option<String>,
    /// `ignoreIdle === true`.
    pub ignore_idle: bool,
    /// `backfill === true` — a rebuild of history: no rate limit, no ledger.
    pub backfill: bool,
    /// The deprecated pre-Phase-D `foldOtherIgnoreIdle === true`.
    pub legacy_fold_other_ignore_idle: bool,
}

impl MemoryExtractionPayload {
    pub fn decode(payload: &Value) -> Self {
        let s = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
        };
        // v4 `?? null` reads — an empty string is a value, as v4's is.
        let opt = |k: &str| payload.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            raw: payload.clone(),
            chat_id: s("chatId"),
            turn_opener_message_id: opt("turnOpenerMessageId"),
            extraction_anchor_message_id: opt("extractionAnchorMessageId"),
            extract_after_message_id: opt("extractAfterMessageId"),
            connection_profile_id: s("connectionProfileId"),
            cursor: opt("cursor"),
            ignore_idle: payload.get("ignoreIdle") == Some(&Value::Bool(true)),
            backfill: payload.get("backfill") == Some(&Value::Bool(true)),
            legacy_fold_other_ignore_idle: payload.get("foldOtherIgnoreIdle")
                == Some(&Value::Bool(true)),
        }
    }

    /// v4 `memoryExtractionPassOf(payload)` — the keystone's `pass_of`
    /// (truthy `pass` first, then the episode legacy flag BEFORE the other).
    pub fn pass_of(&self) -> PassOf {
        match crate::services::queue_service::pass_of(&self.raw) {
            Ok(p) => PassOf::Known(p),
            Err(raw) => PassOf::Unknown(crate::pascal::js_value::to_js_string(&raw)),
        }
    }
}

/// The debug-log write's merge (L-12): v4 writes `withRecallLines(existing,
/// lines)` — the turn's recall record (`[Recall]` / `[Recall:ring]` lines,
/// written by the finalizer) kept FIRST, this job's lines replacing any
/// earlier extraction's. P4.D291's port (`recall/deliver.ts:195-198`), wired
/// here at the `83334b9a5` unification (§S.5 named handoff (5)). A stored
/// `debugMemoryLogs` that is not an array of strings reads as no record (v4
/// would throw on a non-string entry's `startsWith`).
fn debug_memory_logs_for(existing: Option<&Value>, lines: &[String]) -> Vec<String> {
    let existing: Option<Vec<String>> = existing.and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    });
    crate::services::recall::deliver::with_recall_lines(existing.as_deref(), lines)
}

/// What every extraction job does after its passes (v4
/// `finishExtractionJob`): fail for a retry when a pass was lost to a
/// timeout, keep the debug lines on the message they describe, and book the
/// cheap-LLM tokens on the unit's profile.
#[allow(clippy::too_many_arguments)]
pub async fn finish_extraction_job<K: MessageCostEstimator>(
    cost: &K,
    job_id: &str,
    user_id: &str,
    ctx: &ExtractionContext,
    result: &PassResult,
    task_label: &str,
    debug_message_id: Option<&str>,
) -> Result<(), String> {
    if result.timed_out > 0 {
        tracing::error!(
            jobId = %job_id,
            chatId = %ctx.chat_id,
            task = %task_label,
            passesLostToTimeout = result.timed_out,
            "[MemoryExtraction] Extraction passes lost to a cheap-LLM timeout; failing the job for retry"
        );
        return Err(cheap_llm_task_lost_message(
            task_label,
            Some(&format!("{} pass(es) timed out", result.timed_out)),
        ));
    }

    if let Some(message_id) = debug_message_id.filter(|m| !m.is_empty()) {
        if !result.debug_logs.is_empty() {
            let stored = async {
                let existing = ctx
                    .messages()?
                    .iter()
                    .find(|m| m.get("id").and_then(Value::as_str) == Some(message_id))
                    .and_then(|m| m.get("debugMemoryLogs"))
                    .cloned();
                let updates = serde_json::json!({
                    "debugMemoryLogs": debug_memory_logs_for(existing.as_ref(), &result.debug_logs),
                });
                let (chat_id, message_id) = (ctx.chat_id.clone(), message_id.to_string());
                ctx.db
                    .write(move |w| {
                        w.main()
                            .chat_messages()
                            .update_message(&chat_id, &message_id, &updates)
                    })
                    .await
            }
            .await;
            if let Err(error) = stored {
                tracing::warn!(
                    jobId = %job_id,
                    error = %error_text(&error),
                    "[MemoryExtraction] Failed to store debug logs"
                );
            }
        }
    }

    // v4 `if (promptTokens || completionTokens)`. The estimator and the event
    // writer are infallible seams in v5, so v4's `[MemoryExtraction] Failed to
    // emit token tracking event` WARN has no v5 home (recorded).
    if result.usage.prompt_tokens != 0 || result.usage.completion_tokens != 0 {
        let profile = &ctx.route.connection_profile;
        let field = |k: &str| {
            profile
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let (provider, model) = (field("provider"), field("modelName"));
        let estimated = cost
            .estimate(
                &provider,
                &model,
                result.usage.prompt_tokens,
                result.usage.completion_tokens,
                user_id,
            )
            .await;
        let _ = create_memory_extraction_event(
            &ctx.db,
            &ctx.chat_id,
            Some(TokenUsage {
                prompt_tokens: Some(result.usage.prompt_tokens as f64),
                completion_tokens: Some(result.usage.completion_tokens as f64),
                total_tokens: Some(result.usage.total_tokens as f64),
            }),
            Some(provider),
            Some(model),
            estimated,
        )
        .await;
    }
    Ok(())
}

/// What an extraction job adds to its context (v4 `extractionContextForJob`'s
/// `extra`).
#[derive(Default)]
pub struct JobContextExtra {
    pub extra_character_ids: Vec<String>,
    pub backfill: bool,
}

/// The context for an extraction job, or `None` when the chat is gone
/// (nothing to do) — v4 `extractionContextForJob`. A missing connection
/// profile is a job failure (`Connection profile not found: <id>`); every
/// other context error rethrows. The attempt's idempotency scope (L-16)
/// rides the context into every admission.
pub fn extraction_context_for_job(
    db: &Db,
    job_id: &str,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
    extra: JobContextExtra,
    host_zone_fallback: &str,
) -> Result<Option<ExtractionContext>, String> {
    let pid = connection_profile_id.to_string();
    let profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| error_text(&e))?
        .ok_or_else(|| format!("Connection profile not found: {connection_profile_id}"))?;
    match build_extraction_context(
        db,
        chat_id,
        user_id,
        BuildExtractionContextOptions {
            connection_profile: Some(profile),
            extra_character_ids: extra.extra_character_ids,
            backfill: extra.backfill,
            key_scope: Some(AdmissionKeyScope::new(job_id)),
            host_zone_fallback: host_zone_fallback.to_string(),
            ..Default::default()
        },
    ) {
        Ok(ctx) => Ok(Some(ctx)),
        Err(ExtractionContextError::NoChat(_)) => {
            tracing::warn!(
                jobId = %job_id,
                chatId = %chat_id,
                "[MemoryExtraction] Chat not found at job execution; skipping"
            );
            Ok(None)
        }
        Err(e) => Err(e.message()),
    }
}

/// v4 `handleMemoryExtraction(job)`. `Err(message)` is v4's `throw` (the
/// runner records it as `lastError` and the ported backoff decides
/// retry-vs-dead).
#[allow(clippy::too_many_arguments)]
pub async fn handle_memory_extraction<C, E, K>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    cost: &K,
    job_id: &str,
    user_id: &str,
    payload: &MemoryExtractionPayload,
    host_zone_fallback: &str,
) -> Result<(), String>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
    K: MessageCostEstimator,
{
    let pass = payload.pass_of();
    let Some(ctx) = extraction_context_for_job(
        db,
        job_id,
        user_id,
        &payload.chat_id,
        &payload.connection_profile_id,
        JobContextExtra {
            backfill: payload.backfill,
            ..Default::default()
        },
        host_zone_fallback,
    )?
    else {
        return Ok(());
    };
    let deps = ExtractionDeps {
        completion,
        embedding,
        executor,
    };

    if !pass.is_turn() {
        let ignore_idle = payload.ignore_idle || payload.legacy_fold_other_ignore_idle;
        let result = run_fold_catchup(
            &deps,
            &ctx,
            pass.as_str(),
            RunFoldCatchupOptions {
                ignore_idle,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| error_text(&e))?;
        let skipped = result.skipped_reason.map(|r| r.as_str()).unwrap_or("null");
        let advanced = result.cursor_advanced_to.as_deref().unwrap_or("null");
        tracing::info!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            pass = %pass.as_str(),
            skippedReason = %skipped,
            slices = result.slices,
            created = result.result.created_ids.len(),
            reinforced = result.result.reinforced_ids.len(),
            cursorAdvancedTo = %advanced,
            slicesDeferred = result.slices_deferred,
            "[MemoryExtraction] Fold catch-up processed"
        );
        finish_extraction_job(
            cost,
            job_id,
            user_id,
            &ctx,
            &result.result,
            &format!("{}-catchup", pass.as_str()),
            None,
        )
        .await?;
        // The rest of a long backlog is the next job's, keyed on the cursor
        // this one advanced to so successive steps never dedupe into each
        // other. A slice deferred by the rate limit waits for the next fold or
        // sweep, not a successor that would meet the same cap at once.
        if let Some(from) = result.cursor_advanced_to.as_deref() {
            if result.slices_deferred > 0
                && result.skipped_reason
                    != Some(crate::services::extraction::fold::FoldSkipReason::RateLimited)
            {
                enqueue_memory_extraction_with(
                    db,
                    user_id,
                    &MemoryExtractionEnqueue {
                        chat_id: &payload.chat_id,
                        turn_opener_message_id: None,
                        extraction_anchor_message_id: None,
                        connection_profile_id: &payload.connection_profile_id,
                        pass: Some(pass.as_str()),
                        cursor: Some(from),
                        ignore_idle,
                        backfill: payload.backfill,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| error_text(&e))?;
                tracing::debug!(
                    jobId = %job_id,
                    chatId = %payload.chat_id,
                    pass = %pass.as_str(),
                    from = %from,
                    slicesDeferred = result.slices_deferred,
                    "[MemoryExtraction] Fold catch-up continues in a follow-up job"
                );
            }
        }
        return Ok(());
    }

    let result = run_turn_extraction(
        &deps,
        &ctx,
        &TurnLocator {
            turn_opener_message_id: payload.turn_opener_message_id.clone(),
            extraction_anchor_message_id: payload.extraction_anchor_message_id.clone(),
            extract_after_message_id: payload.extract_after_message_id.clone(),
        },
    )
    .await
    .map_err(|e| error_text(&e))?;
    let opener = payload.turn_opener_message_id.as_deref().unwrap_or("null");
    if !result.had_contributions {
        tracing::info!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            turnOpenerMessageId = %opener,
            "[MemoryExtraction] Turn has no character contributions; skipping"
        );
        return Ok(());
    }
    let anchor = payload
        .extraction_anchor_message_id
        .as_deref()
        .unwrap_or("null");
    tracing::info!(
        jobId = %job_id,
        chatId = %payload.chat_id,
        turnOpenerMessageId = %opener,
        extractionAnchorMessageId = %anchor,
        candidates = result.result.admitted.len(),
        created = result.result.created_ids.len(),
        reinforced = result.result.reinforced_ids.len(),
        "[MemoryExtraction] Turn processed"
    );
    // The debug lines go on the turn's latest assistant message, where the
    // debug panel shows what the turn's extraction did.
    finish_extraction_job(
        cost,
        job_id,
        user_id,
        &ctx,
        &result.result,
        "memory-extraction",
        result.source_message_id.as_deref(),
    )
    .await
}
