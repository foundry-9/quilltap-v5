//! Text-block tool-call parser — port of v4
//! `lib/tools/legacy/text-block-parser.ts`.
//!
//! Parses `[[TOOL_NAME param="value"]]content[[/TOOL_NAME]]` markers (content
//! form) and `[[TOOL_NAME param="value" /]]` (self-closing form). Case-
//! insensitive tool names, named params (double/single/backslash-escaped
//! quotes), non-greedy content.
//!
//! ## The content form is a hybrid scanner
//!
//! v4's content-form regex closes with a `\1` backreference (the closing tag
//! must match the opening tool name), which the Rust `regex` crate cannot
//! express. We use the `regex` crate for the **opening tag** (`[[NAME attrs]]`)
//! and the **self-closing** form — the crate handles the intricate attribute /
//! escaped-quote backtracking exactly as JS does — and hand-roll only the
//! nearest-matching-close search. The self-closing regex and both strip regexes
//! carry no backreference, so they run straight through the crate. `\s`/`\w`/`.`
//! are built with JS semantics (JS whitespace set, ASCII `\w`, dot-excludes-line-
//! terminators).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::jsstr::{js_trim, utf16_len, JS_WS_CLASS};

/// A parsed text-block tool call. v4 `ParsedTextBlock`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTextBlock {
    pub tool_name: String,
    /// Named parameters, in the order they appeared.
    pub params: Vec<(String, String)>,
    pub content: String,
    pub full_match: String,
    /// UTF-16 code-unit start offset.
    pub start_index: usize,
    /// UTF-16 code-unit end offset.
    pub end_index: usize,
}

/// The canonical `{ name, arguments }` request.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
}

// ---------------------------------------------------------------------------
// Shared regex fragments (JS semantics).
// ---------------------------------------------------------------------------

/// JS `\s` inner (the `JS_WS_CLASS` chars, without the surrounding brackets).
fn ws_inner() -> &'static str {
    &JS_WS_CLASS[1..JS_WS_CLASS.len() - 1]
}

/// JS `.` (any char except line terminators).
const DOT_NL: &str = r"[^\n\r\u{2028}\u{2029}]";

fn quoted() -> String {
    let dot = DOT_NL;
    // \\?"[^"\\]*(?:\\.[^"\\]*)*\\?"  |  '[^']*'
    format!(r#"\\?"[^"\\]*(?:\\{dot}[^"\\]*)*\\?"|'[^']*'"#)
}

fn attrs() -> String {
    let ws = JS_WS_CLASS;
    let q = quoted();
    // (?:\s+\w+\s*=\s*(?:QUOTED))*
    format!(r"(?:{ws}+[A-Za-z0-9_]+{ws}*={ws}*(?:{q}))*")
}

/// `[[(NAME)(ATTRS)\s*]]` — the content-form opening tag.
static CONTENT_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    let a = attrs();
    Regex::new(&format!(r"\[\[([A-Za-z0-9_]+)({a}){ws}*\]\]")).unwrap()
});

/// `[[(NAME)(ATTRS)\s*/]]` — the self-closing form.
static SELF_CLOSING_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    let a = attrs();
    Regex::new(&format!(r"\[\[([A-Za-z0-9_]+)({a}){ws}*/\]\]")).unwrap()
});

/// `parseTagParams`: `(\w+)\s*=\s*(?:\\?"(INNER)\\?"|'(INNER)')`.
static PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    let dot = DOT_NL;
    Regex::new(&format!(
        r#"([A-Za-z0-9_]+){ws}*={ws}*(?:\\?"([^"\\]*(?:\\{dot}[^"\\]*)*)\\?"|'([^']*)')"#
    ))
    .unwrap()
});

/// The `\\(.)` unescape.
static UNESCAPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\\({DOT_NL})")).unwrap());

