//! Raised-hand yielding — shared pure logic (v4
//! `lib/chat/turn-manager/yield-signal.ts`, whole, at `322a5488e`; landed in
//! `2b5e8ee24`).
//!
//! When the operator raises a hand (the composer's soft-interrupt button), the
//! next character to take the floor finds a Turn note in its prompt asking
//! whether it will yield. It answers with the `[YIELD]` sentinel, or simply
//! takes its turn. A yield is recorded as a Host message
//! (`systemSender: 'host'`, `systemKind: 'turn-yield'`,
//! `hostEvent: { participantId }`) and the chain stops so the floor is the
//! operator's; the chat is not paused. Who has a hand up lives in
//! [`crate::services::turn_control`]; this module is pure.
//!
//! Detection is deliberately forgiving in the same ways as the turn-pass
//! sentinel ([`crate::skip_signal`]): wrapping markdown, optional brackets,
//! case and trailing punctuation are shed, a leading own-name prefix is
//! stripped, and a sentinel on the LAST line counts as well as the first
//! (weaker models narrate a gesture and then tack the sentinel on). Any prose
//! around the sentinel is kept as the character's reply — "After you, my
//! dear" and then the yield.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::jsstr::{js_trim, JS_WS_CLASS};
use crate::message_formatter::{normalize_content_block_format, strip_character_name_prefix};
use crate::skip_signal::matches_sentinel_line;

/// The literal sentinel a character emits to yield the floor.
pub const YIELD_SENTINEL: &str = "[YIELD]";

/// `systemKind` stamped on the Host message that records a yield. (v4 also
/// keeps the writer's module-local `HOST_KIND_TURN_YIELD` — the same bytes;
/// v5 has ONE const, which the Host writer references.)
pub const TURN_YIELD_SYSTEM_KIND: &str = "turn-yield";

const YIELD_PHRASE: &str = "yield";

/// A bracketed `[YIELD]` opening a line that carries more prose after it.
///
/// v4 `/^\s*[*_~"'`]*\[\s*yield\s*\][*_~"'`]*[.!?,;:]*\s*/i`. JS `\s` is
/// [`JS_WS_CLASS`] (U+FEFF / U+00A0 / U+2028-9 in, U+180E / U+0085 out — Rust's
/// `\s` differs on both), and v4's `i` without `u` folds only ASCII, so the
/// case-insensitive group is the ASCII-only `(?i-u:yield)`; the wrapper and
/// punctuation classes carry no case.
static LEADING_BRACKETED_YIELD: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    Regex::new(&format!(
        r#"^{ws}*[*_~"'`]*\[{ws}*(?i-u:yield){ws}*\][*_~"'`]*[.!?,;:]*{ws}*"#
    ))
    .expect("LEADING_BRACKETED_YIELD compiles")
});

/// v4 `DetectYieldResult` — `{ yielded: true, cleaned }` / `{ yielded: false }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DetectYieldResult {
    Yielded {
        /// Always `true` on the wire (v4's discriminant).
        yielded: TrueFlag,
        cleaned: String,
    },
    NotYielded {
        yielded: FalseFlag,
    },
}

/// Serializes as JSON `true` (the `Yielded` discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrueFlag;
impl Serialize for TrueFlag {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}

/// Serializes as JSON `false` (the `NotYielded` discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FalseFlag;
impl Serialize for FalseFlag {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(false)
    }
}

impl DetectYieldResult {
    fn yielded(cleaned: String) -> Self {
        DetectYieldResult::Yielded {
            yielded: TrueFlag,
            cleaned,
        }
    }

    fn not_yielded() -> Self {
        DetectYieldResult::NotYielded { yielded: FalseFlag }
    }

    /// Did the response yield the floor?
    pub fn is_yielded(&self) -> bool {
        matches!(self, DetectYieldResult::Yielded { .. })
    }

    /// The response with the sentinel removed (possibly empty) when it yielded.
    pub fn cleaned(&self) -> Option<&str> {
        match self {
            DetectYieldResult::Yielded { cleaned, .. } => Some(cleaned),
            DetectYieldResult::NotYielded { .. } => None,
        }
    }
}

/// Decide whether a raw model response yields the floor (v4
/// `detectYieldSentinel`).
///
/// `cleaned` is the response with the sentinel removed (possibly empty): the
/// prose a yielding character also said, which the caller persists as an
/// ordinary reply when it is non-empty.
pub fn detect_yield_sentinel(
    response: &str,
    character_name: Option<&str>,
    aliases: Option<&[String]>,
) -> DetectYieldResult {
    if response.is_empty() {
        return DetectYieldResult::not_yielded();
    }

    // As in `detect_skip_sentinel`: only strip a name prefix when there is a
    // name to target, or the generic bracketed-name fallback eats `[YIELD]`
    // itself.
    let normalized_raw = normalize_content_block_format(response);
    let have_name =
        character_name.is_some_and(|n| !n.is_empty()) || aliases.is_some_and(|a| !a.is_empty());
    let normalized = if have_name {
        strip_character_name_prefix(&normalized_raw, character_name, aliases)
    } else {
        normalized_raw
    };

    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut first_idx: Option<usize> = None;
    let mut last_idx = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if js_trim(line).is_empty() {
            continue;
        }
        if first_idx.is_none() {
            first_idx = Some(i);
        }
        last_idx = i;
    }
    let Some(first_idx) = first_idx else {
        return DetectYieldResult::not_yielded();
    };

    let without = |idx: usize| -> String {
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len().saturating_sub(1));
        kept.extend_from_slice(&lines[..idx]);
        kept.extend_from_slice(&lines[idx + 1..]);
        js_trim(&kept.join("\n")).to_string()
    };

    if matches_sentinel_line(lines[first_idx], YIELD_PHRASE) {
        return DetectYieldResult::yielded(without(first_idx));
    }
    if last_idx != first_idx && matches_sentinel_line(lines[last_idx], YIELD_PHRASE) {
        return DetectYieldResult::yielded(without(last_idx));
    }

    // `[YIELD] After you.` on one line: the bracketed form is unambiguous
    // enough to honour at the head of a line; a bare "yield" word inside prose
    // is not.
    if let Some(leading) = LEADING_BRACKETED_YIELD.find(lines[first_idx]) {
        let tail = &lines[first_idx][leading.end()..];
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
        kept.extend_from_slice(&lines[..first_idx]);
        kept.push(tail);
        kept.extend_from_slice(&lines[first_idx + 1..]);
        return DetectYieldResult::yielded(js_trim(&kept.join("\n")).to_string());
    }

    DetectYieldResult::not_yielded()
}

