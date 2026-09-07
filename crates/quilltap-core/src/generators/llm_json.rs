//! v4 `lib/llm/llm-json.ts` — the cheap-LLM JSON parsing helpers.
//!
//! "Shared plumbing for every path that asks an LLM for structured JSON and has
//! to survive what actually comes back: markdown code fences, raw control
//! characters inside string literals, and output truncated at the maxTokens
//! boundary. `parseLLMJson` is the front door; the other exports are its
//! individually-testable stages." (v4's module header, carried.)
//!
//! Every v4 arm THROWS on failure; the Rust `Result` **is** the `catch`, so
//! [`parse_llm_json`] returns `Err` exactly where v4 throws.
//!
//! ## JS-fidelity notes (measured, not assumed — Node 24.13.1, 2026-09-07)
//!
//! * **Whitespace.** Every `\s` and every `.trim()` here is the JS class, not
//!   Rust's: [`crate::jsstr::js_trim`] / [`crate::jsstr::JS_WS_CLASS`]. JS trims
//!   U+FEFF and does not trim U+0085; `char::is_whitespace` is the reverse.
//! * **Case folding.** `parseLLMJsonObject`'s fence regex carries the JS `i`
//!   flag WITHOUT `u`, whose Canonicalize step refuses to fold a non-ASCII code
//!   unit onto an ASCII one — so v4's `/```(?:json)?/i` does NOT match `ſon`
//!   (U+017F). Rust's Unicode-aware `(?i)s` DOES. The port therefore spells the
//!   literal `(?i-u:json)`, ASCII-insensitive only.
//! * **Iteration width.** `escapeControlCharsInStrings` walks UTF-16 code units
//!   (`text[i]` / `charCodeAt(i)`) while `repairTruncatedJson` walks code points
//!   (`for…of`). Both are ported with `chars()`, which is exact for either: the
//!   only units either machine inspects are `"`, `\` and code ≤ 0x1F — all BMP
//!   non-surrogate — and both halves of a surrogate pair are copied through
//!   untouched, reconstituting the pair.
//! * **`JSON.parse` vs `serde_json` — ONE recorded divergence class: lone
//!   surrogates.** `JSON.parse('{"a":"\\ud800"}')` succeeds in V8 and yields an
//!   ill-formed JS string; `serde_json` refuses it ("lone leading surrogate in
//!   hex escape"), because a Rust `String` cannot hold one. Every other axis
//!   measured agrees: duplicate keys take the LAST value at the FIRST key's
//!   position (serde_json's `preserve_order` `IndexMap::insert` does exactly
//!   that), a raw control character inside a string literal is refused by both
//!   (which is the whole reason [`escape_control_chars_in_strings`] exists), and
//!   `-0` / `1e2` / `1.0` all parse. The divergence is unreachable from the
//!   corpus (a lone surrogate cannot survive an NDJSON round trip either) and is
//!   pinned by [`tests::lone_surrogate_is_the_one_recorded_divergence`].
//! * **Two further `JSON.parse`/`serde_json` classes, RECORDED not measured
//!   (the §3 unification review):** nesting depth — `serde_json` refuses more
//!   than 128 levels (`RecursionLimitExceeded`) where `JSON.parse` accepts, so
//!   a valid but absurdly deep document falls through both repairs and
//!   [`parse_llm_json`] answers `Err` where v4 answers the value; and numeric
//!   range — `1e400` is `Infinity` under `JSON.parse` and `NumberOutOfRange`
//!   under serde. Both are unreachable from any model answer this module is
//!   fed; they are listed so the "every other axis agrees" sentence above is
//!   read with its two exceptions.
//! * **Error MESSAGE bytes are NOT v4's.** v4 propagates V8's `SyntaxError`;
//!   [`LlmJsonError`] carries serde's wording. No ported surface may byte-compare
//!   a parse-failure sentence without measuring v4's first.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::jsstr::{js_trim, JS_WS_CLASS};

/// A failed parse — the Rust stand-in for v4's thrown `SyntaxError`. The
/// `message` is serde's, not V8's (see the module header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmJsonError {
    pub message: String,
}

impl fmt::Display for LlmJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LlmJsonError {}

fn ws_regex(pattern: &str) -> Regex {
    Regex::new(&pattern.replace("{WS}", JS_WS_CLASS)).expect("static llm-json regex")
}

