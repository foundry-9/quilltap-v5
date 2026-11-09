//! Established facts — the one place that recognises a fact row and knows the
//! words a model reads around one. Port of v4 `lib/chat/established-fact.ts`
//! (P4.D372, v4 `db6190b44`, pin `624d0d25c`).
//!
//! The operator can declare something **true** in a Salon chat two ways:
//!
//! - **A seat line.** Their own USER row, flagged `isEstablishedFact`. It keeps
//!   its speaker; the frame says what follows is true and known to everyone.
//! - **A machine fact.** A Host row (`systemSender: 'host'`,
//!   `systemKind: 'established-fact'`, no participant, public). It belongs to no
//!   speaker. Transparent rosters hear the Host deliver it; opaque rosters hear
//!   the plain frame with no speaker at all.
//!
//! Two rules give this module its shape (v4's own words):
//!
//! **The frame is applied at read time, never stored.** Stored bodies are what
//! the operator typed, so edits need nothing special and a wording change here
//! applies to every chat at once. A machine fact's frame reads `content` and
//! ignores the stored `opaqueContent`, so the two bodies cannot drift.
//!
//! **The UI name never reaches a model.** "Deus Ex Machina" is a label in the
//! Salon and nowhere else; everything here says *established fact*. The
//! harness's `deus_ex_machina_name_leak_guard` scans every string literal under
//! `crates/*/src` for the name, and [`leak_tests`] walks a fact through every
//! model-facing path this crate builds.
//!
//! Pure: no logger, no repositories. Design of record (v4):
//! `docs/developer/features/deus-ex-machina.md`.
//!
//! **Recorded unreachable (P4.D372 Tier 3 item 29):** v4's reminder renders a
//! machine fact as the template `${m.content}`, which would print `undefined`
//! for a fact row with no `content`. No such row exists: a seat line needs
//! words (the send refine "An established fact needs words") and a machine
//! fact is trimmed non-empty by `postEstablishedFact`. No arm here.

use serde_json::Value;

/// v4 `ESTABLISHED_FACT_KIND` — `systemKind` of a machine-delivered fact (a Host row).
pub const ESTABLISHED_FACT_KIND: &str = "established-fact";

/// v4 `ESTABLISHED_FACT_SPEAKER_LABEL` — fixed speaker label for a machine fact
/// in a labelled transcript (fold, Courier).
pub const ESTABLISHED_FACT_SPEAKER_LABEL: &str = "Established fact";

/// v4 `ESTABLISHED_FACT_SEAT_FRAME` — opening line of a seat line's frame.
pub const ESTABLISHED_FACT_SEAT_FRAME: &str =
    "(Established fact: what follows is true in this story and known to everyone; it needs no verification.)";

/// v4 `ESTABLISHED_FACT_HOST_FRAME` — opening line of a machine fact's frame,
/// transparent roster: the Host delivers it.
pub const ESTABLISHED_FACT_HOST_FRAME: &str =
    "The Host makes it known, as established fact (true in this story, known to everyone, needing no verification):";

/// v4 `ESTABLISHED_FACT_PLAIN_FRAME` — opening line of a machine fact's frame,
/// opaque roster: no speaker at all.
pub const ESTABLISHED_FACT_PLAIN_FRAME: &str =
    "Established fact (true in this story, known to everyone, needing no verification):";

/// v4 `ESTABLISHED_FACT_REMINDER_HEADER` — header of the trailing "since you last
/// spoke" reminder. Second person: read in the seat's own prompt. v4 builds it
/// as a `'…' + '…'` concatenation (one space at the join).
pub const ESTABLISHED_FACT_REMINDER_HEADER: &str = concat!(
    "Established since you last spoke: true in this story, already known to you, needing no verification, ",
    "and outranking any older memory or record that disagrees:"
);