/// Content-form strip regex (close is any `[[/\w+]]`, NOT a backreference).
static CONTENT_STRIP_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    let q = quoted();
    Regex::new(&format!(
        r"\[\[[A-Za-z0-9_]+(?:{ws}+[A-Za-z0-9_]+{ws}*={ws}*(?:{q}))*{ws}*\]\][\s\S]*?\[\[/[A-Za-z0-9_]+\]\]"
    ))
    .unwrap()
});

/// Self-closing strip regex.
static SELF_CLOSING_STRIP_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = JS_WS_CLASS;
    let q = quoted();
    Regex::new(&format!(
        r"\[\[[A-Za-z0-9_]+(?:{ws}+[A-Za-z0-9_]+{ws}*={ws}*(?:{q}))*{ws}*/\]\]"
    ))
    .unwrap()
});

/// `hasTextBlockMarkers`: `/\[\[\w+[\s\w="'\\\/]*\]\]/i`.
static HAS_MARKERS_RE: LazyLock<Regex> = LazyLock::new(|| {
    let ws = ws_inner();
    Regex::new(&format!(r#"\[\[[A-Za-z0-9_]+[{ws}A-Za-z0-9_="'\\/]*\]\]"#)).unwrap()
});

// ---------------------------------------------------------------------------
// Name / param maps.
// ---------------------------------------------------------------------------

/// v4 `mapTextBlockToolName`.
pub fn map_text_block_tool_name(name: &str) -> String {
    let normalized = js_trim(&name.to_lowercase()).to_string();
    text_block_name_alias(&normalized)
        .map(str::to_string)
        .unwrap_or(normalized)
}

fn text_block_name_alias(n: &str) -> Option<&'static str> {
    Some(match n {
        "whisper" => "whisper",
        "generate_image" | "image" | "create_image" => "generate_image",
        "search_web" | "web_search" => "search_web",
        "create_note" | "note" => "create_note",
        "rng" | "dice" | "roll" | "random" => "rng",
        "state" => "state",
        "project_info" | "project" => "project_info",
        "help_search" | "search_help" | "help" => "help_search",
        "help_settings" | "settings" => "help_settings",
        "help_navigate" | "navigate" => "help_navigate",
        "wardrobe_list" | "list_wardrobe" | "wardrobe" | "closet" => "wardrobe_list",
        "wardrobe_read" | "read_wardrobe" | "inspect" => "wardrobe_read",
        "wardrobe_wear" | "wear" | "equip" | "layer" | "swap" | "set_outfit" | "wear_outfit"
        | "outfit" | "change_item" => "wardrobe_wear",
        "wardrobe_take_off" | "take_off" | "unequip" | "remove" | "undress" => "wardrobe_take_off",
        "wardrobe_create" | "create_wardrobe_item" => "wardrobe_create",
        "wardrobe_update" | "update_wardrobe_item" | "edit_wardrobe" => "wardrobe_update",
        "wardrobe_archive" | "archive_wardrobe_item" | "discard" => "wardrobe_archive",
        "search" | "scriptorium" | "memory" | "memories" | "search_memory" | "search_memories"
        | "search_scriptorium" => "search",
        _ => return None,
    })
}

/// v4 `PARAM_ALIAS_MAP` lookup (canonical name for an alias, per tool).
fn param_alias(tool: &str, key_lower: &str) -> Option<&'static str> {
    Some(match (tool, key_lower) {
        ("whisper", "to") | ("whisper", "recipient") | ("whisper", "character") => "target",
        ("whisper", "msg") | ("whisper", "text") => "message",
        ("generate_image", "description") | ("generate_image", "desc") => "prompt",
        ("search_web", "search") | ("search_web", "q") => "query",
        ("rng", "dice") | ("rng", "roll") => "type",
        ("help_search", "search") | ("help_search", "q") => "query",
        ("help_settings", "section") | ("help_settings", "type") => "category",
        ("wardrobe_wear", "id") => "item_id",
        ("wardrobe_wear", "title") | ("wardrobe_wear", "name") => "item_title",
        ("wardrobe_take_off", "id") => "item_id",
        ("wardrobe_take_off", "title") | ("wardrobe_take_off", "name") => "item_title",
        ("wardrobe_read", "id") => "item_id",
        ("wardrobe_read", "title") | ("wardrobe_read", "name") => "item_title",
        ("wardrobe_archive", "id") => "item_id",
        ("wardrobe_archive", "title") | ("wardrobe_archive", "name") => "item_title",
        ("wardrobe_update", "id") => "item_id",
        ("wardrobe_update", "name") => "item_title",
        ("wardrobe_update", "cue") => "image_prompt",
        ("wardrobe_update", "context") => "appropriateness",
        ("wardrobe_create", "name") => "title",
        ("wardrobe_create", "type") => "types",
        ("wardrobe_create", "context") => "appropriateness",
        ("wardrobe_create", "cue") => "image_prompt",
        ("wardrobe_create", "to")
        | ("wardrobe_create", "for")
        | ("wardrobe_create", "give_to")
        | ("wardrobe_create", "gift_to") => "recipient",
        ("wardrobe_create", "components") | ("wardrobe_create", "component_ids") => {
            "component_item_ids"
        }
        ("wardrobe_create", "component_names") => "component_titles",
        ("search", "search") | ("search", "q") => "query",
        ("search", "count") | ("search", "max") => "limit",
        _ => return None,
    })
}

