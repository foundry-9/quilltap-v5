//! The Anthropic request builder + transform (W4.7c part 2). Ported from the
//! anthropic plugin's `streamMessage` + `formatMessagesWithAttachments` +
//! `applyMidHistoryBreakpoint`. The transform is the cache-breakpoint hierarchy
//! (tools → system → messages), the consecutive-tool-result batching, and the
//! adaptive-thinking / sampling-param-rejection rules for Sonnet 5 / Opus 4.7+ /
//! Fable / Mythos.
//!
//! ## The sampling-param-rejected model list
//!
//! Ported as a **compiled constant** ([`SAMPLING_PARAMS_REJECTED_MODELS`]), NOT
//! lifted into the manifest. The rules are *prefix regexes* over stable model
//! aliases (matching future dated snapshots), i.e. compiled matching behavior
//! rather than per-model data; the W4.7a manifest carries only an exact
//! `fallbackModels: Vec<String>`, so a clean lift would need a NEW manifest field
//! (e.g. `samplingRejectedModelPrefixes`) — a manifest-schema extension deferred
//! as a follow-up. Keeping the list in this hook matches v4's structure (the rules
//! live in the anthropic plugin's `requestTransform`). Ported from CURRENT v4
//! (`6b6e39ad`): Sonnet 5, Opus 4.7 / 4.8, Fable 5, Mythos 5 / preview.

use serde_json::{json, Value};
use std::sync::LazyLock;

use regex::Regex;

use super::{
    att_fail, att_id, att_str, num, Body, RequestInput, StreamAttachmentResults, StreamMessage,
};

/// v4 `SAMPLING_PARAMS_REJECTED_MODELS` (current source): these models reject
/// `temperature`/`top_p`/`top_k` outright and 400 on fixed-budget thinking.
static SAMPLING_PARAMS_REJECTED_MODELS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^claude-sonnet-5(-|$)",
        r"^claude-opus-4-7(-|$)",
        r"^claude-opus-4-8(-|$)",
        r"^claude-fable-5(-|$)",
        r"^claude-mythos-5(-|$)",
        r"^claude-mythos-preview(-|$)",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid rejected-model regex"))
    .collect()
});

fn model_rejects_sampling_params(model: &str) -> bool {
    SAMPLING_PARAMS_REJECTED_MODELS
        .iter()
        .any(|re| re.is_match(model))
}

/// `buildCacheControl(ttl)` — `{type:ephemeral}` (5m default) or `{type:ephemeral,
/// ttl:'1h'}`.
fn cache_control(ttl: Option<&str>) -> Value {
    if ttl == Some("1h") {
        json!({ "type": "ephemeral", "ttl": "1h" })
    } else {
        json!({ "type": "ephemeral" })
    }
}

struct CacheOpts {
    strategy: String,
    ttl: Option<String>,
}

/// The non-system messages (v4's `nonSystemMessages`). v4 also filtered
/// tool messages without a `toolCallId` here — that arm is unrepresentable in
/// the carrying enum (the id-less case became the user-text fallback upstream),
/// so the old `tool_use_id: ""` MALFORMED emission below it is gone with it.
fn non_system(messages: &[StreamMessage]) -> Vec<&StreamMessage> {
    messages
        .iter()
        .filter(|m| !matches!(m, StreamMessage::System { .. }))
        .collect()
}

/// v4's supported attachment mime types (`ANTHROPIC_SUPPORTED_MIME_TYPES`).
const ANTHROPIC_SUPPORTED_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "application/pdf",
    "text/plain",
];

/// v4's text/plain document data: base64-LOOKING data (no newline, only the
/// base64 charset) is decoded to text first; anything else — and any decode
/// failure — ships as-is. `toString('utf-8')` maps invalid sequences to
/// replacement chars, hence the lossy conversion.
fn anthropic_text_document_data(data: &str) -> String {
    let looks_base64 = !data.contains('\n')
        && !data.is_empty()
        && data
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    if !looks_base64 {
        return data.to_string();
    }
    match super::responses_api::forgiving_base64(data) {
        Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        None => data.to_string(),
    }
}

