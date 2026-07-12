//! Port of v4 `lib/mount-index/chunker.ts` — split a plain-text document into
//! overlapping chunks suitable for embedding.
//!
//! Every string operation preserves v4's JavaScript semantics: `.length` is the
//! UTF-16 code-unit length (`jsstr::utf16_len`), `.trim()` uses the JS whitespace
//! set (`jsstr::js_trim`), and `.slice(...)` / `.indexOf(...)` operate on UTF-16
//! offsets. The paragraph/heading regexes are recreated with the JS whitespace
//! class so heading detection matches byte-for-byte.

use crate::jsstr::{is_js_ws, js_index_of, js_trim, utf16_len, JS_WS_CLASS};
use regex::Regex;
use std::sync::LazyLock;

/// v4 `CHARS_PER_TOKEN`.
const CHARS_PER_TOKEN: usize = 4;

/// A single emitted chunk (v4 `ChunkResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkResult {
    /// The text content of this chunk (may include overlap from the previous chunk).
    pub content: String,
    /// Estimated token count for this chunk.
    pub token_count: i64,
    /// Zero-based index of this chunk within the document.
    pub chunk_index: i64,
    /// The most recent markdown heading above this chunk, or `None`.
    pub heading_context: Option<String>,
}

/// Chunking options (v4 `ChunkOptions`); `None` fields fall back to the defaults
/// (min 800 / max 1200 / overlap 200 tokens).
#[derive(Debug, Clone, Copy, Default)]
pub struct ChunkOptions {
    pub target_min_tokens: Option<i64>,
    pub target_max_tokens: Option<i64>,
    pub overlap_tokens: Option<i64>,
}

/// v4 `estimateTokens`: rough token estimate, 1 token ≈ 4 characters where
/// "characters" is the JS `String.length` (UTF-16 code units) and the divide is
/// `Math.ceil`.
pub fn estimate_tokens(text: &str) -> i64 {
    (utf16_len(text) as f64 / CHARS_PER_TOKEN as f64).ceil() as i64
}

/// Markdown-style heading matcher (v4 `HEADING_RE = /^(#{1,6})\s+(.+)$/`). Built
/// with the JS whitespace class for `\s+` and JS line-terminator exclusion for
/// `.` so the port matches v4 exactly.
static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(#{{1,6}}){}+([^\n\r\u{{2028}}\u{{2029}}]+)$",
        JS_WS_CLASS
    ))
    .expect("heading regex")
});

/// Extract the heading text from a single line, or `None`. Mirrors
/// `line.match(HEADING_RE)` then `match[2].trim()`.
fn heading_of_line(line: &str) -> Option<String> {
    HEADING_RE
        .captures(line)
        .map(|c| js_trim(c.get(2).unwrap().as_str()).to_string())
}

/// Split on runs of two or more newlines (v4 `text.split(/\n{2,}/)`).
static PARA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{2,}").expect("para regex"));

/// v4 `splitAtSentences`: split after sentence-ending punctuation (`.` `?` `!`)
/// followed by whitespace, keeping each sentence separate and dropping empties.
///
/// v4 uses `text.split(/(?<=[.?!])\s+/)`. Rust's `regex` crate has no lookbehind,
/// so this walks the string: a run of JS whitespace whose immediately-preceding
/// character is `.`/`?`/`!` is a split boundary (and is consumed).
fn split_at_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_js_ws(c) {
            // Gather the full whitespace run.
            let run_start = i;
            while i < chars.len() && is_js_ws(chars[i]) {
                i += 1;
            }
            // Split boundary iff the char before the run is sentence-ending.
            let prev = if run_start > 0 {
                Some(chars[run_start - 1])
            } else {
                None
            };
            if matches!(prev, Some('.') | Some('?') | Some('!')) {
                parts.push(std::mem::take(&mut current));
            } else {
                // Not a boundary: the whitespace stays in the current piece.
                for &w in &chars[run_start..i] {
                    current.push(w);
                }
            }
        } else {
            current.push(c);
            i += 1;
        }
    }
    parts.push(current);
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

/// v4 `splitAtWords`: split on JS whitespace runs, dropping empties.
fn split_at_words(text: &str) -> Vec<String> {
    text.split(is_js_ws)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// v4 `assemblePieces`: greedily assemble small pieces into blocks under
/// `max_chars` (UTF-16 length), joining with a single space.
fn assemble_pieces(pieces: &[String], max_chars: usize) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        let candidate = if current.is_empty() {
            piece.clone()
        } else {
            format!("{current} {piece}")
        };
        if utf16_len(&candidate) > max_chars && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
            current = piece.clone();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// v4 `breakOversizedBlock`: break one oversized paragraph into pieces each
/// under `max_chars`. Tries sentence boundaries, then word boundaries, then a
/// hard UTF-16 character split.
fn break_oversized_block(text: &str, max_chars: usize) -> Vec<String> {
    if utf16_len(text) <= max_chars {
        return vec![text.to_string()];
    }
    let sentences = split_at_sentences(text);
    if sentences.len() > 1 {
        return assemble_pieces(&sentences, max_chars);
    }
    let words = split_at_words(text);
    if words.len() > 1 {
        return assemble_pieces(&words, max_chars);
    }
    // Absolute fallback: hard split by UTF-16 code-unit count.
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut pieces: Vec<String> = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let end = (i + max_chars).min(units.len());
        pieces.push(String::from_utf16_lossy(&units[i..end]));
        i += max_chars;
    }
    pieces
}

