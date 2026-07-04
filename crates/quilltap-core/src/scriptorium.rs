//! Project Scriptorium — the pure annotation-merge/strip leaves.
//!
//! Ports v4's `lib/scriptorium/annotation-merger.ts`
//! (`mergeAnnotations` / `stripAnnotations`), the two pure text transforms the
//! `read_conversation` tool composes over a chat's rendered Markdown. Everything
//! here is deterministic and DB-free (the differential proof is tier-1 exact).
//!
//! ## `merge_annotations`
//!
//! Walks the rendered Markdown line-by-line; after each `### Message {n}` section
//! (and its content) it inserts any annotation whose `messageIndex` matches, as a
//! fenced ```` ```annotation:{characterName} ```` block. Multiple annotations on
//! the same message are emitted sorted by `characterName` — and v4 sorts with
//! `String.prototype.localeCompare` (the default-locale ICU collation), so the
//! port uses [`crate::collation::locale_compare`] (ICU4X en-US/tertiary, the
//! resolved collation seam — this is a fresh site, NOT the code-unit vault seam).
//!
//! The message-header pattern is `/^### Message (\d+)/m` matched **per line**
//! (v4 splits on `\n` first, then tests each line), so only a header at the
//! **start** of a line counts; `\d+` is JS `\d` = ASCII `[0-9]`, and
//! `parseInt(match[1], 10)` parses the run of digits. The grouping map keys on
//! that integer index; an annotation whose index never appears as a header is
//! simply never flushed (dropped), matching v4.
//!
//! ## `strip_annotations`
//!
//! Removes every annotation fenced block, then collapses runs of 3+ newlines to
//! exactly two. v4's regex is
//! `/^```annotation:[^\n]*\n[\s\S]*?^```\n?/gm` (global + multiline): a line
//! starting with ```` ```annotation: ````, its trailing chars to end-of-line, then
//! the shortest span through a ```` ``` ```` that itself starts a line, plus an
//! optional trailing newline. The Rust `regex` crate reproduces this: `(?m)`
//! multiline, `[^\n]` unchanged, `[\s\S]*?` → `(?s:.)*?` (any char incl. newline,
//! lazy), the mid-pattern `^` line anchor, and the optional `\n?`.

use crate::collation::locale_compare;
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// A conversation annotation, in the shape `merge_annotations` consumes (v4's
/// `ConversationAnnotation` subset: the `messageIndex` it groups on plus the
/// `characterName`/`content` it renders). The full DB row carries more columns,
/// but the merge only ever reads these three.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub message_index: i64,
    pub character_name: String,
    pub content: String,
}

/// The lazily-compiled annotation-stripping regex (v4's
/// `/^```annotation:[^\n]*\n[\s\S]*?^```\n?/gm`).
static ANNOTATION_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    // `(?m)` = multiline (JS `/m`); `(?s:.)` = JS `[\s\S]` (any char incl. `\n`).
    Regex::new(r"(?m)^```annotation:[^\n]*\n(?s:.)*?^```\n?").unwrap()
});

/// The run-of-3+-newlines collapse regex (v4's `/\n{3,}/g`).
static NEWLINE_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

/// The message-header pattern (v4's `/^### Message (\d+)/m`), applied per line.
static MESSAGE_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^### Message ([0-9]+)").unwrap());

