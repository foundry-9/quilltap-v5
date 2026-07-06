//! `google-parts` — Google `@google/genai` `generateContentStream` (google).
//!
//! Transport: despite "not SSE" in the design-doc table caption, the genai
//! Node SDK's `processStreamResponse` (verified in
//! `@google/genai@1.52.0/dist/node/index.mjs`) reads a **`data:`-prefixed SSE**
//! body — delimiters `\n\n` / `\r\r` / `\r\n\r\n`, each event trimmed, must
//! `startsWith('data:')`, and the remainder trimmed + JSON-parsed into a
//! `GenerateContentResponse`. So this decoder sits on the SAME shared
//! [`super::sse`] splitter as the other three SSE decoders; the "manual
//! reader.read() loop" is the SDK's, below the plugin. (Flagged: the design-doc
//! table's "JSON array streaming vs newline" caption is superseded by the actual
//! SDK source — SSE `data:` frames.)
//!
//! Per-chunk (v4 `GoogleProvider.streamMessage`): iterate
//! `candidates[0].content.parts`:
//! - `functionCall` part → skipped (recovered from `raw_response` later).
//! - `thought === true` && `text` → append to cumulative reasoning, emit
//!   `{content:"", reasoning_content:<so far>}`.
//! - `thought === true` (no text) → signature-only, skipped.
//! - `text` → append to content, emit `{content:<text>}`.
//!
//! Terminal `done`: usage from the **last chunk**'s `usageMetadata`
//! (cache-adjusted via `cachedContentTokenCount`), the last chunk itself as
//! `raw_response` (`JSON.parse(JSON.stringify(lastResponse))`), the last chunk's
//! `usageMetadata` as `raw_provider_usage`, the `thoughtSignature` extracted
//! from the last chunk's parts, cumulative reasoning, and — for a thinking
//! model that streamed no visible text — a `finalContent` computed as the
//! last chunk's `.text` (concatenation of its non-thought part text).
//!
//! `is_thinking_model` is a v4 model-name predicate; since the terminal
//! `finalContent` branch depends on it, the decoder takes it as a construction
//! input (the manifest/host supplies it, matching v4's `isThinkingModel(model)`).

use serde_json::Value;

use super::sse::SseParser;
use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::{StreamCacheUsage, StreamUsage};

pub struct GooglePartsDecoder {
    sse: SseParser,
    is_thinking_model: bool,
    total_streamed_content: String,
    reasoning: String,
    saw_reasoning: bool,
    last_response: Option<Value>,
    done_emitted: bool,
}

impl GooglePartsDecoder {
    /// `is_thinking_model` mirrors v4 `isThinkingModel(params.model)` — it gates
    /// only the terminal `finalContent` fallback (a thinking model that streamed
    /// no visible text emits its last chunk's `.text` on `done`).
    pub fn new(is_thinking_model: bool) -> Self {
        Self {
            sse: SseParser::new(),
            is_thinking_model,
            total_streamed_content: String::new(),
            reasoning: String::new(),
            saw_reasoning: false,
            last_response: None,
            done_emitted: false,
        }
    }

    fn handle_chunk(&mut self, chunk: &Value, out: &mut Vec<StreamChunk>) {
        self.last_response = Some(chunk.clone());
        let empty = Vec::new();
        let parts = chunk
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .unwrap_or(&empty);
        for part in parts {
            if part.get("functionCall").is_some() {
                continue;
            }
            let is_thought = part.get("thought").and_then(|t| t.as_bool()) == Some(true);
            let text = part.get("text").and_then(|t| t.as_str());
            if is_thought {
                if let Some(text) = text {
                    if !text.is_empty() {
                        self.reasoning.push_str(text);
                        self.saw_reasoning = true;
                        out.push(StreamChunk {
                            content: String::new(),
                            reasoning_content: Some(self.reasoning.clone()),
                            ..Default::default()
                        });
                    }
                    // v4: `part.thought === true && part.text` — empty string is
                    // falsy in JS, so it falls to the signature-only branch (skip).
                }
                // thought part carrying only a signature (or empty text): skip.
                continue;
            }
            if let Some(text) = text {
                if !text.is_empty() {
                    self.total_streamed_content.push_str(text);
                    out.push(StreamChunk::content(text));
                }
            }
        }
    }

