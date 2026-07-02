//! Port of v4's lib/chat/carina-parser.ts — inline LLM query detection.
//!
//! Carina (the "reference desk") lets users and LLM characters pose a quick
//! question to a designated answerer character with a compact `@Name:` markup.
//! This module extracts that markup from raw message text.
//!
//! Forms (spec: docs/developer/features/carina.md):
//!   @CharName: question          → public answer
//!   @CharName? question          → whispered answer (asker only)
//!   @CharName: "quoted question" → quoted (consumes up to the matching close)
//!   @CharName? 'quoted question' → straight or smart quotes both work
//!
//! Rules:
//!   - Detection is per-line; the `@` must begin the line.
//!   - Only the FIRST line that yields a real (non-empty) question fires; any
//!     later `@Name` lines are ignored (one query per message).
//!   - Quoted questions never span multiple lines (we operate per-line).
//!
//! This is a pure function — no imports, trivially unit-testable. It is the
//! Carina counterpart to `detectAndConvertRngPatterns`.

use crate::jsstr::js_trim;
use regex::Regex;
use std::sync::LazyLock;

/// Answerer character name (word chars + interior spaces; trimmed), the
/// separator's whisper flag, and the question text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarinaQuery {
    /// Answerer character name (word chars + interior spaces; trimmed).
    pub character_name: String,
    /// `?` separator → whisper to the asker only; `:` separator → public.
    pub whisper: bool,
    /// The question text (quotes stripped when a matching pair was present).
    pub question: String,
}

/// Opening-quote → closing-quote pairing. Straight quotes pair with
/// themselves; smart quotes pair with their counterparts. The spec's
/// single-regex form uses a `\3` backref, which only works for straight
/// quotes (open === close); we pair quotes explicitly so smart-quote spans
/// close correctly.
fn closing_quote_for(open: char) -> Option<char> {
    match open {
        '"' => Some('"'),
        '\'' => Some('\''),
        '\u{201C}' => Some('\u{201D}'), // “ … ”
        '\u{2018}' => Some('\u{2019}'), // ‘ … ’
        _ => None,
    }
}

/// Matches a single line: `@` + name (starts and ends with a word char, may
/// contain interior spaces) + separator (`:` or `?`) + optional whitespace +
/// the remainder of the line. Quote handling for the remainder happens in
/// `extract_question` so smart quotes pair correctly.
///
/// JS `\w` is always ASCII (`[A-Za-z0-9_]`, unaffected by the `u` flag). JS
/// `.` (no `s` flag) excludes ALL line terminators — `\n`, `\r`, U+2028,
/// U+2029 — not just `\n` as Rust's default `.` does; we split the input on
/// `\r?\n` first, but a *lone* `\r` (no following `\n`) is not a JS line
/// separator and stays inside a "line" here, so `.` must still exclude it
/// (and the two Unicode separators) to match JS exactly — reproduced with an
/// explicit negated class rather than `.`. JS `\s` is reproduced via the
/// shared `JS_WS_CLASS`, not Rust regex's default (narrower) `\s`.
const JS_DOT_CLASS: &str = "[^\n\r\u{2028}\u{2029}]";

// Rust regex's `\w` defaults to Unicode-aware matching (includes e.g. `é`),
// but JS `\w` is always the ASCII class `[A-Za-z0-9_]` — reproduced with
// `(?-u:\w)`, the same technique `mentioned_characters.rs` uses for `\b`.
static LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^@((?-u:\w)(?-u:[\w ])*(?-u:\w))([?:]){}*({}*)$",
        crate::jsstr::JS_WS_CLASS,
        JS_DOT_CLASS
    ))
    .unwrap()
});

