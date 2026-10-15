//! Transcript windows for the extraction passes — port of v4
//! `lib/memory/extraction/window.ts` (`ac6348c5f`, memory refactor Phase D;
//! bug 245's EARLIER section from `64743ee13`; pin `83334b9a5`).
//!
//! v4's why, carried: one renderer per window shape, one speaker map, one
//! truncation rule:
//!
//!  - **Turn windows** (`self-turn`, `other-turn`) render a [`TurnTranscript`]
//!    (`build_turn_transcript`) through [`render_turn_context`].
//!  - **Stretch windows** (`other-fold`, `episode-fold`) render a run of chat
//!    messages through [`render_stretch`]: speaker-labelled by the shared
//!    resolver ([`crate::services::speaker_names`]), dated, each message cut
//!    at [`STRETCH_MESSAGE_CHAR_CAP`] characters.
//!
//! And the fold rules both stretch passes share:
//!
//!  - **Eligibility.** Only a spoken USER/ASSISTANT line from a seat, with
//!    text, is read. Staff whispers, tool rows and empty messages are not.
//!  - **Cursor.** A pass reads the messages strictly after its own cursor
//!    through the end of the window it was handed, ordered by the chat's own
//!    message order ([`fold_pass_range`]) — so a stretch an earlier run lost
//!    to a timeout is read before the window, never jumped. A cursor that
//!    names a message no longer in the chat is treated as absent.
//!  - **Slicing.** A window wider than the pass's prompt bound is processed in
//!    consecutive slices, oldest first, and the cursor advances only past a
//!    slice that was read (bugs 206, 207). Nothing is marked covered unread.
//!
//! v5 shape: a message is the chat row as JSON (`serde_json::Value` — the
//! shape every v5 transcript reader already takes); the generic fold helpers
//! read only its `id`. Lengths and cuts are UTF-16 code units (`.length` /
//! `.slice`), `trim` is JS's whitespace class, `Date.parse` is V8's.

use serde_json::Value;

use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::memory_format::format_name_with_pronouns;
use crate::services::speaker_names::{speaker_label, SpeakerNames};
use crate::services::turn_transcript::{TurnCharacterSlice, TurnTranscript};

// ============================================================================
// Window shapes
// ============================================================================

/// One message of a stretch, speaker-labelled and dated (v4 `StretchMessage`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StretchMessage {
    pub speaker: String,
    pub content: String,
    pub created_at: Option<String>,
}

/// v4 `StretchWindow` — the `kind: 'stretch'` arm of [`TranscriptWindow`].
#[derive(Clone, Debug)]
pub struct StretchWindow {
    /// The eligible messages, chronological.
    pub messages: Vec<Value>,
    pub rendered: Vec<StretchMessage>,
    /// The newest stamped message of the stretch (the clock the model is
    /// shown), else now.
    pub now_iso: String,
    /// The oldest stamped message of the stretch, else `now_iso`.
    pub start_iso: String,
    /// The message the pass's cursor may advance to once this stretch is read.
    pub cover_through_message_id: Option<String>,
}

/// What a pass reads: one closed turn, or a stretch of conversation (v4
/// `TranscriptWindow`).
#[derive(Clone, Debug)]
pub enum TranscriptWindow {
    Turn {
        transcript: TurnTranscript,
        /// The turn's wall clock: the newest stamped message, else now.
        now_iso: String,
    },
    Stretch(StretchWindow),
}

impl TranscriptWindow {
    /// v4 `window.nowIso` — present on both arms.
    pub fn now_iso(&self) -> &str {
        match self {
            Self::Turn { now_iso, .. } => now_iso,
            Self::Stretch(s) => &s.now_iso,
        }
    }
}

/// One message is cut at this many characters inside a stretch.
pub const STRETCH_MESSAGE_CHAR_CAP: usize = 1500;

fn str_field<'a>(m: &'a Value, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

/// A message row's `id` (`""` when absent — every chat row carries one).
pub fn message_id(m: &Value) -> &str {
    str_field(m, "id").unwrap_or_default()
}

/// JS truthiness of a string-or-null row cell (`m.createdAt`, …).
fn truthy_str<'a>(m: &'a Value, key: &str) -> Option<&'a str> {
    str_field(m, key).filter(|s| !s.is_empty())
}

