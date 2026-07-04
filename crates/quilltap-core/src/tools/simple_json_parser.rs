//! Simple-JSON tool-call parser — port of v4 `lib/tools/simple-json-parser.ts`.
//!
//! Parses `<tool_call>{"name":"...","arguments":{...}}</tool_call>` markers (and
//! the `<toolcall>`/`<tool>`/`<call>`/`<function_call>` aliases) from an LLM
//! response. The parser runs in three tiers, escalating only on failure:
//!
//!   1. **strict** — strict JSON parse of the tag body.
//!   2. **repaired** — same body through a bounded JSON repairer (see below).
//!   3. **brace** — a balanced-brace walk from the first `{`, repaired.
//!
//! Only the FIRST block is honoured (one tool call per turn). Total failure logs
//! (in v4) and returns `[]`; here it just returns `[]`.
//!
//! ## The tag scanner is hand-rolled, not regex
//!
//! v4's `TAG_PATTERN` uses a `\1` backreference (the closing tag must match the
//! opening alias), which the Rust `regex` crate cannot express. The scanner
//! reproduces the regex semantics exactly: case-insensitive alias match, the
//! `<alias\s*>` opening form, the non-greedy body up to the FIRST matching
//! `</alias\s*>` close or end-of-string (`$`), and only the leftmost block.
//! Indices are UTF-16 code-unit offsets, matching JS `String.match` `.index`.
//!
//! ## The repair tier is a bounded hand-rolled subset (jsonrepair)
//!
//! v4 uses the `jsonrepair` npm package. We reproduce a bounded subset — the
//! repairs v4's corpus exercises: single-quoted strings/keys, curly "smart"
//! quotes, unquoted object keys, and trailing commas. Out-of-subset
//! malformations (arbitrary garbage, code fences, unquoted string *values*)
//! resolve **conservatively to a tier failure** (never a different non-empty
//! parse), which matches v4's failure shape (`jsonrepair` fails → `[]`). The
//! corpus pins both sides of the boundary. This is the wardrobe-YAML-emitter
//! precedent: a faithful bounded reader instead of a full dependency.

use serde_json::{Map, Value};

use crate::jsstr::{is_js_ws, js_trim};

/// Opening-tag aliases accepted by the parser (v4 `OPENING_TAG_ALIASES`). Order
/// matters for disambiguation when multiple could match at one position — the
/// regex alternation is ordered, so we try them in this order.
const OPENING_TAG_ALIASES: &[&str] = &["tool_call", "toolcall", "tool", "call", "function_call"];

/// Which tier of the parser produced a result. v4 `SimpleJsonParserTier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleJsonParserTier {
    Strict,
    Repaired,
    Brace,
}

impl SimpleJsonParserTier {
    pub fn as_str(self) -> &'static str {
        match self {
            SimpleJsonParserTier::Strict => "strict",
            SimpleJsonParserTier::Repaired => "repaired",
            SimpleJsonParserTier::Brace => "brace",
        }
    }
}

/// A parsed `<tool_call>` block. v4 `ParsedSimpleJsonCall`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSimpleJsonCall {
    pub tool_name: String,
    pub arguments: Value,
    pub full_match: String,
    /// UTF-16 code-unit start offset in the original text.
    pub start_index: usize,
    /// UTF-16 code-unit end offset.
    pub end_index: usize,
    pub parser_tier: SimpleJsonParserTier,
}

/// The canonical `{ name, arguments }` request v4's converter yields.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
}

/// The canonical stop sequence the strategy passes to providers.
/// v4 `SIMPLE_JSON_STOP_SEQUENCES`.
pub const SIMPLE_JSON_STOP_SEQUENCES: &[&str] = &["</tool_call>"];

/// Map a tool name as emitted by the LLM to its internal canonical name.
/// v4 `mapSimpleJsonToolName`.
pub fn map_simple_json_tool_name(name: &str) -> String {
    let normalized = js_trim(&name.to_lowercase()).to_string();
    tool_name_alias(&normalized)
        .map(str::to_string)
        .unwrap_or(normalized)
}

