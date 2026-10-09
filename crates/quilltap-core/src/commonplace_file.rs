//! The Commonplace vault files — paths, rendering, and reading back. v4
//! `lib/memory/commonplace-file.ts` (`d58548051`, 166 lines, pure; no I/O).
//!
//! After a consolidation run, each subject bucket's hot digests are mirrored
//! into the holder's vault (`memory-consolidation-and-tiers.md` §C6):
//!
//!   - `Commonplace/<Subject Name>.md` — what the holder knows about another
//!     character, digested;
//!   - `Commonplace/Self.md` — what the holder knows about themselves.
//!
//! The folder is deliberately separate from the hand-written `Others/` folder
//! so no user prose is ever overwritten; the files are rewritten whole on every
//! run. The extractor's canon loaders read them back as ALREADY ESTABLISHED,
//! which is the loop that stops a digested fact from being re-mined turn after
//! turn. The writer is P4.D271's (`commonplace_digest_vault_bridge`); the
//! readers P4.D270's.
//!
//! JS fidelity: lengths, slices and indices are UTF-16 code units; `\s` and
//! `trim` / `trimEnd` are the JS whitespace set ([`crate::jsstr`]); the sort is
//! STABLE (V8's `Array.prototype.sort`); the two frontmatter scalars are
//! `JSON.stringify`'d (`serde_json::to_string` writes the same escapes). A
//! UTF-16 cut through a surrogate pair leaves JS a lone surrogate that no Rust
//! `String` can hold — v5 cuts LOSSILY (U+FFFD), recorded by the
//! `commonplace_file_equivalence` oracle's `loneSurrogates` list. Tier-1 exact
//! against v4's REAL exports.

use crate::jsstr::{is_js_ws, js_trim, js_trim_end, utf16_truncate};
use crate::token_estimation::{estimate_tokens, DEFAULT_CHARS_PER_TOKEN};
use crate::vault_overlay::sanitize_file_name;

/// Vault folder holding the digest mirrors.
pub const COMMONPLACE_FOLDER: &str = "Commonplace";
/// Frontmatter `type` stamped on every digest mirror.
pub const COMMONPLACE_FRONTMATTER_TYPE: &str = "commonplace-digest";
/// Basename of the self-bucket mirror.
pub const COMMONPLACE_SELF_STEM: &str = "Self";
/// Token ceiling on a mirror file's body (spec: "≤ ~1,500 tokens").
pub const COMMONPLACE_FILE_TOKEN_BUDGET: i64 = 1500;

/// v4's `estimateTokens(text)` with no provider.
fn tokens(text: &str) -> i64 {
    estimate_tokens(text, DEFAULT_CHARS_PER_TOKEN)
}

/// The subject of one mirror — v4's `'self' | { id, name }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommonplaceSubject {
    /// The holder's own bucket → `Commonplace/Self.md`.
    SelfFile,
    /// Another character.
    Other { id: String, name: String },
}

/// v4 `commonplacePathFor(subject)` (`:42-52`): the vault-relative path of a
/// mirror. A character actually named "Self" (case-insensitively, after the
/// `Others/<name>.md` sanitizer) gets its id prefix appended so it can never
/// collide with the self file.
pub fn commonplace_path_for(subject: &CommonplaceSubject) -> String {
    match subject {
        CommonplaceSubject::SelfFile => {
            format!("{COMMONPLACE_FOLDER}/{COMMONPLACE_SELF_STEM}.md")
        }
        CommonplaceSubject::Other { id, name } => {
            let mut stem = sanitize_file_name(name);
            if stem.to_lowercase() == COMMONPLACE_SELF_STEM.to_lowercase() {
                stem = format!("{stem} ({})", utf16_truncate(id, 8));
            }
            format!("{COMMONPLACE_FOLDER}/{stem}.md")
        }
    }
}

/// One digest as the mirror renders it (v4 `CommonplaceDigestEntry`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommonplaceDigestEntry {
    pub content: String,
    /// `'semantic'` | `'episodic'`.
    pub kind: String,
    pub occurred_at: Option<String>,
    /// Ranking key — highest first.
    pub reinforced_importance: f64,
}

/// v4 `CommonplaceFileInput`.
#[derive(Debug, Clone, PartialEq)]
pub struct CommonplaceFileInput {
    /// The subject's character id (the holder's own id for the self file).
    pub subject_character_id: String,
    /// Display name for the heading; ignored for the self file.
    pub subject_name: String,
    pub is_self: bool,
    pub digests: Vec<CommonplaceDigestEntry>,
    /// ISO timestamp stamped as frontmatter `updatedAt`.
    pub updated_at: String,
}

/// v4 `renderCommonplaceFile`'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedCommonplace {
    pub content: String,
    pub entries_written: usize,
    pub entries_dropped: usize,
}