/// v4 `ESTABLISHED_FACT_TRANSCRIPT_RULE` — the rule the cheap-LLM passes that
/// read transcripts (the summary fold, memory extraction) are given, so a fact
/// survives as a fact of the world. A concatenation in v4 (one space at the join).
pub const ESTABLISHED_FACT_TRANSCRIPT_RULE: &str = concat!(
    "Lines labelled or marked as established fact are settled truth in this story. ",
    "Carry them forward as facts of the world, not as anyone's claim, and do not let a later line's doubt undo them."
);

/// v4 `EstablishedFactKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishedFactKind {
    Seat,
    Announcement,
}

impl EstablishedFactKind {
    /// v4's string spelling (`'seat' | 'announcement'`).
    pub fn as_str(self) -> &'static str {
        match self {
            EstablishedFactKind::Seat => "seat",
            EstablishedFactKind::Announcement => "announcement",
        }
    }
}

/// v4 `EstablishedFactVoice`: `host` names the Host as deliverer (transparent
/// roster); `plain` names nobody (opaque roster, cheap-LLM transcripts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishedFactVoice {
    Host,
    Plain,
}

impl EstablishedFactVoice {
    /// v4's string spelling (`'host' | 'plain'`) — the `voice` field of the
    /// `[Context] Framed established facts` debug line.
    pub fn as_str(self) -> &'static str {
        match self {
            EstablishedFactVoice::Host => "host",
            EstablishedFactVoice::Plain => "plain",
        }
    }
}

/// v4 `EstablishedFactFields` — the fields a fact predicate reads. Any message
/// shape carries them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EstablishedFactFields<'a> {
    /// `Some(b)` only for a JSON boolean: v4's `=== true` means a `1` or a
    /// `"true"` is NOT a seat line.
    pub is_established_fact: Option<bool>,
    /// `None` for an absent, `null` or non-string sender.
    pub system_sender: Option<&'a str>,
    pub system_kind: Option<&'a str>,
}

impl<'a> EstablishedFactFields<'a> {
    /// Read the three fields off a raw row (a `ChatEvent` / message JSON).
    pub fn from_value(v: &'a Value) -> Self {
        EstablishedFactFields {
            is_established_fact: v.get("isEstablishedFact").and_then(Value::as_bool),
            system_sender: v.get("systemSender").and_then(Value::as_str),
            system_kind: v.get("systemKind").and_then(Value::as_str),
        }
    }
}

/// v4 `isEstablishedFactAnnouncement` — true only for the machine-delivered kind
/// (a Host fact row).
pub fn is_established_fact_announcement(m: &EstablishedFactFields<'_>) -> bool {
    m.system_sender == Some("host") && m.system_kind == Some(ESTABLISHED_FACT_KIND)
}

/// v4 `isEstablishedFactSeatLine` — true only for a seat line (an operator USER
/// row posted as fact). `!m.systemSender`: an absent, `null` or EMPTY sender
/// counts as no sender.
pub fn is_established_fact_seat_line(m: &EstablishedFactFields<'_>) -> bool {
    m.is_established_fact == Some(true) && m.system_sender.is_none_or(str::is_empty)
}

/// v4 `isEstablishedFact` — true for either kind.
pub fn is_established_fact(m: &EstablishedFactFields<'_>) -> bool {
    is_established_fact_seat_line(m) || is_established_fact_announcement(m)
}

/// v4 `establishedFactKindOf` — which kind a row is, or `None` when it is not a
/// fact. The ANNOUNCEMENT is tested first, so a Host fact row that also carries
/// the seat flag is a machine fact.
pub fn established_fact_kind_of(m: &EstablishedFactFields<'_>) -> Option<EstablishedFactKind> {
    if is_established_fact_announcement(m) {
        return Some(EstablishedFactKind::Announcement);
    }
    if is_established_fact_seat_line(m) {
        return Some(EstablishedFactKind::Seat);
    }
    None
}

