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

/// UTF-16 code-unit length, matching JS `String.length`.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// First `n` UTF-16 code units of `s`, decoded back to a `String` — matching JS
/// `s.slice(0, n)` for `n` within the string. BMP text round-trips exactly; a
/// cut that would split a surrogate pair (only possible with non-BMP text) is
/// decoded lossily rather than producing JS's lone-surrogate string.
pub fn utf16_truncate(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}
