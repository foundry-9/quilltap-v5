//! `chat-completions-sse` — OpenAI Chat Completions SSE.
//!
//! v4 providers: openai-compatible (base), deepseek, z-ai (all via the OpenAI
//! SDK), and openrouter (its `streamViaChatCompletions` raw-`fetch` tool/vision
//! path). See the module-level divergence note: these do NOT share one
//! normalisation, so this decoder carries a [`Flavor`] selecting emission +
//! `raw_response` assembly over one shared SSE + wire parser.
//!
//! Wire: `data: <json>` SSE frames (the shared [`super::sse`] splitter), a
//! terminal `data: [DONE]` sentinel (skipped), and a trailing usage-only frame
//! (`choices: []`, `usage: {...}`). Each JSON is a `ChatCompletionChunk`:
//! `{ choices:[{ index, delta:{ content?, reasoning_content?/reasoning?,
//! tool_calls? }, finish_reason? }], usage? }`.
//!
//! Stateful work reproduced verbatim from v4:
//! - **content deltas** → `StreamChunk::content` (skipped when empty/absent).
//! - **reasoning deltas** → a `{content:"", reasoning_content:<cumulative>}`
//!   chunk (v4 accumulates and re-emits the full string each time).
//! - the **tool-call accumulator** keyed by `tool_calls[].index`, concatenating
//!   fragmented `function.arguments` strings and remembering `id` / `name`.
//! - **usage** captured from any frame carrying it (last wins).
//! - the terminal **`done`** chunk: cache-read-adjusted usage, the assembled
//!   `raw_response` (flavor-shaped), `cache_usage`, `raw_provider_usage`
//!   (openai-sdk flavor only — the raw fetch flavor never sets it), and
//!   cumulative `reasoning_content`.

use serde_json::{json, Value};

use super::sse::SseParser;
use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::{StreamCacheUsage, StreamUsage};

/// Which provider-specific normalisation the decoder applies over the shared
/// chat-completions wire. Selected by the manifest alongside the decoder enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// openai-compatible base: content deltas + a terminal usage-only chunk.
    /// No tool accumulation, no `raw_response`, no reasoning, no cache usage.
    OpenAiCompatible,
    /// deepseek via the OpenAI SDK: reasoning is `delta.reasoning_content`; the
    /// terminal `raw_response` is `{ choices:[{ index:0, message:{ role,
    /// content:"", tool_calls?, reasoning_content? }, finish_reason }], usage }`;
    /// cache from `prompt_cache_hit_tokens` (>0). Does NOT emit
    /// `raw_provider_usage` (deepseek's plugin omits it).
    DeepSeek,
    /// z-ai (GLM) via the OpenAI SDK: same reasoning/rawResponse/usage shape as
    /// DeepSeek, but cache comes from `prompt_tokens_details.cached_tokens` AND it
    /// DOES emit `raw_provider_usage` (the raw `usage` sub-object).
    ZAi,
    /// openrouter `streamViaChatCompletions`: reasoning is `delta.reasoning`;
    /// the terminal `raw_response` is `{ choices:[{ finishReason, delta:{
    /// toolCalls? } }], usage }` (camelCase); cache from
    /// `prompt_tokens_details.cached_tokens`. `raw_provider_usage` never set.
    OpenRouterRaw,
}

impl Flavor {
    /// The two OpenAI-SDK-mediated flavors share reasoning/tool/rawResponse
    /// shape (differing only on cache source + `raw_provider_usage`).
    fn is_openai_sdk(self) -> bool {
        matches!(self, Flavor::DeepSeek | Flavor::ZAi)
    }
}

/// One accumulating tool call, keyed by `tool_calls[].index`.
#[derive(Clone, Default)]
struct AccTool {
    id: String,
    name: String,
    arguments: String,
}

pub struct ChatCompletionsSseDecoder {
    flavor: Flavor,
    sse: SseParser,
    /// index → accumulated tool call (ordered by first-seen index; we store in a
    /// Vec<(index, tool)> to preserve v4's iteration = insertion order for the
    /// SDK-Map flavor and index order for the openrouter sparse-array flavor).
    tools: Vec<(i64, AccTool)>,
    reasoning: String,
    saw_reasoning: bool,
    /// Provider-shape usage sub-object captured from the last frame that had one.
    usage_raw: Option<Value>,
    finish_reason: Option<Value>,
    done_emitted: bool,
}

