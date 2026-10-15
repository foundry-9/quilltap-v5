//! The fold runner — port of v4 `lib/memory/extraction/fold.ts` (`ac6348c5f`;
//! the window rules of bugs 206 / 207 in their PIN shape — the cursor table,
//! never `fold-window.ts`'s `continue:<id>` / watermark columns; pin
//! `83334b9a5`).
//!
//! v4's why, carried: a fold-grain pass reads the messages past its cursor
//! through the end of the window it was handed, in consecutive slices of
//! [`FOLD_MAX_WINDOW_MESSAGES`] (at most [`FOLD_MAX_SLICES_PER_JOB`] a run),
//! and the cursor advances only past a slice that was read — a timed-out or
//! rate-deferred slice stops the run with the cursor before it. Best-effort:
//! it never fails; a throw is the `error` skip. Pre-refactor v5 trimmed the
//! window to its newest 60 and advanced the watermark past ALL (bug 206), and
//! the episode pass had no window logic at all (bug 207).
//!
//! The pass name is the RAW string (L-4): anything but `"other-fold"` runs the
//! EPISODE pass, as v4's `passFor` does, and reads / advances its own cursor.

use serde_json::Value;

use crate::db::fallback::error_text;
use crate::db::DbError;
use crate::memory_extraction::window::{
    fold_pass_range, is_fold_eligible_message, message_id, slice_fold_window, stretch_window,
    TranscriptWindow,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::extraction::context::ExtractionContext;
use crate::services::extraction::passes::ExtractionPass;
use crate::services::extraction::run::{
    empty_pass_result, merge_pass_result, run_pass, ExtractionDeps, ExtractionPassName, PassResult,
    RunPassOptions,
};

/// One prompt reads at most this many eligible messages; a wider window is
/// read in consecutive slices of it.
pub const FOLD_MAX_WINDOW_MESSAGES: usize = 60;

/// One run reads at most this many slices by default; a longer backlog
/// continues from the advanced cursor.
pub const FOLD_MAX_SLICES_PER_JOB: usize = 4;

/// The `other-fold` pass string.
pub const OTHER_FOLD_PASS: &str = "other-fold";
/// The `episode-fold` pass string.
pub const EPISODE_FOLD_PASS: &str = "episode-fold";

/// v4 `FoldResult['skippedReason']` (the fold's seven, and the catch-ups'
/// two), rendering v4's strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldSkipReason {
    ModeTurn,
    EmptyWindow,
    BehindCursor,
    NothingToObserve,
    NobodyPresent,
    RateLimited,
    Error,
    NotIdle,
    NeverFolded,
}

impl FoldSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModeTurn => "mode-turn",
            Self::EmptyWindow => "empty-window",
            Self::BehindCursor => "behind-cursor",
            Self::NothingToObserve => "nothing-to-observe",
            Self::NobodyPresent => "nobody-present",
            Self::RateLimited => "rate-limited",
            Self::Error => "error",
            Self::NotIdle => "not-idle",
            Self::NeverFolded => "never-folded",
        }
    }
}

/// v4 `RunFoldInput`.
#[derive(Clone, Debug, Default)]
pub struct RunFoldInput {
    /// The stretch the fold (or catch-up) hands over: chronological, any rows
    /// (ineligible ones are skipped).
    pub window_messages: Vec<Value>,
    /// The message the cursor may advance to after the last slice. Defaults
    /// to the last window message.
    pub cover_through_message_id: Option<String>,
    /// Slices read this run (default [`FOLD_MAX_SLICES_PER_JOB`]; a dry run
    /// reads them all — `f64::INFINITY`).
    pub max_slices: Option<f64>,
}

/// v4 `FoldResult = PassResult & { skippedReason?, slices, slicesDeferred,
/// cursorAdvancedTo }`. `pass` is the raw pass string the run was asked for.
#[derive(Clone, Debug)]
pub struct FoldResult {
    pub pass: String,
    pub result: PassResult,
    pub skipped_reason: Option<FoldSkipReason>,
    /// Slices read.
    pub slices: usize,
    /// Slices left unread because the run hit `max_slices` (or a deferral /
    /// a throw left them).
    pub slices_deferred: usize,
    /// Where the cursor now stands, when this run moved it.
    pub cursor_advanced_to: Option<String>,
}

