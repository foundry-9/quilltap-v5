//! `responses-api-sse` — OpenAI **Responses API** SSE (openai, grok).
//!
//! Wire: `data: <json>` SSE frames (the shared [`super::sse`] splitter). Each
//! JSON is a Responses-API stream event with a `type` discriminator. v4's
//! generator (openai/grok `streamMessage`) consumes exactly three event types
//! and folds the rest away:
//! - `response.output_text.delta` → `{ content: event.delta }`.
//! - `response.reasoning_summary_text.delta` → **cumulative** reasoning: append
//!   `event.delta` to an accumulator and emit `{content:"",
//!   reasoning_content:<so far>}`.
//! - `response.completed` → captures `event.response` (the full Response), which
//!   the terminal `done` chunk turns into a Chat-Completions-shaped
//!   `raw_response` (via v4 `buildRawResponse`), cache-adjusted usage,
//!   `raw_provider_usage` (the raw `response.usage`), `cache_usage`, and the
//!   cumulative reasoning.
//!
//! If the stream ends WITHOUT a `response.completed` event, v4 emits a terminal
//! chunk with zero usage and no `raw_response` (the "Stream ended without
//! response.completed" warning path).

use serde_json::{json, Value};

use super::sse::SseParser;
use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::{StreamCacheUsage, StreamUsage};

pub struct ResponsesApiSseDecoder {
    sse: SseParser,
    reasoning: String,
    saw_reasoning: bool,
    /// The `event.response` of the terminal `response.completed` event.
    final_response: Option<Value>,
    done_emitted: bool,
}

impl Default for ResponsesApiSseDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponsesApiSseDecoder {
    pub fn new() -> Self {
        Self {
            sse: SseParser::new(),
            reasoning: String::new(),
            saw_reasoning: false,
            final_response: None,
            done_emitted: false,
        }
    }

    fn handle_event(&mut self, ev: &Value, out: &mut Vec<StreamChunk>) {
        let ty = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "response.output_text.delta" => {
                if let Some(delta) = ev.get("delta").and_then(|d| d.as_str()) {
                    out.push(StreamChunk::content(delta));
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(delta) = ev.get("delta").and_then(|d| d.as_str()) {
                    self.reasoning.push_str(delta);
                    self.saw_reasoning = true;
                    out.push(StreamChunk {
                        content: String::new(),
                        reasoning_content: Some(self.reasoning.clone()),
                        ..Default::default()
                    });
                }
            }
            "response.completed" => {
                if let Some(resp) = ev.get("response") {
                    self.final_response = Some(resp.clone());
                }
            }
            _ => {}
        }
    }

