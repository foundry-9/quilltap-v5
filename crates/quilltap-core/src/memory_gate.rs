//! Port of the pure pieces of v4's lib/memory/memory-gate.ts: the
//! reinforced-importance formula and the deterministic, regex-based
//! `extractNovelDetails`. The stateful gate itself (embedding search + the
//! INSERT/REINFORCE/SKIP decision) lands in a later phase.

use crate::jsstr::{js_trim, utf16_len, JS_WS_CLASS};
use regex::Regex;
use std::collections::HashSet;

/// Reinforced importance: `importance + log2(count + 1) * 0.05`, capped at 1.0.
/// `reinforcement_count` is an integer count; `log2(count + 1)` grows the floor
/// each time a memory is re-observed, saturating at the 1.0 ceiling.
pub fn calculate_reinforced_importance(base_importance: f64, reinforcement_count: f64) -> f64 {
    1.0_f64.min(base_importance + (reinforcement_count + 1.0).log2() * 0.05)
}

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
