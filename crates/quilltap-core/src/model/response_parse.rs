//! Non-streaming response parsers (W4.7d) — the sans-IO counterpart to the W4.7b
//! stream decoders. Each provider's `sendMessage(params, apiKey)` returns an
//! [`LLMResponse`](https://…) shape; this module turns the provider's
//! non-streaming JSON response body into a [`NonStreamingResponse`] value (no IO).
//! The request side already exists ([`crate::model::request_builder`]); the
//! transport (W4.7d) feeds the response body here.
//!
//! ## Wire families (v4 plugin `sendMessage` parse, byte-faithful)
//!
//!   - **chat-completions JSON** ([`parse_chat_completions`], flavored per the
//!     W4.7b [`ChatFlavor`] split): `OpenAiCompatible` (the base — content /
//!     finishReason / usage / raw only), `DeepSeek` (+ `reasoning_content`, tool
//!     calls, and the `prompt_cache_hit_tokens` cache-read subtraction), `ZAi`
//!     (+ reasoning / tools / `prompt_tokens_details.cached_tokens` subtraction),
//!     `OpenRouter` (the SNAKE_CASE wire normalized through the reproduced
//!     @openrouter/sdk inbound zod — declared keys, camelCase renames, schema
//!     order — then read as v4 reads the SDK object: `finishReason`,
//!     `promptTokens`, `message.reasoning`; `usage.cachedTokens` is a key the
//!     SDK never materializes, so cacheUsage stays absent — a v4 quirk the
//!     recorded-body corpus pins).
//!   - **responses-API JSON** ([`parse_responses_api`], OpenAI / Grok):
//!     `output_text`, reasoning summaries, `input_tokens`/`output_tokens` minus
//!     cached, and the `buildRawResponse` chat-completions-shaped `raw`.
//!   - **anthropic message** ([`parse_anthropic`]): text-block concat,
//!     thinking-block reasoning, `input_tokens`/`output_tokens` (no subtraction),
//!     `cache_creation`/`cache_read` cache usage.
//!   - **google generateContent** ([`parse_google`]): non-thought/non-functionCall
//!     text parts, thought-part reasoning, `thoughtSignature`,
//!     `promptTokenCount`/`candidatesTokenCount` minus `cachedContentTokenCount`.
//!   - **ollama non-stream** ([`parse_ollama`], `POST /api/chat`):
//!     `message.content` split through the think parser,
//!     `done ? 'stop' : 'length'`, `prompt_eval_count` / `eval_count`, and
//!     `reasoningContent` = native `message.thinking` + the inline `<think>`
//!     interiors (attached only when non-empty — P4.D78 / v4 `d9c5a1c7`).
//!
//! The chat-completions / anthropic / ollama flavors read the wire body directly.
//! The responses-API + google flavors read the SDK-normalized fields (`output_text`
//! / `response.text` materialize identically from the `output` / `parts` arrays);
//! this port reproduces those from the wire arrays so the parse is IO-free.
//!
//! Tool calls are extracted only where v4's `sendMessage` does (DeepSeek / Z.AI —
//! the openai-compatible base + anthropic/google/ollama/responses-API parses do
//! NOT surface `toolCalls`; tool detection there runs over `raw` above the seam).

use serde_json::{json, Map, Value};

use super::stream::{StreamCacheUsage, StreamUsage};

/// A tool call surfaced on a non-streaming response (v4 `ToolCall`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    /// Always `"function"` on the ported paths.
    pub type_: String,
    pub name: String,
    /// The raw arguments JSON string.
    pub arguments: String,
}

/// The parsed non-streaming response (v4 `LLMResponse`, minus `attachmentResults`
/// which is a request-side concern the transport carries). Optional fields are
/// `None`/empty exactly where v4 omits them (`undefined` dropped by
/// `JSON.stringify`).
#[derive(Clone, Debug, PartialEq)]
pub struct NonStreamingResponse {
    pub content: String,
    /// v4 `finishReason: string | null`.
    pub finish_reason: Option<String>,
    pub usage: StreamUsage,
    /// Empty when v4 sends `toolCalls: undefined`.
    pub tool_calls: Vec<ToolCall>,
    pub reasoning_content: Option<String>,
    pub thought_signature: Option<String>,
    pub cache_usage: Option<StreamCacheUsage>,
    /// v4 `raw` — the provider-specific raw response (for tool detection above the
    /// seam). Carried verbatim.
    pub raw: Value,
}

/// The chat-completions non-streaming flavor (mirrors the W4.7b decoder split).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatFlavor {
    /// The `openai-compatible` base: content / finishReason / usage / raw only.
    OpenAiCompatible,
    /// DeepSeek: reasoning + tools + `prompt_cache_hit_tokens` cache-read.
    DeepSeek,
    /// Z.AI: reasoning + tools + `prompt_tokens_details.cached_tokens` cache-read.
    ZAi,
    /// OpenRouter: camelCase normalized fields + `message.reasoning` + `cachedTokens`.
    OpenRouter,
    /// OpenRouter VISION (v4 bug 31's `sendViaChatCompletions`): the raw
    /// snake_case wire read DIRECTLY (no SDK zod), `message.reasoning`,
    /// `finish_reason || 'stop'`, and `prompt_tokens_details.cached_tokens`
    /// materialized into `cacheUsage` (`{cacheReadInputTokens, cachedTokens}`)
    /// — unlike the SDK path, whose `usage.cachedTokens` never exists. Same wire
    /// body, a DIFFERENT `LLMResponse` from [`ChatFlavor::OpenRouter`].
    OpenRouterVision,
    /// NanoGPT (P4.D101): reasoning is `message.reasoning` with
    /// `message.reasoning_content` as the LEGACY fallback — a precedence no
    /// other flavor has — minus the gateway's prose echo (v4 bug 87). Tool
    /// calls go through the OAC base's `normalizeToolCalls`.
    ///
    /// Cache usage (P4.D105, v4 `f8973813`): BOTH dialects the gateway emits,
    /// via [`nanogpt_cache_usage`] — the only flavor that reports cache WRITES
    /// as well as reads.
    NanoGpt,
}

