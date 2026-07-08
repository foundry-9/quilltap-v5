//! The Porter stemmer + tokenizer — a byte-for-byte transcription of v4's
//! hand-rolled `plugins/dist/qtap-plugin-builtin-embeddings/porter-stemmer.ts`.
//!
//! ## Why a hand transcription (no stemming crate)
//!
//! Classic Porter has variant edge cases; a Rust stemming crate would produce
//! *different* stems on some words, which silently shifts every
//! `tfidf_vocabulary` index and misses on stored data. The vocabulary keys must
//! be byte-identical to v4's, so this reproduces v4's exact step order and
//! conditions (`step1a`..`step5b`), including its quirks.
//!
//! ## Domain: ASCII
//!
//! [`tokenize`] first lowercases and replaces every non-`[a-z0-9\s]` char with a
//! space, so the words that reach [`stem`] are pure `[a-z0-9]+` (ASCII). Inside
//! the stemmer we operate on `Vec<char>` (== JS UTF-16 code units for ASCII, ==
//! bytes for ASCII), which is exact for that domain. Calling [`stem`] directly on
//! a non-ASCII word is out of the convergent class (the tokenizer never produces
//! one); the differential corpus keeps direct-`stem` inputs ASCII.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Common English stop words filtered out during text processing. Verbatim from
/// v4's `STOP_WORDS` set (same 127 entries, same spelling).
pub static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "a",
        "an",
        "the",
        "and",
        "or",
        "but",
        "if",
        "then",
        "else",
        "when",
        "at",
        "by",
        "for",
        "with",
        "about",
        "against",
        "between",
        "into",
        "through",
        "during",
        "before",
        "after",
        "above",
        "below",
        "to",
        "from",
        "up",
        "down",
        "in",
        "out",
        "on",
        "off",
        "over",
        "under",
        "again",
        "further",
        "once",
        "here",
        "there",
        "where",
        "why",
        "how",
        "all",
        "each",
        "few",
        "more",
        "most",
        "other",
        "some",
        "such",
        "no",
        "nor",
        "not",
        "only",
        "own",
        "same",
        "so",
        "than",
        "too",
        "very",
        "s",
        "t",
        "can",
        "will",
        "just",
        "don",
        "should",
        "now",
        "i",
        "me",
        "my",
        "myself",
        "we",
        "our",
        "ours",
        "ourselves",
        "you",
        "your",
        "yours",
        "yourself",
        "yourselves",
        "he",
        "him",
        "his",
        "himself",
        "she",
        "her",
        "hers",
        "herself",
        "it",
        "its",
        "itself",
        "they",
        "them",
        "their",
        "theirs",
        "themselves",
        "what",
        "which",
        "who",
        "whom",
        "this",
        "that",
        "these",
        "those",
        "am",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "having",
        "do",
        "does",
        "did",
        "doing",
        "would",
        "could",
        "ought",
        "of",
        "as",
    ]
    .into_iter()
    .collect()
});

/// Is `w[i]` a consonant? (v4 `isConsonant`.) `y` is a consonant unless the char
/// before it is a consonant (so `y` after a consonant, or at index 0, is a
/// consonant; `y` after a vowel is a vowel).
fn is_consonant(w: &[char], i: usize) -> bool {
    let c = w[i];
    if c == 'a' || c == 'e' || c == 'i' || c == 'o' || c == 'u' {
        return false;
    }
    if c == 'y' {
        return i == 0 || !is_consonant(w, i - 1);
    }
    true
}

/// The Porter "measure" of a word — the number of VC (vowel→consonant)
/// transitions after any initial consonant run (v4 `measure`).
fn measure(w: &[char]) -> usize {
    let mut m = 0;
    let mut i = 0;
    let len = w.len();

    while i < len && is_consonant(w, i) {
        i += 1;
    }

    while i < len {
        while i < len && !is_consonant(w, i) {
            i += 1;
        }
        if i >= len {
            break;
        }
        m += 1;
        while i < len && is_consonant(w, i) {
            i += 1;
        }
    }

    m
}

/// Does the word contain any vowel? (v4 `hasVowel`.)
fn has_vowel(w: &[char]) -> bool {
    (0..w.len()).any(|i| !is_consonant(w, i))
}

/// Does the word end with a doubled consonant (e.g. `-tt`)? (v4
/// `endsWithDoubleConsonant`.)
fn ends_with_double_consonant(w: &[char]) -> bool {
    if w.len() < 2 {
        return false;
    }
    let last = w.len() - 1;
    w[last] == w[last - 1] && is_consonant(w, last)
}