/// Merge annotations into rendered conversation Markdown (v4 `mergeAnnotations`).
///
/// Returns `markdown` unchanged when `annotations` is empty. Otherwise groups
/// annotations by `message_index` (each group sorted by `character_name` via the
/// default-locale collation), then walks the markdown lines, flushing a message's
/// annotations after its section as fenced blocks.
pub fn merge_annotations(markdown: &str, annotations: &[Annotation]) -> String {
    if annotations.is_empty() {
        return markdown.to_string();
    }

    // Group by messageIndex; BTreeMap only for `has`/`get` — insertion order does
    // not affect output (each group is re-sorted below; v4 uses a `Map` the same
    // way).
    let mut by_index: BTreeMap<i64, Vec<&Annotation>> = BTreeMap::new();
    for ann in annotations {
        by_index.entry(ann.message_index).or_default().push(ann);
    }
    for group in by_index.values_mut() {
        group.sort_by(|a, b| locale_compare(&a.character_name, &b.character_name));
    }

    // Split on `\n` exactly as v4 (`markdown.split('\n')`); iterate lines, tracking
    // the current message index. Flush the previous message's annotations before a
    // new header, then flush the last message's at the end.
    let lines: Vec<&str> = markdown.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut current_message_index: Option<i64> = None;

    let flush = |result: &mut Vec<String>, idx: i64, by_index: &BTreeMap<i64, Vec<&Annotation>>| {
        if let Some(anns) = by_index.get(&idx) {
            for ann in anns {
                result.push(format!("```annotation:{}", ann.character_name));
                result.push(ann.content.clone());
                result.push("```".to_string());
                result.push(String::new());
            }
        }
    };

    for line in &lines {
        if let Some(caps) = MESSAGE_HEADER.captures(line) {
            if let Some(idx) = current_message_index {
                flush(&mut result, idx, &by_index);
            }
            // `parseInt(match[1], 10)` — the captured ASCII digit run.
            current_message_index = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
        }
        result.push((*line).to_string());
    }
    if let Some(idx) = current_message_index {
        flush(&mut result, idx, &by_index);
    }

    result.join("\n")
}

/// Strip all annotation fenced blocks from Markdown, then collapse 3+ newline runs
/// to two (v4 `stripAnnotations`).
pub fn strip_annotations(markdown: &str) -> String {
    let without_blocks = ANNOTATION_BLOCK.replace_all(markdown, "");
    NEWLINE_RUN
        .replace_all(&without_blocks, "\n\n")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(idx: i64, name: &str, content: &str) -> Annotation {
        Annotation {
            message_index: idx,
            character_name: name.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn merge_empty_returns_unchanged() {
        let md = "### Message 0\nHello\n";
        assert_eq!(merge_annotations(md, &[]), md);
    }

    #[test]
    fn merge_single_annotation_after_message() {
        let md = "### Message 0\nHello\n\n### Message 1\nWorld\n";
        let out = merge_annotations(md, &[ann(0, "Alice", "nice")]);
        // Annotation flushed just before the next `### Message` header.
        assert!(out.contains("```annotation:Alice\nnice\n```"));
        // It appears after Message 0's content, before Message 1.
        let a_pos = out.find("annotation:Alice").unwrap();
        let m1_pos = out.find("### Message 1").unwrap();
        assert!(a_pos < m1_pos);
    }

    #[test]
    fn merge_flushes_last_message() {
        let md = "### Message 0\nHello\n";
        let out = merge_annotations(md, &[ann(0, "Bob", "last")]);
        assert!(out.contains("```annotation:Bob\nlast\n```"));
    }

    #[test]
    fn merge_multiple_sorted_by_character_name() {
        let md = "### Message 0\nHi\n";
        let out = merge_annotations(md, &[ann(0, "Zed", "z"), ann(0, "Amy", "a")]);
        let amy = out.find("annotation:Amy").unwrap();
        let zed = out.find("annotation:Zed").unwrap();
        assert!(amy < zed, "Amy should sort before Zed");
    }

    #[test]
    fn merge_index_never_seen_is_dropped() {
        let md = "### Message 0\nHi\n";
        let out = merge_annotations(md, &[ann(5, "Ghost", "boo")]);
        assert!(!out.contains("annotation:Ghost"));
    }

    #[test]
    fn strip_removes_block_and_collapses_newlines() {
        let md = "### Message 0\nHi\n\n```annotation:Alice\nnote\n```\n\n\n### Message 1\nBye\n";
        let out = strip_annotations(md);
        assert!(!out.contains("annotation:Alice"));
        assert!(!out.contains("note"));
        // 3+ newline runs collapse to exactly two.
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn strip_no_annotations_only_collapses() {
        let md = "a\n\n\n\nb";
        assert_eq!(strip_annotations(md), "a\n\nb");
    }
}
