//! The Anthropic request builder + transform (W4.7c part 2). Ported from the
//! anthropic plugin's `streamMessage` + `formatMessagesWithAttachments` +
//! `applyMidHistoryBreakpoint`. The transform is the cache-breakpoint hierarchy
//! (tools → system → messages), the consecutive-tool-result batching, the
//! empty-message skip, and the adaptive-thinking rule for Sonnet 5 / Opus 5 /
//! Opus 4.7+ / Fable / Mythos.
//!
//! ## API version `2026-10-21` (P4.D364, v4 `544c7ab1e`)
//!
//! v4's plugin dropped its `2023-06-01` pin and takes `@anthropic-ai/sdk` 1.x's
//! default, `2026-10-21`. That version refuses `temperature`, `top_p` and
//! `top_k` on EVERY model and refuses a `document` block placed directly in an
//! assistant message. So:
//!
//! - **No sampling key is ever sent** — on any model, thinking on or off (the
//!   old `temperature` / `top_p` / `temperature: 1.0` block is gone).
//! - **The header** is the manifest's `auth.extra` (`anthropic-version:
//!   2026-10-21`), not this builder's; it must land with this builder (a v5 that
//!   sent the new version WITH sampling keys would 400 every turn).
//! - **The empty-message skip** — on the attachment path, a message of ANY role
//!   whose every attachment failed and whose text is empty is DROPPED (v4
//!   `provider.ts:373-377` at `624d0d25c`); the failures stay recorded.
//! - **The assistant-document relocation is a NAMED divergence,
//!   `ASSISTANT_ATTACHMENTS_UNREPRESENTABLE`** (P4.D364 R-P1). v4 moves an
//!   assistant turn's `document` blocks into a NEW `{role:'user', content:
//!   documents}` message just before it (DEBUG `Moving assistant-message
//!   documents into a preceding user turn` `{context:
//!   'AnthropicProvider.formatMessagesWithAttachments', documentCount}`), the
//!   assistant keeping its text / image blocks, and drops an assistant left with
//!   nothing. v5's carrier ([`StreamMessage::Assistant`]) holds no attachments
//!   and v4's production never hands the plugin one (the context builder stamps
//!   attachments on the user-turn anchor only), so there is nothing to move;
//!   `request_builder_equivalence` records the three corpus rows by name.
//!
//! ## The budget-thinking-rejected model list
//!
//! Ported as a **compiled constant** ([`BUDGET_THINKING_REJECTED_MODELS`]), NOT
//! lifted into the manifest. The rules are *prefix regexes* over stable model
//! aliases (matching future dated snapshots), i.e. compiled matching behavior
//! rather than per-model data; the W4.7a manifest carries only an exact
//! `fallbackModels: Vec<String>`, so a clean lift would need a NEW manifest field
//! — a manifest-schema extension deferred as a follow-up. Keeping the list in
//! this hook matches v4's structure. Until `544c7ab1e` it was
//! named for sampling params and gated the sampling keys too; it now
//! gates ONLY the thinking shape (adaptive vs a fixed budget). Opus 5 joined it
//! late (v4 `48f4b42ec`, 2026-09-04) — the plugin lists models live through
//! `client.models.list()`, so it was selectable before the list knew it.

use serde_json::{json, Value};
use std::sync::LazyLock;

use regex::Regex;

use super::{
    att_fail, att_id, att_str, num, Body, RequestInput, StreamAttachmentResults, StreamMessage,
};

/// v4 `BUDGET_THINKING_REJECTED_MODELS` (`624d0d25c`): "Models that reject
/// fixed-budget thinking (`thinking.type: "enabled"` with `budget_tokens`
/// returns 400) and take adaptive thinking instead: Claude Sonnet 5, Claude
/// Opus 5, the Opus 4.7/4.8 family, Fable and Mythos. Matched by prefix since
/// these are stable aliases (no dated snapshots). Older models (Haiku 4.5,
/// Sonnet 4.5, ...) still need `budget_tokens`." Order mirrors v4's array.
static BUDGET_THINKING_REJECTED_MODELS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^claude-sonnet-5(-|$)",
        r"^claude-opus-5(-|$)",
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