/// Does the word end in CVC where the final consonant is not `w`, `x`, or `y`?
/// (v4 `endsWithCVC`.)
fn ends_with_cvc(w: &[char]) -> bool {
    if w.len() < 3 {
        return false;
    }
    let last = w.len() - 1;
    let c = w[last];
    is_consonant(w, last)
        && !is_consonant(w, last - 1)
        && is_consonant(w, last - 2)
        && c != 'w'
        && c != 'x'
        && c != 'y'
}

/// `word.endsWith(suffix)` over the char slice (suffixes are ASCII literals).
fn ends_with(w: &[char], suffix: &str) -> bool {
    let s: Vec<char> = suffix.chars().collect();
    if w.len() < s.len() {
        return false;
    }
    w[w.len() - s.len()..] == s[..]
}

/// `word.slice(0, -n)` — drop the last `n` chars.
fn drop_last(w: &[char], n: usize) -> Vec<char> {
    w[..w.len() - n].to_vec()
}

/// Step 1a: plural `-s`. (v4 `step1a`.)
fn step1a(w: Vec<char>) -> Vec<char> {
    if ends_with(&w, "sses") {
        return drop_last(&w, 2);
    }
    if ends_with(&w, "ies") {
        return drop_last(&w, 2);
    }
    if ends_with(&w, "ss") {
        return w;
    }
    if ends_with(&w, "s") {
        return drop_last(&w, 1);
    }
    w
}

/// Step 1b: `-eed`/`-ed`/`-ing`. (v4 `step1b`.)
fn step1b(w: Vec<char>) -> Vec<char> {
    if ends_with(&w, "eed") {
        let stem = &w[..w.len() - 3];
        if measure(stem) > 0 {
            let mut s = stem.to_vec();
            s.extend("ee".chars());
            return s;
        }
        return w;
    }

    let mut stem: Vec<char> = Vec::new();
    let mut had_suffix = false;

    if ends_with(&w, "ed") {
        stem = drop_last(&w, 2);
        had_suffix = has_vowel(&stem);
    } else if ends_with(&w, "ing") {
        stem = drop_last(&w, 3);
        had_suffix = has_vowel(&stem);
    }

    if had_suffix {
        let mut word = stem;

        if ends_with(&word, "at") || ends_with(&word, "bl") || ends_with(&word, "iz") {
            word.push('e');
            return word;
        }

        if ends_with_double_consonant(&word)
            && !ends_with(&word, "l")
            && !ends_with(&word, "s")
            && !ends_with(&word, "z")
        {
            word.truncate(word.len() - 1);
            return word;
        }

        if measure(&word) == 1 && ends_with_cvc(&word) {
            word.push('e');
            return word;
        }

        return word;
    }

    w
}

/// Step 1c: `-y` → `-i` (only when the stem has a vowel). (v4 `step1c`.)
fn step1c(w: Vec<char>) -> Vec<char> {
    if ends_with(&w, "y") && has_vowel(&w[..w.len() - 1]) {
        let mut s = drop_last(&w, 1);
        s.push('i');
        return s;
    }
    w
}

/// Apply the first matching `(suffix, replacement)` mapping when `measure(stem) >
/// 0` — the shared body of v4's `step2`/`step3`. Returns on the first suffix that
/// `endsWith` matches (replacing only when the measure holds), else the word.
fn map_suffix_measure_gt_0(w: Vec<char>, mappings: &[(&str, &str)]) -> Vec<char> {
    for (suffix, replacement) in mappings {
        if ends_with(&w, suffix) {
            let stem = &w[..w.len() - suffix.chars().count()];
            if measure(stem) > 0 {
                let mut s = stem.to_vec();
                s.extend(replacement.chars());
                return s;
            }
            return w;
        }
    }
    w
}

/// Step 2: map double suffixes to single ones. (v4 `step2`.)
fn step2(w: Vec<char>) -> Vec<char> {
    const MAPPINGS: &[(&str, &str)] = &[
        ("ational", "ate"),
        ("tional", "tion"),
        ("enci", "ence"),
        ("anci", "ance"),
        ("izer", "ize"),
        ("abli", "able"),
        ("alli", "al"),
        ("entli", "ent"),
        ("eli", "e"),
        ("ousli", "ous"),
        ("ization", "ize"),
        ("ation", "ate"),
        ("ator", "ate"),
        ("alism", "al"),
        ("iveness", "ive"),
        ("fulness", "ful"),
        ("ousness", "ous"),
        ("aliti", "al"),
        ("iviti", "ive"),
        ("biliti", "ble"),
    ];
    map_suffix_measure_gt_0(w, MAPPINGS)
}