/// v4 `extractOverlap`: take the last `chars` UTF-16 units of `content`, then
/// trim forward to the next word boundary.
fn extract_overlap(content: &str, chars: usize) -> String {
    let len = utf16_len(content);
    if chars == 0 || len == 0 {
        return String::new();
    }
    if len <= chars {
        return content.to_string();
    }
    // `content.slice(-chars)` == units [len - chars ..].
    let tail_units: Vec<u16> = content.encode_utf16().skip(len - chars).collect();
    let tail = String::from_utf16_lossy(&tail_units);
    let first_space = js_index_of(&tail, " ", 0);
    if let Some(idx) = first_space {
        // JS: firstSpace > 0 && firstSpace < tail.length - 1
        if idx > 0 && idx < utf16_len(&tail) - 1 {
            let after: Vec<u16> = tail.encode_utf16().skip(idx + 1).collect();
            return String::from_utf16_lossy(&after);
        }
    }
    tail
}

/// Push a trimmed chunk (v4 inner `flushChunk`).
fn flush_chunk(chunks: &mut Vec<ChunkResult>, content: &str, heading: &Option<String>) {
    let trimmed = js_trim(content);
    if trimmed.is_empty() {
        return;
    }
    let idx = chunks.len() as i64;
    chunks.push(ChunkResult {
        content: trimmed.to_string(),
        token_count: estimate_tokens(trimmed),
        chunk_index: idx,
        heading_context: heading.clone(),
    });
}

/// Split a plain-text document into overlapping chunks (v4 `chunkDocument`).
pub fn chunk_document(text: &str, options: ChunkOptions) -> Vec<ChunkResult> {
    let target_max_tokens = options.target_max_tokens.unwrap_or(1200);
    let overlap_tokens = options.overlap_tokens.unwrap_or(200);

    let max_chars = (target_max_tokens as usize).saturating_mul(CHARS_PER_TOKEN);
    let overlap_chars = (overlap_tokens.max(0) as usize).saturating_mul(CHARS_PER_TOKEN);

    // Empty / whitespace-only input.
    if text.is_empty() || js_trim(text).is_empty() {
        return Vec::new();
    }

    let paragraphs: Vec<&str> = PARA_RE.split(text).collect();

    // Small document that fits in a single chunk.
    if estimate_tokens(text) <= target_max_tokens {
        let mut heading: Option<String> = None;
        for para in &paragraphs {
            for line in para.split('\n') {
                if let Some(h) = heading_of_line(line) {
                    heading = Some(h);
                }
            }
        }
        let trimmed = js_trim(text);
        return vec![ChunkResult {
            content: trimmed.to_string(),
            token_count: estimate_tokens(trimmed),
            chunk_index: 0,
            heading_context: heading,
        }];
    }

    // Greedy accumulation.
    let mut chunks: Vec<ChunkResult> = Vec::new();
    let mut current_content = String::new();
    let mut current_heading: Option<String> = None;
    let mut chunk_heading: Option<String> = None;

    for para in &paragraphs {
        let trimmed_para = js_trim(para);
        if trimmed_para.is_empty() {
            continue;
        }

        for line in trimmed_para.split('\n') {
            if let Some(h) = heading_of_line(line) {
                current_heading = Some(h);
            }
        }

        let blocks = if utf16_len(trimmed_para) > max_chars {
            break_oversized_block(trimmed_para, max_chars)
        } else {
            vec![trimmed_para.to_string()]
        };

        for block in &blocks {
            let candidate = if current_content.is_empty() {
                block.clone()
            } else {
                format!("{current_content}\n\n{block}")
            };

            if utf16_len(&candidate) > max_chars && !current_content.is_empty() {
                flush_chunk(&mut chunks, &current_content, &chunk_heading);
                let overlap = extract_overlap(&current_content, overlap_chars);
                current_content = if !overlap.is_empty() {
                    format!("{overlap}\n\n{block}")
                } else {
                    block.clone()
                };
                chunk_heading = current_heading.clone();
            } else {
                current_content = candidate;
                if chunk_heading.is_none() {
                    chunk_heading = current_heading.clone();
                }
            }
        }
    }

    if !js_trim(&current_content).is_empty() {
        flush_chunk(&mut chunks, &current_content, &chunk_heading);
    }

    chunks
}