/// v4 `TOOL_NAME_ALIASES` lookup.
fn tool_name_alias(normalized: &str) -> Option<&'static str> {
    Some(match normalized {
        "image" | "create_image" => "generate_image",
        "web_search" => "search_web",
        "note" => "create_note",
        "dice" | "roll" | "random" => "rng",
        "help" | "search_help" => "help_search",
        "settings" => "help_settings",
        "navigate" => "help_navigate",
        "project" => "project_info",
        "wardrobe" | "closet" | "list_wardrobe" => "wardrobe_list",
        "read_wardrobe" | "inspect" => "wardrobe_read",
        "set_outfit" | "wear_outfit" | "outfit" | "wear" | "change_item" | "equip" | "layer"
        | "swap" => "wardrobe_wear",
        "take_off" | "unequip" | "remove" | "undress" => "wardrobe_take_off",
        "create_wardrobe_item" => "wardrobe_create",
        "update_wardrobe_item" | "edit_wardrobe" => "wardrobe_update",
        "archive_wardrobe_item" | "discard" => "wardrobe_archive",
        "scriptorium" | "memory" | "memories" | "search_memory" | "search_memories"
        | "search_scriptorium" => "search",
        _ => return None,
    })
}

/// Quick boolean check — runs before the full parser. v4 `hasSimpleJsonMarkers`.
pub fn has_simple_json_markers(response: &str) -> bool {
    let lower = response.to_lowercase();
    for tag in OPENING_TAG_ALIASES {
        if lower.contains(&format!("<{tag}>")) || lower.contains(&format!("<{tag} ")) {
            return true;
        }
    }
    false
}

/// Parse the first `<tool_call>` block. v4 `parseSimpleJsonCalls`.
pub fn parse_simple_json_calls(response: &str) -> Vec<ParsedSimpleJsonCall> {
    if response.is_empty() {
        return Vec::new();
    }

    let units: Vec<u16> = response.encode_utf16().collect();
    let m = match find_tag_match(&units) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let tier1_body = String::from_utf16_lossy(&units[m.body_start..m.body_end]);
    let full_match = String::from_utf16_lossy(&units[m.start..m.end]);

    let parsed = match parse_lenient(&tier1_body, &full_match) {
        Some(p) => p,
        None => return Vec::new(),
    };

    // `name` must be a non-empty string.
    let name_raw = match parsed.object.get("name") {
        Some(Value::String(s)) if !js_trim(s).is_empty() => s.clone(),
        _ => return Vec::new(),
    };

    // `arguments` defaults to `{}` unless it is a (non-array) object.
    let args = match parsed.object.get("arguments") {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => Value::Object(Map::new()),
    };

    vec![ParsedSimpleJsonCall {
        tool_name: js_trim(&name_raw).to_string(),
        arguments: args,
        full_match,
        start_index: m.start,
        end_index: m.end,
        parser_tier: parsed.tier,
    }]
}

/// Convert a parsed block to the canonical `{ name, arguments }` request.
/// v4 `convertSimpleJsonToToolCallRequest`.
pub fn convert_simple_json_to_tool_call_request(parsed: &ParsedSimpleJsonCall) -> ToolCallRequest {
    ToolCallRequest {
        name: map_simple_json_tool_name(&parsed.tool_name),
        arguments: parsed.arguments.clone(),
    }
}

/// Escape a string for safe use inside an XML attribute value.
/// v4 `escapeXmlAttribute`. The replacement ORDER matters (`&` first).
pub fn escape_xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------------------------------------------------------------------
// The tag scanner (hand-rolled backreference regex).
// ---------------------------------------------------------------------------

struct TagMatch {
    /// UTF-16 offsets: full match [start, end); body (group 2) [body_start, body_end).
    start: usize,
    end: usize,
    body_start: usize,
    body_end: usize,
}

