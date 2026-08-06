//! The Responses-API family request builders (W4.7c part 2): OpenAI and Grok.
//! Ported from each plugin's `buildBaseRequestParams` + `formatMessagesFor-
//! ResponsesAPI`. The OpenAI `previous_response_id` chaining transform lives in
//! [`build_openai_body`] (the *fallback-to-full-input on send error* is a
//! transport concern — W4.7d).

use serde_json::{json, Value};

use super::{
    att_fail, att_id, att_str, num, Body, RequestInput, StreamAttachmentResults, StreamMessage,
    ToolCallPayload,
};

// ============================================================================
// Message formatting (Responses API input items)
// ============================================================================

const OPENAI_SUPPORTED_MIME_TYPES: &[&str] =
    &["image/jpeg", "image/png", "image/gif", "image/webp"];
const GROK_SUPPORTED_MIME_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Which plugin's user-content attachment arms apply (the two differ only in
/// the failure-string label and Grok's extra — dead — mime branches).
#[derive(Clone, Copy, PartialEq)]
enum ResponsesFlavor {
    OpenAi,
    Grok,
}

/// A user message's content array (v4 `formatMessagesForResponsesAPI`'s user
/// branch): the text part when content is non-empty, then one `input_image`
/// part per surviving attachment, then the empty-text guard ONLY when nothing
/// else was pushed — so an image-only message carries no empty text part
/// (pinned by the `image-attachment-empty-content` vector).
fn user_content(
    msg: &StreamMessage,
    flavor: ResponsesFlavor,
    results: &mut StreamAttachmentResults,
) -> Value {
    let content = msg.content();
    let mut parts: Vec<Value> = Vec::new();
    if !content.is_empty() {
        parts.push(json!({ "type": "input_text", "text": content }));
    }

    let (label, mimes) = match flavor {
        ResponsesFlavor::OpenAi => ("OpenAI", OPENAI_SUPPORTED_MIME_TYPES),
        ResponsesFlavor::Grok => ("Grok", GROK_SUPPORTED_MIME_TYPES),
    };
    for a in msg.attachments() {
        let mime = a
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // The gate + its rejection string are flavor-specific. OpenAI stays
        // images-only. v4 bug 33 (`43a1b5b1`) widened Grok's gate to
        // `isHandledMimeType` — supported images OR `text/*` OR `application/pdf`
        // — so the text/PDF arms below (dead code behind the old images-only
        // gate) are now LIVE. Grok's rejection lists the images then appends
        // `, text/*` AFTER the joined list (PDF is not mentioned).
        let handled = match flavor {
            ResponsesFlavor::OpenAi => mimes.contains(&mime),
            ResponsesFlavor::Grok => {
                mimes.contains(&mime) || mime.starts_with("text/") || mime == "application/pdf"
            }
        };
        if !handled {
            let reject = match flavor {
                ResponsesFlavor::OpenAi => format!(
                    "Unsupported file type: {mime}. {label} supports: {}",
                    mimes.join(", ")
                ),
                ResponsesFlavor::Grok => format!(
                    "Unsupported file type: {mime}. {label} supports: {}, text/*",
                    mimes.join(", ")
                ),
            };
            att_fail(results, a, reject);
            continue;
        }
        // v4 bug 33: text/PDF now reach this data check first — `File data not
        // loaded` is newly reachable for them.
        let Some(data) = att_str(a, "data") else {
            att_fail(results, a, "File data not loaded");
            continue;
        };
        match flavor {
            ResponsesFlavor::OpenAi => {
                parts.push(json!({
                    "type": "input_image",
                    "image_url": format!("data:{mime};base64,{data}"),
                    "detail": "auto",
                }));
                results.sent.push(att_id(a));
            }
            ResponsesFlavor::Grok => {
                // v4 bug 33 made this branch live (it was dead behind the old
                // images-only gate). Images send inline; `text/*` embeds inline
                // (base64-round-tripped, bug 34); PDF reaches the honest
                // Files-API refusal; anything else that passed the gate is
                // unreachable here (only image/text/pdf pass `isHandledMimeType`).
                if mime.starts_with("image/") {
                    parts.push(json!({
                        "type": "input_image",
                        "image_url": format!("data:{mime};base64,{data}"),
                        "detail": "auto",
                    }));
                    results.sent.push(att_id(a));
                } else if mime.starts_with("text/") {
                    // v4 bug 34: `data` may be raw text OR base64;
                    // `decode_base64_text` round-trips to tell which (a bare
                    // decode would mangle already-plain text — see the helper).
                    let text = decode_base64_text(data);
                    let filename = a
                        .get("filename")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    parts.push(json!({
                        "type": "input_text",
                        "text": format!("[File: {filename}]\n{text}"),
                    }));
                    results.sent.push(att_id(a));
                } else {
                    // application/pdf (the only remaining `isHandledMimeType`
                    // arm): the honest Files-API refusal (deferred in v4 too).
                    att_fail(
                        results,
                        a,
                        "PDF and binary document support requires Grok Files API (not yet implemented)",
                    );
                }
            }
        }
    }

    if parts.is_empty() {
        parts.push(json!({ "type": "input_text", "text": "" }));
    }
    Value::Array(parts)
}

