//! A spec-faithful, incremental Server-Sent-Events frame splitter shared by the
//! three SSE-based stream decoders (`chat_completions_sse`, `responses_api_sse`,
//! `anthropic_sse`) and — with the same framing rules — `google_parts` (whose
//! `@google/genai` transport is also `data:`-prefixed SSE, see the module).
//!
//! This is the *transport* layer only: it turns a byte stream (fed in arbitrary
//! `push` slices — one byte at a time, mid-line, mid-UTF-8) into a sequence of
//! **events**, each a `(event_name, data)` pair, where `data` is the joined
//! `data:` field lines. The decoders consume events; they never see raw bytes.
//!
//! ## Framing rules reproduced (WHATWG SSE / EventSource)
//!
//! - The stream is split into **lines** on `\n`, `\r`, or `\r\n`.
//! - A **blank line** dispatches the accumulated event.
//! - A line beginning with `:` is a **comment** (keep-alive) and is ignored.
//! - `field: value` — one optional leading space after the colon is stripped.
//!   A line with no colon is a field with an empty value (the whole line is the
//!   field name).
//! - The `data` field accumulates: multiple `data:` lines are joined with `\n`.
//! - The `event` field sets the event type for the next dispatch (defaults to
//!   the empty string, which callers treat as "message").
//! - On dispatch, if the data buffer had a trailing `\n` from the join it is
//!   dropped (the join never adds a trailing newline here); an event with no
//!   `data` lines still dispatches (data = "").
//! - `id` / `retry` fields exist in the spec but no provider uses them on these
//!   paths; they are parsed as generic fields and ignored by the decoders.
//!
//! ## Incremental correctness
//!
//! Bytes are appended to an internal buffer. Only *complete* lines (terminated
//! by a newline that has actually arrived) are processed; a partial trailing
//! line stays buffered for the next `push`. This makes the splitter correct for
//! any chunking — including a byte-at-a-time feed and a `\r\n` sequence split
//! across two pushes. UTF-8 is decoded lossily only at line boundaries, but
//! since we only decode complete lines and a `\n`/`\r` byte can never appear
//! inside a multi-byte UTF-8 sequence, a mid-UTF-8 split never corrupts a line.
//!
//! `finish()` flushes: if the buffer holds a final line with no trailing
//! newline, that line is processed; if an event is still pending (data seen but
//! not yet dispatched by a blank line), it is dispatched. Real provider streams
//! terminate cleanly, but a truncated stream must not silently drop its last
//! event.

