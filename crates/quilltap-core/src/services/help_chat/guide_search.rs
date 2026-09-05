//! The Guide's text search — v4 `app/api/v1/help-docs/route.ts` (`buildSnippet` +
//! `handleSearch`'s mapper) over the in-process document list. Tier-1 pure;
//! pinned by `help_snippet_equivalence` (the route handler driven over a corpus
//! with `getHelpSearch` mocked, because `buildSnippet` is module-private in v4).
//!
//! v4's *why*, carried forward: the Guide's search box used to filter the topic
//! list by title alone, so a term that lives in the prose ("describe",
//! "uncensored", "timeout") returned nothing. Content is not shipped to the client
//! with the index — far too large — so the match runs server-side. Deliberately a
//! plain case-insensitive substring match: the "find the page with this word on
//! it" affordance, complementary to the semantic `help_search` tool. No stemming,
//! no ranking beyond title-hits-first.
//!
//! ## UTF-16 fidelity
//!
//! Every index here is a JS string index — UTF-16 code units. `query.length`,
//! `content.length`, `indexOf`, `slice` all count units, so an astral character
//! is TWO. Two consequences the corpus pins: a one-astral-character query is
//! NOT short-circuited by `query.length < 2`; and `contentIndex` is found in the
//! LOWERCASED content but applied to the ORIGINAL — where lowercasing changes a
//! string's unit length (`İ` → `i̇`, one unit to two) the slice lands one unit
//! off, exactly as v4's does.
//!
//! ## One recorded divergence: a window that splits a surrogate pair
//!
//! v4's `content.slice(start, end)` can cut INSIDE an astral character (the
//! shipped tree carries 24 of them across four files), and JS keeps the lone
//! surrogate — `JSON.stringify` emits `"\ude00…"`, which no Rust `String` can
//! hold and `serde_json` refuses to parse. v5 decodes such a slice lossily
//! (U+FFFD), the same convention `jsstr::utf16_truncate` records. The corpus
//! places its astral-lead row ON a pair boundary so both sides are comparable;
//! the divergence is one replacement glyph where v4 ships an unpaired half.

use serde_json::Value;

use super::HelpDocument;
use crate::jsstr::{is_js_ws, js_index_of, js_trim, utf16_len};

/// Characters of prose kept either side of a text hit. Deliberately lopsided:
/// the snippet renders on one truncated line, so a match sitting in the middle
/// of a balanced window gets clipped off the right-hand end — the reader is
/// shown context for a term they can no longer see. A short run-up puts the
/// matched word near the start of the line, where it survives truncation.
pub const SNIPPET_LEAD: usize = 30;
pub const SNIPPET_TRAIL: usize = 160;

/// One search hit (v4 `{ slug, titleHit, snippet }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuideSearchMatch {
    pub slug: String,
    pub title_hit: bool,
    /// `null` when the term appears in the title only.
    pub snippet: Option<String>,
}

impl GuideSearchMatch {
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "slug": self.slug,
            "titleHit": self.title_hit,
            "snippet": self.snippet,
        })
    }
}

/// v4 `buildSnippet(content, matchIndex, queryLength)` — a snippet around the
/// first occurrence, the Markdown flattened enough to read on one line. Indices
/// and lengths are UTF-16 units.
pub fn build_snippet(content: &str, match_index: usize, query_length: usize) -> String {
    let units: Vec<u16> = content.encode_utf16().collect();
    let len = units.len();
    let start = match_index.saturating_sub(SNIPPET_LEAD);
    let end = (match_index + query_length + SNIPPET_TRAIL).min(len);
    // `content.slice(start, end)` — a slice that splits a surrogate pair decodes
    // lossily (BMP text, which the help tree is, round-trips exactly).
    let slice = String::from_utf16_lossy(&units[start.min(end)..end]);

    // Markdown furniture reads as noise on a single-line snippet: fences and
    // emphasis markers, heading hashes, list bullets, table pipes. The passes run
    // in v4's ORDER: fences (```+ → ' ') BEFORE the single-char strip, so a fence
    // becomes a space rather than vanishing into its neighbours.
    let pass1 = replace_fences(&slice);
    let pass2: String = pass1
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '|' | '>'))
        .collect();
    let pass3 = collapse_js_whitespace(&pass2);
    let trimmed = js_trim(&pass3);

    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        trimmed,
        if end < len { "…" } else { "" }
    )
}

/// `/```+/g → ' '` — every run of three-or-more backticks becomes one space.
fn replace_fences(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            let mut j = i;
            while j < chars.len() && chars[j] == '`' {
                j += 1;
            }
            if j - i >= 3 {
                out.push(' ');
            } else {
                for c in &chars[i..j] {
                    out.push(*c);
                }
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `/\s+/g → ' '` with JS's `\s` set (`is_js_ws`).
fn collapse_js_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_js_ws(c) {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// v4 `handleSearch`'s core: `q` → the matches (or an empty list for a query
/// under two UTF-16 units after trimming). Title hits sort first; the sort is
/// STABLE so document order holds within each half.
pub fn search_documents(documents: &[HelpDocument], q: &str) -> Vec<GuideSearchMatch> {
    let query = js_trim(q);
    if utf16_len(query) < 2 {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let query_len = utf16_len(query);

    let mut matches: Vec<GuideSearchMatch> = documents
        .iter()
        .filter_map(|doc| {
            let title_hit = doc.title.to_lowercase().contains(&needle);
            let content_index = js_index_of(&doc.content.to_lowercase(), &needle, 0);
            if !title_hit && content_index.is_none() {
                return None;
            }
            Some(GuideSearchMatch {
                slug: doc.slug.clone(),
                title_hit,
                snippet: content_index.map(|i| build_snippet(&doc.content, i, query_len)),
            })
        })
        .collect();
    // A title hit is a stronger signal than a passing mention in the prose.
    matches.sort_by_key(|m| std::cmp::Reverse(m.title_hit));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_shape_and_ellipses() {
        let content = "alpha beta gamma";
        // Whole-string window → no ellipses.
        assert_eq!(build_snippet(content, 6, 4), "alpha beta gamma");
        let long = format!("{}needle{}", "x".repeat(40), "y".repeat(200));
        let snip = build_snippet(&long, 40, 6);
        assert!(snip.starts_with('…') && snip.ends_with('…'));
    }

    #[test]
    fn fences_go_first_then_furniture() {
        // ```code``` → ' code ' → collapsed/trimmed "code"; a lone backtick pair
        // is stripped by the single-char pass.
        assert_eq!(build_snippet("```code``` and `x`", 3, 4), "code and x");
        assert_eq!(
            build_snippet("# Head | cell > q *b* _i_", 0, 1),
            "Head cell q b i"
        );
    }

    #[test]
    fn short_query_short_circuits_in_utf16_units() {
        let docs = vec![HelpDocument {
            id: "1".into(),
            slug: "one".into(),
            title: "T".into(),
            path: "help/one.md".into(),
            url: "/".into(),
            content: "😀 smiley".into(),
        }];
        assert!(search_documents(&docs, " a ").is_empty());
        // One astral char = TWO units → not short-circuited, and it matches.
        assert_eq!(search_documents(&docs, "😀").len(), 1);
    }
}