/// v4 `formatMessagesWithAttachments`. Batches consecutive tool results, expands
/// assistant tool-calls into content blocks, formats a user message's
/// attachments into image/document blocks (collecting sent/failed), and attaches
/// `cache_control` to the last user message (system_and_long_context) or a
/// caller-flagged message — on the LAST content block when the message is
/// multimodal.
fn format_messages(
    messages: &[StreamMessage],
    cache: Option<&CacheOpts>,
    results: &mut StreamAttachmentResults,
) -> Vec<Value> {
    let non = non_system(messages);
    let last_user_index: Option<usize> =
        if cache.is_some_and(|c| c.strategy == "system_and_long_context") {
            non.iter()
                .rposition(|m| matches!(m, StreamMessage::User { .. }))
        } else {
            None
        };
    let ttl = cache.and_then(|c| c.ttl.as_deref());

    let mut out = Vec::new();
    let mut i = 0;
    while i < non.len() {
        let msg = non[i];

        if let StreamMessage::Tool {
            call_id, content, ..
        } = msg
        {
            let mut blocks = vec![json!({
                "type": "tool_result",
                "tool_use_id": call_id,
                "content": content,
            })];
            while i + 1 < non.len() {
                let StreamMessage::Tool {
                    call_id, content, ..
                } = non[i + 1]
                else {
                    break;
                };
                i += 1;
                blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                }));
            }
            out.push(json!({ "role": "user", "content": blocks }));
            i += 1;
            continue;
        }

        if let StreamMessage::Assistant {
            content,
            tool_calls,
            ..
        } = msg
        {
            if !tool_calls.is_empty() {
                let mut blocks = Vec::new();
                if !content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": content }));
                }
                for tc in tool_calls {
                    let input: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
                    blocks.push(
                        json!({ "type": "tool_use", "id": tc.id, "name": tc.function.name, "input": input }),
                    );
                }
                out.push(json!({ "role": "assistant", "content": blocks }));
                i += 1;
                continue;
            }
        }

        let role = if matches!(msg, StreamMessage::User { .. }) {
            "user"
        } else {
            "assistant"
        };
        let is_last_user = Some(i) == last_user_index;
        let honor_msg_cc = cache.is_some()
            && msg
                .cache_control()
                .and_then(|c| c.get("type"))
                .and_then(Value::as_str)
                == Some("ephemeral");

        // No attachments → v4's plain string / single-cached-text-block path.
        let atts = msg.attachments();
        if atts.is_empty() {
            if is_last_user || honor_msg_cc {
                out.push(json!({
                    "role": role,
                    "content": [{ "type": "text", "text": msg.content(), "cache_control": cache_control(ttl) }],
                }));
            } else {
                out.push(json!({ "role": role, "content": msg.content() }));
            }
            i += 1;
            continue;
        }

        // Multimodal content array: text first, then each surviving attachment
        // as its block (PDF → base64 document, text/plain → text document with
        // the base64-decode heuristic, else image).
        let mut blocks: Vec<Value> = Vec::new();
        if !msg.content().is_empty() {
            blocks.push(json!({ "type": "text", "text": msg.content() }));
        }
        for a in atts {
            let mime = a
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !ANTHROPIC_SUPPORTED_MIME_TYPES.contains(&mime) {
                att_fail(
                    results,
                    a,
                    format!(
                        "Unsupported file type: {mime}. Anthropic supports: {}",
                        ANTHROPIC_SUPPORTED_MIME_TYPES.join(", ")
                    ),
                );
                continue;
            }
            let Some(data) = att_str(a, "data") else {
                att_fail(results, a, "File data not loaded");
                continue;
            };
            if mime == "application/pdf" {
                blocks.push(json!({
                    "type": "document",
                    "source": { "type": "base64", "media_type": mime, "data": data },
                }));
            } else if mime == "text/plain" {
                blocks.push(json!({
                    "type": "document",
                    "source": { "type": "text", "media_type": "text/plain", "data": anthropic_text_document_data(data) },
                }));
            } else {
                blocks.push(json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": mime, "data": data },
                }));
            }
            results.sent.push(att_id(a));
        }

        // The breakpoint rides the LAST block (image included) — v4 spreads
        // cache_control onto it, guarded on a non-empty array.
        if (is_last_user || honor_msg_cc) && !blocks.is_empty() {
            if let Some(Value::Object(o)) = blocks.last_mut() {
                o.insert("cache_control".to_string(), cache_control(ttl));
            }
        }
        // v4: `content: content.length > 0 ? content : msg.content` — every
        // attachment failing on an empty-content message falls back to the
        // plain (empty) string.
        if blocks.is_empty() {
            out.push(json!({ "role": role, "content": msg.content() }));
        } else {
            out.push(json!({ "role": role, "content": blocks }));
        }
        i += 1;
    }
    out
}

/// v4 `applyMidHistoryBreakpoint` — a second cache breakpoint stepped by K=15 once
/// the history is long enough (≥20 messages). A no-op below 20.
fn apply_mid_history_breakpoint(messages: &mut [Value], ttl: Option<&str>) {
    const MIN: usize = 20;
    const STEP: usize = 15;
    if messages.len() < MIN {
        return;
    }
    let raw_index = messages.len() - STEP;
    let stepped = (raw_index / STEP) * STEP;
    if stepped >= messages.len() {
        return;
    }
    let target = &messages[stepped];
    let role = target
        .get("role")
        .cloned()
        .unwrap_or(Value::String("user".into()));
    match target.get("content") {
        Some(Value::String(s)) => {
            messages[stepped] = json!({
                "role": role,
                "content": [{ "type": "text", "text": s, "cache_control": cache_control(ttl) }],
            });
        }
        Some(Value::Array(blocks)) if !blocks.is_empty() => {
            let mut updated = blocks.clone();
            let last = updated.len() - 1;
            if let Value::Object(ref mut o) = updated[last] {
                o.insert("cache_control".to_string(), cache_control(ttl));
            }
            messages[stepped] = json!({ "role": role, "content": updated });
        }
        _ => {}
    }
}

