//! Lightweight Markdown frontmatter parsing — ported from v4's
//! `lib/doc-edit/markdown-parser.ts` (the `parseFrontmatter` half). The vault
//! read overlay's per-file parsers (`parsePromptFile` / `parseScenarioFile` /
//! `parseWardrobeItemFile`) all funnel through this to split a file's YAML
//! frontmatter from its body.
//!
//! ## The hand-rolled YAML reader (read-side companion to the locked Decision A)
//!
//! v4's `parseFrontmatter` calls eemeli/yaml's `YAML.parse`. Decision A
//! (2026-06-29) locked the wardrobe YAML **emitter** as hand-rolled — no YAML
//! crate in the vault — and the read side follows the same call: this module
//! hand-rolls a parser for the **constrained subset** our own emitters produce
//! plus simple hand-edits, and matches eemeli/yaml's **YAML 1.2 core schema**
//! output on exactly that subset (verified byte-for-byte against the v4 oracle).
//!
//! The supported subset is a **flat mapping**, one `key: value` per line, where a
//! value is:
//!   - a plain scalar resolved by the YAML 1.2 **core schema** — `~`/`null`/`Null`/
//!     `NULL`/empty → null; `true`/`True`/`TRUE` / `false`/`False`/`FALSE` → bool
//!     (note `yes`/`no` are **strings**, unlike YAML 1.1); decimal int / decimal
//!     float → number; everything else (incl. ISO timestamps) → string;
//!   - a double-quoted scalar (JSON-style escapes — matches v4 `escapeYaml` =
//!     `JSON.stringify`) or a single-quoted scalar (`''` → `'`);
//!   - a flow sequence `[a, b, c]` or a block sequence (`- item` lines) of scalars.
//!
//! Comments follow the YAML rule (`#` begins a comment only at line start or after
//! whitespace; never inside quotes). Duplicate keys are a parse error → null
//! (eemeli throws). **Out of the supported subset** (documented seam — kept out of
//! the corpus; would diverge from `YAML.parse`): nested maps, flow maps,
//! anchors/aliases/tags, block scalars (`|` / `>`), multi-line plain scalars, and
//! exotic numbers (hex / octal / exponent / `.inf` / `.nan`). Anything in this set
//! resolves conservatively (a null/string or a top-level parse error), never to a
//! silently-wrong typed value.

use serde_json::{Map, Value};

/// Result of [`parse_frontmatter`] — mirrors v4 `ParsedFrontmatter`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFrontmatter {
    /// Parsed YAML data: `Some(Value::Object(..))` (possibly empty `{}`), or
    /// `None` when there is no frontmatter block, no closing delimiter, or the
    /// YAML did not parse to a plain object (array / scalar / parse error). v4's
    /// `data: Record<string, unknown> | null`.
    pub data: Option<Value>,
    /// Line index where the body content starts (after the closing `---`).
    pub body_start_line: usize,
    /// **UTF-16 code-unit** offset where the body starts (matches v4's JS
    /// `String.length`-based offset, used as `content.slice(offset)`).
    pub body_start_offset: usize,
}