fn opener(kind: EstablishedFactKind, voice: EstablishedFactVoice) -> &'static str {
    match (kind, voice) {
        (EstablishedFactKind::Seat, _) => ESTABLISHED_FACT_SEAT_FRAME,
        (EstablishedFactKind::Announcement, EstablishedFactVoice::Host) => {
            ESTABLISHED_FACT_HOST_FRAME
        }
        (EstablishedFactKind::Announcement, EstablishedFactVoice::Plain) => {
            ESTABLISHED_FACT_PLAIN_FRAME
        }
    }
}

/// v4 `frameEstablishedFact` — wrap a row's body for a model. The body goes
/// through verbatim; only the opening line differs by kind and voice. `voice`
/// is ignored for a seat line, which keeps its own speaker.
pub fn frame_established_fact(
    body: &str,
    kind: EstablishedFactKind,
    voice: EstablishedFactVoice,
) -> String {
    format!(
        "{}\n\n{}",
        opener(kind, voice),
        strip_established_fact_frame(body)
    )
}

/// v4 `ALL_OPENERS` — the strip's search order.
const ALL_OPENERS: [&str; 3] = [
    ESTABLISHED_FACT_SEAT_FRAME,
    ESTABLISHED_FACT_HOST_FRAME,
    ESTABLISHED_FACT_PLAIN_FRAME,
];

/// v4 `stripEstablishedFactFrame` — remove a frame this module put on a body,
/// so framing is idempotent: a pass that runs twice (or re-voices an
/// already-framed body) never stacks openers. The FIRST opener whose
/// `opener + "\n\n"` prefixes the body is sliced off. v4 slices
/// `opener.length + 2` UTF-16 units; all three openers are ASCII, so the byte
/// slice is the same cut. An operator body that merely STARTS with an opener
/// and a blank line is stripped too (v4's own behaviour).
pub fn strip_established_fact_frame(body: &str) -> &str {
    for o in ALL_OPENERS {
        if let Some(rest) = body.strip_prefix(o) {
            if let Some(rest) = rest.strip_prefix("\n\n") {
                return rest;
            }
        }
    }
    body
}

/// v4 `framedEstablishedFactBody` — the framed body of a fact row, or `None`
/// when the row is not a fact. A machine fact frames `content` (absent → `''`)
/// and never the stored `opaqueContent`.
pub fn framed_established_fact_body(
    m: &EstablishedFactFields<'_>,
    content: Option<&str>,
    voice: EstablishedFactVoice,
) -> Option<String> {
    let kind = established_fact_kind_of(m)?;
    Some(frame_established_fact(content.unwrap_or(""), kind, voice))
}

/// [`framed_established_fact_body`] over a raw row (`content` read as a string).
pub fn framed_established_fact_body_of_value(
    row: &Value,
    voice: EstablishedFactVoice,
) -> Option<String> {
    framed_established_fact_body(
        &EstablishedFactFields::from_value(row),
        row.get("content").and_then(Value::as_str),
        voice,
    )
}

/// v4 `frameEstablishedFacts` — frame every fact row in a history, in place of
/// its body. Pure and idempotent; rows that are not facts pass through
/// untouched. A machine fact gets the framed body in both `content` and
/// `opaqueContent`, so whichever body a later Staff swap picks, it reads the
/// right frame; a seat line only in `content`.
///
/// `None` is v4's "the SAME array" — returned only when the history holds no
/// fact at all (v4 sets `changed` on every fact row, even one already framed).
/// The keys follow v4's spread: an existing `content` / `opaqueContent` keeps
/// its position, an absent one is appended (`content` first).
pub fn frame_established_facts(rows: &[Value], voice: EstablishedFactVoice) -> Option<Vec<Value>> {
    let mut changed = false;
    let out: Vec<Value> = rows
        .iter()
        .map(|m| {
            let Some(kind) = established_fact_kind_of(&EstablishedFactFields::from_value(m)) else {
                return m.clone();
            };
            changed = true;
            let framed = frame_established_fact(
                m.get("content").and_then(Value::as_str).unwrap_or(""),
                kind,
                voice,
            );
            let mut next = m.clone();
            if let Some(obj) = next.as_object_mut() {
                obj.insert("content".into(), Value::String(framed.clone()));
                if kind == EstablishedFactKind::Announcement {
                    obj.insert("opaqueContent".into(), Value::String(framed));
                }
            }
            next
        })
        .collect();
    changed.then_some(out)
}