/// v4 `/^```json\s*/`
static LEAD_JSON_FENCE: LazyLock<Regex> = LazyLock::new(|| ws_regex(r"^```json{WS}*"));
/// v4 `/^```\s*/`
static LEAD_FENCE: LazyLock<Regex> = LazyLock::new(|| ws_regex(r"^```{WS}*"));
/// v4 `/\s*```$/`
static TRAIL_FENCE: LazyLock<Regex> = LazyLock::new(|| ws_regex(r"{WS}*```$"));
/// v4 `/,\s*$/`
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| ws_regex(r",{WS}*$"));
/// v4 `/,?\s*"[^"]*"\s*:\s*$/` — a trailing key with no value.
static TRAILING_KEY: LazyLock<Regex> = LazyLock::new(|| ws_regex(r#",?{WS}*"[^"]*"{WS}*:{WS}*$"#));
/// v4 `/```(?:json)?\s*([\s\S]*?)```/i` — a fence ANYWHERE in the text. The `i`
/// is ASCII-only on purpose (see the module header's case-folding note).
static ANY_FENCE: LazyLock<Regex> =
    LazyLock::new(|| ws_regex(r"```(?:(?i-u:json))?{WS}*((?s:.)*?)```"));

/// v4 `stripCodeFences` — strip a markdown code fence that opens at the very
/// start of the response (and its closing fence at the very end).
pub fn strip_code_fences(text: &str) -> String {
    let trimmed = js_trim(text);
    // Remove ```json ... ``` or ``` ... ``` (single source for every cheap-LLM
    // JSON parser; the regex form tolerates both newline- and inline-delimited
    // fences).
    let cleaned = if trimmed.starts_with("```json") {
        let once = LEAD_JSON_FENCE.replace(trimmed, "");
        TRAIL_FENCE.replace(&once, "").into_owned()
    } else if trimmed.starts_with("```") {
        let once = LEAD_FENCE.replace(trimmed, "");
        TRAIL_FENCE.replace(&once, "").into_owned()
    } else {
        trimmed.to_string()
    };
    js_trim(&cleaned).to_string()
}

/// v4 `repairTruncatedJson` — "Attempt to repair truncated JSON from LLM output
/// that was cut off by maxTokens limits. Closes unclosed strings, arrays, and
/// objects."
pub fn repair_truncated_json(text: &str) -> String {
    let mut repaired = js_trim(text).to_string();

    // If it already parses, return as-is.
    if serde_json::from_str::<Value>(&repaired).is_ok() {
        return repaired;
    }

    // Remove trailing comma (common at truncation point).
    repaired = TRAILING_COMMA.replace(&repaired, "").into_owned();

    // Track bracket/brace depth to close unclosed structures. NOTE the order v4
    // fixed here and this port keeps: the stack is walked over the string as it
    // stands AFTER the first trailing-comma strip and BEFORE the unclosed-string
    // close and the trailing-key strip below.
    let mut in_string = false;
    let mut escaped = false;
    let mut stack: Vec<char> = Vec::new();

    for ch in repaired.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }

        if ch == '{' {
            stack.push('}');
        } else if ch == '[' {
            stack.push(']');
        } else if ch == '}' || ch == ']' {
            stack.pop();
        }
    }

    // If we're mid-string, close it.
    if in_string {
        repaired.push('"');
    }

    // Remove any trailing key without a value (e.g. `"field":` or `"field": `).
    repaired = TRAILING_KEY.replace(&repaired, "").into_owned();

    // Remove trailing comma again after cleanup.
    repaired = TRAILING_COMMA.replace(&repaired, "").into_owned();

    // Close all unclosed brackets/braces.
    while let Some(closer) = stack.pop() {
        repaired.push(closer);
    }

    repaired
}

/// v4 `escapeControlCharsInStrings` — "Escape raw control characters
/// (newlines, tabs, etc.) that appear *inside* a JSON string literal. LLMs
/// routinely emit a literal newline within a string value instead of the `\n`
/// escape sequence, which makes JSON.parse throw 'Bad control character in
/// string literal'. This walks the text with a small string-aware state machine
/// and escapes only the control characters that fall inside a string, leaving
/// structural whitespace between tokens untouched."
pub fn escape_control_chars_in_strings(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in text.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            result.push(ch);
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            result.push(ch);
            continue;
        }

        let code = ch as u32;
        if in_string && code <= 0x1f {
            match ch {
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '\u{8}' => result.push_str("\\b"),
                '\u{c}' => result.push_str("\\f"),
                _ => result.push_str(&format!("\\u{code:04x}")),
            }
            continue;
        }

        result.push(ch);
    }

    result
}

/// v4 `parseLLMJsonObject` — "Pull the JSON *object* out of a response that may
/// wrap it in prose and/or a code fence anywhere in the text (a verdict such as
/// \"Here you go: ```json {...}``` hope that helps\"). `stripCodeFences` only
/// recognises a fence at the very start; this matches one anywhere, then slices
/// from the first `{` to the last `}` before handing off to [`parse_llm_json`]
/// for the usual repairs. Throws when nothing parses."
pub fn parse_llm_json_object(text: &str) -> Result<Value, LlmJsonError> {
    let trimmed = js_trim(text);
    let body: &str = match ANY_FENCE.captures(trimmed) {
        Some(c) => js_trim(c.get(1).map(|m| m.as_str()).unwrap_or("")),
        None => trimmed,
    };
    let start = body.find('{');
    let end = body.rfind('}');
    let json_text = match (start, end) {
        // v4: `start >= 0 && end > start`
        (Some(s), Some(e)) if e > s => &body[s..=e],
        _ => body,
    };
    parse_llm_json(json_text)
}