fn find_tag_match(units: &[u16]) -> Option<TagMatch> {
    let lt = u16::from(b'<');
    for i in 0..units.len() {
        if units[i] != lt {
            continue;
        }
        for alias in OPENING_TAG_ALIASES {
            if let Some(open_end) = match_open_tag(units, i, alias) {
                let (body_end, match_end) = find_close_or_eof(units, open_end, alias);
                return Some(TagMatch {
                    start: i,
                    end: match_end,
                    body_start: open_end,
                    body_end,
                });
            }
        }
    }
    None
}

/// Match `<alias\s*>` (case-insensitive) at `i`; return the index after `>`.
fn match_open_tag(units: &[u16], i: usize, alias: &str) -> Option<usize> {
    // units[i] is '<'
    let mut j = i + 1;
    j = match_alias_ci(units, j, alias)?;
    j = skip_ws(units, j);
    if units.get(j) == Some(&u16::from(b'>')) {
        Some(j + 1)
    } else {
        None
    }
}

/// Find the nearest `</alias\s*>` at or after `from`. Returns
/// `(body_end, match_end)`; if none, `(len, len)` (the regex `$` alternative).
fn find_close_or_eof(units: &[u16], from: usize, alias: &str) -> (usize, usize) {
    let lt = u16::from(b'<');
    let slash = u16::from(b'/');
    let gt = u16::from(b'>');
    let mut k = from;
    while k < units.len() {
        if units[k] == lt && units.get(k + 1) == Some(&slash) {
            if let Some(after_alias) = match_alias_ci(units, k + 2, alias) {
                let g = skip_ws(units, after_alias);
                if units.get(g) == Some(&gt) {
                    return (k, g + 1);
                }
            }
        }
        k += 1;
    }
    (units.len(), units.len())
}

/// Match the ASCII `alias` case-insensitively at `j`; return the index after it.
fn match_alias_ci(units: &[u16], j: usize, alias: &str) -> Option<usize> {
    let bytes = alias.as_bytes();
    if j + bytes.len() > units.len() {
        return None;
    }
    for (k, &b) in bytes.iter().enumerate() {
        let u = units[j + k];
        if u > 0x7f {
            return None;
        }
        if !(u as u8).eq_ignore_ascii_case(&b) {
            return None;
        }
    }
    Some(j + bytes.len())
}

/// Skip JS-whitespace code units.
fn skip_ws(units: &[u16], mut j: usize) -> usize {
    while let Some(&u) = units.get(j) {
        match char::from_u32(u32::from(u)) {
            Some(c) if is_js_ws(c) => j += 1,
            _ => break,
        }
    }
    j
}

// ---------------------------------------------------------------------------
// The three parse tiers.
// ---------------------------------------------------------------------------

struct LenientResult {
    object: Map<String, Value>,
    tier: SimpleJsonParserTier,
}

fn parse_lenient(raw_body: &str, full_block: &str) -> Option<LenientResult> {
    let stripped = js_trim(raw_body);
    if !stripped.is_empty() {
        // Tier 1 — strict.
        if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(stripped) {
            return Some(LenientResult {
                object: o,
                tier: SimpleJsonParserTier::Strict,
            });
        }
        // Tier 2 — repaired (bounded jsonrepair subset).
        if let Some(Value::Object(o)) = repair_parse(stripped) {
            return Some(LenientResult {
                object: o,
                tier: SimpleJsonParserTier::Repaired,
            });
        }
    }

    // Tier 3 — balanced-brace walk inside the full block.
    let block_bytes = full_block.as_bytes();
    if let Some(brace_start) = full_block.find('{') {
        if let Some(brace_end) = find_balanced_brace_end(block_bytes, brace_start) {
            let candidate = &full_block[brace_start..brace_end];
            if let Some(Value::Object(o)) = repair_parse(candidate) {
                return Some(LenientResult {
                    object: o,
                    tier: SimpleJsonParserTier::Brace,
                });
            }
        }
    }

    None
}

