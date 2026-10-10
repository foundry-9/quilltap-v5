//! Fold-grain OTHER pass (memory consolidation, workstream A) — v4
//! `lib/memory/fold-other-pass.ts` (`d58548051`, pin `01a83539d`).
//!
//! A single turn shows one beat of a thread, so the per-turn OTHER pass emits
//! fragments of it and the gate cannot fold paraphrases back together. This
//! pass reads a *stretch* of conversation instead — the window the context
//! summary has just folded, or the uncovered tail of an idle chat — and asks,
//! per (observer, subject) pair, for at most `foldCandidatesPerSubject`
//! candidates, each thread stated once.
//!
//! **The floor needs the fold.** In `hybrid` (the default
//! `memoryExtractionMode`) the per-turn OTHER pass drops every candidate below
//! `perTurnOtherFloor` (0.75) — commitments, agreements and new standing facts
//! stay per-turn; the rest is "left to the fold-grain pass". Nothing else ever
//! re-reads those turns, so this pass, its fold hook and its idle catch-up are
//! what keep the floor from losing observations outright (P4.D270 R-A: they
//! land in the same commit as the floor).
//!
//! Candidates reach the store through the very writer the per-turn pass uses
//! ([`write_candidate`] → `create_memory_with_gate`) — one write path serves
//! both grains, so about-character resolution, witnessedContext and
//! occurredAt stamping cannot drift between them. Speaker labels come from
//! the shared resolver ([`crate::services::speaker_names`]), never from
//! message roles. The Concierge policy and uncensored fallback are inherited
//! exactly as the per-turn pass inherits them.
//!
//! Mode (`memoryExtractionMode.otherPass`): runs in `fold` and `hybrid`,
//! never in `turn`.
//!
//! **Watermark:** `chats.otherExtractionWatermarkMessageId` is advanced to the
//! last message covered after every pass that was not lost to a timeout,
//! through the ONE column writer
//! (`ChatsRepository::set_other_extraction_watermark`). Messages at or before
//! it are never re-read, which is also what keeps a fold that re-walks old
//! turns (a summary rebuild) from re-paying for them. The daily maintenance
//! sweep uses the same watermark to catch up short or idle chats that never
//! reach a fold ([`find_fold_other_catchup_candidates`]).
//!
//! Best-effort throughout: [`run_fold_other_pass`] never fails into the fold.
//!
//! ## What it says, and the two v4 lines it deliberately does not (R-G)
//!
//! Every `[FoldOtherPass]` / `[FoldOtherCatchup]` line is v4's, at v4's level
//! and bytes, fields camelCase in v4's order, under this module's default
//! target, proven against v4's own logger output by
//! `fold_other_pass_tier3_equivalence`. The watermark write runs inside a
//! `Db::write` closure; its two lines are logged on the CALLER thread after
//! the write returns (memory note `capture-rig-writer-thread`).
//!
//! - `[FoldOtherPass] Fold-grain OTHER pass failed (non-fatal)` (WARN) IS
//!   ported — it is REACHABLE: a gate write that fails (`create_memory_with_
//!   gate` → `Err`) is not swallowed by the writer, so it lands here with the
//!   partially-filled result, `skippedReason: 'error'`, no watermark advance
//!   and no completion INFO. The differential plants one.
//! - `[Context Summary] Fold OTHER pass failed:` (ERROR, `context-summary.ts:
//!   593`) is NOT ported — it cannot fire. The only throwing candidates inside
//!   its `try` are `repos.chatSettings.findByUserId` (a `safeQuery` with a
//!   `null` fallback), `resolveConciergeSettings` (pure) and `runFoldOtherPass`
//!   (whose whole body is try/caught). v5's hook therefore has no catch site.
//! - `repos.chats.getMessages` never throws either (a FALLBACK `safeQuery` —
//!   `Failed to get messages for chat` + `[]`), so v5 reads it through the
//!   fallback [`chats_messages_read::get_messages`].

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, UncensoredFallbackOptions,
};
use crate::context_budget::resolve_max_tokens;
use crate::db::fallback::error_text;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, instance_settings, DbError};
use crate::jsstr::js_trim;
use crate::memory_format::Pronouns;
use crate::memory_tasks::{ExtractionClock, FoldOtherMessage, OrientingContext, OtherSubjectInput};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::dangerous_content::resolver::resolve_concierge_settings;
use crate::services::image_job_common::cheap_llm_profile_from_value;
use crate::services::memory_extraction_job::limits_from_value;
use crate::services::memory_processor::{
    apply_importance_floor, extract_other_memories_from_fold,
    load_canon_for_observer_about_subject, resolve_extraction_rate_limit, write_candidate,
    CandidateWriteContext, CanonSubject, CheapLlmSettings, ExtractedCandidate, RateLimitDecision,
    RunState, UsageTotals, WriteArgs,
};
use crate::services::speaker_names::{resolve_speaker_names, speaker_label};
use crate::services::turn_transcript::{
    pronouns_from_character, resolve_user_character_participant,
};

