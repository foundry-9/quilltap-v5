//! Port of v4's turn-state machine: lib/chat/turn-manager/queue.ts and
//! state.ts (plus `getQueuePosition` from utils.ts). Pure, immutable updates to
//! a [`TurnState`] — the session-only rotation bookkeeping for multi-character
//! chats.
//!
//! USER and ASSISTANT messages count symmetrically (a human typing as a
//! user-controlled character takes a turn). Whisper messages — those with a
//! non-empty `targetParticipantIds` — never affect turn order. The
//! "spoken this cycle" list wraps (resets to just the new speaker) once every
//! present CHARACTER participant has spoken, mirroring `selectNextSpeaker`'s
//! `cycleComplete`.

use serde_json::Value;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};

/// Turn-state tracking for a multi-character chat session. Recalculated from
/// history on reload; otherwise updated in place per message.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TurnState {
    /// Participants who have spoken since the user last spoke (this cycle).
    pub spoken_since_user_turn: Vec<String>,
    /// Whose turn it is (`None` = user's turn).
    pub current_turn_participant_id: Option<String>,
    /// Manually queued participants, in order (first = next).
    pub queue: Vec<String>,
    /// Last speaker (cannot speak again unless nudged/queued, except if sole character).
    pub last_speaker_id: Option<String>,
}

/// A message as the turn machine reads it. `msg_type` is the `ChatEvent.type`
/// discriminator (`Some("message")` for real messages); it is unused by the
/// `MessageEvent`-typed functions and may be `None` there. A non-empty
/// `target_participant_ids` marks a whisper.
#[derive(Clone, Debug)]
pub struct MessageView {
    pub msg_type: Option<String>,
    pub role: String,
    pub participant_id: Option<String>,
    pub target_participant_ids: Option<Vec<String>>,
}

impl MessageView {
    /// A whisper is a message whose `targetParticipantIds` is present and non-empty.
    fn is_whisper(&self) -> bool {
        self.target_participant_ids
            .as_ref()
            .is_some_and(|t| !t.is_empty())
    }

    fn is_user_or_assistant(&self) -> bool {
        self.role == "USER" || self.role == "ASSISTANT"
    }

    /// A non-empty participant id, if this message carries one (JS truthiness:
    /// `null`/empty is dropped).
    fn nonempty_participant_id(&self) -> Option<&str> {
        self.participant_id.as_deref().filter(|p| !p.is_empty())
    }
}

/// A participant as the cycle-wrap reads it. The wrap counts present CHARACTER
/// participants that carry a character id.
#[derive(Clone, Debug)]
pub struct ParticipantView {
    pub id: String,
    pub participant_type: String,
    pub status: ParticipantStatus,
    pub character_id: Option<String>,
}

impl ParticipantView {
    /// Eligible for the active-cycle set: a present CHARACTER with a (truthy)
    /// character id.
    fn is_active_character(&self) -> bool {
        self.participant_type == "CHARACTER"
            && is_participant_present(self.status)
            && self.character_id.as_deref().is_some_and(|c| !c.is_empty())
    }
}

// ---------------------------------------------------------------------------
// queue.ts
// ---------------------------------------------------------------------------

/// Append a participant to the queue, skipping duplicates.
pub fn add_to_queue(state: &TurnState, participant_id: &str) -> TurnState {
    if state.queue.iter().any(|id| id == participant_id) {
        return state.clone();
    }
    let mut next = state.clone();
    next.queue.push(participant_id.to_string());
    next
}

/// Remove a participant from the queue (all occurrences).
pub fn remove_from_queue(state: &TurnState, participant_id: &str) -> TurnState {
    let mut next = state.clone();
    next.queue.retain(|id| id != participant_id);
    next
}

/// Pop the next participant from the front of the queue. Returns the updated
/// state and the popped id (`None` when the queue was empty).
pub fn pop_from_queue(state: &TurnState) -> (TurnState, Option<String>) {
    if state.queue.is_empty() {
        return (state.clone(), None);
    }
    let mut next = state.clone();
    let first = next.queue.remove(0);
    (next, Some(first))
}

/// Move a participant to the front of the queue (adding it if absent).
pub fn nudge_participant(state: &TurnState, participant_id: &str) -> TurnState {
    let mut queue = Vec::with_capacity(state.queue.len() + 1);
    queue.push(participant_id.to_string());
    queue.extend(
        state
            .queue
            .iter()
            .filter(|id| *id != participant_id)
            .cloned(),
    );
    TurnState {
        queue,
        ..state.clone()
    }
}

/// Reset the cycle on a user skip: clear `spoken_since_user_turn`, keep the
/// queue and last speaker intact.
pub fn reset_cycle_for_user_skip(state: &TurnState) -> TurnState {
    TurnState {
        spoken_since_user_turn: Vec::new(),
        ..state.clone()
    }
}

