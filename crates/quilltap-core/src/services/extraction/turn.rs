//! The turn grain — port of v4 `lib/memory/extraction/turn.ts` (`ac6348c5f`;
//! bug 245's `turn` cursor from `64743ee13`; pin `83334b9a5`).
//!
//! v4's why, carried: a live turn job reads past the chat's `turn` cursor —
//! a continue, a nudge or a Skip re-closes the SAME turn (same opener), and
//! before bug 245 every re-close re-extracted the whole turn. Lines an
//! earlier extraction covered ride along as context ("EARLIER IN THIS
//! TURN"); the cursor moves past this turn's lines once its passes have run,
//! forward-only (a regenerate's turns and an autonomous room's speakers
//! commit out of order).

use serde_json::Value;

use crate::db::DbError;
use crate::memory_candidate::CandidatePass;
use crate::memory_extraction::window::{message_id, turn_window};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::extraction::context::ExtractionContext;
use crate::services::extraction::cursors::TURN_CURSOR_PASS;
use crate::services::extraction::passes::ExtractionPass;
use crate::services::extraction::run::{
    empty_pass_result, merge_pass_result, run_pass, ExtractionDeps, ExtractionPassName, PassResult,
    RunPassOptions,
};
use crate::services::turn_transcript::{
    build_turn_transcript, BuildTurnTranscriptOptions, TurnCharacterSlice, TurnTranscript,
};

/// v4 `TurnLocator`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnLocator {
    pub turn_opener_message_id: Option<String>,
    pub extraction_anchor_message_id: Option<String>,
    /// Lines at or before this message are context, not material (bug 245).
    /// When absent, a live job takes the chat's `turn` cursor; a dry run and
    /// a backfill read the whole turn.
    pub extract_after_message_id: Option<String>,
}

/// v4 `TurnExtractionResult = PassResult & { sourceMessageId,
/// hadContributions }`.
#[derive(Clone, Debug)]
pub struct TurnExtractionResult {
    pub result: PassResult,
    /// The message the turn's debug log lines are kept on (its latest
    /// assistant message).
    pub source_message_id: Option<String>,
    /// False when the turn held no character contribution.
    pub had_contributions: bool,
}

/// The transcript of one turn, built against the context's cast (v4
/// `turnTranscriptFor`).
pub fn turn_transcript_for(
    ctx: &ExtractionContext,
    locator: &TurnLocator,
) -> Result<TurnTranscript, DbError> {
    let messages = ctx.messages()?;
    let participants: Vec<Value> = ctx
        .chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let cast_characters: std::collections::HashMap<String, Value> = ctx
        .cast
        .iter()
        .map(|m| (m.character_id.clone(), m.character.clone()))
        .collect();
    let user = ctx.user_character();
    Ok(build_turn_transcript(
        messages,
        &participants,
        &cast_characters,
        &BuildTurnTranscriptOptions {
            turn_opener_message_id: locator.turn_opener_message_id.clone(),
            extraction_anchor_message_id: locator.extraction_anchor_message_id.clone(),
            extract_after_message_id: locator.extract_after_message_id.clone(),
            user_character_id: user.map(|u| u.character_id.clone()),
            user_character_name: user.map(|u| u.name.clone()),
            user_character_pronouns: user.and_then(|u| u.pronouns.clone()),
        },
    ))
}

/// Run the turn passes over a built transcript: SELF for every slice, then
/// OTHER (by mode) — v4 `runTurnPasses`.
pub async fn run_turn_passes<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &ExtractionContext,
    transcript: TurnTranscript,
    candidate_pass: CandidatePass,
) -> TurnExtractionResult
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let mut out = TurnExtractionResult {
        result: empty_pass_result(ExtractionPassName::SelfTurn),
        source_message_id: transcript.latest_assistant_message_id.clone(),
        had_contributions: !transcript.character_slices.is_empty(),
    };
    if !out.had_contributions {
        out.result
            .debug_logs
            .push("[Memory] Turn has no character contributions — nothing to extract".to_string());
        return out;
    }
    let turn_opener = transcript.turn_opener_message_id.clone();
    let window = turn_window(transcript, crate::clock::now_unix_ms() as f64);
    let mut self_pass = ExtractionPass::SelfTurn { candidate_pass };
    let r = run_pass(
        deps,
        &mut self_pass,
        ctx,
        &window,
        RunPassOptions::default(),
    )
    .await;
    merge_pass_result(&mut out.result, r);
    if ctx.settings.mode.other_pass == "fold" {
        out.result.debug_logs.push(
            "[Memory] OTHER pass runs at fold grain only (otherPass=fold); skipping per-turn OTHER"
                .to_string(),
        );
    } else {
        let mut other = ExtractionPass::OtherTurn;
        let r = run_pass(deps, &mut other, ctx, &window, RunPassOptions::default()).await;
        merge_pass_result(&mut out.result, r);
    }
    let opener = turn_opener.as_deref().unwrap_or("null");
    tracing::debug!(
        chatId = %ctx.chat_id,
        turnOpenerMessageId = %opener,
        dryRun = ctx.dry_run,
        candidates = out.result.admitted.len(),
        created = out.result.created_ids.len(),
        reinforced = out.result.reinforced_ids.len(),
        timedOut = out.result.timed_out,
        "[Memory] Turn extraction complete"
    );
    out
}