/// A fold window can be a whole rebuilt history; the prompt reads at most the
/// newest this many messages.
pub const FOLD_OTHER_MAX_WINDOW_MESSAGES: usize = 60;

/// A chat must have been quiet this long before the sweep treats its tail as
/// uncovered.
pub const FOLD_OTHER_CATCHUP_IDLE_MS: i64 = 2 * 60 * 60 * 1000;

/// Chats whose last message is older than this are not swept (an old backlog
/// is a backfill, not a catch-up).
pub const FOLD_OTHER_CATCHUP_LOOKBACK_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Most catch-up jobs one sweep will enqueue; the rest wait for tomorrow.
pub const FOLD_OTHER_CATCHUP_MAX_CHATS: usize = 50;

/// v4 `RunFoldOtherPassInput` (the selection and the uncensored fallback ride
/// [`run_fold_other_pass`]'s own parameters).
#[derive(Clone, Debug, Default)]
pub struct RunFoldOtherPassInput {
    pub chat_id: String,
    pub user_id: String,
    /// The covered window: chronological RAW message rows; may contain
    /// system / tool rows (they are filtered, but a trailing one is still the
    /// default watermark target).
    pub window_messages: Vec<Value>,
    /// The profile whose token budget bounds the call.
    pub cheap_max_tokens: Option<i64>,
    /// `'realtime'` | `'narrative'`.
    pub timeline_mode: String,
    pub project_id: Option<String>,
    pub in_autonomous_room: bool,
    /// Background only — never a source of memories.
    pub project_description: Option<String>,
    pub chat_context_summary: Option<String>,
    /// The message the watermark advances to. Defaults to the last window
    /// message.
    pub cover_through_message_id: Option<String>,
    /// Run every call but write nothing (v4 `dryRun` — no production caller
    /// sets it).
    pub dry_run: bool,
}

/// v4 `FoldOtherPassResult`.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldOtherPassResult {
    /// Set when the pass did nothing on purpose (`mode-turn`, `no-chat`,
    /// `behind-watermark`, `nothing-to-observe`, `not-idle`) or failed
    /// (`error`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<&'static str>,
    pub observers: usize,
    pub memories_written: usize,
    pub memories_reinforced: usize,
    pub candidates_proposed: usize,
    /// Passes that never happened because the cheap LLM timed out. The caller
    /// may fail a job on this.
    pub passes_lost_to_timeout: usize,
    pub watermark_advanced_to: Option<String>,
    pub usage: UsageTotals,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_candidates: Option<Vec<ExtractedCandidate>>,
}

fn empty_result(skipped_reason: Option<&'static str>) -> FoldOtherPassResult {
    FoldOtherPassResult {
        skipped_reason,
        ..Default::default()
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// JS truthiness of a JSON value (`if (m.systemSender)`).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// v4 `isFoldOtherEligibleMessage`: a message that carries conversation — a
/// spoken USER/ASSISTANT line from a seat, not staff or tool output.
pub fn is_fold_other_eligible_message(m: &Value) -> bool {
    if str_field(m, "type") != Some("message") {
        return false;
    }
    if !matches!(str_field(m, "role"), Some("USER") | Some("ASSISTANT")) {
        return false;
    }
    if truthy(m.get("systemSender")) {
        return false;
    }
    if !truthy(m.get("participantId")) {
        return false;
    }
    matches!(m.get("content"), Some(Value::String(s)) if !js_trim(s).is_empty())
}

/// v4 `messagesPastWatermark`: the messages strictly after the watermark. A
/// watermark that names a message no longer in the chat (deleted) is treated
/// as absent — better to re-read the tail once than to go blind forever.
pub fn messages_past_watermark<'a>(all: &'a [Value], watermark: Option<&str>) -> &'a [Value] {
    let Some(watermark) = watermark.filter(|w| !w.is_empty()) else {
        return all;
    };
    match all
        .iter()
        .position(|m| str_field(m, "id") == Some(watermark))
    {
        Some(idx) => &all[idx + 1..],
        None => all,
    }
}

/// One seat the pass observes or treats as a subject (v4's inline `Seat`).
struct Seat {
    id: String,
    name: String,
    pronouns: Option<Pronouns>,
    is_user: bool,
}

/// The chat's message rows of `type === 'message'` (v4
/// `getMessages(chatId).filter(m => m.type === 'message')`).
fn chat_message_events(db: &Db, chat_id: &str) -> Result<Vec<Value>, DbError> {
    let all = db.read_main(|conn| chats_messages_read::get_messages(conn, chat_id))?;
    Ok(all
        .into_iter()
        .filter(|m| str_field(m, "type") == Some("message"))
        .collect())
}

