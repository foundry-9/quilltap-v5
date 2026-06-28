//! Port of the pure gating logic in v4's lib/chat/context-summary.ts — the
//! rolling-window summarisation cadence. These four functions decide *when* a
//! fold / rebuild / title-check fires; the actual LLM fold, the DB writes and
//! the Librarian-whisper sweep stay in later (stateful) phases.
//!
//! Cadence knobs (carried verbatim from v4):
//!   - FOLD_TURN_BATCH: turns folded per fire (5).
//!   - FOLD_TAIL_FLOOR: minimum recent turns kept verbatim (5).
//!   - FOLD_TRIGGER_DELTA: accumulated tail before a fold fires (floor + batch
//!     = 10). When `currentTurn - lastFoldedTurn > FOLD_TRIGGER_DELTA`, fold.
//!   - T_HARD_TURN_THRESHOLD: periodic full from-scratch rebuild (50), cheap
//!     insurance against accumulated paraphrase drift across many folds.

use crate::chat_predicates::is_help_like_chat_type;

/// Number of turns folded per fire.
pub const FOLD_TURN_BATCH: i64 = 5;
/// Minimum recent turns kept verbatim (never folded away).
pub const FOLD_TAIL_FLOOR: i64 = 5;
/// Turns of accumulated tail before a fold fires.
pub const FOLD_TRIGGER_DELTA: i64 = FOLD_TAIL_FLOOR + FOLD_TURN_BATCH;
/// Periodic full from-scratch rebuild threshold.
pub const T_HARD_TURN_THRESHOLD: i64 = 50;

/// The gate's verdict: leave the summary alone, fold the next batch, or do a
/// full from-scratch rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummarizationGateDecision {
    Skip,
    Fold,
    Hard,
}

impl SummarizationGateDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            SummarizationGateDecision::Skip => "skip",
            SummarizationGateDecision::Fold => "fold",
            SummarizationGateDecision::Hard => "hard",
        }
    }
}

/// Decide whether to skip, fold the next batch, or do a full rebuild. Pure —
/// no side effects. `current_turn` is the interchange count; `last_folded_turn`
/// is the turn through which the running summary has already absorbed content;
/// `last_full_rebuild_turn` anchors the T_hard window.
pub fn evaluate_summarization_gate(
    current_turn: i64,
    last_folded_turn: i64,
    last_full_rebuild_turn: i64,
) -> SummarizationGateDecision {
    // Below the floor + batch threshold, no fold needs to happen — the LLM
    // still sees the full conversation as recent tail.
    if current_turn <= FOLD_TRIGGER_DELTA {
        return SummarizationGateDecision::Skip;
    }

    // T_hard wins over the regular fold path. A from-scratch rebuild implicitly
    // satisfies any fold that was due.
    if current_turn - last_full_rebuild_turn >= T_HARD_TURN_THRESHOLD {
        return SummarizationGateDecision::Hard;
    }

    if current_turn - last_folded_turn > FOLD_TRIGGER_DELTA {
        return SummarizationGateDecision::Fold;
    }

    SummarizationGateDecision::Skip
}

/// A minimal view of a chat message for the cadence functions: only the three
/// fields they read. `role`/`message_type` are `Option` to preserve the JS
/// distinction between absent and empty (the two guards treat them
/// differently — see the call sites).
#[derive(Clone, Debug)]
pub struct InterchangeMessage {
    pub role: Option<String>,
    pub message_type: Option<String>,
    pub system_sender: Option<String>,
}

/// Count the interchanges in a chat.
///
/// For normal chats an interchange is one user message + one assistant
/// response — count is the minimum of the two so partial pairs don't tick the
/// meter. Autonomous rooms have no human user, so that floor is permanently
/// zero; instead, count each assistant turn as one interchange.
///
/// Staff whispers (`systemSender` set) carry role ASSISTANT but are synthetic
/// system messages, not real turns — excluded so the cadence isn't inflated by
/// the ~5 whispers each autonomous turn drags along.
pub fn calculate_interchange_count(
    messages: &[InterchangeMessage],
    chat_type: Option<&str>,
) -> i64 {
    let mut user_messages = 0i64;
    let mut assistant_messages = 0i64;

    for msg in messages {
        // Skip non-message types (context-summary, tool-result, etc.). JS:
        // `if (msg.type && msg.type !== 'message')` — an absent or empty type
        // is falsy and is treated as a real message.
        if msg
            .message_type
            .as_deref()
            .is_some_and(|t| !t.is_empty() && t != "message")
        {
            continue;
        }

        // Skip staff whispers — not interchange-bearing turns. Empty string is
        // falsy in JS, so only a non-empty systemSender skips.
        if msg.system_sender.as_deref().is_some_and(|s| !s.is_empty()) {
            continue;
        }

        // `msg.role?.toUpperCase()`.
        let role = msg.role.as_deref().map(|r| r.to_uppercase());
        match role.as_deref() {
            Some("USER") => user_messages += 1,
            Some("ASSISTANT") => assistant_messages += 1,
            _ => {}
        }
    }

    if chat_type == Some("autonomous") {
        return assistant_messages;
    }

    // An interchange is complete when both user and assistant have spoken —
    // the minimum of the two is the limiting factor.
    user_messages.min(assistant_messages)
}

