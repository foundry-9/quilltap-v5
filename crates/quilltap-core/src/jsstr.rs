//! JS string-semantics primitives shared across the regex-fidelity ports.
//!
//! JavaScript's whitespace set (used by `\s` and `String.prototype.trim`) and
//! its UTF-16 string length / slicing differ from Rust's native equivalents.
//! These helpers reproduce the JS behaviour exactly so ported regex and
//! string-shaping code stays byte-equal with the v4 oracle.

/// The exact set JS `\s` (and `String.prototype.trim`) treats as whitespace:
/// the ASCII control spaces + U+0020, the Unicode space separators, the
/// line/paragraph separators, and U+FEFF. This differs from Rust's
/// `char::is_whitespace` (which excludes U+FEFF and includes U+0085).
pub fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The same whitespace set as a regex character class (including the brackets),
/// for building patterns whose `\s` must match JS semantics.
pub const JS_WS_CLASS: &str = "[\t\n\u{0B}\u{0C}\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}]";

/// Trim leading/trailing JS-whitespace, matching JS `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// Trim leading JS-whitespace only, matching JS `String.prototype.trimStart`.
pub fn js_trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_ws)
}

/// Trim trailing JS-whitespace only, matching JS `String.prototype.trimEnd`.
pub fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_ws)
}

/// UTF-16 code-unit length, matching JS `String.length`.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Unicode code-point length — Zod ≥ 4.5's `util.codePointLength`, which counts
/// a surrogate PAIR as one and any other UTF-16 unit as one. A Rust `&str` holds
/// no lone surrogates, so `chars().count()` is that function exactly.
pub fn code_point_len(s: &str) -> usize {
    s.chars().count()
}

/// Zod ≥ 4.5.4 `$ZodCheckMaxLength` for strings (v4 `6e1a64ea6` moved `zod`
/// 4.4.3 → 4.5.4): "Strings are measured in Unicode code points, not UTF-16
/// units. A code point is at most two units, so a string that already fits in
/// units fits in code points; only an overflow has to be counted." Under 4.4.3
/// this was `utf16_len(s) <= max` — 101 astral characters (202 units) FAILED a
/// `.max(200)`; since 4.5.4 they pass (101 code points).
pub fn zod_len_max_ok(s: &str, max: usize) -> bool {
    let units = utf16_len(s);
    let length = if units > max {
        code_point_len(s)
    } else {
        units
    };
    length <= max
}

/// Zod ≥ 4.5.4 `$ZodCheckMinLength` for strings: "A code point is one or two
/// UTF-16 units, so fewer units than the floor can never reach it and twice the
/// floor always clears it. Only in between is the exact count in doubt." — so
/// three astral characters (6 units) now FAIL a `.min(5)` where 4.4.3 passed.
pub fn zod_len_min_ok(s: &str, min: usize) -> bool {
    let units = utf16_len(s);
    let length = if units >= min && units < min.saturating_mul(2) {
        code_point_len(s)
    } else {
        units
    };
    length >= min
}

/// Zod ≥ 4.5.4 `$ZodCheckLengthEquals` for strings: "outside `[length, length *
/// 2]` units the target is missed either way — and missed in the same direction
/// in both measures."
pub fn zod_len_eq_ok(s: &str, n: usize) -> bool {
    let units = utf16_len(s);
    let length = if units >= n && units <= n.saturating_mul(2) {
        code_point_len(s)
    } else {
        units
    };
    length == n
}