/// Run the fold-grain OTHER pass over a window (v4 `runFoldOtherPass`). See the
/// module docs. Never fails — v4's whole body sits in one `try`, whose catch
/// is the `Fold-grain OTHER pass failed (non-fatal)` WARN.
pub async fn run_fold_other_pass<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
    input: &RunFoldOtherPassInput,
) -> FoldOtherPassResult {
    let mut result = empty_result(None);
    match run_pass_body(
        db,
        completion,
        embedding,
        executor,
        selection,
        uncensored_fallback,
        input,
        &mut result,
    )
    .await
    {
        Ok(Some(done)) => done,
        Ok(None) => result,
        Err(error) => {
            tracing::warn!(
                chatId = %input.chat_id,
                error = %error_text(&error),
                "[FoldOtherPass] Fold-grain OTHER pass failed (non-fatal)"
            );
            result.skipped_reason = Some("error");
            result
        }
    }
}

/// v4's `try` body. `Ok(Some(r))` is an early `return emptyResult(…)` (a
/// fresh result, as v4 builds one); `Ok(None)` returns the shared `result`.
#[allow(clippy::too_many_arguments)]
async fn run_pass_body<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
    input: &RunFoldOtherPassInput,
    result: &mut FoldOtherPassResult,
) -> Result<Option<FoldOtherPassResult>, DbError> {
    let chat_id = input.chat_id.as_str();
    let mode = db.read_main(|c| Ok(instance_settings::get_memory_extraction_mode_settings(c)))?;
    if mode.other_pass == "turn" {
        tracing::debug!(chatId = %chat_id, "[FoldOtherPass] otherPass=turn; fold-grain pass not run");
        return Ok(Some(empty_result(Some("mode-turn"))));
    }
    let per_subject_cap = mode.fold_candidates_per_subject.max(1);

    // v4 `repos.chats.findById` is `_findById`, a FALLBACK read (`null` on a
    // failed query, with its ERROR).
    let Some(chat) = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|conn| chats_read::find_by_id(conn, chat_id))
    }) else {
        return Ok(Some(empty_result(Some("no-chat"))));
    };

    // Drop what the watermark already covers. The fold may re-walk early turns
    // (a summary rebuild); the watermark is what stops us paying twice.
    let mut window: Vec<&Value> = input.window_messages.iter().collect();
    let watermark = str_field(&chat, "otherExtractionWatermarkMessageId").filter(|w| !w.is_empty());
    if let (Some(watermark), false) = (watermark, window.is_empty()) {
        let all = chat_message_events(db, chat_id)?;
        if let Some(watermark_idx) = all
            .iter()
            .position(|m| str_field(m, "id") == Some(watermark))
        {
            let order: HashMap<&str, usize> = all
                .iter()
                .enumerate()
                .filter_map(|(i, m)| str_field(m, "id").map(|id| (id, i)))
                .collect();
            let before = window.len();
            // `order.get(m.id) ?? Infinity` — an id absent from the chat is KEPT.
            window.retain(|m| {
                str_field(m, "id")
                    .and_then(|id| order.get(id))
                    .is_none_or(|i| *i > watermark_idx)
            });
            tracing::debug!(
                chatId = %chat_id,
                watermark = %watermark,
                before = before,
                after = window.len(),
                "[FoldOtherPass] Applied watermark to window"
            );
            if window.is_empty() {
                return Ok(Some(empty_result(Some("behind-watermark"))));
            }
        }
    }

    // The watermark target is the last row of the watermark-filtered window,
    // eligible or not (a trailing tool/system row is still covered).
    let cover_through: Option<String> = input.cover_through_message_id.clone().or_else(|| {
        window
            .last()
            .and_then(|m| str_field(m, "id"))
            .map(str::to_string)
    });
    let eligible: Vec<&Value> = window
        .iter()
        .copied()
        .filter(|m| is_fold_other_eligible_message(m))
        .collect();
    let trimmed: Vec<&Value> = eligible[eligible
        .len()
        .saturating_sub(FOLD_OTHER_MAX_WINDOW_MESSAGES)..]
        .to_vec();
    if trimmed.len() < eligible.len() {
        tracing::debug!(
            chatId = %chat_id,
            eligible = eligible.len(),
            kept = trimmed.len(),
            "[FoldOtherPass] Window trimmed to newest messages"
        );
    }

    // Seats and characters.
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let participants_by_id: HashMap<&str, &Value> = participants
        .iter()
        .filter_map(|p| str_field(p, "id").map(|id| (id, p)))
        .collect();
    let mut participant_characters: HashMap<String, Value> = HashMap::new();
    for p in &participants {
        if str_field(p, "type") != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = str_field(p, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        let read = db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, character_id))
        });
        match read {
            Ok(Some(character)) => {
                participant_characters.insert(character_id.to_string(), character);
            }
            Ok(None) => {}
            Err(error) => tracing::debug!(
                chatId = %chat_id,
                characterId = %character_id,
                error = %error_text(&error),
                "[FoldOtherPass] Character unavailable; leaving it out"
            ),
        }
    }
    let user_character = resolve_user_character_participant(&participants, &participant_characters);

    // Observers: characters who spoke in the window. Subjects: those plus the
    // user-controlled character even if silent — as in the per-turn pass.
    let mut observers: Vec<Seat> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut last_message_by_character: HashMap<String, &Value> = HashMap::new();
    for m in &trimmed {
        let Some(seat) = str_field(m, "participantId").and_then(|p| participants_by_id.get(p))
        else {
            continue;
        };
        if str_field(seat, "type") != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = str_field(seat, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        last_message_by_character.insert(character_id.to_string(), m);
        if seen.contains(character_id) {
            continue;
        }
        let Some(character) = participant_characters.get(character_id) else {
            continue;
        };
        seen.insert(character_id.to_string());
        observers.push(Seat {
            id: character_id.to_string(),
            name: str_field(character, "name").unwrap_or_default().to_string(),
            pronouns: pronouns_from_character(character),
            is_user: str_field(seat, "controlledBy") == Some("user"),
        });
    }
    let mut subjects: Vec<Seat> = observers
        .iter()
        .map(|o| Seat {
            id: o.id.clone(),
            name: o.name.clone(),
            pronouns: o.pronouns.clone(),
            is_user: o.is_user,
        })
        .collect();
    if let Some(user_character) = user_character {
        let id = str_field(user_character, "id").unwrap_or_default();
        if !seen.contains(id) {
            subjects.push(Seat {
                id: id.to_string(),
                name: str_field(user_character, "name")
                    .unwrap_or_default()
                    .to_string(),
                pronouns: pronouns_from_character(user_character),
                is_user: true,
            });
        }
    }

    result.observers = observers.len();
    tracing::debug!(
        chatId = %chat_id,
        mode = %mode.other_pass,
        perSubjectCap = per_subject_cap,
        windowMessages = trimmed.len(),
        observers = observers.len(),
        subjects = subjects.len(),
        "[FoldOtherPass] Pass set up"
    );

    if trimmed.is_empty() || observers.is_empty() || subjects.len() < 2 {
        result.skipped_reason = Some("nothing-to-observe");
        advance_watermark(db, chat_id, cover_through.as_deref(), input.dry_run, result).await;
        return Ok(None);
    }

    let speaker_names = db
        .read_main(|conn| Ok(resolve_speaker_names(conn, &participants)))
        .unwrap_or_default();
    let rendered: Vec<FoldOtherMessage> = trimmed
        .iter()
        .map(|m| FoldOtherMessage {
            speaker: speaker_label(
                str_field(m, "participantId"),
                str_field(m, "role").unwrap_or_default(),
                &speaker_names,
            ),
            content: str_field(m, "content").unwrap_or_default().to_string(),
            created_at: str_field(m, "createdAt").map(str::to_string),
        })
        .collect();
    let last_stamped = trimmed
        .iter()
        .rev()
        .find_map(|m| str_field(m, "createdAt").filter(|s| !s.is_empty()));
    let clock = ExtractionClock {
        now_iso: last_stamped
            .map(str::to_string)
            .unwrap_or_else(crate::clock::now_iso),
        timeline_mode: input.timeline_mode.clone(),
        // v4 builds the fold clock with only `nowIso` + `timelineMode`.
        narrative_now: None,
    };
    let orienting = OrientingContext {
        project_description: input.project_description.clone(),
        chat_context_summary: input.chat_context_summary.clone(),
    };
    let limits = limits_from_value(&db.read_main(instance_settings::get_memory_extraction_limits)?);
    let mut state = RunState::default();
    // The per-turn debug-log lines the shared writer appends are DISCARDED
    // here (v4 never persists the fold pass's `debugLogs`).
    let write_ctx = CandidateWriteContext {
        chat_id,
        user_id: &input.user_id,
        project_id: input.project_id.as_deref(),
        source_message_timestamp: None,
        timeline_mode: Some(input.timeline_mode.as_str()),
        dry_run: input.dry_run,
        in_autonomous_room: input.in_autonomous_room,
        turn_timestamp: Some(clock.now_iso.as_str()),
    };

    for observer in &observers {
        let rl = resolve_extraction_rate_limit(db, &observer.id, Some(&limits));
        if let RateLimitDecision::Skip { recent_count, cap } = rl {
            tracing::debug!(
                chatId = %chat_id,
                observer = %observer.name,
                recentCount = recent_count,
                cap = cap,
                "[FoldOtherPass] Observer rate-limited; skipping"
            );
            continue;
        }

        let observer_mount = participant_characters
            .get(&observer.id)
            .and_then(|c| str_field(c, "characterDocumentMountPointId"));
        struct Resolved {
            input: OtherSubjectInput,
            canon_source: &'static str,
        }
        let mut resolved: Vec<Resolved> = Vec::new();
        for subject in subjects.iter().filter(|s| s.id != observer.id) {
            let subject_character = participant_characters.get(&subject.id);
            let canon = load_canon_for_observer_about_subject(
                db,
                observer_mount,
                &observer.id,
                &CanonSubject {
                    id: subject.id.clone(),
                    name: subject.name.clone(),
                    identity: subject_character
                        .and_then(|c| str_field(c, "identity"))
                        .map(str::to_string),
                    description: subject_character
                        .and_then(|c| str_field(c, "description"))
                        .map(str::to_string),
                },
                true,
            );
            resolved.push(Resolved {
                input: OtherSubjectInput {
                    id: subject.id.clone(),
                    name: subject.name.clone(),
                    pronouns: subject.pronouns.clone(),
                    is_user: subject.is_user,
                    canon_block: crate::canon::render_other_canon_block(&canon),
                },
                canon_source: canon.source.as_str(),
            });
        }
        if resolved.is_empty() {
            continue;
        }
        let subject_inputs: Vec<OtherSubjectInput> =
            resolved.iter().map(|r| r.input.clone()).collect();

        let extraction = extract_other_memories_from_fold(
            executor,
            completion,
            &rendered,
            &observer.id,
            &observer.name,
            observer.pronouns.as_ref(),
            &subject_inputs,
            per_subject_cap,
            selection,
            uncensored_fallback,
            input.cheap_max_tokens.map(|t| t as f64),
            input.in_autonomous_room,
            Some(&orienting),
            Some(&clock),
        )
        .await;

        if let Some(u) = extraction.usage {
            result.usage.prompt_tokens += u.prompt_tokens;
            result.usage.completion_tokens += u.completion_tokens;
            result.usage.total_tokens += u.total_tokens;
        }
        if !extraction.success {
            if extraction.timed_out {
                result.passes_lost_to_timeout += 1;
            }
            tracing::warn!(
                chatId = %chat_id,
                observer = %observer.name,
                timedOut = extraction.timed_out,
                error = %extraction.error.as_deref().unwrap_or_default(),
                "[FoldOtherPass] OTHER extraction failed"
            );
            continue;
        }

        let by_subject = extraction.result.unwrap_or_default();
        let last_message: Option<&Value> = last_message_by_character
            .get(&observer.id)
            .copied()
            .or_else(|| trimmed.last().copied());
        for subject in &resolved {
            // The parser allows one extra slot for an anchored event; the fold
            // grain's contract is a hard cap per subject.
            let mut proposed = by_subject
                .iter()
                .find(|(id, _)| *id == subject.input.id)
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            proposed.truncate(per_subject_cap as usize);
            let proposed_len = proposed.len();
            let candidates = match &rl {
                RateLimitDecision::Throttle { floor, .. } => {
                    apply_importance_floor(proposed, *floor)
                }
                _ => proposed,
            };
            result.candidates_proposed += candidates.len();
            tracing::debug!(
                chatId = %chat_id,
                observer = %observer.name,
                subject = %subject.input.name,
                canon = %subject.canon_source,
                proposed = proposed_len,
                kept = candidates.len(),
                "[FoldOtherPass] Candidates for pair"
            );
            for candidate in &candidates {
                write_candidate(
                    db,
                    embedding,
                    &write_ctx,
                    &mut state,
                    WriteArgs {
                        character_id: &observer.id,
                        character_name: &observer.name,
                        about_character_id: &subject.input.id,
                        about_character_name: &subject.input.name,
                        pass: "OTHER",
                        candidate,
                        pass_label: format!(
                            "fold OTHER memory {} about {}",
                            observer.name, subject.input.name
                        ),
                        source_message_id: last_message
                            .and_then(|m| str_field(m, "id"))
                            .map(str::to_string),
                        source_message_created_at: last_message
                            .and_then(|m| str_field(m, "createdAt"))
                            .map(str::to_string),
                    },
                )
                .await?;
            }
        }
    }

    result.memories_written = state.created_memory_ids.len();
    result.memories_reinforced = state.reinforced_memory_ids.len();
    if input.dry_run {
        result.extracted_candidates = Some(std::mem::take(&mut state.collected_candidates));
    }

    // A pass lost to a timeout leaves the watermark where it was: the window is
    // still uncovered and the next fold or sweep will take it up.
    if result.passes_lost_to_timeout == 0 {
        advance_watermark(db, chat_id, cover_through.as_deref(), input.dry_run, result).await;
    } else {
        tracing::warn!(
            chatId = %chat_id,
            passesLostToTimeout = result.passes_lost_to_timeout,
            "[FoldOtherPass] Passes lost to timeout; watermark not advanced"
        );
    }

    tracing::info!(
        chatId = %chat_id,
        observers = result.observers,
        candidatesProposed = result.candidates_proposed,
        memoriesWritten = result.memories_written,
        memoriesReinforced = result.memories_reinforced,
        watermarkAdvancedTo = %result.watermark_advanced_to.as_deref().unwrap_or("null"),
        "[FoldOtherPass] Fold-grain OTHER pass complete"
    );
    Ok(None)
}

