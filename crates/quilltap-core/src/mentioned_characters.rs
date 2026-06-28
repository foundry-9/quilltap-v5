//! Port of v4's lib/chat/context/mentioned-characters.ts — scanning a chat
//! corpus for mentions of non-participant characters by name or alias.
//!
//! v4 builds one case-insensitive regex `\b(?:tok1|tok2|…)\b` (flags `gi`, no
//! `u`) and runs `exec` over the corpus, mapping each lowercased hit back to the
//! character ids that own that token. Fidelity points:
//!   * `\b` is JS's **ASCII** word boundary (no `u` flag) — reproduced with
//!     `(?-u:\b)`. This faithfully carries the quirk that a token ending in a
//!     non-ASCII letter (e.g. `José`) has no trailing boundary and so won't
//!     match — the ASCII-mode `\b` sees the trailing UTF-8 byte as a non-word
//!     byte just as JS sees the non-ASCII char as a non-word char.
//!   * Tokens are tried **longest-first**; both the `regex` crate and JS use
//!     leftmost-first ("Perl-like") alternation, so a longer token wins over a
//!     shorter prefix at the same position.
//!   * The token list is an **insertion-ordered, exact-string-deduped** set
//!     (JS `Set` iteration order) then **stably** sorted by descending UTF-16
//!     length (JS `String.length`).
//!   * The hit→ids map is keyed by the trimmed, lowercased token; two characters
//!     can share an alias, so a key maps to a set of ids.

use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

/// A character that could be mentioned: its id and the names/aliases to scan for.
#[derive(Clone, Debug)]
pub struct MentionCandidate {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

/// UTF-16 code-unit length, matching JS `String.length` (used as the sort key).
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Scan `scan_corpus` for mentions of any candidate's name or alias as a whole
/// (ASCII-)word, returning the set of matched character ids. Case-insensitive.
/// Equivalent to v4's `findMentionedCharacterIds`.
pub fn find_mentioned_character_ids(
    scan_corpus: &str,
    candidates: &[MentionCandidate],
) -> BTreeSet<String> {
    let mut matched: BTreeSet<String> = BTreeSet::new();
    if scan_corpus.is_empty() || candidates.is_empty() {
        return matched;
    }

    // Insertion-ordered, exact-trimmed-string-deduped token set (JS Set order).
    let mut seen: HashSet<String> = HashSet::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut add_unique = |raw: &str| {
        let t = raw.trim();
        if !t.is_empty() && seen.insert(t.to_string()) {
            tokens.push(t.to_string());
        }
    };
    for c in candidates {
        if !c.name.trim().is_empty() {
            add_unique(&c.name);
        }
        for alias in &c.aliases {
            if !alias.trim().is_empty() {
                add_unique(alias);
            }
        }
    }
    if tokens.is_empty() {
        return matched;
    }

    // Longer tokens first so "John Smith" beats "John"; stable to preserve the
    // insertion order of equal-length tokens (JS stable sort).
    tokens.sort_by_key(|t| std::cmp::Reverse(utf16_len(t)));
    let alternation = tokens
        .iter()
        .map(|t| regex::escape(t))
        .collect::<Vec<_>>()
        .join("|");
    let re = Regex::new(&format!("(?i)(?-u:\\b)(?:{alternation})(?-u:\\b)")).unwrap();

    // Lowercased token -> owning character ids (an alias may be shared).
    let mut token_to_ids: HashMap<String, HashSet<String>> = HashMap::new();
    let mut add_token = |token: &str, id: &str| {
        let key = token.trim().to_lowercase();
        if key.is_empty() {
            return;
        }
        token_to_ids.entry(key).or_default().insert(id.to_string());
    };
    for c in candidates {
        if !c.name.is_empty() {
            add_token(&c.name, &c.id);
        }
        for alias in &c.aliases {
            add_token(alias, &c.id);
        }
    }

    for m in re.find_iter(scan_corpus) {
        let hit = m.as_str().to_lowercase();
        if let Some(ids) = token_to_ids.get(&hit) {
            for id in ids {
                matched.insert(id.clone());
            }
        }
    }

    matched
}
