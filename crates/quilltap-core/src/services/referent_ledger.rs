//! The trailing referent ledger and the closing echo of the user's line (v4
//! `lib/chat/context/referent-ledger.ts`, `f7a1789df`).
//!
//! A short line like "When she's not scolding Vergil" leans on a pronoun whose
//! antecedent sits in the *previous* turn ("…Abigail will absolutely make that a
//! formal finding. You've seen how she documents."). By the time the model reads
//! it, a Host card, the scene state and twenty-odd kilobytes of recall stand
//! between the two, and a weaker model resolves "she" to the nearest woman it
//! can see — itself. Two trailing sections answer that:
//!
//! - **The ledger** ([`build_referent_ledger`]) lists the people named in the
//!   last few spoken lines, most recent first, with their pronouns, whether they
//!   are in the room, who last named them and the sentence they were named in.
//!   Anyone listed who is not in the room brings their card — name, aliases,
//!   pronouns and identity — which is the only place the model now meets it:
//!   the Host's "spoken of" introduction in the Salon is record-only.
//!   Deterministic: a name/alias scan, no LLM call. The responder ("you") and
//!   the author of the latest line ("I") are left out — they are not who a
//!   third-person pronoun in that line points at.
//! - **The echo** ([`build_closing_echo`]) repeats the line being answered at
//!   the very end of the trailing sections, after recall has had its say, so the
//!   last thing the model reads is what it is replying to.
//!
//! ## Where they land
//!
//! Trailing per-turn sections on the uncached tail, assembled by
//! `build_context` after the inform block and before the turn notes. Never in
//! system block 1, never persisted, so neither builder version moves.
//!
//! ## Empty is byte-for-byte nothing
//!
//! With nobody named in the window the ledger returns `""`; with no new user
//! message the echo returns `""`. The caller pushes nothing.
//!
//! ## UTF-16
//!
//! Every length, offset and slice here is a JS string's — UTF-16 code units —
//! and the two regexes (the sentence boundary and `\s+`) run through `regress`
//! so `\s` is JS's set and the indices are units. Where a JS `slice` would cut a
//! surrogate pair (an astral character exactly at the 1,500-unit echo cut or the
//! 220-unit quote window) JS yields a lone surrogate a Rust `String` cannot hold;
//! v5 rounds the cut to the code-point boundary — one unit shorter, never longer
//! (the ruled divergences `ECHO_SLICE_SURROGATE` / `SENTENCE_SLICE_SURROGATE`).

use std::sync::LazyLock;

use crate::jsstr::{is_js_ws, js_trim, js_trim_end, js_trim_start, utf16_len};
use crate::name_mentions::{build_name_matcher, last_name_match};

const CONTEXT: &str = "chat.context.referent-ledger";

/// How many spoken lines (user and character, no Staff) the ledger scans.
pub const REFERENT_LEDGER_WINDOW_LINES: usize = 6;

/// At most this many people are listed; the most recently named win.
pub const REFERENT_LEDGER_MAX_PEOPLE: usize = 8;

/// The quoted sentence is trimmed to about this many characters around the name.
pub const REFERENT_LEDGER_QUOTE_CHARS: usize = 220;

/// A user line longer than this is echoed by its closing paragraphs only.
pub const CLOSING_ECHO_MAX_CHARS: usize = 1500;

/// One spoken line in the window, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferentLedgerLine {
    /// Display name of who said it.
    pub speaker: String,
    /// True when the responding character said it.
    pub is_responder: bool,
    pub content: String,
}

/// A person the ledger may name.
pub struct ReferentLedgerPerson<'a> {
    pub id: String,
    pub name: String,
    pub aliases: Option<Vec<String>>,
    /// `(subject, object, possessive)`.
    pub pronouns: Option<(String, String, String)>,
    /// Seated (not removed) in this chat.
    pub in_room: bool,
    /// The person's particulars (the Host's off-scene card), rendered only if
    /// they are listed. Given for people not in the room.
    pub render_card: Option<Box<dyn Fn() -> String + 'a>>,
}