/// Whether a title-update check should fire at this interchange count.
///
/// - Regular chats: 2, 3, 5, 7, 10, then every 10 after.
/// - Help-like chats: 1, 2, 3, 5, 7, 10, then every 10 after (fires right after
///   the first Q&A).
///
/// Crossing semantics: each checkpoint fires when the counter has *reached or
/// passed* it since the last check — autonomous rooms can bump the counter by
/// 5+ in one turn and skip past exact 10/20/30 marks, so `>=` (not `==`) is
/// required.
pub fn should_check_title_at_interchange(
    current_interchange: i64,
    last_checked_interchange: i64,
    chat_type: Option<&str>,
) -> bool {
    let is_help_chat = is_help_like_chat_type(chat_type);

    // Help-like chats fire at interchange 1; regular chats never before 2.
    let minimum_interchange = if is_help_chat { 1 } else { 2 };
    if current_interchange < minimum_interchange {
        return false;
    }

    // Fire if we've crossed any not-yet-checked early checkpoint.
    let early_checkpoints: &[i64] = if is_help_chat {
        &[1, 2, 3, 5, 7, 10]
    } else {
        &[2, 3, 5, 7, 10]
    };
    for &checkpoint in early_checkpoints {
        if current_interchange >= checkpoint && last_checked_interchange < checkpoint {
            return true;
        }
    }

    // After 10, fire if the most recently crossed multiple of 10 is one we
    // haven't checked yet. `floor(n / 10) * 10` is the highest multiple of 10
    // ≤ n; if that exceeds `lastCheckedInterchange`, a new boundary was crossed.
    if current_interchange >= 10 {
        let last_crossed_multiple_of_10 = (current_interchange / 10) * 10;
        if last_crossed_multiple_of_10 > last_checked_interchange {
            return true;
        }
    }

    false
}

/// A message as seen by [`partition_messages_into_turns`].
#[derive(Clone, Debug)]
pub struct PartitionInputMessage {
    pub id: String,
    pub role: Option<String>,
    pub message_type: Option<String>,
    pub system_sender: Option<String>,
}

/// One turn produced by [`partition_messages_into_turns`]: a 1-indexed number
/// and the IDs of the USER + non-staff ASSISTANT messages composing it,
/// chronological.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldedTurn {
    pub turn_number: usize,
    pub ids: Vec<String>,
}

/// Walk chat history in chronological order, grouping messages into turns. A
/// turn begins on a USER message; trailing non-staff ASSISTANT messages before
/// the next USER message belong to that turn. Staff-authored messages
/// (`systemSender` set) are excluded but do not affect numbering. ASSISTANT-only
/// greeting messages before any USER message are folded into turn 1.
///
/// Autonomous rooms have no USER pivot — partition on each character-attributed
/// ASSISTANT message instead, else `turns.length` is permanently 0 and
/// summarisation always bails with "No messages to summarize".
///
/// NB v4 compares `m.role` against the uppercase literals without uppercasing
/// (MessageEvent.role is a `'USER' | 'ASSISTANT'` union) — mirrored here as a
/// case-sensitive compare, unlike [`calculate_interchange_count`].
pub fn partition_messages_into_turns(
    all_messages: &[PartitionInputMessage],
    chat_type: Option<&str>,
) -> Vec<FoldedTurn> {
    let mut turns: Vec<FoldedTurn> = Vec::new();
    // Index into `turns` of the in-progress turn, mirroring v4's `currentTurn`.
    let mut current_turn: Option<usize> = None;
    let mut leading_assistant: Option<Vec<String>> = None;
    let is_autonomous = chat_type == Some("autonomous");

    for msg in all_messages {
        if msg.message_type.as_deref() != Some("message") {
            continue;
        }
        let role = msg.role.as_deref();
        if role != Some("USER") && role != Some("ASSISTANT") {
            continue;
        }
        if msg.system_sender.as_deref().is_some_and(|s| !s.is_empty()) {
            continue;
        }

        let starts_new_turn = role == Some("USER") || (is_autonomous && role == Some("ASSISTANT"));

        if starts_new_turn {
            let turn_number = turns.len() + 1;
            let mut start_ids = leading_assistant.take().unwrap_or_default();
            start_ids.push(msg.id.clone());
            turns.push(FoldedTurn {
                turn_number,
                ids: start_ids,
            });
            current_turn = Some(turns.len() - 1);
        } else if let Some(idx) = current_turn {
            turns[idx].ids.push(msg.id.clone());
        } else {
            leading_assistant
                .get_or_insert_with(Vec::new)
                .push(msg.id.clone());
        }
    }

    turns
}