/// Node's `Buffer.from(s, 'base64')`, byte-faithful — it NEVER throws (v4's
/// decode-failure `catch` arms are dead code). Probed on Node 24 (the §3
/// unification review): invalid characters are SKIPPED (whitespace and
/// URL-safe `-`/`_` map into the alphabet), decoding STOPS at the first `=`,
/// and the accumulated 6-bit groups emit `floor(bits / 8)` bytes — so
/// `"hello"` decodes to `[0x85, 0xE9, 0x65]` and `"x=1"` to `[]`, where a
/// strict decoder would refuse both.
pub(crate) fn node_lenient_base64(data: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in data.bytes() {
        let v = match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// v4 bug 34 (`43a1b5b1`)'s `decodeBase64Text`. `Buffer.from(s,'base64')` never
/// throws — a bare decode silently mangles already-plain text (`"hello"` →
/// mojibake, `"x=1"` → `""`) — so a try/catch cannot tell base64 from plain
/// text. Round-trip instead: decode ([`node_lenient_base64`], the exact
/// `Buffer.from` byte-behaviour), re-encode (standard base64), normalize BOTH
/// (strip whitespace + trailing `=`) and compare. A match means the input really
/// was base64 → return the utf-8 decode (lossy, `toString('utf-8')`); a mismatch
/// means plain text all along → return it verbatim. Applied in the Anthropic
/// text/plain document arm and the Grok `text/*` arm.
pub(crate) fn decode_base64_text(data: &str) -> String {
    use base64::Engine as _;
    let decoded = node_lenient_base64(data);
    let reencoded = base64::engine::general_purpose::STANDARD.encode(&decoded);
    if normalize_base64(&reencoded) == normalize_base64(data) {
        String::from_utf8_lossy(&decoded).into_owned()
    } else {
        data.to_string()
    }
}

/// v4's `normalize`: `s.replace(/\s+/g, '').replace(/=+$/, '')` — strip every
/// JS-`\s` whitespace char, then trailing `=` padding.
fn normalize_base64(s: &str) -> String {
    let no_ws: String = s.chars().filter(|c| !is_js_whitespace(*c)).collect();
    no_ws.trim_end_matches('=').to_string()
}

/// JS regex `\s` — the exact set `String.prototype.replace(/\s+/g,'')` strips
/// (Unicode whitespace plus `﻿`, which is `\s` in JS but NOT Rust's
/// `char::is_whitespace`; and NOT ``, which Rust would include).
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn function_call_items(tool_calls: &[ToolCallPayload]) -> Vec<Value> {
    tool_calls
        .iter()
        .map(|tc| {
            json!({
                "type": "function_call",
                "call_id": tc.id,
                "name": tc.function.name,
                "arguments": tc.function.arguments,
            })
        })
        .collect()
}

/// v4 OpenAI `formatMessagesForResponsesAPI`: the FIRST system message becomes
/// top-level `instructions`; later system messages become `developer` role items.
/// A tool result always carries its `call_id` (the enum requires one — v4's
/// `if (msg.toolCallId)` drop arm is unrepresentable).
fn format_openai_messages(
    messages: &[StreamMessage],
    results: &mut StreamAttachmentResults,
) -> (Vec<Value>, Option<String>) {
    let mut instructions: Option<String> = None;
    let mut input = Vec::new();
    for msg in messages {
        match msg {
            StreamMessage::System { content } if instructions.is_none() => {
                instructions = Some(content.clone());
            }
            StreamMessage::System { content } => {
                input.push(json!({ "type": "message", "role": "developer", "content": content }));
            }
            StreamMessage::Tool {
                call_id, content, ..
            } => {
                input.push(json!({ "type": "function_call_output", "call_id": call_id, "output": content }));
            }
            StreamMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                input.push(json!({ "type": "message", "role": "assistant", "content": content }));
                input.extend(function_call_items(tool_calls));
            }
            StreamMessage::User { .. } => {
                input.push(
                    json!({ "type": "message", "role": "user", "content": user_content(msg, ResponsesFlavor::OpenAi, results) }),
                );
            }
        }
    }
    (input, instructions)
}

/// v4 Grok `formatMessagesForResponsesAPI`: system stays as a `system`-role item
/// (xAI has no `developer`/`instructions`).
fn format_grok_messages(
    messages: &[StreamMessage],
    results: &mut StreamAttachmentResults,
) -> Vec<Value> {
    let mut input = Vec::new();
    for msg in messages {
        match msg {
            StreamMessage::Tool {
                call_id, content, ..
            } => {
                input.push(json!({ "type": "function_call_output", "call_id": call_id, "output": content }));
            }
            StreamMessage::System { content } => {
                input.push(json!({ "type": "message", "role": "system", "content": content }));
            }
            StreamMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                input.push(json!({ "type": "message", "role": "assistant", "content": content }));
                input.extend(function_call_items(tool_calls));
            }
            StreamMessage::User { .. } => {
                input.push(
                    json!({ "type": "message", "role": "user", "content": user_content(msg, ResponsesFlavor::Grok, results) }),
                );
            }
        }
    }
    input
}