/// The 1-indexed queue position of a participant, or 0 if not queued.
pub fn get_queue_position(state: &TurnState, participant_id: &str) -> i64 {
    match state.queue.iter().position(|id| id == participant_id) {
        Some(i) => (i as i64) + 1,
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// state.ts
// ---------------------------------------------------------------------------

/// A fresh, empty turn state.
pub fn create_initial_turn_state() -> TurnState {
    TurnState::default()
}

/// Parse a persisted `spokenThisCycleParticipantIds` JSON string into a list of
/// participant ids. Anything falsy/unparseable/non-array yields an empty list;
/// a JSON array keeps only its string elements.
fn parse_spoken(json: Option<&str>) -> Vec<String> {
    let Some(s) = json.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(s) {
        Ok(Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// Calculate turn state: `spoken_since_user_turn` from the persisted cycle JSON,
/// and `last_speaker_id` from the most recent non-whisper USER/ASSISTANT message
/// that carries a participant id. Queue and current-turn start empty.
pub fn calculate_turn_state_from_history(
    messages: &[MessageView],
    spoken_this_cycle_json: Option<&str>,
) -> TurnState {
    let mut state = create_initial_turn_state();
    state.spoken_since_user_turn = parse_spoken(spoken_this_cycle_json);

    for msg in messages.iter().rev() {
        if !msg.is_user_or_assistant() {
            continue;
        }
        let Some(pid) = msg.nonempty_participant_id() else {
            continue;
        };
        if msg.is_whisper() {
            continue;
        }
        state.last_speaker_id = Some(pid.to_string());
        break;
    }

    state
}

/// Update turn state after a message is sent. Non-turn messages (wrong role,
/// missing participant id, or whisper) leave the state unchanged.
pub fn update_turn_state_after_message(state: &TurnState, message: &MessageView) -> TurnState {
    if !message.is_user_or_assistant() {
        return state.clone();
    }
    let Some(pid) = message.nonempty_participant_id() else {
        return state.clone();
    };
    if message.is_whisper() {
        return state.clone();
    }

    let mut next = state.clone();
    if !next.spoken_since_user_turn.iter().any(|id| id == pid) {
        next.spoken_since_user_turn.push(pid.to_string());
    }
    next.last_speaker_id = Some(pid.to_string());
    next.queue.retain(|id| id != pid);
    next.current_turn_participant_id = None;
    next
}

/// Shared cycle-append + wrap logic for the two `computeSpokenThisCycle*`
/// entry points. Returns the next JSON value for the column, or `None` when no
/// write is needed.
fn compute_spoken_next(
    participant_id: &str,
    participants: &[ParticipantView],
    current_spoken_json: Option<&str>,
) -> Option<String> {
    let current = parse_spoken(current_spoken_json);
    let already = current.iter().any(|id| id == participant_id);
    let next: Vec<String> = if already {
        current
    } else {
        let mut n = current;
        n.push(participant_id.to_string());
        n
    };

    // Cycle wrap: once every present CHARACTER has spoken, restart with just
    // this speaker.
    let active_ids: std::collections::HashSet<&str> = participants
        .iter()
        .filter(|p| p.is_active_character())
        .map(|p| p.id.as_str())
        .collect();
    if !active_ids.is_empty() {
        let spoken_active = next
            .iter()
            .filter(|id| active_ids.contains(id.as_str()))
            .count();
        if spoken_active >= active_ids.len() {
            return Some(json_ids(&[participant_id.to_string()]));
        }
    }

    if already {
        // `next === current` in v4 — nothing changed, no write needed.
        return None;
    }
    Some(json_ids(&next))
}

/// JSON-encode a list of ids exactly as JS `JSON.stringify` does (compact, no spaces).
fn json_ids(ids: &[String]) -> String {
    serde_json::to_string(ids).expect("string array always serializes")
}

/// Next `spokenThisCycleParticipantIds` after a message is persisted, or `None`
/// when the message doesn't affect turn order (wrong type/role, whisper, or no
/// participant id) or when nothing changed.
pub fn compute_spoken_this_cycle_after_message(
    message: &MessageView,
    participants: &[ParticipantView],
    current_spoken_json: Option<&str>,
) -> Option<String> {
    if message.msg_type.as_deref() != Some("message") {
        return None;
    }
    if !message.is_user_or_assistant() {
        return None;
    }
    let pid = message.nonempty_participant_id()?;
    if message.is_whisper() {
        return None;
    }
    compute_spoken_next(pid, participants, current_spoken_json)
}

/// Next `spokenThisCycleParticipantIds` after a skip-user-turn: append the
/// skipped participant (as if they took a turn), applying the same wrap rules.
pub fn compute_spoken_this_cycle_after_skip(
    skipped_participant_id: &str,
    participants: &[ParticipantView],
    current_spoken_json: Option<&str>,
) -> Option<String> {
    compute_spoken_next(skipped_participant_id, participants, current_spoken_json)
}
