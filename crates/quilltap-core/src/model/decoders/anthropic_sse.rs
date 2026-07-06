//! `anthropic-sse` — Anthropic Messages SSE (anthropic).
//!
//! Wire: named SSE events (`event: <type>` + `data: <json>`) via the shared
//! [`super::sse`] splitter. The event `type` is ALSO present inside the JSON
//! `data` (`{"type":"message_start",...}`), and v4's SDK dispatches on the JSON
//! `type` — so this decoder reads the discriminator from the parsed JSON, not
//! the SSE `event:` line (they always agree, but the JSON is authoritative and
//! keeps a comment/`ping` frame harmless).
//!
//! State machine (v4 `AnthropicProvider.streamMessage`):
//! - `message_start` → capture input tokens, id, model, and a snapshot of the
//!   provider-shape usage (`rawProviderUsage`).
//! - `content_block_start` → init block at `index` (text / thinking / tool_use).
//! - `content_block_delta`:
//!   - `text_delta` → append to block + `fullContent`, emit `{content:<delta>}`.
//!   - `thinking_delta` → append (cumulative), emit `{content:"",
//!     reasoning_content:<so far>}`.
//!   - `signature_delta` → append to the thinking block's signature (no emit).
//!   - `input_json_delta` → buffer `partial_json` per block index (no emit).
//! - `content_block_stop` → parse the tool_use block's buffered JSON into
//!   `input` (drop `partialJson`).
//! - `message_delta` → output tokens + stop_reason; merge delta usage into the
//!   `rawProviderUsage` snapshot.
//! - `message_stop` → terminal `done`: usage (input/output, no cache
//!   subtraction — Anthropic reports cache reads separately), the assembled
//!   full message object as `raw_response`, `rawProviderUsage`, `cache_usage`
//!   (cache_creation/read_input_tokens), cumulative reasoning.
//! - `error` → surface as a `DecodeError` (v4's SDK throws).

use serde_json::{json, Value};

use super::sse::SseParser;
use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::{StreamCacheUsage, StreamUsage};

#[derive(Clone, Debug)]
enum Block {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    ToolUse {
        id: Value,
        name: Value,
        input: Value,
        partial_json: String,
    },
}

pub struct AnthropicSseDecoder {
    sse: SseParser,
    /// Content blocks by index (sparse → Vec of Option).
    blocks: Vec<Option<Block>>,
    full_content: String,
    reasoning: String,
    saw_reasoning: bool,
    input_tokens: i64,
    output_tokens: i64,
    message_id: Value,
    model: Value,
    stop_reason: Value,
    cache_creation_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    raw_provider_usage: Option<Value>,
    done_emitted: bool,
    /// An `error` event was seen mid-stream. v4's SDK throws at that point; we
    /// preserve any chunks emitted *before* the error in the same push (return
    /// them `Ok`), then surface the error on the next `push`/`finish`. This
    /// keeps content-before-error faithful (v4 streams can fail after yielding).
    pending_error: Option<DecodeError>,
}

impl Default for AnthropicSseDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicSseDecoder {
    pub fn new() -> Self {
        Self {
            sse: SseParser::new(),
            blocks: Vec::new(),
            full_content: String::new(),
            reasoning: String::new(),
            saw_reasoning: false,
            input_tokens: 0,
            output_tokens: 0,
            message_id: Value::Null,
            model: Value::Null,
            stop_reason: Value::Null,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            raw_provider_usage: None,
            done_emitted: false,
            pending_error: None,
        }
    }

    fn ensure_index(&mut self, index: usize) {
        if self.blocks.len() <= index {
            self.blocks.resize(index + 1, None);
        }
    }