/// v4 `countEstablishedFacts` — `(seat, announcement)`, for the debug lines on
/// every path that frames.
pub fn count_established_facts<'a, I>(rows: I) -> (usize, usize)
where
    I: IntoIterator<Item = EstablishedFactFields<'a>>,
{
    let mut seat = 0;
    let mut announcement = 0;
    for m in rows {
        match established_fact_kind_of(&m) {
            Some(EstablishedFactKind::Seat) => seat += 1,
            Some(EstablishedFactKind::Announcement) => announcement += 1,
            None => {}
        }
    }
    (seat, announcement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn f<'a>(
        flag: Option<bool>,
        sender: Option<&'a str>,
        kind: Option<&'a str>,
    ) -> EstablishedFactFields<'a> {
        EstablishedFactFields {
            is_established_fact: flag,
            system_sender: sender,
            system_kind: kind,
        }
    }

    #[test]
    fn the_concatenated_constants_carry_one_space_at_the_join() {
        assert!(
            ESTABLISHED_FACT_REMINDER_HEADER.contains("needing no verification, and outranking")
        );
        assert!(ESTABLISHED_FACT_TRANSCRIPT_RULE.contains("in this story. Carry them"));
    }

    #[test]
    fn the_announcement_is_tested_before_the_seat() {
        let both = f(Some(true), Some("host"), Some(ESTABLISHED_FACT_KIND));
        assert_eq!(
            established_fact_kind_of(&both),
            Some(EstablishedFactKind::Announcement)
        );
    }

    #[test]
    fn an_empty_sender_counts_as_absent_for_a_seat_line() {
        assert!(is_established_fact_seat_line(&f(
            Some(true),
            Some(""),
            None
        )));
        assert!(!is_established_fact_seat_line(&f(
            Some(true),
            Some("aurora"),
            None
        )));
    }

    #[test]
    fn a_non_boolean_flag_is_not_a_seat_line() {
        let one = json!({"isEstablishedFact": 1});
        let s = json!({"isEstablishedFact": "true"});
        assert!(!is_established_fact(&EstablishedFactFields::from_value(
            &one
        )));
        assert!(!is_established_fact(&EstablishedFactFields::from_value(&s)));
    }

    #[test]
    fn framing_is_idempotent_for_every_opener() {
        for (kind, voice) in [
            (EstablishedFactKind::Seat, EstablishedFactVoice::Plain),
            (
                EstablishedFactKind::Announcement,
                EstablishedFactVoice::Host,
            ),
            (
                EstablishedFactKind::Announcement,
                EstablishedFactVoice::Plain,
            ),
        ] {
            let once = frame_established_fact("body", kind, voice);
            assert_eq!(frame_established_fact(&once, kind, voice), once);
        }
        // Re-voicing a host-framed body to plain swaps the opener, never stacks.
        let host = frame_established_fact(
            "x",
            EstablishedFactKind::Announcement,
            EstablishedFactVoice::Host,
        );
        assert_eq!(
            frame_established_fact(
                &host,
                EstablishedFactKind::Announcement,
                EstablishedFactVoice::Plain
            ),
            format!("{ESTABLISHED_FACT_PLAIN_FRAME}\n\nx")
        );
    }

    #[test]
    fn an_opener_with_one_newline_is_not_stripped() {
        let b = format!("{ESTABLISHED_FACT_PLAIN_FRAME}\nx");
        assert_eq!(strip_established_fact_frame(&b), b);
    }

    #[test]
    fn frame_established_facts_returns_none_when_nothing_is_a_fact() {
        let rows = vec![json!({"role": "USER", "content": "hi"})];
        assert!(frame_established_facts(&rows, EstablishedFactVoice::Host).is_none());
    }

    #[test]
    fn a_machine_fact_gains_opaque_content_appended_when_absent() {
        let rows =
            vec![json!({"systemSender": "host", "systemKind": "established-fact", "content": "c"})];
        let out = frame_established_facts(&rows, EstablishedFactVoice::Plain).unwrap();
        let keys: Vec<_> = out[0].as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["systemSender", "systemKind", "content", "opaqueContent"]
        );
        assert_eq!(out[0]["opaqueContent"], out[0]["content"]);
    }

    #[test]
    fn counts_split_by_kind() {
        let rows = [
            f(Some(true), None, None),
            f(None, Some("host"), Some(ESTABLISHED_FACT_KIND)),
            f(Some(false), None, None),
        ];
        assert_eq!(count_established_facts(rows), (1, 1));
    }
}

