//! The chat-completions family request builders (W4.7c part 2): DeepSeek, Z.AI,
//! OpenRouter (the raw-fetch tools/vision path), Ollama, and the OpenAI-compatible
//! base. Ported from each plugin's `streamMessage` request-body construction; the
//! DeepSeek `reasoning_content` echo transform lives in [`chat_messages`].
//!
//! Every builder inserts keys in v4's exact assignment order (the SDK / raw fetch
//! sends `JSON.stringify(body)` verbatim — see the module note).

use serde_json::{json, Value};

use super::{num, Body, RequestInput, RequestMessage};

// ============================================================================
// Shared chat-completions message formatting
// ============================================================================

/// `msg.content || null` — a falsy (empty) content collapses to JSON null (the
/// assistant-with-tool-calls shape).
fn content_or_null(content: &str) -> Value {
    if content.is_empty() {
        Value::Null
    } else {
        Value::String(content.to_string())
    }
}

/// Map messages to OpenAI chat-completion format. `literal_function_type` emits
/// `type: "function"` on a tool call (DeepSeek / Z.AI) vs the message's own
/// `tc.type` (OpenRouter / Ollama / base). `echo_reasoning` appends
/// `reasoning_content` on a tool-call assistant turn (the DeepSeek / Z.AI rule).
fn chat_messages(
    messages: &[RequestMessage],
    literal_function_type: bool,
    echo_reasoning: bool,
) -> Value {
    let mut out = Vec::new();
    for msg in messages {
        match msg.role.as_str() {
            "tool" => {
                let Some(tcid) = &msg.tool_call_id else {
                    continue; // backward-compat: drop tool messages without an id
                };
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": tcid,
                    "content": msg.content,
                }));
            }
            "assistant" if !msg.tool_calls.is_empty() => {
                let tool_calls: Vec<Value> = msg
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": if literal_function_type { "function" } else { tc.type_.as_str() },
                            "function": { "name": tc.name, "arguments": tc.arguments },
                        })
                    })
                    .collect();
                let mut m = Body::new();
                m.set("role", json!("assistant"))
                    .set("content", content_or_null(&msg.content))
                    .set("tool_calls", Value::Array(tool_calls));
                if echo_reasoning {
                    if let Some(rc) = &msg.reasoning_content {
                        if !rc.is_empty() {
                            m.set("reasoning_content", json!(rc));
                        }
                    }
                }
                out.push(m.into_value());
            }
            _ => {
                // system / assistant (no tool calls) / user → {role, content}
                out.push(json!({ "role": msg.role, "content": msg.content }));
            }
        }
    }
    Value::Array(out)
}

/// `params.responseFormat` → the OpenAI `response_format` object, or `None`.
fn response_format(input: &RequestInput) -> Option<Value> {
    let rf = input.response_format.as_ref()?;
    match rf.get("type").and_then(Value::as_str) {
        Some("json_object") => Some(json!({ "type": "json_object" })),
        Some("json_schema") => rf
            .get("jsonSchema")
            .map(|schema| json!({ "type": "json_schema", "json_schema": schema })),
        _ => None,
    }
}

/// The base chat-completions body (model, messages, temperature/max_tokens/top_p
/// with v4's defaults, stream, stream_options). The caller layers tools / stop /
/// etc. on top in v4's order.
fn base_body(input: &RequestInput, messages: Value) -> Body {
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set("stream", json!(true))
        .set("stream_options", json!({ "include_usage": true }));
    b
}

fn stop_value(input: &RequestInput) -> Option<Value> {
    input.stop.as_ref().map(|s| json!(s))
}

// ============================================================================
// OpenAI-compatible base
// ============================================================================

/// v4 `OpenAICompatibleProvider.streamMessage`. The plainest chat-completions
/// body — no tools, no profile params, just `user` on a cache key.
pub fn build_openai_compatible_body(input: &RequestInput) -> Value {
    let messages = chat_messages(&input.messages, false, false);
    let mut b = base_body(input, messages);
    b.set_opt("stop", stop_value(input));
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("user", json!(k));
        }
    }
    b.into_value()
}

// ============================================================================
// DeepSeek (+ reasoning_content echo + thinking-incompatible strip)
// ============================================================================

const DEEPSEEK_PROFILE_ALLOWLIST: &[&str] = &[
    "frequency_penalty",
    "presence_penalty",
    "logprobs",
    "top_logprobs",
    "thinking",
    "reasoning_effort",
];

/// v4 DeepSeek `applyProfileParameters` — forward the allow-listed keys, with the
/// `thinking: "enabled"` string → `{ type: "enabled" }` normalization.
fn apply_profile_params(b: &mut Body, input: &RequestInput, allowlist: &[&str]) {
    let Some(Value::Object(profile)) = &input.profile_parameters else {
        return;
    };
    for key in allowlist {
        let Some(value) = profile.get(*key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        // Empty string = "omit and use the model default".
        if value.as_str() == Some("") {
            continue;
        }
        if *key == "thinking" {
            if let Some(s) = value.as_str() {
                b.set(key, json!({ "type": s }));
                continue;
            }
        }
        b.set(key, value.clone());
    }
}

/// v4 DeepSeek `stripThinkingIncompatibleParams` — when thinking is enabled, drop
/// temperature/top_p/frequency_penalty/presence_penalty.
fn strip_thinking_incompatible(b: &mut Body) {
    let enabled = matches!(b.get("thinking"), Some(Value::Object(o)) if o.get("type").and_then(Value::as_str) == Some("enabled"));
    if !enabled {
        return;
    }
    for k in [
        "temperature",
        "top_p",
        "frequency_penalty",
        "presence_penalty",
    ] {
        b.remove(k);
    }
}

pub fn build_deepseek_body(input: &RequestInput) -> Value {
    let messages = chat_messages(&input.messages, true, true);
    let mut b = base_body(input, messages);
    b.set_opt("stop", stop_value(input));
    if let Some(tools) = &input.tools {
        if !tools.is_empty() {
            b.set("tools", json!(tools));
            b.set_opt("tool_choice", input.tool_choice.clone());
        }
    }
    b.set_opt("response_format", response_format(input));
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("user_id", json!(k));
        }
    }
    apply_profile_params(&mut b, input, DEEPSEEK_PROFILE_ALLOWLIST);
    strip_thinking_incompatible(&mut b);
    b.into_value()
}

