//! The chat-completions family request builders (W4.7c part 2): DeepSeek, Z.AI,
//! OpenRouter (the raw-fetch tools/vision path), Ollama, and the OpenAI-compatible
//! base. Ported from each plugin's `streamMessage` AND `sendMessage` request-body
//! construction; the DeepSeek `reasoning_content` echo transform lives in
//! [`chat_messages`].
//!
//! Every builder inserts keys in v4's exact assignment order (the SDK / raw fetch
//! sends `JSON.stringify(body)` verbatim — see the module note).
//!
//! **`RequestInput.stream` selects which v4 method is being reproduced** (P4.11).
//! The non-streaming halves are NOT "the streaming body with the flag flipped" —
//! each plugin differs, and the differences are recorded, not reasoned:
//!
//! - DeepSeek / Z.AI: `stream` flips and `stream_options` disappears.
//! - OpenAI-compatible: `sendMessage` omits the `stream` key ENTIRELY, and `stop`
//!   sits before it in both modes (v5 used to emit `stop` last — a pre-existing
//!   divergence this provider's first-ever corpus coverage exposed).
//! - Ollama: `stream` flips.
//! - OpenRouter: a different builder altogether — see [`build_openrouter_body`].

use serde_json::{json, Value};

use super::{
    att_fail, att_id, att_str, collect_drop_failures, num, Body, BuildError, RequestInput,
    StreamAttachmentResults, StreamMessage,
};

// ============================================================================
// Attachment handling (P4.21)
// ============================================================================

/// v4's fixed drop-and-report messages (each provider's `attachmentErrorMessage`
/// / `collectAttachmentFailures` literal).
const DEEPSEEK_ATTACHMENT_ERROR: &str =
    "DeepSeek models do not accept file attachments. Send text-only messages.";
const OLLAMA_ATTACHMENT_ERROR: &str =
    "Ollama file attachment support not yet implemented (requires multimodal model detection)";
const OPENAI_COMPATIBLE_ATTACHMENT_ERROR: &str =
    "OpenAI-compatible provider file attachment support varies by implementation (not yet implemented)";

/// v4's Z.AI supported mime types (`Z_AI_SUPPORTED_MIME_TYPES`).
const Z_AI_SUPPORTED_MIME_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
/// v4's OpenRouter supported image mime types (`SUPPORTED_IMAGE_MIME_TYPES`).
const OPENROUTER_IMAGE_MIME_TYPES: &[&str] =
    &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// v4 Z.AI `isVisionModel` (`VISION_MODEL_PATTERNS`: `/^glm-\d+(\.\d+)?v/i`,
/// `/^glm-5v/i`, `/^autoglm-phone/i` — the second is subsumed by the first;
/// kept as written).
fn is_zai_vision_model(model: &str) -> bool {
    use std::sync::LazyLock;
    static PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
        vec![
            regex::Regex::new(r"(?i)^glm-\d+(\.\d+)?v").unwrap(),
            regex::Regex::new(r"(?i)^glm-5v").unwrap(),
            regex::Regex::new(r"(?i)^autoglm-phone").unwrap(),
        ]
    });
    PATTERNS.iter().any(|re| re.is_match(model))
}

/// v4 Z.AI `buildUserContent`: no attachments → the plain string; otherwise a
/// content-parts ARRAY (text + `image_url` parts), even when every attachment
/// fails — the non-vision / no-data arms still switch the wire shape to an
/// array (pinned by the `image-attachment-non-vision` vector).
fn zai_user_content(
    msg: &StreamMessage,
    model_supports_vision: bool,
    results: &mut StreamAttachmentResults,
) -> Value {
    let atts = msg.attachments();
    if atts.is_empty() {
        return json!(msg.content());
    }
    let mut parts: Vec<Value> = Vec::new();
    if !msg.content().is_empty() {
        parts.push(json!({ "type": "text", "text": msg.content() }));
    }
    for a in atts {
        let mime = a
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !Z_AI_SUPPORTED_MIME_TYPES.contains(&mime) {
            att_fail(
                results,
                a,
                format!(
                    "Unsupported file type: {mime}. Z.AI supports: {}",
                    Z_AI_SUPPORTED_MIME_TYPES.join(", ")
                ),
            );
            continue;
        }
        if !model_supports_vision {
            att_fail(
                results,
                a,
                "Selected Z.AI model does not support image input. Use a vision model such as glm-4.5v or glm-4.6v.",
            );
            continue;
        }
        // v4 `attachmentToImageUrl`: `attachment.url` first, else the data URL.
        let url = match att_str(a, "url") {
            Some(u) => u.to_string(),
            None => match att_str(a, "data") {
                Some(d) => format!("data:{mime};base64,{d}"),
                None => {
                    att_fail(results, a, "Attachment missing data or URL");
                    continue;
                }
            },
        };
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
        results.sent.push(att_id(a));
    }
    if parts.is_empty() {
        parts.push(json!({ "type": "text", "text": "" }));
    }
    Value::Array(parts)
}