/// Tools that have a `PARAM_ALIAS_MAP` entry (so `resolveParamAliases` rewrites).
fn has_param_alias_map(tool: &str) -> bool {
    matches!(
        tool,
        "whisper"
            | "generate_image"
            | "search_web"
            | "rng"
            | "help_search"
            | "help_settings"
            | "wardrobe_wear"
            | "wardrobe_take_off"
            | "wardrobe_read"
            | "wardrobe_archive"
            | "wardrobe_update"
            | "wardrobe_create"
            | "search"
    )
}

/// v4 `CONTENT_PARAM_MAP`: which param receives the content block.
fn content_param(tool: &str) -> &'static str {
    match tool {
        "whisper" => "message",
        "generate_image" => "prompt",
        "search_web" => "query",
        "create_note" => "content",
        "help_search" => "query",
        "search" => "query",
        _ => "content",
    }
}

// ---------------------------------------------------------------------------
// parseTagParams.
// ---------------------------------------------------------------------------

/// v4 `parseTagParams` — order-preserving.
fn parse_tag_params(attr_string: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();
    for caps in PARAM_RE.captures_iter(attr_string) {
        let name = caps.get(1).unwrap().as_str().to_string();
        let value = caps
            .get(2)
            .or_else(|| caps.get(3))
            .map(|m| m.as_str())
            .unwrap_or("");
        // `value ? value.replace(/\\(.)/g, '$1') : value`
        let unescaped = if value.is_empty() {
            value.to_string()
        } else {
            UNESCAPE_RE.replace_all(value, "$1").to_string()
        };
        params.push((name, unescaped));
    }
    params
}

// ---------------------------------------------------------------------------
// parseTextBlockCalls.
// ---------------------------------------------------------------------------

struct RawBlock {
    tool_name: String,
    params: Vec<(String, String)>,
    content: String,
    full_match: String,
    byte_start: usize,
    byte_end: usize,
}