/// v4 `parseLLMJson` — "Parse JSON from LLM output, handling code fences,
/// truncated output, and common formatting issues from LLM responses."
///
/// The three-stage chain, in v4's order: parse → escape control chars → parse →
/// repair truncation → parse. Each `catch` is a fall-through; the last failure
/// is the returned `Err`.
pub fn parse_llm_json(text: &str) -> Result<Value, LlmJsonError> {
    let cleaned = strip_code_fences(text);
    match serde_json::from_str::<Value>(&cleaned) {
        Ok(v) => Ok(v),
        Err(_) => {
            // LLMs commonly emit raw control characters inside string literals
            // and/or truncate output at the maxTokens boundary. Escape control
            // chars first, then repair any truncation before a final parse
            // attempt.
            let escaped = escape_control_chars_in_strings(&cleaned);
            match serde_json::from_str::<Value>(&escaped) {
                Ok(v) => Ok(v),
                Err(_) => {
                    let repaired = repair_truncated_json(&escaped);
                    serde_json::from_str::<Value>(&repaired).map_err(|e| LlmJsonError {
                        message: e.to_string(),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ONE measured `JSON.parse`/`serde_json` divergence (module header):
    /// V8 accepts a lone-surrogate escape, serde_json refuses it. Measured on
    /// Node 24.13.1 — `JSON.parse` of a `\ud800` escape returns an ill-formed
    /// JS string. Nothing in the corpus can reach it (a lone surrogate cannot
    /// survive an NDJSON round trip either), so it is pinned here instead.
    #[test]
    fn lone_surrogate_is_the_one_recorded_divergence() {
        assert!(parse_llm_json(r#"{"a":"\ud800"}"#).is_err());
        // A well-formed surrogate PAIR is accepted by both.
        let v = parse_llm_json("{\"a\":\"\u{1f600}\"}").expect("pair parses");
        assert_eq!(v["a"], "\u{1f600}");
    }

    /// Duplicate keys: JS keeps the FIRST key's position with the LAST value
    /// (measured: `{"a":1,"b":2,"a":3}` parses to `{"a":3,"b":2}`), which is
    /// exactly what serde_json's `preserve_order` `IndexMap` does.
    #[test]
    fn duplicate_keys_take_the_last_value_at_the_first_position() {
        let v = parse_llm_json(r#"{"a":1,"b":2,"a":3}"#).unwrap();
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"a":3,"b":2}"#);
    }

    /// The JS `i` flag WITHOUT `u` refuses to fold U+017F onto ASCII `s`
    /// (Canonicalize never maps a non-ASCII code unit onto an ASCII one), so
    /// v4's fence regex does NOT read `jſon` as a `json` hint — measured on
    /// Node 24.13.1: the capture comes back as `"jſon\n{\"a\":1}\n"`, hint
    /// and all. A Unicode-aware `(?i)json` in Rust WOULD fold it and capture
    /// only the body, so the port spells `(?i-u:json)`. Changing that back to
    /// `(?i)` reddens the third assertion.
    #[test]
    fn fence_case_folding_is_ascii_only() {
        let capture = |s: &str| {
            ANY_FENCE
                .captures(s)
                .map(|c| c.get(1).unwrap().as_str().to_string())
        };
        assert_eq!(
            capture("```json\n{\"a\":1}\n```").as_deref(),
            Some("{\"a\":1}\n")
        );
        assert_eq!(
            capture("```JSON\n{\"a\":1}\n```").as_deref(),
            Some("{\"a\":1}\n")
        );
        assert_eq!(
            capture("```j\u{17f}on\n{\"a\":1}\n```").as_deref(),
            Some("j\u{17f}on\n{\"a\":1}\n")
        );
    }

    /// JS whitespace, not Rust's: U+FEFF is trimmed, U+0085 is not.
    #[test]
    fn trimming_uses_the_js_whitespace_set() {
        assert_eq!(strip_code_fences("\u{feff}{\"a\":1}\u{feff}"), r#"{"a":1}"#);
        assert_eq!(
            strip_code_fences("\u{85}{\"a\":1}\u{85}"),
            "\u{85}{\"a\":1}\u{85}"
        );
    }

    #[test]
    fn escape_leaves_structural_whitespace_alone() {
        assert_eq!(
            escape_control_chars_in_strings("{\n\"a\": 1\n}"),
            "{\n\"a\": 1\n}"
        );
        assert_eq!(
            escape_control_chars_in_strings("{\"a\": \"x\ny\"}"),
            r#"{"a": "x\ny"}"#
        );
        // A control character with no short escape takes the \u00XX form.
        assert_eq!(
            escape_control_chars_in_strings("{\"a\": \"x\u{1}y\"}"),
            r#"{"a": "x\u0001y"}"#
        );
    }

    /// A surrogate pair inside a string survives the code-unit to code-point
    /// port unchanged (module header, "Iteration width").
    #[test]
    fn astral_characters_pass_through_both_state_machines() {
        assert_eq!(
            escape_control_chars_in_strings("{\"a\": \"\u{1f600}\nb\"}"),
            "{\"a\": \"\u{1f600}\\nb\"}"
        );
        assert_eq!(
            repair_truncated_json("{\"a\": \"\u{1f600}"),
            "{\"a\": \"\u{1f600}\"}"
        );
    }
}
