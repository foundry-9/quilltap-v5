//! `ollama-ndjson` — Ollama newline-delimited JSON (ollama). NOT SSE.
//!
//! Ollama's plugin is self-contained (raw `fetch` + `reader.read()`), NOT an
//! SDK. Per network read it does `decoder.decode(value).split('\n').filter(Boolean)`
//! and `JSON.parse`s each piece, **skipping parse failures with a warning** and
//! **without buffering across reads**. In sans-IO terms one `reader.read()` = one
//! [`StreamDecoder::push`]: each push splits its own bytes on `\n`, drops empty
//! pieces, parses each, skips failures — with **no cross-push buffering**.
//!
//! ### ⚠️ This decoder is push-boundary-sensitive BY DESIGN (a ported v4 bug)
//!
//! Because v4 does not buffer, a JSON object split across two `reader.read()`
//! calls has BOTH halves fail to parse → the content is **silently lost**. This
//! is a real v4 bug ([[stream-decoder-decisions]]). The port reproduces it: the
//! decoder's output depends on where pushes fall. Consequently the three-chunking
//! equivalence that the other decoders satisfy does NOT hold here vs v4 — v4 sees
//! exactly one chunking (the recorder's, aligned on complete lines as real Ollama
//! sends them). The differential feeds the SAME line-aligned chunking to v4 and
//! Rust; a separate Rust-side test documents the lossy split-line behavior (both
//! sides lose it identically when fed a mid-object split — bug parity).
//!
//! Per parsed object: track `model`; append `message.tool_calls` (whole objects);
//! on `message.content` emit `{content}`; capture `prompt_eval_count` /
//! `eval_count`; on `done:true` emit the terminal chunk (usage + a
//! `raw_response` = `{ model, message:{role,content:<accumulated>}, tool_calls?
//! (normalized to OpenAI shape) }`).

use serde_json::{json, Value};

use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::StreamUsage;

pub struct OllamaNdjsonDecoder {
    model: String,
    total_content: String,
    tool_calls: Vec<Value>,
    prompt_tokens: i64,
    completion_tokens: i64,
    /// Ollama fires the terminal chunk on the `done:true` object *inside* the
    /// loop; once fired, subsequent pushes still parse but never re-emit a
    /// terminal (v4 would re-emit on a second done object, but real Ollama sends
    /// exactly one). We guard to keep `finish()` idempotent and avoid a spurious
    /// second terminal from a stray trailing done.
    done_emitted: bool,
}

impl OllamaNdjsonDecoder {
    /// `default_model` is v4's `lastModel = params.model` seed (used in
    /// `raw_response` if no object carries a `model`).
    pub fn new(default_model: impl Into<String>) -> Self {
        Self {
            model: default_model.into(),
            total_content: String::new(),
            tool_calls: Vec::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            done_emitted: false,
        }
    }

    fn handle_object(&mut self, data: &Value, out: &mut Vec<StreamChunk>) {
        if let Some(m) = data.get("model").and_then(|m| m.as_str()) {
            self.model = m.to_string();
        }
        if let Some(tcs) = data
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .and_then(|t| t.as_array())
        {
            self.tool_calls.extend(tcs.iter().cloned());
        }
        if let Some(content) = data
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            if !content.is_empty() {
                self.total_content.push_str(content);
                out.push(StreamChunk::content(content));
            }
        }
        if let Some(p) = data.get("prompt_eval_count").and_then(|v| v.as_i64()) {
            // v4: `if (data.prompt_eval_count)` — 0 is falsy, so a 0 leaves the
            // running total untouched.
            if p != 0 {
                self.prompt_tokens = p;
            }
        }
        if let Some(e) = data.get("eval_count").and_then(|v| v.as_i64()) {
            if e != 0 {
                self.completion_tokens = e;
            }
        }
        if data.get("done").and_then(|d| d.as_bool()) == Some(true) && !self.done_emitted {
            self.done_emitted = true;
            out.push(self.build_done());
        }
    }

    fn build_done(&self) -> StreamChunk {
        let mut raw = serde_json::Map::new();
        raw.insert("model".into(), json!(self.model));
        raw.insert(
            "message".into(),
            json!({ "role": "assistant", "content": self.total_content }),
        );
        if !self.tool_calls.is_empty() {
            let normalized: Vec<Value> = self
                .tool_calls
                .iter()
                .map(|tc| {
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    // arguments may be an object (Ollama) or string (OpenAI).
                    let args = tc.get("function").and_then(|f| f.get("arguments"));
                    let args_str = match args {
                        Some(Value::String(s)) => s.clone(),
                        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".into()),
                        None => "{}".into(),
                    };
                    // v4: `{ id: tc.id, type, function }` — an undefined `tc.id`
                    // is DROPPED by JSON.stringify (not emitted as null). So we
                    // include `id` only when the source object carries it,
                    // preserving its leading position when present.
                    let mut obj = serde_json::Map::new();
                    if let Some(id) = tc.get("id") {
                        obj.insert("id".into(), id.clone());
                    }
                    obj.insert("type".into(), json!("function"));
                    obj.insert(
                        "function".into(),
                        json!({ "name": name, "arguments": args_str }),
                    );
                    Value::Object(obj)
                })
                .collect();
            raw.insert("tool_calls".into(), Value::Array(normalized));
        }
        StreamChunk {
            content: String::new(),
            done: true,
            usage: Some(StreamUsage {
                prompt_tokens: self.prompt_tokens,
                completion_tokens: self.completion_tokens,
                total_tokens: self.prompt_tokens + self.completion_tokens,
            }),
            attachment_results: Some(Default::default()),
            raw_response: Some(Value::Object(raw)),
            ..Default::default()
        }
    }
}