/// Parse YAML frontmatter from file content (v4 `parseFrontmatter`). The block
/// must start at the very beginning of the file with `---\n`; the closing
/// delimiter is a line that is exactly `---`. Offsets are computed whenever a
/// closing delimiter is found, even if the YAML itself fails to yield an object.
pub fn parse_frontmatter(content: &str) -> ParsedFrontmatter {
    if !content.starts_with("---\n") {
        return ParsedFrontmatter {
            data: None,
            body_start_line: 0,
            body_start_offset: 0,
        };
    }

    let lines: Vec<&str> = content.split('\n').collect();

    // Closing delimiter: the first line (after the opener) that is exactly `---`.
    let mut closing = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if *line == "---" {
            closing = Some(i);
            break;
        }
    }
    let closing = match closing {
        Some(c) => c,
        None => {
            // No closing delimiter — v4 logs a warning and returns 0/0/null.
            return ParsedFrontmatter {
                data: None,
                body_start_line: 0,
                body_start_offset: 0,
            };
        }
    };

    let yaml_content = lines[1..closing].join("\n");
    let data = match parse_yaml_subset(&yaml_content) {
        YamlDoc::Map(m) => Some(Value::Object(m)),
        // Parsed to null / empty / comments-only → v4 normalizes to `{}`.
        YamlDoc::Null => Some(Value::Object(Map::new())),
        // Array / scalar root, or a parse error → v4 falls back to `null`.
        YamlDoc::Other | YamlDoc::Error => None,
    };

    // bodyStartOffset = (UTF-16 length of lines[0..=closing] joined by '\n') + 1.
    // The join length is the sum of per-line UTF-16 lengths plus one `\n` per
    // separator (there are `closing` separators across `closing + 1` lines).
    let sum_u16: usize = lines[..=closing]
        .iter()
        .map(|l| crate::jsstr::utf16_len(l))
        .sum();
    let body_start_offset = sum_u16 + closing + 1;

    ParsedFrontmatter {
        data,
        body_start_line: closing + 1,
        body_start_offset,
    }
}

/// Slice `content` from a parsed frontmatter's `body_start_offset` — the body
/// after the closing `---`. v4 slices by the UTF-16 offset (`content.slice(off)`);
/// this maps that UTF-16 offset to a byte index (always a char boundary, since
/// the offset lands right after a `\n`) and returns the body slice. With no
/// frontmatter the offset is 0 and the whole content is returned.
pub fn body_after<'a>(content: &'a str, fm: &ParsedFrontmatter) -> &'a str {
    let target = fm.body_start_offset;
    if target == 0 {
        return content;
    }
    let mut u16 = 0usize;
    for (byte_idx, ch) in content.char_indices() {
        if u16 >= target {
            return &content[byte_idx..];
        }
        u16 += ch.len_utf16();
    }
    ""
}

/// The top-level shape a frontmatter YAML document resolves to.
enum YamlDoc {
    /// A mapping (the only shape v4 keeps as `data`).
    Map(Map<String, Value>),
    /// Parsed to null — empty / whitespace / comments-only (→ v4 `{}`).
    Null,
    /// A non-mapping root: a sequence or a bare scalar (→ v4 `null`).
    Other,
    /// A parse error — duplicate key, or out-of-subset structure (→ v4 `null`).
    Error,
}

/// Trim only YAML inline whitespace (space + tab) from both ends of a line slice.
fn trim_yaml(s: &str) -> &str {
    s.trim_matches(|c| c == ' ' || c == '\t')
}

/// Is `t` (an already-`trim_yaml`'d line) a block-sequence item — `-` alone or
/// `-` followed by whitespace?
fn is_seq_item(t: &str) -> bool {
    if t == "-" {
        return true;
    }
    let mut chars = t.chars();
    chars.next() == Some('-') && matches!(chars.next(), Some(' ') | Some('\t'))
}

/// Split a top-level `key: value` line. The separator is a `:` that is followed
/// by a space/tab or ends the line. Returns `(key, rest_after_colon)`, or `None`
/// when the line carries no such separator (a bare scalar document).
fn split_key(line: &str) -> Option<(String, &str)> {
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b':' {
            let next = bytes.get(i + 1);
            if next.is_none() || matches!(next, Some(b' ') | Some(b'\t')) {
                let key = trim_yaml(&line[..i]).to_string();
                let rest = &line[i + 1..];
                return Some((key, rest));
            }
        }
    }
    None
}