/// v4 `advanceWatermark`: no id (or a dry run) → nothing; else the ONE column
/// writer. v4's `chats.update` on a vanished chat is a silent no-op that still
/// "succeeds", so v5's `Ok(false)` is treated exactly as `Ok(true)` (set +
/// DEBUG); only `Err` takes the WARN. Logged on the caller thread, after the
/// write returns.
async fn advance_watermark(
    db: &Db,
    chat_id: &str,
    message_id: Option<&str>,
    dry_run: bool,
    result: &mut FoldOtherPassResult,
) {
    let Some(message_id) = message_id.filter(|m| !m.is_empty()) else {
        return;
    };
    if dry_run {
        return;
    }
    let (cid, mid) = (chat_id.to_string(), message_id.to_string());
    let outcome = db
        .write(move |w| {
            w.main()
                .chats()
                .set_other_extraction_watermark(&cid, Some(&mid))
        })
        .await;
    match outcome {
        Ok(_) => {
            result.watermark_advanced_to = Some(message_id.to_string());
            tracing::debug!(
                chatId = %chat_id,
                messageId = %message_id,
                "[FoldOtherPass] Watermark advanced"
            );
        }
        Err(error) => tracing::warn!(
            chatId = %chat_id,
            error = %error_text(&error),
            "[FoldOtherPass] Failed to advance watermark"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v4 jest `fold-other-pass.test.ts` — `messagesPastWatermark` ×3.
    #[test]
    fn messages_past_watermark_mirrors_v4() {
        let list = vec![json!({"id": "a"}), json!({"id": "b"}), json!({"id": "c"})];
        assert_eq!(messages_past_watermark(&list, None).len(), 3);
        assert_eq!(
            messages_past_watermark(&list, Some("b")),
            &[json!({"id": "c"})]
        );
        // A vanished watermark reads as absent — re-read the tail once.
        assert_eq!(messages_past_watermark(&list, Some("zzz")).len(), 3);
        // `!watermarkMessageId` — an empty id is falsy.
        assert_eq!(messages_past_watermark(&list, Some("")).len(), 3);
    }

    #[test]
    fn eligibility_is_a_spoken_seat_line() {
        let base = json!({
            "type": "message", "role": "ASSISTANT", "participantId": "s1", "content": "Hello."
        });
        assert!(is_fold_other_eligible_message(&base));
        let with = |k: &str, v: Value| {
            let mut m = base.clone();
            m[k] = v;
            m
        };
        assert!(is_fold_other_eligible_message(&with("role", json!("USER"))));
        assert!(!is_fold_other_eligible_message(&with(
            "role",
            json!("TOOL")
        )));
        assert!(!is_fold_other_eligible_message(&with(
            "type",
            json!("system-event")
        )));
        assert!(!is_fold_other_eligible_message(&with(
            "systemSender",
            json!("lantern")
        )));
        // JS truthiness: an empty `systemSender` / `null` passes.
        assert!(is_fold_other_eligible_message(&with(
            "systemSender",
            json!("")
        )));
        assert!(is_fold_other_eligible_message(&with(
            "systemSender",
            Value::Null
        )));
        assert!(!is_fold_other_eligible_message(&with(
            "participantId",
            Value::Null
        )));
        assert!(!is_fold_other_eligible_message(&with(
            "participantId",
            json!("")
        )));
        // JS `trim()`: U+FEFF is whitespace there.
        assert!(!is_fold_other_eligible_message(&with(
            "content",
            json!(" \u{feff}\n")
        )));
        assert!(!is_fold_other_eligible_message(&with("content", json!(42))));
    }
}

// ============================================================================
// Idle catch-up (daily maintenance sweep)
// ============================================================================

/// v4 `FoldOtherCatchupCandidate`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldOtherCatchupCandidate {
    pub chat_id: String,
    pub user_id: String,
    /// The chat's last conversation message — the watermark target and the
    /// job's dedupe anchor.
    pub last_message_id: String,
    pub connection_profile_id: String,
}

/// v4 `FoldOtherCatchupSelection`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldOtherCatchupSelection {
    pub candidates: Vec<FoldOtherCatchupCandidate>,
    /// Eligible chats beyond the per-sweep cap, left for the next sweep.
    pub deferred: usize,
}

