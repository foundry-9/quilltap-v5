//! Text-based (spontaneous XML) tool-call parsers (W4.7c) — v4
//! `packages/plugin-utils/src/tools/text-parsers.ts`.
//!
//! Some models emit tool-call-like XML markup in their prose instead of using
//! native function calling. Plugins compose these leaves into their
//! `hasTextToolMarkers` / `parseTextToolCalls` / `stripTextToolMarkers` methods —
//! the provider-text-markers strategy of the text-tool loop (W4.1f).
//!
//! ## Regex fidelity
//!
//! JS `\w` is ASCII (`[A-Za-z0-9_]`); JS `\s` in a non-`u` regex is a fixed set.
//! The Rust `regex` crate defaults to Unicode classes, so the ports below use
//! `(?-u:\s)` for structural whitespace (ASCII whitespace — a subset of JS `\s`;
//! non-ASCII whitespace inside a tag separator is a documented seam, never in
//! practice) and `(?i)` case-insensitivity for the ASCII literal tag names (the
//! exotic Unicode case-folds of `k`/`s`/… that JS's non-`u` `i` flag would NOT
//! fold are a documented seam, absent from real model output).
//!
//! ## The one backreference
//!
//! `/<(\w+)>([^<]*)<\/\1>/gi` (the `<key>value</key>` pair scanner in
//! `parseToolCallFormat` / `parseToolUseFormat`) uses a backreference — unsupported
//! by the `regex` crate — so it is hand-rolled in [`parse_named_tag_pairs`]
//! (ASCII-word tag names, case-insensitive close-tag match, `[^<]*` content).
//!
//! ## Offsets
//!
//! v4 reports `startIndex`/`endIndex` as UTF-16 offsets; they are used ONLY to
//! deduplicate (by `startIndex`) and sort across the format parsers. Both
//! operations are monotonic in string position, so byte offsets (what the `regex`
//! crate reports, and what [`parse_function_calls_format`] composes consistently)
//! produce an IDENTICAL dedup/sort result — no UTF-16 conversion needed here.

use serde_json::{Map, Value};
use std::sync::LazyLock;

use regex::Regex;

use super::parsers::normalize_js_numbers;
use super::ToolCallRequest;

/// The detected text-format tag family (v4 `ParsedTextTool.format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextToolFormat {
    Deepseek,
    Claude,
    Generic,
    FunctionCall,
    ToolUse,
    Invoke,
}

/// A parsed text-based tool call (v4 `ParsedTextTool`). Byte offsets (see the
/// module note).
#[derive(Debug, Clone)]
pub struct ParsedTextTool {
    pub tool_name: String,
    pub arguments: Value,
    pub full_match: String,
    pub start_index: usize,
    pub end_index: usize,
    pub format: TextToolFormat,
}

// ============================================================================
// Tool-name normalization (TOOL_NAME_ALIASES)
// ============================================================================

/// Map a hallucinated tool name to its canonical Quilltap name; pass unknown
/// names through (v4 `TOOL_NAME_ALIASES` + `normalizeToolName`).
pub fn normalize_tool_name(name: &str) -> String {
    let lowered = name.to_lowercase();
    let normalized = lowered.trim();
    let canonical = match normalized {
        "search" => "search",
        "generate_image" => "generate_image",
        "search_web" => "search_web",
        "memory" | "memory_search" | "search_memory" | "memories" | "search_memories"
        | "search_scriptorium" => "search",
        "image" | "create_image" | "image_generation" | "gen_image" => "generate_image",
        "web_search" | "websearch" | "web" => "search_web",
        "help_search" | "helpsearch" | "search_help" => "help_search",
        "help_navigate" | "helpnavigate" => "help_navigate",
        _ => return name.to_string(),
    };
    canonical.to_string()
}

/// `Object.values(args)[0]` — the first inserted value (preserve_order), or `""`.
fn first_value_or_empty(args: &Value) -> Value {
    args.as_object()
        .and_then(|m| m.values().next())
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()))
}

