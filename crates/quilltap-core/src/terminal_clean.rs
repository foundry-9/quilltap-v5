//! Terminal output cleaning — a faithful port of v4's
//! `lib/terminal/clean-output.ts` (`stripAnsi` / `applyBackspaces` /
//! `applyCarriageReturns` / `cleanTerminalOutput`).
//!
//! These are pure string transforms shared between the `terminal_read` tool
//! handler (which serves cleaned scrollback to the LLM) and v4's PTY manager.
//! The `terminal_read` handler runs [`clean_terminal_output`] on the sliced
//! scrollback; the raw bytes survive only when `raw === true`.
//!
//! ## The ANSI-strip regex — JS semantics, hand-reproduced
//!
//! v4's [`strip_ansi`] regex is:
//!
//! ```text
//! /\x1B\[[0-?]*[ -/]*[@-~]|\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)|\x1B[ -/][0-~]|\x1B[0-~]|\x1B/g
//! ```
//!
//! Five alternatives, in order (JS alternation is leftmost / first-match-wins):
//!   1. **CSI**: `ESC [` then any of `[0-?]` (params), any of `[ -/]`
//!      (intermediates), one `[@-~]` (final).
//!   2. **OSC**: `ESC ]` then any run of non-`BEL`/non-`ESC` chars, terminated
//!      by `BEL` (`\x07`) or `ST` (`ESC \`).
//!   3. **two-byte**: `ESC` then one intermediate `[ -/]` then one `[0-~]`.
//!   4. **single-byte**: `ESC` then one `[0-~]` (Fp/Fe/Fs).
//!   5. **orphan `ESC`**: a lone trailing/unmatched `ESC`.
//!
//! The Rust `regex` crate supports all of this directly (no backreferences, no
//! lookaround). The one subtlety is that these are **byte ranges over Unicode
//! scalars**: JS `RegExp` (no `u` flag here) matches UTF-16 code units, but every
//! character class involved (`\x1B`, `[0-?]`, `[ -/]`, `[@-~]`, `[0-~]`,
//! `[^\x07\x1B]`) is bounded at or below `~` (U+007E) except the negated OSC
//! class `[^\x07\x1B]`, which in JS matches ANY code unit other than BEL/ESC —
//! including code units ≥ U+0080. To reproduce that exactly the OSC body class is
//! written `[^\x07\x1B]` with the Unicode flag left ON (the `regex` crate default),
//! so it matches any Unicode scalar except BEL/ESC — the same set of characters JS
//! consumes between `ESC ]` and its terminator. All other classes are pure
//! ASCII-range, so Unicode-mode matching is identical to JS's code-unit matching
//! for them. The result is byte-for-byte faithful over the terminal value space
//! (ANSI sequences are ASCII; the OSC body may carry arbitrary text).

use std::sync::LazyLock;

use regex::Regex;

/// v4's `stripAnsi` regex — the five-alternative ANSI escape matcher. Compiled
/// once. Note `\x1b` is the ESC control character; `[ -/]` is the intermediate
/// range U+0020..U+002F; `[@-~]` the CSI final range U+0040..U+007E; `[0-~]`
/// U+0030..U+007E; `[0-?]` U+0030..U+003F.
static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "\u{1b}\\[[0-?]*[ -/]*[@-~]\
         |\u{1b}\\][^\u{07}\u{1b}]*(?:\u{07}|\u{1b}\\\\)\
         |\u{1b}[ -/][0-~]\
         |\u{1b}[0-~]\
         |\u{1b}",
    )
    .expect("ANSI strip regex compiles")
});

/// Strip ANSI escape sequences. Faithful port of v4 `stripAnsi`.
pub fn strip_ansi(input: &str) -> String {
    ANSI_RE.replace_all(input, "").into_owned()
}

