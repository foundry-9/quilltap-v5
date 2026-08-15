//! Inline `<think>` block recognition for Ollama responses (P4.D78, the port of
//! v4 `plugins/dist/qtap-plugin-ollama/think-parser.ts`, added by `d9c5a1c7`).
//!
//! Ollama separates a thinking model's reasoning into `message.thinking` when it
//! recognises the model's template, but plenty of community GGUF imports (and
//! older Ollama versions) leak the raw `<think>...</think>` block straight into
//! `message.content`. [`ThinkTagStreamParser`] routes that text into the
//! reasoning channel instead of the visible message, surviving tags that
//! straddle streaming chunk boundaries.
//!
//! Three mechanics carry over exactly and are the whole subtlety:
//!
//! - **the partial-tag holdback** — at a chunk boundary the longest strict
//!   prefix of a tag that the pending text ends with is HELD, not emitted, until
//!   the next push reveals whether it completes;
//! - **the swallowed-opening-tag rule** — a `</think>` arriving before any think
//!   block AND before any visible output means everything ahead of it is
//!   reasoning (v4's `orphanEligible = !sawThinkBlock && !emittedVisible`); once
//!   real content has been emitted the rule is off the table, so a stray close
//!   tag in an ordinary answer stays put;
//! - **the sanitize rule** — leading whitespace between a consumed think block
//!   and the first visible character is dropped, but ONLY while nothing visible
//!   has been emitted and a think block was actually seen, so a no-think
//!   response passes through byte-for-byte.
//!
//! ## JS-fidelity notes
//!
//! v4 indexes with `String.prototype.indexOf`/`slice` over UTF-16 code units;
//! this port indexes BYTES. The two agree here because both tags are pure ASCII:
//! every index this code takes is either a `find` hit on an ASCII needle or a
//! suffix that matched an ASCII tag prefix, so every split point is a char
//! boundary and every held length is the same number in either measure. The
//! leading-whitespace strip uses [`crate::jsstr::js_trim_start`] (JS `\s`, which
//! is not Rust's `char::is_whitespace`), matching v4's `replace(/^\s+/, '')`.
//!
//! Differential: `quilltap-harness/tests/ollama_think_parser_equivalence.rs`
//! (tier 1, exact) drives v4's REAL `think-parser.ts` over the same committed
//! case table.

use crate::jsstr::js_trim_start;

const OPEN_TAG: &str = "<think>";
const CLOSE_TAG: &str = "</think>";

/// Length (in bytes) of the longest strict prefix of `tag` that `text` ends
/// with — v4's `partialTagSuffixLength`. Used to hold back a partial tag at the
/// end of a streaming chunk until the next chunk reveals whether it completes.
fn partial_tag_suffix_length(text: &str, tag: &str) -> usize {
    let max = std::cmp::min(text.len(), tag.len() - 1);
    for k in (1..=max).rev() {
        if text.as_bytes().ends_with(&tag.as_bytes()[..k]) {
            return k;
        }
    }
    0
}

/// Stateful streaming splitter: feed content deltas through [`push`], which
/// returns the text safe to display; anything inside `<think>...</think>`
/// accumulates on [`reasoning`]. Call [`flush`] once the stream ends to release
/// any held-back partial tag (an unterminated think block counts as reasoning).
///
/// [`push`]: ThinkTagStreamParser::push
/// [`flush`]: ThinkTagStreamParser::flush
/// [`reasoning`]: ThinkTagStreamParser::reasoning
#[derive(Default, Debug)]
pub struct ThinkTagStreamParser {
    pending: String,
    in_think: bool,
    saw_think_block: bool,
    emitted_visible: bool,
    reasoning_text: String,
}