fn i64_at(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// v4 `Math.max(0, a - b)`.
fn sub_floor(a: i64, b: i64) -> i64 {
    (a - b).max(0)
}

/// v4 NanoGPT `extractCacheUsage` (P4.D105, `f8973813`) — the only ported
/// extractor that reads TWO dialects and reports cache WRITES.
///
/// ```text
/// if (!usage) return undefined;
/// const read = usage.cache_read_input_tokens
///           ?? usage.prompt_tokens_details?.cached_tokens ?? 0;
/// const written = usage.cache_creation_input_tokens ?? 0;
/// if (read <= 0 && written <= 0) return undefined;
/// return { ...(read > 0 ? { cacheReadInputTokens: read, cachedTokens: read } : {}),
///          ...(written > 0 ? { cacheCreationInputTokens: written } : {}) };
/// ```
///
/// Two details the shape of this code is carrying, both load-bearing:
///
///   - The dialect chain is `??`, not `||`. A PRESENT zero
///     `cache_read_input_tokens` (Anthropic-routed, nothing read this turn)
///     therefore does NOT fall through to the OpenAI-style
///     `prompt_tokens_details.cached_tokens` — `or_else` on the `Option`
///     reproduces exactly that, since `Some(0)` short-circuits.
///   - The two output keys are independently conditional, so a write-only turn
///     yields `{ cacheCreationInputTokens }` with no read keys at all, and the
///     caller's `cacheUsage?.cacheReadInputTokens ?? 0` then correctly
///     subtracts nothing.
///
/// The cache-read EXCLUSION itself lives at the two call sites (v4 applies it
/// in `sendMessage` / the streaming final chunk, not in the extractor): the
/// shared `sub_floor(prompt_tokens, read)` / `sub_floor(total_tokens, read)`
/// below, and the decoder's twin.
pub(crate) fn nanogpt_cache_usage(usage: &Value) -> Option<StreamCacheUsage> {
    if usage.is_null() {
        return None;
    }
    let read = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);
    let written = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if read <= 0 && written <= 0 {
        return None;
    }
    Some(StreamCacheUsage {
        cached_tokens: (read > 0).then_some(read),
        cache_read_input_tokens: (read > 0).then_some(read),
        cache_creation_input_tokens: (written > 0).then_some(written),
        cache_discount: None,
    })
}

/// Extract the OpenAI-family `tool_calls` from a message (v4 filters
/// `type === 'function' || 'function' in tc`, stringifies non-string arguments).
fn extract_openai_tool_calls(msg: &Value) -> Vec<ToolCall> {
    let Some(arr) = msg.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter(|tc| {
            tc.get("type").and_then(Value::as_str) == Some("function")
                || tc.get("function").is_some()
        })
        .filter_map(|tc| {
            let f = tc.get("function")?;
            let raw_args = f.get("arguments");
            let arguments = match raw_args {
                Some(Value::String(s)) => s.clone(),
                Some(other) => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            };
            Some(ToolCall {
                id: tc
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                type_: "function".to_string(),
                name: f
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                arguments,
            })
        })
        .collect()
}

/// v4 `OpenAICompatibleProvider.normalizeToolCalls`
/// (`packages/plugin-utils/src/providers/openai-compatible.ts:302–324`, bug 71)
/// — the OAC base's OWN filter, deliberately distinct from
/// [`extract_openai_tool_calls`] in three arms: an entry lacking a truthy
/// `function.name` is SKIPPED (the DeepSeek-shape helper keeps it with an empty
/// name), `arguments: null` stringifies through v4's `?? {}` to `"{}"` (not
/// `"null"`), and there is no `type` filter at all — any entry with a named
/// function passes. Tolerates servers that hand back already-parsed argument
/// objects where the OpenAI wire format specifies a JSON string. (v4's
/// unchecked cast means a non-string truthy `name` would also pass there; a
/// `Value`-typed read skips it — out of the modeled domain, noted not modeled.)
fn normalize_oac_tool_calls(msg: &Value) -> Vec<ToolCall> {
    let Some(arr) = msg.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|tc| {
            let f = tc.get("function")?;
            let name = f
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())?;
            let arguments = match f.get("arguments") {
                Some(Value::String(s)) => s.clone(),
                // v4: `JSON.stringify(args ?? {})` — null and absent both `{}`.
                Some(Value::Null) | None => "{}".to_string(),
                Some(other) => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
            };
            Some(ToolCall {
                id: tc
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                type_: "function".to_string(),
                name: name.to_string(),
                arguments,
            })
        })
        .collect()
}

