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

use crate::jsstr::{is_js_ws, js_trim, JS_WS_CLASS};
use crate::message_formatter::{normalize_content_block_format, strip_character_name_prefix};
use crate::skip_signal::matches_sentinel_line;

/// The literal sentinel a character emits to yield the floor.
pub const YIELD_SENTINEL: &str = "[YIELD]";

/// `systemKind` stamped on the Host message that records a yield. (v4 also
/// keeps the writer's module-local `HOST_KIND_TURN_YIELD` — the same bytes;
/// v5 has ONE const, which the Host writer references.)
pub const TURN_YIELD_SYSTEM_KIND: &str = "turn-yield";

const YIELD_PHRASE: &str = "yield";

// === P4.D366 (v4 `9eab8d2da`: gesture-only answers count as a yield) ===
/// Longest reply read as a gesture-only yield; past this it is a scene beat.
/// UTF-16 code units (JS `String.length`).
pub const GESTURE_ONLY_MAX_CHARS: usize = 280;

/// Quotation marks: any of them means a word was spoken (v4 `SPOKEN_QUOTE`,
/// `/["“”«»„]/`).
const SPOKEN_QUOTES: [char; 6] = ['"', '\u{201C}', '\u{201D}', '\u{AB}', '\u{BB}', '\u{201E}'];

/// Stage-direction wrappers: opening character → closing character (v4
/// `GESTURE_WRAPPERS = { '*': '*', '_': '_', '(': ')' }`).
fn gesture_close(open: u16) -> Option<u16> {
    match open {
        0x2A => Some(0x2A), // '*'
        0x5F => Some(0x5F), // '_'
        0x28 => Some(0x29), // '(' → ')'
        _ => None,
    }
}

/// v4's `/\n\s*\n/` with JS `\s` — a blank line between paragraphs.
static BLANK_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\n{}*\n", JS_WS_CLASS)).expect("BLANK_LINE compiles"));

/// v4's `/[\p{L}]/u` — General_Category Letter.
static A_LETTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{L}").expect("A_LETTER compiles"));

/// JS `trim` over UTF-16 code units (every JS whitespace is one BMP unit).
fn trim_units(s: &[u16]) -> &[u16] {
    let ws = |u: &u16| char::from_u32(u32::from(*u)).is_some_and(is_js_ws);
    let start = s.iter().position(|u| !ws(u)).unwrap_or(s.len());
    let end = s.iter().rposition(|u| !ws(u)).map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

/// v4 `isGestureOnlyReply`: is this reply nothing but one stage direction —
/// wrapped whole in `*…*`, `_…_` or `(…)` (layered as the model likes), on one
/// paragraph, short, with no quotation marks? A closing wrapper inside a layer
/// means prose sits between two directions (`*nods* Fine. *smiles*`), which is
/// not a gesture.
///
/// Every index, the length cap and the `run < s.length / 2` compare are v4's
/// UTF-16 code-unit arithmetic (a char-indexed port differs on astral text).
pub fn is_gesture_only_reply(text: &str) -> bool {
    let trimmed = js_trim(text);
    let units: Vec<u16> = trimmed.encode_utf16().collect();
    if units.is_empty() || units.len() > GESTURE_ONLY_MAX_CHARS {
        return false;
    }
    if BLANK_LINE.is_match(trimmed) || trimmed.contains(&SPOKEN_QUOTES[..]) {
        return false;
    }

    let mut s: &[u16] = &units;
    let mut peeled = false;
    while let Some((first, close)) = s.first().and_then(|&f| gesture_close(f).map(|c| (f, c))) {
        if s.len() < 2 || s[s.len() - 1] != close {
            break;
        }
        let mut run = 1usize;
        if close == first {
            // `**bold**`-style runs peel together. `run < s.length / 2` is a
            // FLOAT compare in JS: `2 * run < len`.
            while 2 * run < s.len() && s[run] == first && s[s.len() - 1 - run] == close {
                run += 1;
            }
        }
        // JS `slice(start, end)` with start ≥ end is the empty string.
        let end = s.len() - run;
        let inner = if run < end { &s[run..end] } else { &s[0..0] };
        let inner = trim_units(inner);
        if inner.contains(&close) {
            return false;
        }
        s = inner;
        peeled = true;
    }
    // A lone surrogate decodes to U+FFFD, which is not a letter — as in V8.
    peeled && A_LETTER.is_match(&String::from_utf16_lossy(s))
}
// === end P4.D366 ===

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

/// v4 `DetectYieldResult` — `{ yielded: true, cleaned, inferred?: true }` /
/// `{ yielded: false }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DetectYieldResult {
    Yielded {
        /// Always `true` on the wire (v4's discriminant).
        yielded: TrueFlag,
        cleaned: String,
        // === P4.D366 ===
        /// No sentinel; the reply was a gesture-only yield (v4 `9eab8d2da`).
        /// ABSENT unless true (v4's `inferred?: true`).
        #[serde(skip_serializing_if = "Option::is_none")]
        inferred: Option<TrueFlag>,
        // === end P4.D366 ===
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
            inferred: None,
        }
    }

    // === P4.D366 ===
    /// Was the yield inferred from a gesture-only reply (no sentinel)?
    pub fn is_inferred(&self) -> bool {
        matches!(
            self,
            DetectYieldResult::Yielded {
                inferred: Some(_),
                ..
            }
        )
    }
    // === end P4.D366 ===

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

    // === P4.D366 ===
    // No sentinel, but nothing spoken either: a gesture handing over the floor.
    if is_gesture_only_reply(&normalized) {
        return DetectYieldResult::Yielded {
            yielded: TrueFlag,
            cleaned: js_trim(&normalized).to_string(),
            inferred: Some(TrueFlag),
        };
    }
    // === end P4.D366 ===

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
speak, say), but keep it short \u{2014} the floor will be theirs. A reply that is
only a gesture, with no words spoken aloud, is also taken as yielding.