/// Step 3: `-icate`/`-ative`/`-ness` etc. (v4 `step3`.)
fn step3(w: Vec<char>) -> Vec<char> {
    const MAPPINGS: &[(&str, &str)] = &[
        ("icate", "ic"),
        ("ative", ""),
        ("alize", "al"),
        ("iciti", "ic"),
        ("ical", "ic"),
        ("ful", ""),
        ("ness", ""),
    ];
    map_suffix_measure_gt_0(w, MAPPINGS)
}

/// Step 4: remove `-ant`/`-ence`/etc. when `measure(stem) > 1` (with the `-ion`
/// s/t special case). (v4 `step4`.)
fn step4(w: Vec<char>) -> Vec<char> {
    const SUFFIXES: &[&str] = &[
        "al", "ance", "ence", "er", "ic", "able", "ible", "ant", "ement", "ment", "ent", "ion",
        "ou", "ism", "ate", "iti", "ous", "ive", "ize",
    ];

    for suffix in SUFFIXES {
        if ends_with(&w, suffix) {
            let stem = &w[..w.len() - suffix.chars().count()];
            if measure(stem) > 1 {
                if *suffix == "ion" {
                    if ends_with(stem, "s") || ends_with(stem, "t") {
                        return stem.to_vec();
                    }
                } else {
                    return stem.to_vec();
                }
            }
            return w;
        }
    }
    w
}

/// Step 5a: remove a trailing `-e`. (v4 `step5a`.)
fn step5a(w: Vec<char>) -> Vec<char> {
    if ends_with(&w, "e") {
        let stem = &w[..w.len() - 1];
        let m = measure(stem);
        if m > 1 {
            return stem.to_vec();
        }
        if m == 1 && !ends_with_cvc(stem) {
            return stem.to_vec();
        }
    }
    w
}

/// Step 5b: collapse a final doubled `-ll` when `measure > 1`. (v4 `step5b`.)
fn step5b(w: Vec<char>) -> Vec<char> {
    if measure(&w) > 1 && ends_with_double_consonant(&w) && ends_with(&w, "l") {
        let mut s = w;
        s.truncate(s.len() - 1);
        return s;
    }
    w
}

/// Apply the Porter Stemming algorithm to a word (v4 `stem`). Short words
/// (length ≤ 2) are returned unchanged and un-lowercased (v4's early return uses
/// the ORIGINAL word). Otherwise the word is lowercased, then steps 1a–5b run in
/// order.
pub fn stem(word: &str) -> String {
    // v4 `word.length <= 2` on the ORIGINAL word (UTF-16 code units == chars for
    // ASCII), returning the original word (not lowercased).
    if word.chars().count() <= 2 {
        return word.to_string();
    }

    let mut w: Vec<char> = word.to_lowercase().chars().collect();
    w = step1a(w);
    w = step1b(w);
    w = step1c(w);
    w = step2(w);
    w = step3(w);
    w = step4(w);
    w = step5a(w);
    w = step5b(w);

    w.into_iter().collect()
}

/// Tokenize text into stemmed tokens (v4 `tokenize`, `removeStopWords` default
/// `true`).
///
/// The pipeline: lowercase → replace every non-`[a-z0-9\s]` char with a space →
/// split on whitespace runs → drop stop words (when `remove_stop_words`) → drop
/// words shorter than 2 → [`stem`] the rest.
///
/// Reproduces v4's `.replace(/[^a-z0-9\s]/g, ' ').split(/\s+/)`: any char that is
/// not `[a-z0-9]` and not JS-whitespace becomes a space (a delimiter), and JS
/// whitespace is itself a delimiter — so tokens are maximal `[a-z0-9]+` runs.
pub fn tokenize(text: &str, remove_stop_words: bool) -> Vec<String> {
    let lower = text.to_lowercase();

    // Split into maximal `[a-z0-9]+` runs. Every other char (JS-whitespace OR a
    // char the regex would replace with a space) breaks the run.
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }

    let mut tokens: Vec<String> = Vec::new();
    for word in words {
        // v4 order: stop-word check first, then the length gate.
        if remove_stop_words && STOP_WORDS.contains(word.as_str()) {
            continue;
        }
        // words are pure ASCII `[a-z0-9]`, so byte length == UTF-16 length.
        if word.len() < 2 {
            continue;
        }
        tokens.push(stem(&word));
    }
    tokens
}

/// Generate bigrams from a token list (v4 `generateBigrams`): each adjacent pair
/// joined with `_`. Empty / single-token input yields no bigrams.
pub fn generate_bigrams(tokens: &[String]) -> Vec<String> {
    if tokens.len() < 2 {
        return Vec::new();
    }
    (0..tokens.len() - 1)
        .map(|i| format!("{}_{}", tokens[i], tokens[i + 1]))
        .collect()
}