/// Build the ephemeral "the operator has raised a hand" Turn note (v4
/// `buildTurnYieldInstruction`, byte for byte). Never persisted — it rides
/// only in this one turn's outgoing context, so the sentinel never sits in
/// history to be learned. `operator_name` is the name the room knows the
/// operator by (their persona, or "The User").
pub fn build_turn_yield_instruction(operator_name: &str) -> String {
    format!(
        "[Turn note from the Salon \u{2014} not spoken by any character]
{operator_name} has raised a hand, wishing to interject before you take your turn.
You may yield the floor to them, or carry on as though you had not noticed.

To yield, reply with exactly this line:

{YIELD_SENTINEL}

You may follow it with a brief in-character line (a gesture inviting them to
speak, say), but keep it short \u{2014} the floor will be theirs. To carry on
instead, write your reply as normal and ignore this note entirely; do not
mention the raised hand unless you choose to acknowledge it in character."
    )
}

#[cfg(test)]
mod tests {
    //! v4 `__tests__/unit/lib/chat/turn-manager/yield-signal.test.ts` — the
    //! ten cases 1:1 (the differential over v4's REAL code is
    //! `turn_control_equivalence` in the harness).
    use super::*;

    fn y(r: &str, name: Option<&str>) -> DetectYieldResult {
        detect_yield_sentinel(r, name, None)
    }

    #[test]
    fn bare_and_wrapped_forms_yield_with_nothing_left() {
        for r in [
            "[YIELD]",
            "**[YIELD]**",
            "[yield].",
            "YIELD",
            "  [ YIELD ]  ",
        ] {
            assert_eq!(y(r, Some("Abigail")).cleaned(), Some(""), "{r:?}");
        }
    }

    #[test]
    fn keeps_prose_around_a_leading_or_trailing_sentinel() {
        assert_eq!(
            y("[YIELD]\n*bows and gestures to the floor*", Some("Abigail")).cleaned(),
            Some("*bows and gestures to the floor*")
        );
        assert_eq!(
            y(
                "*bows and gestures to the floor*\n\n[YIELD]",
                Some("Abigail")
            )
            .cleaned(),
            Some("*bows and gestures to the floor*")
        );
        assert_eq!(
            y("[YIELD] After you.", Some("Abigail")).cleaned(),
            Some("After you.")
        );
    }

    #[test]
    fn strips_a_leading_name_prefix_but_not_without_a_name() {
        assert!(y("Abigail: [YIELD]", Some("Abigail")).is_yielded());
        assert!(y("[YIELD]", None).is_yielded());
    }

    #[test]
    fn a_bare_yield_word_in_prose_is_not_a_yield() {
        assert!(!y("I will not yield to anyone.", Some("Abigail")).is_yielded());
        assert!(!y("", Some("Abigail")).is_yielded());
        assert!(!y("   \n  ", Some("Abigail")).is_yielded());
    }

    #[test]
    fn the_regex_uses_js_whitespace_and_ascii_case() {
        // U+FEFF is JS whitespace; U+0085 / U+180E are not.
        assert!(y("\u{FEFF}[YIELD] Go on.", None).is_yielded());
        assert!(!y("\u{0085}[YIELD] Go on.", None).is_yielded());
        // `K` (U+212A) does not fold to `k` without `u`; nothing in `yield`
        // folds non-ASCII — the long-s `ſ` stays a non-match.
        assert!(!y("[YIEſD] Go on.", None).is_yielded());
    }

    #[test]
    fn serializes_to_v4_shapes() {
        assert_eq!(
            serde_json::to_string(&y("[YIELD] Go.", None)).unwrap(),
            r#"{"yielded":true,"cleaned":"Go."}"#
        );
        assert_eq!(
            serde_json::to_string(&y("Hello", None)).unwrap(),
            r#"{"yielded":false}"#
        );
    }

    #[test]
    fn the_note_names_the_operator_and_carries_the_sentinel_alone() {
        let n = build_turn_yield_instruction("Charlie");
        assert!(n.starts_with("[Turn note from the Salon \u{2014} not spoken by any character]\nCharlie has raised a hand"));
        assert!(n.contains("\n\n[YIELD]\n\n"));
    }
}