/// Parse a chat-completions non-streaming response body.
pub fn parse_chat_completions(response: &Value, flavor: ChatFlavor) -> NonStreamingResponse {
    let empty = Value::Null;
    let choice = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .unwrap_or(&empty);
    let msg = choice.get("message").unwrap_or(&empty);
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);

    match flavor {
        ChatFlavor::OpenRouter => {
            // v4's `sendMessage` reads the object the @openrouter/sdk returns,
            // which is the WIRE body passed through the SDK's inbound zod:
            // declared keys only (unknown keys stripped), snake_case renamed to
            // camelCase, keys re-emitted in schema-declaration order. The
            // recorded-body corpus caught v5 reading camelCase off the raw
            // snake_case wire (usage parsed to zeros on every OpenRouter
            // non-streaming call — the #24 class); normalize first, exactly as
            // the SDK does, and use the normalized object for BOTH the field
            // reads and `raw` (v4's `raw: response` is the SDK object).
            let normalized = openrouter_sdk_normalize(response);
            let response = &normalized;
            let empty = Value::Null;
            let choice = response
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .unwrap_or(&empty);
            let msg = choice.get("message").unwrap_or(&empty);
            let usage = response.get("usage").cloned().unwrap_or(Value::Null);
            let content = choice
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let finish_reason = Some(
                choice
                    .get("finishReason")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("stop")
                    .to_string(),
            );
            let reasoning = msg
                .get("reasoning")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let cached = usage.get("cachedTokens").and_then(Value::as_i64);
            let discount = usage.get("cacheDiscount").and_then(Value::as_f64);
            // v4 `usageAny?.cachedTokens || usageAny?.cacheDiscount` truthiness.
            let has_cache = cached.map(|c| c != 0).unwrap_or(false)
                || discount.map(|d| d != 0.0).unwrap_or(false);
            let cache_usage = has_cache.then_some(StreamCacheUsage {
                cached_tokens: cached,
                cache_discount: discount,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            });
            let read = cache_usage
                .and_then(|c| c.cache_read_input_tokens.or(c.cached_tokens))
                .unwrap_or(0);
            NonStreamingResponse {
                content,
                finish_reason,
                usage: StreamUsage {
                    prompt_tokens: sub_floor(i64_at(&usage, "promptTokens"), read),
                    completion_tokens: i64_at(&usage, "completionTokens"),
                    total_tokens: sub_floor(i64_at(&usage, "totalTokens"), read),
                },
                tool_calls: Vec::new(),
                reasoning_content: reasoning,
                thought_signature: None,
                cache_usage,
                raw: normalized.clone(),
            }
        }
        ChatFlavor::OpenRouterVision => {
            // v4 bug 31's `sendViaChatCompletions` reads the RAW snake_case wire
            // body directly (no SDK zod): `content`, `finish_reason || 'stop'`,
            // `message.reasoning`, and `usage.prompt_tokens_details.cached_tokens`
            // (materialized into cacheUsage, unlike the SDK path). `raw: data` is
            // the wire body itself.
            let content = msg
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let finish_reason = Some(
                choice
                    .get("finish_reason")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("stop")
                    .to_string(),
            );
            let reasoning = msg
                .get("reasoning")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            // v4: `cachedTokens = usage.prompt_tokens_details?.cached_tokens`;
            // cacheUsage when it is present AND > 0; `cacheRead = cachedTokens ?? 0`.
            let cached = usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(Value::as_i64);
            let cache_usage = cached.filter(|&c| c > 0).map(|c| StreamCacheUsage {
                cached_tokens: Some(c),
                cache_read_input_tokens: Some(c),
                cache_discount: None,
                cache_creation_input_tokens: None,
            });
            let read = cached.unwrap_or(0);
            NonStreamingResponse {
                content,
                finish_reason,
                usage: StreamUsage {
                    prompt_tokens: sub_floor(i64_at(&usage, "prompt_tokens"), read),
                    completion_tokens: i64_at(&usage, "completion_tokens"),
                    total_tokens: sub_floor(i64_at(&usage, "total_tokens"), read),
                },
                tool_calls: Vec::new(),
                reasoning_content: reasoning,
                thought_signature: None,
                cache_usage,
                raw: response.clone(),
            }
        }
        _ => {
            // snake_case OpenAI shape (base / deepseek / z-ai).
            let content = msg
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let finish_reason = choice
                .get("finish_reason")
                .and_then(|v| if v.is_null() { None } else { v.as_str() })
                .map(str::to_string);

            let (reasoning, tool_calls, cache_usage) = match flavor {
                // Bug 71 (v4 `93ed8abf`): the OAC base normalizes `tool_calls`
                // on the non-streaming path too, with its own filter (see
                // `normalize_oac_tool_calls`). No reasoning channel, no cache
                // usage — v4's base `sendMessage` reads neither.
                ChatFlavor::OpenAiCompatible => (None, normalize_oac_tool_calls(msg), None),
                ChatFlavor::DeepSeek => {
                    let reasoning = msg
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    // extractCacheUsage: prompt_cache_hit_tokens > 0.
                    let hit = usage.get("prompt_cache_hit_tokens").and_then(Value::as_i64);
                    let cache = hit.filter(|&h| h > 0).map(|h| StreamCacheUsage {
                        cached_tokens: Some(h),
                        cache_read_input_tokens: Some(h),
                        cache_discount: None,
                        cache_creation_input_tokens: None,
                    });
                    (reasoning, extract_openai_tool_calls(msg), cache)
                }
                ChatFlavor::ZAi => {
                    let reasoning = msg
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    let cached = usage
                        .get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(Value::as_i64);
                    let cache = cached.filter(|&c| c > 0).map(|c| StreamCacheUsage {
                        cached_tokens: Some(c),
                        cache_read_input_tokens: Some(c),
                        cache_discount: None,
                        cache_creation_input_tokens: None,
                    });
                    (reasoning, extract_openai_tool_calls(msg), cache)
                }
                // P4.D101. Two facts no other flavor combines:
                //
                //  1. `message.reasoning` is NanoGPT's main-endpoint field and
                //     `message.reasoning_content` its legacy dialect — read in
                //     that precedence (`??`), not one or the other.
                //  2. v4 bug 87 (`4cb1035e`): the gateway sometimes echoes the
                //     whole answer back down the reasoning channel, which would
                //     repeat the reply inside a thinking fold. An echo EQUAL to
                //     the content is dropped. v4 compares the raw run against
                //     `msg.content ?? ''` BEFORE any truthiness filter, so an
                //     empty reasoning against empty content also drops — which
                //     the `!s.is_empty()` filter would have done anyway.
                //
                // Tool calls use the OAC base's `normalizeToolCalls`
                // (`normalize_oac_tool_calls`), which is what v4's NanoGPT
                // provider calls — NOT DeepSeek's `extract_openai_tool_calls`,
                // whose `type === 'function'` prefilter it does not have. And
                // v4's `sendMessage` returns no `cacheUsage` at all.
                ChatFlavor::NanoGpt => {
                    let raw_reasoning = msg
                        .get("reasoning")
                        .or_else(|| msg.get("reasoning_content"))
                        .and_then(Value::as_str);
                    let reasoning = raw_reasoning
                        .filter(|r| *r != content)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    (
                        reasoning,
                        normalize_oac_tool_calls(msg),
                        nanogpt_cache_usage(&usage),
                    )
                }
                ChatFlavor::OpenRouter | ChatFlavor::OpenRouterVision => unreachable!(),
            };
            let read = cache_usage
                .and_then(|c| c.cache_read_input_tokens)
                .unwrap_or(0);
            NonStreamingResponse {
                content,
                finish_reason,
                usage: StreamUsage {
                    prompt_tokens: sub_floor(i64_at(&usage, "prompt_tokens"), read),
                    completion_tokens: i64_at(&usage, "completion_tokens"),
                    total_tokens: sub_floor(i64_at(&usage, "total_tokens"), read),
                },
                tool_calls,
                reasoning_content: reasoning,
                thought_signature: None,
                cache_usage,
                raw: response.clone(),
            }
        }
    }
}