/// v4 `pickConnectionProfileId`: the first CHARACTER seat not controlled by
/// the user that carries a profile, else ANY seat that carries one.
fn pick_connection_profile_id(chat: &Value) -> Option<String> {
    let participants = chat.get("participants").and_then(Value::as_array)?;
    let has_profile = |p: &&Value| truthy(p.get("connectionProfileId"));
    let llm_seat = participants.iter().find(|p| {
        str_field(p, "type") == Some("CHARACTER")
            && str_field(p, "controlledBy") != Some("user")
            && has_profile(p)
    });
    llm_seat
        .or_else(|| participants.iter().find(has_profile))
        .and_then(|p| str_field(p, "connectionProfileId"))
        .map(str::to_string)
}

/// v4 `findFoldOtherCatchupCandidates({ now, limit })`: chats whose tail the
/// fold-grain pass has not covered — idle for more than
/// [`FOLD_OTHER_CATCHUP_IDLE_MS`], last message past the watermark, not a
/// help/brahma chat, and (cheap prefilter) active within the lookback. Mode
/// `turn` selects nothing. Most recently active first, bounded by `limit`.
///
/// v4's reads here are all FALLBACKS (`chats.findAll` → `[]`, `getMessages` →
/// `[]`, the mode getter → defaults), so its search never throws; v5's only
/// `Err` is a pool checkout that fails.
pub fn find_fold_other_catchup_candidates(
    db: &Db,
    now_ms: i64,
    limit: usize,
) -> Result<FoldOtherCatchupSelection, DbError> {
    let mode = db.read_main(|c| Ok(instance_settings::get_memory_extraction_mode_settings(c)))?;
    if mode.other_pass == "turn" {
        tracing::debug!("[FoldOtherCatchup] otherPass=turn; no catch-up needed");
        return Ok(FoldOtherCatchupSelection::default());
    }

    let chats =
        crate::db::fallback::find_all_or_empty("chats", || db.read_main(chats_read::find_all));
    let mut prefiltered: Vec<(&Value, i64)> = chats
        .iter()
        .filter(|c| !crate::chat_predicates::is_help_like_chat_type(str_field(c, "chatType")))
        .filter(|c| c.get("messageCount").and_then(Value::as_f64).unwrap_or(0.0) >= 2.0)
        .filter_map(|c| {
            let last = str_field(c, "lastMessageAt")
                .filter(|s| !s.is_empty())
                .and_then(crate::episodic::js_date_parse_ms)?;
            Some((c, last))
        })
        .filter(|(_, last)| {
            now_ms - last > FOLD_OTHER_CATCHUP_IDLE_MS
                && now_ms - last < FOLD_OTHER_CATCHUP_LOOKBACK_MS
        })
        .collect();
    // `(a, b) => b.last - a.last` — a STABLE sort, newest first.
    prefiltered.sort_by_key(|x| std::cmp::Reverse(x.1));

    let mut selection = FoldOtherCatchupSelection::default();
    for (chat, _) in &prefiltered {
        let chat_id = str_field(chat, "id").unwrap_or_default();
        if selection.candidates.len() >= limit {
            // Counted without reading their messages; some may turn out covered.
            selection.deferred += 1;
            continue;
        }
        let messages = match chat_message_events(db, chat_id) {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(
                    chatId = %chat_id,
                    error = %error_text(&error),
                    "[FoldOtherCatchup] Could not inspect chat; skipping"
                );
                continue;
            }
        };
        let Some(last_id) = messages
            .last()
            .map(|m| str_field(m, "id").unwrap_or_default())
        else {
            continue;
        };
        if str_field(chat, "otherExtractionWatermarkMessageId") == Some(last_id) {
            continue;
        }
        let Some(profile_id) = pick_connection_profile_id(chat) else {
            tracing::debug!(
                chatId = %chat_id,
                "[FoldOtherCatchup] No connection profile on any seat; skipping chat"
            );
            continue;
        };
        selection.candidates.push(FoldOtherCatchupCandidate {
            chat_id: chat_id.to_string(),
            user_id: str_field(chat, "userId").unwrap_or_default().to_string(),
            last_message_id: last_id.to_string(),
            connection_profile_id: profile_id,
        });
    }

    tracing::debug!(
        scanned = chats.len(),
        idleAndRecent = prefiltered.len(),
        selected = selection.candidates.len(),
        deferred = selection.deferred,
        "[FoldOtherCatchup] Selection complete"
    );
    Ok(selection)
}

