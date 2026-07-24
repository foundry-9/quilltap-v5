//! The Google request LOGIC (W4.7c part 2): the recursive JSON-Schema sanitizer
//! and the `formatMessagesForGoogle` `contents` builder (with the `thoughtSignature`
//! round-trip). Ported from the google plugin.
//!
//! **The genai-SDK wire framing is DEFERRED to the transport (W4.7d).** The genai
//! `ai.models.generateContentStream({ model, contents, config })` SDK restructures
//! `config` into a top-level `generationConfig` / `safetySettings` / `tools` /
//! `systemInstruction` body AND reorders object keys (e.g. `{name, args}` →
//! `{args, name}`), so the *wire* body is the SDK's serialization — not something
//! this sans-IO builder produces. This module ports + verifies the Quilltap-side
//! request LOGIC that the SDK consumes: [`sanitize_schema_for_google`] and
//! [`format_messages_for_google`] (the two named W4.7c deliverables:
//! schema-sanitize + thoughtSignature), plus [`build_config`] for the config bag.
//! The `format_messages_for_google` output matches the PLUGIN's `contents` exactly
//! (verified in `request_builder_equivalence` against v4's real method).

use serde_json::{json, Map, Value};

use super::{RequestInput, StreamMessage};

/// JSON-Schema fields Google's function-calling API rejects (v4
/// `UNSUPPORTED_SCHEMA_FIELDS`).
const UNSUPPORTED_SCHEMA_FIELDS: &[&str] = &[
    "propertyNames",
    "additionalItems",
    "contains",
    "patternProperties",
    "dependencies",
    "if",
    "then",
    "else",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "$schema",
    "$id",
    "$ref",
    "$comment",
    "definitions",
    "$defs",
    "examples",
    "default",
    "const",
    "contentMediaType",
    "contentEncoding",
];