impl StreamDecoder for OllamaNdjsonDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        // v4: `decoder.decode(value, {stream:true})` — lossy UTF-8 per read,
        // then split('\n').filter(Boolean). No cross-push buffer.
        let s = String::from_utf8_lossy(bytes);
        let mut out = Vec::new();
        for line in s.split('\n') {
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(v) => self.handle_object(&v, &mut out),
                Err(_) => {
                    // v4: "Skip invalid JSON lines" (warn). Faithful skip.
                }
            }
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        // Ollama's terminal chunk is emitted on the `done:true` object, not at
        // EOF. If the stream truncated before a done object, v4 emits nothing
        // terminal (the loop just ends). We match: no synthetic done at EOF.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_and_done() {
        let wire = b"{\"model\":\"llama\",\"message\":{\"role\":\"assistant\",\"content\":\"Hi\"}}\n{\"model\":\"llama\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"prompt_eval_count\":5,\"eval_count\":2}\n";
        let mut d = OllamaNdjsonDecoder::new("llama");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "Hi");
        let done = out.last().unwrap();
        assert!(done.done);
        assert_eq!(done.usage.unwrap().total_tokens, 7);
        assert_eq!(
            done.raw_response.as_ref().unwrap()["message"]["content"],
            "Hi"
        );
    }

    #[test]
    fn tool_calls_normalized_to_openai_shape() {
        let wire = b"{\"model\":\"m\",\"message\":{\"role\":\"assistant\",\"content\":\"\",\"tool_calls\":[{\"function\":{\"name\":\"foo\",\"arguments\":{\"a\":1}}}]}}\n{\"model\":\"m\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n";
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        let done = out.last().unwrap();
        let tc = &done.raw_response.as_ref().unwrap()["tool_calls"][0];
        assert_eq!(tc["type"], "function");
        assert_eq!(tc["function"]["name"], "foo");
        assert_eq!(tc["function"]["arguments"], "{\"a\":1}");
    }

    #[test]
    fn split_line_across_pushes_loses_content_bug_parity() {
        // A JSON object split across two pushes: both halves fail to parse and
        // are dropped (the ported v4 bug). The done object (whole) still fires.
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d
            .push(b"{\"model\":\"m\",\"message\":{\"content\":\"Lo")
            .unwrap();
        out.extend(d.push(b"st\"}}\n").unwrap());
        out.extend(
            d.push(b"{\"message\":{\"content\":\"\"},\"done\":true,\"eval_count\":1}\n")
                .unwrap(),
        );
        out.extend(d.finish().unwrap());
        // The "Lost" content never appears (both fragments unparseable).
        assert!(out.iter().all(|c| c.content != "Lost"));
        assert!(out.last().unwrap().done);
    }

    #[test]
    fn done_object_carrying_content_emits_both() {
        // A single object with content AND done:true → content chunk, then done.
        let wire = b"{\"message\":{\"content\":\"final\"},\"done\":true,\"prompt_eval_count\":1,\"eval_count\":1}\n";
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "final");
        assert!(out[1].done);
        assert_eq!(
            out.last().unwrap().raw_response.as_ref().unwrap()["message"]["content"],
            "final"
        );
    }
}
