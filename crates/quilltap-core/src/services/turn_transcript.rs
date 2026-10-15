//! The per-turn transcript builder (v4
//! `lib/services/chat-message/turn-transcript.ts`, pin `83334b9a5` — bug
//! 245's bound from `64743ee13`, the extraction turn list and the shared
//! message read from `ac6348c5f`).
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
//! The output structs ([`TurnTranscript`] / [`TurnCharacterSlice`]) live here,
//! in their v4 home (work order P4.D296 L-11); `crate::memory_tasks`
//! re-exports them until its extraction half is deleted.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats_messages_read, DbError};
use crate::memory_format::Pronouns;

/// One character's joined contribution to a turn (v4 `TurnCharacterSlice`).
/// `is_user_controlled` is `?? false` at every v4 use site, so the optional
/// boolean collapses to a plain `bool`.
#[derive(Clone, Debug, Default)]
pub struct TurnCharacterSlice {
    pub character_id: String,
    pub character_name: String,
    pub character_pronouns: Option<Pronouns>,
    /// Joined text from every assistant message this character contributed
    /// during the turn, in chronological order.
    pub text: String,
    /// Every assistant message ID that contributed to this slice.
    pub contributing_message_ids: Vec<String>,
    /// True when this slice's text was authored by the human driving a
    /// user-controlled character.
    pub is_user_controlled: bool,
    /// `createdAt` of the slice's LAST contributing message — the wall-clock
    /// anchor memories derived from this slice are stamped from (episodic
    /// spine).
    pub last_message_created_at: Option<String>,
}

/// The joined per-turn transcript (v4 `TurnTranscript`). `user_message` is
/// `string | null` in v4 — `None` here means null (an empty string is a real,
/// present opener).
#[derive(Clone, Debug, Default)]
pub struct TurnTranscript {
    /// ID of the USER message that opened the turn, or `None` for
    /// greeting/continue turns with no fresh user input.
    pub turn_opener_message_id: Option<String>,
    /// Verbatim user-message text, or `None` when the turn has no user opener.
    pub user_message: Option<String>,
    pub user_character_id: Option<String>,
    pub user_character_name: Option<String>,
    pub user_character_pronouns: Option<Pronouns>,
    /// ASSISTANT participants that spoke during the turn, ordered by first
    /// contribution.
    pub character_slices: Vec<TurnCharacterSlice>,
    /// Lines of this turn an earlier committed extraction already covered (the
    /// part at or before `extract_after_message_id`), grouped the same way.
    /// Context only: rendered under their own heading, never extraction
    /// material, and capped at [`PRIOR_CONTEXT_MAX_MESSAGES`] messages. `None`
    /// (v4's absent key) when the whole turn is new — never `Some(vec![])`
    /// from the builder (bug 245).
    pub prior_slices: Option<Vec<TurnCharacterSlice>>,
    /// The most recent ASSISTANT message ID in the turn — used as
    /// sourceMessageId on derived memories.
    pub latest_assistant_message_id: Option<String>,
    /// `createdAt` of the turn's newest message (v4 `turnTimestamp`) — the
    /// extraction clock's fallback anchor when no slice timestamp is available.
    pub turn_timestamp: Option<String>,
}

/// The most already-extracted messages a transcript keeps as context (bug
/// 245).
pub const PRIOR_CONTEXT_MAX_MESSAGES: usize = 8;

/// A chat's message rows (`type: 'message'`), chronological — the one read of
/// a transcript every extraction path starts from (v4
/// `loadChatMessageEvents`).
pub fn load_chat_message_events(db: &Db, chat_id: &str) -> Result<Vec<Value>, DbError> {
    let chat_id = chat_id.to_string();
    let all = db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id))?;
    Ok(all
        .into_iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .collect())
}

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
    /// The last message an earlier extraction of this stretch covered. When
    /// it is one of the turn's own assistant lines, everything up to and
    /// including it (the opener's slice too) becomes `prior_slices` —
    /// context, not material. A bound outside the turn (an older turn's
    /// line, or a later one from a concurrent job) is ignored, so it can
    /// never hide a turn's own lines (bug 245).
    pub extract_after_message_id: Option<String>,
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

/// v4 `isAutonomousTurnAnchor`: a non-staff, non-silent ASSISTANT line from a
/// seat.
fn is_autonomous_turn_anchor(m: &Value) -> bool {
    m.get("type").and_then(Value::as_str) == Some("message")
        && m.get("role").and_then(Value::as_str) == Some("ASSISTANT")
        && !is_truthy_system_sender(m.get("systemSender"))
        && !is_truthy_silent(m.get("isSilentMessage"))
        && is_js_truthy(m.get("participantId"))
}

/// The latest message that closes an autonomous-room turn: a non-staff,
/// non-silent ASSISTANT line from a seat; `None` when there is none (v4
/// `findAutonomousTurnAnchorMessageId` — the finalizer trigger's old inline
/// walk, hoisted here unchanged).
pub fn find_autonomous_turn_anchor_message_id(messages: &[Value]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| is_autonomous_turn_anchor(m))
        .and_then(|m| m.get("id").and_then(Value::as_str).map(String::from))
}