/// The @openrouter/sdk `ChatResult` inbound normalization, reproduced from the
/// SDK's Speakeasy zod schemas (`esm/models/chatresult.js` and its nested
/// models): declared keys only (unknown wire keys are STRIPPED), snake_case
/// renamed to camelCase, keys re-emitted in schema-declaration order. This is
/// the response-side twin of the request builder's zod re-emission — v4's
/// `sendMessage` reads (and returns as `raw`) the object the SDK's inbound
/// transform produced, never the wire body itself.
///
/// Values whose nested schemas v4 chats never exercise are passed through
/// verbatim (a documented seam, not silence): `logprobs` contents (v4 never
/// requests logprobs), `audio` / `images`, `reasoningDetails` items, and the
/// `openrouterMetadata` value.
fn openrouter_sdk_normalize(response: &Value) -> Value {
    let copy = |src: &Value, m: &mut Map<String, Value>, wire: &str, out: &str| {
        if let Some(v) = src.get(wire) {
            m.insert(out.to_string(), v.clone());
        }
    };

    let norm_prompt_details = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(v, &mut m, "audio_tokens", "audioTokens");
        copy(v, &mut m, "cache_write_tokens", "cacheWriteTokens");
        copy(v, &mut m, "cached_tokens", "cachedTokens");
        copy(v, &mut m, "video_tokens", "videoTokens");
        Value::Object(m)
    };
    let norm_completion_details = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(
            v,
            &mut m,
            "accepted_prediction_tokens",
            "acceptedPredictionTokens",
        );
        copy(v, &mut m, "audio_tokens", "audioTokens");
        copy(v, &mut m, "reasoning_tokens", "reasoningTokens");
        copy(
            v,
            &mut m,
            "rejected_prediction_tokens",
            "rejectedPredictionTokens",
        );
        Value::Object(m)
    };
    let norm_cost_details = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(
            v,
            &mut m,
            "upstream_inference_completions_cost",
            "upstreamInferenceCompletionsCost",
        );
        copy(
            v,
            &mut m,
            "upstream_inference_cost",
            "upstreamInferenceCost",
        );
        copy(
            v,
            &mut m,
            "upstream_inference_prompt_cost",
            "upstreamInferencePromptCost",
        );
        Value::Object(m)
    };
    let norm_server_tool_details = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(v, &mut m, "tool_calls_executed", "toolCallsExecuted");
        copy(v, &mut m, "tool_calls_requested", "toolCallsRequested");
        copy(v, &mut m, "web_search_requests", "webSearchRequests");
        Value::Object(m)
    };
    let norm_usage = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(v, &mut m, "completion_tokens", "completionTokens");
        if let Some(d) = v.get("completion_tokens_details") {
            m.insert("completionTokensDetails".into(), norm_completion_details(d));
        }
        copy(v, &mut m, "cost", "cost");
        if let Some(d) = v.get("cost_details") {
            m.insert("costDetails".into(), norm_cost_details(d));
        }
        copy(v, &mut m, "is_byok", "isByok");
        copy(v, &mut m, "prompt_tokens", "promptTokens");
        if let Some(d) = v.get("prompt_tokens_details") {
            m.insert("promptTokensDetails".into(), norm_prompt_details(d));
        }
        if let Some(d) = v.get("server_tool_use_details") {
            m.insert("serverToolUseDetails".into(), norm_server_tool_details(d));
        }
        copy(v, &mut m, "total_tokens", "totalTokens");
        Value::Object(m)
    };
    let norm_tool_call = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        if let Some(f) = v.get("function") {
            let mut fm = Map::new();
            copy(f, &mut fm, "arguments", "arguments");
            copy(f, &mut fm, "name", "name");
            m.insert("function".into(), Value::Object(fm));
        }
        copy(v, &mut m, "id", "id");
        copy(v, &mut m, "type", "type");
        Value::Object(m)
    };
    let norm_message = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(v, &mut m, "audio", "audio");
        copy(v, &mut m, "content", "content");
        copy(v, &mut m, "images", "images");
        copy(v, &mut m, "name", "name");
        copy(v, &mut m, "reasoning", "reasoning");
        copy(v, &mut m, "reasoning_details", "reasoningDetails");
        copy(v, &mut m, "refusal", "refusal");
        copy(v, &mut m, "role", "role");
        if let Some(tc) = v.get("tool_calls").and_then(Value::as_array) {
            m.insert(
                "toolCalls".into(),
                Value::Array(tc.iter().map(norm_tool_call).collect()),
            );
        }
        Value::Object(m)
    };
    let norm_choice = |v: &Value| -> Value {
        if !v.is_object() {
            return v.clone();
        }
        let mut m = Map::new();
        copy(v, &mut m, "finish_reason", "finishReason");
        copy(v, &mut m, "index", "index");
        copy(v, &mut m, "logprobs", "logprobs");
        if let Some(msg) = v.get("message") {
            m.insert("message".into(), norm_message(msg));
        }
        Value::Object(m)
    };

    let mut m = Map::new();
    if let Some(choices) = response.get("choices").and_then(Value::as_array) {
        m.insert(
            "choices".into(),
            Value::Array(choices.iter().map(norm_choice).collect()),
        );
    }
    copy(response, &mut m, "created", "created");
    copy(response, &mut m, "id", "id");
    copy(response, &mut m, "model", "model");
    copy(response, &mut m, "object", "object");
    copy(
        response,
        &mut m,
        "openrouter_metadata",
        "openrouterMetadata",
    );
    copy(response, &mut m, "service_tier", "serviceTier");
    copy(response, &mut m, "system_fingerprint", "systemFingerprint");
    if let Some(u) = response.get("usage") {
        m.insert("usage".into(), norm_usage(u));
    }
    Value::Object(m)
}