/// v4 `RunFoldOtherCatchupInput`.
pub struct RunFoldOtherCatchupInput<'a> {
    pub chat_id: &'a str,
    pub user_id: &'a str,
    /// The job's connection profile row.
    pub connection_profile: &'a Value,
    /// The user's `chatSettings.cheapLLMSettings`.
    pub cheap_llm_settings: &'a CheapLlmSettings,
    /// The user's connection profile rows.
    pub available_profiles: &'a [Value],
    /// Skip the idle recheck (the deferred regenerate rebuild, tests).
    pub ignore_idle: bool,
    /// v4 `input.now ?? Date.now()` (P4.D270 R-J — injectable).
    pub now_ms: Option<i64>,
}

/// v4 `runFoldOtherCatchup`: catch up one idle chat — build the window of
/// messages past the watermark, resolve the cheap LLM (Concierge-aware, as the
/// per-turn handler does) and run the pass. Called by the `MEMORY_EXTRACTION`
/// handler's `foldOtherCatchup` branch. v4 has NO catch here — a read error is
/// the job's (every read it makes is a fallback in v4, so only v5's pool
/// failure reaches `Err`).
pub async fn run_fold_other_catchup<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    input: &RunFoldOtherCatchupInput<'_>,
) -> Result<FoldOtherPassResult, DbError> {
    let chat_id = input.chat_id;
    let Some(chat) = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|conn| chats_read::find_by_id(conn, chat_id))
    }) else {
        return Ok(empty_result(Some("no-chat")));
    };

    let now = input.now_ms.unwrap_or_else(crate::clock::now_unix_ms);
    let last_at = str_field(&chat, "lastMessageAt")
        .filter(|s| !s.is_empty())
        .and_then(crate::episodic::js_date_parse_ms);
    if let (false, Some(last_at)) = (input.ignore_idle, last_at) {
        if now - last_at <= FOLD_OTHER_CATCHUP_IDLE_MS {
            tracing::debug!(
                chatId = %chat_id,
                "[FoldOtherCatchup] Chat active again since the sweep; leaving it to the fold"
            );
            return Ok(empty_result(Some("not-idle")));
        }
    }

    let messages = chat_message_events(db, chat_id)?;
    let watermark = str_field(&chat, "otherExtractionWatermarkMessageId");
    let window = messages_past_watermark(&messages, watermark);
    if window.is_empty() {
        return Ok(empty_result(Some("behind-watermark")));
    }

    let profiles: Vec<CheapLlmProfile> = input
        .available_profiles
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();
    let connection_profile = cheap_llm_profile_from_value(input.connection_profile);
    let selection = get_cheap_llm_provider(
        &connection_profile,
        &CheapLlmConfig {
            strategy: input.cheap_llm_settings.strategy.clone(),
            // v4 `userDefinedProfileId ?? undefined` — `getCheapLLMProvider`'s
            // own guard drops an empty string, so the net effect is the filter.
            user_defined_profile_id: input
                .cheap_llm_settings
                .user_defined_profile_id
                .clone()
                .filter(|s| !s.is_empty()),
            default_cheap_profile_id: None,
            fallback_to_local: input.cheap_llm_settings.fallback_to_local,
        },
        &profiles,
        false,
        None,
    );
    // v4 `chatSettings.findByUserId` — a FALLBACK read (ERROR + `null`).
    let chat_settings =
        match db.read_main(|c| crate::db::chat_settings::find_by_user_id(c, input.user_id)) {
            Ok(row) => row,
            Err(e) => {
                tracing::error!(
                    target: "quilltap::db",
                    collection = "chat_settings",
                    userId = %input.user_id,
                    error = %error_text(&e),
                    "Error finding chat settings by user ID"
                );
                None
            }
        };
    let policy = resolve_concierge_settings(chat_settings.as_ref(), Some(&chat));
    let selection = resolve_uncensored_cheap_llm_selection(
        selection,
        should_use_uncensored_route(Some(&chat)),
        Some(&policy),
        &profiles,
    );

    // Background only; a read failure degrades to no description, silently.
    let mut project_description: Option<String> = None;
    if let Some(project_id) = str_field(&chat, "projectId").filter(|s| !s.is_empty()) {
        let read = db.read_main(|main| {
            db.read_mount_index(|mount| {
                Ok(crate::db::projects::ProjectsRepository::new(main, mount)
                    .find_by_id(project_id)
                    .ok()
                    .flatten())
            })
        });
        if let Ok(Some(project)) = read {
            project_description = str_field(&project, "description").map(str::to_string);
        }
    }

    tracing::debug!(
        chatId = %chat_id,
        windowMessages = window.len(),
        watermark = %watermark.unwrap_or("null"),
        "[FoldOtherCatchup] Running catch-up pass"
    );
    let fallback = UncensoredFallbackOptions {
        concierge_policy: &policy,
        available_profiles: &profiles,
        is_dangerous_chat: None,
    };
    Ok(run_fold_other_pass(
        db,
        completion,
        embedding,
        executor,
        &selection,
        Some(&fallback),
        &RunFoldOtherPassInput {
            chat_id: chat_id.to_string(),
            user_id: input.user_id.to_string(),
            window_messages: window.to_vec(),
            cheap_max_tokens: Some(resolve_max_tokens(
                connection_profile.max_tokens.map(|v| v as i64),
                connection_profile.model_class.as_deref(),
            )),
            timeline_mode: str_field(&chat, "timelineMode")
                .unwrap_or("realtime")
                .to_string(),
            project_id: str_field(&chat, "projectId").map(str::to_string),
            in_autonomous_room: str_field(&chat, "chatType") == Some("autonomous"),
            project_description,
            chat_context_summary: str_field(&chat, "contextSummary").map(str::to_string),
            cover_through_message_id: messages
                .last()
                .and_then(|m| str_field(m, "id"))
                .map(str::to_string),
            dry_run: false,
        },
    )
    .await)
}