/// How a turn is addressed for extraction: its USER opener, and (autonomous
/// rooms) the message that closes it (v4 `ExtractionTurnLocator`). `None` on
/// the two optional ids is v4's absent key OR `null` — every reader treats
/// them alike (`?? null`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractionTurnLocator {
    pub turn_opener_message_id: Option<String>,
    pub extraction_anchor_message_id: Option<String>,
    /// Lines at or before this message are context, not material (bug 245).
    /// Set by [`list_extraction_turns`] for an autonomous room (the previous
    /// speaker's anchor); a live job takes it from the chat's `turn` cursor.
    pub extract_after_message_id: Option<String>,
}

/// Every turn of a chat, as extraction addresses them (v4
/// `listExtractionTurns`) — the one scan the queue-memories action, the dry
/// run, the regenerate and the SillyTavern import share.
///
///  - An ordinary chat: one null-opener turn for the greeting when any
///    character line comes before the first user message (the live trigger
///    enqueues it when the greeting finalizes), then one turn per non-staff
///    USER message (each opens a turn). A chat with no user message and some
///    character line is just the greeting turn; with neither, no turns.
///  - An autonomous room: no USER opener ever, so one turn per closing
///    ASSISTANT line, each anchored on it (the live trigger's shape), each
///    read past the previous anchor — without the bound every job would
///    re-read the room from its first line (bug 245).
pub fn list_extraction_turns(
    messages: &[Value],
    chat_type: Option<&str>,
) -> Vec<ExtractionTurnLocator> {
    let id_of = |m: &Value| m.get("id").and_then(Value::as_str).map(String::from);
    if chat_type == Some("autonomous") {
        let anchors: Vec<&Value> = messages
            .iter()
            .filter(|m| is_autonomous_turn_anchor(m))
            .collect();
        return anchors
            .iter()
            .enumerate()
            .map(|(i, m)| ExtractionTurnLocator {
                turn_opener_message_id: None,
                extraction_anchor_message_id: id_of(m),
                extract_after_message_id: if i > 0 { id_of(anchors[i - 1]) } else { None },
            })
            .collect();
    }
    let is_opener = |m: &Value| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("role").and_then(Value::as_str) == Some("USER")
            && !is_truthy_system_sender(m.get("systemSender"))
    };
    let before_first_opener = match messages.iter().position(is_opener) {
        None => messages,
        Some(first) => &messages[..first],
    };
    let mut turns: Vec<ExtractionTurnLocator> = Vec::new();
    if before_first_opener.iter().any(is_autonomous_turn_anchor) {
        turns.push(ExtractionTurnLocator::default());
    }
    for m in messages.iter().filter(|m| is_opener(m)) {
        turns.push(ExtractionTurnLocator {
            turn_opener_message_id: id_of(m),
            ..Default::default()
        });
    }
    turns
}

/// Resolve the user-controlled character (if any) from a chat's participants,
/// hydrated from a map of the chat's character rows — v4
/// `resolveUserCharacterParticipant`, which `0246c6c8` moved here from the two
/// byte-identical copies it had grown.
///
/// Quilltap is single-user per instance: at most one CHARACTER participant is
/// marked as the user persona. When none exists — or its character row is
/// missing from the map — the caller simply omits the user from its subject set.
///
/// Lives here because every caller is feeding [`build_turn_transcript`]'s
/// `user_character_*` options, which is exactly what this returns.
///
/// v5 has ONE caller where v4 has two: the second is v4's streaming
/// extraction DRY RUN (`?action=extract-memories-dry-run`,
/// `handleExtractMemoriesDryRun` at the `83334b9a5` pin), deferred to Tier 3 by
/// P4.D296's R-W — its loud refusal stands; no wire variant is added.
pub fn resolve_user_character_participant<'a>(
    participants: &[Value],
    participant_characters: &'a std::collections::HashMap<String, Value>,
) -> Option<&'a Value> {
    let p = participants.iter().find(|p| {
        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
            && p.get("controlledBy").and_then(Value::as_str) == Some("user")
            && p.get("characterId")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
    })?;
    let char_id = p.get("characterId").and_then(Value::as_str)?;
    participant_characters.get(char_id)
}

/// One collected turn line: the message and the character who spoke it (v4
/// `TurnLine`).
struct TurnLine<'a> {
    message: &'a Value,
    character: &'a Value,
}