/// The pass-result name for a fold pass string (the episode arm for anything
/// but `other-fold`).
pub(crate) fn fold_pass_name(pass: &str) -> ExtractionPassName {
    if pass == OTHER_FOLD_PASS {
        ExtractionPassName::OtherFold
    } else {
        ExtractionPassName::EpisodeFold
    }
}

/// v4 `foldResult(pass, skippedReason?)`.
pub(crate) fn fold_result(pass: &str, skipped: Option<FoldSkipReason>) -> FoldResult {
    FoldResult {
        pass: pass.to_string(),
        result: empty_pass_result(fold_pass_name(pass)),
        skipped_reason: skipped,
        slices: 0,
        slices_deferred: 0,
        cursor_advanced_to: None,
    }
}

/// v4 `passFor(name)` — ONE instance per run, so the episode pass's
/// `writtenEpisodeIds` spans every slice.
fn pass_for(name: &str) -> ExtractionPass {
    if name == OTHER_FOLD_PASS {
        ExtractionPass::OtherFold
    } else {
        ExtractionPass::episode_fold()
    }
}

/// v4 `advance(ctx, pass, messageId, result)` — a failed advance WARNs and
/// leaves `cursor_advanced_to` where it was.
async fn advance(
    ctx: &ExtractionContext,
    pass: &str,
    message: Option<&str>,
    result: &mut FoldResult,
) {
    let Some(message_id) = message.filter(|m| !m.is_empty()) else {
        return;
    };
    match ctx.cursors.advance(&ctx.db, pass, message_id).await {
        Ok(()) => result.cursor_advanced_to = Some(message_id.to_string()),
        Err(error) => tracing::warn!(
            chatId = %ctx.chat_id,
            pass = %pass,
            error = %error_text(&error),
            "[Memory] Failed to advance extraction cursor"
        ),
    }
}

/// Run one fold-grain pass over a window (v4 `runFoldPass`). See the module
/// docs.
pub async fn run_fold_pass<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &ExtractionContext,
    pass_name: &str,
    input: RunFoldInput,
) -> FoldResult
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    // Kept outside the fallible body, so a throw mid-run still reports what
    // earlier slices did — the cursor they advanced, the slices left, the
    // tokens spent.
    let mut result = fold_result(pass_name, None);
    // Slices of this run not yet read; a throw leaves them to a successor job.
    let mut unread = 0usize;
    match run_fold_body(deps, ctx, pass_name, input, &mut result, &mut unread).await {
        Ok(Some(early)) => early,
        Ok(None) => result,
        Err(error) => {
            tracing::warn!(
                chatId = %ctx.chat_id,
                pass = %pass_name,
                slicesRead = result.slices,
                cursorAdvancedTo = %result.cursor_advanced_to.as_deref().unwrap_or("null"),
                error = %error_text(&error),
                "[Memory] Fold pass failed (non-fatal)"
            );
            result.skipped_reason = Some(FoldSkipReason::Error);
            // The slice that threw and those after it are unread; a catch-up
            // job that advanced the cursor first hands them to its successor.
            result.slices_deferred += unread;
            result
        }
    }
}

