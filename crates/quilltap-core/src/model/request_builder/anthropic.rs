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

use super::{num, Body, RequestInput, RequestMessage};

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

/// The non-system, tool-id-filtered messages (v4's `nonSystemMessages`).
fn non_system(messages: &[RequestMessage]) -> Vec<&RequestMessage> {
    messages
        .iter()
        .filter(|m| {
            if m.role == "system" {
                return false;
            }
            if m.role == "tool" && m.tool_call_id.is_none() {
                return false;
            }
            true
        })
        .collect()
}

/// v4 `formatMessagesWithAttachments` (the text path — attachments are the file
/// subsystem's concern). Batches consecutive tool results, expands assistant
/// tool-calls into content blocks, and attaches `cache_control` to the last user
/// message (system_and_long_context) or a caller-flagged message.
fn format_messages(messages: &[RequestMessage], cache: Option<&CacheOpts>) -> Vec<Value> {
    let non = non_system(messages);
    let last_user_index: Option<usize> =
        if cache.is_some_and(|c| c.strategy == "system_and_long_context") {
            non.iter().rposition(|m| m.role == "user")
        } else {
            None
        };
    let ttl = cache.and_then(|c| c.ttl.as_deref());

    let mut out = Vec::new();
    let mut i = 0;
    while i < non.len() {
        let msg = non[i];

        if msg.role == "tool" {
            let mut blocks = vec![json!({
                "type": "tool_result",
                "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                "content": msg.content,
            })];
            while i + 1 < non.len() && non[i + 1].role == "tool" {
                i += 1;
                let next = non[i];
                blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": next.tool_call_id.clone().unwrap_or_default(),
                    "content": next.content,
                }));
            }
            out.push(json!({ "role": "user", "content": blocks }));
            i += 1;
            continue;
        }

        if msg.role == "assistant" && !msg.tool_calls.is_empty() {
            let mut content = Vec::new();
            if !msg.content.is_empty() {
                content.push(json!({ "type": "text", "text": msg.content }));
            }
            for tc in &msg.tool_calls {
                let input: Value =
                    serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
                content.push(
                    json!({ "type": "tool_use", "id": tc.id, "name": tc.name, "input": input }),
                );
            }
            out.push(json!({ "role": "assistant", "content": content }));
            i += 1;
            continue;
        }

        let role = if msg.role == "user" {
            "user"
        } else {
            "assistant"
        };
        let is_last_user = Some(i) == last_user_index;
        let honor_msg_cc = cache.is_some()
            && msg
                .cache_control
                .as_ref()
                .and_then(|c| c.get("type"))
                .and_then(Value::as_str)
                == Some("ephemeral");
        if is_last_user || honor_msg_cc {
            out.push(json!({
                "role": role,
                "content": [{ "type": "text", "text": msg.content, "cache_control": cache_control(ttl) }],
            }));
        } else {
            out.push(json!({ "role": role, "content": msg.content }));
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

pub fn build_body(input: &RequestInput) -> Value {
    // System messages (string, non-empty).
    let system_messages: Vec<&RequestMessage> = input
        .messages
        .iter()
        .filter(|m| m.role == "system" && !m.content.is_empty())
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
    let mut messages = format_messages(&input.messages, cache_opts.as_ref());
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
        .set("max_tokens", num(effective_max as f64))
        .set("stream", json!(true));

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
                        .set("text", json!(m.content));
                    if i == 0 {
                        block.set("cache_control", cache_control(cache_ttl.as_deref()));
                    }
                    block.into_value()
                })
                .collect();
            b.set("system", Value::Array(blocks));
        } else if system_messages.len() == 1 {
            b.set("system", json!(system_messages[0].content));
        } else {
            let blocks: Vec<Value> = system_messages
                .iter()
                .map(|m| json!({ "type": "text", "text": m.content }))
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