/// v4 OpenRouter `buildMessageContent`: plain string unless the message carries
/// at least one SUPPORTED image with data-or-url; then a parts array. The url
/// is `img.url ?? data:` — NULLISH, so a present-but-empty `url` ships verbatim
/// (never happens in practice; carried faithfully).
fn openrouter_message_content(msg: &StreamMessage) -> Value {
    let images: Vec<&Value> = msg
        .attachments()
        .iter()
        .filter(|a| {
            let mime = a
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            OPENROUTER_IMAGE_MIME_TYPES.contains(&mime)
                && (att_str(a, "data").is_some() || att_str(a, "url").is_some())
        })
        .collect();
    if images.is_empty() {
        return json!(msg.content());
    }
    let mut parts: Vec<Value> = Vec::new();
    if !msg.content().is_empty() {
        parts.push(json!({ "type": "text", "text": msg.content() }));
    }
    for img in images {
        let mime = img
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let url = match img.get("url").and_then(Value::as_str) {
            Some(u) => u.to_string(),
            None => format!(
                "data:{mime};base64,{}",
                img.get("data").and_then(Value::as_str).unwrap_or_default()
            ),
        };
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    Value::Array(parts)
}

/// v4 OpenRouter `collectAttachmentResults`: image mimes with data-or-url are
/// `sent` (they ship inline as content parts); image rows missing both fail;
/// non-image mimes fail with the not-yet-implemented message.
fn openrouter_collect_attachment_results(
    messages: &[StreamMessage],
    results: &mut StreamAttachmentResults,
) {
    for m in messages {
        for a in m.attachments() {
            let mime = a
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if OPENROUTER_IMAGE_MIME_TYPES.contains(&mime) {
                if att_str(a, "data").is_some() || att_str(a, "url").is_some() {
                    results.sent.push(att_id(a));
                } else {
                    att_fail(results, a, "Image attachment missing data and url");
                }
            } else {
                att_fail(
                    results,
                    a,
                    format!("OpenRouter {mime} attachments are not yet implemented"),
                );
            }
        }
    }
}

/// Whether a non-streaming OpenRouter request takes the raw-wire VISION send
/// path (v4 bug 31: a formattable image escapes the SDK to
/// `sendViaChatCompletions`). The non-streaming RESPONSE parse flavor depends on
/// this — the same wire body yields two different `LLMResponse`s — so the
/// completion composer threads it into
/// [`crate::model::response_parse::parse_for_provider_ex`].
pub fn openrouter_non_streaming_is_vision(messages: &[StreamMessage]) -> bool {
    openrouter_has_formattable_images(messages)
}

/// Does any message carry an image OpenRouter would format inline? (v4
/// `hasImageAttachments` — the vision trigger for both the streaming
/// chat-completions routing and the non-streaming vision send.)
fn openrouter_has_formattable_images(messages: &[StreamMessage]) -> bool {
    messages.iter().any(|m| {
        m.attachments().iter().any(|a| {
            let mime = a
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            OPENROUTER_IMAGE_MIME_TYPES.contains(&mime)
                && (att_str(a, "data").is_some() || att_str(a, "url").is_some())
        })
    })
}

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
    messages: &[StreamMessage],
    literal_function_type: bool,
    echo_reasoning: bool,
    user_content: &mut dyn FnMut(&StreamMessage) -> Value,
) -> Value {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            // A tool result ALWAYS pairs by id — the carrying enum requires
            // one by construction, so v4's "drop tool messages without an id"
            // backward-compat arm is unrepresentable here (the id-less case
            // became the explicit user-text fallback upstream).
            StreamMessage::Tool {
                call_id, content, ..
            } => {
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content,
                }));
            }
            StreamMessage::Assistant {
                content,
                tool_calls,
                reasoning_content,
                ..
            } if !tool_calls.is_empty() => {
                let tool_calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": if literal_function_type { "function" } else { tc.kind },
                            "function": { "name": tc.function.name, "arguments": tc.function.arguments },
                        })
                    })
                    .collect();
                let mut m = Body::new();
                m.set("role", json!("assistant"))
                    .set("content", content_or_null(content))
                    .set("tool_calls", Value::Array(tool_calls));
                if echo_reasoning {
                    if let Some(rc) = reasoning_content {
                        if !rc.is_empty() {
                            m.set("reasoning_content", json!(rc));
                        }
                    }
                }
                out.push(m.into_value());
            }
            StreamMessage::User { .. } => {
                // The user content is provider-shaped: a plain string for most,
                // a content-parts array for the vision providers (Z.AI /
                // OpenRouter) when attachments ride the message (P4.21).
                out.push(json!({ "role": "user", "content": user_content(msg) }));
            }
            _ => {
                // system / assistant (no tool calls) → {role, content}
                out.push(json!({ "role": msg.role_str(), "content": msg.content() }));
            }
        }
    }
    Value::Array(out)
}

