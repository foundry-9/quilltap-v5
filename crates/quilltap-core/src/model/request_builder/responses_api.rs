//! The Responses-API family request builders (W4.7c part 2): OpenAI and Grok.
//! Ported from each plugin's `buildBaseRequestParams` + `formatMessagesFor-
//! ResponsesAPI`. The OpenAI `previous_response_id` chaining transform lives in
//! [`build_openai_body`] (the *fallback-to-full-input on send error* is a
//! transport concern — W4.7d).

use serde_json::{json, Value};

use super::{num, Body, RequestInput, RequestMessage, ToolCallMsg};

// ============================================================================
// Message formatting (Responses API input items)
// ============================================================================

/// A user message's content array: `[{type:input_text, text}]` (empty text when
/// the content is empty — v4's "avoid empty content array" guard).
fn user_content(content: &str) -> Value {
    if content.is_empty() {
        json!([{ "type": "input_text", "text": "" }])
    } else {
        json!([{ "type": "input_text", "text": content }])
    }
}

fn function_call_items(tool_calls: &[ToolCallMsg]) -> Vec<Value> {
    tool_calls
        .iter()
        .map(|tc| {
            json!({
                "type": "function_call",
                "call_id": tc.id,
                "name": tc.name,
                "arguments": tc.arguments,
            })
        })
        .collect()
}

/// v4 OpenAI `formatMessagesForResponsesAPI`: the FIRST system message becomes
/// top-level `instructions`; later system messages become `developer` role items.
fn format_openai_messages(messages: &[RequestMessage]) -> (Vec<Value>, Option<String>) {
    let mut instructions: Option<String> = None;
    let mut input = Vec::new();
    for msg in messages {
        match msg.role.as_str() {
            "system" if instructions.is_none() => {
                instructions = Some(msg.content.clone());
            }
            "system" => {
                input.push(
                    json!({ "type": "message", "role": "developer", "content": msg.content }),
                );
            }
            "tool" => {
                if let Some(id) = &msg.tool_call_id {
                    input.push(json!({ "type": "function_call_output", "call_id": id, "output": msg.content }));
                }
            }
            "assistant" => {
                input.push(
                    json!({ "type": "message", "role": "assistant", "content": msg.content }),
                );
                input.extend(function_call_items(&msg.tool_calls));
            }
            _ => {
                input.push(json!({ "type": "message", "role": "user", "content": user_content(&msg.content) }));
            }
        }
    }
    (input, instructions)
}

/// v4 Grok `formatMessagesForResponsesAPI`: system stays as a `system`-role item
/// (xAI has no `developer`/`instructions`).
fn format_grok_messages(messages: &[RequestMessage]) -> Vec<Value> {
    let mut input = Vec::new();
    for msg in messages {
        match msg.role.as_str() {
            "tool" => {
                if let Some(id) = &msg.tool_call_id {
                    input.push(json!({ "type": "function_call_output", "call_id": id, "output": msg.content }));
                }
            }
            "system" => {
                input.push(json!({ "type": "message", "role": "system", "content": msg.content }));
            }
            "assistant" => {
                input.push(
                    json!({ "type": "message", "role": "assistant", "content": msg.content }),
                );
                input.extend(function_call_items(&msg.tool_calls));
            }
            _ => {
                input.push(json!({ "type": "message", "role": "user", "content": user_content(&msg.content) }));
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

pub fn build_openai_body(input: &RequestInput) -> Value {
    let (item_input, instructions) = format_openai_messages(&input.messages);
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
        b.set("stream", json!(true));
        b.into_value()
    } else {
        let mut b = base;
        b.set("stream", json!(true));
        b.into_value()
    }
}

// ============================================================================
// Grok
// ============================================================================

pub fn build_grok_body(input: &RequestInput) -> Value {
    let item_input = format_grok_messages(&input.messages);
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
        .set("stream", json!(true));
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