/// The behavioural half of the name-leak guard (v4
/// `__tests__/unit/lib/chat/established-fact-leaks.test.ts`): one seat line and
/// one machine fact walked through every model-facing path this crate builds,
/// each output carrying the right frame and never the UI name. The rows grow
/// with each read path P4.D372 lands.
#[cfg(test)]
pub(crate) mod leak_tests {
    use super::*;
    use serde_json::{json, Value};

    /// v4's `UI_NAME = /deus\s+ex\s+machina/i`.
    pub(crate) fn names_the_feature(text: &str) -> bool {
        regex::Regex::new(r"(?i)deus\s+ex\s+machina")
            .unwrap()
            .is_match(text)
    }

    pub(crate) const SEAT_FACT: &str = "The bridge to Ashford collapsed in the night.";
    pub(crate) const MACHINE_FACT: &str = "The river has risen above the old mill.";

    /// v4's `seatLine()`.
    pub(crate) fn seat_line() -> Value {
        json!({
            "id": "m-seat", "type": "message", "role": "USER", "content": SEAT_FACT,
            "participantId": "p-user", "targetParticipantIds": null,
            "isEstablishedFact": true, "attachments": [],
            "createdAt": "2026-10-01T10:00:00.000Z"
        })
    }

    /// v4's `machineFact()`, as `postEstablishedFact` writes it.
    pub(crate) fn machine_fact() -> Value {
        json!({
            "id": "m-fact", "type": "message", "role": "ASSISTANT", "content": MACHINE_FACT,
            "opaqueContent": MACHINE_FACT, "participantId": null, "systemSender": "host",
            "systemKind": "established-fact", "targetParticipantIds": null,
            "customAnnouncer": null, "attachments": [],
            "createdAt": "2026-10-01T10:01:00.000Z"
        })
    }

    /// v4's `reply()`.
    pub(crate) fn reply() -> Value {
        json!({
            "id": "m-reply", "type": "message", "role": "ASSISTANT",
            "content": "Then we take the ferry.", "participantId": "p-alice",
            "targetParticipantIds": null, "attachments": [],
            "createdAt": "2026-10-01T10:02:00.000Z"
        })
    }

    /// v4's `expectFactsFramed`.
    pub(crate) fn expect_facts_framed(text: &str, voice: EstablishedFactVoice) {
        assert!(text.contains(SEAT_FACT), "seat fact missing: {text}");
        assert!(text.contains(MACHINE_FACT), "machine fact missing: {text}");
        assert!(
            text.contains(ESTABLISHED_FACT_SEAT_FRAME),
            "seat frame missing: {text}"
        );
        match voice {
            EstablishedFactVoice::Host => {
                assert!(
                    text.contains(ESTABLISHED_FACT_HOST_FRAME),
                    "host frame missing: {text}"
                )
            }
            EstablishedFactVoice::Plain => {
                assert!(
                    text.contains(ESTABLISHED_FACT_PLAIN_FRAME),
                    "plain frame missing: {text}"
                );
                assert!(
                    !text.contains(ESTABLISHED_FACT_HOST_FRAME),
                    "host frame present: {text}"
                );
            }
        }
        assert!(!names_the_feature(text), "UI name leaked: {text}");
    }

