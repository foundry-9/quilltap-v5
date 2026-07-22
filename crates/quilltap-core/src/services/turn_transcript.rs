//! The per-turn transcript builder (v4
//! `lib/services/chat-message/turn-transcript.ts`).
//!
//! Per-turn memory extraction: instead of running extraction once per
//! assistant message (with each pass seeing only its own slice), v4 waits
//! until the turn closes and runs extraction once against a joined transcript
//! of the whole turn — the user message that opened it plus every character
//! response that followed, keyed by character.
//!
//! "Turn opener" is the most recent non-system USER message. The turn closes
//! when control returns to the user (`turnInfo.isUsersTurn === true` on the
//! last finalizer of the turn).
//!
//! The output structs ([`TurnTranscript`] / [`TurnCharacterSlice`]) live in
//! [`crate::memory_tasks`] (they predate this module — the Carina handler
//! builds a synthetic one-slice transcript by hand); this module owns the
//! general builder plus `findTurnOpenerMessageId`, which moved here from the
//! finalizer so the turn machinery has one home.

use serde_json::Value;

use crate::memory_format::Pronouns;
use crate::memory_tasks::{TurnCharacterSlice, TurnTranscript};

/// v4 `BuildTurnTranscriptOptions`.
#[derive(Clone, Debug, Default)]
pub struct BuildTurnTranscriptOptions {
    /// The USER message that opened the turn. `None` for greeting-only turns.
    pub turn_opener_message_id: Option<String>,
    /// Optional terminal ASSISTANT message ID. When set, the forward walk
    /// stops after collecting the message whose id matches — used by
    /// autonomous chats so each speaker's turn becomes its own transcript
    /// rather than re-extracting the whole tail every time.
    pub extraction_anchor_message_id: Option<String>,
    pub user_character_id: Option<String>,
    pub user_character_name: Option<String>,
    pub user_character_pronouns: Option<Pronouns>,
}

/// v4 `findTurnOpenerMessageId` (turn-transcript.ts) — the most recent
/// non-system USER message id, or `None`. (Moved here from the finalizer;
/// `trigger_turn_memory_extraction` re-uses it.)
pub fn find_turn_opener_message_id(messages: &[Value]) -> Option<String> {
    for m in messages.iter().rev() {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if m.get("role").and_then(Value::as_str) != Some("USER") {
            continue;
        }
        if is_truthy_system_sender(m.get("systemSender")) {
            continue;
        }
        return m.get("id").and_then(Value::as_str).map(String::from);
    }
    None
}

