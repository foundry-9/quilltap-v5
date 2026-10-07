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

// ── V8's `JSON.parse` failure wording (P4.154; moved here from
// `generators::optimizer` at P4.162 — a JS-string-semantics twin, not an
// optimizer concern; five modules read it). ──

/// V8's `JSON.parse` failure wording (`json-parser.cc`), MEASURED on Node
/// 24.13.1 — the text v4 logs wherever it renders a caught parse failure's
/// `err.message`. The whole corpus is recorded by the tier-1 family
/// `v8_json_parse_message_equivalence` (`harness/oracle/cases/v8-json-parse-
/// messages.ts`); the unit table below is its fixed-row twin.
///
/// The scan is V8's `JsonParser` walk over UTF-16 units, reporting its FIRST
/// failure:
/// - a value position that holds no value: end of input → `Unexpected end of
///   JSON input`; otherwise V8's token message — the whole source when it is
///   one of V8's special strings (`undefined`, `NaN`, `Infinity`, `[object
///   Object]` → `"<source>" is not valid JSON`), else `Unexpected token '<c>',
///   <context> is not valid JSON`, where the context is the whole source under
///   21 units, else a 10-unit window on the side(s) of the position with `...`
///   marking the elided side (`GetErrorMessageWithEllipses`);
/// - a `t`/`f`/`n` literal is scanned against its keyword (`ScanLiteral`): the
///   first mismatching unit is the token at ITS position; a source ending
///   inside the keyword is `Unexpected end of JSON input`;
/// - every other failure is a FIXED template `… at position N (line L column
///   C)` — `Expected property name or '}' in JSON` (after `{`), `Expected
///   double-quoted property name in JSON` (after a member's `,`), `Expected ':'
///   after property name in JSON`, `Expected ',' or '}' after property value in
///   JSON`, `Expected ',' or ']' after array element in JSON`, `Unterminated
///   string in JSON` (at the end), `Bad control character in string literal in
///   JSON`, `Bad escaped character in JSON` (an escape at the end of input is
///   `Unexpected end of JSON input`), `Bad Unicode escape in JSON` (at the
///   first non-hex unit), `No number after minus sign in JSON`, `Unexpected
///   number in JSON` (a leading zero's next digit), `Unterminated fractional
///   number in JSON`, `Exponent part is missing a number in JSON`, and
///   `Unexpected non-whitespace character after JSON` (trailing input) — where
///   N is the UTF-16 offset, L counts `\n`, `\r` and `\r\n` (one break) from
///   the source start, and C is `1 +` the units since the last break
///   (`CalculateFileLocation`).
///
/// `None` ONLY where V8 ACCEPTS the text. A caller reaches this after serde
/// refused it, so `None` is serde refusing what V8 takes — a number past f64
/// (`1e400` is `Infinity` to V8), a lone-surrogate `\u` escape, nesting past
/// serde's 128-deep limit — and the caller falls back to serde's message, a
/// RECORDED divergence. A token or window edge that splits an astral pair
/// renders its lone unit as U+FFFD (a `String` cannot carry it; V8's message
/// does) — the family's one lossy class.
pub fn v8_json_parse_message(text: &str) -> Option<String> {
    const EOS: &str = "Unexpected end of JSON input";

    /// A container still open on V8's continuation stack.
    enum Open {
        Object,
        Array,
    }

    struct Scan<'a> {
        source: &'a str,
        units: Vec<u16>,
        pos: usize,
    }

    impl Scan<'_> {
        fn peek(&self) -> Option<u16> {
            self.units.get(self.pos).copied()
        }

        fn skip_whitespace(&mut self) {
            while matches!(self.peek(), Some(0x20 | 0x09 | 0x0a | 0x0d)) {
                self.pos += 1;
            }
        }

        fn digit(&self) -> bool {
            matches!(self.peek(), Some(0x30..=0x39))
        }

        fn skip_digits(&mut self) {
            while self.digit() {
                self.pos += 1;
            }
        }

        /// A fixed template at the current position, with V8's location.
        fn at(&self, template: &str) -> String {
            let units = &self.units;
            let end = self.pos;
            let (mut line, mut last_break, mut i) = (1, 0, 0);
            while i < end {
                if units[i] == 0x0d && i + 1 < end && units[i + 1] == 0x0a {
                    i += 1; // `\r\n` counts as one break
                }
                if units[i] == 0x0d || units[i] == 0x0a {
                    line += 1;
                    last_break = i + 1;
                }
                i += 1;
            }
            let column = 1 + end - last_break;
            format!("{template} at position {end} (line {line} column {column})")
        }

        /// V8's message for an unexpected token at `pos`.
        fn token(&self, pos: usize) -> String {
            if pos >= self.units.len() {
                return EOS.to_string();
            }
            if matches!(
                self.source,
                "[object Object]" | "undefined" | "Infinity" | "NaN"
            ) {
                return format!("\"{}\" is not valid JSON", self.source);
            }
            const K: usize = 10;
            let len = self.units.len();
            let sub = |a: usize, b: usize| String::from_utf16_lossy(&self.units[a..b]);
            let token = sub(pos, pos + 1);
            if len < 2 * K + 1 {
                format!(
                    "Unexpected token '{token}', \"{}\" is not valid JSON",
                    sub(0, len)
                )
            } else if pos < K {
                format!(
                    "Unexpected token '{token}', \"{}\"... is not valid JSON",
                    sub(0, pos + K)
                )
            } else if pos >= len - K {
                format!(
                    "Unexpected token '{token}', ...\"{}\" is not valid JSON",
                    sub(pos - K, len)
                )
            } else {
                format!(
                    "Unexpected token '{token}', ...\"{}\"... is not valid JSON",
                    sub(pos - K, pos + K)
                )
            }
        }

        /// `ScanJsonString` from the opening quote.
        fn string(&mut self) -> Result<(), String> {
            self.pos += 1;
            loop {
                match self.peek() {
                    None => return Err(self.at("Unterminated string in JSON")),
                    Some(0x22) => {
                        self.pos += 1;
                        return Ok(());
                    }
                    Some(0x5c) => {
                        self.pos += 1;
                        match self.peek() {
                            None => return Err(EOS.to_string()),
                            // " \ / b f n r t
                            Some(0x22 | 0x5c | 0x2f | 0x62 | 0x66 | 0x6e | 0x72 | 0x74) => {
                                self.pos += 1
                            }
                            Some(0x75) => {
                                self.pos += 1;
                                for _ in 0..4 {
                                    match self.peek() {
                                        Some(u) if u < 0x80 && (u as u8).is_ascii_hexdigit() => {
                                            self.pos += 1
                                        }
                                        _ => return Err(self.at("Bad Unicode escape in JSON")),
                                    }
                                }
                            }
                            // V8 classifies the escaped unit through its
                            // one-byte table: above U+00FF it is no escape
                            // candidate at all, so the failure is the plain
                            // context-window token (`"\’"` → `Unexpected token
                            // '’', …`), not `Bad escaped character` (the
                            // `94fbb1ae3` smalls unification, measured on Node
                            // 24.13.1: rows `escape-of-*`).
                            Some(u) if u > 0xff => return Err(self.token(self.pos)),
                            Some(_) => return Err(self.at("Bad escaped character in JSON")),
                        }
                    }
                    Some(u) if u < 0x20 => {
                        return Err(self.at("Bad control character in string literal in JSON"))
                    }
                    Some(_) => self.pos += 1,
                }
            }
        }

        /// `ParseJsonNumber` from its `-` or first digit.
        fn number(&mut self) -> Result<(), String> {
            if self.peek() == Some(0x2d) {
                self.pos += 1;
                if !self.digit() {
                    return Err(self.at("No number after minus sign in JSON"));
                }
            }
            if self.peek() == Some(0x30) {
                self.pos += 1;
                if self.digit() {
                    return Err(self.at("Unexpected number in JSON"));
                }
            } else {
                self.skip_digits();
            }
            if self.peek() == Some(0x2e) {
                self.pos += 1;
                if !self.digit() {
                    return Err(self.at("Unterminated fractional number in JSON"));
                }
                self.skip_digits();
            }
            if matches!(self.peek(), Some(0x65 | 0x45)) {
                self.pos += 1;
                if matches!(self.peek(), Some(0x2b | 0x2d)) {
                    self.pos += 1;
                }
                if !self.digit() {
                    return Err(self.at("Exponent part is missing a number in JSON"));
                }
                self.skip_digits();
            }
            Ok(())
        }

        /// `ScanLiteral`: the first mismatching unit is the token — and V8
        /// reports its TOKEN TYPE, so a digit or `-` reads `Unexpected number`
        /// and a `"` reads `Unexpected string`, each positional; anything else
        /// is the context-window token (the `94fbb1ae3` smalls unification,
        /// measured on Node 24.13.1: rows `literal-broken-by-*`).
        fn literal(&mut self, keyword: &str) -> Result<(), String> {
            for k in keyword.encode_utf16().skip(1) {
                self.pos += 1;
                match self.peek() {
                    None => return Err(EOS.to_string()),
                    Some(u) if u != k => {
                        return Err(match u {
                            0x30..=0x39 | 0x2d => self.at("Unexpected number in JSON"),
                            0x22 => self.at("Unexpected string in JSON"),
                            _ => self.token(self.pos),
                        })
                    }
                    Some(_) => {}
                }
            }
            self.pos += 1;
            Ok(())
        }

        /// A member's key and colon (`ExpectNext(STRING, missing)` then
        /// `ExpectNext(COLON, …)`).
        fn key(&mut self, missing: &str) -> Result<(), String> {
            self.skip_whitespace();
            if self.peek() != Some(0x22) {
                return Err(self.at(missing));
            }
            self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(0x3a) {
                return Err(self.at("Expected ':' after property name in JSON"));
            }
            self.pos += 1;
            Ok(())
        }

        /// V8's iterative `ParseJsonValue` over the whole source.
        fn document(&mut self) -> Result<(), String> {
            let mut open: Vec<Open> = Vec::new();
            'value: loop {
                self.skip_whitespace();
                match self.peek() {
                    None => return Err(EOS.to_string()),
                    Some(0x7b) => {
                        self.pos += 1;
                        self.skip_whitespace();
                        if self.peek() == Some(0x7d) {
                            self.pos += 1;
                        } else {
                            self.key("Expected property name or '}' in JSON")?;
                            open.push(Open::Object);
                            continue 'value;
                        }
                    }
                    Some(0x5b) => {
                        self.pos += 1;
                        self.skip_whitespace();
                        if self.peek() == Some(0x5d) {
                            self.pos += 1;
                        } else {
                            open.push(Open::Array);
                            continue 'value;
                        }
                    }
                    Some(0x22) => self.string()?,
                    Some(0x2d | 0x30..=0x39) => self.number()?,
                    Some(0x74) => self.literal("true")?,
                    Some(0x66) => self.literal("false")?,
                    Some(0x6e) => self.literal("null")?,
                    Some(_) => return Err(self.token(self.pos)),
                }
                // A value is complete: close what it completes.
                loop {
                    self.skip_whitespace();
                    match open.last() {
                        None => {
                            return match self.peek() {
                                None => Ok(()),
                                Some(_) => {
                                    Err(self.at("Unexpected non-whitespace character after JSON"))
                                }
                            };
                        }
                        Some(Open::Object) => match self.peek() {
                            Some(0x2c) => {
                                self.pos += 1;
                                self.key("Expected double-quoted property name in JSON")?;
                                continue 'value;
                            }
                            Some(0x7d) => {
                                self.pos += 1;
                                open.pop();
                            }
                            _ => {
                                return Err(
                                    self.at("Expected ',' or '}' after property value in JSON")
                                )
                            }
                        },
                        Some(Open::Array) => match self.peek() {
                            Some(0x2c) => {
                                self.pos += 1;
                                continue 'value;
                            }
                            Some(0x5d) => {
                                self.pos += 1;
                                open.pop();
                            }
                            _ => {
                                return Err(
                                    self.at("Expected ',' or ']' after array element in JSON")
                                )
                            }
                        },
                    }
                }
            }
        }
    }

    Scan {
        source: text,
        units: text.encode_utf16().collect(),
        pos: 0,
    }
    .document()
    .err()
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

