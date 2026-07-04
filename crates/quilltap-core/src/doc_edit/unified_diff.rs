//! Unified diff helpers — v4 `lib/doc-edit/unified-diff.ts`.
//!
//! A **hand-rolled** line diff (NOT a library) that produces git-style `@@`
//! hunks, used for autosave notifications and document-mode change summaries.
//! Ported faithfully — the exact greedy look-ahead algorithm (window 3), not "a"
//! unified diff — so the hunk output matches v4 byte-for-byte. Lines are split on
//! `\n` only (JS `split('\n')` keeps any `\r`), and compared by whole-string
//! equality, so there is no UTF-16/offset subtlety.

/// Generate a simple unified diff between two strings (v4 `generateUnifiedDiff`).
/// Produces output similar to `git diff` with `@@` line markers; `""` when the
/// texts are identical.
// The two look-ahead scans need the index `k` itself (it becomes `found_in_*`),
// so an index loop is the faithful transcription of v4's algorithm.
#[allow(clippy::needless_range_loop)]
pub fn generate_unified_diff(old_text: &str, new_text: &str, filename: &str) -> String {
    let old_lines: Vec<&str> = old_text.split('\n').collect();
    let new_lines: Vec<&str> = new_text.split('\n').collect();
    let mut hunks: Vec<String> = Vec::new();

    let mut i = 0usize;
    let mut j = 0usize;
    let look_ahead = 3usize;

    while i < old_lines.len() || j < new_lines.len() {
        if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
            i += 1;
            j += 1;
            continue;
        }

        let hunk_start_old = i + 1;
        let hunk_start_new = j + 1;
        let mut removed_lines: Vec<&str> = Vec::new();
        let mut added_lines: Vec<&str> = Vec::new();

        while i < old_lines.len() || j < new_lines.len() {
            if i < old_lines.len() && j < new_lines.len() {
                // Where does oldLines[i] next appear in the new window?
                let mut found_in_new: isize = -1;
                for k in j..(j + look_ahead).min(new_lines.len()) {
                    if old_lines[i] == new_lines[k] {
                        found_in_new = k as isize;
                        break;
                    }
                }
                // Where does newLines[j] next appear in the old window?
                let mut found_in_old: isize = -1;
                for k in i..(i + look_ahead).min(old_lines.len()) {
                    if new_lines[j] == old_lines[k] {
                        found_in_old = k as isize;
                        break;
                    }
                }

                if found_in_new == j as isize && found_in_old == i as isize {
                    break;
                }

                if found_in_new >= 0
                    && (found_in_old < 0 || found_in_new - j as isize <= found_in_old - i as isize)
                {
                    while (j as isize) < found_in_new {
                        added_lines.push(new_lines[j]);
                        j += 1;
                    }
                    removed_lines.push(old_lines[i]);
                    i += 1;
                    continue;
                }

                if found_in_old >= 0 {
                    while (i as isize) < found_in_old {
                        removed_lines.push(old_lines[i]);
                        i += 1;
                    }
                    added_lines.push(new_lines[j]);
                    j += 1;
                    continue;
                }
            }

            if i < old_lines.len() {
                removed_lines.push(old_lines[i]);
                i += 1;
            }
            if j < new_lines.len() {
                added_lines.push(new_lines[j]);
                j += 1;
            }

            if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
                break;
            }
        }

        if !removed_lines.is_empty() || !added_lines.is_empty() {
            hunks.push(format!(
                "@@ -{},{} +{},{} @@",
                hunk_start_old,
                removed_lines.len(),
                hunk_start_new,
                added_lines.len()
            ));
            for line in &removed_lines {
                hunks.push(format!("-{line}"));
            }
            for line in &added_lines {
                hunks.push(format!("+{line}"));
            }
        }
    }

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
        assert_eq!(generate_unified_diff("a\nb\n", "a\nb\n", "f.md"), "");
        assert_eq!(format_autosave_notification("x", "x", "f.md"), None);
    }

    #[test]
    fn single_line_change() {
        let d = generate_unified_diff("a\nb\nc\n", "a\nB\nc\n", "f.md");
        assert_eq!(d, "--- a/f.md\n+++ b/f.md\n@@ -2,1 +2,1 @@\n-b\n+B");
    }
}