fn model_rejects_budget_thinking(model: &str) -> bool {
    BUDGET_THINKING_REJECTED_MODELS
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

/// v4's text/plain document data. The newline/charset pre-guard is UNCHANGED
/// (only base64-charset, newline-free, non-empty data is a decode candidate);
/// v4 bug 34 (`43a1b5b1`) replaced the inner bare decode with the round-trip
/// `decodeBase64Text` ([`super::responses_api::decode_base64_text`]), so
/// base64-LOOKING-but-not data now ships VERBATIM instead of mangled: `"hello"`
/// stays `"hello"` (was `"\u{FFFD}\u{FFFD}e"`), `"x=1"` stays `"x=1"` (was
/// `""`), while genuine base64 still decodes. Anything failing the pre-guard
/// ships as-is.
fn anthropic_text_document_data(data: &str) -> String {
    let looks_base64 = !data.contains('\n')
        && !data.is_empty()
        && data
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    if !looks_base64 {
        return data.to_string();
    }
    super::responses_api::decode_base64_text(data)
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
        // v4 (`544c7ab1e`): "An assistant turn that carried only documents has
        // nothing left to say once they move; an empty assistant message would
        // be refused." The guard is ROLE-INDEPENDENT — a user message whose
        // every attachment failed and whose text is empty is dropped too (the
        // failures above stay recorded). It runs after the cache-control step,
        // as v4's does, so a dropped last user message carries no breakpoint.
        if blocks.is_empty() && msg.content().is_empty() {
            i += 1;
            continue;
        }
        // v4: `content: content.length > 0 ? content : msg.content` — every
        // attachment failing on a message WITH text falls back to its string.
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
    // Computed once per call, as v4's `budgetThinkingRejected` is (in both
    // `sendMessage` and `streamMessage`); it picks the thinking shape only.
    let budget_thinking_rejected = model_rejects_budget_thinking(&input.model);

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
            if budget_thinking_rejected {
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

    // No sampling parameters (v4 `544c7ab1e`): API version 2026-10-21 refuses
    // `temperature`, `top_p` and `top_k` on every model, so the plugin sends
    // none — `input.temperature` / `input.top_p` are ignored here, thinking on
    // or off. (The old block sent `temperature`, else `top_p`, else a
    // `temperature: 1.0` default.)

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

/// P4.D364 — the shape of v4's own `anthropic-model-family-params.test.ts` (at
/// `624d0d25c`): sampling params are never sent, thinking shape by generation,
/// and the empty-message skip.
#[cfg(test)]
mod tests {
    use super::*;

    fn input(model: &str) -> RequestInput {
        RequestInput {
            model: model.to_string(),
            messages: vec![
                StreamMessage::system("You are a helpful assistant."),
                StreamMessage::user("Hello there."),
            ],
            max_tokens: Some(1000),
            stream: true,
            ..Default::default()
        }
    }

    fn body(i: &RequestInput) -> Value {
        build_body(i, &mut StreamAttachmentResults::default())
    }

    fn assistant(content: &str) -> StreamMessage {
        StreamMessage::Assistant {
            content: content.to_string(),
            tool_calls: Vec::new(),
            reasoning_content: None,
            thought_signature: None,
            cache_control: None,
            name: None,
        }
    }

    fn user_with(content: &str, attachment: Value) -> StreamMessage {
        StreamMessage::User {
            content: content.to_string(),
            cache_control: None,
            attachments: vec![attachment],
            name: None,
        }
    }

    fn tiff() -> Value {
        json!({ "id": "att-tiff-1", "filename": "scan.tiff", "mimeType": "image/tiff", "size": 9, "data": "AAAA" })
    }

    #[test]
    fn sampling_params_are_never_sent() {
        for model in ["claude-opus-4-6", "claude-sonnet-5", "claude-haiku-4-5"] {
            for stream in [true, false] {
                for (t, p) in [(Some(0.5), Some(0.9)), (None, Some(0.9)), (None, None)] {
                    let mut i = input(model);
                    i.stream = stream;
                    i.temperature = t;
                    i.top_p = p;
                    let b = body(&i);
                    for key in ["temperature", "top_p", "top_k"] {
                        assert!(
                            b.get(key).is_none(),
                            "{model} stream={stream} sent {key}: {b}"
                        );
                    }
                }
                // Thinking on too.
                let mut i = input(model);
                i.stream = stream;
                i.temperature = Some(0.5);
                i.profile_parameters = Some(json!({ "thinkingBudget": 2048 }));
                let b = body(&i);
                assert!(b.get("temperature").is_none() && b.get("top_p").is_none());
            }
        }
    }

    #[test]
    fn thinking_shape_by_generation() {
        let thinking = |model: &str| {
            let mut i = input(model);
            i.profile_parameters = Some(json!({ "thinkingBudget": 2048 }));
            body(&i)["thinking"].clone()
        };
        let adaptive = json!({ "type": "adaptive", "display": "summarized" });
        assert_eq!(thinking("claude-sonnet-5"), adaptive);
        assert_eq!(
            thinking("claude-opus-4-8-20260215"),
            adaptive,
            "a dated snapshot is new-gen"
        );
        assert_eq!(
            thinking("claude-opus-4-6"),
            json!({ "type": "enabled", "budget_tokens": 2048 }),
            "opus-4-6 is NOT new-gen"
        );
    }

    #[test]
    fn an_empty_message_whose_attachments_all_failed_is_dropped() {
        let mut i = input("claude-opus-4-6");
        i.messages = vec![
            StreamMessage::system("sys"),
            StreamMessage::user("Hello there."),
            assistant("Hi there."),
            user_with("", tiff()),
            StreamMessage::user("Again."),
        ];
        let mut results = StreamAttachmentResults::default();
        let b = build_body(&i, &mut results);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3, "the empty failing message is absent: {b}");
        assert_eq!(msgs[2]["content"], json!("Again."));
        assert!(results.sent.is_empty());
        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, "att-tiff-1");
    }

    /// v4 pushes the text block BEFORE the attachment loop, so a message with
    /// text whose attachment fails is still sent — as its one text block (the
    /// corpus `unsupported-attachment` row, byte-pinned).
    #[test]
    fn a_message_with_text_and_a_failing_attachment_is_still_sent() {
        let mut i = input("claude-opus-4-6");
        i.messages = vec![
            StreamMessage::system("sys"),
            user_with("What is this?", tiff()),
        ];
        let mut results = StreamAttachmentResults::default();
        let b = build_body(&i, &mut results);
        assert_eq!(
            b["messages"],
            json!([{ "role": "user", "content": [{ "type": "text", "text": "What is this?" }] }])
        );
        assert_eq!(results.failed.len(), 1);
    }
}