/// `args.a || args.b || Object.values(args)[0] || ''` (JS `||` chain).
fn arg_or_first(args: &Value, keys: &[&str]) -> Value {
    for k in keys {
        if let Some(v) = args.get(*k) {
            if is_truthy(v) {
                return v.clone();
            }
        }
    }
    let first = first_value_or_empty(args);
    if is_truthy(&first) {
        first
    } else {
        Value::String(String::new())
    }
}

/// JS truthiness for a JSON value (`false`/`0`/`""`/`null` are falsy).
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// v4 `convertToToolCallRequest`: normalize argument names for well-known tools,
/// else pass through.
pub fn convert_to_tool_call_request(parsed: &ParsedTextTool) -> ToolCallRequest {
    let args = &parsed.arguments;
    // A pair whose value is `None` is a JS `undefined` field — dropped by
    // `JSON.stringify` (e.g. `limit: parsed.arguments.limit` when absent).
    let build = |name: &str, pairs: Vec<(&str, Option<Value>)>| {
        let mut m = Map::new();
        for (k, v) in pairs {
            if let Some(v) = v {
                m.insert(k.to_string(), v);
            }
        }
        ToolCallRequest {
            name: name.to_string(),
            arguments: Value::Object(m),
            call_id: None,
        }
    };
    // `parsed.arguments.<key>` — present (possibly falsy) → Some, absent → None.
    let opt = |key: &str| args.get(key).cloned();
    match parsed.tool_name.as_str() {
        "search" => build(
            "search",
            vec![
                ("query", Some(arg_or_first(args, &["query", "search"]))),
                ("limit", opt("limit")),
            ],
        ),
        "generate_image" => build(
            "generate_image",
            vec![(
                "prompt",
                Some(arg_or_first(args, &["prompt", "description"])),
            )],
        ),
        "search_web" => build(
            "search_web",
            vec![("query", Some(arg_or_first(args, &["query", "search"])))],
        ),
        "help_search" => build(
            "help_search",
            vec![
                ("query", Some(arg_or_first(args, &["query", "search"]))),
                ("limit", opt("limit")),
            ],
        ),
        "help_navigate" => build(
            "help_navigate",
            vec![("url", Some(arg_or_first(args, &["url", "path"])))],
        ),
        _ => ToolCallRequest {
            name: parsed.tool_name.clone(),
            arguments: args.clone(),
            call_id: None,
        },
    }
}

// ============================================================================
// The hand-rolled backreference scanner  /<(\w+)>([^<]*)<\/\1>/gi
// ============================================================================

fn is_ascii_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Reproduce `/<(\w+)>([^<]*)<\/\1>/gi` global matching: `<name>value</name>`
/// where `name` is ASCII word chars and the close tag matches case-insensitively.
/// Content is `[^<]*` (stops at the first `<`, which must be the close). Returns
/// `(name, value)` pairs in match order — the caller trims (`.trim()`).
fn parse_named_tag_pairs(s: &str) -> Vec<(String, String)> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let name_start = i + 1;
        let mut j = name_start;
        while j < n && is_ascii_word(b[j]) {
            j += 1;
        }
        if j == name_start || j >= n || b[j] != b'>' {
            i += 1;
            continue;
        }
        let name = &s[name_start..j];
        let content_start = j + 1;
        let mut k = content_start;
        while k < n && b[k] != b'<' {
            k += 1;
        }
        if k + 1 >= n || b[k] != b'<' || b[k + 1] != b'/' {
            i += 1;
            continue;
        }
        let close_name_start = k + 2;
        let mut m = close_name_start;
        while m < n && is_ascii_word(b[m]) {
            m += 1;
        }
        if m == close_name_start || m >= n || b[m] != b'>' {
            i += 1;
            continue;
        }
        let close_name = &s[close_name_start..m];
        if !name.eq_ignore_ascii_case(close_name) {
            i += 1;
            continue;
        }
        let value = &s[content_start..k];
        out.push((name.to_string(), value.to_string()));
        i = m + 1;
    }
    out
}