/// The `try` body of v4 `runFoldPass`. `Ok(Some(r))` is an early `return
/// foldResult(...)` (a FRESH result, as v4's early returns are); `Ok(None)`
/// means `result` is the answer.
async fn run_fold_body<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &ExtractionContext,
    pass_name: &str,
    input: RunFoldInput,
    result: &mut FoldResult,
    unread: &mut usize,
) -> Result<Option<FoldResult>, DbError>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    if pass_name == OTHER_FOLD_PASS && ctx.settings.mode.other_pass == "turn" {
        tracing::debug!(chatId = %ctx.chat_id, "[Memory] otherPass=turn; fold-grain OTHER pass not run");
        return Ok(Some(fold_result(pass_name, Some(FoldSkipReason::ModeTurn))));
    }
    if input.window_messages.is_empty() {
        return Ok(Some(fold_result(
            pass_name,
            Some(FoldSkipReason::EmptyWindow),
        )));
    }

    let cursor = ctx.cursors.read(&ctx.db, pass_name)?;
    let mut window = input.window_messages;
    if let Some(after) = cursor.after_message_id.as_deref().filter(|c| !c.is_empty()) {
        let range = fold_pass_range(&window, ctx.messages()?, Some(after));
        if range.applied {
            tracing::debug!(
                chatId = %ctx.chat_id,
                pass = %pass_name,
                cursor = %after,
                before = window.len(),
                after = range.messages.len(),
                gap = range.gap,
                "[Memory] Fold window cut to what lies past the cursor"
            );
            window = range.messages;
            if window.is_empty() {
                return Ok(Some(fold_result(
                    pass_name,
                    Some(FoldSkipReason::BehindCursor),
                )));
            }
        }
    }

    let cover_through = input
        .cover_through_message_id
        .clone()
        .or_else(|| window.last().map(|m| message_id(m).to_string()));
    let eligible: Vec<Value> = window
        .into_iter()
        .filter(is_fold_eligible_message)
        .collect();
    let all_slices = slice_fold_window(
        &eligible,
        FOLD_MAX_WINDOW_MESSAGES as f64,
        cover_through.as_deref(),
    );
    let max_slices = input.max_slices.unwrap_or(FOLD_MAX_SLICES_PER_JOB as f64);
    let take = if max_slices.is_finite() {
        (max_slices.max(0.0) as usize).min(all_slices.len())
    } else {
        all_slices.len()
    };
    let slices = &all_slices[..take];
    result.slices_deferred = all_slices.len() - slices.len();
    *unread = slices.len();
    if result.slices_deferred > 0 {
        tracing::info!(
            chatId = %ctx.chat_id,
            pass = %pass_name,
            slicesRead = slices.len(),
            slicesDeferred = result.slices_deferred,
            "[Memory] Fold backlog wider than one run; the rest waits for the next fold or catch-up"
        );
    }

    if slices.is_empty() {
        result.skipped_reason = Some(FoldSkipReason::NothingToObserve);
        advance(ctx, pass_name, cover_through.as_deref(), result).await;
        return Ok(None);
    }

    let mut pass = pass_for(pass_name);
    let mut slices_with_no_calls = 0usize;
    for (i, slice) in slices.iter().enumerate() {
        let window = TranscriptWindow::Stretch(stretch_window(
            slice.messages.clone(),
            &ctx.speaker_names,
            slice.cover_through_message_id.clone(),
            crate::clock::now_unix_ms() as f64,
        ));
        let slice_result = run_pass(
            deps,
            &mut pass,
            ctx,
            &window,
            RunPassOptions {
                defer_on_rate_limit: true,
            },
        )
        .await;
        let (deferred, calls, timed_out) = (
            slice_result.rate_deferred,
            slice_result.calls,
            slice_result.timed_out,
        );
        merge_pass_result(&mut result.result, slice_result);
        if deferred {
            // Not read: the cursor stays before this slice, and the next fold
            // or catch-up reads it once the hour has passed.
            result.skipped_reason = Some(FoldSkipReason::RateLimited);
            result.slices_deferred += slices.len() - i;
            *unread = 0;
            tracing::info!(
                chatId = %ctx.chat_id,
                pass = %pass_name,
                slice = i + 1,
                slices = slices.len(),
                "[Memory] Fold slice deferred by the rate limit; cursor not advanced past it"
            );
            break;
        }
        result.slices += 1;
        *unread -= 1;
        if calls == 0 && timed_out == 0 {
            slices_with_no_calls += 1;
        }
        if timed_out > 0 {
            tracing::warn!(
                chatId = %ctx.chat_id,
                pass = %pass_name,
                slice = i + 1,
                slices = slices.len(),
                "[Memory] Fold slice lost to a timeout; cursor not advanced past it"
            );
            break;
        }
        advance(
            ctx,
            pass_name,
            slice.cover_through_message_id.as_deref(),
            result,
        )
        .await;
    }
    if !result.result.rate_deferred
        && slices_with_no_calls == result.slices
        && result.result.timed_out == 0
    {
        result.skipped_reason = Some(if pass_name == EPISODE_FOLD_PASS {
            FoldSkipReason::NobodyPresent
        } else {
            FoldSkipReason::NothingToObserve
        });
    }

    tracing::info!(
        chatId = %ctx.chat_id,
        pass = %pass_name,
        dryRun = ctx.dry_run,
        slices = result.slices,
        calls = result.result.calls,
        candidates = result.result.admitted.len(),
        created = result.result.created_ids.len(),
        reinforced = result.result.reinforced_ids.len(),
        cursorAdvancedTo = %result.cursor_advanced_to.as_deref().unwrap_or("null"),
        "[Memory] Fold pass complete"
    );
    Ok(None)
}