/// Apply backspace (`0x08`) by erasing the prior character on the same line.
/// Orphan backspaces at the start of a line (or of the whole string) are dropped
/// silently. Faithful port of v4 `applyBackspaces`.
///
/// v4 iterates `input[i]` over UTF-16 code units and appends the character, but
/// the only character it ever *removes* is the last-appended one via
/// `out.slice(0, -1)` (which drops one UTF-16 code unit). For non-BMP characters
/// v4's `input[i]` would yield a lone surrogate; the terminal value space here is
/// scrollback text where backspaces erase ordinary (BMP) characters, so iterating
/// over Unicode scalar values matches v4 for that space. The backspace-erase pops
/// the last scalar (`out.pop()`), the direct analogue of `out.slice(0, -1)`.
pub fn apply_backspaces(input: &str) -> String {
    if !input.contains('\u{08}') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch == '\u{08}' {
            // Erase the prior character on the same line; drop orphan BS
            // (nothing to erase, or the prior char is a newline).
            if !out.is_empty() && !out.ends_with('\n') {
                out.pop();
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Treat lone carriage returns (not part of CRLF) as "reset to start of line":
/// keep only the content after the last `\r` on each line. Faithful port of v4
/// `applyCarriageReturns`.
pub fn apply_carriage_returns(input: &str) -> String {
    if !input.contains('\r') {
        return input.to_string();
    }
    // v4: input.replace(/\r\n/g, '\n') then split on '\n', keep tail-after-last-\r.
    let normalized = input.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| match line.rfind('\r') {
            Some(idx) => &line[idx + 1..],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Full cleaning pipeline: carriage-returns ∘ backspaces ∘ strip-ANSI. Faithful
/// port of v4 `cleanTerminalOutput`. An empty input is returned unchanged (v4:
/// `if (!input) return input`).
pub fn clean_terminal_output(input: &str) -> String {
    if input.is_empty() {
        return input.to_string();
    }
    apply_carriage_returns(&apply_backspaces(&strip_ansi(input)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_csi_color() {
        // ESC[31mred ESC[0m -> "red"
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn strip_ansi_cursor_and_clear() {
        // ESC[2J (clear), ESC[H (home), ESC[1;5H (position)
        assert_eq!(strip_ansi("\u{1b}[2J\u{1b}[Habc\u{1b}[1;5Hdef"), "abcdef");
    }

    #[test]
    fn strip_ansi_osc_bel_and_st() {
        // OSC set-title terminated by BEL, and by ST (ESC \).
        assert_eq!(strip_ansi("\u{1b}]0;my title\u{07}text"), "text");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{1b}\\text"), "text");
    }

    #[test]
    fn strip_ansi_two_byte_and_single_byte() {
        // ESC ( B  (designate charset — two-byte: ESC intermediate final)
        assert_eq!(strip_ansi("\u{1b}(Bhello"), "hello");
        // ESC =  and ESC >  (single-byte)
        assert_eq!(strip_ansi("\u{1b}=\u{1b}>x"), "x");
        // ESC 7 (save cursor — single byte)
        assert_eq!(strip_ansi("\u{1b}7saved"), "saved");
    }

    #[test]
    fn strip_ansi_orphan_esc() {
        // A lone trailing ESC is dropped.
        assert_eq!(strip_ansi("done\u{1b}"), "done");
    }

    #[test]
    fn strip_ansi_osc_body_non_ascii() {
        // The OSC body may carry non-ASCII text (the negated class matches it).
        assert_eq!(strip_ansi("\u{1b}]0;café ☕\u{07}rest"), "rest");
    }

    #[test]
    fn strip_ansi_noop_when_no_esc() {
        assert_eq!(strip_ansi("plain text 123"), "plain text 123");
    }

    #[test]
    fn backspaces_erase_prior_char() {
        // "abc\b\bx" -> "ax"
        assert_eq!(apply_backspaces("abc\u{08}\u{08}x"), "ax");
    }

    #[test]
    fn backspaces_orphan_at_line_start_dropped() {
        // BS at absolute start: dropped, nothing to erase.
        assert_eq!(apply_backspaces("\u{08}hi"), "hi");
        // BS right after a newline: does not erase across the newline.
        assert_eq!(apply_backspaces("a\n\u{08}b"), "a\nb");
    }

    #[test]
    fn backspaces_noop_when_absent() {
        assert_eq!(apply_backspaces("no backspaces"), "no backspaces");
    }

    #[test]
    fn carriage_returns_reset_line() {
        // "loading...\rdone" -> "done"
        assert_eq!(apply_carriage_returns("loading...\rdone"), "done");
    }

    #[test]
    fn carriage_returns_crlf_preserved_as_newline() {
        // CRLF is normalized to LF, not treated as a reset.
        assert_eq!(apply_carriage_returns("line1\r\nline2"), "line1\nline2");
    }

    #[test]
    fn carriage_returns_multiple_per_line() {
        // Keep content after the LAST \r on the line.
        assert_eq!(apply_carriage_returns("a\rb\rc"), "c");
    }

    #[test]
    fn carriage_returns_noop_when_absent() {
        assert_eq!(apply_carriage_returns("plain\ntext"), "plain\ntext");
    }

    #[test]
    fn clean_full_pipeline() {
        // ANSI + CR progress redraw + backspace correction.
        let input =
            "\u{1b}[32mProgress: 50%\rProgress: 100%\u{1b}[0m done\u{08}\u{08}\u{08}\u{08}DONE";
        assert_eq!(clean_terminal_output(input), "Progress: 100% DONE");
    }

    #[test]
    fn clean_empty_unchanged() {
        assert_eq!(clean_terminal_output(""), "");
    }
}
