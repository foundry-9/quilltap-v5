//! Unified diff helpers — v4 `lib/doc-edit/unified-diff.ts`.
//!
//! A real, minimal, git-style unified diff: a Myers shortest-edit script over
//! lines (see [`crate::doc_edit::line_diff`]), grouped into hunks with
//! surrounding context, so the output reads the way a `diff` is expected to read
//! (matched lines stay put, only the genuinely changed lines get `-`/`+`
//! markers, and nearby edits coalesce into a single hunk). Used for autosave
//! notifications and document-mode change summaries.
//!
//! Lines are split on `\n` only (JS `split('\n')` keeps any `\r`) and compared
//! by whole-string equality, so there is no UTF-16/offset subtlety.

use super::line_diff::{diff_lines, LineOp, LineOpType};

/// Lines of unchanged context kept on each side of a change, matching git's default.
const CONTEXT_LINES: usize = 3;

/// Guard against pathological inputs: the Myers trace is O(D · (N+M)) memory,
/// which is tiny for the common "lightly edited document" case (small D) but can
/// blow up when two very large, wholly-dissimilar files are compared. Past this
/// combined line count we fall back to a coarse whole-file replacement hunk,
/// which is still a valid diff — just not minimal.
const MAX_DIFFABLE_LINES: usize = 10000;

/// Format one side of a hunk header (`start,count`), matching git's conventions.
/// `start` is a 1-based line number (`>= 1` on every path), so `start - 1` never
/// underflows.
fn format_range(start: usize, count: usize) -> String {
    if count == 0 {
        // Empty range: git points at the line *before* the change position.
        return format!("{},0", start - 1);
    }
    if count == 1 {
        return format!("{start}");
    }
    format!("{start},{count}")
}

/// An op annotated with the 1-based line number it starts at on each side.
struct EnrichedOp<'a> {
    op: LineOp<'a>,
    old_no: usize,
    new_no: usize,
}

/// Group an edit script into unified-diff hunks, each carrying up to
/// `CONTEXT_LINES` of unchanged context on either side, coalescing changes that
/// fall within one context window of each other.
fn build_hunks(ops: &[LineOp]) -> Vec<String> {
    if !ops.iter().any(|op| op.op_type != LineOpType::Equal) {
        return Vec::new();
    }

    // Annotate each op with the 1-based line number it starts at on each side.
    let mut enriched: Vec<EnrichedOp> = Vec::with_capacity(ops.len());
    let mut old_no = 1usize;
    let mut new_no = 1usize;
    for &op in ops {
        enriched.push(EnrichedOp { op, old_no, new_no });
        match op.op_type {
            LineOpType::Equal => {
                old_no += 1;
                new_no += 1;
            }
            LineOpType::Del => old_no += 1,
            LineOpType::Ins => new_no += 1,
        }
    }

    // Locate maximal runs of changed ops, expand each by context, then merge runs
    // whose expanded ranges touch. Ranges are inclusive `(start, end)` indices.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < enriched.len() {
        if enriched[i].op.op_type == LineOpType::Equal {
            i += 1;
            continue;
        }
        let change_start = i;
        while i < enriched.len() && enriched[i].op.op_type != LineOpType::Equal {
            i += 1;
        }
        let change_end = i - 1;
        let start = change_start.saturating_sub(CONTEXT_LINES);
        let end = (change_end + CONTEXT_LINES).min(enriched.len() - 1);

        let extend = matches!(merged.last(), Some(last) if start <= last.1 + 1);
        if extend {
            let last = merged.last_mut().unwrap();
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }

    let mut hunks: Vec<String> = Vec::new();
    for (group_start, group_end) in merged {
        let slice = &enriched[group_start..=group_end];
        let old_start = slice[0].old_no;
        let new_start = slice[0].new_no;
        let mut old_count = 0usize;
        let mut new_count = 0usize;
        let mut body: Vec<String> = Vec::new();
        for e in slice {
            match e.op.op_type {
                LineOpType::Equal => {
                    old_count += 1;
                    new_count += 1;
                    body.push(format!(" {}", e.op.line));
                }
                LineOpType::Del => {
                    old_count += 1;
                    body.push(format!("-{}", e.op.line));
                }
                LineOpType::Ins => {
                    new_count += 1;
                    body.push(format!("+{}", e.op.line));
                }
            }
        }
        hunks.push(format!(
            "@@ -{} +{} @@",
            format_range(old_start, old_count),
            format_range(new_start, new_count)
        ));
        hunks.extend(body);
    }

    hunks
}