    /// v4 `extractThoughtSignature(lastResponse)`: `parts[0].thoughtSignature`,
    /// else the first `functionCall.thoughtSignature`.
    fn extract_thought_signature(resp: &Value) -> Option<String> {
        let parts = resp
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())?;
        if let Some(first) = parts.first() {
            if let Some(sig) = first.get("thoughtSignature").and_then(|s| s.as_str()) {
                return Some(sig.to_string());
            }
        }
        for part in parts {
            if let Some(sig) = part
                .get("functionCall")
                .and_then(|f| f.get("thoughtSignature"))
                .and_then(|s| s.as_str())
            {
                return Some(sig.to_string());
            }
        }
        None
    }

    /// v4 `extractTextFromResponse`: SDK `.text` (concatenation of non-thought,
    /// non-functionCall part text of the last chunk).
    fn extract_text(resp: &Value) -> String {
        let empty = Vec::new();
        let parts = resp
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .unwrap_or(&empty);
        let mut out = String::new();
        for part in parts {
            if part.get("functionCall").is_some()
                || part.get("thought").and_then(|t| t.as_bool()) == Some(true)
            {
                continue;
            }
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        out
    }

    fn build_done(&self) -> StreamChunk {
        let usage = self
            .last_response
            .as_ref()
            .and_then(|r| r.get("usageMetadata"));
        let cached = usage
            .and_then(|u| u.get("cachedContentTokenCount"))
            .and_then(|v| v.as_i64())
            .filter(|c| *c > 0);
        let cache_read = cached.unwrap_or(0);
        let g = |k: &str| {
            usage
                .and_then(|u| u.get(k))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        };

        let final_content = match &self.last_response {
            Some(resp) if self.is_thinking_model && self.total_streamed_content.is_empty() => {
                Self::extract_text(resp)
            }
            _ => String::new(),
        };

        let cache_usage = cached.map(|c| StreamCacheUsage {
            cached_tokens: Some(c),
            cache_read_input_tokens: Some(c),
            ..Default::default()
        });

        StreamChunk {
            content: final_content,
            done: true,
            usage: Some(StreamUsage {
                prompt_tokens: (g("promptTokenCount") - cache_read).max(0),
                completion_tokens: g("candidatesTokenCount"),
                total_tokens: (g("totalTokenCount") - cache_read).max(0),
            }),
            cache_usage,
            attachment_results: Some(Default::default()),
            raw_response: self.last_response.clone(),
            raw_provider_usage: Some(usage.cloned().unwrap_or(Value::Null)),
            thought_signature: self
                .last_response
                .as_ref()
                .and_then(Self::extract_thought_signature),
            reasoning_content: if self.saw_reasoning {
                Some(self.reasoning.clone())
            } else {
                None
            },
        }
    }

    fn process_events(
        &mut self,
        events: Vec<super::sse::SseEvent>,
    ) -> Result<Vec<StreamChunk>, DecodeError> {
        let mut out = Vec::new();
        for ev in events {
            let data = ev.data.trim();
            if data.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                // The genai SDK's early per-network-chunk error probe fires only
                // when a whole read is exactly an error JSON; the per-event
                // branch here treats a `data:` frame carrying an `error` key as a
                // fatal decode (the SDK would throw an ApiError).
                if let Some(err) = v.get("error") {
                    let status = err.get("status").cloned().unwrap_or(Value::Null);
                    let msg = format!("got status: {}. {}", status, v);
                    return Err(DecodeError::new(msg));
                }
                self.handle_chunk(&v, &mut out);
            }
        }
        Ok(out)
    }
}

impl StreamDecoder for GooglePartsDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        let events = self.sse.push(bytes);
        self.process_events(events)
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        let events = self.sse.finish();
        let mut out = self.process_events(events)?;
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

    fn drive(thinking: bool, wire: &[u8], chunk: usize) -> Result<Vec<StreamChunk>, DecodeError> {
        let mut d = GooglePartsDecoder::new(thinking);
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

    #[test]
    fn text_parts_and_usage() {
        let wire = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"!\"}]}}],\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2,\"totalTokenCount\":7}}\n\n";
        let out = drive(false, wire, 0).unwrap();
        assert_eq!(out[0].content, "Hi");
        assert_eq!(out[1].content, "!");
        let done = out.last().unwrap();
        assert_eq!(done.usage.unwrap().total_tokens, 7);
    }

    #[test]
    fn thought_parts_route_to_reasoning() {
        let wire = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"pondering\"},{\"text\":\"Answer\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
        let out = drive(true, wire, 0).unwrap();
        assert_eq!(out[0].reasoning_content.as_deref(), Some("pondering"));
        assert_eq!(out[1].content, "Answer");
        let done = out.last().unwrap();
        assert_eq!(done.reasoning_content.as_deref(), Some("pondering"));
    }

    #[test]
    fn thought_signature_from_last_chunk() {
        let wire = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"x\"}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"thoughtSignature\":\"SIG\",\"text\":\"y\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
        let out = drive(false, wire, 0).unwrap();
        assert_eq!(
            out.last().unwrap().thought_signature.as_deref(),
            Some("SIG")
        );
    }

    #[test]
    fn byte_at_a_time_matches() {
        let wire = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"He\"}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"llo\"}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
        assert_eq!(
            drive(false, wire, 0).unwrap(),
            drive(false, wire, 1).unwrap()
        );
    }

    #[test]
    fn error_frame_surfaces() {
        let wire = b"data: {\"error\":{\"status\":\"RESOURCE_EXHAUSTED\",\"code\":429,\"message\":\"quota\"}}\n\n";
        let err = drive(false, wire, 0).unwrap_err();
        assert!(err.message.contains("RESOURCE_EXHAUSTED"));
    }
}