/// `extractLastUserMessage(input)` — the last `role:user` item, else the last item.
fn extract_last_user_message(input: &[Value]) -> Vec<Value> {
    for item in input.iter().rev() {
        if item.get("role").and_then(Value::as_str) == Some("user") {
            return vec![item.clone()];
        }
    }
    input.last().cloned().into_iter().collect()
}

// ============================================================================
// Model classification (v4 prefix lists)
// ============================================================================

const REASONING_MODEL_PREFIXES: &[&str] = &["o1", "o3", "o4", "gpt-5"];
const EXTENDED_RETENTION_MODEL_PREFIXES: &[&str] = &["gpt-5.1", "gpt-5.2", "gpt-5.5"];

fn is_reasoning_model(model: &str) -> bool {
    let m = model.to_lowercase();
    REASONING_MODEL_PREFIXES.iter().any(|p| m.starts_with(p))
}

fn supports_extended_cache_retention(model: &str) -> bool {
    let m = model.to_lowercase();
    EXTENDED_RETENTION_MODEL_PREFIXES
        .iter()
        .any(|p| m.starts_with(p))
}

// ============================================================================
// Tool + text-config helpers
// ============================================================================

/// v4 `formatToolsForResponsesAPI`: `{type:function, name, description?, parameters,
/// strict:false}` (flat, not nested under `function`). `description` is omitted
/// when absent (JS `?? undefined` dropped by `JSON.stringify`).
fn format_tools_for_responses(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            let f = tool.get("function").unwrap_or(&Value::Null);
            let name = f.get("name").cloned().unwrap_or(Value::Null);
            let params = f.get("parameters").cloned().unwrap_or(Value::Null);
            let mut m = Body::new();
            m.set("type", json!("function")).set("name", name);
            if let Some(desc) = f.get("description") {
                if !desc.is_null() {
                    m.set("description", desc.clone());
                }
            }
            m.set("parameters", params).set("strict", json!(false));
            m.into_value()
        })
        .collect()
}