/// Walk `bytes` from `start` (a `{`) to the index just past the matching `}`.
/// Respects `"`/`'` string literals and `\` escapes. v4 `findBalancedBraceEnd`.
fn find_balanced_brace_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut quote = 0u8;
    let mut i = start;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_string {
            if ch == b'\\' {
                i += 2;
                continue;
            }
            if ch == quote {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match ch {
            b'"' | b'\'' => {
                in_string = true;
                quote = ch;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// The bounded JSON repairer (jsonrepair subset).
// ---------------------------------------------------------------------------

/// Parse `input` leniently (single/smart quotes, unquoted keys, trailing commas)
/// and return the value only if the whole (trimmed) input is consumed. Returns
/// `None` for out-of-subset malformations — the conservative direction.
fn repair_parse(input: &str) -> Option<Value> {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Lenient {
        chars: &chars,
        pos: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos == p.chars.len() {
        Some(v)
    } else {
        None
    }
}

struct Lenient<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Lenient<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if is_js_ws(c) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        self.skip_ws();
        match self.peek()? {
            '{' => self.parse_object(),
            '[' => self.parse_array(),
            '"' | '\'' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}' => {
                self.parse_string().map(Value::String)
            }
            't' => self.parse_literal("true", Value::Bool(true)),
            'f' => self.parse_literal("false", Value::Bool(false)),
            'n' => self.parse_literal("null", Value::Null),
            c if c == '-' || c.is_ascii_digit() => self.parse_number(),
            _ => None,
        }
    }

    fn parse_literal(&mut self, word: &str, value: Value) -> Option<Value> {
        for w in word.chars() {
            if self.peek() != Some(w) {
                return None;
            }
            self.pos += 1;
        }
        Some(value)
    }

    fn parse_number(&mut self) -> Option<Value> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        let mut saw_digit = false;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
            saw_digit = true;
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
                saw_digit = true;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if !saw_digit {
            return None;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        serde_json::from_str::<Value>(&s).ok()
    }

    fn parse_string(&mut self) -> Option<String> {
        let open = self.peek()?;
        let close = match open {
            '"' => '"',
            '\'' => '\'',
            '\u{201C}' | '\u{201D}' => '\u{201D}', // curly double
            '\u{2018}' | '\u{2019}' => '\u{2019}', // curly single
            _ => return None,
        };
        self.pos += 1;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            if c == '\\' {
                self.pos += 1;
                let e = self.peek()?;
                self.pos += 1;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            let h = self.peek()?;
                            let d = h.to_digit(16)?;
                            code = code * 16 + d;
                            self.pos += 1;
                        }
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => out.push(other),
                }
            } else if c == close {
                self.pos += 1;
                return Some(out);
            } else {
                out.push(c);
                self.pos += 1;
            }
        }
    }

    fn parse_key(&mut self) -> Option<String> {
        self.skip_ws();
        match self.peek()? {
            '"' | '\'' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}' => self.parse_string(),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '$')
                {
                    self.pos += 1;
                }
                Some(self.chars[start..self.pos].iter().collect())
            }
            _ => None,
        }
    }

    fn parse_object(&mut self) -> Option<Value> {
        self.pos += 1; // consume '{'
        let mut map = Map::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Some(Value::Object(map));
        }
        loop {
            let key = self.parse_key()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return None;
            }
            self.pos += 1;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                    self.skip_ws();
                    if self.peek() == Some('}') {
                        self.pos += 1;
                        return Some(Value::Object(map));
                    }
                }
                Some('}') => {
                    self.pos += 1;
                    return Some(Value::Object(map));
                }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self) -> Option<Value> {
        self.pos += 1; // consume '['
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Some(Value::Array(arr));
        }
        loop {
            let value = self.parse_value()?;
            arr.push(value);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                    self.skip_ws();
                    if self.peek() == Some(']') {
                        self.pos += 1;
                        return Some(Value::Array(arr));
                    }
                }
                Some(']') => {
                    self.pos += 1;
                    return Some(Value::Array(arr));
                }
                _ => return None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stripping.
// ---------------------------------------------------------------------------