/// The sentence boundary, v4 `/[.!?…]["'”’*_)]*\s+|\n+/g` (no `u` flag: matched
/// over UTF-16 code units, as JS does).
static BOUNDARY: LazyLock<regress::Regex> = LazyLock::new(|| {
    let pattern = "[.!?\u{2026}][\"'\u{201D}\u{2019}*_)]*\\s+|\\n+";
    regress::Regex::from_unicode(pattern.encode_utf16().map(u32::from), "g")
        .expect("referent-ledger boundary regex")
});

/// v4 `split(/\n\s*\n/)`'s separator.
static PARAGRAPH_BREAK: LazyLock<regress::Regex> = LazyLock::new(|| {
    regress::Regex::from_unicode("\\n\\s*\\n".encode_utf16().map(u32::from), "g")
        .expect("closing-echo paragraph regex")
});

/// The first boundary match at or after `from` (JS `exec` with `lastIndex =
/// from`), as `(index, end)` in UTF-16 units.
fn boundary_from(units: &[u16], from: usize) -> Option<(usize, usize)> {
    BOUNDARY
        .find_from_ucs2(units, from)
        .next()
        .map(|m| (m.start(), m.end()))
}

fn decode(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// A JS `slice(from, to)` over `units`, rounded to code-point boundaries where it
/// would split a surrogate pair (one unit shorter at either end, never longer).
fn slice_on_code_points(units: &[u16], from: usize, to: usize) -> String {
    let is_low = |u: u16| (0xDC00..=0xDFFF).contains(&u);
    let is_high = |u: u16| (0xD800..=0xDBFF).contains(&u);
    let mut from = from.min(units.len());
    let mut to = to.min(units.len());
    if from < units.len() && from > 0 && is_low(units[from]) && is_high(units[from - 1]) {
        from += 1;
    }
    if to > 0 && to < units.len() && is_high(units[to - 1]) && is_low(units[to]) {
        to -= 1;
    }
    if from >= to {
        return String::new();
    }
    decode(&units[from..to])
}

/// JS `s.replace(/\s+/g, ' ')`: every run of JS whitespace becomes one space.
fn collapse_js_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for ch in s.chars() {
        if is_js_ws(ch) {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(ch);
            in_run = false;
        }
    }
    out
}

/// v4 `sentenceAround(content, offset, length)` — the sentence around `offset`
/// (UTF-16 units), whitespace-collapsed and trimmed to the quote budget.
pub(crate) fn sentence_around(content: &str, offset: usize, length: usize) -> String {
    let units: Vec<u16> = content.encode_utf16().collect();
    let mut start = 0usize;
    let mut end = units.len();
    let mut last_index = 0usize;
    while let Some((index, after)) = boundary_from(&units, last_index) {
        last_index = after;
        if after <= offset {
            start = after;
        } else if index >= offset + length {
            end = index + utf16_len(js_trim_end(&decode(&units[index..after])));
            // Carry on into the next sentence when it fits: a name is so often
            // followed by the pronoun that stands for it ("…Abigail will make
            // it a formal finding. You've seen how she documents.").
            let next_end = match boundary_from(&units, last_index) {
                Some((ni, na)) => ni + utf16_len(js_trim_end(&decode(&units[ni..na]))),
                None => units.len(),
            };
            if next_end > end && next_end - start <= REFERENT_LEDGER_QUOTE_CHARS {
                end = next_end;
            }
            break;
        }
    }
    let sentence = js_trim(&collapse_js_whitespace(&decode(
        &units[start..end.max(start)],
    )))
    .to_string();
    let s_units: Vec<u16> = sentence.encode_utf16().collect();
    if s_units.len() <= REFERENT_LEDGER_QUOTE_CHARS {
        return sentence;
    }
    let local = offset as i64 - start as i64;
    let half = (REFERENT_LEDGER_QUOTE_CHARS / 2) as i64;
    let len = s_units.len() as i64;
    let from = 0i64.max((local - half).min(len - REFERENT_LEDGER_QUOTE_CHARS as i64)) as usize;
    let to = s_units.len().min(from + REFERENT_LEDGER_QUOTE_CHARS);
    format!(
        "{}{}{}",
        if from > 0 { "\u{2026}" } else { "" },
        js_trim(&slice_on_code_points(&s_units, from, to)),
        if to < s_units.len() { "\u{2026}" } else { "" }
    )
}