/// P4.154's fixed-row pin of [`v8_json_parse_message`], moved with it (P4.162).
#[cfg(test)]
mod v8_json_parse_message_tests {
    use super::*;

    /// `v8_json_parse_message` against fixed rows of the table RECORDED on
    /// Node 24.13.1 at the `94fbb1ae3` pin (P4.154 — `harness/oracle/cases/
    /// v8-json-parse-messages.ts`; the rows were copied from that recording,
    /// never typed): the start-of-input context forms, the literal scan, the
    /// special whole-source strings, the inside-a-value templates the twin
    /// learned for dogfood #146, the `\r` / `\r\n` location rule, and a `None`
    /// where V8 accepts (serde refuses `1e400`). The tier-1 family
    /// `v8_json_parse_message_equivalence` is the proof; this is its pin.
    #[test]
    fn v8_json_parse_message_matches_the_measured_table() {
        let table: &[(&str, Option<&str>)] = &[
            ("The character is fine as she is.", Some("Unexpected token 'T', \"The charac\"... is not valid JSON")),
            ("I have no", Some("Unexpected token 'I', \"I have no\" is not valid JSON")),
            ("abc", Some("Unexpected token 'a', \"abc\" is not valid JSON")),
            ("A", Some("Unexpected token 'A', \"A\" is not valid JSON")),
            ("", Some("Unexpected end of JSON input")),
            ("   \n", Some("Unexpected end of JSON input")),
            ("Sure! Here is the JSON you asked for: []", Some("Unexpected token 'S', \"Sure! Here\"... is not valid JSON")),
            ("\n\n\t  Sure thing, here it is: {}", Some("Unexpected token 'S', \"\n\n\t  Sure thing\"... is not valid JSON")),
            ("            The character", Some("Unexpected token 'T', ...\"          The charac\"... is not valid JSON")),
            ("xxxxxxxxxxxxxxxxxxxxx", Some("Unexpected token 'x', \"xxxxxxxxxx\"... is not valid JSON")),
            ("xxxxxxxxxxxxxxxxxxxx", Some("Unexpected token 'x', \"xxxxxxxxxxxxxxxxxxxx\" is not valid JSON")),
            (".5", Some("Unexpected token '.', \".5\" is not valid JSON")),
            ("tru", Some("Unexpected end of JSON input")),
            ("n", Some("Unexpected end of JSON input")),
            ("nul", Some("Unexpected end of JSON input")),
            ("fals", Some("Unexpected end of JSON input")),
            ("null", None),
            ("no json here", Some("Unexpected token 'o', \"no json here\" is not valid JSON")),
            ("nope", Some("Unexpected token 'o', \"nope\" is not valid JSON")),
            ("nonsense that is quite long indeed", Some("Unexpected token 'o', \"nonsense th\"... is not valid JSON")),
            ("    nah, not json at all here", Some("Unexpected token 'a', \"    nah, not js\"... is not valid JSON")),
            ("null x", Some("Unexpected non-whitespace character after JSON at position 5 (line 1 column 6)")),
            ("truex", Some("Unexpected non-whitespace character after JSON at position 4 (line 1 column 5)")),
            ("false!", Some("Unexpected non-whitespace character after JSON at position 5 (line 1 column 6)")),
            ("\nnull x", Some("Unexpected non-whitespace character after JSON at position 6 (line 2 column 6)")),
            ("\n\n  true  !", Some("Unexpected non-whitespace character after JSON at position 10 (line 3 column 9)")),
            ("null\n\nx", Some("Unexpected non-whitespace character after JSON at position 6 (line 3 column 1)")),
            ("12ab", Some("Unexpected non-whitespace character after JSON at position 2 (line 1 column 3)")),
            ("{\"a\":1,}", Some("Expected double-quoted property name in JSON at position 7 (line 1 column 8)")),
            ("[1,2", Some("Expected ',' or ']' after array element in JSON at position 4 (line 1 column 5)")),
            ("\"unterminated", Some("Unterminated string in JSON at position 13 (line 1 column 14)")),
            ("-", Some("No number after minus sign in JSON at position 1 (line 1 column 2)")),
            ("{", Some("Expected property name or '}' in JSON at position 1 (line 1 column 2)")),
            ("{\"a\"", Some("Expected ':' after property name in JSON at position 4 (line 1 column 5)")),
            ("{\"a\":1 \"b\":2}", Some("Expected ',' or '}' after property value in JSON at position 7 (line 1 column 8)")),
            ("[1 2]", Some("Expected ',' or ']' after array element in JSON at position 3 (line 1 column 4)")),
            ("[1,]", Some("Unexpected token ']', \"[1,]\" is not valid JSON")),
            ("{a:1}", Some("Expected property name or '}' in JSON at position 1 (line 1 column 2)")),
            ("{\"a\":tru}", Some("Unexpected token '}', \"{\"a\":tru}\" is not valid JSON")),
            ("\"a\\x\"", Some("Bad escaped character in JSON at position 3 (line 1 column 4)")),
            ("\"\\u12G4\"", Some("Bad Unicode escape in JSON at position 5 (line 1 column 6)")),
            ("\"a\nb\"", Some("Bad control character in string literal in JSON at position 2 (line 1 column 3)")),
            ("01", Some("Unexpected number in JSON at position 1 (line 1 column 2)")),
            ("1.", Some("Unterminated fractional number in JSON at position 2 (line 1 column 3)")),
            ("1e", Some("Exponent part is missing a number in JSON at position 2 (line 1 column 3)")),
            ("undefined", Some("\"undefined\" is not valid JSON")),
            ("[object Object]", Some("\"[object Object]\" is not valid JSON")),
            ("{\r\n\"a\" 1}", Some("Expected ':' after property name in JSON at position 7 (line 2 column 5)")),
            ("[\r\r1 2]", Some("Expected ',' or ']' after array element in JSON at position 5 (line 3 column 3)")),
            ("{\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\":xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx}", Some("Unexpected token 'x', ...\"xxxxxxxx\":xxxxxxxxxx\"... is not valid JSON")),
            ("{\n  \"name\": \"Foundry-9\"\n  \"color\": null\n}", Some("Expected ',' or '}' after property value in JSON at position 26 (line 3 column 3)")),
            ("{\"a\":1e400}", None),
        ];
        for (input, want) in table {
            assert_eq!(
                v8_json_parse_message(input).as_deref(),
                *want,
                "input {input:?}"
            );
        }
    }
}
