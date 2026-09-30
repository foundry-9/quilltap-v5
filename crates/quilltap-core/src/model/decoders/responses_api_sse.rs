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
//! - `response.completed` (and, since v4 `8bd080267`, `response.incomplete`) →
//!   captures `event.response` (the full Response), which
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
    /// Grok's `buildRawResponse` reads `this.extractTextFromResponse(response)`
    /// for the raw's `content` (the SDK's `output_text` when truthy, else the
    /// `output_text` parts of every `message` item concatenated — `''` when
    /// there are none), where OpenAI's reads `response.output_text` raw. The
    /// two only part on a final response WITHOUT a truthy `output_text`, which
    /// no corpus wire carried until P4.D225's refusal wires.
    grok: bool,
    /// P4.122: a mid-stream error frame (see
    /// [`super::openai_sdk_frame_error`]), stashed so the chunks decoded
    /// before it in the same push are returned first.
    pending_error: Option<DecodeError>,
    /// Set once the error has been surfaced — the SDK's iterator has thrown,
    /// so nothing after it (not even the terminal `done`) is ever yielded.
    failed: bool,
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
            grok: false,
            pending_error: None,
            failed: false,
        }
    }

    /// Surface the stashed error exactly once and latch the decoder shut.
    fn take_pending(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        match self.pending_error.take() {
            Some(e) => {
                self.failed = true;
                Err(e)
            }
            None => Ok(Vec::new()),
        }
    }

    /// The Grok flavor (see [`Self::grok`]'s field doc).
    pub fn grok() -> Self {
        Self {
            grok: true,
            ..Self::new()
        }
    }

    /// v4 Grok `extractTextFromResponse`.
    fn grok_text(resp: &Value) -> String {
        if let Some(text) = resp
            .get("output_text")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
        {
            return text.to_string();
        }
        let mut text = String::new();
        for item in resp
            .get("output")
            .and_then(|o| o.as_array())
            .into_iter()
            .flatten()
        {
            if item.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }
            for part in item
                .get("content")
                .and_then(|c| c.as_array())
                .into_iter()
                .flatten()
            {
                if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                    // `text += content.text` — a missing text would concat
                    // "undefined" in JS; the Responses API always carries it.
                    text.push_str(part.get("text").and_then(|t| t.as_str()).unwrap_or(""));
                }
            }
        }
        text
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
            // v4 `8bd080267`: an incomplete response
            // (`incomplete_details.reason: 'content_filter'`) ends the stream
            // too, and its reason is the one the host needs.
            "response.completed" | "response.incomplete" => {
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
    fn build_raw_response(resp: &Value, grok: bool) -> Value {
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
        // v4 `8bd080267`: `this.getFinishReason(response)` — the real reason
        // (a refusal, an incomplete's `content_filter`), not a guess.
        let finish_reason = crate::model::response_parse::responses_finish_reason(resp, output);
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
        //
        // P4.D225: when the key is ABSENT (every real stream, per the note
        // above) v4's `content: undefined` is DROPPED by `JSON.stringify`, so
        // the raw carries no `content` key at all — v5 had written `null`. The
        // pre-existing corpus wires all carried an `output_text`, so the gap was
        // unmeasured until the `8bd080267` refusal wires (which do not) arrived.
        if grok {
            message.insert("content".into(), Value::String(Self::grok_text(resp)));
        } else if let Some(text) = resp.get("output_text") {
            message.insert("content".into(), text.clone());
        }
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
                    raw_response: Some(Self::build_raw_response(resp, self.grok)),
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
                // P4.122: the generic `Stream` both Responses plugins iterate
                // throws on `event: error` and on a truthy top-level
                // `data.error` — and on nothing else: a `response.failed`
                // frame (its error at `response.error`) falls through to
                // `handle_event`'s `_ => {}` and the stream ends without
                // `response.completed`, as on v4.
                if let Some(e) = super::openai_sdk_frame_error(&ev.event, &v) {
                    self.pending_error = Some(e);
                    break;
                }
                self.handle_event(&v, &mut out);
            }
        }
        out
    }
}

impl StreamDecoder for ResponsesApiSseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        if self.failed {
            return Ok(Vec::new());
        }
        if self.pending_error.is_some() {
            return self.take_pending();
        }
        let events = self.sse.push(bytes);
        let out = self.process_events(events);
        if out.is_empty() && self.pending_error.is_some() {
            return self.take_pending();
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        if self.failed {
            return Ok(Vec::new());
        }
        if self.pending_error.is_some() {
            return self.take_pending();
        }
        let events = self.sse.finish();
        let mut out = self.process_events(events);
        // At most one event dispatches at EOF, so nothing precedes it here.
        if self.pending_error.is_some() {
            return self.take_pending();
        }
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

    /// P4.122: `event: error` throws with the frame's code on the side; a
    /// `response.failed` frame does NOT (v4 ends "without
    /// response.completed" and yields an empty `done`).
    #[test]
    fn event_error_throws_and_response_failed_does_not() {
        let mut d = ResponsesApiSseDecoder::new();
        let err = d
            .push(b"event: error\ndata: {\"type\":\"error\",\"code\":\"content_filter\",\"message\":\"No.\"}\n\n")
            .unwrap_err();
        assert_eq!(err.message, "No.");
        assert_eq!(err.refusal.unwrap().code.as_deref(), Some("content_filter"));
        assert!(d.finish().unwrap().is_empty());

        let out = drive(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"x\"}}}\n\n",
            0,
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].done);
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