/// Build a per-turn transcript from chat history (v4 `buildTurnTranscript`).
///
/// Walks forward from the turn opener (exclusive) to the end of the message
/// list, grouping ASSISTANT messages by `participantId`. Skips system whispers
/// (Host, Librarian, Concierge, etc.), tool messages, and silent-mode messages
/// — none of those represent participant speech.
///
/// If `turn_opener_message_id` is `None` we treat every assistant message in
/// the history as belonging to "the current turn"; the user-message side of
/// the transcript is `None` and the user-pass extraction skips itself. When
/// `extraction_anchor_message_id` is set, the walk stops after COLLECTING the
/// anchor (v4 checks it after every skip guard, so an anchor id on a skipped
/// row does not stop the walk) — autonomous chats use this to bound each
/// character's slice.
///
/// `participants` are the chat's participant rows; `participant_characters`
/// maps `characterId` → the hydrated character row.
pub fn build_turn_transcript(
    messages: &[Value],
    participants: &[Value],
    participant_characters: &std::collections::HashMap<String, Value>,
    options: &BuildTurnTranscriptOptions,
) -> TurnTranscript {
    // Keyed slices + first-contribution order (v4 keeps a Map + an order list).
    let mut slices: Vec<TurnCharacterSlice> = Vec::new();
    let mut user_message: Option<String> = None;
    let mut user_slice: Option<TurnCharacterSlice> = None;
    let mut latest_assistant_message_id: Option<String> = None;
    let mut turn_timestamp: Option<String> = None;

    let mut scanning = options.turn_opener_message_id.is_none();
    for m in messages {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }

        if !scanning {
            if m.get("id").and_then(Value::as_str) == options.turn_opener_message_id.as_deref()
                && m.get("role").and_then(Value::as_str) == Some("USER")
            {
                let content = m
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                user_message = Some(content.clone());
                scanning = true;
                if let Some(ts) = m.get("createdAt").and_then(Value::as_str) {
                    turn_timestamp = Some(ts.to_string());
                }
                // Promote a user-controlled opener to a first-class slice so its
                // driver forms memories. The opener's participantId
                // authoritatively identifies which user character spoke this turn
                // (more reliable than the singular userCharacterId option, which
                // is just the first user-controlled participant). Built before
                // the forward walk; prepended below so it reads first
                // chronologically.
                let opener_participant_id = m.get("participantId").and_then(Value::as_str);
                let opener_participant = opener_participant_id.and_then(|pid| {
                    participants
                        .iter()
                        .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
                });
                if let Some(p) = opener_participant {
                    if p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                        && p.get("controlledBy").and_then(Value::as_str) == Some("user")
                    {
                        if let Some(char_id) = p
                            .get("characterId")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            if let Some(character) = participant_characters.get(char_id) {
                                user_slice = Some(TurnCharacterSlice {
                                    character_id: json_str(character, "id"),
                                    character_name: json_str(character, "name"),
                                    character_pronouns: pronouns_from_character(character),
                                    text: content.clone(),
                                    contributing_message_ids: vec![json_str(m, "id")],
                                    is_user_controlled: true,
                                    last_message_created_at: m
                                        .get("createdAt")
                                        .and_then(Value::as_str)
                                        .map(String::from),
                                });
                            }
                        }
                    }
                }
            }
            continue;
        }

        // The turn ends when control returns to the user.
        if m.get("role").and_then(Value::as_str) == Some("USER")
            && !is_truthy_system_sender(m.get("systemSender"))
        {
            break;
        }

        if is_truthy_system_sender(m.get("systemSender")) {
            continue;
        }
        if m.get("role").and_then(Value::as_str) != Some("ASSISTANT") {
            continue;
        }
        if is_truthy_silent(m.get("isSilentMessage")) {
            continue;
        }
        let Some(participant_id) = m
            .get("participantId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };

        let Some(participant) = participants
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
        else {
            continue;
        };
        if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = participant
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };

        let Some(character) = participant_characters.get(character_id) else {
            continue;
        };

        let message_id = json_str(m, "id");
        let content = m
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let created_at = m.get("createdAt").and_then(Value::as_str).map(String::from);

        if let Some(existing) = slices.iter_mut().find(|s| s.character_id == character_id) {
            existing.text = if existing.text.is_empty() {
                content
            } else {
                format!("{}\n\n{content}", existing.text)
            };
            existing.contributing_message_ids.push(message_id.clone());
            if created_at.is_some() {
                existing.last_message_created_at = created_at.clone();
            }
        } else {
            slices.push(TurnCharacterSlice {
                character_id: json_str(character, "id"),
                character_name: json_str(character, "name"),
                character_pronouns: pronouns_from_character(character),
                text: content,
                contributing_message_ids: vec![message_id.clone()],
                is_user_controlled: false,
                last_message_created_at: created_at.clone(),
            });
        }

        latest_assistant_message_id = Some(message_id.clone());
        if let Some(ts) = created_at {
            turn_timestamp = Some(ts);
        }

        if options.extraction_anchor_message_id.as_deref() == Some(message_id.as_str()) {
            break;
        }
    }

    let character_slices = match user_slice {
        Some(u) => {
            let mut v = vec![u];
            v.extend(slices);
            v
        }
        None => slices,
    };

    TurnTranscript {
        turn_opener_message_id: options.turn_opener_message_id.clone(),
        user_message,
        user_character_id: options.user_character_id.clone(),
        user_character_name: options.user_character_name.clone(),
        user_character_pronouns: options.user_character_pronouns.clone(),
        character_slices,
        latest_assistant_message_id,
        turn_timestamp,
    }
}

/// JS truthiness of `m.systemSender` (v4 `if (m.systemSender)`): a non-empty
/// string. The column is `string | null` in the row schema, so other JSON
/// shapes never occur; a defensive JS-truthy read covers them anyway.
pub(crate) fn is_truthy_system_sender(v: Option<&Value>) -> bool {
    is_js_truthy(v)
}

/// JS truthiness of a read-side string-or-bool message cell.
pub(crate) fn is_js_truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(Value::Null) | None => false,
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// JS-truthy read of the TEXT-affinity `isSilentMessage` cell after v4's
/// read-side coercion: `"1.0"`/`"1"`/`true` → true, `"0.0"`/`false`/NULL →
/// false. v4 reads `m.isSilentMessage` after the read coerces the stored
/// `"1.0"`/`"0.0"` back to a bool, so a false silent message does NOT skip
/// (the string arms are the defensive raw-row read; `get_messages` normally
/// hands real bools). Moved here from the finalizer (P4.6bj).
pub(crate) fn is_truthy_silent(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "1.0" || s == "1" || s == "true",
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        _ => false,
    }
}

/// `character.pronouns ?? null` — the character row's pronouns object, typed.
/// A partial object (missing any of the three fields) reads as absent; real
/// rows are schema-complete or null.
pub(crate) fn pronouns_from_character(c: &Value) -> Option<Pronouns> {
    let p = c.get("pronouns")?;
    Some(Pronouns {
        subject: p.get("subject").and_then(Value::as_str)?.to_string(),
        object: p.get("object").and_then(Value::as_str)?.to_string(),
        possessive: p.get("possessive").and_then(Value::as_str)?.to_string(),
    })
}

fn json_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