/// The first of `isos` that parses as a time, else `None` — an empty or
/// malformed stamp falls through (v4 `firstValidIso`: `!!iso &&
/// Number.isFinite(Date.parse(iso))`).
pub fn first_valid_iso(isos: &[Option<&str>]) -> Option<String> {
    isos.iter()
        .flatten()
        .find(|iso| !iso.is_empty() && crate::episodic::js_date_parse_ms(iso).is_some())
        .map(|s| s.to_string())
}

/// v4 `new Date(nowMs).toISOString()`.
fn now_iso_of(now_ms: f64) -> String {
    crate::clock::iso_from_unix_ms(now_ms as i64)
}

/// A stretch window over `messages` (already eligible), labelled with
/// `speaker_names` (v4 `stretchWindow`). `now_ms` is v4's defaulted
/// `Date.now()` — read only when no message is stamped.
pub fn stretch_window(
    messages: Vec<Value>,
    speaker_names: &SpeakerNames,
    cover_through_message_id: Option<String>,
    now_ms: f64,
) -> StretchWindow {
    let last_stamped = messages
        .iter()
        .rev()
        .find_map(|m| truthy_str(m, "createdAt"));
    let first_stamped = messages.iter().find_map(|m| truthy_str(m, "createdAt"));
    let now_iso = last_stamped
        .map(str::to_string)
        .unwrap_or_else(|| now_iso_of(now_ms));
    let rendered = messages
        .iter()
        .map(|m| StretchMessage {
            speaker: speaker_label(
                str_field(m, "participantId"),
                str_field(m, "role").unwrap_or_default(),
                speaker_names,
            ),
            content: str_field(m, "content").unwrap_or_default().to_string(),
            created_at: str_field(m, "createdAt").map(str::to_string),
        })
        .collect();
    let start_iso = first_stamped
        .map(str::to_string)
        .unwrap_or_else(|| now_iso.clone());
    StretchWindow {
        messages,
        rendered,
        now_iso,
        start_iso,
        cover_through_message_id,
    }
}

/// A turn window over a built transcript (v4 `turnWindow`).
pub fn turn_window(transcript: TurnTranscript, now_ms: f64) -> TranscriptWindow {
    let now_iso = transcript
        .turn_timestamp
        .clone()
        .unwrap_or_else(|| now_iso_of(now_ms));
    TranscriptWindow::Turn {
        transcript,
        now_iso,
    }
}

