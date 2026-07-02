//! Aurora Core whisper — cadence trigger.
//!
//! Port of v4's lib/chat/context/core-whisper-trigger.ts. Pure function. Given
//! a chat's event history and the participant about to take the next turn,
//! decide whether to offer this character their own `Core/` packet before
//! that turn fires.
//!
//! The three triggers (composed by OR; only one fire per turn):
//!
//! - **first** — the character has never been offered a Core whisper in this
//!   chat. Covers two cases: the character has not yet spoken (the literal
//!   first-turn case) AND chats that predate the feature (the bootstrap
//!   case, where the character has spoken before but never been offered a
//!   Core packet).
//! - **periodic** — `interval` of their own visible turns have passed since
//!   the last Core whisper for them.
//! - **silence** — the last `silenceThreshold` visible conversational turns
//!   immediately before this one were authored by *someone else* (user or
//!   another participant). Long stretches without your voice are exactly
//!   where convergence pressure builds; the packet says "you're still
//!   here — what do you think?"
//!
//! Visible conversational turn (`isVisibleConversationalTurn`) excludes:
//!
//! - Non-message events (`context-summary`, `system`).
//! - Staff whispers — any message with a `systemSender` set.
//! - `isSilentMessage === true`.
//! - Empty / tool-call-only assistant turns (trimmed content empty).
//! - Private whispers targeted away from this responding character
//!   (`targetParticipantIds` set, but doesn't include them) — those aren't
//!   part of this character's room cadence.
//!
//! Continue / nudge turns skip the whisper entirely — a continuation is not a
//! new response.
//!
//! Computed on demand from the message history (no persistent counter).

/// The subset of a `ChatEvent` this module reads. `event_type` mirrors v4's
/// discriminated `type` field (`"message"` / `"context-summary"` / `"system"`);
/// only `"message"` events carry the rest of the fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WhisperEvent {
    pub event_type: String,
    pub role: Option<String>,
    pub participant_id: Option<String>,
    pub content: Option<String>,
    pub system_sender: Option<String>,
    pub system_kind: Option<String>,
    pub is_silent_message: Option<bool>,
    pub target_participant_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreWhisperReason {
    First,
    Periodic,
    Silence,
    ContextTransition,
}

impl CoreWhisperReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreWhisperReason::First => "first",
            CoreWhisperReason::Periodic => "periodic",
            CoreWhisperReason::Silence => "silence",
            CoreWhisperReason::ContextTransition => "context-transition",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShouldFireCoreWhisperResult {
    pub fire: bool,
    pub reason: Option<CoreWhisperReason>,
}

pub struct ShouldFireCoreWhisperOptions<'a> {
    pub events: &'a [WhisperEvent],
    pub responding_participant_id: &'a str,
    pub is_continue: bool,
    pub is_nudge: bool,
    pub interval: i64,
    pub silence_threshold: i64,
    /// When true, the first visible turn after a Librarian rolling-summary
    /// fold counts as a `context-transition` fire.
    pub fire_on_context_transition: bool,
}

/// A "visible conversational turn" for the purposes of this responding
/// character's cadence.
fn is_visible_conversational_turn(m: &WhisperEvent, responding_participant_id: &str) -> bool {
    if m.system_sender.is_some() {
        return false;
    }
    if m.is_silent_message == Some(true) {
        return false;
    }
    match &m.content {
        Some(content) if !crate::jsstr::js_trim(content).is_empty() => {}
        _ => return false,
    }
    if let Some(targets) = &m.target_participant_ids {
        if !targets.is_empty() && !targets.iter().any(|t| t == responding_participant_id) {
            return false;
        }
    }
    true
}

