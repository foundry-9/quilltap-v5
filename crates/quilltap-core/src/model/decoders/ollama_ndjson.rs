//! `ollama-ndjson` — Ollama newline-delimited JSON (ollama). NOT SSE.
//!
//! Ollama's plugin is self-contained (raw `fetch` + `reader.read()`), NOT an
//! SDK. v4 bug 35 (`43a1b5b1`) rewrote its stream splitter to buffer across
//! reads: `streamMessage` now carries the trailing partial line between reads
//! and flushes the final tail. Per read it does
//! `buffer += decoder.decode(value, {stream:true})`, `lines = buffer.split('\n')`,
//! `buffer = lines.pop() ?? ''` mid-stream, and a full flush
//! (`buffer += decoder.decode()`; `lines = buffer.split('\n')`; `buffer = ''`) at
//! done; each line is `.trim()`ed before `JSON.parse`, empty-after-trim skipped,
//! parse failures skipped with a log. In sans-IO terms one `reader.read()` = one
//! [`StreamDecoder::push`] and stream end = [`StreamDecoder::finish`]:
//!
//! - a **line buffer** carries the trailing partial line between pushes, so a
//!   JSON object split across two reads reassembles instead of being lost;
//! - an **incremental UTF-8 decoder** carries an incomplete multi-byte sequence
//!   across pushes (v4's `TextDecoder({stream:true})`), so a multi-byte char
//!   split across reads is not corrupted — `finish()` flushes both, exactly as
//!   the bare `decoder.decode()` flush does at done;
//! - each line is `.trim()`ed (so a CRLF `\r` no longer defeats the parse) and
//!   empty-after-trim lines are skipped.
//!
//! This makes the decoder **push-boundary-insensitive**: whole-buffer, per-line,
//! and byte-at-a-time chunkings all produce the same chunk sequence, so ollama
//! now joins the three-chunking stream-decoder equivalence the other decoders
//! satisfy. (Before bug 35 v4 did NOT buffer, so a straddling object was
//! silently lost and the decoder was diffed at only whole + per-line — that
//! ported v4 bug is gone.)
//!
//! Per parsed object: track `model`; append `message.tool_calls` (whole objects);
//! route `message.thinking` + `message.content` through the reasoning/visible
//! split below; capture `prompt_eval_count` / `eval_count`; on `done:true` emit
//! the terminal chunk (usage + a `raw_response` = `{ model,
//! message:{role,content:<accumulated VISIBLE text>}, tool_calls? (normalized to
//! OpenAI shape) }`).
//!
//! ## Reasoning (P4.D78 — v4 `d9c5a1c7`)
//!
//! Reasoning arrives on two channels and both land on ONE cumulative counter:
//!
//! - **native** — `message.thinking` deltas (Ollama parsed the model's
//!   template), appended verbatim;
//! - **inline** — `<think>…</think>` leaking into `message.content` (it did
//!   not), split out by the shared
//!   [`ThinkTagStreamParser`](crate::model::ollama_think_parser::ThinkTagStreamParser)
//!   and spliced in by index, so the same interior is never counted twice.
//!
//! `reasoningContent` is emitted **CUMULATIVELY** — the full thinking-so-far,
//! not a delta — riding the visible-content chunk when one is being emitted
//! anyway, and otherwise on a chunk with `content: ""`. Visible text still
//! drives `total_content` and is NOT yielded when the split leaves it empty (a
//! chunk that was all think-tag interior emits reasoning only). At `done` the
//! parser is FLUSHED: any released tail yields as one more content chunk
//! BEFORE the terminal chunk (carrying no reasoning of its own), reasoning the
//! flush revealed is folded into the counter, and the terminal chunk carries
//! `reasoningContent` iff the counter is non-empty.

use serde_json::{json, Value};

use crate::model::ollama_think_parser::ThinkTagStreamParser;

use super::{DecodeError, StreamChunk, StreamDecoder};
use crate::model::stream::StreamUsage;