// ============================================================================
// Compiled regexes (LazyLock)
// ============================================================================

// `(?i)` = ASCII-literal tag case-insensitivity (Unicode-fold seam documented);
// `(?s)` = `.` matches newlines (`[\s\S]`); `(?-u:\s)` = ASCII `\s`.
static FUNCTION_CALLS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<function_calls>(.*?)</function_calls>").expect("re"));
static INVOKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<invoke(?-u:\s)+name=["']([^"']+)["']>(.*?)</invoke>"#).expect("re")
});
static DEEPSEEK_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?is)<parameter(?-u:\s)+name=["']([^"']+)["'](?-u:\s)+string=["']([^"']*)["'][^>]*>([^<]*)</parameter>"#,
    )
    .expect("re")
});
static CLAUDE_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<parameter(?-u:\s)+name=["']([^"']+)["']>([^<]*)</parameter>"#).expect("re")
});
// v4's `antmlParamPattern` closes on the literal `</` + namespaced `parameter`
// (assembled at runtime so the sequence never appears verbatim in this source).
static ANTML_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    let pat = format!(
        r#"(?is)<parameter(?-u:\s)+name=["']([^"']+)["']>([^<]*)</{}:parameter>"#,
        "antml"
    );
    Regex::new(&pat).expect("re")
});
static TOOL_CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<tool_call>(.*?)</tool_call>").expect("re"));
static NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<name>([^<]+)</name>").expect("re"));
static ARGUMENTS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<arguments>(.*?)</arguments>").expect("re"));
static FUNCTION_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<function_call(?-u:\s)+name=["']([^"']+)["']>(.*?)</function_call>"#)
        .expect("re")
});
static PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<param(?-u:\s)+name=["']([^"']+)["']>([^<]*)</param>"#).expect("re")
});
static TOOL_USE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<tool_use(?:(?-u:\s)+name=["']([^"']+)["'])?(?-u:\s)*>(.*?)</tool_use>"#)
        .expect("re")
});
static ARGS_INPUT_PARAMS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:arguments|input|parameters)>(.*?)</(?:arguments|input|parameters)>")
        .expect("re")
});

// Marker checks + strippers.
static HAS_FUNCTION_CALLS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<function_calls>").expect("re"));
static HAS_TOOL_CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<tool_call>").expect("re"));
static HAS_FUNCTION_CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<function_call(?-u:\s)+").expect("re"));
static HAS_TOOL_USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<tool_use[\s>]").expect("re"));
static HAS_INVOKE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)<invoke(?-u:\s)+name="#).expect("re"));

static STRIP_FUNCTION_CALLS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<function_calls>.*?</function_calls>").expect("re"));
static STRIP_TOOL_CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<tool_call>.*?</tool_call>").expect("re"));
static STRIP_FUNCTION_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<function_call(?-u:\s)+[^>]*>.*?</function_call>").expect("re")
});
static STRIP_TOOL_USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<tool_use[\s>].*?</tool_use>").expect("re"));
static STRIP_INVOKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<invoke(?-u:\s)+name=["'][^"']*["']>.*?</invoke>"#).expect("re")
});
static NL3_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("re"));
static SPACES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"  +").expect("re"));

// ============================================================================
// JS Number(str) — the deepseek `string="false"` numeric coercion
// ============================================================================

/// A focused port of JS `Number(str)` for the deepseek text-block param subset:
/// trimmed-empty → 0; a plain decimal int/float/exponent → that value; anything
/// with a non-`e` letter (hex `0x…`, `Infinity`, `NaN`) → `None` (a documented
/// seam, absent from real emissions). Returns `None` on `isNaN`.
fn js_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return Some(0.0);
    }
    if t.chars()
        .any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
    {
        return None;
    }
    t.parse::<f64>().ok().filter(|f| !f.is_nan())
}

// ============================================================================
// Individual format parsers
// ============================================================================

