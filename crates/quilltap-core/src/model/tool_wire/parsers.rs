//! Native tool-call parsers (W4.7c) — v4
//! `packages/plugin-utils/src/tools/parsers.ts`.
//!
//! Each parser extracts native tool calls from a provider's raw (non-streamed)
//! response object into the standardized [`ToolCallRequest`] shape. The
//! detector ([`super::detect_tool_calls`]) dispatches to these by the provider's
//! `toolFormat`.
//!
//! **Number fidelity.** A tool call's `arguments` is `JSON.parse`d from the
//! provider's argument STRING, so an integer-valued number renders bare (`3`, not
//! `3.0`) when `JSON.stringify`d back. The parsed `serde_json::Value` is walked by
//! [`normalize_js_numbers`] to collapse integer-valued floats — matching v4's JS
//! numbers byte-for-byte on the common path (the exotic `1e21` / `Infinity`
//! renderings are a documented seam, absent from the corpus).

use serde_json::Value;

/// A standardized tool-call request (v4 `ToolCallRequest`): `{ name, arguments,
/// callId? }`. `arguments` is an arbitrary JSON object; `call_id` is `None` when
/// the provider omits it (v4 `callId: id || undefined`, dropped by `JSON.stringify`).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
    pub call_id: Option<String>,
}

/// Walk a JSON value collapsing integer-valued floats to integers (JS numbers do
/// not distinguish `3` from `3.0`; `JSON.stringify` emits `3` for both).
pub fn normalize_js_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) => crate::db::js_number_to_json(f),
            None => Value::Number(n),
        },
        Value::Array(a) => Value::Array(a.into_iter().map(normalize_js_numbers).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, normalize_js_numbers(v)))
                .collect(),
        ),
        other => other,
    }
}

fn as_array(v: Option<&Value>) -> Option<&Vec<Value>> {
    v.and_then(Value::as_array)
}

/// v4 `parseOpenAIToolCalls` (OpenAI / Grok / DeepSeek / z-ai / OpenRouter /
/// Ollama). Reads `tool_calls` / `toolCalls` directly, then
/// `choices[0].message.*`, then `choices[0].delta.*` (OpenRouter streaming). Skips
/// tool calls whose arguments are not a complete `{...}` JSON object (the
/// mid-stream-incomplete guard).
pub fn parse_openai_tool_calls(response: &Value) -> Vec<ToolCallRequest> {
    let mut tool_calls = Vec::new();

    // Direct tool_calls / toolCalls
    let mut arr = as_array(response.get("tool_calls"));
    if arr.is_none() {
        arr = as_array(response.get("toolCalls"));
    }
    // choices[0].message.tool_calls | .toolCalls
    if arr.is_none() {
        if let Some(msg) = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c0| c0.get("message"))
        {
            arr = as_array(msg.get("tool_calls")).or_else(|| as_array(msg.get("toolCalls")));
        }
    }
    // choices[0].delta.tool_calls | .toolCalls (OpenRouter SDK streaming)
    if arr.is_none() {
        if let Some(delta) = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c0| c0.get("delta"))
        {
            arr = as_array(delta.get("tool_calls")).or_else(|| as_array(delta.get("toolCalls")));
        }
    }

    let Some(arr) = arr else {
        return tool_calls;
    };
    if arr.is_empty() {
        return tool_calls;
    }

    for tc in arr {
        // tc.type === 'function' && tc.function
        if tc.get("type").and_then(Value::as_str) != Some("function") {
            continue;
        }
        let Some(function) = tc.get("function") else {
            continue;
        };
        // argsStr = tc.function.arguments || '{}'
        let args_str = match function.get("arguments").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => s,
            _ => "{}",
        };
        let trimmed = args_str.trim();
        // Skip incomplete JSON (must be a complete braced object).
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(args_str) else {
            continue;
        };
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let call_id = tc
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        tool_calls.push(ToolCallRequest {
            name,
            arguments: normalize_js_numbers(parsed),
            call_id,
        });
    }

    tool_calls
}

/// v4 `parseAnthropicToolCalls`. Reads `content[]` for `{ type: 'tool_use', name,
/// input, id }` blocks.
pub fn parse_anthropic_tool_calls(response: &Value) -> Vec<ToolCallRequest> {
    let mut tool_calls = Vec::new();
    let Some(content) = response.get("content").and_then(Value::as_array) else {
        return tool_calls;
    };
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = block
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let arguments = block
            .get("input")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        let call_id = block
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        tool_calls.push(ToolCallRequest {
            name: name.to_string(),
            arguments: normalize_js_numbers(arguments),
            call_id,
        });
    }
    tool_calls
}

/// v4 `parseGoogleToolCalls`. Reads `candidates[0].content.parts[]` for
/// `{ functionCall: { name, args } }`. No `callId` in Google's shape.
pub fn parse_google_tool_calls(response: &Value) -> Vec<ToolCallRequest> {
    let mut tool_calls = Vec::new();
    let Some(parts) = response
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c0| c0.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
    else {
        return tool_calls;
    };
    for part in parts {
        let Some(fc) = part.get("functionCall") else {
            continue;
        };
        let name = fc
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments = fc
            .get("args")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        tool_calls.push(ToolCallRequest {
            name,
            arguments: normalize_js_numbers(arguments),
            call_id: None,
        });
    }
    tool_calls
}

/// The Google plugin's `parseToolCalls`: the pre-extracted `response.functionCalls`
/// fast path (from the provider wrapper), else [`parse_google_tool_calls`].
pub fn parse_google_plugin_tool_calls(response: &Value) -> Vec<ToolCallRequest> {
    if let Some(fcs) = response.get("functionCalls").and_then(Value::as_array) {
        return fcs
            .iter()
            .map(|fc| ToolCallRequest {
                name: fc
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments: normalize_js_numbers(
                    fc.get("args")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Default::default())),
                ),
                call_id: None,
            })
            .collect();
    }
    parse_google_tool_calls(response)
}