/// Strip all `<tool_call>` (and alias-tag) blocks for display. Idempotent.
/// v4 `stripSimpleJsonMarkers`.
pub fn strip_simple_json_markers(response: &str) -> String {
    if response.is_empty() {
        return response.to_string();
    }

    let mut stripped = response.to_string();

    // Remove well-formed open+close blocks for every alias (non-greedy, all
    // occurrences).
    for tag in OPENING_TAG_ALIASES {
        stripped = remove_closed_blocks(&stripped, tag);
    }

    // Remove dangling opening tags + the JSON object that follows.
    for tag in OPENING_TAG_ALIASES {
        stripped = remove_dangling_open(&stripped, tag);
    }

    collapse_blank_lines(&stripped)
}

/// v4: `new RegExp('<tag\\s*>[\\s\\S]*?<\\/tag\\s*>', 'gi')` → ''.
fn remove_closed_blocks(text: &str, tag: &str) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let lt = u16::from(b'<');
    let slash = u16::from(b'/');
    let gt = u16::from(b'>');
    let mut i = 0;
    while i < units.len() {
        // Try to match an opening `<tag\s*>` at i.
        if units[i] == lt {
            if let Some(open_end) = try_open(&units, i, tag) {
                // Find nearest `</tag\s*>` after open_end.
                let mut k = open_end;
                let mut found = None;
                while k < units.len() {
                    if units[k] == lt && units.get(k + 1) == Some(&slash) {
                        if let Some(after) = match_alias_ci(&units, k + 2, tag) {
                            let g = skip_ws(&units, after);
                            if units.get(g) == Some(&gt) {
                                found = Some(g + 1);
                                break;
                            }
                        }
                    }
                    k += 1;
                }
                if let Some(end) = found {
                    // Drop the whole block; continue after it.
                    i = end;
                    continue;
                }
            }
        }
        out.push(units[i]);
        i += 1;
    }
    String::from_utf16_lossy(&out)
}

/// Match `<tag\s*>` at `i` (units[i] is checked to be '<'); return index after '>'.
fn try_open(units: &[u16], i: usize, tag: &str) -> Option<usize> {
    let mut j = match_alias_ci(units, i + 1, tag)?;
    j = skip_ws(units, j);
    if units.get(j) == Some(&u16::from(b'>')) {
        Some(j + 1)
    } else {
        None
    }
}

/// v4: `new RegExp('<tag\\s*>\\s*\\{[\\s\\S]*$', 'gi')` with the balanced-brace
/// callback. Matches from the first `<tag\s*>` followed by `\s*{` to EOF; keeps
/// only what follows the balanced object.
fn remove_dangling_open(text: &str, tag: &str) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let lt = u16::from(b'<');
    let brace = u16::from(b'{');
    let mut i = 0;
    while i < units.len() {
        if units[i] == lt {
            if let Some(open_end) = try_open(&units, i, tag) {
                // Require `\s*{` after the open tag.
                let after_ws = skip_ws(&units, open_end);
                if units.get(after_ws) == Some(&brace) {
                    // `open` is the matched region [i, end-of-string).
                    let open: String = String::from_utf16_lossy(&units[i..]);
                    let replacement = dangling_replacement(&open);
                    let mut out: String = String::from_utf16_lossy(&units[..i]);
                    out.push_str(&replacement);
                    return out;
                }
            }
        }
        i += 1;
    }
    text.to_string()
}

/// v4's replacement callback: keep whatever comes AFTER the balanced object.
fn dangling_replacement(open: &str) -> String {
    let bytes = open.as_bytes();
    let brace_start = match open.find('{') {
        Some(b) => b,
        None => return String::new(),
    };
    match find_balanced_brace_end(bytes, brace_start) {
        Some(brace_end) => open[brace_end..].to_string(),
        None => String::new(),
    }
}

/// v4: `.replace(/\n{3,}/g, '\n\n').trim()`.
fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0usize;
    for c in text.chars() {
        if c == '\n' {
            run += 1;
            if run <= 2 {
                out.push('\n');
            }
        } else {
            run = 0;
            out.push(c);
        }
    }
    js_trim(&out).to_string()
}