const NUMBER_WORDS: [&str; 11] = [
    "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// v4 `whereNamed(lineIndex, lines)`. (`NUMBER_WORDS[back] ?? String(back)`'s
/// digit arm is dead at a six-line window — `back` is at most five.)
pub(crate) fn where_named(line_index: usize, lines: &[ReferentLedgerLine]) -> String {
    let line = &lines[line_index];
    let back = lines.len() - 1 - line_index;
    let by = if line.is_responder {
        "by you".to_string()
    } else {
        format!("by {}", line.speaker)
    };
    if back == 0 {
        return format!("{by}, in the line you are answering");
    }
    if line.is_responder {
        let later_own = lines[line_index + 1..].iter().any(|l| l.is_responder);
        if !later_own {
            return format!("{by}, in your last turn");
        }
    }
    let count = NUMBER_WORDS
        .get(back)
        .map(|w| w.to_string())
        .unwrap_or_else(|| back.to_string());
    format!(
        "{by}, {count} line{} back",
        if back == 1 { "" } else { "s" }
    )
}

struct Mention<'p, 'a> {
    person: &'p ReferentLedgerPerson<'a>,
    line_index: usize,
    offset: usize,
    length: usize,
}

/// v4 `buildReferentLedger({lines, people, responderId})` — the ledger section,
/// or `""` when nobody but the responder and the latest line's author is named
/// in the window.
pub fn build_referent_ledger(
    lines: &[ReferentLedgerLine],
    people: &[ReferentLedgerPerson<'_>],
    responder_id: &str,
) -> String {
    let lines = &lines[lines.len().saturating_sub(REFERENT_LEDGER_WINDOW_LINES)..];
    let Some(last_line) = lines.last() else {
        return String::new();
    };

    let latest_speaker = js_trim(&last_line.speaker).to_lowercase();
    let candidates: Vec<&ReferentLedgerPerson<'_>> = people
        .iter()
        .filter(|p| p.id != responder_id && js_trim(&p.name).to_lowercase() != latest_speaker)
        .collect();

    // v4 keys a `Map` by person id: insertion order is the candidates' order,
    // and a second person with the same id is skipped.
    let mut latest: Vec<Mention<'_, '_>> = Vec::new();
    for person in &candidates {
        if latest.iter().any(|m| m.person.id == person.id) {
            continue;
        }
        let names = std::iter::once(Some(person.name.as_str()))
            .chain(person.aliases.iter().flatten().map(|a| Some(a.as_str())));
        let Some(matcher) = build_name_matcher(names) else {
            continue;
        };
        for i in (0..lines.len()).rev() {
            if let Some(last) = last_name_match(&lines[i].content, &matcher) {
                latest.push(Mention {
                    person,
                    line_index: i,
                    offset: last.index,
                    length: utf16_len(&last.text),
                });
                break;
            }
        }
    }

    if latest.is_empty() {
        return String::new();
    }

    // Stable, as V8's sort is: `b.lineIndex - a.lineIndex || b.offset - a.offset`.
    latest.sort_by(|a, b| {
        b.line_index
            .cmp(&a.line_index)
            .then(b.offset.cmp(&a.offset))
    });
    latest.truncate(REFERENT_LEDGER_MAX_PEOPLE);

    let entries: Vec<String> = latest
        .iter()
        .map(|m| {
            let pronouns = match &m.person.pronouns {
                Some((subject, object, _)) => format!("{subject}/{object}"),
                None => "pronouns not recorded".to_string(),
            };
            let room = if m.person.in_room {
                "in the room"
            } else {
                "not in the room"
            };
            let quote = sentence_around(&lines[m.line_index].content, m.offset, m.length);
            format!(
                "- **{}** ({pronouns}; {room}) \u{2014} last named {}: \u{201C}{quote}\u{201D}",
                m.person.name,
                where_named(m.line_index, lines)
            )
        })
        .collect();

    let cards: Vec<String> = latest
        .iter()
        .filter(|m| !m.person.in_room)
        .filter_map(|m| m.person.render_card.as_ref().map(|render| render()))
        // A card with nothing under its heading tells the model nothing.
        .filter(|card| js_trim(card).contains('\n'))
        .collect();

    let listed: Vec<&str> = latest.iter().map(|m| m.person.name.as_str()).collect();
    tracing::debug!(
        context = CONTEXT,
        windowLines = lines.len(),
        candidates = candidates.len(),
        listedJson = %serde_json::to_string(&listed).unwrap_or_default(),
        cards = cards.len(),
        "[ReferentLedger] Ledger built"
    );

    let mut section: Vec<String> = vec![
        "## Who is being spoken of".to_string(),
        String::new(),
        "The people named in the last few lines, most recently named first. When a he, she, they, him or her in the line you are answering does not point at someone in that same line, it most likely carries on from the most recently named of these whose pronouns fit \u{2014} read it from what was just said, not from who is nearest in your memories.".to_string(),
        String::new(),
    ];
    section.extend(entries);
    if !cards.is_empty() {
        section.push(String::new());
        section.push("Particulars of those not in the room \u{2014} for accurate reference only. They are not present, cannot hear what is said here, and are not summoned by being named.".to_string());
        section.push(String::new());
        section.push(cards.join("\n\n"));
    }
    section.join("\n")
}

/// v4 `buildClosingEcho({newUserMessage, speakerName})` — the line being
/// answered, repeated last. Long lines are echoed by their closing paragraphs,
/// which is where a dangling pronoun lives.
pub fn build_closing_echo(new_user_message: Option<&str>, speaker_name: &str) -> String {
    let text = js_trim(new_user_message.unwrap_or(""));
    if text.is_empty() {
        return String::new();
    }

    let mut body = text.to_string();
    let mut partial = false;
    let text_len = utf16_len(text);
    if text_len > CLOSING_ECHO_MAX_CHARS {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut paragraphs: Vec<String> = Vec::new();
        let mut cursor = 0usize;
        for m in PARAGRAPH_BREAK.find_from_ucs2(&units, 0) {
            paragraphs.push(decode(&units[cursor..m.start()]));
            cursor = m.end();
        }
        paragraphs.push(decode(&units[cursor..]));

        let mut kept: Vec<String> = Vec::new();
        let mut used = 0usize;
        for raw in paragraphs.iter().rev() {
            let p = js_trim(raw);
            if p.is_empty() {
                continue;
            }
            let p_len = utf16_len(p);
            if !kept.is_empty() && used + p_len > CLOSING_ECHO_MAX_CHARS {
                break;
            }
            let piece = if p_len > CLOSING_ECHO_MAX_CHARS {
                let p_units: Vec<u16> = p.encode_utf16().collect();
                let tail = slice_on_code_points(&p_units, p_len - CLOSING_ECHO_MAX_CHARS, p_len);
                format!("\u{2026}{}", js_trim_start(&tail))
            } else {
                p.to_string()
            };
            kept.insert(0, piece);
            used += p_len;
        }
        body = kept.join("\n\n");
        partial = true;
    }

    tracing::debug!(
        context = CONTEXT,
        length = text_len,
        partial,
        "[ReferentLedger] Closing echo built"
    );

    let quoted = body
        .split('\n')
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let lead = if partial {
        format!("## The line you are answering\n\nThe closing part of what {speaker_name} just said, which is what you are replying to:")
    } else {
        format!("## The line you are answering\n\nWhat {speaker_name} just said, which is what you are replying to:")
    };
    format!("{lead}\n\n{quoted}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    fn line(speaker: &str, is_responder: bool, content: &str) -> ReferentLedgerLine {
        ReferentLedgerLine {
            speaker: speaker.into(),
            is_responder,
            content: content.into(),
        }
    }

    fn person<'a>(id: &str, name: &str, in_room: bool) -> ReferentLedgerPerson<'a> {
        ReferentLedgerPerson {
            id: id.into(),
            name: name.into(),
            aliases: None,
            pronouns: Some(("she".into(), "her".into(), "hers".into())),
            in_room,
            render_card: None,
        }
    }

    #[test]
    fn the_motivating_case() {
        let lines = vec![
            line("Friday", true, "Abigail will absolutely make that a formal finding. You've seen how she documents."),
            line("Charlie", false, "When she's not scolding Vergil"),
        ];
        let people = vec![
            person("a", "Abigail", false),
            person("v", "Vergil", true),
            person("f", "Friday", true),
        ];
        let out = build_referent_ledger(&lines, &people, "f");
        assert!(out.starts_with("## Who is being spoken of\n\n"), "{out}");
        assert!(out.contains("- **Vergil** (she/her; in the room) — last named by Charlie, in the line you are answering: “When she's not scolding Vergil”"), "{out}");
        assert!(out.contains("- **Abigail** (she/her; not in the room) — last named by you, in your last turn: “Abigail will absolutely make that a formal finding. You've seen how she documents.”"), "{out}");
        assert!(out.find("**Vergil**").unwrap() < out.find("**Abigail**").unwrap());
    }

    #[test]
    fn nobody_named_is_empty_and_logs_nothing() {
        let (out, lines) = captured_with(|| {
            build_referent_ledger(
                &[line("A", false, "hello")],
                &[person("b", "Bea", true)],
                "x",
            )
        });
        assert_eq!(out, "");
        assert!(
            lines.iter().all(|l| !l.contains("Ledger built")),
            "{lines:?}"
        );
        let (out, lines) = captured_with(|| build_closing_echo(Some("  "), "Ann"));
        assert_eq!(out, "");
        assert!(
            lines.iter().all(|l| !l.contains("Closing echo built")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_debug_lines_carry_v4s_keys() {
        let (_, lines) = captured_with(|| {
            build_referent_ledger(
                &[line("A", false, "Bea said so")],
                &[person("b", "Bea", true)],
                "x",
            )
        });
        let l = lines
            .iter()
            .find(|l| l.contains("[ReferentLedger] Ledger built"))
            .expect("ledger line");
        assert!(l.starts_with("DEBUG"), "{l}");
        for key in [
            "context=chat.context.referent-ledger",
            "windowLines=1",
            "candidates=1",
            "listedJson=[\"Bea\"]",
            "cards=0",
        ] {
            assert!(l.contains(key), "{key} in {l}");
        }
        let (_, lines) = captured_with(|| build_closing_echo(Some("hi"), "Ann"));
        let l = lines
            .iter()
            .find(|l| l.contains("[ReferentLedger] Closing echo built"))
            .expect("echo line");
        for key in ["length=2", "partial=false"] {
            assert!(l.contains(key), "{key} in {l}");
        }
    }

    #[test]
    fn echo_quotes_each_line() {
        assert_eq!(
            build_closing_echo(Some("one\n\ntwo"), "Ann"),
            "## The line you are answering\n\nWhat Ann just said, which is what you are replying to:\n\n> one\n>\n> two"
        );
    }

    #[test]
    fn a_surrogate_at_the_echo_cut_rounds_shorter() {
        // 1,499 'a' then an astral char straddling the 1,500-unit cut from the end.
        let p = format!("😀{}", "a".repeat(1499));
        let out = build_closing_echo(Some(&p), "Ann");
        assert!(out.contains(&format!("> …{}", "a".repeat(1499))), "{out}");
    }
}