/// Parse the flat-mapping subset. See the module docs for what's supported.
fn parse_yaml_subset(yaml: &str) -> YamlDoc {
    let lines: Vec<&str> = yaml.split('\n').collect();
    let mut map: Map<String, Value> = Map::new();
    let mut saw_entry = false;
    let mut i = 0;

    while i < lines.len() {
        let raw = lines[i];
        let t = trim_yaml(raw);

        // Blank or comment-only line — skip.
        if t.is_empty() || t.starts_with('#') {
            i += 1;
            continue;
        }
        // A top-level sequence item means the document is a sequence, not a map.
        if is_seq_item(t) {
            return YamlDoc::Other;
        }
        // A leading indent at top level is out-of-subset (e.g. a nested map).
        if raw.starts_with(' ') || raw.starts_with('\t') {
            return YamlDoc::Error;
        }

        let (key, rest) = match split_key(raw) {
            Some(kr) => kr,
            // No `key:` separator → a bare scalar document.
            None => return YamlDoc::Other,
        };
        if map.contains_key(&key) {
            // eemeli/yaml throws on duplicate keys; v4 catches → null.
            return YamlDoc::Error;
        }

        // Leading-trim the value; an empty value or one that begins a comment
        // routes to the null / block-sequence path.
        let val = rest.trim_start_matches([' ', '\t']);
        if val.is_empty() || val.starts_with('#') {
            match collect_block_sequence(&lines, i + 1) {
                None => return YamlDoc::Error,
                Some((items, next)) => {
                    if items.is_empty() {
                        map.insert(key, Value::Null);
                        i += 1;
                    } else {
                        map.insert(key, Value::Array(items));
                        i = next;
                    }
                }
            }
        } else if val.starts_with('[') {
            match parse_flow_seq(val) {
                Some(v) => {
                    map.insert(key, v);
                    i += 1;
                }
                None => return YamlDoc::Error,
            }
        } else {
            match parse_scalar_value(val) {
                Some(v) => {
                    map.insert(key, v);
                    i += 1;
                }
                None => return YamlDoc::Error,
            }
        }
        saw_entry = true;
    }

    if saw_entry {
        YamlDoc::Map(map)
    } else {
        // Empty / comments-only → null (v4 normalizes to `{}`).
        YamlDoc::Null
    }
}

/// Collect a block sequence beginning at line `start`. Returns the items and the
/// index of the first line not part of the sequence, or `None` on a malformed
/// scalar item. An empty result (no `-` lines follow) means the key's value is
/// null, handled by the caller.
fn collect_block_sequence(lines: &[&str], start: usize) -> Option<(Vec<Value>, usize)> {
    let mut items: Vec<Value> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let t = trim_yaml(lines[i]);
        if t.is_empty() {
            break; // a blank line ends the sequence
        }
        if !is_seq_item(t) {
            break; // a non-`-` line ends the sequence
        }
        // Value after the leading `-`.
        let after = trim_yaml(&t[1..]);
        if after.is_empty() || after.starts_with('#') {
            items.push(Value::Null);
        } else if after.starts_with('[') {
            items.push(parse_flow_seq(after)?);
        } else {
            items.push(parse_scalar_value(after)?);
        }
        i += 1;
    }
    Some((items, i))
}

/// Resolve a non-empty inline value (not starting with `#` or `[`) — a quoted or
/// plain scalar. `None` on a malformed quoted string (unterminated).
fn parse_scalar_value(val: &str) -> Option<Value> {
    match val.chars().next() {
        Some('"') => Some(Value::String(parse_double_quoted(val)?)),
        Some('\'') => Some(Value::String(parse_single_quoted(val)?)),
        _ => {
            // Plain scalar — strip a trailing ` #` comment, trim, then resolve.
            let cut = strip_plain_comment(val);
            Some(resolve_plain_scalar(trim_yaml(cut)))
        }
    }
}

/// Cut a plain scalar at the first `#` that follows whitespace (a comment). `#`
/// bytes can't be UTF-8 continuation bytes, so byte scanning is safe.
fn strip_plain_comment(v: &str) -> &str {
    let bytes = v.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i] == b'#' && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return &v[..i];
        }
    }
    v
}