/// v4's `new TextDecoder()` used with `{stream:true}`: a streaming UTF-8 decoder
/// that holds an incomplete trailing multi-byte sequence between calls and emits
/// it (as replacement chars) only on the final bare `decode()` flush. `feed`
/// decodes as much as is complete, keeping any incomplete tail for the next call;
/// `flush` emits a replacement char for any leftover bytes (Node's end-of-stream
/// behaviour) and clears the buffer. Invalid bytes become `U+FFFD` exactly as
/// `String::from_utf8_lossy` (and Node's decoder) map them — one per maximal
/// subpart.
#[derive(Default)]
struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    fn feed(&mut self, bytes: &[u8], out: &mut String) {
        self.pending.extend_from_slice(bytes);
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    return;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // SAFETY: `valid_up_to` bounds a valid UTF-8 prefix.
                    out.push_str(unsafe { std::str::from_utf8_unchecked(&self.pending[..valid]) });
                    match e.error_len() {
                        // Incomplete but potentially-valid trailing sequence:
                        // hold it for the next push (v4's `{stream:true}`).
                        None => {
                            self.pending.drain(..valid);
                            return;
                        }
                        // A genuinely invalid sequence → one replacement char
                        // (from_utf8_lossy semantics), then keep scanning.
                        Some(bad) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + bad);
                        }
                    }
                }
            }
        }
    }

    fn flush(&mut self, out: &mut String) {
        // v4's bare `decoder.decode()` at done flushes any held incomplete bytes
        // as a single replacement char (WHATWG end-of-stream).
        if !self.pending.is_empty() {
            out.push('\u{FFFD}');
            self.pending.clear();
        }
    }
}

pub struct OllamaNdjsonDecoder {
    model: String,
    total_content: String,
    tool_calls: Vec<Value>,
    prompt_tokens: i64,
    completion_tokens: i64,
    /// v4's cross-read `buffer`: the trailing partial line held between pushes
    /// (a JSON object straddling two reads reassembles here instead of being
    /// lost).
    line_buffer: String,
    /// v4's `TextDecoder` streaming state: an incomplete multi-byte UTF-8
    /// sequence held between pushes.
    utf8: Utf8Stream,
    /// Ollama fires the terminal chunk on the `done:true` object *inside* the
    /// loop; once fired, subsequent pushes still parse but never re-emit a
    /// terminal (v4 would re-emit on a second done object, but real Ollama sends
    /// exactly one). We guard to keep `finish()` idempotent and avoid a spurious
    /// second terminal from a stray trailing done.
    done_emitted: bool,
    /// The inline `<think>` splitter shared across the whole stream (v4's
    /// per-call `thinkParser`).
    think_parser: ThinkTagStreamParser,
    /// v4 `reasoningSoFar` — the CUMULATIVE reasoning (native + inline).
    reasoning_so_far: String,
    /// v4 `inlineReasoningSeen` — how much of the parser's monotonic
    /// `reasoning` has already been spliced into `reasoning_so_far`. v4 counts
    /// UTF-16 units; bytes are equivalent here because the slice is always a
    /// prefix boundary of the same string.
    inline_reasoning_seen: usize,
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
            line_buffer: String::new(),
            utf8: Utf8Stream::default(),
            done_emitted: false,
            think_parser: ThinkTagStreamParser::new(),
            reasoning_so_far: String::new(),
            inline_reasoning_seen: 0,
        }
    }

    /// Splice whatever the think parser has newly captured into the cumulative
    /// counter (v4's `if (thinkParser.reasoning.length > inlineReasoningSeen)`
    /// guard, used at both the per-object and the flush call sites). Returns
    /// whether the counter grew.
    fn absorb_inline_reasoning(&mut self) -> bool {
        let reasoning = self.think_parser.reasoning();
        if reasoning.len() <= self.inline_reasoning_seen {
            return false;
        }
        let fresh = reasoning[self.inline_reasoning_seen..].to_string();
        self.inline_reasoning_seen = reasoning.len();
        self.reasoning_so_far.push_str(&fresh);
        true
    }

    /// Parse one already-`trim`med, non-empty line and dispatch it (v4's
    /// `JSON.parse(trimmedLine)` with the failure silently skipped).
    fn handle_line(&mut self, line: &str, out: &mut Vec<StreamChunk>) {
        // JS String.prototype.trim, not Rust trim: the sets differ (JS strips
        // U+FEFF, Rust strips U+0085) and the house helper exists for this.
        let trimmed = crate::jsstr::js_trim(line);
        if trimmed.is_empty() {
            return;
        }
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            self.handle_object(&v, out);
        }
        // v4: parse failures are skipped (warn mid-stream / debug at done) —
        // no observable output either way.
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
        // v4: `if (typeof data.message?.thinking === 'string' &&
        // data.message.thinking)` — a non-string or empty thinking is skipped.
        let mut reasoning_grew = false;
        if let Some(thinking) = data
            .get("message")
            .and_then(|m| m.get("thinking"))
            .and_then(|t| t.as_str())
        {
            if !thinking.is_empty() {
                self.reasoning_so_far.push_str(thinking);
                reasoning_grew = true;
            }
        }
        if let Some(content) = data
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            // v4's truthiness check: an empty content string never reaches the
            // parser (a no-op there anyway) and never yields.
            if !content.is_empty() {
                let visible = self.think_parser.push(content);
                if self.absorb_inline_reasoning() {
                    reasoning_grew = true;
                }
                if !visible.is_empty() {
                    // `total_content` accumulates the VISIBLE text, so the
                    // terminal `raw_response.message.content` is think-free.
                    self.total_content.push_str(&visible);
                    let mut chunk = StreamChunk::content(visible);
                    if reasoning_grew {
                        chunk.reasoning_content = Some(self.reasoning_so_far.clone());
                        reasoning_grew = false;
                    }
                    out.push(chunk);
                }
            }
        }
        // Reasoning that had no visible chunk to ride on gets its own
        // content-empty chunk (v4's trailing `if (reasoningGrew)`).
        if reasoning_grew {
            out.push(StreamChunk {
                content: String::new(),
                done: false,
                reasoning_content: Some(self.reasoning_so_far.clone()),
                ..Default::default()
            });
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
            // Release anything the think parser held back (a trailing partial
            // tag, or an unterminated think block's reasoning). The tail is one
            // more CONTENT chunk before the terminal one and carries no
            // reasoning of its own; reasoning the flush revealed only reaches
            // the terminal chunk.
            let tail = self.think_parser.flush();
            self.absorb_inline_reasoning();
            if !tail.is_empty() {
                self.total_content.push_str(&tail);
                out.push(StreamChunk::content(tail));
            }
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
            // v4's `...(reasoningSoFar ? { reasoningContent } : {})`.
            reasoning_content: if self.reasoning_so_far.is_empty() {
                None
            } else {
                Some(self.reasoning_so_far.clone())
            },
            ..Default::default()
        }
    }
}