/// The plain user-content arm (attachments stripped from the body — the
/// drop-and-report providers).
fn plain_user_content(msg: &StreamMessage) -> Value {
    json!(msg.content())
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
/// with v4's defaults, stream, and — on the streaming path only — stream_options).
/// The caller layers tools / stop / etc. on top in v4's order.
///
/// v4's DeepSeek / Z.AI `sendMessage` keeps `stream` in the same slot with `false`
/// and simply omits `stream_options`.
fn base_body(input: &RequestInput, messages: Value) -> Body {
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set("stream", json!(input.stream));
    if input.stream {
        b.set("stream_options", json!({ "include_usage": true }));
    }
    b
}

fn stop_value(input: &RequestInput) -> Option<Value> {
    input.stop.as_ref().map(|s| json!(s))
}

// ============================================================================
// OpenAI-compatible base
// ============================================================================

/// v4 `OpenAICompatibleProvider.streamMessage` / `.sendMessage`
/// (`@quilltap/plugin-utils`). The plainest chat-completions body — no tools, no
/// profile params, just `user` on a cache key.
///
/// This provider does NOT share [`base_body`]'s key order: v4 writes one object
/// literal per method, with `stop` BEFORE the stream keys, and `sendMessage`
/// carries no `stream` key at all (the OpenAI SDK's non-streaming create sends
/// the literal verbatim).
pub fn build_openai_compatible_body(
    input: &RequestInput,
    results: &mut StreamAttachmentResults,
) -> Value {
    collect_drop_failures(&input.messages, OPENAI_COMPATIBLE_ATTACHMENT_ERROR, results);
    let messages = chat_messages(&input.messages, false, false, &mut plain_user_content);
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set_opt("stop", stop_value(input));
    if input.stream {
        b.set("stream", json!(true))
            .set("stream_options", json!({ "include_usage": true }));
    }
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

/// v4 `applyProfileParameters` (`@quilltap/plugin-utils` 2.3.0, `93ed8abf`) —
/// copy allow-listed keys from the profile's free-form `parameters` bag onto a
/// request body.
///
/// Rules, all of which predate the helper (DeepSeek and Z.AI hand-rolled this
/// identical loop before v4 collapsed them onto it):
///
/// - **Allow-list, never spread.** `model` / `messages` / `stream` /
///   `stream_options` / `tools` must be unreachable from a profile — a
///   misconfigured bag cannot be allowed to retarget the request.
/// - **`null` and the empty string skip.** The empty string is what the
///   schema-driven profile editor stores for "omit this and use the model
///   default". (v4 also skips `undefined`, which JSON cannot express.)
/// - **The allow-list is ORDERED.** Keys are applied in the order given, so a
///   normalizer that merges into an earlier key (OAC's `chat_template_kwargs`)
///   sees whatever the profile set for it.
///
/// `normalize` is v4's `ProfileParamNormalizer`: it returns the value to send,
/// or `None` to omit the key entirely, and it takes the body so a value can be
/// placed under a DIFFERENT key (OAC folds `reasoning_effort` into
/// `chat_template_kwargs` that way, then omits the flat key).
fn apply_profile_params_with(
    b: &mut Body,
    input: &RequestInput,
    allowlist: &[&str],
    normalize: impl Fn(&str, &Value, &RequestInput, &mut Body) -> Option<Value>,
) {
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
        let Some(out) = normalize(key, value, input, b) else {
            continue;
        };
        b.set(key, out);
    }
}

/// v4 DeepSeek `normalizeProfileParam` (`93ed8abf`): the schema-driven editor
/// stores `thinking` as a flat string (`"enabled"` / `"disabled"`); DeepSeek's
/// wire shape is `{ type: … }`. A profile that already stored the object form
/// passes through unchanged.
fn deepseek_normalize_profile_param(key: &str, value: &Value) -> Value {
    if key == "thinking" {
        if let Some(s) = value.as_str() {
            return json!({ "type": s });
        }
    }
    value.clone()
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

pub fn build_deepseek_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    collect_drop_failures(&input.messages, DEEPSEEK_ATTACHMENT_ERROR, results);
    let messages = chat_messages(&input.messages, true, true, &mut plain_user_content);
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
    apply_profile_params_with(
        &mut b,
        input,
        DEEPSEEK_PROFILE_ALLOWLIST,
        |key, value, _, _| Some(deepseek_normalize_profile_param(key, value)),
    );
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

pub fn build_zai_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    let vision = is_zai_vision_model(&input.model);
    let messages = chat_messages(&input.messages, true, true, &mut |m| {
        zai_user_content(m, vision, results)
    });
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
    // v4 Z.AI's normalizer (`93ed8abf` moved the loop out; the reshaping stayed).
    // The `reasoning_effort` gate is the half v5 never had: v4 has skipped the
    // key on a model that does not honour it since long before the collapse, and
    // v5 forwarded it to every GLM — a pre-existing wire divergence this lane's
    // per-key hook finally has somewhere to live.
    apply_profile_params_with(
        &mut b,
        input,
        ZAI_PROFILE_ALLOWLIST,
        |key, value, inp, _| {
            if key == "reasoning_effort" && !zai_supports_reasoning_effort(&inp.model) {
                return None;
            }
            if key == "thinking" {
                if let Some(s) = value.as_str() {
                    return Some(json!({ "type": s }));
                }
            }
            Some(value.clone())
        },
    );
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

/// `profileParameters.reasoningEffort`, when it is a non-empty string.
fn openrouter_reasoning_effort(input: &RequestInput) -> Option<&str> {
    input
        .profile_parameters
        .as_ref()
        .and_then(|p| p.get("reasoningEffort"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// `profileParameters.fallbackModels`, when it is a non-empty array.
fn openrouter_fallback_models(input: &RequestInput) -> Option<&Vec<Value>> {
    input
        .profile_parameters
        .as_ref()
        .and_then(|p| p.get("fallbackModels"))
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
}

/// The OpenRouter request builders. v4's `streamMessage` and `sendMessage` take
/// **completely different code paths**, so this is not a flag flip — and the
/// non-streaming half now itself forks on whether an image rides along (v4 bug
/// 31):
///
/// - streaming (tools / vision) → [`build_openrouter_stream_body`], a raw
///   `fetch` to `/chat/completions` with a hand-built chat-completions body;
/// - non-streaming, NO image → [`build_openrouter_send_body`],
///   `@openrouter/sdk`'s `chat.send()`, whose Speakeasy-generated zod codec
///   re-emits the body in the SDK's own (camelCase-alphabetical) field order,
///   renames camelCase to snake_case, and DROPS every key its schema does not
///   declare;
/// - non-streaming, WITH a formattable image →
///   [`build_openrouter_vision_send_body`], a THIRD shape: a direct raw `fetch`
///   (bug 31's escape hatch around the SDK's content-parts rejection) whose
///   assignment order IS wire order — snake_case, raw tools passthrough,
///   assignment-order `provider`, `reasoning.exclude:false`.
pub fn build_openrouter_body(
    input: &RequestInput,
    results: &mut StreamAttachmentResults,
) -> Result<Value, BuildError> {
    openrouter_collect_attachment_results(&input.messages, results);
    if input.stream {
        Ok(build_openrouter_stream_body(input))
    } else {
        build_openrouter_send_body(input)
    }
}

fn build_openrouter_stream_body(input: &RequestInput) -> Value {
    let messages = chat_messages(
        &input.messages,
        false,
        false,
        &mut openrouter_message_content,
    );
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
    // `route: 'fallback'` rides the fallbackModels profile param (the streaming
    // raw-fetch path sends `route` but NOT `models` — v4's own asymmetry).
    if openrouter_fallback_models(input).is_some() {
        b.set("route", json!("fallback"));
    }
    // reasoning is ALWAYS set (effort from profile, else {exclude:false}).
    b.set(
        "reasoning",
        match openrouter_reasoning_effort(input) {
            Some(e) => json!({ "effort": e, "exclude": false }),
            None => json!({ "exclude": false }),
        },
    );
    b.into_value()
}

/// One message as the @openrouter/sdk re-emits it: `{content, role}` and nothing
/// else. An assistant turn's `tool_calls` are silently DROPPED (they are not in
/// the SDK's assistant-message schema) — a v4 behaviour, carried faithfully.
fn openrouter_send_message(msg: &StreamMessage) -> Value {
    let mut m = Body::new();
    if matches!(msg, StreamMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty()) {
        m.set("content", content_or_null(msg.content()));
    } else {
        m.set("content", json!(msg.content()));
    }
    m.set("role", json!(msg.role_str()));
    m.into_value()
}

/// One tool as the SDK re-emits it: `{function: {description?, name, parameters?},
/// type}`, reading v4's already-formatted `{type, function:{…}}` shape.
fn openrouter_send_tool(tool: &Value) -> Value {
    let f = tool.get("function");
    let get = |k: &str| f.and_then(|f| f.get(k)).cloned();
    let mut func = Body::new();
    func.set_opt("description", get("description"))
        .set("name", get("name").unwrap_or(Value::Null))
        .set_opt("parameters", get("parameters"));
    let mut t = Body::new();
    t.set("function", func.into_value()).set(
        "type",
        tool.get("type").cloned().unwrap_or(json!("function")),
    );
    t.into_value()
}

/// `provider` — the merged `providerPreferences` bag (plus `enableZDR` →
/// `dataCollection: 'deny'`), emitted in the SDK's schema order with its
/// snake_case names. v4 only forwards these six keys.
fn openrouter_send_provider(input: &RequestInput) -> Option<Value> {
    let profile = input.profile_parameters.as_ref()?;
    let prefs = profile.get("providerPreferences");
    let get = |k: &str| {
        prefs
            .and_then(|p| p.get(k))
            .cloned()
            .filter(|v| !v.is_null())
    };
    let zdr = profile.get("enableZDR").and_then(Value::as_bool) == Some(true);
    let data_collection = if zdr {
        Some(json!("deny"))
    } else {
        get("dataCollection")
    };

    let mut p = Body::new();
    // v4's guards: `!== undefined` for allowFallbacks, plain truthiness for the
    // rest (an empty array / empty string is skipped).
    p.set_opt("allow_fallbacks", get("allowFallbacks"));
    p.set_opt("data_collection", data_collection.filter(truthy));
    p.set_opt("ignore", get("ignore").filter(truthy));
    p.set_opt("only", get("only").filter(truthy));
    p.set_opt("order", get("order").filter(truthy));
    p.set_opt(
        "require_parameters",
        get("requireParameters").filter(truthy),
    );
    let v = p.into_value();
    if v.as_object().is_some_and(|o| o.is_empty()) {
        None
    } else {
        Some(v)
    }
}

/// JS truthiness for the values v4 guards with a bare `if (x)`: `false`, `0`,
/// `""` and empty containers are all skipped (an empty array is truthy in JS but
/// v4's `provider.order = []` would be pointless — keep JS semantics exactly:
/// only `false` / `0` / `""` / `null` are falsy).
fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// `response_format` as the SDK re-emits v4's `responseFormat`.
fn openrouter_send_response_format(input: &RequestInput) -> Option<Value> {
    let rf = input.response_format.as_ref()?;
    let ty = rf.get("type").and_then(Value::as_str)?;
    if ty == "json_schema" {
        if let Some(schema) = rf.get("jsonSchema") {
            let mut js = Body::new();
            js.set_opt("name", schema.get("name").cloned())
                .set_opt("schema", schema.get("schema").cloned())
                .set(
                    "strict",
                    schema.get("strict").cloned().unwrap_or(json!(true)),
                );
            let mut out = Body::new();
            out.set("json_schema", js.into_value())
                .set("type", json!("json_schema"));
            return Some(out.into_value());
        }
    }
    if ty == "text" {
        return None;
    }
    Some(json!({ "type": ty }))
}

/// v4 `OpenRouterProvider.sendMessage` → `@openrouter/sdk` `chat.send()`.
///
/// Key order is the SDK's `ChatRequest` schema order (its camelCase field names
/// sorted), NOT v4's assignment order, and the outbound transform renames to
/// snake_case: `maxTokens, messages, model, models, plugins, provider, reasoning,
/// responseFormat, stop, stream, temperature, toolChoice, tools, topP, user`.
/// Keys the schema does not declare are dropped — which is why v4's
/// `route: 'fallback'` and `reasoning.exclude` never reach the wire here.
fn build_openrouter_send_body(input: &RequestInput) -> Result<Value, BuildError> {
    // v4 bug 31 (`43a1b5b1`): a formattable image now routes the non-streaming
    // send AROUND the SDK to a direct `POST /chat/completions` (stream:false) —
    // the same escape hatch `streamMessage` uses — because the @openrouter/sdk
    // 1.2.2 `chat.send()` rejects OpenAI content-parts (image_url) messages at
    // client-side zod validation, so the image never reached the model. The raw
    // vision path handles content-parts AND tool-role messages fine, so it is
    // checked FIRST (a vision tool round-trip succeeds where the image-free path
    // refuses). No-image sends keep the SDK path unchanged.
    if openrouter_has_formattable_images(&input.messages) {
        return Ok(build_openrouter_vision_send_body(input));
    }
    // v4 drops tool messages without a toolCallId (unrepresentable in the
    // carrying enum — the id-less case is the user-text fallback upstream, so
    // that FILTER half is gone), then hands the rest to the SDK — where a
    // `tool`-role message fails input validation and NO request is made.
    // Reproduce the refusal rather than inventing a body v4 never sends. (The
    // image case above escapes the SDK, so the refusal is IMAGE-FREE only.)
    let messages: Vec<&StreamMessage> = input.messages.iter().collect();
    if messages
        .iter()
        .any(|m| matches!(m, StreamMessage::Tool { .. }))
    {
        return Err(BuildError::ProviderRefused(
            "OPENROUTER: the @openrouter/sdk rejects a tool-role message on the \
             non-streaming path (its schema wants toolCallId, not tool_call_id), \
             so v4 sends no request at all"
                .to_string(),
        ));
    }

    let has_tools = input.tools.as_ref().is_some_and(|t| !t.is_empty());
    let mut b = Body::new();
    b.set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set(
            "messages",
            Value::Array(
                messages
                    .iter()
                    .map(|m| openrouter_send_message(m))
                    .collect(),
            ),
        );
    // `models` REPLACES `model` when fallbacks are configured (v4 deletes it).
    match openrouter_fallback_models(input) {
        Some(fallbacks) => {
            let mut models = vec![json!(input.model)];
            models.extend(fallbacks.iter().cloned());
            b.set("models", Value::Array(models));
        }
        None => {
            b.set("model", json!(input.model));
        }
    }
    if input.web_search_enabled {
        b.set("plugins", json!([{ "id": "web", "max_results": 5 }]));
    }
    b.set_opt("provider", openrouter_send_provider(input));
    b.set(
        "reasoning",
        match openrouter_reasoning_effort(input) {
            Some(e) => json!({ "effort": e }),
            None => json!({}),
        },
    );
    b.set_opt("response_format", openrouter_send_response_format(input));
    b.set_opt("stop", stop_value(input));
    b.set("stream", json!(false))
        .set("temperature", num(input.temperature.unwrap_or(0.7)));
    if has_tools {
        b.set("tool_choice", json!("auto")).set(
            "tools",
            Value::Array(
                input
                    .tools
                    .as_ref()
                    .map(|t| t.iter().map(openrouter_send_tool).collect())
                    .unwrap_or_default(),
            ),
        );
    }
    b.set("top_p", num(input.top_p.unwrap_or(1.0)));
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("user", json!(k));
        }
    }
    Ok(b.into_value())
}

/// v4 bug 31's `OpenRouterProvider.sendViaChatCompletions` — the non-streaming
/// VISION path: a direct `POST https://openrouter.ai/api/v1/chat/completions`
/// with `stream:false`, hand-built (no SDK), so JS assignment order IS wire
/// order. Feature parity with the SDK path (cache key, tools, web search,
/// structured output, fallback models, provider preferences incl. ZDR,
/// reasoning), but the SHAPES differ from `build_openrouter_send_body`: raw
/// snake_case keys, `tools` a RAW passthrough (not the SDK reshape), `provider`
/// in v4's ASSIGNMENT order (not the SDK's alphabetical), `reasoning` keeping
/// `exclude:false`, and the `delete body.model` fallback interplay. The message
/// shape is the STREAM path's (content-parts, tool_calls kept, tool-role
/// messages carried) — the same `chat_messages(..openrouter_message_content)`
/// the streaming builder uses.
fn build_openrouter_vision_send_body(input: &RequestInput) -> Value {
    let messages = chat_messages(
        &input.messages,
        false,
        false,
        &mut openrouter_message_content,
    );
    let mut b = Body::new();
    // v4 literal: `{ model, messages, stream:false, temperature, max_tokens,
    // top_p, stop }`. `model` is deleted below when fallbacks are configured.
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("stream", json!(false))
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("max_tokens", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)));
    b.set_opt("stop", stop_value(input));
    // `user` on a non-empty cacheKey.
    if let Some(k) = &input.cache_key {
        if !k.is_empty() {
            b.set("user", json!(k));
        }
    }
    // `tools` RAW passthrough (params.tools verbatim — NOT the openrouter_tool
    // reshape the streaming raw-fetch path applies) + tool_choice.
    if input.tools.as_ref().is_some_and(|t| !t.is_empty()) {
        b.set(
            "tools",
            Value::Array(input.tools.clone().unwrap_or_default()),
        )
        .set("tool_choice", json!("auto"));
    }
    if input.web_search_enabled {
        b.set("plugins", json!([{ "id": "web", "max_results": 5 }]));
    }
    b.set_opt("response_format", openrouter_vision_response_format(input));
    // fallbackModels: `models` REPLACES `model` (v4 sets models + route then
    // `delete body.model`; the order-preserving remove keeps models/route in
    // their assignment slot).
    if let Some(fallbacks) = openrouter_fallback_models(input) {
        let mut models = vec![json!(input.model)];
        models.extend(fallbacks.iter().cloned());
        b.set("models", Value::Array(models))
            .set("route", json!("fallback"));
        b.remove("model");
    }
    b.set_opt("provider", openrouter_vision_send_provider(input));
    // reasoning is ALWAYS set and KEEPS `exclude:false` (raw fetch, no SDK
    // schema to drop it).
    b.set(
        "reasoning",
        match openrouter_reasoning_effort(input) {
            Some(e) => json!({ "effort": e, "exclude": false }),
            None => json!({ "exclude": false }),
        },
    );
    b.into_value()
}

/// v4 `resolveProviderPrefs`: the merged `providerPreferences` map (plus
/// `enableZDR` → `dataCollection:'deny'`), or `None` when the merge is empty
/// (v4 returns undefined and emits no `provider` key at all).
fn openrouter_resolve_provider_prefs(
    input: &RequestInput,
) -> Option<serde_json::Map<String, Value>> {
    let profile = input.profile_parameters.as_ref()?;
    let mut merged = profile
        .get("providerPreferences")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if profile.get("enableZDR").and_then(Value::as_bool) == Some(true) {
        merged.insert("dataCollection".to_string(), json!("deny"));
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

/// `provider` for the raw vision path, in v4's ASSIGNMENT order (`order,
/// allow_fallbacks, require_parameters, data_collection, ignore, only`) — NOT
/// the SDK's alphabetical order. v4 emits `body.provider = {}` whenever
/// `resolveProviderPrefs` is truthy, then adds the guarded keys, so an empty
/// object is possible when every guard skips (unreachable by any realistic
/// vector, carried faithfully).
fn openrouter_vision_send_provider(input: &RequestInput) -> Option<Value> {
    let merged = openrouter_resolve_provider_prefs(input)?;
    let mut p = Body::new();
    // `order` — truthiness guard.
    if let Some(v) = merged.get("order").filter(|v| truthy(v)) {
        p.set("order", v.clone());
    }
    // `allowFallbacks` — v4's `!== undefined`: present at all (even `false`).
    if let Some(v) = merged.get("allowFallbacks") {
        p.set("allow_fallbacks", v.clone());
    }
    if let Some(v) = merged.get("requireParameters").filter(|v| truthy(v)) {
        p.set("require_parameters", v.clone());
    }
    if let Some(v) = merged.get("dataCollection").filter(|v| truthy(v)) {
        p.set("data_collection", v.clone());
    }
    if let Some(v) = merged.get("ignore").filter(|v| truthy(v)) {
        p.set("ignore", v.clone());
    }
    if let Some(v) = merged.get("only").filter(|v| truthy(v)) {
        p.set("only", v.clone());
    }
    Some(p.into_value())
}

/// `response_format` for the raw vision path, in v4's ASSIGNMENT order (`type`
/// first, then `json_schema:{name, strict, schema}`) — NOT the SDK's schema
/// order. Same branch logic as [`openrouter_send_response_format`].
fn openrouter_vision_response_format(input: &RequestInput) -> Option<Value> {
    let rf = input.response_format.as_ref()?;
    let ty = rf.get("type").and_then(Value::as_str)?;
    if ty == "json_schema" {
        if let Some(schema) = rf.get("jsonSchema") {
            let mut js = Body::new();
            // v4's JSON.stringify DROPS an undefined `name` — mirror the SDK-path
            // builder's set_opt rather than emitting "name": null.
            js.set_opt("name", schema.get("name").cloned())
                .set(
                    "strict",
                    schema.get("strict").cloned().unwrap_or(json!(true)),
                )
                .set_opt("schema", schema.get("schema").cloned());
            let mut out = Body::new();
            out.set("type", json!("json_schema"))
                .set("json_schema", js.into_value());
            return Some(out.into_value());
        }
    }
    if ty == "text" {
        return None;
    }
    Some(json!({ "type": ty }))
}

// ============================================================================
// Ollama (raw fetch to /api/chat)
// ============================================================================

/// v4 `resolveEnableThinking` (`plugins/dist/qtap-plugin-ollama/provider.ts`,
/// `d9c5a1c7`): the profile's `enable_thinking` option, defaulting to false —
/// thinking models answer without a reasoning pass, keeping output clean for
/// JSON-shaped work. The string form is tolerated in case an older profile
/// stored one; EVERY other value (including a truthy non-`"true"` string) is
/// false.
pub fn ollama_enable_thinking(profile_parameters: Option<&Value>) -> bool {
    match profile_parameters.and_then(|p| p.get("enable_thinking")) {
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
}

/// v4 `resolveNumCtx`: the context window to request via `options.num_ctx`. The
/// host injects the profile's Max Context into `profileParameters.num_ctx` for
/// Ollama profiles; without it the server allocates its own default and
/// silently truncates longer prompts. `None` leaves the field off the wire —
/// the server/Modelfile default applies, exactly as before.
///
/// v4 coerces ONLY the number and string arms (`typeof value === 'number' ?
/// value : typeof value === 'string' ? Number(value) : NaN`), so a boolean or
/// `null` is NaN here rather than JS `ToNumber`'s 1/0 — reproduced literally.
pub fn ollama_num_ctx(profile_parameters: Option<&Value>) -> Option<f64> {
    let n = match profile_parameters.and_then(|p| p.get("num_ctx")) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => crate::jsnum::number_from_str(s),
        _ => f64::NAN,
    };
    if n.is_finite() && n > 0.0 {
        Some(n.floor())
    } else {
        None
    }
}

pub fn build_ollama_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    collect_drop_failures(&input.messages, OLLAMA_ATTACHMENT_ERROR, results);
    let messages = chat_messages(&input.messages, false, false, &mut plain_user_content);
    let mut options = Body::new();
    options
        .set("temperature", num(input.temperature.unwrap_or(0.7)))
        .set("num_predict", num(input.max_tokens.unwrap_or(4096) as f64))
        .set("top_p", num(input.top_p.unwrap_or(1.0)))
        .set_opt("stop", stop_value(input))
        // Context window from the profile's Max Context (undefined keys are
        // dropped by JSON.stringify, leaving the server default in charge).
        .set_opt(
            "num_ctx",
            ollama_num_ctx(input.profile_parameters.as_ref()).map(num),
        );
    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", messages)
        .set("stream", json!(input.stream))
        // Ollama's native thinking switch (0.9+; older servers ignore unknown
        // fields). ALWAYS present — `think: false` when off. When off,
        // thinking-capable models answer directly; when on, Ollama returns the
        // reasoning separately as `message.thinking`.
        .set(
            "think",
            json!(ollama_enable_thinking(input.profile_parameters.as_ref())),
        )
        .set("options", options.into_value());
    if let Some(t) = &input.tools {
        if !t.is_empty() {
            b.set("tools", json!(t));
        }
    }
    b.into_value()
}
