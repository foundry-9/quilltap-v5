//! The established-fact reminder — a trailing section that repeats, once, every
//! established fact posted since this seat last spoke. Port of v4
//! `lib/chat/context/established-fact-reminder.ts` (P4.D372, v4 `db6190b44`,
//! pin `624d0d25c`).
//!
//! The facts are already in the history the seat reads, framed in place
//! (`message_context`'s frame pass). This section is the "since you last spoke"
//! nudge: the last thing the model reads before it answers, under one vouching
//! header ([`ESTABLISHED_FACT_REMINDER_HEADER`]), so a fact posted three lines up
//! is not outvoted by an older memory recalled in the same prompt.
//!
//! - **Select:** every fact row (both kinds, public or addressed to this seat)
//!   newer than the seat's last own turn
//!   ([`crate::core_whisper::find_last_own_turn_ms`]). With no prior own turn:
//!   every fact since the seat joined, or since the start for a seat with
//!   history access (or with no join time).
//! - **Format:** posting order; a seat line is prefixed with its speaker's
//!   name, a machine fact is not. Bodies verbatim (the RAW body — the frame is
//!   the history's, the header is this section's).
//! - **Cap:** [`ESTABLISHED_FACT_REMINDER_CAP`] UTF-16 units over the header
//!   plus every item and its `\n\n` join, the OLDEST dropped first while more
//!   than one remains. The newest fact always rides; dropped facts stay in
//!   history.
//! - **Empty-is-absent:** no facts, no section — and the speaker-name loader is
//!   never called.
//! - **Never writes.** Nothing is stored or consumed; the cadence is derived
//!   from history, as progressions' is.
//! - **Swipes:** with `regeneration_of_message_ids`, the history is cut at the
//!   first re-rolled MESSAGE row, so "last own turn" is measured on what the
//!   original generation saw and the re-roll gets the same reminder. An id
//!   that is not found cuts nothing.
//!
//! Every `Date.parse` is [`crate::episodic::js_date_parse_ms`] (`None` = NaN).
//! Two of v4's NaN behaviours are kept exactly: a fact whose `createdAt` will
//! not parse is DROPPED when there is a floor (`!Number.isFinite(ms)`), and an
//! unparseable `joinedAt` gives a NaN floor — against which `ms <= NaN` is
//! false, so every parseable fact is KEPT.
//!
//! Recorded unreachable (P4.D372 Tier 3 item 29): v4's template renders a
//! seat line's absent `content` as `undefined` (and a machine fact's absent
//! `content` throws on `.length`). No such row exists — a seat line needs
//! words (the send refine) and a machine fact is trimmed non-empty by
//! `post_established_fact` — so there is no arm here; an absent body reads as
//! empty.

use crate::core_whisper::{find_last_own_turn_ms, WhisperEvent};
use crate::established_fact::{
    established_fact_kind_of, EstablishedFactFields, EstablishedFactKind,
    ESTABLISHED_FACT_REMINDER_HEADER,
};
use crate::services::speaker_names::{speaker_label, SpeakerNames, SpeakerRow};

/// v4 `ESTABLISHED_FACT_REMINDER_CAP` — the reminder never grows past this many
/// characters (UTF-16 units); oldest facts go first.
pub const ESTABLISHED_FACT_REMINDER_CAP: usize = 8000;

/// v4 `BuildEstablishedFactReminderInput` (the names loader is the second
/// argument of [`build_established_fact_reminder`]).
pub struct ReminderInput<'a> {
    pub events: &'a [WhisperEvent],
    /// The responding seat (a chat participant id).
    pub responding_participant_id: &'a str,
    /// The seat's join time (`participant.createdAt`); ignored with history
    /// access. `None` and `""` are both v4's falsy `!input.joinedAt`.
    pub joined_at: Option<&'a str>,
    pub has_history_access: bool,
    /// Swipe re-apply: the message being re-rolled plus its whole swipe group.
    pub regeneration_of_message_ids: Option<&'a [String]>,
    /// v4's optional `chatId` — OMITTED from the debug line when `None`.
    pub chat_id: Option<&'a str>,
}