/// v4 OpenAI `buildTextConfig` + the verbosity add-on.
fn build_text_config(input: &RequestInput) -> Option<Value> {
    let mut cfg = Body::new();
    if let Some(rf) = &input.response_format {
        match rf.get("type").and_then(Value::as_str) {
            Some("json_object") => {
                cfg.set("format", json!({ "type": "json_object" }));
            }
            Some("json_schema") => {
                if let Some(schema) = rf.get("jsonSchema") {
                    let name = schema
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("response");
                    let strict = schema
                        .get("strict")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    cfg.set(
                        "format",
                        json!({ "type": "json_schema", "name": name, "schema": schema.get("schema").cloned().unwrap_or(Value::Null), "strict": strict }),
                    );
                }
            }
            _ => {}
        }
    }
    let verbosity = input
        .profile_parameters
        .as_ref()
        .and_then(|p| p.get("verbosity"))
        .and_then(Value::as_str)
        .filter(|v| ["low", "medium", "high"].contains(v));
    if let Some(v) = verbosity {
        cfg.set("verbosity", json!(v));
    }
    let value = cfg.into_value();
    if value.as_object().is_some_and(|o| o.is_empty()) {
        None
    } else {
        Some(value)
    }
}

// ============================================================================
// OpenAI
// ============================================================================

/// v4 OpenAI `buildBaseRequestParams` — the request minus the streaming/chaining
/// tail.
fn openai_base(input: &RequestInput, item_input: Vec<Value>, instructions: Option<String>) -> Body {
    let is_reasoning = is_reasoning_model(&input.model);
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("input", Value::Array(item_input))
        .set("store", json!(false))
        .set(
            "max_output_tokens",
            num(input.max_tokens.unwrap_or(4096) as f64),
        );
    if let Some(instr) = instructions {
        b.set("instructions", json!(instr));
    }
    if !is_reasoning {
        b.set("top_p", num(input.top_p.unwrap_or(1.0)));
        if let Some(t) = input.temperature {
            b.set("temperature", num(t));
        }
    } else if !input.strict_max_tokens {
        if input.max_tokens.unwrap_or(0) < 4096 {
            b.set("max_output_tokens", num(4096.0));
        }
        let effort = input
            .profile_parameters
            .as_ref()
            .and_then(|p| p.get("reasoningEffort"))
            .and_then(Value::as_str)
            .filter(|e| ["minimal", "low", "medium", "high"].contains(e));
        let include_summary = input
            .profile_parameters
            .as_ref()
            .and_then(|p| p.get("reasoningSummary"))
            .and_then(Value::as_bool)
            == Some(true);
        if let Some(e) = effort {
            b.set(
                "reasoning",
                if include_summary {
                    json!({ "effort": e, "summary": "auto" })
                } else {
                    json!({ "effort": e })
                },
            );
        } else if include_summary {
            b.set("reasoning", json!({ "summary": "auto" }));
        }
    } else {
        b.set("reasoning", json!({ "effort": "low" }));
    }

    // Tools + web search.
    let mut tools: Vec<Value> = Vec::new();
    if input.web_search_enabled {
        tools.push(json!({ "type": "web_search_preview" }));
        b.set("include", json!(["web_search_call.action.sources"]));
    }
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            tools.extend(format_tools_for_responses(t));
        }
    }
    if !tools.is_empty() {
        b.set("tools", Value::Array(tools));
    }

    b.set_opt("text", build_text_config(input));

    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("prompt_cache_key", json!(k));
            if supports_extended_cache_retention(&input.model) {
                b.set("prompt_cache_retention", json!("24h"));
            }
        }
    }
    if let Some(stop) = &input.stop {
        if !stop.is_empty() {
            b.set("stop", json!(stop));
        }
    }
    b
}