/// The OpenAI SDK's `response.output_text` convenience getter, reproduced from
/// the wire arrays: concat the `text` of every `output_text` content part of
/// every `message` output item, in order.
///
/// **This must not read a top-level `output_text` key.** v4's plugin reads
/// `response.output_text` off the object the OpenAI **Node SDK** returns, where
/// that property is synthesized by exactly this aggregation. The raw HTTP body
/// carries no such key (verified against a live `POST /v1/responses` on
/// 2026-07-23: top-level keys are `id`/`object`/`created_at`/`status`/…/`output`
/// /`usage`, and the text lives at `output[i].content[j].text`). v5 parses the
/// wire body, so reading the key directly yielded `""` for **every**
/// non-streaming OpenAI/Grok call — dogfood finding #24.
pub(crate) fn responses_output_text(output: &[Value]) -> String {
    let mut text = String::new();
    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(parts) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in parts {
            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                if let Some(t) = part.get("text").and_then(Value::as_str) {
                    text.push_str(t);
                }
            }
        }
    }
    text
}

/// Parse a responses-API (OpenAI / Grok) non-streaming response body. Reproduces
/// `buildLLMResponse`: `output_text`, the reasoning-summary concat, the cache-read
/// subtraction, and the `buildRawResponse` chat-completions-shaped `raw`.
pub fn parse_responses_api(response: &Value) -> NonStreamingResponse {
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let content = responses_output_text(&output);

    // Reasoning: concat summary_text parts of reasoning items.
    let mut reasoning = String::new();
    for item in &output {
        if item.get("type").and_then(Value::as_str) == Some("reasoning") {
            if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                for part in summary {
                    if part.get("type").and_then(Value::as_str) == Some("summary_text") {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            reasoning.push_str(t);
                        }
                    }
                }
            }
        }
    }

    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let cached = usage
        .get("input_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_i64);
    let cache_usage = cached.filter(|&c| c > 0).map(|c| StreamCacheUsage {
        cached_tokens: Some(c),
        cache_read_input_tokens: Some(c),
        cache_discount: None,
        cache_creation_input_tokens: None,
    });
    let read = cached.unwrap_or(0);

    NonStreamingResponse {
        content,
        finish_reason: Some(responses_finish_reason(response, &output)),
        usage: StreamUsage {
            prompt_tokens: sub_floor(i64_at(&usage, "input_tokens"), read),
            completion_tokens: i64_at(&usage, "output_tokens"),
            total_tokens: sub_floor(i64_at(&usage, "total_tokens"), read),
        },
        tool_calls: Vec::new(),
        reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
        thought_signature: None,
        cache_usage,
        raw: build_responses_raw(response, &output),
    }
}

/// v4 `getFinishReason` — a `function_call` output item → `tool_calls`, else the
/// `status` mapping.
fn responses_finish_reason(response: &Value, output: &[Value]) -> String {
    if output
        .iter()
        .any(|i| i.get("type").and_then(Value::as_str) == Some("function_call"))
    {
        return "tool_calls".to_string();
    }
    match response.get("status").and_then(Value::as_str) {
        Some("completed") => "stop".to_string(),
        Some("incomplete") => response
            .get("incomplete_details")
            .and_then(|d| d.get("reason"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("length")
            .to_string(),
        Some("failed") => "error".to_string(),
        _ => "stop".to_string(),
    }
}

/// v4 `buildRawResponse` — reshape a responses-API response into a
/// chat-completions-shaped object (for tool detection above the seam).
fn build_responses_raw(response: &Value, output: &[Value]) -> Value {
    let mut tool_calls: Vec<Value> = Vec::new();
    for item in output {
        if item.get("type").and_then(Value::as_str) == Some("function_call") {
            tool_calls.push(json!({
                "id": item.get("call_id").cloned().unwrap_or(Value::Null),
                "type": "function",
                "function": {
                    "name": item.get("name").cloned().unwrap_or(Value::Null),
                    "arguments": item.get("arguments").cloned().unwrap_or(Value::Null),
                },
            }));
        }
    }
    // v4 reads the SDK's `response.output_text`; reproduce it from the wire
    // arrays (see `responses_output_text` — finding #24).
    let content = Value::String(responses_output_text(output));
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let mut message = Map::new();
    message.insert("role".into(), json!("assistant"));
    message.insert("content".into(), content);
    if !tool_calls.is_empty() {
        message.insert("tool_calls".into(), Value::Array(tool_calls.clone()));
    }
    json!({
        "id": response.get("id").cloned().unwrap_or(Value::Null),
        "object": "chat.completion",
        "created": response.get("created_at").cloned().unwrap_or(Value::Null),
        "model": response.get("model").cloned().unwrap_or(Value::Null),
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" },
        }],
        "usage": {
            "prompt_tokens": i64_at(&usage, "input_tokens"),
            "completion_tokens": i64_at(&usage, "output_tokens"),
            "total_tokens": i64_at(&usage, "total_tokens"),
        },
    })
}

/// Parse an Anthropic Messages API non-streaming response body.
pub fn parse_anthropic(response: &Value) -> NonStreamingResponse {
    let blocks = response
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut text = String::new();
    let mut thinking = String::new();
    for b in &blocks {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = b.get("text").and_then(Value::as_str) {
                    text.push_str(t);
                }
            }
            Some("thinking") => {
                if let Some(t) = b.get("thinking").and_then(Value::as_str) {
                    thinking.push_str(t);
                }
            }
            _ => {}
        }
    }

    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let has_cache = usage.get("cache_creation_input_tokens").is_some()
        || usage.get("cache_read_input_tokens").is_some();
    let cache_usage = has_cache.then(|| StreamCacheUsage {
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(Value::as_i64),
        cache_read_input_tokens: usage.get("cache_read_input_tokens").and_then(Value::as_i64),
        cached_tokens: None,
        cache_discount: None,
    });

    let input = i64_at(&usage, "input_tokens");
    let output = i64_at(&usage, "output_tokens");
    NonStreamingResponse {
        content: text,
        finish_reason: Some(
            response
                .get("stop_reason")
                .and_then(|v| if v.is_null() { None } else { v.as_str() })
                .unwrap_or("stop")
                .to_string(),
        ),
        usage: StreamUsage {
            prompt_tokens: input,
            completion_tokens: output,
            total_tokens: input + output,
        },
        tool_calls: Vec::new(),
        reasoning_content: (!thinking.is_empty()).then_some(thinking),
        thought_signature: None,
        cache_usage,
        raw: response.clone(),
    }
}