impl StreamDecoder for OllamaNdjsonDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError> {
        // v4 bug 35: `buffer += decoder.decode(value, {stream:true})` then
        // `lines = buffer.split('\n'); buffer = lines.pop() ?? ''`. Decode this
        // read (holding any incomplete multi-byte tail), append to the line
        // buffer, process every COMPLETE line, and carry the trailing fragment.
        self.utf8.feed(bytes, &mut self.line_buffer);
        let mut out = Vec::new();
        while let Some(idx) = self.line_buffer.find('\n') {
            let line: String = self.line_buffer.drain(..=idx).collect();
            self.handle_line(&line, &mut out);
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError> {
        // v4 bug 35's done branch: `buffer += decoder.decode()` (flush the
        // TextDecoder's held bytes), then `lines = buffer.split('\n'); buffer =
        // ''` — the trailing fragment is now a COMPLETE final line. The terminal
        // chunk still fires only on a `done:true` object (which may live in this
        // tail); a stream truncated before one emits nothing terminal.
        self.utf8.flush(&mut self.line_buffer);
        let mut out = Vec::new();
        let remaining = std::mem::take(&mut self.line_buffer);
        for line in remaining.split('\n') {
            self.handle_line(line, &mut out);
        }
        Ok(out)
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
    fn split_line_across_pushes_preserves_content() {
        // v4 bug 35 INVERTS the old bug-parity pin: a JSON object split across
        // two pushes now reassembles via the line buffer, so its content
        // survives (was silently lost pre-fix).
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d
            .push(b"{\"model\":\"m\",\"message\":{\"content\":\"Kept")
            .unwrap();
        out.extend(d.push(b"\"}}\n").unwrap());
        out.extend(
            d.push(b"{\"message\":{\"content\":\"\"},\"done\":true,\"eval_count\":1}\n")
                .unwrap(),
        );
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "Kept");
        assert!(out.last().unwrap().done);
        assert_eq!(
            out.last().unwrap().raw_response.as_ref().unwrap()["message"]["content"],
            "Kept"
        );
    }

    #[test]
    fn multibyte_char_split_across_pushes_not_corrupted() {
        // A multi-byte UTF-8 char (é = 0xC3 0xA9) split across pushes must
        // reassemble intact — v4's TextDecoder({stream:true}) holds the
        // incomplete tail; the incremental decoder matches.
        let mut d = OllamaNdjsonDecoder::new("m");
        let full = "{\"message\":{\"content\":\"caf\u{e9}\"}}\n".as_bytes();
        let split = full.len() - 4; // land inside the é bytes
        let mut out = d.push(&full[..split]).unwrap();
        out.extend(d.push(&full[split..]).unwrap());
        out.extend(
            d.push(b"{\"message\":{\"content\":\"\"},\"done\":true}\n")
                .unwrap(),
        );
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "caf\u{e9}");
    }