fn fact_fields(e: &WhisperEvent) -> EstablishedFactFields<'_> {
    EstablishedFactFields {
        is_established_fact: e.is_established_fact,
        system_sender: e.system_sender.as_deref(),
        system_kind: e.system_kind.as_deref(),
    }
}

/// v4 `msOf(value)` — `Date.parse(String(value))` (an absent value stringifies
/// to `"undefined"`, NaN either way).
fn ms_of(value: Option<&str>) -> f64 {
    value
        .and_then(crate::episodic::js_date_parse_ms)
        .map_or(f64::NAN, |ms| ms as f64)
}

/// v4 `buildEstablishedFactReminder` — the reminder section, or `""` when there
/// is nothing to remind. `load_speaker_names` is called at most once, and only
/// when a seat line is selected; its `Err` (and nothing else) is this
/// function's `Err`, which the caller turns into v4's WARN and `""`.
pub fn build_established_fact_reminder(
    input: &ReminderInput<'_>,
    load_speaker_names: impl FnOnce() -> Result<SpeakerNames, String>,
) -> Result<String, String> {
    let regen = input.regeneration_of_message_ids.unwrap_or(&[]);
    let is_swipe = !regen.is_empty();
    let mut events = input.events;
    if is_swipe {
        if let Some(cut) = events.iter().position(|e| {
            e.event_type == "message"
                && e.id
                    .as_deref()
                    .is_some_and(|id| regen.iter().any(|r| r == id))
        }) {
            events = &events[..cut];
        }
    }

    let floor_ms: Option<f64> = match find_last_own_turn_ms(events, input.responding_participant_id)
    {
        Some(ms) => Some(ms as f64),
        None if input.has_history_access || input.joined_at.is_none_or(str::is_empty) => None,
        None => Some(ms_of(input.joined_at)),
    };

    let mut facts: Vec<(&WhisperEvent, EstablishedFactKind)> = Vec::new();
    for e in events {
        if e.event_type != "message" {
            continue;
        }
        let Some(kind) = established_fact_kind_of(&fact_fields(e)) else {
            continue;
        };
        if let Some(targets) = &e.target_participant_ids {
            if !targets.is_empty() && !targets.iter().any(|t| t == input.responding_participant_id)
            {
                continue;
            }
        }
        if let Some(floor) = floor_ms {
            let ms = ms_of(e.created_at.as_deref());
            // v4 `!Number.isFinite(ms) || ms <= floorMs` (a NaN floor makes
            // the comparison false, so the fact is kept).
            if !ms.is_finite() || ms <= floor {
                continue;
            }
        }
        facts.push((e, kind));
    }
    if facts.is_empty() {
        return Ok(String::new());
    }

    let has_seat_line = facts.iter().any(|(_, k)| *k == EstablishedFactKind::Seat);
    let names = if has_seat_line {
        load_speaker_names()?
    } else {
        SpeakerNames::default()
    };
    let items: Vec<String> = facts
        .iter()
        .map(|(m, kind)| {
            let body = m.content.as_deref().unwrap_or_default();
            match kind {
                EstablishedFactKind::Seat => {
                    let label = speaker_label(
                        &SpeakerRow {
                            participant_id: m.participant_id.as_deref(),
                            role: m.role.as_deref().unwrap_or(""),
                            system_sender: m.system_sender.as_deref(),
                            system_kind: m.system_kind.as_deref(),
                        },
                        &names,
                    );
                    format!("{label}: {body}")
                }
                EstablishedFactKind::Announcement => body.to_string(),
            }
        })
        .collect();

    // Cap: drop the oldest until the section fits. The newest fact always rides.
    use crate::jsstr::utf16_len;
    let mut start = 0usize;
    let mut length = utf16_len(ESTABLISHED_FACT_REMINDER_HEADER)
        + items.iter().map(|i| utf16_len(i) + 2).sum::<usize>();
    while items.len() - start > 1 && length > ESTABLISHED_FACT_REMINDER_CAP {
        length -= utf16_len(&items[start]) + 2;
        start += 1;
    }
    let kept = &items[start..];
    let dropped = start;

    match input.chat_id {
        Some(chat_id) => tracing::debug!(
            chatId = %chat_id,
            participantId = %input.responding_participant_id,
            selected = facts.len(),
            droppedByCap = dropped,
            isSwipe = is_swipe,
            "[EstablishedFact] Built the since-you-last-spoke reminder"
        ),
        None => tracing::debug!(
            participantId = %input.responding_participant_id,
            selected = facts.len(),
            droppedByCap = dropped,
            isSwipe = is_swipe,
            "[EstablishedFact] Built the since-you-last-spoke reminder"
        ),
    }

    let mut parts: Vec<&str> = Vec::with_capacity(kept.len() + 1);
    parts.push(ESTABLISHED_FACT_REMINDER_HEADER);
    parts.extend(kept.iter().map(String::as_str));
    Ok(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, role: &str, pid: Option<&str>, content: &str, at: &str) -> WhisperEvent {
        WhisperEvent {
            event_type: "message".into(),
            id: Some(id.into()),
            role: Some(role.into()),
            participant_id: pid.map(str::to_string),
            content: Some(content.into()),
            created_at: Some(at.into()),
            ..Default::default()
        }
    }

    fn machine(id: &str, content: &str, at: &str) -> WhisperEvent {
        WhisperEvent {
            system_sender: Some("host".into()),
            system_kind: Some("established-fact".into()),
            ..ev(id, "ASSISTANT", None, content, at)
        }
    }

    fn input<'a>(events: &'a [WhisperEvent]) -> ReminderInput<'a> {
        ReminderInput {
            events,
            responding_participant_id: "p-a",
            joined_at: None,
            has_history_access: true,
            regeneration_of_message_ids: None,
            chat_id: Some("chat-1"),
        }
    }

    #[test]
    fn no_fact_is_empty_and_never_loads_names() {
        let events = [ev(
            "m1",
            "USER",
            Some("p-u"),
            "hi",
            "2026-01-01T00:00:00.000Z",
        )];
        let out = build_established_fact_reminder(&input(&events), || panic!("loaded")).unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn the_debug_line_and_its_silence_leg() {
        let events = [machine(
            "f1",
            "The bell is cracked.",
            "2026-01-01T00:00:00.000Z",
        )];
        let (out, lines) = crate::test_support::captured_with(|| {
            build_established_fact_reminder(&input(&events), || panic!("no seat line")).unwrap()
        });
        assert_eq!(
            out,
            format!("{ESTABLISHED_FACT_REMINDER_HEADER}\n\nThe bell is cracked.")
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].ends_with(
            "[EstablishedFact] Built the since-you-last-spoke reminder chatId=chat-1 participantId=p-a selected=1 droppedByCap=0 isSwipe=false"
        ), "{}", lines[0]);
        // chatId omitted when absent (v4's `undefined`).
        let no_chat = ReminderInput {
            chat_id: None,
            ..input(&events)
        };
        let (_, lines) = crate::test_support::captured_with(|| {
            build_established_fact_reminder(&no_chat, || panic!()).unwrap()
        });
        assert!(!lines[0].contains("chatId"), "{}", lines[0]);
        // Silence: no fact, no line.
        let quiet = [ev(
            "m1",
            "USER",
            Some("p-u"),
            "hi",
            "2026-01-01T00:00:00.000Z",
        )];
        let (_, lines) = crate::test_support::captured_with(|| {
            build_established_fact_reminder(&input(&quiet), || panic!()).unwrap()
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn a_loader_error_is_the_err() {
        let mut seat = ev("s1", "USER", Some("p-u"), "x", "2026-01-01T00:00:00.000Z");
        seat.is_established_fact = Some(true);
        let events = [seat];
        let r = build_established_fact_reminder(&input(&events), || Err("boom".into()));
        assert_eq!(r, Err("boom".into()));
    }
}