/// v4 `parseFunctionCallsFormat` — `<function_calls><invoke name="…">…</invoke>`.
pub fn parse_function_calls_format(response: &str) -> Vec<ParsedTextTool> {
    let mut results = Vec::new();
    for wrapper in FUNCTION_CALLS_RE.captures_iter(response) {
        let whole = wrapper.get(0).unwrap();
        let wrapper_content = wrapper.get(1).unwrap().as_str();
        let content_offset = whole.start() + "<function_calls>".len();

        for invoke in INVOKE_RE.captures_iter(wrapper_content) {
            let invoke_whole = invoke.get(0).unwrap();
            let tool_name = invoke.get(1).unwrap().as_str();
            let param_content = invoke.get(2).unwrap().as_str();
            let invoke_start = content_offset + invoke_whole.start();
            let invoke_end = invoke_start + invoke_whole.as_str().len();

            let mut args = Map::new();
            let mut format = TextToolFormat::Claude;

            // DeepSeek: <parameter name="…" string="…">value</parameter>
            for pm in DEEPSEEK_PARAM_RE.captures_iter(param_content) {
                let name = pm.get(1).unwrap().as_str().to_string();
                let string_attr = pm.get(2).unwrap().as_str();
                let value = pm.get(3).unwrap().as_str().trim();
                let coerced: Value = if string_attr == "false" {
                    match js_number(value) {
                        Some(f) => crate::db::js_number_to_json(f),
                        None if value == "true" => Value::Bool(true),
                        None if value == "false" => Value::Bool(false),
                        None => Value::String(value.to_string()),
                    }
                } else {
                    Value::String(value.to_string())
                };
                args.insert(name, coerced);
                format = TextToolFormat::Deepseek;
            }

            // Claude: <parameter name="…">value</parameter> (only if empty)
            if args.is_empty() {
                for pm in CLAUDE_PARAM_RE.captures_iter(param_content) {
                    args.insert(
                        pm.get(1).unwrap().as_str().to_string(),
                        Value::String(pm.get(2).unwrap().as_str().trim().to_string()),
                    );
                }
            }

            // antml:parameter (Claude Code style) — always.
            for pm in ANTML_PARAM_RE.captures_iter(param_content) {
                args.insert(
                    pm.get(1).unwrap().as_str().to_string(),
                    Value::String(pm.get(2).unwrap().as_str().trim().to_string()),
                );
            }

            results.push(ParsedTextTool {
                tool_name: normalize_tool_name(tool_name),
                arguments: Value::Object(args),
                full_match: invoke_whole.as_str().to_string(),
                start_index: invoke_start,
                end_index: invoke_end,
                format,
            });
        }
    }
    results
}

/// v4 `parseToolCallFormat` — `<tool_call><name>…</name><arguments>…</arguments></tool_call>`.
pub fn parse_tool_call_format(response: &str) -> Vec<ParsedTextTool> {
    let mut results = Vec::new();
    for m in TOOL_CALL_RE.captures_iter(response) {
        let whole = m.get(0).unwrap();
        let content = m.get(1).unwrap().as_str();
        let Some(name_cap) = NAME_RE.captures(content) else {
            continue;
        };
        let tool_name = name_cap.get(1).unwrap().as_str().trim();
        let mut args = Map::new();
        if let Some(args_cap) = ARGUMENTS_RE.captures(content) {
            for (k, v) in parse_named_tag_pairs(args_cap.get(1).unwrap().as_str()) {
                args.insert(k, Value::String(v.trim().to_string()));
            }
        }
        results.push(ParsedTextTool {
            tool_name: normalize_tool_name(tool_name),
            arguments: Value::Object(args),
            full_match: whole.as_str().to_string(),
            start_index: whole.start(),
            end_index: whole.end(),
            format: TextToolFormat::Generic,
        });
    }
    results
}