impl ThinkTagStreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reasoning captured so far (raw, monotonically growing).
    pub fn reasoning(&self) -> &str {
        &self.reasoning_text
    }

    /// Feed a content delta; returns the displayable text it releases.
    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        let mut out = String::new();
        loop {
            if self.in_think {
                if let Some(close) = self.pending.find(CLOSE_TAG) {
                    self.reasoning_text.push_str(&self.pending[..close]);
                    self.pending = self.pending[close + CLOSE_TAG.len()..].to_string();
                    self.in_think = false;
                    continue;
                }
                let hold = partial_tag_suffix_length(&self.pending, CLOSE_TAG);
                let keep = self.pending.len() - hold;
                self.reasoning_text.push_str(&self.pending[..keep]);
                self.pending = self.pending[keep..].to_string();
                break;
            }
            let open = self.pending.find(OPEN_TAG);
            // Swallowed-opening-tag pattern: some templates driven in no-think
            // mode (Qwen3 GGUF imports, notably) let the model reason anyway and
            // Ollama eats the opening <think>, so the reasoning arrives untagged
            // with only the closing tag surviving. A close tag showing up before
            // any think block and before any visible output means everything
            // ahead of it is reasoning, not content. Once real content has been
            // emitted the rule is off the table — a stray close tag in an
            // ordinary answer stays put.
            let orphan_eligible = !self.saw_think_block && !self.emitted_visible;
            if orphan_eligible {
                if let Some(close) = self.pending.find(CLOSE_TAG) {
                    if open.is_none_or(|o| close < o) {
                        self.reasoning_text.push_str(&out);
                        self.reasoning_text.push_str(&self.pending[..close]);
                        out.clear();
                        self.pending = self.pending[close + CLOSE_TAG.len()..].to_string();
                        self.saw_think_block = true;
                        continue;
                    }
                }
            }
            if let Some(open) = open {
                out.push_str(&self.pending[..open]);
                self.pending = self.pending[open + OPEN_TAG.len()..].to_string();
                self.in_think = true;
                self.saw_think_block = true;
                continue;
            }
            let hold_open = partial_tag_suffix_length(&self.pending, OPEN_TAG);
            let hold_close = if orphan_eligible {
                partial_tag_suffix_length(&self.pending, CLOSE_TAG)
            } else {
                0
            };
            let hold = std::cmp::max(hold_open, hold_close);
            let keep = self.pending.len() - hold;
            out.push_str(&self.pending[..keep]);
            self.pending = self.pending[keep..].to_string();
            break;
        }
        self.sanitize(out)
    }

    /// Release whatever is still held. Call exactly once, at end of stream.
    pub fn flush(&mut self) -> String {
        let tail = std::mem::take(&mut self.pending);
        if self.in_think {
            // Unterminated think block — the stream ended mid-thought.
            self.reasoning_text.push_str(&tail);
            return String::new();
        }
        self.sanitize(tail)
    }

    /// Drop the whitespace a template leaves between the think block and the
    /// real answer — but only when a think block was actually consumed, so
    /// ordinary responses come through untouched.
    fn sanitize(&mut self, text: String) -> String {
        if text.is_empty() {
            return text;
        }
        let mut text = text;
        if !self.emitted_visible && self.saw_think_block {
            text = js_trim_start(&text).to_string();
            if text.is_empty() {
                return text;
            }
        }
        self.emitted_visible = true;
        text
    }
}

/// The captured halves of a one-shot split (v4's `extractThinkBlocks` return).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkSplit {
    pub content: String,
    pub reasoning: String,
}

/// One-shot variant for non-streaming responses: split `content` into the
/// displayable text and the reasoning captured from `<think>` blocks (v4's
/// `extractThinkBlocks` — `push` + `flush` through one parser).
pub fn extract_think_blocks(content: &str) -> ThinkSplit {
    let mut parser = ThinkTagStreamParser::new();
    let mut out = parser.push(content);
    out.push_str(&parser.flush());
    ThinkSplit {
        content: out,
        reasoning: parser.reasoning_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_whole_response_into_content_and_reasoning() {
        let s = extract_think_blocks("<think>pondering deeply</think>\n\nHello there");
        assert_eq!(s.content, "Hello there");
        assert_eq!(s.reasoning, "pondering deeply");
    }

    #[test]
    fn think_free_response_passes_through_byte_for_byte() {
        let text = "  leading space kept\nand newlines too ";
        let s = extract_think_blocks(text);
        assert_eq!(s.content, text);
        assert_eq!(s.reasoning, "");
    }

    #[test]
    fn unterminated_think_block_is_reasoning() {
        let s = extract_think_blocks("<think>cut off mid-");
        assert_eq!(s.content, "");
        assert_eq!(s.reasoning, "cut off mid-");
    }

    #[test]
    fn leading_orphan_close_is_a_swallowed_think_block() {
        let s = extract_think_blocks(
            "Okay, let's see. The user wants JSON.\n</think>\n\n[\"cat\", \"dog\"]",
        );
        assert_eq!(s.content, "[\"cat\", \"dog\"]");
        assert_eq!(s.reasoning, "Okay, let's see. The user wants JSON.\n");
    }

    #[test]
    fn stray_close_after_a_real_block_stays_put() {
        let s = extract_think_blocks("<think>real</think>answer </think> tail");
        assert_eq!(s.content, "answer </think> tail");
        assert_eq!(s.reasoning, "real");
    }

    #[test]
    fn multiple_think_blocks() {
        let s = extract_think_blocks(
            "<think>first</think>Answer part one <think>second</think>and two",
        );
        assert_eq!(s.content, "Answer part one and two");
        assert_eq!(s.reasoning, "firstsecond");
    }

    #[test]
    fn chopping_does_not_change_the_result() {
        let text = "Intro <think>some\nreasoning</think> and the answer<think>more</think>!";
        for size in [1usize, 2, 3, 5, 7, 11] {
            let mut parser = ThinkTagStreamParser::new();
            let mut content = String::new();
            let bytes: Vec<char> = text.chars().collect();
            let mut i = 0;
            while i < bytes.len() {
                let end = std::cmp::min(i + size, bytes.len());
                let piece: String = bytes[i..end].iter().collect();
                content.push_str(&parser.push(&piece));
                i = end;
            }
            content.push_str(&parser.flush());
            assert_eq!(content, "Intro  and the answer!", "size {size}");
            assert_eq!(parser.reasoning(), "some\nreasoningmore", "size {size}");
        }
    }

    #[test]
    fn a_partial_tag_that_never_completes_flushes_as_content() {
        let mut parser = ThinkTagStreamParser::new();
        let mut content = parser.push("half a tag <thi");
        assert_eq!(content, "half a tag ");
        content.push_str(&parser.push("rd of the way"));
        content.push_str(&parser.flush());
        assert_eq!(content, "half a tag <third of the way");
        assert_eq!(parser.reasoning(), "");
    }
}