/// v4 `parseTextBlockCalls`.
pub fn parse_text_block_calls(response: &str) -> Vec<ParsedTextBlock> {
    let mut raw: Vec<RawBlock> = Vec::new();

    // Content form — hybrid scan (regex open, manual backreference close).
    let mut pos = 0usize;
    while pos <= response.len() {
        let caps = match CONTENT_OPEN_RE.captures_at(response, pos) {
            Some(c) => c,
            None => break,
        };
        let whole = caps.get(0).unwrap();
        let name = caps.get(1).unwrap().as_str();
        let attr_string = caps.get(2).unwrap().as_str();
        let open_end = whole.end();
        let open_start = whole.start();

        match find_close(response, open_end, name) {
            Some((close_start, close_end)) => {
                let content = js_trim(&response[open_end..close_start]).to_string();
                raw.push(RawBlock {
                    tool_name: name.to_string(),
                    params: parse_tag_params(attr_string),
                    content,
                    full_match: response[open_start..close_end].to_string(),
                    byte_start: open_start,
                    byte_end: close_end,
                });
                pos = close_end;
            }
            None => {
                // This open has no matching close; JS advances by one and retries.
                pos = open_start + 1;
            }
        }
    }

    let content_count = raw.len();

    // Self-closing form.
    for caps in SELF_CLOSING_RE.captures_iter(response) {
        let whole = caps.get(0).unwrap();
        let start = whole.start();
        let end = whole.end();
        // Skip if overlapping a content-form match (v4's overlap check).
        let overlaps = raw[..content_count].iter().any(|r| {
            (start >= r.byte_start && start < r.byte_end)
                || (end > r.byte_start && end <= r.byte_end)
        });
        if overlaps {
            continue;
        }
        raw.push(RawBlock {
            tool_name: caps.get(1).unwrap().as_str().to_string(),
            params: parse_tag_params(caps.get(2).unwrap().as_str()),
            content: String::new(),
            full_match: whole.as_str().to_string(),
            byte_start: start,
            byte_end: end,
        });
    }

    // Sort by position (byte order = UTF-16 order; stable).
    raw.sort_by_key(|r| r.byte_start);

    raw.into_iter()
        .map(|r| ParsedTextBlock {
            tool_name: r.tool_name,
            params: r.params,
            content: r.content,
            full_match: r.full_match,
            start_index: utf16_len(&response[..r.byte_start]),
            end_index: utf16_len(&response[..r.byte_end]),
        })
        .collect()
}

/// Find the nearest `[[/NAME]]` (NAME case-insensitive) at or after `from`.
/// Returns byte `(close_start, close_end)`.
fn find_close(text: &str, from: usize, name: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let needle_open = b"[[/";
    let name_bytes = name.as_bytes();
    let mut i = from;
    while i + 3 + name_bytes.len() + 2 <= bytes.len() {
        if &bytes[i..i + 3] == needle_open {
            let name_region = &bytes[i + 3..i + 3 + name_bytes.len()];
            if name_region.eq_ignore_ascii_case(name_bytes) {
                let after = i + 3 + name_bytes.len();
                if &bytes[after..after + 2] == b"]]" {
                    return Some((i, after + 2));
                }
            }
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// convertTextBlockToToolCallRequest.
// ---------------------------------------------------------------------------

/// v4 `resolveParamAliases`.
fn resolve_param_aliases(tool: &str, params: &[(String, String)]) -> Vec<(String, String)> {
    if !has_param_alias_map(tool) {
        return params.to_vec();
    }
    let mut resolved: Vec<(String, String)> = Vec::new();
    for (key, value) in params {
        let canonical = param_alias(tool, &key.to_lowercase())
            .map(str::to_string)
            .unwrap_or_else(|| key.clone());
        // JS object semantics: setting an existing key updates in place.
        if let Some(slot) = resolved.iter_mut().find(|(k, _)| *k == canonical) {
            slot.1 = value.clone();
        } else {
            resolved.push((canonical, value.clone()));
        }
    }
    resolved
}

/// v4 `convertTextBlockToToolCallRequest`.
pub fn convert_text_block_to_tool_call_request(parsed: &ParsedTextBlock) -> ToolCallRequest {
    let internal_name = map_text_block_tool_name(&parsed.tool_name);
    let resolved = resolve_param_aliases(&internal_name, &parsed.params);

    // Build args from resolved params (string values for now).
    let mut args: Vec<(String, Value)> = resolved
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();

    // Map content to the tool-specific content parameter.
    if !parsed.content.is_empty() {
        let cp = content_param(&internal_name);
        // `!args[contentParam]` — falsy iff missing or empty string.
        let existing_falsy = match args.iter().find(|(k, _)| k == cp) {
            Some((_, Value::String(s))) => s.is_empty(),
            Some(_) => false,
            None => true,
        };
        if existing_falsy {
            set_arg(&mut args, cp, Value::String(parsed.content.clone()));
        }
    }

    // Numeric / boolean coercion.
    for (key, val) in args.iter_mut() {
        if let Value::String(s) = val {
            if s == "true" {
                *val = Value::Bool(true);
            } else if s == "false" {
                *val = Value::Bool(false);
            } else if !matches!(
                key.as_str(),
                "query" | "message" | "prompt" | "content" | "target"
            ) && !js_trim(s).is_empty()
            {
                if let Some(n) = js_number(s) {
                    *val = js_number_to_json(n);
                }
            }
        }
    }

    // wardrobe_wear / wardrobe_take_off wrap into a single-op `operations` array.
    if internal_name == "wardrobe_wear" || internal_name == "wardrobe_take_off" {
        let mut op: Vec<(String, Value)> = Vec::new();
        for field in ["mode", "item_id", "item_title", "slot"] {
            if let Some((_, v)) = args.iter().find(|(k, _)| k == field) {
                op.push((field.to_string(), v.clone()));
            }
        }
        let rest: Vec<(String, Value)> = args
            .into_iter()
            .filter(|(k, _)| !matches!(k.as_str(), "mode" | "item_id" | "item_title" | "slot"))
            .collect();
        let mut obj = to_map(rest);
        obj.insert(
            "operations".to_string(),
            Value::Array(vec![Value::Object(to_map(op))]),
        );
        return ToolCallRequest {
            name: internal_name,
            arguments: Value::Object(obj),
        };
    }

    ToolCallRequest {
        name: internal_name,
        arguments: Value::Object(to_map(args)),
    }
}

fn set_arg(args: &mut Vec<(String, Value)>, key: &str, value: Value) {
    if let Some(slot) = args.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value;
    } else {
        args.push((key.to_string(), value));
    }
}

fn to_map(entries: Vec<(String, Value)>) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k, v);
    }
    m
}