// ============================================================================
// Z.AI (+ web search tool + reasoning-effort default)
// ============================================================================

const ZAI_PROFILE_ALLOWLIST: &[&str] = &["thinking", "do_sample", "reasoning_effort"];

fn zai_web_search_tool() -> Value {
    json!({
        "type": "web_search",
        "web_search": { "enable": "True", "search_engine": "search-prime", "search_result": "True" },
    })
}

/// v4 `supportsReasoningEffort`: glm-5.2-and-newer (parse `glm-<major>[.<minor>]`,
/// exclude the `glm-Nv` vision family).
fn zai_supports_reasoning_effort(model: &str) -> bool {
    let m = model.trim().to_lowercase();
    // Exclude the vision family (glm-<digits>v…): a 'v' right after the major.
    let re_vision = regex::Regex::new(r"(?i)^glm-\d+v").unwrap();
    if re_vision.is_match(model.trim()) {
        return false;
    }
    let re = regex::Regex::new(r"(?i)^glm-(\d+)(?:\.(\d+))?").unwrap();
    let Some(caps) = re.captures(&m) else {
        return false;
    };
    let major: i64 = caps
        .get(1)
        .and_then(|x| x.as_str().parse().ok())
        .unwrap_or(0);
    let minor: i64 = caps
        .get(2)
        .and_then(|x| x.as_str().parse().ok())
        .unwrap_or(0);
    if major > 5 {
        return true;
    }
    if major < 5 {
        return false;
    }
    minor >= 2
}

pub fn build_zai_body(input: &RequestInput) -> Value {
    let messages = chat_messages(&input.messages, true, true);
    let mut b = base_body(input, messages);
    b.set_opt("stop", stop_value(input));

    let mut tools: Vec<Value> = Vec::new();
    if input.web_search_enabled {
        tools.push(zai_web_search_tool());
    }
    if let Some(t) = &input.tools {
        tools.extend(t.iter().cloned());
    }
    if !tools.is_empty() {
        b.set("tools", Value::Array(tools));
        b.set_opt("tool_choice", input.tool_choice.clone());
    }
    b.set_opt("response_format", response_format(input));
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("user", json!(k));
        }
    }
    apply_profile_params(&mut b, input, ZAI_PROFILE_ALLOWLIST);
    // Default reasoning_effort=high on glm-5.2+ unless thinking is disabled.
    if zai_supports_reasoning_effort(&input.model) && !b.contains("reasoning_effort") {
        let thinking_disabled = matches!(b.get("thinking"), Some(Value::Object(o)) if o.get("type").and_then(Value::as_str) == Some("disabled"));
        if !thinking_disabled {
            b.set("reasoning_effort", json!("high"));
        }
    }
    b.into_value()
}

// ============================================================================
// OpenRouter (the raw-fetch chat-completions path — tools / vision)
// ============================================================================

/// OpenRouter reshapes tools to `{type:function, function:{name, description,
/// parameters}}` reading `tool.function?.x || tool.x`.
fn openrouter_tool(tool: &Value) -> Value {
    let get = |k: &str| {
        tool.get("function")
            .and_then(|f| f.get(k))
            .or_else(|| tool.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    json!({
        "type": "function",
        "function": { "name": get("name"), "description": get("description"), "parameters": get("parameters") },
    })
}

pub fn build_openrouter_body(input: &RequestInput) -> Value {
    let messages = chat_messages(&input.messages, false, false);
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("stream", json!(true))
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)));
    // v4 sets `stop: params.stop` in the literal (dropped by stringify when absent).
    b.set_opt("stop", stop_value(input));

    let mut tools: Vec<Value> = Vec::new();
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            tools.extend(t.iter().map(openrouter_tool));
        }
    }
    // web_search_preview appended after function tools (the tools array is
    // created lazily; if only web search, it starts empty then gets the preview).
    if input.web_search_enabled {
        tools.push(json!({ "type": "web_search_preview" }));
    }
    if !tools.is_empty() {
        b.set("tools", Value::Array(tools.clone()));
    }
    // tool_choice:'auto' only set when function tools were present.
    if input.tools.as_ref().is_some_and(|t| !t.is_empty()) {
        b.set("tool_choice", json!("auto"));
    }
    // reasoning is ALWAYS set (effort from profile, else {exclude:false}).
    let reasoning_effort = input
        .profile_parameters
        .as_ref()
        .and_then(|p| p.get("reasoningEffort"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    b.set(
        "reasoning",
        match reasoning_effort {
            Some(e) => json!({ "effort": e, "exclude": false }),
            None => json!({ "exclude": false }),
        },
    );
    b.into_value()
}

// ============================================================================
// Ollama (raw fetch to /api/chat)
// ============================================================================

pub fn build_ollama_body(input: &RequestInput) -> Value {
    let messages = chat_messages(&input.messages, false, false);
    let mut options = Body::new();
    options
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("num_predict", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set_opt("stop", stop_value(input));
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("stream", json!(true))
        .set("options", options.into_value());
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            b.set("tools", json!(t));
        }
    }
    b.into_value()
}