/// `[2026-07-14 09:30] Amy: …` — one line per message, the one stretch
/// rendering (v4 `renderStretch`). The stamp is the first 16 code units with
/// the FIRST `T` replaced (`String.prototype.replace` with a string
/// pattern), absent for a falsy `createdAt`.
pub fn render_stretch(messages: &[StretchMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            let stamp = match m.created_at.as_deref().filter(|s| !s.is_empty()) {
                Some(created_at) => format!(
                    "[{}] ",
                    utf16_truncate(created_at, 16).replacen('T', " ", 1)
                ),
                None => String::new(),
            };
            let content = if utf16_len(&m.content) > STRETCH_MESSAGE_CHAR_CAP {
                format!("{}…", utf16_truncate(&m.content, STRETCH_MESSAGE_CHAR_CAP))
            } else {
                m.content.clone()
            };
            format!("{stamp}{}: {content}", m.speaker)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ============================================================================
// Turn rendering
// ============================================================================

/// The transcript heading when the turn carries a human line (spec §8.2).
pub const ORDERED_TURN_TRANSCRIPT_HEADING: &str = "TURN TRANSCRIPT (in the order spoken — the USER's lines came first; nothing the USER says here answers anything a character says below it):";

/// The heading over a turn's already-extracted lines (bug 245): they explain
/// what the new lines answer, but every memory they hold has been formed.
pub const PRIOR_TURN_CONTEXT_HEADING: &str = "EARLIER IN THIS TURN (already noted — context only; form no memory from these lines, only from the TURN TRANSCRIPT below):";

/// v4's `` `${isUserControlled ? 'user' : 'ai'}:${characterId}` `` speaker key.
pub(crate) fn speaker_key(slice: &TurnCharacterSlice) -> String {
    format!(
        "{}:{}",
        if slice.is_user_controlled {
            "user"
        } else {
            "ai"
        },
        slice.character_id
    )
}

fn render_slice(slice: &TurnCharacterSlice) -> String {
    let role = if slice.is_user_controlled {
        "the user-controlled character"
    } else {
        "the character"
    };
    let character_label = format!(
        "{} ({role})",
        format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
    );
    format!("{character_label} says:\n\"{}\"", slice.text)
}

/// Render the participant roster + joined transcript for inclusion in
/// extraction prompts (v4 `renderTurnContext`). Single shared formatter so
/// the SELF and OTHER passes see byte-identical input prefixes.
pub fn render_turn_context(transcript: &TurnTranscript) -> String {
    // A user-controlled character arrives as a slice (built from the turn
    // opener). It is the human participant *and* a memory-forming character,
    // so it is listed on the USER line and its lines render once in the body
    // labeled "(the user-controlled character)" — never under the AI-character
    // roster and never duplicated as a standalone opener. AI slices keep their
    // existing labeling, so a turn with no user-controlled slice renders
    // byte-identically to before.
    // The roster names everyone who spoke in the turn, already-covered lines
    // included: an earlier speaker still observes what was said after them.
    let prior_slices: &[TurnCharacterSlice] = transcript.prior_slices.as_deref().unwrap_or(&[]);
    let mut seen_speakers: Vec<String> = Vec::new();
    let roster_slices: Vec<&TurnCharacterSlice> = prior_slices
        .iter()
        .chain(transcript.character_slices.iter())
        .filter(|s| {
            let key = speaker_key(s);
            if seen_speakers.contains(&key) {
                false
            } else {
                seen_speakers.push(key);
                true
            }
        })
        .collect();
    let ai_slices: Vec<&TurnCharacterSlice> = roster_slices
        .iter()
        .copied()
        .filter(|s| !s.is_user_controlled)
        .collect();
    let user_slices: Vec<&TurnCharacterSlice> = roster_slices
        .iter()
        .copied()
        .filter(|s| s.is_user_controlled)
        .collect();
    let has_user_slice = !user_slices.is_empty();

    let mut roster: Vec<String> = vec!["PARTICIPANTS IN THIS TURN:".to_string()];
    // v4 `userCharacterName ?? userSlices[0]?.characterName ?? null`, then a
    // truthy check — an empty name falls through to the plain-human line.
    let user_display_name = transcript
        .user_character_name
        .as_deref()
        .or_else(|| user_slices.first().map(|s| s.character_name.as_str()))
        .filter(|n| !n.is_empty());
    if let Some(name) = user_display_name {
        roster.push(format!("- USER: {name} (the human participant)"));
    } else if transcript.user_message.is_some() {
        roster.push("- USER: The human participant".to_string());
    }

    if ai_slices.len() == 1 {
        let slice = ai_slices[0];
        roster.push(format!(
            "- CHARACTER: {} (an AI character)",
            format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
        ));
    } else if ai_slices.len() > 1 {
        roster.push("- CHARACTERS (AI characters in this chat):".to_string());
        for slice in &ai_slices {
            roster.push(format!(
                "  * {}",
                format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
            ));
        }
    }

    let mut transcript_sections: Vec<String> = Vec::new();
    // The standalone opener renders only when no user slice carries it — i.e.
    // a plain human with no character. When a user slice exists, its body line
    // below is the single rendering of that text. (A plain human's opener
    // stays MATERIAL under the turn heading even when a bound moved the turn
    // past it — only a user-character opener becomes a prior slice; v4's
    // shape, ported as-is: work order P4.D296 L-5.)
    if let Some(user_message) = transcript.user_message.as_deref() {
        if !has_user_slice {
            // v4 `userCharacterName ? … : 'The user'` — truthy.
            let user_label = match transcript
                .user_character_name
                .as_deref()
                .filter(|n| !n.is_empty())
            {
                Some(name) => format!("{name} (the user)"),
                None => "The user".to_string(),
            };
            transcript_sections.push(format!("{user_label} says:\n\"{user_message}\""));
        }
    }
    for slice in &transcript.character_slices {
        transcript_sections.push(render_slice(slice));
    }

    // By construction the human's line opens the turn and the next human line
    // is not in it, so nothing the human says here answers a character below.
    // Saying so is what keeps the extractor from manufacturing assent. A turn
    // with no human line keeps the plain heading, byte-identical to before.
    let heading = if transcript.user_message.is_some() || has_user_slice {
        ORDERED_TURN_TRANSCRIPT_HEADING
    } else {
        "TURN TRANSCRIPT:"
    };

    // Lines an earlier extraction already covered (bug 245): context for what
    // follows, never material. Absent, the output is byte-identical to before.
    let earlier = if prior_slices.is_empty() {
        String::new()
    } else {
        format!(
            "{PRIOR_TURN_CONTEXT_HEADING}\n\n{}\n\n",
            prior_slices
                .iter()
                .map(render_slice)
                .collect::<Vec<_>>()
                .join("\n\n")
        )
    };

    format!(
        "{}\n\n{earlier}{heading}\n\n{}",
        roster.join("\n"),
        transcript_sections.join("\n\n")
    )
}

// ============================================================================
// Fold rules
// ============================================================================

/// A message that carries conversation: a spoken USER/ASSISTANT line from a
/// seat, not staff or tool output (v4 `isFoldEligibleMessage`).
pub fn is_fold_eligible_message(m: &Value) -> bool {
    if str_field(m, "type") != Some("message") {
        return false;
    }
    let role = str_field(m, "role");
    if role != Some("USER") && role != Some("ASSISTANT") {
        return false;
    }
    if crate::services::turn_transcript::is_truthy_system_sender(m.get("systemSender")) {
        return false;
    }
    if !crate::services::turn_transcript::is_js_truthy(m.get("participantId")) {
        return false;
    }
    match m.get("content") {
        Some(Value::String(s)) => !js_trim(s).is_empty(),
        _ => false,
    }
}

/// Messages strictly after the cursor (v4 `messagesPastCursor`). A cursor
/// that names a message no longer in the chat (deleted) is treated as absent
/// — better to re-read the tail once than to go blind forever.
pub fn messages_past_cursor(all_messages: &[Value], cursor_message_id: Option<&str>) -> Vec<Value> {
    let Some(cursor) = cursor_message_id.filter(|c| !c.is_empty()) else {
        return all_messages.to_vec();
    };
    match all_messages.iter().position(|m| message_id(m) == cursor) {
        None => all_messages.to_vec(),
        Some(idx) => all_messages[idx + 1..].to_vec(),
    }
}

/// v4 `windowPastCursor`'s `{ window, applied }`.
#[derive(Clone, Debug)]
pub struct WindowPastCursor {
    pub window: Vec<Value>,
    pub applied: bool,
}

/// Cut a window (any subset of the chat's messages) to what lies strictly
/// after the cursor, using `all_messages` (the chat's messages,
/// chronological) for order (v4 `windowPastCursor`). The window comes back
/// unchanged when there is no cursor or the cursor names a message the chat
/// no longer has. A window row the chat does not know keeps its place
/// (`order.get(id) ?? Infinity`).
pub fn window_past_cursor(
    window: &[Value],
    all_messages: &[Value],
    cursor_message_id: Option<&str>,
) -> WindowPastCursor {
    let Some(cursor) = cursor_message_id.filter(|c| !c.is_empty()) else {
        return WindowPastCursor {
            window: window.to_vec(),
            applied: false,
        };
    };
    let Some(cursor_idx) = all_messages.iter().position(|m| message_id(m) == cursor) else {
        return WindowPastCursor {
            window: window.to_vec(),
            applied: false,
        };
    };
    // v4 `new Map(allMessages.map((m, i) => [m.id, i]))` — a later duplicate
    // id overwrites an earlier one, so the LAST index wins.
    let order =
        |id: &str| -> Option<usize> { all_messages.iter().rposition(|m| message_id(m) == id) };
    WindowPastCursor {
        window: window
            .iter()
            .filter(|m| order(message_id(m)).is_none_or(|i| i > cursor_idx))
            .cloned()
            .collect(),
        applied: true,
    }
}

/// One slice of a fold window, and the message the cursor may advance to
/// once it is read (v4 `FoldWindowSlice`).
#[derive(Clone, Debug)]
pub struct FoldWindowSlice {
    pub messages: Vec<Value>,
    pub cover_through_message_id: Option<String>,
}

/// Split eligible messages into consecutive slices of at most
/// `max_per_slice`, oldest first (v4 `sliceFoldWindow`). Every slice but the
/// last covers through its own last message; the last covers through
/// `final_cover_through_id` (the end of the untrimmed window, which may be an
/// ineligible row) when given. Messages between two slices are ineligible by
/// construction, so covering through a slice's last eligible message skips
/// nothing a pass would have read.
pub fn slice_fold_window(
    eligible: &[Value],
    max_per_slice: f64,
    final_cover_through_id: Option<&str>,
) -> Vec<FoldWindowSlice> {
    // v4 `Math.max(1, Math.floor(maxPerSlice))` — an infinite bound is one
    // slice of everything.
    let size_f = crate::jsnum::math_max(1.0, max_per_slice.floor());
    let size = if size_f.is_finite() {
        size_f as usize
    } else {
        usize::MAX
    };
    let mut slices = Vec::new();
    let mut start = 0usize;
    while start < eligible.len() {
        let end = start.saturating_add(size).min(eligible.len());
        let messages = eligible[start..end].to_vec();
        let is_last = start.saturating_add(size) >= eligible.len();
        let own_last = messages.last().map(|m| message_id(m).to_string());
        slices.push(FoldWindowSlice {
            cover_through_message_id: if is_last {
                final_cover_through_id.map(str::to_string).or(own_last)
            } else {
                own_last
            },
            messages,
        });
        start = start.saturating_add(size);
    }
    slices
}

/// What a fold pass should read this run (v4 `FoldPassRange`).
#[derive(Clone, Debug)]
pub struct FoldPassRange {
    pub messages: Vec<Value>,
    /// True when the cursor shaped the range (it named a message still in the
    /// chat).
    pub applied: bool,
    /// Messages in the range that were not in the window handed over — an
    /// unread stretch the pass is catching up on.
    pub gap: usize,
}

/// The stretch a fold-grain pass reads (v4 `foldPassRange`): the messages
/// strictly after its cursor, through the last message of the window it was
/// handed, in the chat's own order. A pass never reads only the window,
/// because the window is the *fold's* — a slice the pass lost to a timeout
/// lies before it, and reading the window alone would carry the cursor over
/// that slice for good. Without a cursor (or with one naming a vanished
/// message) the window is the range, as before: a pass that has never run
/// does not backfill all history on the fold path.
pub fn fold_pass_range(
    window: &[Value],
    all_messages: &[Value],
    cursor_message_id: Option<&str>,
) -> FoldPassRange {
    let unchanged = || FoldPassRange {
        messages: window.to_vec(),
        applied: false,
        gap: 0,
    };
    let Some(cursor) = cursor_message_id.filter(|c| !c.is_empty()) else {
        return unchanged();
    };
    if window.is_empty() {
        return unchanged();
    }
    let Some(cursor_idx) = all_messages.iter().position(|m| message_id(m) == cursor) else {
        return unchanged();
    };
    let last_id = message_id(window.last().expect("non-empty window"));
    let Some(end_idx) = all_messages.iter().position(|m| message_id(m) == last_id) else {
        // The window's end is not in the transcript we were given; fall back
        // to cutting the window itself.
        let cut = window_past_cursor(window, all_messages, Some(cursor));
        return FoldPassRange {
            messages: cut.window,
            applied: cut.applied,
            gap: 0,
        };
    };
    if end_idx <= cursor_idx {
        return FoldPassRange {
            messages: Vec::new(),
            applied: true,
            gap: 0,
        };
    }
    let messages = all_messages[cursor_idx + 1..=end_idx].to_vec();
    let in_window: std::collections::HashSet<&str> = window.iter().map(message_id).collect();
    let gap = messages
        .iter()
        .filter(|m| !in_window.contains(message_id(m)))
        .count();
    FoldPassRange {
        messages,
        applied: true,
        gap,
    }
}