/// v4 `OpenAIProvider.streamMessage` / `.sendMessage`. Both spread the same
/// `buildBaseRequestParams` result and append `stream` last (`true` / `false`);
/// nothing else moves between the two methods — the whole Responses-API family
/// differs only by the flag.
pub fn build_openai_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    let (item_input, instructions) = format_openai_messages(&input.messages, results);
    let base = openai_base(input, item_input.clone(), instructions);

    if let Some(prev) = &input.previous_response_id {
        // Chained: `{...baseParams, input: lastUser, previous_response_id, stream}`.
        // The spread keeps `input`'s position (2nd); previous_response_id + stream
        // are appended.
        let mut b = base.clone();
        b.set(
            "input",
            Value::Array(extract_last_user_message(&item_input)),
        );
        b.set("previous_response_id", json!(prev));
        b.set("stream", json!(input.stream));
        b.into_value()
    } else {
        let mut b = base;
        b.set("stream", json!(input.stream));
        b.into_value()
    }
}

// ============================================================================
// Grok
// ============================================================================

pub fn build_grok_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    let item_input = format_grok_messages(&input.messages, results);
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("input", Value::Array(item_input))
        .set("store", json!(false))
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set(
            "max_output_tokens",
            num(input.max_tokens.unwrap_or(4096) as f64),
        )
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set("stream", json!(input.stream));
    if let Some(stop) = &input.stop {
        b.set("stop", json!(stop));
    }
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("prompt_cache_key", json!(k));
        }
    }
    if input
        .profile_parameters
        .as_ref()
        .and_then(|p| p.get("reasoningSummary"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        b.set("reasoning", json!({ "summary": "auto" }));
    }
    // Tools: web search (web_search + x_search + include:citations) then function.
    let mut tools: Vec<Value> = Vec::new();
    if input.web_search_enabled {
        tools.push(json!({ "type": "web_search" }));
        tools.push(json!({ "type": "x_search" }));
        b.set("include", json!(["citations"]));
    }
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            tools.extend(format_tools_for_responses(t));
        }
    }
    if !tools.is_empty() {
        b.set("tools", Value::Array(tools));
    }
    b.into_value()
}

#[cfg(test)]
mod tests {
    use super::{decode_base64_text, node_lenient_base64};

    /// v4 bug 34's `decodeBase64Text` round-trip. Node-24-verified consequences
    /// (the fix's whole point): base64-LOOKING-but-not text ships VERBATIM
    /// instead of mangled, while genuine base64 still decodes.
    #[test]
    fn decode_base64_text_round_trip() {
        // Plain text that merely looks base64 → verbatim (was mojibake / "").
        assert_eq!(decode_base64_text("hello"), "hello");
        assert_eq!(decode_base64_text("x=1"), "x=1");
        // Raw text with a newline (Grok has no pre-guard) → verbatim.
        assert_eq!(
            decode_base64_text("line one\nline two"),
            "line one\nline two"
        );
        // Genuine base64 still decodes.
        assert_eq!(decode_base64_text("aGVsbG8="), "hello");
        assert_eq!(decode_base64_text("aGVsbG8gd29ybGQ="), "hello world");
        // Whitespace inside genuine base64 is normalized away on both sides.
        assert_eq!(decode_base64_text("aGVs bG8="), "hello");
    }

    /// Every vector probed on Node 24 (`Buffer.from(s, 'base64')`) at the §3
    /// unification review — the decoder must be byte-faithful, incl. the
    /// mangle arms a strict decoder would refuse.
    #[test]
    fn node_lenient_base64_matches_node_24() {
        let cases: &[(&str, &[u8])] = &[
            ("hello", &[133, 233, 101]),
            ("x=1", &[]),
            ("a!b", &[105]),
            ("ab=cd", &[105]),
            ("aGVsbG8=", b"hello"),
            ("aGVs bG8=", b"hello"),
            ("YQ", b"a"),
            ("YQ=", b"a"),
            ("!!!", &[]),
            ("TWFu", b"Man"),
            ("TWE=", b"Ma"),
            ("TQ==", b"M"),
            ("hel\tlo", &[133, 233, 101]),
            ("++//", &[251, 239, 255]),
            ("-_", &[251]),
            ("+/", &[251]),
            ("", &[]),
        ];
        for (input, want) in cases {
            assert_eq!(node_lenient_base64(input), want.to_vec(), "input {input:?}");
        }
    }
}