/// JS `Number(value)` for a string, over the realistic tool-arg subset. Since
/// P4.6bd a thin adapter over the canonical [`crate::jsnum::number_from_str`]
/// (NaN → `None`, which is how this parser signals "not a number"). The
/// canonical fn is also strictly MORE faithful than the local copy it
/// replaced (e.g. `Number('+0x10')` is NaN in JS — signed non-decimal
/// literals are illegal — where the old copy accepted it); the corpus stays
/// within the shared subset either way.
fn js_number(s: &str) -> Option<f64> {
    let f = crate::jsnum::number_from_str(s);
    if f.is_nan() {
        None
    } else {
        Some(f)
    }
}

/// Integer-valued finite float → JSON integer (mirrors JS `JSON.stringify`).
fn js_number_to_json(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

// ---------------------------------------------------------------------------
// Stripping / detection.
// ---------------------------------------------------------------------------

/// v4 `stripTextBlockMarkers`.
pub fn strip_text_block_markers(response: &str) -> String {
    let s = CONTENT_STRIP_RE.replace_all(response, "");
    let s = SELF_CLOSING_STRIP_RE.replace_all(&s, "");
    // `\n{3,}` → `\n\n`, `  +` → ` `, trim.
    let s = collapse_newlines(&s);
    let s = collapse_spaces(&s);
    js_trim(&s).to_string()
}

/// v4 `hasTextBlockMarkers`.
pub fn has_text_block_markers(response: &str) -> bool {
    HAS_MARKERS_RE.is_match(response)
}

fn collapse_newlines(text: &str) -> String {
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
    out
}

fn collapse_spaces(text: &str) -> String {
    // `/  +/g` → ' ' : runs of TWO OR MORE ASCII spaces collapse to one.
    let mut out = String::with_capacity(text.len());
    let mut run = 0usize;
    for c in text.chars() {
        if c == ' ' {
            run += 1;
        } else {
            if run >= 1 {
                out.push(' ');
            }
            run = 0;
            out.push(c);
        }
    }
    if run >= 1 {
        out.push(' ');
    }
    out
}