/// Where a live turn job starts reading: the chat's `turn` cursor (v4
/// `liveTurnBound`). A dry run and a backfill (regenerate) read each turn
/// whole and never consult it — a regenerate's turns run in parallel, and a
/// later turn's cursor must not hide an earlier one's lines. The bound is
/// ignored anyway unless it falls inside the turn, so a stale or foreign
/// cursor is harmless.
fn live_turn_bound(ctx: &ExtractionContext) -> Result<Option<String>, DbError> {
    if ctx.dry_run || ctx.backfill {
        return Ok(None);
    }
    Ok(ctx
        .cursors
        .read(&ctx.db, TURN_CURSOR_PASS)?
        .after_message_id)
}

/// Move the `turn` cursor to the turn's latest line, never backwards (v4
/// `advanceTurnCursor`). A dry run moves nothing; a BACKFILL advances.
async fn advance_turn_cursor(
    ctx: &ExtractionContext,
    message_id_to: Option<&str>,
) -> Result<(), DbError> {
    let Some(target) = message_id_to.filter(|m| !m.is_empty()) else {
        return Ok(());
    };
    if ctx.dry_run {
        return Ok(());
    }
    let current = ctx
        .cursors
        .read(&ctx.db, TURN_CURSOR_PASS)?
        .after_message_id;
    if current.as_deref() == Some(target) {
        return Ok(());
    }
    if let Some(current) = current.as_deref().filter(|c| !c.is_empty()) {
        let messages = ctx.messages()?;
        // `findIndex` — a missing id is -1.
        let at = |id: &str| -> i64 {
            messages
                .iter()
                .position(|m| message_id(m) == id)
                .map(|i| i as i64)
                .unwrap_or(-1)
        };
        if at(current) > at(target) {
            return Ok(());
        }
    }
    ctx.cursors.advance(&ctx.db, TURN_CURSOR_PASS, target).await
}

/// One closed turn, located in the chat (v4 `runTurnExtraction`). Lines an
/// earlier extraction covered (bug 245) ride along as context; the cursor
/// moves past this turn's lines once its passes have run — ALWAYS (also
/// after a no-contribution turn, also for a backfill). A cursor read or
/// advance failure propagates (v4's repository reads throw).
pub async fn run_turn_extraction<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &ExtractionContext,
    locator: &TurnLocator,
) -> Result<TurnExtractionResult, DbError>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let extract_after = match locator.extract_after_message_id.clone() {
        Some(b) => Some(b),
        None => live_turn_bound(ctx)?,
    };
    let transcript = turn_transcript_for(
        ctx,
        &TurnLocator {
            extract_after_message_id: extract_after.clone(),
            ..locator.clone()
        },
    )?;
    if let (Some(after), Some(prior)) = (
        extract_after.as_deref().filter(|a| !a.is_empty()),
        transcript.prior_slices.as_ref().filter(|p| !p.is_empty()),
    ) {
        let opener = transcript
            .turn_opener_message_id
            .as_deref()
            .unwrap_or("null");
        tracing::debug!(
            chatId = %ctx.chat_id,
            turnOpenerMessageId = %opener,
            extractAfterMessageId = %after,
            priorSpeakers = prior.len(),
            newSpeakers = transcript.character_slices.len(),
            "[Memory] Turn read past lines already extracted"
        );
    }
    let latest = transcript.latest_assistant_message_id.clone();
    let result = run_turn_passes(deps, ctx, transcript, CandidatePass::SelfTurn).await;
    advance_turn_cursor(ctx, latest.as_deref()).await?;
    Ok(result)
}

/// The one-slice transcript of a Carina answer (v4 `carinaTranscript`): the
/// question opens it, the answer is the answerer's only contribution. `None`
/// when the answer is empty or the answerer is not in the context's cast.
pub fn carina_transcript(
    ctx: &ExtractionContext,
    message: &Value,
    answerer_id: &str,
) -> Option<TurnTranscript> {
    let answerer = ctx.cast_get(answerer_id)?;
    let answer = message
        .get("content")
        .and_then(Value::as_str)
        .map(crate::jsstr::js_trim)
        .unwrap_or_default();
    if answer.is_empty() {
        return None;
    }
    let question = message
        .get("carinaMeta")
        .and_then(|c| c.get("question"))
        .and_then(Value::as_str)
        .map(crate::jsstr::js_trim)
        .unwrap_or_default();
    let created_at = message
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(TurnTranscript {
        turn_opener_message_id: None,
        user_message: (!question.is_empty()).then(|| question.to_string()),
        user_character_id: None,
        user_character_name: None,
        user_character_pronouns: None,
        character_slices: vec![TurnCharacterSlice {
            character_id: answerer.character_id.clone(),
            character_name: answerer.name.clone(),
            character_pronouns: answerer.pronouns.clone(),
            text: answer.to_string(),
            contributing_message_ids: vec![message_id(message).to_string()],
            is_user_controlled: false,
            last_message_created_at: created_at.clone(),
        }],
        prior_slices: None,
        latest_assistant_message_id: Some(message_id(message).to_string()),
        turn_timestamp: created_at,
    })
}

/// A Carina answer's memories: SELF only, with the `carina` provenance (v4
/// `runCarinaExtraction`). The context's user character is set aside, so the
/// answerer forms nothing about the asker — with the answerer the only
/// speaker and no user character, the OTHER pass has no subject.
pub async fn run_carina_extraction<C, E>(
    deps: &ExtractionDeps<'_, C, E>,
    ctx: &mut ExtractionContext,
    message: &Value,
    answerer_id: &str,
) -> Option<TurnExtractionResult>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let transcript = carina_transcript(ctx, message, answerer_id)?;
    // v4 `{ ...ctx, userCharacter: null }` — a copy; restored after.
    let saved = ctx.user_character_id.take();
    let result = run_turn_passes(deps, ctx, transcript, CandidatePass::Carina).await;
    ctx.user_character_id = saved;
    Some(result)
}
