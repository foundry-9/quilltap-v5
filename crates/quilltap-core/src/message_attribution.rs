//! Port of the pure history-access / presence / attribution helpers in v4's
//! lib/chat/context/message-attribution.ts. These shape the conversation as a
//! particular responding character sees it: history-access gating, presence
//! windows, whisper visibility, and per-character role/name attribution.
//!
//! The one impurity in v4 is `new Date(createdAt).getTime()` inside the
//! history-access filter; here it is injected as pre-parsed epoch millis (the
//! same seam used elsewhere in the port). The presence-window filters compare
//! ISO strings lexically exactly as v4 does — lex order matches chronological
//! order for the uniform `…Z` timestamps the data uses.

use std::collections::HashMap;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};

/// A participant as the attribution helpers read it. Not every field is used by
/// every function (history-access reads `has_history_access`/join time;
/// name attribution reads `participant_type`/`character_id`; etc.).
#[derive(Clone, Debug)]
pub struct AttributionParticipant {
    pub id: String,
    pub participant_type: String,
    pub character_id: Option<String>,
    pub controlled_by: String,
    pub status: ParticipantStatus,
}

/// A Host status announcement payload. Presence tracking filters on the
/// `(participant_id, to_status)` shape; off-scene introductions (only
/// `introducedCharacterIds` set) carry `to_status: None` and are ignored.
#[derive(Clone, Debug)]
pub struct HostEvent {
    pub participant_id: Option<String>,
    pub to_status: Option<ParticipantStatus>,
}

/// A message in multi-character context building. `created_at` is the ISO
/// string (used lexically by the presence filters).
#[derive(Clone, Debug)]
pub struct AttributionMessage {
    pub id: Option<String>,
    pub role: String,
    pub content: String,
    pub participant_id: Option<String>,
    pub thought_signature: Option<String>,
    pub created_at: Option<String>,
    pub target_participant_ids: Option<Vec<String>>,
    pub host_event: Option<HostEvent>,
}

/// A message reduced to what the history-access numeric filter needs: its
/// creation instant in epoch millis (None when `createdAt` was absent — such a
/// message is always kept). The ISO→ms parse is the injected date seam.
#[derive(Clone, Debug)]
pub struct HistoryMessage {
    pub created_at_ms: Option<f64>,
}

/// A presence window: an interval `[from, to)` during which the participant was
/// 'active' or 'silent'. `to` is `None` on the trailing open window. Times are
/// ISO strings; lex order matches chronological order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceWindow {
    pub from: String,
    pub to: Option<String>,
}

/// The result of attributing a message to a responding character's viewpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCharacterMessage {
    pub role: &'static str,
    pub content: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub participant_id: Option<String>,
    pub thought_signature: Option<String>,
}

/// Filter messages by a participant's history access. With full access, every
/// message is returned. Otherwise only messages at or after the participant's
/// join time survive; a message with no `createdAt` (`None` ms) is kept
/// (shouldn't happen). Returns a keep-mask aligned to `messages`.
///
/// `join_ms` is `new Date(participant.createdAt).getTime()` and each message's
/// `created_at_ms` is the same parse — both injected at the boundary.
pub fn filter_messages_by_history_access(
    messages: &[HistoryMessage],
    has_history_access: bool,
    join_ms: f64,
) -> Vec<bool> {
    if has_history_access {
        return vec![true; messages.len()];
    }
    messages
        .iter()
        .map(|m| match m.created_at_ms {
            None => true,
            Some(t) => t >= join_ms,
        })
        .collect()
}

/// Compute the presence windows for a participant by walking Host status
/// announcements (`hostEvent.participantId === participant.id`) in chronological
/// order. An interval is open while the participant is `active`/`silent` and
/// closed by a transition to `absent`/`removed`; the trailing open window has
/// `to: None`. With no Host events, the participant is assumed present from
/// `participant_created_at` onward (one open window).
pub fn compute_presence_windows_for_participant(
    messages: &[AttributionMessage],
    participant_id: &str,
    participant_created_at: &str,
) -> Vec<PresenceWindow> {
    let mut events: Vec<(String, ParticipantStatus)> = messages
        .iter()
        .filter_map(|m| {
            let he = m.host_event.as_ref()?;
            let to_status = he.to_status?;
            let created_at = m.created_at.as_ref()?;
            if he.participant_id.as_deref() == Some(participant_id) {
                Some((created_at.clone(), to_status))
            } else {
                None
            }
        })
        .collect();

    // Stable sort by ISO instant, matching v4's comparator.
    events.sort_by(|a, b| a.0.cmp(&b.0));

    if events.is_empty() {
        return vec![PresenceWindow {
            from: participant_created_at.to_string(),
            to: None,
        }];
    }

    let mut windows: Vec<PresenceWindow> = Vec::new();
    let mut open_from: Option<String> = None;
    for (at, to_status) in events {
        let present = is_participant_present(to_status);
        if present {
            if open_from.is_none() {
                open_from = Some(at);
            }
        } else if let Some(from) = open_from.take() {
            windows.push(PresenceWindow { from, to: Some(at) });
        }
    }
    if let Some(from) = open_from {
        windows.push(PresenceWindow { from, to: None });
    }

    windows
}