/// v4 `sanitizeSchemaForGoogle` — strip unsupported fields, uppercase `type`
/// strings (recursively).
pub fn sanitize_schema_for_google(schema: &Value) -> Value {
    match schema {
        Value::Array(a) => Value::Array(a.iter().map(sanitize_schema_for_google).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (key, value) in o {
                if UNSUPPORTED_SCHEMA_FIELDS.contains(&key.as_str()) {
                    continue;
                }
                if key == "type" {
                    match value {
                        Value::String(s) => {
                            out.insert(key.clone(), Value::String(s.to_uppercase()));
                            continue;
                        }
                        Value::Array(arr) => {
                            out.insert(
                                key.clone(),
                                Value::Array(
                                    arr.iter()
                                        .map(|v| match v {
                                            Value::String(s) => Value::String(s.to_uppercase()),
                                            other => other.clone(),
                                        })
                                        .collect(),
                                ),
                            );
                            continue;
                        }
                        _ => {}
                    }
                }
                if value.is_object() || value.is_array() {
                    out.insert(key.clone(), sanitize_schema_for_google(value));
                } else {
                    out.insert(key.clone(), value.clone());
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

// ============================================================================
// Model classification
// ============================================================================

/// v4 `GoogleProvider.isThinkingModel` — the Gemini thinking-model name
/// predicate. Public for the streaming composer's `google-parts` decoder
/// selection (the decoder's terminal `finalContent` fallback is gated on it).
pub fn is_thinking_model(model: &str) -> bool {
    let m = model.to_lowercase();
    if m.contains("gemini-3") {
        return true;
    }
    [
        "gemini-2.5-pro",
        "gemini-2.5-flash",
        "gemini-pro-latest",
        "gemini-flash-latest",
        "gemini-flash-lite-latest",
    ]
    .iter()
    .any(|t| m.contains(t))
}

fn is_gemini3_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("gemini-3")
        || m.contains("gemini-pro-latest")
        || m.contains("gemini-flash-latest")
        || m.contains("gemini-flash-lite-latest")
}

fn supports_tool_calling(model: &str) -> bool {
    let m = model.to_lowercase();
    let no_tools = [
        "gemini-2.5-flash-image",
        "gemini-2.0-flash-exp-image-generation",
        "imagen",
    ];
    if no_tools.iter().any(|t| m.contains(t)) {
        return false;
    }
    if m.contains("-image") && !m.contains("vision") {
        return false;
    }
    true
}

// ============================================================================
// Message formatting (the `contents` builder + thoughtSignature)
// ============================================================================

/// The result of [`format_messages_for_google`] (v4's return shape).
#[derive(Debug, Clone)]
pub struct GoogleContents {
    pub contents: Vec<Value>,
    pub system_instruction: Option<String>,
    pub should_disable_tools: bool,
}

/// v4 `formatMessagesForGoogle` (the text path). Extracts the system instruction,
/// decides `shouldDisableTools` (unsupported model, or a thinking+tools model with
/// a legacy assistant message lacking a `thoughtSignature`), merges consecutive
/// user messages, batches tool responses, and emits `contents` — attaching
/// `thoughtSignature` to assistant text parts (the Gemini 3 round-trip).
pub fn format_messages_for_google(
    messages: &[StreamMessage],
    model: &str,
    has_tools: bool,
) -> GoogleContents {
    let is_thinking = is_thinking_model(model);

    let system_msgs: Vec<&StreamMessage> = messages
        .iter()
        .filter(|m| matches!(m, StreamMessage::System { .. }))
        .collect();
    let system_instruction = if system_msgs.is_empty() {
        None
    } else {
        Some(
            system_msgs
                .iter()
                .map(|m| m.content())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };
    let non_system: Vec<&StreamMessage> = messages
        .iter()
        .filter(|m| !matches!(m, StreamMessage::System { .. }))
        .collect();

    let mut should_disable_tools = false;
    if has_tools && !supports_tool_calling(model) {
        should_disable_tools = true;
    }
    if !should_disable_tools && is_thinking && has_tools {
        let legacy = non_system.iter().any(|m| {
            matches!(
                m,
                StreamMessage::Assistant {
                    thought_signature: None,
                    ..
                }
            )
        });
        if legacy {
            should_disable_tools = true;
        }
    }

    // Merge consecutive plain user messages (not tool results — v4 also guarded
    // `msg.toolCallId == null`, vacuous here: a User variant never carries one).
    let mut merged: Vec<StreamMessage> = Vec::new();
    for msg in &non_system {
        if let (
            Some(StreamMessage::User { content: last, .. }),
            StreamMessage::User { content, .. },
        ) = (merged.last_mut(), msg)
        {
            *last = format!("{last}\n\n{content}");
            continue;
        }
        merged.push((*msg).clone());
    }

    let mut contents: Vec<Value> = Vec::new();
    let mut pending: Vec<Value> = Vec::new();
    let flush = |pending: &mut Vec<Value>, contents: &mut Vec<Value>| {
        if !pending.is_empty() {
            contents.push(json!({ "role": "user", "parts": std::mem::take(pending) }));
        }
    };

    for msg in &merged {
        // Tool result. v4's arm also caught "any message with a toolCallId"
        // regardless of role — unrepresentable in the carrying enum (only a
        // Tool variant pairs by id), so that half is gone; and the
        // `"unknown_function"` fallback is unreachable (name falls back to the
        // REQUIRED call id) but kept as v4's written chain.
        if let StreamMessage::Tool {
            call_id,
            name,
            content,
        } = msg
        {
            let response = serde_json::from_str::<Value>(content)
                .unwrap_or_else(|_| json!({ "result": content }));
            let function_name = name.clone().unwrap_or_else(|| call_id.clone());
            pending.push(
                json!({ "functionResponse": { "name": function_name, "response": response } }),
            );
            continue;
        }

        flush(&mut pending, &mut contents);
        let mut parts: Vec<Value> = Vec::new();

        if let StreamMessage::Assistant {
            content,
            tool_calls,
            thought_signature,
            ..
        } = msg
        {
            if !tool_calls.is_empty() {
                if !content.is_empty() {
                    if let Some(sig) = thought_signature {
                        parts.push(json!({ "text": content, "thoughtSignature": sig }));
                    } else {
                        parts.push(json!({ "text": content }));
                    }
                }
                for tc in tool_calls {
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
                    parts.push(
                        json!({ "functionCall": { "name": tc.function.name, "args": args } }),
                    );
                }
                contents.push(json!({ "role": "model", "parts": parts }));
                continue;
            }
        }

        if !msg.content().is_empty() {
            if let StreamMessage::Assistant {
                content,
                thought_signature: Some(sig),
                ..
            } = msg
            {
                parts.push(json!({ "text": content, "thoughtSignature": sig }));
            } else {
                parts.push(json!({ "text": msg.content() }));
            }
        }

        let role = if matches!(msg, StreamMessage::Assistant { .. }) {
            "model"
        } else {
            "user"
        };
        contents.push(json!({ "role": role, "parts": parts }));
    }
    flush(&mut pending, &mut contents);

    GoogleContents {
        contents,
        system_instruction,
        should_disable_tools,
    }
}

// ============================================================================
// The config bag (the input to the genai SDK; wire framing deferred)
// ============================================================================

const SAFETY_CATEGORIES: &[&str] = &[
    "HARM_CATEGORY_HATE_SPEECH",
    "HARM_CATEGORY_SEXUALLY_EXPLICIT",
    "HARM_CATEGORY_DANGEROUS_CONTENT",
    "HARM_CATEGORY_HARASSMENT",
];

/// v4's `config` object (the SDK input). Not the wire body — the genai SDK reframes
/// this into `generationConfig`/`safetySettings`/etc. (transport, W4.7d).
pub fn build_config(
    input: &RequestInput,
    sys: &GoogleContents,
    tools: Vec<Value>,
    has_tools: bool,
) -> Value {
    let mut cfg = super::Body::new();
    cfg.set(
        "safetySettings",
        Value::Array(
            SAFETY_CATEGORIES
                .iter()
                .map(|c| json!({ "category": c, "threshold": "BLOCK_NONE" }))
                .collect(),
        ),
    );
    let mut max_out = input.max_tokens.unwrap_or(4096);
    cfg.set("temperature", super::num(input.temperature.unwrap_or(0.7)))
        .set("maxOutputTokens", super::num(max_out as f64))
        .set("topP", super::num(input.top_p.unwrap_or(1.0)));
    if let Some(instr) = &sys.system_instruction {
        cfg.set("systemInstruction", json!(instr));
    }
    if has_tools && !sys.should_disable_tools {
        cfg.set("tools", Value::Array(tools));
    }
    if let Some(stop) = &input.stop {
        cfg.set("stopSequences", json!(stop));
    }
    if is_thinking_model(&input.model) {
        let is_g3 = is_gemini3_model(&input.model);
        if !input.strict_max_tokens {
            cfg.set(
                "thinkingConfig",
                if is_g3 {
                    json!({ "thinkingLevel": "high", "includeThoughts": true })
                } else {
                    json!({ "thinkingBudget": 4096, "includeThoughts": true })
                },
            );
            max_out = max_out.max(8192);
            cfg.set("maxOutputTokens", super::num(max_out as f64));
        } else {
            cfg.set(
                "thinkingConfig",
                if is_g3 {
                    json!({ "thinkingLevel": "low", "includeThoughts": true })
                } else {
                    json!({ "thinkingBudget": 1024, "includeThoughts": true })
                },
            );
        }
    }
    cfg.into_value()
}

// ============================================================================
// The genai-SDK config → wire framing (W4.7d — closes the W4.7c deferral)
// ============================================================================

/// The genai SDK's `generationConfig` field order (a FIXED schema order, NOT
/// insertion order — verified against the recorded wire: our config inserts
/// `temperature, maxOutputTokens, topP` but the wire emits `temperature, topP,
/// maxOutputTokens`). Only the fields present in the config bag are emitted, in
/// this order. (The full genai list; the chat path produces the leading subset.)
const GENERATION_CONFIG_FIELDS: &[&str] = &[
    "temperature",
    "topP",
    "topK",
    "candidateCount",
    "maxOutputTokens",
    "stopSequences",
    "responseLogprobs",
    "logprobs",
    "presencePenalty",
    "frequencyPenalty",
    "seed",
    "responseMimeType",
    "responseSchema",
    "responseJsonSchema",
    "responseModalities",
    "mediaResolution",
    "speechConfig",
    "thinkingConfig",
    "imageConfig",
    "enableEnhancedCivicAnswers",
];

/// An error building the google wire body (a `partialArgs` / `willContinue`
/// functionCall part, which the genai SDK rejects).
#[derive(Debug)]
pub struct GoogleWireError(pub String);

impl std::fmt::Display for GoogleWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for GoogleWireError {}

/// Reframe one `functionCall` object `{ name, args }` → `{ id?, args, name }`
/// (the SDK's field-by-field rebuild; `partialArgs`/`willContinue` THROW).
fn reframe_function_call(fc: &Value) -> Result<Value, GoogleWireError> {
    if fc.get("partialArgs").is_some() || fc.get("willContinue").is_some() {
        return Err(GoogleWireError(
            "google functionCall partialArgs/willContinue unsupported".to_string(),
        ));
    }
    let mut out = Map::new();
    if let Some(id) = fc.get("id") {
        out.insert("id".into(), id.clone());
    }
    if let Some(args) = fc.get("args") {
        out.insert("args".into(), args.clone());
    }
    if let Some(name) = fc.get("name") {
        out.insert("name".into(), name.clone());
    }
    Ok(Value::Object(out))
}

/// Reframe one part: a `functionCall` part rebuilds its inner object; every other
/// part shape passes through unchanged.
fn reframe_part(part: &Value) -> Result<Value, GoogleWireError> {
    if let Some(fc) = part.get("functionCall") {
        let mut out = Map::new();
        out.insert("functionCall".into(), reframe_function_call(fc)?);
        return Ok(Value::Object(out));
    }
    Ok(part.clone())
}

/// Reframe one content `{ role, parts }` → `{ parts, role }` (parts-before-role,
/// each part reframed).
fn reframe_content(content: &Value) -> Result<Value, GoogleWireError> {
    let parts = content
        .get("parts")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(reframe_part).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    let mut out = Map::new();
    out.insert("parts".into(), Value::Array(parts));
    if let Some(role) = content.get("role") {
        out.insert("role".into(), role.clone());
    }
    Ok(Value::Object(out))
}

/// Reproduce the genai SDK's `{ model, contents, config }` → wire body reframing
/// (W4.7d, closing the W4.7c deferral). Verified byte-exact against the recorded
/// wire (`request_builder_google_wire_equivalence`):
///   - each content `{ role, parts }` → `{ parts, role }`; `functionCall`
///     `{ name, args }` → `{ args, name }` (id first if present);
///   - the string `systemInstruction` → `{ parts: [{ text }], role: "user" }`;
///   - the flat `config`'s sampling/output fields nest under `generationConfig`
///     (in the SDK's fixed field order), while `systemInstruction` /
///     `safetySettings` / `tools` stay at the request root;
///   - root key order: `contents, systemInstruction, safetySettings, tools?,
///     generationConfig`.
pub fn build_google_wire_body(input: &RequestInput) -> Result<Value, GoogleWireError> {
    let (tools, has_tools) = build_tools(input);
    let sys = format_messages_for_google(&input.messages, &input.model, has_tools);
    let config = build_config(input, &sys, tools, has_tools);
    let config_obj = config.as_object().cloned().unwrap_or_default();

    let mut root = super::Body::new();

    // contents (reframed).
    let contents: Vec<Value> = sys
        .contents
        .iter()
        .map(reframe_content)
        .collect::<Result<Vec<_>, _>>()?;
    root.set("contents", Value::Array(contents));

    // systemInstruction → Content wrapper.
    if let Some(instr) = &sys.system_instruction {
        root.set(
            "systemInstruction",
            json!({ "parts": [{ "text": instr }], "role": "user" }),
        );
    }

    // safetySettings (root).
    if let Some(ss) = config_obj.get("safetySettings") {
        root.set("safetySettings", ss.clone());
    }

    // tools (root, only when present).
    if let Some(t) = config_obj.get("tools") {
        root.set("tools", t.clone());
    }

    // generationConfig: the sampling/output fields, in the SDK's fixed order.
    let mut gen = super::Body::new();
    for field in GENERATION_CONFIG_FIELDS {
        if let Some(v) = config_obj.get(*field) {
            gen.set(field, v.clone());
        }
    }
    root.set("generationConfig", gen.into_value());

    Ok(root.into_value())
}

/// The google chat URL for a model (v4's genai endpoint):
/// `{baseUrl}/models/{model}:{streamGenerateContent?alt=sse | generateContent}`.
pub fn google_chat_url(base_url: &str, model: &str, stream: bool) -> String {
    if stream {
        format!("{base_url}/models/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base_url}/models/{model}:generateContent")
    }
}

/// Build the google function-declaration tools (v4's
/// `tools.push({functionDeclarations})` + `{googleSearch:{}}`), applying
/// [`sanitize_schema_for_google`] to each tool's properties.
pub fn build_tools(input: &RequestInput) -> (Vec<Value>, bool) {
    let mut tools = Vec::new();
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            let decls: Vec<Value> = t
                .iter()
                .map(|tool| {
                    let props = tool
                        .get("parameters")
                        .and_then(|p| p.get("properties"))
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let required = tool
                        .get("parameters")
                        .and_then(|p| p.get("required"))
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    json!({
                        "name": tool.get("name").cloned().unwrap_or(Value::Null),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": {
                            "type": "OBJECT",
                            "properties": sanitize_schema_for_google(&props),
                            "required": required,
                        },
                    })
                })
                .collect();
            tools.push(json!({ "functionDeclarations": decls }));
        }
    }
    if input.web_search_enabled {
        tools.push(json!({ "googleSearch": {} }));
    }
    let has_tools = !tools.is_empty();
    (tools, has_tools)
}