impl ChatCompletionsSseDecoder {
    pub fn new(flavor: Flavor) -> Self {
        Self {
            flavor,
            sse: SseParser::new(),
            tools: Vec::new(),
            reasoning: String::new(),
            saw_reasoning: false,
            usage_raw: None,
            finish_reason: None,
            done_emitted: false,
        }
    }

    fn tool_mut(&mut self, index: i64) -> &mut AccTool {
        if let Some(pos) = self.tools.iter().position(|(i, _)| *i == index) {
            return &mut self.tools[pos].1;
        }
        self.tools.push((index, AccTool::default()));
        let last = self.tools.len() - 1;
        &mut self.tools[last].1
    }

    /// Handle one parsed chat-completion chunk JSON, appending chunks to `out`.
    fn handle_chunk(&mut self, chunk: &Value, out: &mut Vec<StreamChunk>) {
        let choice = chunk.get("choices").and_then(|c| c.get(0));

        if let Some(choice) = choice {
            let delta = choice.get("delta");

            // content delta
            if let Some(content) = delta
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !content.is_empty() {
                    out.push(StreamChunk::content(content));
                }
            }

            // reasoning delta (field name differs by flavor)
            let reasoning_field = match self.flavor {
                Flavor::OpenRouterRaw => "reasoning",
                _ => "reasoning_content",
            };
            if self.flavor != Flavor::OpenAiCompatible {
                if let Some(r) = delta
                    .and_then(|d| d.get(reasoning_field))
                    .and_then(|c| c.as_str())
                {
                    if !r.is_empty() {
                        self.reasoning.push_str(r);
                        self.saw_reasoning = true;
                        out.push(StreamChunk {
                            content: String::new(),
                            reasoning_content: Some(self.reasoning.clone()),
                            ..Default::default()
                        });
                    }
                }
            }

            // tool-call fragments
            if self.flavor != Flavor::OpenAiCompatible {
                if let Some(tcs) = delta
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|t| t.as_array())
                {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if !id.is_empty() {
                                self.tool_mut(idx).id = id.to_string();
                            }
                        }
                        if let Some(name) = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                        {
                            if !name.is_empty() {
                                self.tool_mut(idx).name = name.to_string();
                            }
                        }
                        if let Some(args) = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                        {
                            if !args.is_empty() {
                                self.tool_mut(idx).arguments.push_str(args);
                            }
                        }
                    }
                }
            }

            if let Some(fr) = choice.get("finish_reason") {
                if !fr.is_null() {
                    self.finish_reason = Some(fr.clone());
                }
            }
        }

        // usage may ride any frame (including the choices:[] trailing frame).
        if let Some(usage) = chunk.get("usage") {
            if !usage.is_null() {
                self.usage_raw = Some(usage.clone());
            }
        }
    }

    /// Build the cache-adjusted normalised usage + the cache-usage object.
    ///
    /// Flavor difference on an ABSENT usage frame: deepseek/z-ai (OpenAiSdk)
    /// ALWAYS emit a usage object (`usage?.x ?? 0` → all-zero), while openrouter
    /// (OpenRouterRaw) and the plain base leave `usage` undefined (omitted).
    fn build_usage(&self) -> (Option<StreamUsage>, Option<StreamCacheUsage>) {
        let raw = match &self.usage_raw {
            Some(v) => v,
            None => {
                // OpenAiSdk still emits an all-zero usage; the others omit it.
                return if self.flavor.is_openai_sdk() {
                    (Some(StreamUsage::default()), None)
                } else {
                    (None, None)
                };
            }
        };
        let get = |k: &str| raw.get(k).and_then(|v| v.as_i64());
        let cached_from_details = || {
            raw.get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_i64())
                .filter(|c| *c > 0)
        };
        // Cache-read tokens: flavor-specific source.
        let cached: Option<i64> = match self.flavor {
            // deepseek: prompt_cache_hit_tokens (>0), see `extractCacheUsage`.
            Flavor::DeepSeek => get("prompt_cache_hit_tokens").filter(|h| *h > 0),
            // z-ai / openrouter: prompt_tokens_details.cached_tokens.
            Flavor::ZAi | Flavor::OpenRouterRaw => cached_from_details(),
            Flavor::OpenAiCompatible => None,
        };
        let cache_read = cached.unwrap_or(0);
        let prompt = get("prompt_tokens").unwrap_or(0);
        let completion = get("completion_tokens").unwrap_or(0);
        let total = get("total_tokens").unwrap_or(0);
        let usage = StreamUsage {
            prompt_tokens: (prompt - cache_read).max(0),
            completion_tokens: completion,
            total_tokens: (total - cache_read).max(0),
        };
        let cache_usage = cached.map(|c| StreamCacheUsage {
            cached_tokens: Some(c),
            cache_read_input_tokens: Some(c),
            ..Default::default()
        });
        (Some(usage), cache_usage)
    }

    /// Assemble the terminal `raw_response` value in the flavor's exact shape.
    fn build_raw_response(&self) -> Value {
        if self.flavor == Flavor::OpenAiCompatible {
            return Value::Null; // no raw_response on this flavor
        }
        if self.flavor.is_openai_sdk() {
            {
                let tool_calls: Vec<Value> = self
                    .tools
                    .iter()
                    .map(|(_, t)| {
                        json!({
                            "id": t.id,
                            "type": "function",
                            "function": { "name": t.name, "arguments": t.arguments },
                        })
                    })
                    .collect();
                let mut message = serde_json::Map::new();
                message.insert("role".into(), json!("assistant"));
                message.insert("content".into(), json!(""));
                if !tool_calls.is_empty() {
                    message.insert("tool_calls".into(), Value::Array(tool_calls));
                }
                if self.saw_reasoning {
                    message.insert("reasoning_content".into(), json!(self.reasoning));
                }
                return json!({
                    "choices": [{
                        "index": 0,
                        "message": Value::Object(message),
                        "finish_reason": self.finish_reason.clone().unwrap_or(Value::Null),
                    }],
                    "usage": self.usage_raw.clone().unwrap_or(Value::Null),
                });
            }
        }
        // OpenRouterRaw.
        let tool_calls: Vec<Value> = self
            .tools
            .iter()
            .map(|(_, t)| {
                json!({
                    "id": t.id,
                    "type": "function",
                    "function": { "name": t.name, "arguments": t.arguments },
                })
            })
            .collect();
        let finish = if tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        let mut delta = serde_json::Map::new();
        if !tool_calls.is_empty() {
            delta.insert("toolCalls".into(), Value::Array(tool_calls));
        }
        // usage is the normalised object here (openrouter builds
        // rawResponse.usage from its `usage` local, which is the
        // normalised {promptTokens,...}) — see build_done.
        json!({
            "choices": [{
                "finishReason": finish,
                "delta": Value::Object(delta),
            }],
        })
    }

    fn build_done(&self) -> StreamChunk {
        let (usage, cache_usage) = self.build_usage();
        let mut chunk = StreamChunk {
            content: String::new(),
            done: true,
            usage,
            cache_usage,
            attachment_results: Some(Default::default()),
            ..Default::default()
        };
        match self.flavor {
            Flavor::OpenAiCompatible => {
                // No raw_response, no reasoning, no raw_provider_usage.
            }
            Flavor::DeepSeek | Flavor::ZAi => {
                chunk.raw_response = Some(self.build_raw_response());
                if self.saw_reasoning {
                    chunk.reasoning_content = Some(self.reasoning.clone());
                }
                // z-ai emits rawProviderUsage (the raw `usage` sub-object);
                // deepseek does not (its plugin omits the field).
                if self.flavor == Flavor::ZAi {
                    chunk.raw_provider_usage = Some(self.usage_raw.clone().unwrap_or(Value::Null));
                }
            }
            Flavor::OpenRouterRaw => {
                // openrouter's rawResponse.usage is the *normalised* usage; when
                // no usage frame arrived v4's `usage` is undefined → the key is
                // dropped from rawResponse by JSON.stringify.
                let mut raw = self.build_raw_response();
                if let (Value::Object(ref mut map), Some(u)) = (&mut raw, &chunk.usage) {
                    map.insert(
                        "usage".into(),
                        json!({
                            "promptTokens": u.prompt_tokens,
                            "completionTokens": u.completion_tokens,
                            "totalTokens": u.total_tokens,
                        }),
                    );
                }
                chunk.raw_response = Some(raw);
                if self.saw_reasoning {
                    chunk.reasoning_content = Some(self.reasoning.clone());
                }
            }
        }
        chunk
    }

    fn process_events(&mut self, events: Vec<super::sse::SseEvent>) -> Vec<StreamChunk> {
        let mut out = Vec::new();
        for ev in events {
            let data = ev.data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                continue;
            }
            match serde_json::from_str::<Value>(data) {
                Ok(v) => self.handle_chunk(&v, &mut out),
                Err(_) => {
                    // v4's SDK path would surface a malformed frame as a parse
                    // error; the openrouter raw path silently skips ("Skip
                    // malformed JSON chunks"). We skip — the recorder produces
                    // only well-formed frames, and a skip is the conservative,
                    // never-wrong choice (matching the raw path exactly and the
                    // SDK path in practice, which never sees a malformed frame).
                }
            }
        }
        out
    }
}