pub fn build_body(input: &RequestInput, results: &mut StreamAttachmentResults) -> Value {
    // System messages (string, non-empty).
    let system_messages: Vec<&StreamMessage> = input
        .messages
        .iter()
        .filter(|m| matches!(m, StreamMessage::System { .. }) && !m.content().is_empty())
        .collect();

    let profile = input.profile_parameters.as_ref();
    let caching_enabled = profile
        .and_then(|p| p.get("enableCacheBreakpoints"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cache_strategy = profile
        .and_then(|p| p.get("cacheStrategy"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("system_and_long_context")
        .to_string();
    let cache_ttl = profile
        .and_then(|p| p.get("cacheTTL"))
        .and_then(Value::as_str)
        .map(str::to_string);

    // Thinking budget (raw ≥1024, else extendedThinking → 4096, else 0).
    let raw_budget = profile
        .and_then(|p| p.get("thinkingBudget"))
        .and_then(Value::as_f64);
    let thinking_budget = match raw_budget {
        Some(b) if b >= 1024.0 => b as i64,
        _ => {
            if profile
                .and_then(|p| p.get("extendedThinking"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                4096
            } else {
                0
            }
        }
    };
    let thinking_enabled = thinking_budget > 0;
    let sampling_rejected = model_rejects_sampling_params(&input.model);

    let cache_opts = if caching_enabled {
        Some(CacheOpts {
            strategy: cache_strategy,
            ttl: cache_ttl.clone(),
        })
    } else {
        None
    };
    let mut messages = format_messages(&input.messages, cache_opts.as_ref(), results);
    if caching_enabled {
        apply_mid_history_breakpoint(&mut messages, cache_ttl.as_deref());
    }

    let base_max = input.max_tokens.unwrap_or(4096);
    let effective_max = if thinking_enabled {
        base_max.max(thinking_budget + 1024)
    } else {
        base_max
    };

    let mut b = Body::new();
    b.set("model", json!(input.model))
        .set("messages", Value::Array(messages))
        .set("max_tokens", num(effective_max as f64));
    // v4's `sendMessage` builds the SAME literal MINUS the `stream` key — it does
    // not send `stream: false` (the Anthropic SDK carries streaming in its own
    // request options, not the body). Everything after this slot is shared.
    if input.stream {
        b.set("stream", json!(true));
    }

    if thinking_enabled {
        b.set(
            "thinking",
            if sampling_rejected {
                json!({ "type": "adaptive", "display": "summarized" })
            } else {
                json!({ "type": "enabled", "budget_tokens": thinking_budget })
            },
        );
    }

    if !system_messages.is_empty() {
        if caching_enabled {
            let blocks: Vec<Value> = system_messages
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let mut block = Body::new();
                    block
                        .set("type", json!("text"))
                        .set("text", json!(m.content()));
                    if i == 0 {
                        block.set("cache_control", cache_control(cache_ttl.as_deref()));
                    }
                    block.into_value()
                })
                .collect();
            b.set("system", Value::Array(blocks));
        } else if system_messages.len() == 1 {
            b.set("system", json!(system_messages[0].content()));
        } else {
            let blocks: Vec<Value> = system_messages
                .iter()
                .map(|m| json!({ "type": "text", "text": m.content() }))
                .collect();
            b.set("system", Value::Array(blocks));
        }
    }

    // Sampling params: only when thinking off AND not a rejected model.
    if !thinking_enabled && !sampling_rejected {
        if let Some(t) = input.temperature {
            b.set("temperature", num(t));
        } else if let Some(p) = input.top_p {
            b.set("top_p", num(p));
        } else {
            b.set("temperature", num(1.0));
        }
    }

    // Tools (cache_control on the last tool when caching).
    if let Some(tools) = &input.tools {
        if !tools.is_empty() {
            let mut tools = tools.clone();
            if caching_enabled {
                let last = tools.len() - 1;
                if let Value::Object(ref mut o) = tools[last] {
                    o.insert(
                        "cache_control".to_string(),
                        cache_control(cache_ttl.as_deref()),
                    );
                }
            }
            b.set("tools", Value::Array(tools));
        }
    }

    // Stop sequences (filter falsy, cap at 4).
    if let Some(stop) = &input.stop {
        let arr: Vec<&String> = stop.iter().filter(|s| !s.is_empty()).collect();
        if !arr.is_empty() {
            let capped: Vec<Value> = arr.iter().take(4).map(|s| json!(s)).collect();
            b.set("stop_sequences", Value::Array(capped));
        }
    }

    b.into_value()
}