/// v4 `parseFunctionCallFormat` — `<function_call name="…"><param …>…</param></function_call>`.
pub fn parse_function_call_format(response: &str) -> Vec<ParsedTextTool> {
    let mut results = Vec::new();
    for m in FUNCTION_CALL_RE.captures_iter(response) {
        let whole = m.get(0).unwrap();
        let tool_name = m.get(1).unwrap().as_str();
        let content = m.get(2).unwrap().as_str();
        let mut args = Map::new();
        for pm in PARAM_RE.captures_iter(content) {
            args.insert(
                pm.get(1).unwrap().as_str().to_string(),
                Value::String(pm.get(2).unwrap().as_str().trim().to_string()),
            );
        }
        for pm in CLAUDE_PARAM_RE.captures_iter(content) {
            args.insert(
                pm.get(1).unwrap().as_str().to_string(),
                Value::String(pm.get(2).unwrap().as_str().trim().to_string()),
            );
        }
        results.push(ParsedTextTool {
            tool_name: normalize_tool_name(tool_name),
            arguments: Value::Object(args),
            full_match: whole.as_str().to_string(),
            start_index: whole.start(),
            end_index: whole.end(),
            format: TextToolFormat::FunctionCall,
        });
    }
    results
}

/// v4 `parseToolUseFormat` — bare JSON, XML children, or attributed `<tool_use>`.
pub fn parse_tool_use_format(response: &str) -> Vec<ParsedTextTool> {
    let mut results = Vec::new();
    for m in TOOL_USE_RE.captures_iter(response) {
        let whole = m.get(0).unwrap();
        let attr_name = m.get(1).map(|x| x.as_str());
        let content = m.get(2).unwrap().as_str();
        let full_match = whole.as_str().to_string();
        let start_index = whole.start();
        let end_index = whole.end();

        // Bare JSON first (Gemini's primary format).
        let trimmed_content = content.trim();
        if trimmed_content.starts_with('{') {
            if let Ok(blob) = serde_json::from_str::<Value>(trimmed_content) {
                if blob.is_object() && is_truthy(blob.get("name").unwrap_or(&Value::Null)) {
                    let name = blob.get("name").and_then(Value::as_str).unwrap_or_default();
                    let raw_args = ["input", "arguments", "parameters"]
                        .iter()
                        .find_map(|k| blob.get(*k).filter(|v| is_truthy(v)))
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Map::new()));
                    let args = if raw_args.is_object() {
                        normalize_js_numbers(raw_args)
                    } else {
                        Value::Object(Map::new())
                    };
                    results.push(ParsedTextTool {
                        tool_name: normalize_tool_name(name),
                        arguments: args,
                        full_match,
                        start_index,
                        end_index,
                        format: TextToolFormat::ToolUse,
                    });
                    continue;
                }
            }
        }

        // Name from attribute or <name> child.
        let tool_name: String = match attr_name {
            Some(n) => n.to_string(),
            None => match NAME_RE.captures(content) {
                Some(c) => c.get(1).unwrap().as_str().trim().to_string(),
                None => continue,
            },
        };

        let mut args = Map::new();
        if let Some(args_cap) = ARGS_INPUT_PARAMS_RE.captures(content) {
            let args_content = args_cap.get(1).unwrap().as_str().trim();
            if args_content.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<Value>(args_content) {
                    if let Value::Object(obj) = normalize_js_numbers(parsed) {
                        for (k, v) in obj {
                            args.insert(k, v);
                        }
                    }
                }
            }
            if args.is_empty() {
                for (k, v) in parse_named_tag_pairs(args_content) {
                    args.insert(k, Value::String(v.trim().to_string()));
                }
            }
        }

        results.push(ParsedTextTool {
            tool_name: normalize_tool_name(&tool_name),
            arguments: Value::Object(args),
            full_match,
            start_index,
            end_index,
            format: TextToolFormat::ToolUse,
        });
    }
    results
}