/// First `n` UTF-16 code units of `s`, decoded back to a `String` — matching JS
/// `s.slice(0, n)` for `n` within the string. BMP text round-trips exactly; a
/// cut that would split a surrogate pair (only possible with non-BMP text) is
/// decoded lossily rather than producing JS's lone-surrogate string.
pub fn utf16_truncate(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// The UTF-16 code units of `s` from index `start` to the end, decoded back to a
/// `String` — matching JS `s.slice(start)` for `0 <= start <= s.length`. BMP text
/// round-trips exactly; a `start` that would split a surrogate pair (only possible
/// with non-BMP text) is decoded lossily rather than producing JS's lone-surrogate
/// string.
pub fn utf16_slice_from(s: &str, start: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().skip(start).collect();
    String::from_utf16_lossy(&units)
}

/// The UTF-16 code-unit index of the first occurrence of `needle` in `haystack`
/// at or after the UTF-16 offset `from`, matching JS
/// `haystack.indexOf(needle, from)`. Returns `None` for no match (JS `-1`). An
/// empty needle returns `min(from, len)` (JS's empty-string search). All indices
/// are UTF-16 code units, so surrogate offsets align with JS `String.prototype`.
pub fn js_index_of(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let hay: Vec<u16> = haystack.encode_utf16().collect();
    let nee: Vec<u16> = needle.encode_utf16().collect();
    if nee.is_empty() {
        return Some(from.min(hay.len()));
    }
    if nee.len() > hay.len() {
        return None;
    }
    let start = from.min(hay.len());
    let last = hay.len() - nee.len();
    (start..=last).find(|&i| hay[i..i + nee.len()] == nee[..])
}

/// The UTF-16 code-unit index of the LAST occurrence of `needle` in `haystack`,
/// matching JS `haystack.lastIndexOf(needle)` (no `fromIndex` — the whole string
/// is searched). Returns `None` for no match (JS `-1`). An empty needle returns
/// `Some(haystack.len())` (JS's empty-string search returns the string length).
/// All indices are UTF-16 code units, so surrogate offsets align with JS
/// `String.prototype`.
pub fn js_last_index_of(haystack: &str, needle: &str) -> Option<usize> {
    let hay: Vec<u16> = haystack.encode_utf16().collect();
    let nee: Vec<u16> = needle.encode_utf16().collect();
    if nee.is_empty() {
        return Some(hay.len());
    }
    if nee.len() > hay.len() {
        return None;
    }
    let last = hay.len() - nee.len();
    (0..=last).rev().find(|&i| hay[i..i + nee.len()] == nee[..])
}

#[cfg(test)]
mod zod_length_tests {
    use super::*;

    // Zod 4.5.4's code-point window rules (v4 `6e1a64ea6`), measured against
    // `node_modules/zod/v4/core/checks.js` at the `d883a5ee1` unification.
    #[test]
    fn zod_length_checks_count_code_points_only_inside_the_window() {
        let hats = |n: usize| "\u{1F3A9}".repeat(n); // 🎩 = 2 UTF-16 units, 1 code point
                                                     // max: 101 astral chars (202 units) pass a .max(200) since 4.5.4; 201 fail.
        assert!(zod_len_max_ok(&hats(101), 200));
        assert!(!zod_len_max_ok(&hats(201), 200));
        assert!(zod_len_max_ok(&"a".repeat(200), 200));
        assert!(!zod_len_max_ok(&"a".repeat(201), 200));
        // min: 3 astral chars (6 units) FAIL a .min(5) — inside [5, 10) the code
        // points decide; 5 astral chars (10 units) clear it without counting.
        assert!(!zod_len_min_ok(&hats(3), 5));
        assert!(zod_len_min_ok(&hats(5), 5));
        assert!(zod_len_min_ok(&"a".repeat(5), 5));
        assert!(!zod_len_min_ok(&"a".repeat(4), 5));
        assert!(zod_len_min_ok(&hats(1), 1));
        // eq: 64 astral chars (128 units) EQUAL a .length(64); 32 (64 units) do not.
        assert!(zod_len_eq_ok(&hats(64), 64));
        assert!(!zod_len_eq_ok(&hats(32), 64));
        assert!(zod_len_eq_ok(&"a".repeat(64), 64));
        assert_eq!(code_point_len(&hats(3)), 3);
        assert_eq!(utf16_len(&hats(3)), 6);
    }
}
