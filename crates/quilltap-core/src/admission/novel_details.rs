//! Port of v4 `lib/memory/admission/novel-details.ts` (`8af764514`, Phase C
//! of the memory system refactor) — `extractNovelDetails`, MOVED: v4's file is
//! byte-identical to the pre `memory-gate.ts:942-1046` body (`STOP_WORDS`
//! identical), and this module is v5's `memory_gate.rs:84-316` copied
//! VERBATIM (work order P4.D289 Tier 1 item 7); the crate-root
//! `memory_gate::extract_novel_details` becomes the forwarding shim.
//!
//! v4's why, carried: deterministic and regex-based (no LLM call), pure, safe
//! in the job child. `foldInto` turns what it finds into `[+]` footnotes; the
//! create path's episodic safety net (P4.D290's `prepare`) uses it to recover
//! entities and a date phrase the extractor left out.

use crate::jsstr::{js_trim, utf16_len, JS_WS_CLASS};
use regex::Regex;
use std::collections::HashSet;

/// Stop words filtered out of the proper-noun and acronym detail candidates.
/// Verbatim from v4's `STOP_WORDS` set, same order (order is irrelevant for a
/// set but kept for an easy diff against the source).
const STOP_WORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "and",
    "or",
    "but",
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "with",
    "by",
    "from",
    "as",
    "is",
    "was",
    "are",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "shall",
    "can",
    "this",
    "that",
    "these",
    "those",
    "it",
    "its",
    "i",
    "he",
    "she",
    "we",
    "they",
    "me",
    "him",
    "her",
    "us",
    "them",
    "my",
    "his",
    "our",
    "their",
    "your",
    "not",
    "no",
    "if",
    "then",
    "so",
    "up",
    "out",
    "just",
    "also",
    "very",
    "about",
    "into",
    "over",
    "after",
    "before",
    "between",
    "through",
    "during",
    "without",
    "again",
    "further",
    "once",
    "here",
    "there",
    "when",
    "where",
    "why",
    "how",
    "all",
    "each",
    "every",
    "both",
    "few",
    "more",
    "most",
    "other",
    "some",
    "such",
    "only",
    "own",
    "same",
    "than",
    "too",
    "now",
    "new",
    "old",
    "first",
    "last",
    "long",
    "great",
    "little",
    "right",
    "big",
    "high",
    "small",
    "large",
    "next",
    "early",
    "young",
    "important",
    "public",
    "bad",
    "good",
    "said",
    "told",
    "asked",
    "went",
    "came",
    "made",
    "got",
    "see",
    "know",
    "think",
    "want",
    "say",
    "tell",
];

/// Extract novel, deterministic details from `candidate_content` that are not
/// already present (case-insensitively, as substrings) in `existing_content`:
/// proper nouns (non-sentence-initial Capitalised words), dates, currency,
/// numbers-with-units, CamelCase terms, and acronyms. Returns them in discovery
/// order, deduped case-insensitively. Mirrors v4's `extractNovelDetails`.
///
/// Regex-engine fidelity vs the JS source: `\d` and `\b` are JS-ASCII (reproduced
/// as `[0-9]` and `(?-u:\b)`), `\s` is the JS whitespace set (reproduced via a
/// literal class), and the case-insensitive date/number patterns use `(?i)`.
pub fn extract_novel_details(candidate_content: &str, existing_content: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();
    let existing_lower = existing_content.to_lowercase();
    let mut novel: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut add_if_novel = |detail: &str| {
        let normalized = js_trim(detail).to_string();
        let lower = normalized.to_lowercase();
        if utf16_len(&normalized) > 1 && !seen.contains(&lower) && !existing_lower.contains(&lower)
        {
            seen.insert(lower);
            novel.push(normalized);
        }
    };

    // ASCII word boundary / digit class, and the JS `\s` class, as fragments.
    let b = r"(?-u:\b)";
    let d = "[0-9]";
    let ws = JS_WS_CLASS;
    let months =
        "January|February|March|April|May|June|July|August|September|October|November|December";

    // 1. Proper nouns: Capitalised, non-sentence-initial, non-stop-word words.
    let sentence_split = Regex::new(r"[.!?]+").unwrap();
    let word_split = Regex::new(&format!("{ws}+")).unwrap();
    let trailing_punct = Regex::new(r#"[,;:'")\]}>]+$"#).unwrap();
    let leading_punct = Regex::new(r#"^['"(\[{<]+"#).unwrap();
    let proper_noun = Regex::new("^[A-Z][a-z]+").unwrap();
    for sentence in sentence_split.split(candidate_content) {
        let trimmed = js_trim(sentence);
        let words: Vec<&str> = word_split.split(trimmed).collect();
        for word in words.iter().skip(1) {
            let stripped = trailing_punct.replace(word, "");
            let clean = leading_punct.replace(&stripped, "").into_owned();
            if utf16_len(&clean) > 1
                && proper_noun.is_match(&clean)
                && !stop.contains(clean.to_lowercase().as_str())
            {
                add_if_novel(&clean);
            }
        }
    }

    // 2. Dates (four formats), currency, numbers-with-units.
    let date_patterns = [
        format!("{b}{d}{{4}}-{d}{{2}}-{d}{{2}}{b}"),
        format!("{b}{d}{{1,2}}/{d}{{1,2}}/{d}{{2,4}}{b}"),
        format!("(?i){b}(?:{months}){ws}+{d}{{1,2}}(?:,?{ws}+{d}{{4}})?{b}"),
        format!("(?i){b}{d}{{1,2}}{ws}+(?:{months})(?:{ws}+{d}{{4}})?{b}"),
    ];
    for pat in &date_patterns {
        let re = Regex::new(pat).unwrap();
        for m in re.find_iter(candidate_content) {
            add_if_novel(m.as_str());
        }
    }

    let currency = Regex::new(&format!(r"\$[0-9,]+(?:\.{d}{{2}})?{b}")).unwrap();
    for m in currency.find_iter(candidate_content) {
        add_if_novel(m.as_str());
    }

    let units = "years?|months?|days?|hours?|minutes?|miles?|km|lbs?|kg|ft|cm|percent|%|times?";
    let numbers = Regex::new(&format!(r"(?i){b}{d}+(?:\.{d}+)?(?:{ws}*(?:{units})){b}")).unwrap();
    for m in numbers.find_iter(candidate_content) {
        add_if_novel(m.as_str());
    }

    // 3. Technical terms: CamelCase words and acronyms (2+ uppercase letters).
    let camel = Regex::new(&format!("{b}[A-Z][a-z]+(?:[A-Z][a-z]+)+{b}")).unwrap();
    for m in camel.find_iter(candidate_content) {
        add_if_novel(m.as_str());
    }

    let acronym = Regex::new(&format!("{b}[A-Z]{{2,}}{b}")).unwrap();
    for m in acronym.find_iter(candidate_content) {
        if !stop.contains(m.as_str().to_lowercase().as_str()) {
            add_if_novel(m.as_str());
        }
    }

    novel
}