    /// v4 `buildRawResponse`: fold the Responses output into a Chat-Completions
    /// -shaped object (id/object/created/model/choices[0].message + tool_calls
    /// from function_call items + usage).
    fn build_raw_response(resp: &Value) -> Value {
        let empty = Vec::new();
        let output = resp
            .get("output")
            .and_then(|o| o.as_array())
            .unwrap_or(&empty);
        let mut tool_calls: Vec<Value> = Vec::new();
        for item in output {
            if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
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
        let finish_reason = if tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        let mut message = serde_json::Map::new();
        message.insert("role".into(), json!("assistant"));
        // DELIBERATELY the phantom key — do NOT aggregate from `output[]` here.
        // v4 opens this stream with `responses.create({stream: true})`, whose
        // events the SDK hands over RAW (`responses.js:27-32` applies
        // `addOutputText` only when the unwrapped result is a response object,
        // i.e. the non-streaming call). So v4's `buildRawResponse` reads
        // `undefined` here on every real stream and its raw carries no content.
        // v5 matches. Fixing it would diverge from the oracle — see dogfood
        // finding #24, whose non-streaming half IS fixed
        // (`response_parse::responses_output_text`).
        message.insert(
            "content".into(),
            resp.get("output_text").cloned().unwrap_or(Value::Null),
        );
        if !tool_calls.is_empty() {
            message.insert("tool_calls".into(), Value::Array(tool_calls));
        }
        let usage = resp.get("usage");
        let g = |k: &str| {
            usage
                .and_then(|u| u.get(k))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        };
        json!({
            "id": resp.get("id").cloned().unwrap_or(Value::Null),
            "object": "chat.completion",
            "created": resp.get("created_at").cloned().unwrap_or(Value::Null),
            "model": resp.get("model").cloned().unwrap_or(Value::Null),
            "choices": [{
                "index": 0,
                "message": Value::Object(message),
                "finish_reason": finish_reason,
            }],
            "usage": {
                "prompt_tokens": g("input_tokens"),
                "completion_tokens": g("output_tokens"),
                "total_tokens": g("total_tokens"),
            },
        })
    }

    fn build_done(&self) -> StreamChunk {
        match &self.final_response {
            None => StreamChunk {
                content: String::new(),
                done: true,
                usage: Some(StreamUsage::default()),
                attachment_results: Some(Default::default()),
                ..Default::default()
            },
            Some(resp) => {
                let usage = resp.get("usage");
                let cached = usage
                    .and_then(|u| u.get("input_tokens_details"))
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_i64())
                    .filter(|c| *c > 0);
                let cache_read = cached.unwrap_or(0);
                let g = |k: &str| {
                    usage
                        .and_then(|u| u.get(k))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                };
                let cache_usage = cached.map(|c| StreamCacheUsage {
                    cached_tokens: Some(c),
                    cache_read_input_tokens: Some(c),
                    ..Default::default()
                });
                StreamChunk {
                    content: String::new(),
                    done: true,
                    usage: Some(StreamUsage {
                        prompt_tokens: (g("input_tokens") - cache_read).max(0),
                        completion_tokens: g("output_tokens"),
                        total_tokens: (g("total_tokens") - cache_read).max(0),
                    }),
                    cache_usage,
                    attachment_results: Some(Default::default()),
                    raw_response: Some(Self::build_raw_response(resp)),
                    // rawProviderUsage: the raw response.usage (null when absent).
                    raw_provider_usage: Some(usage.cloned().unwrap_or(Value::Null)),
                    reasoning_content: if self.saw_reasoning {
                        Some(self.reasoning.clone())
                    } else {
                        None
                    },
                    ..Default::default()
                }
            }
        }
    }

    fn process_events(&mut self, events: Vec<super::sse::SseEvent>) -> Vec<StreamChunk> {
        let mut out = Vec::new();
        for ev in events {
            let data = ev.data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                self.handle_event(&v, &mut out);
            }
        }
        out
    }
}

impl StreamDecoder for ResponsesApiSseDecoder {
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

    fn drive(wire: &[u8], chunk: usize) -> Vec<StreamChunk> {
        let mut d = ResponsesApiSseDecoder::new();
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
    fn text_reasoning_and_completed() {
        let wire = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\ndata: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"th\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"model\":\"gpt-5\",\"output_text\":\"Hi\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":2,\"total_tokens\":12}}}\n\n";
        let out = drive(wire, 0);
        assert_eq!(out[0].content, "Hi");
        assert_eq!(out[1].reasoning_content.as_deref(), Some("th"));
        let done = out.last().unwrap();
        assert_eq!(done.usage.unwrap().total_tokens, 12);
        assert_eq!(
            done.raw_response.as_ref().unwrap()["choices"][0]["message"]["content"],
            "Hi"
        );
    }

    #[test]
    fn no_completed_event() {
        let wire = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n";
        let out = drive(wire, 0);
        let done = out.last().unwrap();
        assert!(done.done);
        assert_eq!(done.usage.unwrap().total_tokens, 0);
        assert!(done.raw_response.is_none());
    }

    #[test]
    fn byte_at_a_time_matches() {
        let wire = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"He\"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"llo\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r\",\"output_text\":\"Hello\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n";
        assert_eq!(drive(wire, 0), drive(wire, 1));
    }
}
