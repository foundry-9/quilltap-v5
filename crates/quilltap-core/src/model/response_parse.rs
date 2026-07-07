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
//!     `OpenRouter` (the camelCase normalized shape: `finishReason`,
//!     `promptTokens`, `message.reasoning`, `cachedTokens`/`cacheDiscount`).
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
//!     `message.content`, `done ? 'stop' : 'length'`, `prompt_eval_count` /
//!     `eval_count`.
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
}

fn i64_at(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// v4 `Math.max(0, a - b)`.
fn sub_floor(a: i64, b: i64) -> i64 {
    (a - b).max(0)
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
            // camelCase normalized shape.
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
                ChatFlavor::OpenAiCompatible => (None, Vec::new(), None),
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
                ChatFlavor::OpenRouter => unreachable!(),
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

/// Parse a responses-API (OpenAI / Grok) non-streaming response body. Reproduces
/// `buildLLMResponse`: `output_text`, the reasoning-summary concat, the cache-read
/// subtraction, and the `buildRawResponse` chat-completions-shaped `raw`.
pub fn parse_responses_api(response: &Value) -> NonStreamingResponse {
    let content = response
        .get("output_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

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
    let content = response.get("output_text").cloned().unwrap_or(Value::Null);
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
    // v4 adds a flattened functionCalls array (name/args) when present.
    if let Some(fcs) = response.get("functionCalls").and_then(Value::as_array) {
        let mapped: Vec<Value> = fcs
            .iter()
            .map(|fc| {
                json!({
                    "name": fc.get("name").cloned().unwrap_or(Value::Null),
                    "args": fc.get("args").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        obj.insert("functionCalls".into(), Value::Array(mapped));
    }
    Value::Object(obj)
}

/// Parse an Ollama `POST /api/chat` non-streaming response body.
pub fn parse_ollama(response: &Value) -> NonStreamingResponse {
    let content = response
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let done = response
        .get("done")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let prompt = i64_at(response, "prompt_eval_count");
    let eval = i64_at(response, "eval_count");
    NonStreamingResponse {
        content,
        finish_reason: Some(if done { "stop" } else { "length" }.to_string()),
        usage: StreamUsage {
            prompt_tokens: prompt,
            completion_tokens: eval,
            total_tokens: prompt + eval,
        },
        tool_calls: Vec::new(),
        reasoning_content: None,
        thought_signature: None,
        cache_usage: None,
        raw: response.clone(),
    }
}

/// Dispatch the non-streaming parse for the canonical provider id (the transport
/// picks the family from the manifest's stream decoder; the non-streaming shape
/// mirrors it).
pub fn parse_for_provider(provider: &str, response: &Value) -> NonStreamingResponse {
    match provider {
        "ANTHROPIC" => parse_anthropic(response),
        "OPENAI" | "GROK" => parse_responses_api(response),
        "GOOGLE" => parse_google(response),
        "OLLAMA" => parse_ollama(response),
        "DEEPSEEK" => parse_chat_completions(response, ChatFlavor::DeepSeek),
        "Z_AI" => parse_chat_completions(response, ChatFlavor::ZAi),
        "OPENROUTER" => parse_chat_completions(response, ChatFlavor::OpenRouter),
        // OPENAI_COMPATIBLE + any unknown chat-completions provider.
        _ => parse_chat_completions(response, ChatFlavor::OpenAiCompatible),
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
            "output_text": "the answer",
            "output": [
                { "type": "reasoning", "summary": [{ "type": "summary_text", "text": "why" }] },
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
    fn openrouter_camel_case() {
        let r = json!({
            "choices": [{ "message": { "content": "hi", "reasoning": "r" }, "finishReason": "stop" }],
            "usage": { "promptTokens": 10, "completionTokens": 2, "totalTokens": 12, "cachedTokens": 3 }
        });
        let p = parse_chat_completions(&r, ChatFlavor::OpenRouter);
        assert_eq!(p.content, "hi");
        assert_eq!(p.reasoning_content.as_deref(), Some("r"));
        assert_eq!(p.usage.prompt_tokens, 7); // 10 - 3
        assert_eq!(p.cache_usage.unwrap().cached_tokens, Some(3));
    }
}