/// Keep messages whose `createdAt` falls inside one of the presence windows.
/// `from` inclusive, `to` exclusive; a `None` `to` means still open. Messages
/// without `createdAt` are dropped. Empty window set drops everything. Returns
/// a keep-mask aligned to `messages`.
pub fn filter_messages_by_presence_windows(
    messages: &[AttributionMessage],
    windows: &[PresenceWindow],
) -> Vec<bool> {
    if windows.is_empty() {
        return vec![false; messages.len()];
    }
    messages
        .iter()
        .map(|m| match m.created_at.as_deref() {
            None => false,
            Some(t) => windows
                .iter()
                .any(|w| t >= w.from.as_str() && w.to.as_deref().is_none_or(|to| t < to)),
        })
        .collect()
}

/// Keep whisper messages visible to the responding participant. A public
/// message (no `targetParticipantIds`) is always visible; a whisper is visible
/// to its sender and its targets only. Returns a keep-mask aligned to
/// `messages`.
pub fn filter_whisper_messages(
    messages: &[AttributionMessage],
    responding_participant_id: &str,
) -> Vec<bool> {
    messages
        .iter()
        .map(|m| {
            match &m.target_participant_ids {
                // Public message — always include.
                None => true,
                Some(targets) if targets.is_empty() => true,
                Some(targets) => {
                    // Sender sees their own whispers; targets see whispers at them.
                    m.participant_id.as_deref() == Some(responding_participant_id)
                        || targets.iter().any(|t| t == responding_participant_id)
                }
            }
        })
        .collect()
}

/// Resolve a participant's display name for attribution. CHARACTER participants
/// (LLM or user-controlled) resolve via their `characterId` into the character
/// map. Returns `None` when the id is absent, the participant is missing, or it
/// isn't a CHARACTER with a known character.
pub fn get_participant_name(
    participant_id: Option<&str>,
    participant_characters: &HashMap<String, String>,
    all_participants: &[AttributionParticipant],
) -> Option<String> {
    let participant_id = participant_id?;
    let participant = all_participants.iter().find(|p| p.id == participant_id)?;

    // v4 guards on `participant.characterId` truthiness — empty string is falsy.
    if participant.participant_type == "CHARACTER" {
        if let Some(cid) = participant.character_id.as_deref() {
            if !cid.is_empty() {
                return participant_characters.get(cid).cloned();
            }
        }
    }
    None
}

/// Attribute messages for a responding character's viewpoint: messages from the
/// responder become `assistant`; everything else becomes `user`, carrying the
/// sender's name for disambiguation.
pub fn attribute_messages_for_character(
    messages: &[AttributionMessage],
    responding_participant_id: &str,
    participant_characters: &HashMap<String, String>,
    all_participants: &[AttributionParticipant],
) -> Vec<MultiCharacterMessage> {
    messages
        .iter()
        .map(|msg| {
            let participant_name = get_participant_name(
                msg.participant_id.as_deref(),
                participant_characters,
                all_participants,
            );

            // assistant iff the message is from the responding character; the
            // other-character and USER branches both yield `user` in v4.
            let role = if msg.participant_id.as_deref() == Some(responding_participant_id) {
                "assistant"
            } else {
                "user"
            };

            // `msg.participantId || undefined` — empty string drops to None.
            let participant_id = match msg.participant_id.as_deref() {
                Some(p) if !p.is_empty() => Some(p.to_string()),
                _ => None,
            };

            MultiCharacterMessage {
                role,
                content: msg.content.clone(),
                id: msg.id.clone(),
                name: participant_name,
                participant_id,
                thought_signature: msg.thought_signature.clone(),
            }
        })
        .collect()
}

/// Find the user participant's display name for attribution. Prefers the
/// actively-selected "Speaking As" participant when it is a present,
/// user-controlled CHARACTER; otherwise the first such participant. A character
/// with no (truthy) name yields `None`.
pub fn find_user_participant_name(
    all_participants: &[AttributionParticipant],
    participant_characters: &HashMap<String, String>,
    active_typing_participant_id: Option<&str>,
) -> Option<String> {
    let is_user_character = |p: &AttributionParticipant| {
        p.participant_type == "CHARACTER"
            && p.controlled_by == "user"
            && is_participant_present(p.status)
            && p.character_id.as_deref().is_some_and(|c| !c.is_empty())
    };

    let selected = active_typing_participant_id.and_then(|atid| {
        all_participants
            .iter()
            .find(|p| p.id == atid && is_user_character(p))
    });
    let user_character_participant =
        selected.or_else(|| all_participants.iter().find(|p| is_user_character(p)));

    if let Some(p) = user_character_participant {
        if let Some(cid) = p.character_id.as_deref() {
            if let Some(name) = participant_characters.get(cid) {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}