/// v4 `parseInvokeFormat` — bare `<invoke name="…">` (no `<function_calls>` wrapper).
pub fn parse_invoke_format(response: &str) -> Vec<ParsedTextTool> {
    let mut results = Vec::new();
    for m in INVOKE_RE.captures_iter(response) {
        let whole = m.get(0).unwrap();
        let tool_name = m.get(1).unwrap().as_str();
        let param_content = m.get(2).unwrap().as_str();
        let mut args = Map::new();
        for pm in CLAUDE_PARAM_RE.captures_iter(param_content) {
            args.insert(
                pm.get(1).unwrap().as_str().to_string(),
                Value::String(pm.get(2).unwrap().as_str().trim().to_string()),
            );
        }
        results.push(ParsedTextTool {
            tool_name: normalize_tool_name(tool_name),
            arguments: Value::Object(args),
            full_match: whole.as_str().to_string(),
            start_index: whole.start(),
            end_index: whole.end(),
            format: TextToolFormat::Invoke,
        });
    }
    results
}

// ============================================================================
// Composite utilities
// ============================================================================

/// v4 `parseAllXMLFormats` — every format, deduped by `start_index` (first-wins),
/// sorted by `start_index`. Bare `<invoke>` runs LAST so wrapped invokes are
/// claimed by `parse_function_calls_format` first and deduped.
pub fn parse_all_xml_formats(response: &str) -> Vec<ParsedTextTool> {
    let mut all = Vec::new();
    all.extend(parse_function_calls_format(response));
    all.extend(parse_tool_call_format(response));
    all.extend(parse_function_call_format(response));
    all.extend(parse_tool_use_format(response));
    all.extend(parse_invoke_format(response));

    let mut seen = std::collections::HashSet::new();
    let mut deduped: Vec<ParsedTextTool> = all
        .into_iter()
        .filter(|r| seen.insert(r.start_index))
        .collect();
    deduped.sort_by_key(|r| r.start_index);
    deduped
}

/// v4 `parseAllXMLAsToolCalls`.
pub fn parse_all_xml_as_tool_calls(response: &str) -> Vec<ToolCallRequest> {
    parse_all_xml_formats(response)
        .iter()
        .map(convert_to_tool_call_request)
        .collect()
}

/// v4 `hasAnyXMLToolMarkers`.
pub fn has_any_xml_tool_markers(response: &str) -> bool {
    HAS_FUNCTION_CALLS_RE.is_match(response)
        || HAS_TOOL_CALL_RE.is_match(response)
        || HAS_FUNCTION_CALL_RE.is_match(response)
        || HAS_TOOL_USE_RE.is_match(response)
        || HAS_INVOKE_RE.is_match(response)
}

/// v4 `hasToolUseMarkers`.
pub fn has_tool_use_markers(response: &str) -> bool {
    HAS_TOOL_USE_RE.is_match(response)
}

fn cleanup_whitespace(s: &str) -> String {
    let s = NL3_RE.replace_all(s, "\n\n");
    let s = SPACES_RE.replace_all(&s, " ");
    s.trim().to_string()
}

/// v4 `stripAllXMLToolMarkers`.
pub fn strip_all_xml_tool_markers(response: &str) -> String {
    let s = STRIP_FUNCTION_CALLS_RE.replace_all(response, "");
    let s = STRIP_TOOL_CALL_RE.replace_all(&s, "");
    let s = STRIP_FUNCTION_CALL_RE.replace_all(&s, "");
    let s = STRIP_TOOL_USE_RE.replace_all(&s, "");
    let s = STRIP_INVOKE_RE.replace_all(&s, "");
    cleanup_whitespace(&s)
}

/// v4 `stripToolUseMarkers` (the raw Gemini stripper, no whitespace cleanup).
pub fn strip_tool_use_markers(response: &str) -> String {
    STRIP_TOOL_USE_RE.replace_all(response, "").to_string()
}

/// The Google plugin's `stripTextToolMarkers` = `stripToolUseMarkers` + cleanup.
pub fn strip_tool_use_markers_with_cleanup(response: &str) -> String {
    cleanup_whitespace(&strip_tool_use_markers(response))
}
// v4's close tag is the literal `