/// Conservative v1 definition of a major context transition: the most recent
/// Librarian rolling-summary fold whisper. Returns its index, or `None`.
///
/// The Librarian's rolling-summary `systemKind` values use kebab-case slugs
/// containing the literal words `summary` or `rolling` (e.g. `rolling-summary`,
/// `per-character-summary`). The earlier substring check on `fold` was too
/// loose — it false-matched announcements like `folder-created-by-character`,
/// which represent a character filing a folder, not a memory fold. The
/// tightened check uses kebab-segment membership so `summary` / `rolling`
/// match cleanly without catching unrelated kinds.
fn find_latest_context_transition_index(
    events: &[WhisperEvent],
    responding_participant_id: &str,
) -> Option<usize> {
    for i in (0..events.len()).rev() {
        let m = &events[i];
        if m.event_type != "message" {
            continue;
        }
        if m.system_sender.as_deref() != Some("librarian") {
            continue;
        }
        let Some(system_kind) = &m.system_kind else {
            continue;
        };
        let lowered = system_kind.to_lowercase();
        let segments: Vec<&str> = lowered.split(['-', '_']).collect();
        if !segments.contains(&"summary") && !segments.contains(&"rolling") {
            continue;
        }
        // A per-character Librarian summary that isn't addressed to this
        // character shouldn't count as a transition for them.
        if let Some(targets) = &m.target_participant_ids {
            if !targets.is_empty() && !targets.iter().any(|t| t == responding_participant_id) {
                continue;
            }
        }
        return Some(i);
    }
    None
}

pub fn should_fire_core_whisper(
    options: ShouldFireCoreWhisperOptions<'_>,
) -> ShouldFireCoreWhisperResult {
    let ShouldFireCoreWhisperOptions {
        events,
        responding_participant_id,
        is_continue,
        is_nudge,
        interval,
        silence_threshold,
        fire_on_context_transition,
    } = options;

    if is_continue || is_nudge {
        return ShouldFireCoreWhisperResult {
            fire: false,
            reason: None,
        };
    }

    let mut last_core_whisper_idx: Option<usize> = None;
    let mut my_self_turn_count: i64 = 0;
    let mut my_self_turns_after_whisper: i64 = 0;
    let mut silence_run: i64 = 0;

    for (i, m) in events.iter().enumerate() {
        if m.event_type != "message" {
            // System/context-summary events neither extend nor break the
            // silence run.
            continue;
        }

        if m.system_sender.as_deref() == Some("aurora")
            && m.system_kind.as_deref() == Some("core-whisper")
            && m.target_participant_ids
                .as_ref()
                .map(|t| t.iter().any(|p| p == responding_participant_id))
                .unwrap_or(false)
        {
            last_core_whisper_idx = Some(i);
            my_self_turns_after_whisper = 0;
            // A Core whisper isn't a turn by anyone; don't touch silence_run.
            continue;
        }

        let visible = is_visible_conversational_turn(m, responding_participant_id);
        if !visible {
            continue;
        }

        let is_mine = m.role.as_deref() == Some("ASSISTANT")
            && m.participant_id.as_deref() == Some(responding_participant_id);

        if is_mine {
            my_self_turn_count += 1;
            if let Some(last_idx) = last_core_whisper_idx {
                if i > last_idx {
                    my_self_turns_after_whisper += 1;
                }
            }
            silence_run = 0;
        } else {
            silence_run += 1;
        }
    }
    let _ = my_self_turn_count; // mirrors v4's unused-after-loop local

    // "First" covers both the literal first-turn case AND the bootstrap case
    // for chats that predate the feature — a character who has spoken many
    // times but never been offered a Core packet gets one on their next turn.
    let Some(last_core_whisper_idx) = last_core_whisper_idx else {
        return ShouldFireCoreWhisperResult {
            fire: true,
            reason: Some(CoreWhisperReason::First),
        };
    };

    if my_self_turns_after_whisper >= interval {
        return ShouldFireCoreWhisperResult {
            fire: true,
            reason: Some(CoreWhisperReason::Periodic),
        };
    }

    if silence_run >= silence_threshold {
        return ShouldFireCoreWhisperResult {
            fire: true,
            reason: Some(CoreWhisperReason::Silence),
        };
    }

    if fire_on_context_transition {
        if let Some(transition_idx) =
            find_latest_context_transition_index(events, responding_participant_id)
        {
            if transition_idx > last_core_whisper_idx {
                return ShouldFireCoreWhisperResult {
                    fire: true,
                    reason: Some(CoreWhisperReason::ContextTransition),
                };
            }
        }
    }

    ShouldFireCoreWhisperResult {
        fire: false,
        reason: None,
    }
}
