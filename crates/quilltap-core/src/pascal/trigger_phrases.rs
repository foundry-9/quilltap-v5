//! Trigger phrases — the pure scanner (v4 `lib/pascal/trigger-phrases.ts`,
//! `362426c8f`).
//!
//! A custom tool may declare `triggers` — phrases that run it, with every
//! parameter at its default, when a message from anyone in the chat contains
//! one. This module answers ONE question, purely: given a message and the
//! roster's definitions, which tools fire, in what order. The gate, the roster,
//! the deal and the posts are the runner's (`pascal::trigger_runner`).
//!
//! Matching is a plain substring after a case fold — `fire` matches
//! `fireplace`; no word boundaries, no regex. Carina query lines are stripped
//! first ([`strip_carina_query_lines`]), so a question addressed to Carina is
//! never read as words spoken in the scene.

use serde::Serialize;

use crate::carina_parser::strip_carina_query_lines;
use crate::jsstr::{js_index_of, js_trim};

/// Most tools one message may fire. Each run may consult a model and posts a
/// message; an author who lists common words in eight tools should not spend
/// eight LLM calls per sentence. Over the cap, the rest are returned as
/// `dropped` for the caller to log — never silently truncated.
pub const MAX_TRIGGER_RUNS_PER_MESSAGE: usize = 4;

/// One tool a message fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerHit {
    /// The tool's `name`.
    pub tool: String,
    /// The phrase AS WRITTEN in the definition (not folded).
    pub phrase: String,
    /// Offset of the match in the scanned text — for ordering only. A UTF-16
    /// offset (JS `indexOf`) into the FOLDED, STRIPPED haystack: folding can
    /// change the UTF-16 length (`İ` → `i̇`), so it is not an offset into the
    /// message as written.
    pub index: u64,
}

/// The scan's answer: the tools that fire, and those the cap turned away.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanResult {
    pub hits: Vec<TriggerHit>,
    pub dropped: Vec<TriggerHit>,
}

/// The one case fold every trigger comparison uses (the mention ranker's
/// precedent). v4's `toLocaleLowerCase()` with no locale, which the ICU ruling
/// makes byte-identical to `str::to_lowercase` — never `to_ascii_lowercase`.
pub fn fold_trigger_text(text: &str) -> String {
    text.to_lowercase()
}

/// The tool's own phrases that `text` contains, AS WRITTEN and in definition
/// order — used by the Workbench to warn when an outcome message would echo
/// its own trigger. `[]` for empty text or no triggers; a zero-length phrase is
/// skipped.
pub fn triggers_contained_in(text: &str, triggers: Option<&[String]>) -> Vec<String> {
    let triggers = match triggers {
        Some(t) if !text.is_empty() && !t.is_empty() => t,
        _ => return Vec::new(),
    };
    let haystack = fold_trigger_text(text);
    triggers
        .iter()
        .filter(|phrase| {
            !phrase.is_empty() && js_index_of(&haystack, &fold_trigger_text(phrase), 0).is_some()
        })
        .cloned()
        .collect()
}

/// Strip Carina query lines, fold case, and find every tool whose phrases the
/// text contains. ONE hit per tool — the earliest occurrence of any of its
/// phrases — in order of appearance, capped at [`MAX_TRIGGER_RUNS_PER_MESSAGE`]
/// (the rest are returned separately so the caller can log them).
///
/// `tools` is `(name, triggers)` in ROSTER order (the roster map's insertion
/// order); that order breaks ties.
pub fn scan_trigger_phrases<'a>(
    content: &str,
    tools: impl IntoIterator<Item = (&'a str, Option<&'a [String]>)>,
) -> ScanResult {
    if content.is_empty() {
        return ScanResult::default();
    }

    // Strip THEN fold — v4's order.
    let haystack = fold_trigger_text(&strip_carina_query_lines(content));
    if js_trim(&haystack).is_empty() {
        return ScanResult::default();
    }

    let mut all: Vec<TriggerHit> = Vec::new();
    for (name, triggers) in tools {
        let Some(triggers) = triggers.filter(|t| !t.is_empty()) else {
            continue;
        };

        let mut best: Option<TriggerHit> = None;
        for phrase in triggers {
            let needle = fold_trigger_text(phrase);
            if needle.is_empty() {
                continue;
            }
            let Some(index) = js_index_of(&haystack, &needle, 0) else {
                continue;
            };
            let index = index as u64;
            // Strictly lower: two phrases at the same offset keep the FIRST in
            // definition order.
            if best.as_ref().is_none_or(|b| index < b.index) {
                best = Some(TriggerHit {
                    tool: name.to_string(),
                    phrase: phrase.clone(),
                    index,
                });
            }
        }
        if let Some(hit) = best {
            all.push(hit);
        }
    }

    // Order of appearance; ties (two tools whose phrases start at the same
    // offset) keep roster order, which a stable sort preserves (`sort_by_key`
    // is stable, as V8's `Array.prototype.sort` is).
    all.sort_by_key(|hit| hit.index);

    let dropped = if all.len() > MAX_TRIGGER_RUNS_PER_MESSAGE {
        all.split_off(MAX_TRIGGER_RUNS_PER_MESSAGE)
    } else {
        Vec::new()
    };
    ScanResult { hits: all, dropped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool<'a>(name: &'a str, triggers: &'a [String]) -> (&'a str, Option<&'a [String]>) {
        (name, Some(triggers))
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// v4 `trigger-phrases.test.ts`'s fold row: both sides fold, and the offset
    /// is into the folded text.
    #[test]
    fn folds_both_sides() {
        let t = s(&["AbraCadabra"]);
        let r = scan_trigger_phrases("She cried ABRACADABRA!", [tool("spell", &t)]);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].index, 10);
        assert_eq!(r.hits[0].phrase, "AbraCadabra");
    }

    /// An astral character is TWO UTF-16 units before the match.
    #[test]
    fn index_is_utf16() {
        let t = s(&["abracadabra"]);
        let r = scan_trigger_phrases("😀 abracadabra", [tool("spell", &t)]);
        assert_eq!(r.hits[0].index, 3);
    }

    /// Over the cap: the LATEST by offset is dropped, not the last in roster
    /// order.
    #[test]
    fn caps_by_offset() {
        let t: Vec<Vec<String>> = (0..5).map(|i| s(&[&format!("w{i}")])).collect();
        let names = ["t0", "t1", "t2", "t3", "t4"];
        // t0's word appears LAST in the text.
        let r = scan_trigger_phrases(
            "w1 w2 w3 w4 w0",
            names
                .iter()
                .zip(t.iter())
                .map(|(n, t)| (*n, Some(t.as_slice()))),
        );
        assert_eq!(
            r.hits.iter().map(|h| h.tool.as_str()).collect::<Vec<_>>(),
            ["t1", "t2", "t3", "t4"]
        );
        assert_eq!(r.dropped.len(), 1);
        assert_eq!(r.dropped[0].tool, "t0");
    }

    #[test]
    fn contained_in_returns_as_written() {
        let t = s(&["Abracadabra", "open sesame"]);
        assert_eq!(
            triggers_contained_in("say abracadabra", Some(&t)),
            s(&["Abracadabra"])
        );
        assert!(triggers_contained_in("say abracadabra", None).is_empty());
        assert!(triggers_contained_in("", Some(&t)).is_empty());
    }
}