    #[test]
    fn no_constant_names_the_feature() {
        for c in [
            ESTABLISHED_FACT_KIND,
            ESTABLISHED_FACT_SPEAKER_LABEL,
            ESTABLISHED_FACT_SEAT_FRAME,
            ESTABLISHED_FACT_HOST_FRAME,
            ESTABLISHED_FACT_PLAIN_FRAME,
            ESTABLISHED_FACT_REMINDER_HEADER,
            ESTABLISHED_FACT_TRANSCRIPT_RULE,
        ] {
            assert!(!names_the_feature(c), "{c}");
        }
        // The detector itself is live (a guard that cannot fail tests nothing).
        // Built at run time so the source census never sees the name in a literal.
        assert!(names_the_feature(&format!("Deus  {}\nMachina", "Ex")));
    }

    #[test]
    fn the_frame_pass_frames_both_kinds_in_either_voice() {
        for voice in [EstablishedFactVoice::Host, EstablishedFactVoice::Plain] {
            let rows = vec![seat_line(), machine_fact(), reply()];
            let framed = frame_established_facts(&rows, voice).unwrap();
            expect_facts_framed(&serde_json::to_string(&framed).unwrap(), voice);
            // The swipe path reads the same framed history (idempotent).
            assert_eq!(frame_established_facts(&framed, voice).unwrap(), framed);
        }
    }
}

#[cfg(test)]
mod leak_tests_read_paths {
    //! The rest of v4's leak walk (`established-fact-leaks.test.ts`): the
    //! cheap-LLM transcripts, the fold, the extraction window and the turn
    //! transcript. (The context frame pass rides `message_context`'s own
    //! `established_fact_pass` tests; attribution and the rehearsal theirs.)
    use super::leak_tests::*;
    use super::*;
    use serde_json::Value;

    fn rows() -> Vec<Value> {
        vec![seat_line(), machine_fact(), reply()]
    }

    #[test]
    fn extract_visible_conversation_frames_both_kinds_in_the_plain_voice() {
        let raw: Vec<_> = rows()
            .iter()
            .map(crate::chat_tasks::RawMessage::from_event)
            .collect();
        let visible = crate::chat_tasks::extract_visible_conversation(&raw);
        expect_facts_framed(
            &format!("{visible:?}").replace("\\n", "\n"),
            EstablishedFactVoice::Plain,
        );
        let m = visible
            .iter()
            .find(|m| m.content.contains(MACHINE_FACT))
            .unwrap();
        assert_eq!(m.role, "user");
    }

    #[test]
    fn the_fold_labels_the_machine_fact_and_the_prompt_carries_the_rule() {
        use crate::context_summary::{partition_messages_into_turns, PartitionInputMessage};
        let all = rows();
        let input: Vec<_> = all.iter().map(PartitionInputMessage::from_event).collect();
        let turns = partition_messages_into_turns(&input, None);
        let ids: Vec<_> = turns.iter().flat_map(|t| t.ids.clone()).collect();
        assert_eq!(ids, ["m-seat", "m-fact", "m-reply"]);
        assert_eq!(turns.len(), 2);
        let by_id: std::collections::HashMap<&str, &Value> =
            all.iter().map(|m| (m["id"].as_str().unwrap(), m)).collect();
        let names = crate::services::speaker_names::SpeakerNames::from_pairs([
            ("p-user".to_string(), "Captain".to_string()),
            ("p-alice".to_string(), "Alice".to_string()),
        ]);
        let lines =
            crate::services::context_summary::turns_to_chat_messages(&turns, &by_id, &names);
        let machine = lines.iter().find(|l| l.content == MACHINE_FACT).unwrap();
        assert_eq!(machine.speaker, ESTABLISHED_FACT_SPEAKER_LABEL);
        let seat = lines
            .iter()
            .find(|l| l.content.contains(SEAT_FACT))
            .unwrap();
        assert_eq!(seat.speaker, "Captain");
        assert!(seat.content.contains(ESTABLISHED_FACT_SEAT_FRAME));
        let prompt = crate::services::context_summary::prompt_text::FOLD_SUMMARY_PROMPT;
        assert!(prompt.contains(ESTABLISHED_FACT_TRANSCRIPT_RULE));
        assert!(!names_the_feature(prompt));
        assert!(!lines
            .iter()
            .any(|l| names_the_feature(&l.content) || names_the_feature(&l.speaker)));
    }