    /// Handle one event; returns `true` if this event is a fatal `error` (the
    /// caller should stop and surface `self.pending_error`).
    fn handle_event(&mut self, ev: &Value, out: &mut Vec<StreamChunk>) -> bool {
        let ty = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "error" => {
                // v4's Anthropic SDK throws the whole error EVENT on an `error`
                // frame (verified against the real SDK: the message is the
                // JSON-stringified event, not just `error.message`). Stash it;
                // any chunks emitted before this in the batch are returned first.
                self.pending_error = Some(DecodeError::new(ev.to_string()));
                return true;
            }
            "message_start" => {
                let usage = ev.get("message").and_then(|m| m.get("usage"));
                self.input_tokens = usage
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.message_id = ev
                    .get("message")
                    .and_then(|m| m.get("id"))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.model = ev
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.cache_creation_input_tokens = usage
                    .and_then(|u| u.get("cache_creation_input_tokens"))
                    .and_then(|v| v.as_i64());
                self.cache_read_input_tokens = usage
                    .and_then(|u| u.get("cache_read_input_tokens"))
                    .and_then(|v| v.as_i64());
                self.raw_provider_usage = Some(usage.cloned().unwrap_or(Value::Null));
            }
            "content_block_start" => {
                let index = ev.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                self.ensure_index(index);
                let block = ev.get("content_block");
                let bt = block.and_then(|b| b.get("type")).and_then(|t| t.as_str());
                self.blocks[index] = match bt {
                    Some("text") => Some(Block::Text {
                        text: block
                            .and_then(|b| b.get("text"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }),
                    Some("thinking") => Some(Block::Thinking {
                        thinking: String::new(),
                        signature: String::new(),
                    }),
                    Some("tool_use") => Some(Block::ToolUse {
                        id: block
                            .and_then(|b| b.get("id"))
                            .cloned()
                            .unwrap_or(Value::Null),
                        name: block
                            .and_then(|b| b.get("name"))
                            .cloned()
                            .unwrap_or(Value::Null),
                        input: json!({}),
                        partial_json: String::new(),
                    }),
                    _ => None,
                };
            }
            "content_block_delta" => {
                let index = ev.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let delta = ev.get("delta");
                let dt = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str());
                match dt {
                    Some("text_delta") => {
                        if let Some(text) =
                            delta.and_then(|d| d.get("text")).and_then(|t| t.as_str())
                        {
                            if !text.is_empty() {
                                self.full_content.push_str(text);
                                if let Some(Some(Block::Text { text: t })) =
                                    self.blocks.get_mut(index)
                                {
                                    t.push_str(text);
                                }
                                out.push(StreamChunk::content(text));
                            }
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(th) = delta
                            .and_then(|d| d.get("thinking"))
                            .and_then(|t| t.as_str())
                        {
                            if !th.is_empty() {
                                if let Some(Some(Block::Thinking { thinking, .. })) =
                                    self.blocks.get_mut(index)
                                {
                                    thinking.push_str(th);
                                }
                                self.reasoning.push_str(th);
                                self.saw_reasoning = true;
                                out.push(StreamChunk {
                                    content: String::new(),
                                    reasoning_content: Some(self.reasoning.clone()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    Some("signature_delta") => {
                        if let Some(sig) = delta
                            .and_then(|d| d.get("signature"))
                            .and_then(|t| t.as_str())
                        {
                            if !sig.is_empty() {
                                if let Some(Some(Block::Thinking { signature, .. })) =
                                    self.blocks.get_mut(index)
                                {
                                    signature.push_str(sig);
                                }
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(pj) = delta
                            .and_then(|d| d.get("partial_json"))
                            .and_then(|t| t.as_str())
                        {
                            if !pj.is_empty() {
                                if let Some(Some(Block::ToolUse { partial_json, .. })) =
                                    self.blocks.get_mut(index)
                                {
                                    partial_json.push_str(pj);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = ev.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(Some(Block::ToolUse {
                    input,
                    partial_json,
                    ..
                })) = self.blocks.get_mut(index)
                {
                    if !partial_json.is_empty() {
                        if let Ok(v) = serde_json::from_str::<Value>(partial_json) {
                            *input = v;
                        }
                        // v4 logs on parse failure and keeps input = {} (the default).
                    }
                }
            }
            "message_delta" => {
                if let Some(ot) = ev
                    .get("usage")
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_i64())
                {
                    self.output_tokens = ot;
                }
                if let Some(sr) = ev.get("delta").and_then(|d| d.get("stop_reason")) {
                    if !sr.is_null() {
                        self.stop_reason = sr.clone();
                    }
                }
                // Merge delta usage into the snapshot.
                if let Some(delta_usage) = ev.get("usage").and_then(|u| u.as_object()) {
                    let base = match self.raw_provider_usage.take() {
                        Some(Value::Object(m)) => m,
                        _ => serde_json::Map::new(),
                    };
                    let mut merged = base;
                    for (k, v) in delta_usage {
                        merged.insert(k.clone(), v.clone());
                    }
                    self.raw_provider_usage = Some(Value::Object(merged));
                }
            }
            "message_stop" if !self.done_emitted => {
                // v4 yields the terminal chunk right here on the event. Mark
                // done so a stray second `message_stop` (never sent by the real
                // API) or `finish()` cannot double-emit.
                self.done_emitted = true;
                out.push(self.build_done());
            }
            _ => {}
        }
        false
    }

    /// Assemble the full message object v4 hands back as `raw_response`.
    fn build_full_message(&self) -> Value {
        let content: Vec<Value> = if self.blocks.is_empty() {
            vec![json!({ "type": "text", "text": self.full_content })]
        } else {
            self.blocks
                .iter()
                .filter_map(|b| b.as_ref())
                .map(|b| match b {
                    Block::Thinking {
                        thinking,
                        signature,
                    } => json!({
                        "type": "thinking",
                        "thinking": thinking,
                        "signature": signature,
                    }),
                    Block::ToolUse {
                        id, name, input, ..
                    } => json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }),
                    Block::Text { text } => json!({ "type": "text", "text": text }),
                })
                .collect()
        };
        // If blocks existed but all were filtered out (none), v4 keeps the
        // block-derived (empty) array — but v4's `contentBlocks.length > 0`
        // check uses the raw sparse array length, so an all-`None` blocks vec
        // (length > 0) yields []. We reproduce: empty blocks => text fallback,
        // non-empty => mapped (possibly empty) list.
        json!({
            "id": self.message_id,
            "type": "message",
            "role": "assistant",
            "content": content,
            "model": self.model,
            "stop_reason": self.stop_reason,
            "usage": {
                "input_tokens": self.input_tokens,
                "output_tokens": self.output_tokens,
            },
        })
    }

    fn build_done(&self) -> StreamChunk {
        let cache_usage = if self.cache_creation_input_tokens.is_some()
            || self.cache_read_input_tokens.is_some()
        {
            Some(StreamCacheUsage {
                cache_creation_input_tokens: self.cache_creation_input_tokens,
                cache_read_input_tokens: self.cache_read_input_tokens,
                ..Default::default()
            })
        } else {
            None
        };
        StreamChunk {
            content: String::new(),
            done: true,
            usage: Some(StreamUsage {
                prompt_tokens: self.input_tokens,
                completion_tokens: self.output_tokens,
                total_tokens: self.input_tokens + self.output_tokens,
            }),
            cache_usage,
            raw_provider_usage: Some(self.raw_provider_usage.clone().unwrap_or(Value::Null)),
            attachment_results: Some(Default::default()),
            raw_response: Some(self.build_full_message()),
            reasoning_content: if self.saw_reasoning {
                Some(self.reasoning.clone())
            } else {
                None
            },
            ..Default::default()
        }
    }

    /// Drive the given events; return chunks produced up to (not including) any
    /// error event. Stops at the first error, stashing it in `pending_error`.
    fn process_events(&mut self, events: Vec<super::sse::SseEvent>) -> Vec<StreamChunk> {
        let mut out = Vec::new();
        for ev in events {
            let data = ev.data.trim();
            if data.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if self.handle_event(&v, &mut out) {
                    break;
                }
            }
        }
        out
    }

    /// Return the stashed error and clear it — so the caller surfaces it exactly
    /// once, after any content already emitted.
    fn take_pending(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        match self.pending_error.take() {
            Some(e) => Err(e),
            None => Ok(Vec::new()),
        }
    }
}

impl StreamDecoder for AnthropicSseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        // A previously-stashed error surfaces before any further processing.
        if self.pending_error.is_some() {
            return self.take_pending();
        }
        let events = self.sse.push(bytes);
        let out = self.process_events(events);
        // If this push both emitted chunks and then hit an error, return the
        // chunks now (Ok); the error surfaces on the next call. If it emitted
        // nothing before the error, surface the error immediately.
        if out.is_empty() && self.pending_error.is_some() {
            return self.take_pending();
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        if self.pending_error.is_some() {
            return self.take_pending();
        }
        let events = self.sse.finish();
        // If message_stop never arrived (truncated), v4 emits nothing terminal
        // (the loop just ends). We match: no synthetic done on truncation.
        let out = self.process_events(events);
        if out.is_empty() && self.pending_error.is_some() {
            return self.take_pending();
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(wire: &[u8], chunk: usize) -> Result<Vec<StreamChunk>, DecodeError> {
        let mut d = AnthropicSseDecoder::new();
        let mut out = Vec::new();
        if chunk == 0 {
            out.extend(d.push(wire)?);
        } else {
            for c in wire.chunks(chunk) {
                out.extend(d.push(c)?);
            }
        }
        out.extend(d.finish()?);
        Ok(out)
    }

    const BASIC: &[u8] = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude\",\"usage\":{\"input_tokens\":10}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

    #[test]
    fn basic_text() {
        let out = drive(BASIC, 0).unwrap();
        assert_eq!(out[0].content, "Hi");
        let done = out.last().unwrap();
        assert!(done.done);
        let u = done.usage.unwrap();
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.completion_tokens, 3);
        assert_eq!(u.total_tokens, 13);
        assert_eq!(
            done.raw_response.as_ref().unwrap()["content"][0]["text"],
            "Hi"
        );
    }

    #[test]
    fn byte_at_a_time_matches() {
        assert_eq!(drive(BASIC, 0).unwrap(), drive(BASIC, 1).unwrap());
    }

    #[test]
    fn tool_use_input_json_buffering() {
        let wire = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"model\":\"c\",\"usage\":{\"input_tokens\":1}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"foo\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\":\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"1}\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":5}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let out = drive(wire, 1).unwrap();
        let done = out.last().unwrap();
        let block = &done.raw_response.as_ref().unwrap()["content"][0];
        assert_eq!(block["type"], "tool_use");
        assert_eq!(block["input"]["a"], 1);
    }

    /// Drive the wire and return (chunks emitted before any error, error?).
    fn drive_lossy(wire: &[u8], chunk: usize) -> (Vec<StreamChunk>, Option<DecodeError>) {
        let mut d = AnthropicSseDecoder::new();
        let mut out = Vec::new();
        let mut err = None;
        let feed = |d: &mut AnthropicSseDecoder,
                    r: Result<Vec<StreamChunk>, DecodeError>,
                    out: &mut Vec<StreamChunk>,
                    err: &mut Option<DecodeError>| {
            let _ = d;
            match r {
                Ok(cs) => out.extend(cs),
                Err(e) => {
                    if err.is_none() {
                        *err = Some(e);
                    }
                }
            }
        };
        if chunk == 0 {
            let r = d.push(wire);
            feed(&mut d, r, &mut out, &mut err);
        } else {
            for c in wire.chunks(chunk) {
                let r = d.push(c);
                feed(&mut d, r, &mut out, &mut err);
            }
        }
        let r = d.finish();
        feed(&mut d, r, &mut out, &mut err);
        (out, err)
    }

    #[test]
    fn error_event_surfaces_after_content() {
        let wire = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"model\":\"c\",\"usage\":{\"input_tokens\":5}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Starting\"}}\n\nevent: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        // Whole-buffer and byte-at-a-time both: "Starting" is emitted, then error.
        for chunk in [0usize, 1] {
            let (chunks, err) = drive_lossy(wire, chunk);
            assert_eq!(chunks.len(), 1, "chunk={chunk}");
            assert_eq!(chunks[0].content, "Starting", "chunk={chunk}");
            let e = err.expect("error surfaces");
            // v4's SDK throws the whole error EVENT, JSON-stringified.
            assert_eq!(
                e.message,
                "{\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}"
            );
        }
    }

    #[test]
    fn thinking_then_text_interleaved() {
        let wire = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"model\":\"c\",\"usage\":{\"input_tokens\":1}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hm\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let out = drive(wire, 0).unwrap();
        // reasoning chunk, then content, then done.
        assert_eq!(out[0].reasoning_content.as_deref(), Some("hm"));
        assert_eq!(out[1].content, "Hello");
        let done = out.last().unwrap();
        assert_eq!(done.reasoning_content.as_deref(), Some("hm"));
        let raw = done.raw_response.as_ref().unwrap();
        assert_eq!(raw["content"][0]["type"], "thinking");
        assert_eq!(raw["content"][0]["signature"], "sig");
        assert_eq!(raw["content"][1]["text"], "Hello");
    }
}