/// JS `content.replace(/\s*\n\s*/g, ' ')`: every maximal JS-whitespace run that
/// contains a `\n` becomes ONE space (the greedy `\s*` on either side of the
/// `\n` takes the whole run); a run without a `\n` is left alone.
fn collapse_line_breaks(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;
    while i < chars.len() {
        if is_js_ws(chars[i]) {
            let mut j = i;
            while j < chars.len() && is_js_ws(chars[j]) {
                j += 1;
            }
            if chars[i..j].contains(&'\n') {
                out.push(' ');
            } else {
                out.extend(&chars[i..j]);
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// v4's private `renderEntry` (`:74-80`).
fn render_entry(d: &CommonplaceDigestEntry) -> String {
    let collapsed = collapse_line_breaks(&d.content);
    let text = js_trim(&collapsed);
    match &d.occurred_at {
        Some(at) if d.kind == "episodic" && !at.is_empty() => {
            format!("- [{}] {text}", utf16_truncate(at, 10))
        }
        _ => format!("- {text}"),
    }
}

/// v4 `renderCommonplaceFile(input)` (`:88-121`): frontmatter, a heading, then
/// one bullet per digest, highest `reinforcedImportance` first (a STABLE sort),
/// stopping before the body would pass [`COMMONPLACE_FILE_TOKEN_BUDGET`] — the
/// FIRST entry is always written.
pub fn render_commonplace_file(input: &CommonplaceFileInput) -> RenderedCommonplace {
    let heading = if input.is_self {
        format!("# {COMMONPLACE_SELF_STEM}")
    } else {
        let name = js_trim(&input.subject_name);
        format!("# {}", if name.is_empty() { "Unnamed" } else { name })
    };
    let mut ordered: Vec<&CommonplaceDigestEntry> = input.digests.iter().collect();
    // `(a, b) => b.reinforcedImportance - a.reinforcedImportance`.
    ordered.sort_by(|a, b| {
        b.reinforced_importance
            .partial_cmp(&a.reinforced_importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<String> = vec![heading.clone(), String::new()];
    let mut used = tokens(&heading);
    let mut written = 0usize;
    for digest in &ordered {
        let line = render_entry(digest);
        let cost = tokens(&line);
        if written > 0 && used + cost > COMMONPLACE_FILE_TOKEN_BUDGET {
            break;
        }
        lines.push(line);
        used += cost;
        written += 1;
    }

    let json = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let frontmatter = [
        "---".to_string(),
        format!("type: {COMMONPLACE_FRONTMATTER_TYPE}"),
        format!("subjectCharacterId: {}", json(&input.subject_character_id)),
        format!("updatedAt: {}", json(&input.updated_at)),
        "---".to_string(),
    ]
    .join("\n");

    RenderedCommonplace {
        content: format!("{frontmatter}\n{}\n", lines.join("\n")),
        entries_written: written,
        entries_dropped: ordered.len() - written,
    }
}

/// v4 `commonplaceFileToCanonText(raw)` (`:127-143`): drop the frontmatter
/// block and every `/^#\s/` heading line (`#h` and `## h` are KEPT), JS-trim,
/// `None` when nothing is left (or the input is falsy).
pub fn commonplace_file_to_canon_text(raw: Option<&str>) -> Option<String> {
    let raw = raw.filter(|r| !r.is_empty())?;
    let mut body = raw;
    if body.starts_with("---\n") {
        // Byte offsets stand in for UTF-16 ones: every marker is ASCII and
        // every slice is taken at one.
        if let Some(rel) = body[4..].find("\n---") {
            let close = 4 + rel;
            body = match body[close + 4..].find('\n') {
                Some(r) => &body[close + 4 + r + 1..],
                None => "",
            };
        }
    }
    let kept = body
        .split('\n')
        .filter(|line| {
            let mut chars = line.chars();
            !(chars.next() == Some('#') && chars.next().is_some_and(is_js_ws))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let kept = js_trim(&kept);
    (!kept.is_empty()).then(|| kept.to_string())
}

/// v4 `truncateToTokenBudget(text, budgetTokens)` (`:149-166`): `None` for a
/// non-positive budget; the text itself when it fits; else whole lines while
/// they fit (each costed with its `\n`); else a proportional UTF-16 cut of the
/// over-long first line, `trimEnd`ed, plus `…` — or `None` when nothing is
/// left.
pub fn truncate_to_token_budget(text: &str, budget_tokens: i64) -> Option<String> {
    if budget_tokens <= 0 {
        return None;
    }
    if tokens(text) <= budget_tokens {
        return Some(text.to_string());
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0i64;
    for line in text.split('\n') {
        let cost = tokens(&format!("{line}\n"));
        if used + cost > budget_tokens {
            break;
        }
        kept.push(line);
        used += cost;
    }
    if !kept.is_empty() {
        return Some(kept.join("\n"));
    }
    let ratio = budget_tokens as f64 / 1f64.max(tokens(text) as f64);
    let len = text.encode_utf16().count() as f64;
    let end = 0f64.max((len * ratio).floor() - 1.0) as usize;
    let cut = utf16_truncate(text, end);
    let cut = js_trim_end(&cut);
    (!cut.is_empty()).then(|| format!("{cut}…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_the_self_collision() {
        assert_eq!(
            commonplace_path_for(&CommonplaceSubject::SelfFile),
            "Commonplace/Self.md"
        );
        let other = |name: &str| CommonplaceSubject::Other {
            id: "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa".into(),
            name: name.into(),
        };
        assert_eq!(
            commonplace_path_for(&other("SELF")),
            "Commonplace/SELF (aaaaaaaa).md"
        );
        assert_eq!(
            commonplace_path_for(&other("Self:")),
            "Commonplace/Self_.md"
        );
        assert_eq!(commonplace_path_for(&other("")), "Commonplace/untitled.md");
    }

    #[test]
    fn line_breaks_collapse_only_in_runs_that_carry_one() {
        assert_eq!(collapse_line_breaks("a \u{a0}\n\t b  c"), "a b  c");
        assert_eq!(collapse_line_breaks("a\u{2028}b"), "a\u{2028}b");
    }

    #[test]
    fn canon_text_keeps_hash_lines_without_a_space() {
        assert_eq!(
            commonplace_file_to_canon_text(Some("---\nt: x\n---\n# h\n#h\n## h\n- b\n")),
            Some("#h\n## h\n- b".to_string())
        );
        assert_eq!(commonplace_file_to_canon_text(Some("")), None);
        assert_eq!(commonplace_file_to_canon_text(Some("---\nx\n---")), None);
    }
}