/// Coarse fallback for oversized inputs: replace the whole file in one hunk.
fn whole_file_hunk(old_lines: &[&str], new_lines: &[&str]) -> Vec<String> {
    if old_lines.is_empty() && new_lines.is_empty() {
        return Vec::new();
    }
    let mut hunks: Vec<String> = vec![format!(
        "@@ -{} +{} @@",
        format_range(1, old_lines.len()),
        format_range(1, new_lines.len())
    )];
    for line in old_lines {
        hunks.push(format!("-{line}"));
    }
    for line in new_lines {
        hunks.push(format!("+{line}"));
    }
    hunks
}

/// Generate a unified diff between two strings (v4 `generateUnifiedDiff`).
/// Produces git-style output with `@@` hunk headers and surrounding context;
/// `""` when the two inputs are identical.
pub fn generate_unified_diff(old_text: &str, new_text: &str, filename: &str) -> String {
    if old_text == new_text {
        return String::new();
    }

    // Treat truly-empty content as zero lines (git's 0-byte-file semantics) rather
    // than as `[""]`, so creating/emptying a file doesn't churn a phantom blank line.
    let old_lines: Vec<&str> = if old_text.is_empty() {
        Vec::new()
    } else {
        old_text.split('\n').collect()
    };
    let new_lines: Vec<&str> = if new_text.is_empty() {
        Vec::new()
    } else {
        new_text.split('\n').collect()
    };

    let hunks = if old_lines.len() + new_lines.len() > MAX_DIFFABLE_LINES {
        whole_file_hunk(&old_lines, &new_lines)
    } else {
        build_hunks(&diff_lines(&old_lines, &new_lines))
    };

    if hunks.is_empty() {
        return String::new();
    }

    format!("--- a/{filename}\n+++ b/{filename}\n{}", hunks.join("\n"))
}

/// Wrap a unified diff in an autosave notification (v4 `formatAutosaveNotification`).
/// `None` when the texts are identical (empty diff).
pub fn format_autosave_notification(
    old_text: &str,
    new_text: &str,
    filename: &str,
) -> Option<String> {
    let diff = generate_unified_diff(old_text, new_text, filename);
    if diff.is_empty() {
        return None;
    }
    Some(format!(
        "I've made changes to \"{filename}\":\n\n```diff\n{diff}\n```"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_empty() {
        assert_eq!(
            generate_unified_diff("same\ntext", "same\ntext", "f.md"),
            ""
        );
        assert_eq!(generate_unified_diff("", "", "f.md"), "");
        assert_eq!(format_autosave_notification("x", "x", "f.md"), None);
    }

    #[test]
    fn minimal_hunk_context_not_churn() {
        let d = generate_unified_diff(
            "one\ntwo\nthree\nfour\nfive",
            "one\ntwo\nTHREE\nfour\nfive",
            "draft.md",
        );
        let body: Vec<&str> = d.split('\n').skip(2).collect();
        assert_eq!(
            body,
            vec![
                "@@ -1,5 +1,5 @@",
                " one",
                " two",
                "-three",
                "+THREE",
                " four",
                " five",
            ]
        );
    }

    #[test]
    fn create_from_empty_zero_old_range() {
        let d = generate_unified_diff("", "hello\nworld", "draft.md");
        assert!(d.contains("@@ -0,0 +1,2 @@"), "{d}");
        assert!(d.contains("+hello"));
        assert!(d.contains("+world"));
    }

    #[test]
    fn distant_changes_split_into_two_hunks() {
        let lines: Vec<String> = (1..=20).map(|i| format!("line{i}")).collect();
        let mut changed = lines.clone();
        changed[1] = "TOP".into();
        changed[18] = "BOTTOM".into();
        let d = generate_unified_diff(&lines.join("\n"), &changed.join("\n"), "draft.md");
        let headers = d.split('\n').filter(|l| l.starts_with("@@")).count();
        assert_eq!(headers, 2, "{d}");
    }
}