/// YAML 1.2 core-schema resolution of an isolated plain scalar.
fn resolve_plain_scalar(s: &str) -> Value {
    match s {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        _ => {}
    }
    // Decimal integer (`[-+]?[0-9]+`).
    if is_decimal_int(s) {
        if let Ok(n) = s.parse::<i64>() {
            return Value::from(n);
        }
    }
    // Decimal float with a literal `.` (no exponent/inf/nan in the subset).
    if is_decimal_float(s) {
        if let Ok(f) = s.parse::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(f) {
                return Value::Number(num);
            }
        }
    }
    Value::String(s.to_string())
}

/// `^[-+]?[0-9]+$`.
fn is_decimal_int(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
}

/// `^[-+]?(?:[0-9]+\.[0-9]*|\.[0-9]+)$` — exactly one `.`, at least one digit,
/// only digits otherwise. (Exponent / `.inf` / `.nan` are out of the subset.)
fn is_decimal_float(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    if body.bytes().filter(|b| *b == b'.').count() != 1 {
        return false;
    }
    let digits = body.bytes().filter(|b| b.is_ascii_digit()).count();
    digits >= 1 && body.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// Parse a double-quoted scalar starting at `s[0] == '"'`. Returns the decoded
/// string (trailing content after the closing quote is ignored). `None` if
/// unterminated.
fn parse_double_quoted(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    chars.next(); // opening quote
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => {
                let e = chars.next()?;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    '0' => out.push('\u{0000}'),
                    'x' => out.push(decode_hex(&mut chars, 2)?),
                    'u' => out.push(decode_hex(&mut chars, 4)?),
                    'U' => out.push(decode_hex(&mut chars, 8)?),
                    // Other escapes are out of the subset.
                    _ => return None,
                }
            }
            _ => out.push(c),
        }
    }
    None // unterminated
}

/// Consume exactly `n` hex digits and decode to a `char`.
fn decode_hex(chars: &mut std::str::Chars, n: usize) -> Option<char> {
    let mut code: u32 = 0;
    for _ in 0..n {
        let d = chars.next()?.to_digit(16)?;
        code = code * 16 + d;
    }
    char::from_u32(code)
}

/// Parse a single-quoted scalar starting at `s[0] == '\''`. Only escape is `''`
/// → `'`. Trailing content after the closing quote is ignored. `None` if
/// unterminated.
fn parse_single_quoted(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    chars.next(); // opening quote
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
                out.push('\'');
            } else {
                return Some(out);
            }
        } else {
            out.push(c);
        }
    }
    None // unterminated
}

/// Parse a flow sequence starting at `s[0] == '['`. Elements are scalars
/// (quoted or plain), comma-separated, with surrounding whitespace ignored and a
/// trailing comma tolerated. `None` if unterminated or an element is malformed.
fn parse_flow_seq(s: &str) -> Option<Value> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 1; // past '['
    let mut items: Vec<Value> = Vec::new();

    loop {
        // Skip whitespace and separating commas.
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t' || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            return None; // unterminated
        }
        if chars[i] == ']' {
            return Some(Value::Array(items));
        }
        // Read one element up to the next top-level ',' or ']'.
        let start = i;
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && quote == '"' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    // single-quote `''` escape
                    if quote == '\'' && i + 1 < chars.len() && chars[i + 1] == '\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            // consume to the next comma / close
            while i < chars.len() && chars[i] != ',' && chars[i] != ']' {
                i += 1;
            }
            let elem: String = chars[start..i].iter().collect();
            items.push(parse_scalar_value(trim_yaml(&elem))?);
        } else {
            while i < chars.len() && chars[i] != ',' && chars[i] != ']' {
                i += 1;
            }
            let elem: String = chars[start..i].iter().collect();
            let elem = trim_yaml(&elem);
            items.push(resolve_plain_scalar(elem));
        }
        if i >= chars.len() {
            return None; // unterminated
        }
    }
}