To carry on instead, write your reply as normal and ignore this note entirely;
do not mention the raised hand unless you choose to acknowledge it in
character."
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

    /// R-18 — the `systemKind` reader census (P4.D335). v4 `2b5e8ee24` adds NO
    /// handling of `turn-yield` anywhere: every reader equality-tests a SPECIFIC
    /// kind, and the only structural consumers of a Host kind are the turn-pass
    /// converters — so a `turn-yield` row is a generic Host message everywhere.
    #[test]
    fn a_turn_yield_row_is_a_generic_host_message_to_every_reader() {
        let row = serde_json::json!({
            "type": "message",
            "id": "y1",
            "role": "ASSISTANT",
            "content": "The Host, spying a hand raised …",
            "participantId": null,
            "systemSender": "host",
            "systemKind": TURN_YIELD_SYSTEM_KIND,
            "hostEvent": { "participantId": "p-ada" },
        });
        assert!(!crate::skip_signal::is_turn_pass_message(&row));
        let views = crate::services::turn_orchestrator::to_message_views(&[row]);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].msg_type.as_deref(), Some("message"));
        assert_ne!(
            views[0].msg_type.as_deref(),
            Some(crate::turn_state::TURN_PASS_VIEW_TYPE)
        );
    }

    /// The file with every `#[cfg(test)]` item removed by brace balance (a test
    /// module may sit mid-file, as `post_office/mailbox.rs`'s does).
    fn production_zone(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut rest = src;
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let Some(b) = after.find('{') else {
                return out;
            };
            let mut depth = 0usize;
            let mut end = after.len();
            for (offset, ch) in after[b..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = b + offset + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest = &after[end..];
        }
        out.push_str(rest);
        out
    }

    /// R-18's forward-only guard: the `"turn-yield"` literal lives in this
    /// module's const and in `MailAlertsVia::TurnYield`'s `via` string (a
    /// DIFFERENT field — the mail ledger, not `systemKind`) and nowhere else in
    /// core production code. A later reader that branches on the kind must add
    /// itself here, deliberately.
    #[test]
    fn the_turn_yield_kind_literal_has_no_unlisted_reader() {
        const ALLOWED: &[&str] = &["post_office/mailbox.rs", "yield_signal.rs"];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![root.clone()];
        let mut hits = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    if production_zone(&text).contains("\"turn-yield\"") {
                        let rel = path
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        hits.push(rel);
                    }
                }
            }
        }
        hits.sort();
        assert_eq!(
            hits, ALLOWED,
            "an unlisted `\"turn-yield\"` reader appeared"
        );
    }

    // === P4.D366: v4 `9eab8d2da`'s `it`s, verbatim ===
    #[test]
    fn reads_a_gesture_only_reply_as_an_inferred_yield_keeping_the_gesture() {
        let gesture = "*(a tilt of the head, one hand opening toward Tester \u{2014} go on, then)*";
        assert_eq!(
            serde_json::to_string(&y(gesture, Some("Riya"))).unwrap(),
            serde_json::json!({"yielded": true, "cleaned": gesture, "inferred": true}).to_string()
        );
        let r = y(
            "Riya: *steps back and sweeps a hand toward you*",
            Some("Riya"),
        );
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"yielded":true,"cleaned":"*steps back and sweeps a hand toward you*","inferred":true}"#
        );
        assert!(r.is_inferred());
        assert!(!y("[YIELD] Go.", None).is_inferred());
    }

    #[test]
    fn reads_a_reply_with_spoken_words_or_a_long_beat_as_carrying_on() {
        let long = format!(
            "*{}*",
            "she works the knot loose and frowns at the water ".repeat(8)
        );
        for r in [
            "*nods* \"Go on, then.\"",
            "*nods* Go on, then. *smiles*",
            long.as_str(),
            "*She looks up.*\n\n*She looks down again.*",
        ] {
            assert_eq!(
                serde_json::to_string(&y(r, None)).unwrap(),
                r#"{"yielded":false}"#,
                "{r:?}"
            );
        }
    }

    #[test]
    fn accepts_a_single_wrapped_stage_direction_in_any_of_the_usual_wrappers() {
        for form in [
            "*bows*",
            "_bows_",
            "(bows)",
            "**bows**",
            "*(bows to Tester)*",
            "*It's yours, the gesture says.*",
        ] {
            assert!(is_gesture_only_reply(form), "{form:?}");
        }
    }

    #[test]
    fn refuses_bare_prose_quoted_speech_empty_wrappers_and_prose_between_directions() {
        for form in [
            "bows",
            "\"After you.\"",
            "*bows* \"After you.\"",
            "**",
            "*\u{2026}*",
            "*nods* Fine. *smiles*",
            "",
        ] {
            assert!(!is_gesture_only_reply(form), "{form:?}");
        }
    }

    /// The UTF-16 arithmetic: an astral letter is two units — so 140 of them
    /// wrapped (282 units) is past the cap even though it is 142 chars.
    #[test]
    fn the_cap_and_indices_are_utf16_code_units() {
        let astral = format!("*{}*", "\u{1D49C}".repeat(139)); // 280 units
        assert!(is_gesture_only_reply(&astral));
        let over = format!("*{}*", "\u{1D49C}".repeat(140)); // 282 units
        assert!(!is_gesture_only_reply(&over));
        assert!(!is_gesture_only_reply("*123*"));
        assert!(is_gesture_only_reply("***x***"));
    }

    #[test]
    fn the_note_tells_the_model_a_gesture_yields() {
        let n = build_turn_yield_instruction("Charlie");
        assert!(n.contains("only a gesture"));
        assert!(n.ends_with(
            "the floor will be theirs. A reply that is\nonly a gesture, with no words spoken aloud, is also taken as yielding.\n\nTo carry on instead, write your reply as normal and ignore this note entirely;\ndo not mention the raised hand unless you choose to acknowledge it in\ncharacter."
        ));
    }
    // === end P4.D366 ===

    #[test]
    fn the_note_names_the_operator_and_carries_the_sentinel_alone() {
        let n = build_turn_yield_instruction("Charlie");
        assert!(n.starts_with("[Turn note from the Salon \u{2014} not spoken by any character]\nCharlie has raised a hand"));
        assert!(n.contains("\n\n[YIELD]\n\n"));
    }
}