/// One dispatched SSE event: the `event:` type (empty string = the default
/// "message" event) and the joined `data:` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Incremental SSE parser. Feed bytes with [`SseParser::push`]; drain complete
/// events from the returned Vec. Call [`SseParser::finish`] at EOF.
#[derive(Default)]
pub struct SseParser {
    /// Undecoded bytes not yet forming a complete line.
    buf: Vec<u8>,
    /// The `event:` value for the event under construction.
    event_type: String,
    /// The joined `data:` lines for the event under construction.
    data: String,
    /// Whether any field line has been seen since the last dispatch (so a
    /// blank line after a comment-only run does not dispatch an empty event
    /// spuriously — matching EventSource, which dispatches only when the data
    /// buffer is non-empty OR fields were set). We track "saw a field" so an
    /// explicit empty-data event (`event: ping\n\n`) still dispatches.
    have_fields: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed received bytes; return any events completed by this push.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        self.drain_complete_lines(&mut out);
        out
    }

    /// EOF: process any trailing partial line and dispatch a pending event.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        // Process a final line with no trailing newline.
        if !self.buf.is_empty() {
            let line = std::mem::take(&mut self.buf);
            let line = String::from_utf8_lossy(&line).into_owned();
            self.process_line(&line, &mut out);
        }
        // Dispatch a pending event (data seen but no closing blank line).
        if self.have_fields {
            self.dispatch(&mut out);
        }
        out
    }

    /// Extract every complete line currently in `buf` (a line is complete when
    /// its terminating `\n` or a bare `\r` **followed by a non-`\n` byte or the
    /// end of an already-terminated segment** has arrived). To keep `\r\n`
    /// atomic across pushes, a trailing lone `\r` at the very end of the buffer
    /// is left unprocessed until the next push reveals whether a `\n` follows.
    fn drain_complete_lines(&mut self, out: &mut Vec<SseEvent>) {
        let mut start = 0usize;
        let mut i = 0usize;
        let n = self.buf.len();
        while i < n {
            let b = self.buf[i];
            if b == b'\n' {
                let line = &self.buf[start..i];
                let line = String::from_utf8_lossy(line).into_owned();
                self.process_line(&line, out);
                i += 1;
                start = i;
            } else if b == b'\r' {
                // A lone trailing `\r` at the buffer end: could be the first half
                // of a `\r\n` split across pushes. Defer it.
                if i + 1 == n {
                    break;
                }
                let line = &self.buf[start..i];
                let line = String::from_utf8_lossy(line).into_owned();
                self.process_line(&line, out);
                if self.buf[i + 1] == b'\n' {
                    i += 2;
                } else {
                    i += 1;
                }
                start = i;
            } else {
                i += 1;
            }
        }
        if start > 0 {
            self.buf.drain(0..start);
        }
    }

    /// Handle one full line (without its terminator).
    fn process_line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            // Blank line: dispatch the pending event (if any fields were set).
            if self.have_fields {
                self.dispatch(out);
            }
            return;
        }
        if let Some(stripped) = line.strip_prefix(':') {
            // Comment line (keep-alive). Ignored. Note we don't set have_fields.
            let _ = stripped;
            return;
        }
        // field[: value]
        let (field, value) = match line.find(':') {
            Some(idx) => {
                let field = &line[..idx];
                // One optional leading space after the colon is removed.
                let rest = &line[idx + 1..];
                let value = rest.strip_prefix(' ').unwrap_or(rest);
                (field, value)
            }
            None => (line, ""),
        };
        match field {
            "event" => {
                self.event_type = value.to_string();
                self.have_fields = true;
            }
            "data" => {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.have_fields = true;
            }
            // id / retry / anything else: parsed but ignored (still counts as a
            // field so an event carrying only such a line would still dispatch,
            // matching EventSource — harmless for these providers).
            _ => {
                self.have_fields = true;
            }
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        let event = std::mem::take(&mut self.event_type);
        let data = std::mem::take(&mut self.data);
        self.have_fields = false;
        out.push(SseEvent { event, data });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(parser: &mut SseParser, bytes: &[u8]) -> Vec<SseEvent> {
        let mut v = parser.push(bytes);
        v.extend(parser.finish());
        v
    }

    #[test]
    fn simple_data_event() {
        let mut p = SseParser::new();
        let ev = all(&mut p, b"data: hello\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                event: String::new(),
                data: "hello".into()
            }]
        );
    }

    #[test]
    fn multi_line_data_joined_with_newline() {
        let mut p = SseParser::new();
        let ev = all(&mut p, b"data: a\ndata: b\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                event: String::new(),
                data: "a\nb".into()
            }]
        );
    }

    #[test]
    fn named_event() {
        let mut p = SseParser::new();
        let ev = all(&mut p, b"event: ping\ndata: {}\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                event: "ping".into(),
                data: "{}".into()
            }]
        );
    }

    #[test]
    fn comment_lines_ignored() {
        let mut p = SseParser::new();
        let ev = all(&mut p, b": keep-alive\n\ndata: x\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                event: String::new(),
                data: "x".into()
            }]
        );
    }

    #[test]
    fn byte_at_a_time_equivalent() {
        let wire = b"event: e\ndata: line1\ndata: line2\n\ndata: second\n\n";
        let mut whole = SseParser::new();
        let want = all(&mut whole, wire);

        let mut p = SseParser::new();
        let mut got = Vec::new();
        for b in wire {
            got.extend(p.push(std::slice::from_ref(b)));
        }
        got.extend(p.finish());
        assert_eq!(got, want);
    }

    #[test]
    fn crlf_split_across_pushes() {
        let mut p = SseParser::new();
        let mut got = p.push(b"data: hi\r");
        got.extend(p.push(b"\n\r\n"));
        got.extend(p.finish());
        assert_eq!(
            got,
            vec![SseEvent {
                event: String::new(),
                data: "hi".into()
            }]
        );
    }

    #[test]
    fn no_space_after_colon() {
        let mut p = SseParser::new();
        let ev = all(&mut p, b"data:hello\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                event: String::new(),
                data: "hello".into()
            }]
        );
    }

    #[test]
    fn finish_flushes_unterminated_final_event() {
        let mut p = SseParser::new();
        let mut got = p.push(b"data: tail");
        got.extend(p.finish());
        assert_eq!(
            got,
            vec![SseEvent {
                event: String::new(),
                data: "tail".into()
            }]
        );
    }

    #[test]
    fn mid_utf8_split_is_safe() {
        // "é" is 0xC3 0xA9. Split the two bytes across pushes.
        let mut p = SseParser::new();
        let mut got = p.push(b"data: \xc3");
        got.extend(p.push(b"\xa9\n\n"));
        got.extend(p.finish());
        assert_eq!(
            got,
            vec![SseEvent {
                event: String::new(),
                data: "é".into()
            }]
        );
    }
}