/// Extract the question from the post-separator remainder. When the first
/// character is an opening quote and a matching close quote exists later on
/// the line, return the text between them; otherwise return the whole
/// remainder (the unquoted form, which mirrors the spec regex falling
/// through to `(.*)`).
fn extract_question(rest: &str) -> String {
    if rest.is_empty() {
        return String::new();
    }
    // `rest[0]` in JS is the first UTF-16 code unit; for the quote characters
    // we pair (all BMP), matching on the first Rust `char` is equivalent.
    let open = rest.chars().next().unwrap();
    if let Some(close) = closing_quote_for(open) {
        // Search for `close` strictly after the opening character — mirrors
        // JS `rest.indexOf(close, 1)`. `open.len_utf8()` is the byte offset
        // of the first character after `open` (all candidate opens are
        // single UTF-16 units, so this can't skip past a surrogate half).
        let search_start = open.len_utf8();
        if let Some(rel_idx) = rest[search_start..].find(close) {
            let close_byte_idx = search_start + rel_idx;
            // `closeIdx > 0` in JS: since `close` was searched for starting
            // at index 1, any match found is inherently > 0, so this branch
            // is unconditionally the "matched" case once `find` succeeds.
            return js_trim(&rest[search_start..close_byte_idx]).to_string();
        }
        // No matching close quote — fall through to the unquoted form (keeps
        // the leading quote, exactly as the spec's `(.*)` alternative would).
    }
    js_trim(rest).to_string()
}

/// Parse the first Carina query from a message's raw content.
/// Returns `None` when no line yields a valid (non-empty) query.
pub fn parse_carina_query(content: Option<&str>) -> Option<CarinaQuery> {
    let content = content?;
    if content.is_empty() {
        return None;
    }

    // v4 splits on `/\r?\n/` — an optional `\r` before each `\n`, but a lone
    // `\r` (no following `\n`) is NOT a line separator.
    for line in split_crlf_or_lf(content) {
        let caps = match LINE_RE.captures(line) {
            Some(c) => c,
            None => continue,
        };

        let character_name = js_trim(&caps[1]).to_string();
        if character_name.is_empty() {
            continue;
        }

        let question = extract_question(&caps[3]);
        if question.is_empty() {
            // An `@Name:` with no question text isn't a usable query — keep
            // scanning in case a later line carries a real one.
            continue;
        }

        return Some(CarinaQuery {
            character_name,
            whisper: &caps[2] == "?",
            question,
        });
    }

    None
}

/// Split on JS's `/\r?\n/`: each `\n`, optionally preceded by a `\r`, ends a
/// line; a lone `\r` with no following `\n` does not split.
fn split_crlf_or_lf(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut end = i;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            lines.push(&s[start..end]);
            start = i + 1;
        }
        i += 1;
    }
    lines.push(&s[start..]);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_markup_is_none() {
        assert_eq!(parse_carina_query(Some("just some text")), None);
        assert_eq!(parse_carina_query(None), None);
        assert_eq!(parse_carina_query(Some("")), None);
    }

    #[test]
    fn basic_public_and_whisper() {
        assert_eq!(
            parse_carina_query(Some("@Alice: hello")),
            Some(CarinaQuery {
                character_name: "Alice".to_string(),
                whisper: false,
                question: "hello".to_string(),
            })
        );
        assert_eq!(
            parse_carina_query(Some("@Bob? are you there")),
            Some(CarinaQuery {
                character_name: "Bob".to_string(),
                whisper: true,
                question: "are you there".to_string(),
            })
        );
    }

    #[test]
    fn requires_two_char_name() {
        assert_eq!(parse_carina_query(Some("@A: hi")), None);
    }

    #[test]
    fn straight_quote_pairing() {
        assert_eq!(
            parse_carina_query(Some(r#"@Bob: "What was the capital?""#))
                .unwrap()
                .question,
            "What was the capital?"
        );
    }

    #[test]
    fn unterminated_quote_falls_through() {
        assert_eq!(
            parse_carina_query(Some("@Bob: \"no closing quote here"))
                .unwrap()
                .question,
            "\"no closing quote here"
        );
    }

    #[test]
    fn empty_quoted_question_is_none() {
        assert_eq!(parse_carina_query(Some(r#"@Bob: """#)), None);
    }

    #[test]
    fn skips_empty_lines_to_next() {
        let content = "@Alice:\n@Bob: real question";
        let r = parse_carina_query(Some(content)).unwrap();
        assert_eq!(r.character_name, "Bob");
        assert_eq!(r.question, "real question");
    }

    #[test]
    fn crlf_strips_trailing_cr() {
        let content = "@Alice: question\r\nmore text";
        let r = parse_carina_query(Some(content)).unwrap();
        assert_eq!(r.question, "question");
    }
}