    #[test]
    fn a_stretch_frames_the_seat_line_too() {
        let names = crate::services::speaker_names::SpeakerNames::from_pairs([(
            "p-user".to_string(),
            "Captain".to_string(),
        )]);
        let w = crate::memory_extraction::window::stretch_window(
            vec![seat_line(), reply()],
            &names,
            Some("m-reply".to_string()),
            0.0,
        );
        let rendered = crate::memory_extraction::window::render_stretch(&w.rendered);
        assert!(
            rendered.contains(&format!("Captain: {ESTABLISHED_FACT_SEAT_FRAME}")),
            "{rendered}"
        );
        assert!(!names_the_feature(&rendered));
    }

    #[test]
    fn the_turn_transcript_frames_a_seat_line_opener() {
        let participants = vec![
            serde_json::json!({"id": "p-alice", "characterId": "c-alice", "type": "CHARACTER", "controlledBy": "llm", "status": "active"}),
            serde_json::json!({"id": "p-user", "characterId": null, "type": "CHARACTER", "controlledBy": "user", "status": "active"}),
        ];
        let characters = std::collections::HashMap::from([(
            "c-alice".to_string(),
            serde_json::json!({"id": "c-alice", "name": "Alice", "pronouns": null}),
        )]);
        let t = crate::services::turn_transcript::build_turn_transcript(
            &rows(),
            &participants,
            &characters,
            &crate::services::turn_transcript::BuildTurnTranscriptOptions {
                turn_opener_message_id: Some("m-seat".to_string()),
                ..Default::default()
            },
        );
        let rendered = crate::memory_extraction::window::render_turn_context(&t);
        assert!(
            rendered.contains(SEAT_FACT) && rendered.contains(ESTABLISHED_FACT_SEAT_FRAME),
            "{rendered}"
        );
        assert!(!names_the_feature(&rendered));
    }
}

/// Leak rows that needed a sibling lane's file on the union (§S.4 / H4) —
/// un-ignored at the `624d0d25c` unification, where P4.D373's extraction
/// prompt bodies carry the `ESTABLISHED FACTS` block.
#[cfg(test)]
mod union_leak_tests {
    use super::*;

    #[test]
    fn the_extraction_prompts_carry_the_transcript_rule() {
        use crate::memory_extraction::prompts::prompt_text as pt;
        for (name, body) in [
            (
                "SELF",
                format!("{}{}", pt::SELF_BODY_BEFORE_CAP, pt::SELF_BODY_AFTER_CAP),
            ),
            (
                "OTHER",
                format!("{}{}", pt::OTHER_BODY_BEFORE_CAP, pt::OTHER_BODY_AFTER_CAP),
            ),
            (
                "EPISODE",
                format!(
                    "{}{}",
                    pt::FOLD_EPISODE_PROMPT_BEFORE_CAP,
                    pt::FOLD_EPISODE_PROMPT_AFTER_CAP
                ),
            ),
        ] {
            assert!(
                body.contains(&format!(
                    "ESTABLISHED FACTS\n{ESTABLISHED_FACT_TRANSCRIPT_RULE}"
                )),
                "the {name} prompt lacks the ESTABLISHED FACTS block"
            );
            assert!(!leak_tests::names_the_feature(&body), "{name}");
        }
    }
}