impl StreamDecoder for ChatCompletionsSseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        let events = self.sse.push(bytes);
        Ok(self.process_events(events))
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        let events = self.sse.finish();
        let mut out = self.process_events(events);
        if !self.done_emitted {
            self.done_emitted = true;
            out.push(self.build_done());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(flavor: Flavor, wire: &[u8], chunk: usize) -> Vec<StreamChunk> {
        let mut d = ChatCompletionsSseDecoder::new(flavor);
        let mut out = Vec::new();
        if chunk == 0 {
            out.extend(d.push(wire).unwrap());
        } else {
            for c in wire.chunks(chunk) {
                out.extend(d.push(c).unwrap());
            }
        }
        out.extend(d.finish().unwrap());
        out
    }

    #[test]
    fn basic_content_and_usage() {
        let wire = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1,\"total_tokens\":6}}\n\ndata: [DONE]\n\n";
        let out = drive(Flavor::DeepSeek, wire, 0);
        assert_eq!(out[0].content, "Hi");
        let done = out.last().unwrap();
        assert!(done.done);
        assert_eq!(done.usage.unwrap().total_tokens, 6);
    }

    #[test]
    fn tool_call_fragments_accumulate() {
        let wire = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"foo\",\"arguments\":\"{\\\"a\\\":\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n";
        let out = drive(Flavor::DeepSeek, wire, 1);
        let done = out.last().unwrap();
        let raw = done.raw_response.as_ref().unwrap();
        let tc = &raw["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["function"]["arguments"], "{\"a\":1}");
        assert_eq!(tc["id"], "call_1");
    }

    #[test]
    fn byte_at_a_time_matches_whole() {
        let wire = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"t\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3}}\n\ndata: [DONE]\n\n";
        let whole = drive(Flavor::DeepSeek, wire, 0);
        let byte = drive(Flavor::DeepSeek, wire, 1);
        assert_eq!(whole.len(), byte.len());
        for (a, b) in whole.iter().zip(byte.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn openrouter_raw_shape() {
        let wire = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\ndata: [DONE]\n\n";
        let out = drive(Flavor::OpenRouterRaw, wire, 0);
        let done = out.last().unwrap();
        let raw = done.raw_response.as_ref().unwrap();
        assert_eq!(raw["choices"][0]["finishReason"], "tool_calls");
        assert_eq!(raw["choices"][0]["delta"]["toolCalls"][0]["id"], "c");
        assert_eq!(raw["usage"]["promptTokens"], 3);
    }

    #[test]
    fn finish_is_idempotent() {
        let wire = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"}}]}\n\ndata: [DONE]\n\n";
        let mut d = ChatCompletionsSseDecoder::new(Flavor::DeepSeek);
        let _ = d.push(wire).unwrap();
        let a = d.finish().unwrap();
        let b = d.finish().unwrap();
        assert_eq!(a.len(), 1);
        assert!(b.is_empty());
    }
}