/// Group lines into per-speaker slices, ordered by first contribution (v4
/// `groupIntoSlices`, keyed by `character.id`).
fn group_into_slices(lines: &[TurnLine<'_>]) -> Vec<TurnCharacterSlice> {
    let mut slices: Vec<TurnCharacterSlice> = Vec::new();
    for TurnLine {
        message: m,
        character,
    } in lines
    {
        let character_id = json_str(character, "id");
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
            existing.contributing_message_ids.push(json_str(m, "id"));
            // `m.createdAt ?? existing.lastMessageCreatedAt`
            if created_at.is_some() {
                existing.last_message_created_at = created_at;
            }
        } else {
            slices.push(TurnCharacterSlice {
                character_id,
                character_name: json_str(character, "name"),
                character_pronouns: pronouns_from_character(character),
                text: content,
                contributing_message_ids: vec![json_str(m, "id")],
                is_user_controlled: false,
                last_message_created_at: created_at,
            });
        }
    }
    slices
}

/// The turn as one stretch of material: its already-extracted lines folded
/// back into the slices they belong to (prior lines first) — v4
/// `withPriorAsMaterial`. For an observer who joined the turn after an
/// earlier extraction, everything said in it is new (bug 245). A transcript
/// with no prior lines is returned as it is.
pub fn with_prior_as_material(transcript: &TurnTranscript) -> TurnTranscript {
    let Some(prior) = transcript.prior_slices.as_ref().filter(|p| !p.is_empty()) else {
        return transcript.clone();
    };
    let mut merged: Vec<(String, TurnCharacterSlice)> = Vec::new();
    for slice in prior.iter().chain(transcript.character_slices.iter()) {
        let key = format!(
            "{}:{}",
            if slice.is_user_controlled {
                "user"
            } else {
                "ai"
            },
            slice.character_id
        );
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| *k == key) {
            existing.text = format!("{}\n\n{}", existing.text, slice.text);
            existing
                .contributing_message_ids
                .extend(slice.contributing_message_ids.iter().cloned());
            // `slice.lastMessageCreatedAt ?? existing.lastMessageCreatedAt`
            if slice.last_message_created_at.is_some() {
                existing.last_message_created_at = slice.last_message_created_at.clone();
            }
        } else {
            merged.push((key, slice.clone()));
        }
    }
    TurnTranscript {
        character_slices: merged.into_iter().map(|(_, s)| s).collect(),
        prior_slices: None,
        ..transcript.clone()
    }
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
/// Bug 245: the collected lines split at `extract_after_message_id` when it
/// is one of them — material after it, prior (the last
/// [`PRIOR_CONTEXT_MAX_MESSAGES`]) up to and including it; the opener's user
/// slice goes to PRIOR once the bound is inside the turn. A bound outside the
/// turn is ignored. `latest_assistant_message_id` / `turn_timestamp` still
/// cover the WHOLE turn.
///
/// `participants` are the chat's participant rows; `participant_characters`
/// maps `characterId` → the hydrated character row.
pub fn build_turn_transcript(
    messages: &[Value],
    participants: &[Value],
    participant_characters: &std::collections::HashMap<String, Value>,
    options: &BuildTurnTranscriptOptions,
) -> TurnTranscript {
    let mut lines: Vec<TurnLine<'_>> = Vec::new();
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

        lines.push(TurnLine {
            message: m,
            character,
        });
        let message_id = json_str(m, "id");
        latest_assistant_message_id = Some(message_id.clone());
        if let Some(ts) = m.get("createdAt").and_then(Value::as_str) {
            turn_timestamp = Some(ts.to_string());
        }

        // v4 `options.extractionAnchorMessageId && m.id === …` — an empty
        // anchor never stops the walk.
        if options
            .extraction_anchor_message_id
            .as_deref()
            .is_some_and(|a| !a.is_empty() && a == message_id)
        {
            break;
        }
    }

    // Split at the bound when it is one of this turn's own lines (bug 245).
    let bound_index = options
        .extract_after_message_id
        .as_deref()
        .filter(|b| !b.is_empty())
        .and_then(|b| {
            lines
                .iter()
                .position(|l| l.message.get("id").and_then(Value::as_str) == Some(b))
        });
    let (material, prior): (&[TurnLine<'_>], &[TurnLine<'_>]) = match bound_index {
        Some(i) => {
            let prior_all = &lines[..=i];
            let keep_from = prior_all.len().saturating_sub(PRIOR_CONTEXT_MAX_MESSAGES);
            (&lines[i + 1..], &prior_all[keep_from..])
        }
        None => (&lines[..], &[]),
    };
    let mut material_slices = group_into_slices(material);
    let mut prior_slices = group_into_slices(prior);
    // The opener was read with the turn's first extraction; once the bound is
    // inside the turn it is context too.
    if let Some(u) = user_slice {
        if bound_index.is_some() {
            prior_slices.insert(0, u);
        } else {
            material_slices.insert(0, u);
        }
    }

    TurnTranscript {
        turn_opener_message_id: options.turn_opener_message_id.clone(),
        user_message,
        user_character_id: options.user_character_id.clone(),
        user_character_name: options.user_character_name.clone(),
        user_character_pronouns: options.user_character_pronouns.clone(),
        character_slices: material_slices,
        prior_slices: if prior_slices.is_empty() {
            None
        } else {
            Some(prior_slices)
        },
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