/// Parse a Google `generateContent` non-streaming response body.
pub fn parse_google(response: &Value) -> NonStreamingResponse {
    let candidates = response
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first = candidates.first().cloned().unwrap_or(Value::Null);
    let parts = first
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // content: concat non-thought, non-functionCall text parts (the `.text` getter
    // equivalent). v4 tries `response.text` first — reproduced by the parts concat.
    let content = if let Some(t) = response.get("text").and_then(Value::as_str) {
        t.to_string()
    } else {
        let mut out = String::new();
        for p in &parts {
            if p.get("functionCall").is_some()
                || p.get("thought").and_then(Value::as_bool) == Some(true)
            {
                continue;
            }
            if let Some(t) = p.get("text").and_then(Value::as_str) {
                out.push_str(t);
            }
        }
        out
    };

    // reasoning: concat thought-part text.
    let mut reasoning = String::new();
    for p in &parts {
        if p.get("thought").and_then(Value::as_bool) == Some(true) {
            if let Some(t) = p.get("text").and_then(Value::as_str) {
                reasoning.push_str(t);
            }
        }
    }

    // thoughtSignature: first part's, else any functionCall's.
    let thought_signature = parts
        .first()
        .and_then(|p| p.get("thoughtSignature"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            parts.iter().find_map(|p| {
                p.get("functionCall")
                    .and_then(|fc| fc.get("thoughtSignature"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        });

    let usage = response
        .get("usageMetadata")
        .cloned()
        .unwrap_or(Value::Null);
    let cached = usage.get("cachedContentTokenCount").and_then(Value::as_i64);
    let cache_usage = cached.filter(|&c| c > 0).map(|c| StreamCacheUsage {
        cached_tokens: Some(c),
        cache_read_input_tokens: Some(c),
        cache_discount: None,
        cache_creation_input_tokens: None,
    });
    let read = cached.unwrap_or(0);

    let finish_reason = first
        .get("finishReason")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("STOP")
        .to_string();

    NonStreamingResponse {
        content,
        finish_reason: Some(finish_reason),
        usage: StreamUsage {
            prompt_tokens: sub_floor(i64_at(&usage, "promptTokenCount"), read),
            completion_tokens: i64_at(&usage, "candidatesTokenCount"),
            total_tokens: sub_floor(i64_at(&usage, "totalTokenCount"), read),
        },
        tool_calls: Vec::new(),
        reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
        thought_signature,
        cache_usage,
        // v4 `raw = { ...JSON.parse(JSON.stringify(response)), functionCalls }`.
        raw: build_google_raw(response),
    }
}

fn build_google_raw(response: &Value) -> Value {
    let mut obj = response.as_object().cloned().unwrap_or_default();
    // v4 adds a flattened functionCalls array (name/args) when present —
    // `response.functionCalls` is a genai-SDK GETTER over
    // `candidates[0].content.parts[].functionCall` (first candidate only;
    // `undefined` when there are no parts or no calls), NOT a wire key. The
    // recorded-body corpus caught v5 reading a top-level `functionCalls` key no
    // body carries (the #24 class); reproduce the getter from the arrays.
    let getter_calls: Vec<&Value> = response
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| parts.iter().filter_map(|p| p.get("functionCall")).collect())
        .unwrap_or_default();
    if !getter_calls.is_empty() {
        let mapped: Vec<Value> = getter_calls
            .iter()
            .map(|fc| {
                // v4 `{ name: fc.name, args: fc.args }` — an absent field is
                // `undefined` and drops from the serialized object.
                let mut m = Map::new();
                if let Some(n) = fc.get("name") {
                    m.insert("name".into(), n.clone());
                }
                if let Some(a) = fc.get("args") {
                    m.insert("args".into(), a.clone());
                }
                Value::Object(m)
            })
            .collect();
        obj.insert("functionCalls".into(), Value::Array(mapped));
    }
    Value::Object(obj)
}

/// Parse an Ollama `POST /api/chat` non-streaming response body.
pub fn parse_ollama(response: &Value) -> NonStreamingResponse {
    // P4.D78 (v4 `d9c5a1c7`): reasoning may arrive on the dedicated `thinking`
    // field (Ollama parsed the model's template) or as inline `<think>` blocks
    // in the content (it did not). Route BOTH into `reasoningContent` and keep
    // the message clean. A non-string on either field is ignored (v4's `typeof
    // … === 'string' ? … : ''` guards).
    let raw_content = response
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let split = crate::model::ollama_think_parser::extract_think_blocks(raw_content);
    let native_thinking = response
        .get("message")
        .and_then(|m| m.get("thinking"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let reasoning_content = format!("{native_thinking}{}", split.reasoning);
    let done = response
        .get("done")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let prompt = i64_at(response, "prompt_eval_count");
    let eval = i64_at(response, "eval_count");
    NonStreamingResponse {
        content: split.content,
        finish_reason: Some(if done { "stop" } else { "length" }.to_string()),
        usage: StreamUsage {
            prompt_tokens: prompt,
            completion_tokens: eval,
            total_tokens: prompt + eval,
        },
        tool_calls: Vec::new(),
        // v4's `...(reasoningContent ? { reasoningContent } : {})` — an empty
        // concatenation leaves the field OFF the response entirely.
        reasoning_content: if reasoning_content.is_empty() {
            None
        } else {
            Some(reasoning_content)
        },
        thought_signature: None,
        cache_usage: None,
        raw: response.clone(),
    }
}

/// Dispatch the non-streaming parse for the canonical provider id (the transport
/// picks the family from the manifest's stream decoder; the non-streaming shape
/// mirrors it). See [`parse_for_provider_ex`] for the OpenRouter vision variant.
pub fn parse_for_provider(provider: &str, response: &Value) -> NonStreamingResponse {
    parse_for_provider_ex(provider, response, false)
}

/// Like [`parse_for_provider`], but `openrouter_vision` selects OpenRouter's
/// raw-wire vision parse (v4 bug 31): a non-streaming OpenRouter request that
/// carried a formattable image escaped the SDK to `sendViaChatCompletions`, so
/// its wire body is read DIRECTLY rather than through the SDK zod. Same wire
/// bytes, a different `LLMResponse`. Ignored for every non-OpenRouter provider.
pub fn parse_for_provider_ex(
    provider: &str,
    response: &Value,
    openrouter_vision: bool,
) -> NonStreamingResponse {
    use super::provider_io::ProviderKind;
    match ProviderKind::of(provider) {
        Some(ProviderKind::Anthropic) => parse_anthropic(response),
        Some(ProviderKind::OpenAi | ProviderKind::Grok) => parse_responses_api(response),
        Some(ProviderKind::Google) => parse_google(response),
        Some(ProviderKind::Ollama) => parse_ollama(response),
        Some(ProviderKind::OpenRouter) if openrouter_vision => {
            parse_chat_completions(response, ChatFlavor::OpenRouterVision)
        }
        Some(kind) => parse_chat_completions(
            response,
            kind.chat_parse_flavor()
                .expect("every remaining kind is a chat-completions flavor"),
        ),
        // An unknown provider falls back to the chat-completions base (v4's
        // default plugin shape).
        None => parse_chat_completions(response, ChatFlavor::OpenAiCompatible),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_completions_base_minimal() {
        let r = json!({
            "choices": [{ "message": { "content": "hi" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12 }
        });
        let p = parse_chat_completions(&r, ChatFlavor::OpenAiCompatible);
        assert_eq!(p.content, "hi");
        assert_eq!(p.finish_reason.as_deref(), Some("stop"));
        assert_eq!(p.usage.total_tokens, 12);
        assert!(p.tool_calls.is_empty());
        assert!(p.reasoning_content.is_none());
    }

    #[test]
    fn deepseek_cache_subtracts_and_reasons_and_tools() {
        let r = json!({
            "choices": [{
                "message": {
                    "content": "answer",
                    "reasoning_content": "thinking",
                    "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "f", "arguments": "{\"x\":1}" } }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 100, "completion_tokens": 5, "total_tokens": 105, "prompt_cache_hit_tokens": 40 }
        });
        let p = parse_chat_completions(&r, ChatFlavor::DeepSeek);
        assert_eq!(p.usage.prompt_tokens, 60); // 100 - 40
        assert_eq!(p.usage.total_tokens, 65); // 105 - 40
        assert_eq!(p.reasoning_content.as_deref(), Some("thinking"));
        assert_eq!(p.tool_calls.len(), 1);
        assert_eq!(p.tool_calls[0].name, "f");
        assert_eq!(p.cache_usage.unwrap().cache_read_input_tokens, Some(40));
    }

    #[test]
    fn anthropic_concat_text_and_thinking() {
        let r = json!({
            "content": [
                { "type": "thinking", "thinking": "hmm" },
                { "type": "text", "text": "part1 " },
                { "type": "text", "text": "part2" }
            ],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 50, "output_tokens": 8, "cache_read_input_tokens": 10 }
        });
        let p = parse_anthropic(&r);
        assert_eq!(p.content, "part1 part2");
        assert_eq!(p.reasoning_content.as_deref(), Some("hmm"));
        assert_eq!(p.finish_reason.as_deref(), Some("end_turn"));
        assert_eq!(p.usage.prompt_tokens, 50); // no subtraction
        assert_eq!(p.usage.total_tokens, 58);
        assert_eq!(p.cache_usage.unwrap().cache_read_input_tokens, Some(10));
    }

    #[test]
    fn ollama_done_maps_to_stop() {
        let r = json!({
            "message": { "content": "hello" }, "done": true,
            "prompt_eval_count": 7, "eval_count": 3
        });
        let p = parse_ollama(&r);
        assert_eq!(p.content, "hello");
        assert_eq!(p.finish_reason.as_deref(), Some("stop"));
        assert_eq!(p.usage.total_tokens, 10);
    }

    #[test]
    fn responses_api_output_text_and_toolcall_finish() {
        let r = json!({
            "id": "resp_1", "model": "gpt-x", "created_at": 123, "status": "completed",
            "output": [
                { "type": "reasoning", "summary": [{ "type": "summary_text", "text": "why" }] },
                { "type": "message", "role": "assistant", "status": "completed",
                  "content": [{ "type": "output_text", "text": "the answer" }] },
                { "type": "function_call", "call_id": "fc1", "name": "tool", "arguments": "{}" }
            ],
            "usage": { "input_tokens": 20, "output_tokens": 4, "total_tokens": 24, "input_tokens_details": { "cached_tokens": 5 } }
        });
        let p = parse_responses_api(&r);
        assert_eq!(p.content, "the answer");
        assert_eq!(p.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(p.reasoning_content.as_deref(), Some("why"));
        assert_eq!(p.usage.prompt_tokens, 15); // 20 - 5
                                               // raw is the chat-completions reshape.
        assert_eq!(p.raw["object"], json!("chat.completion"));
        assert_eq!(p.raw["choices"][0]["finish_reason"], json!("tool_calls"));
        // The SDK aggregation also feeds `raw` (v4 `buildRawResponse`).
        assert_eq!(
            p.raw["choices"][0]["message"]["content"],
            json!("the answer")
        );
    }

    /// Dogfood finding #24 — the shape transcribed from a live
    /// `POST https://api.openai.com/v1/responses` (gpt-5-nano, 2026-07-23):
    /// **no top-level `output_text`**, a leading `reasoning` item with empty
    /// `content`/`summary`, and the text at `output[i].content[j].text`.
    /// Reading the phantom key returned `""` for every non-streaming
    /// OpenAI/Grok call while `usage` parsed correctly — the exact production
    /// fingerprint (335 completion tokens, `contentLength: 0`).
    #[test]
    fn responses_api_real_wire_body_has_no_top_level_output_text() {
        let r = json!({
            "id": "resp_08b4", "object": "response", "created_at": 1784821066,
            "status": "completed", "background": false, "error": null,
            "incomplete_details": null, "instructions": null,
            "max_output_tokens": null, "model": "gpt-5-nano-2025-08-07",
            "output": [
                { "id": "rs_08b4", "type": "reasoning", "content": [],
                  "encrypted_content": "gAAAAA…", "summary": [] },
                { "id": "msg_08b4", "type": "message", "status": "completed",
                  "role": "assistant",
                  "content": [{ "type": "output_text", "annotations": [],
                                "logprobs": [], "text": "pong" }] }
            ],
            "usage": { "input_tokens": 508, "output_tokens": 335, "total_tokens": 843 }
        });
        assert!(
            r.get("output_text").is_none(),
            "the real wire body carries no top-level output_text — if this \
             fixture grows one, it is no longer the wire shape"
        );
        let p = parse_responses_api(&r);
        assert_eq!(p.content, "pong");
        assert_eq!(p.usage.completion_tokens, 335);
        assert_eq!(p.finish_reason.as_deref(), Some("stop"));
        // An empty `summary` array must not manufacture reasoning content.
        assert_eq!(p.reasoning_content, None);
        assert_eq!(p.raw["choices"][0]["message"]["content"], json!("pong"));
    }

    /// Multiple message items concatenate in order, matching the SDK getter.
    #[test]
    fn responses_api_concats_every_output_text_part() {
        let r = json!({
            "status": "completed",
            "output": [
                { "type": "message", "content": [
                    { "type": "output_text", "text": "He" },
                    { "type": "refusal", "text": "IGNORED" },
                    { "type": "output_text", "text": "llo" }
                ]},
                { "type": "message", "content": [{ "type": "output_text", "text": " there" }] }
            ]
        });
        assert_eq!(parse_responses_api(&r).content, "Hello there");
    }

    #[test]
    fn google_parts_concat_and_thought() {
        let r = json!({
            "candidates": [{
                "content": { "parts": [
                    { "thoughtSignature": "sig", "text": "reason", "thought": true },
                    { "text": "visible" }
                ] },
                "finishReason": "STOP"
            }],
            "usageMetadata": { "promptTokenCount": 30, "candidatesTokenCount": 6, "totalTokenCount": 36, "cachedContentTokenCount": 8 }
        });
        let p = parse_google(&r);
        assert_eq!(p.content, "visible");
        assert_eq!(p.reasoning_content.as_deref(), Some("reason"));
        assert_eq!(p.thought_signature.as_deref(), Some("sig"));
        assert_eq!(p.usage.prompt_tokens, 22); // 30 - 8
    }

    #[test]
    fn openrouter_normalizes_the_wire_like_the_sdk() {
        // The WIRE is snake_case; the @openrouter/sdk's inbound zod renames it
        // camelCase before v4 reads it (the P4.13 unit-4 corpus caught the old
        // camelCase-input assumption — a hand-authored fixture that proved
        // nothing about wire shape, the #24 class).
        let r = json!({
            "id": "gen-1", "model": "openai/gpt-4o", "object": "chat.completion",
            "created": 1_700_000_000, "system_fingerprint": null,
            "choices": [{ "logprobs": null, "finish_reason": "stop", "index": 0,
                "message": { "role": "assistant", "content": "hi", "reasoning": "r", "refusal": null } }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12,
                "prompt_tokens_details": { "cached_tokens": 3 } }
        });
        let p = parse_chat_completions(&r, ChatFlavor::OpenRouter);
        assert_eq!(p.content, "hi");
        assert_eq!(p.reasoning_content.as_deref(), Some("r"));
        // v4 reads `usage.cachedTokens` off the SDK object — a key the SDK
        // never materializes (cached tokens live under promptTokensDetails), so
        // cacheUsage stays absent and nothing is subtracted (the v4 quirk the
        // recorded corpus pins; do not "fix" it).
        assert_eq!(p.usage.prompt_tokens, 10);
        assert!(p.cache_usage.is_none());
        // The raw is the SDK-normalized object, camelCase in schema order.
        assert_eq!(p.raw["systemFingerprint"], Value::Null);
        assert_eq!(p.raw["usage"]["promptTokensDetails"]["cachedTokens"], 3);
        assert!(p.raw.get("system_fingerprint").is_none());
    }

    /// Bug 71 (v4 `93ed8abf`): the OAC non-streaming `tool_calls` normalize —
    /// v4 `normalizeToolCalls` (`openai-compatible.ts:302–324`). The three arms
    /// that make it DISTINCT from the DeepSeek-shape helper are each pinned:
    /// a nameless entry is skipped (not kept with an empty name), a null
    /// `arguments` becomes `"{}"` (not `"null"`), and an entry with no `type`
    /// key still passes. Object arguments stringify; `id` defaults empty.
    #[test]
    fn oac_nonstreaming_tool_calls_normalize() {
        let r = json!({
            "choices": [{
                "message": {
                    "content": "",
                    "tool_calls": [
                        { "id": "a", "type": "function",
                          "function": { "name": "lookup", "arguments": "{\"q\":1}" } },
                        { "id": "b", "type": "function",
                          "function": { "name": "", "arguments": "{}" } },
                        { "id": "c", "type": "function",
                          "function": { "arguments": "{}" } },
                        { "function": { "name": "parsed_obj",
                                        "arguments": { "x": [1, 2] } } },
                        { "id": "e", "type": "function",
                          "function": { "name": "null_args", "arguments": null } }
                    ]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        });
        let p = parse_chat_completions(&r, ChatFlavor::OpenAiCompatible);
        let names: Vec<&str> = p.tool_calls.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["lookup", "parsed_obj", "null_args"]);
        assert_eq!(p.tool_calls[0].id, "a");
        assert_eq!(p.tool_calls[0].arguments, "{\"q\":1}");
        assert_eq!(p.tool_calls[1].id, "");
        assert_eq!(p.tool_calls[1].arguments, "{\"x\":[1,2]}");
        assert_eq!(p.tool_calls[2].arguments, "{}");
        assert!(p.tool_calls.iter().all(|t| t.type_ == "function"));
    }
}