    #[test]
    fn crlf_line_parses_after_trim() {
        // v4 bug 35 trims each line before JSON.parse, so a CRLF terminator's
        // stray `\r` no longer defeats the parse.
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d
            .push(b"{\"message\":{\"content\":\"hi\"}}\r\n{\"message\":{\"content\":\"\"},\"done\":true}\r\n")
            .unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "hi");
        assert!(out.last().unwrap().done);
    }

    #[test]
    fn reasoning_is_cumulative_and_rides_a_visible_chunk_when_one_exists() {
        // P4.D78: native + inline land on ONE counter, emitted CUMULATIVELY;
        // a chunk that is all think-tag interior yields reasoning with empty
        // content, and a chunk with visible text carries the reasoning along.
        let wire = b"{\"message\":{\"content\":\"\",\"thinking\":\"one \"}}\n{\"message\":{\"content\":\"<think>two</think>Vis\"}}\n{\"message\":{\"content\":\"\"},\"done\":true}\n";
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "");
        assert_eq!(out[0].reasoning_content.as_deref(), Some("one "));
        assert_eq!(out[1].content, "Vis");
        assert_eq!(out[1].reasoning_content.as_deref(), Some("one two"));
        let done = out.last().unwrap();
        assert!(done.done);
        assert_eq!(done.reasoning_content.as_deref(), Some("one two"));
        // The raw response the tool loop reads is think-FREE.
        assert_eq!(
            done.raw_response.as_ref().unwrap()["message"]["content"],
            "Vis"
        );
    }

    #[test]
    fn a_held_partial_tag_flushes_as_content_before_the_terminal_chunk() {
        // v4 releases the parser's tail as one more `{content, done:false}`
        // chunk BEFORE the terminal one, and it carries no reasoning.
        let wire = b"{\"message\":{\"content\":\"answer<thi\"}}\n{\"message\":{\"content\":\"\"},\"done\":true}\n";
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out[0].content, "answer");
        assert_eq!(out[1].content, "<thi");
        assert!(!out[1].done);
        assert_eq!(out[1].reasoning_content, None);
        assert!(out[2].done);
        assert_eq!(out[2].reasoning_content, None);
    }

    #[test]
    fn flush_reasoning_reaches_only_the_terminal_chunk() {
        // An unterminated block: the flush folds its tail into the counter, but
        // v4 emits NO extra reasoning-only chunk at done — the growth shows up
        // solely on the terminal chunk.
        let wire = b"{\"message\":{\"content\":\"<think>abc</thin\"}}\n{\"message\":{\"content\":\"\"},\"done\":true}\n";
        let mut d = OllamaNdjsonDecoder::new("m");
        let mut out = d.push(wire).unwrap();
        out.extend(d.finish().unwrap());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].reasoning_content.as_deref(), Some("abc"));
        assert!(out[1].done);
        assert_eq!(out[1].reasoning_content.as_deref(), Some("abc</thin"));
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
