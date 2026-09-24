//! Translating a user's search box into an FTS5 `MATCH` expression — a whole
//! port of v4 `lib/database/repositories/fts-query.ts` (`f45a517a9`).
//!
//! ## The search-behaviour contract
//!
//! `LIKE '%x%'` is SUBSTRING matching. FTS5 `unicode61` is TOKEN matching.
//! Swapping one for the other changes what the search bar finds, and this is
//! the contract v4 commits to (also stated for users in `help/search.md`):
//!
//! - Search matches **whole words and word prefixes**, not arbitrary
//!   substrings. `walk` finds *walking* and *walked*; it no longer finds
//!   *sidewalk*.
//! - **Accents fold**: `café` and `cafe` find each other. Case folding now
//!   covers non-ASCII letters too, where `LIKE` folded ASCII only.
//! - **Punctuation is not indexed.** A query that is only punctuation, or only
//!   one-character tokens, falls back to the slower exact scan.
//! - Queries containing `.`, `+`, `(` and friends **start working** — and that
//!   sentence is about v5 as much as v4. v5 reproduced v4's pre-index
//!   `$regex` → `LIKE` conversion byte-for-byte (`chats_search::like_pattern`,
//!   now deleted), which turned a user's `.` into `_` and emitted no `ESCAPE`
//!   clause, so `Mr. Smith`, `C++`, `foo(bar)` and `$500` returned NOTHING,
//!   confidently. Closing that is this module's point.
//! - Results stay capped and stay ordered `createdAt DESC` — **not** by FTS
//!   rank. Relevance ordering is a deliberate follow-up, not a side effect.
//!
//! ## Why the whole query is one quoted phrase with a trailing star
//!
//! v4 measured this against 20,000 real messages:
//!
//! | User types    | `LIKE` hits | FTS bare | FTS prefix |
//! |---------------|------------:|---------:|-----------:|
//! | `djinn`       |         657 |      651 |        651 |
//! | `the estate`  |        2504 |     2503 |       2503 |
//! | `walk`        |        2010 | **1089** |   **2007** |
//! | `café`        |          10 |       34 |         35 |
//! | `C++`         |           0 |      152 |  **14260** |
//!
//! A PHRASE (adjacent tokens, in order) is the token-level equivalent of a
//! substring, which is why `the estate` lands within one hit of `LIKE`. The
//! trailing `*` is mandatory: without it `walk` loses half its hits, because
//! `LIKE` was matching *walking*. Quoting also means a user who types FTS5
//! operator syntax (`OR`, `NEAR`, `-`, `:`) gets a literal search rather than a
//! syntax error or a surprise.
//!
//! `C++` is the case the star cannot save: it collapses to the single token
//! `c`, which matches 14,260 rows as a prefix — worse than useless. Queries
//! whose tokens are all shorter than two characters fall back to an exact scan.
//!
//! ## The two JS-fidelity seams
//!
//! - **The tokenizer's character classes.** v4 splits on
//!   `/[^\p{L}\p{N}]+/u`; the Rust `regex` crate spells the same two General
//!   Categories the same way. Measured 2026-09-21 over this module's 48-query
//!   corpus — `regex` 1.12.4 / `regex-syntax` 0.8.11, whose
//!   `unicode_tables/general_category.rs` is generated from **ucd-16.0.0**,
//!   against Node 24.13.1's V8/ICU — with identical tokenizations on every
//!   row, including Greek, Arabic-Indic digits, a Roman numeral, a vulgar
//!   fraction, CJK, combining marks and astral letters. The memory note
//!   `js-regex-to-rust-regex-fidelity` records that only `\s`, `m`-anchors and
//!   case folding diverge between the two engines — none of which this
//!   pattern uses.
//! - **`MIN_USEFUL_TOKEN_LENGTH` is measured in UTF-16 code units**, because
//!   v4 reads `t.length`. A single astral character (`𝔞`, an emoji) is TWO
//!   units in JS and one `char` in Rust, so `chars().count()` would call a
//!   one-emoji query "long enough" where v4 calls it... also long enough, by
//!   accident, for the opposite reason. [`utf16_len`] removes the accident.

use std::sync::OnceLock;

use regex::Regex;

use crate::jsstr::utf16_len;

/// Tokens shorter than this cannot usefully drive a prefix query (v4
/// `MIN_USEFUL_TOKEN_LENGTH`). Counted in UTF-16 code units — see the module
/// doc's second seam.
const MIN_USEFUL_TOKEN_LENGTH: usize = 2;

/// `/[^\p{L}\p{N}]+/u` — everything outside letters and numbers is a separator.
fn separator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^\p{L}\p{N}]+").expect("static regex"))
}

/// Split like FTS5's `unicode61` tokenizer: everything outside letters and
/// numbers is a separator (v4 `tokenizeLikeUnicode61`).
///
/// This is an APPROXIMATION and deliberately so — it only decides *whether* to
/// use the index, and positions snippets. The index itself is tokenized by
/// SQLite, not by this function, so small disagreements cost nothing.
///
/// JS `split(re).filter(Boolean)` drops the empty strings a leading or
/// trailing separator produces; `filter(|s| !s.is_empty())` is that filter.
pub fn tokenize_like_unicode61(query: &str) -> Vec<String> {
    separator_re()
        .split(query)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Escape a user string for a `LIKE ? ESCAPE '\'` comparison (v4
/// `escapeLikeLiteral`, imported by `fts-query.ts` from `./like-escape` since
/// `ad1c4c37f` — the module's own `escapeLikePattern` is DELETED there; v5's
/// name here predates that merge and is kept for source stability).
///
/// The backslash must go first, or it would escape the escapes we add after
/// it. Built directly rather than through the repository's `$regex` filter:
/// that path escapes regex metacharacters, then translates `.` to `_`, and
/// emits no `ESCAPE` clause — which is the bug this fallback exists to avoid
/// repeating.
///
/// **Folded onto [`super::like_escape::escape_like_literal`]**, which is v4's
/// ONLY home for the function since `ad1c4c37f` (`like-escape.ts`'s
/// `escapeLikeLiteral` — same regex, same replacement, measured character for
/// character; only `likeContainsPattern`'s extra `toLowerCase` differs, and
/// this path must NOT lowercase). v4 used to keep two copies (this module had
/// its own `escapeLikePattern`); v5 always kept one, with
/// `escape_set_matches_v4_fts_query_home` below pinning that the shared
/// implementation satisfies v4's `escapeLikeLiteral` vectors exactly.
pub fn escape_like_pattern(value: &str) -> String {
    super::like_escape::escape_like_literal(value)
}

/// Why [`build_fts_match_expression`] refused the index (v4's
/// `FtsFallbackPlan['reason']`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsFallbackReason {
    /// The query tokenized to nothing at all (punctuation or whitespace only).
    NoTokens,
    /// Every token was shorter than [`MIN_USEFUL_TOKEN_LENGTH`].
    TokensTooShort,
}

impl FtsFallbackReason {
    /// The wire spelling v4 logs in the `Global message search plan` line.
    pub fn as_str(self) -> &'static str {
        match self {
            FtsFallbackReason::NoTokens => "no-tokens",
            FtsFallbackReason::TokensTooShort => "tokens-too-short",
        }
    }
}

/// How to run a user's query (v4 `FtsQueryPlan` — the `FtsMatchPlan` /
/// `FtsFallbackPlan` union).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FtsQueryPlan {
    /// Use the index: `match_expr` goes straight into `chat_messages_fts MATCH ?`.
    Fts {
        match_expr: String,
        tokens: Vec<String>,
    },
    /// Don't use the index: `like_pattern` goes into `LIKE ? ESCAPE '\'`.
    Fallback {
        like_pattern: String,
        tokens: Vec<String>,
        reason: FtsFallbackReason,
    },
}

impl FtsQueryPlan {
    /// v4's `plan.kind` — the `path` field of the search-plan debug line.
    pub fn kind(&self) -> &'static str {
        match self {
            FtsQueryPlan::Fts { .. } => "fts",
            FtsQueryPlan::Fallback { .. } => "fallback",
        }
    }

    /// The tokens the plan was decided from (both arms carry them).
    pub fn tokens(&self) -> &[String] {
        match self {
            FtsQueryPlan::Fts { tokens, .. } | FtsQueryPlan::Fallback { tokens, .. } => tokens,
        }
    }
}

/// Decide how to run a user's query (v4 `buildFtsMatchExpression`).
///
/// Returns an FTS plan for anything with at least one token of two or more
/// UTF-16 units, and a `LIKE` fallback otherwise. The fallback is slow,
/// correct and rare.
pub fn build_fts_match_expression(query: &str) -> FtsQueryPlan {
    let tokens = tokenize_like_unicode61(query);
    let like_pattern = format!("%{}%", escape_like_pattern(query));

    if tokens.is_empty() {
        return FtsQueryPlan::Fallback {
            like_pattern,
            tokens,
            reason: FtsFallbackReason::NoTokens,
        };
    }
    if tokens
        .iter()
        .all(|t| utf16_len(t) < MIN_USEFUL_TOKEN_LENGTH)
    {
        return FtsQueryPlan::Fallback {
            like_pattern,
            tokens,
            reason: FtsFallbackReason::TokensTooShort,
        };
    }

    // One phrase, quotes doubled, prefix star on the final token.
    FtsQueryPlan::Fts {
        match_expr: format!("\"{}\"*", query.replace('"', "\"\"")),
        tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // v4's `fts-query.test.ts` cases, one for one. The differential
    // (`fts_query_equivalence`) drives v4's REAL module over a wider corpus;
    // these are the in-tree pins that fail fast when the module is edited.

    fn toks(q: &str) -> Vec<String> {
        tokenize_like_unicode61(q)
    }

    #[test]
    fn splits_on_everything_outside_letters_and_numbers() {
        assert_eq!(toks("Mr. Smith"), ["Mr", "Smith"]);
        assert_eq!(toks("the estate"), ["the", "estate"]);
        assert_eq!(toks("don't"), ["don", "t"]);
        assert_eq!(toks("C++"), ["C"]);
        assert_eq!(toks("room 12b"), ["room", "12b"]);
    }

    #[test]
    fn keeps_non_ascii_letters_whole() {
        assert_eq!(toks("café"), ["café"]);
        assert_eq!(toks("Ίσταμβουλ"), ["Ίσταμβουλ"]);
    }

    #[test]
    fn returns_nothing_for_punctuation_or_whitespace_alone() {
        assert!(toks("   ").is_empty());
        assert!(toks("--").is_empty());
        assert!(toks("").is_empty());
    }

    #[test]
    fn escapes_the_wildcards_and_the_escape_character_itself() {
        assert_eq!(escape_like_pattern("100%"), "100\\%");
        assert_eq!(escape_like_pattern("a_b"), "a\\_b");
        assert_eq!(escape_like_pattern("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn escapes_the_backslash_first_so_added_escapes_are_not_re_escaped() {
        assert_eq!(escape_like_pattern("\\%"), "\\\\\\%");
    }

    /// The pin the fold rests on: v4's `escapeLikeLiteral` vectors (the fold
    /// target since `ad1c4c37f`) satisfied by the shared `like_escape`
    /// implementation, INCLUDING the regex-metacharacter row — this is LIKE,
    /// not a regex, so `.` and `(` are left alone.
    #[test]
    fn escape_set_matches_v4_fts_query_home() {
        assert_eq!(escape_like_pattern("Mr. Smith (esq.)"), "Mr. Smith (esq.)");
        // And it is NOT `likeContainsPattern`: no lowercasing, no `%…%` wrap.
        assert_eq!(escape_like_pattern("Mixed CASE"), "Mixed CASE");
    }

    #[test]
    fn wraps_a_single_word_as_a_prefix_phrase() {
        let plan = build_fts_match_expression("walk");
        assert_eq!(plan.kind(), "fts");
        assert!(matches!(&plan, FtsQueryPlan::Fts { match_expr, .. } if match_expr == "\"walk\"*"));
    }

    #[test]
    fn keeps_a_multi_word_query_as_one_phrase() {
        let plan = build_fts_match_expression("the estate");
        assert!(
            matches!(&plan, FtsQueryPlan::Fts { match_expr, .. } if match_expr == "\"the estate\"*")
        );
    }

    #[test]
    fn doubles_embedded_quotes_rather_than_breaking_the_phrase() {
        let plan = build_fts_match_expression("he said \"hi\"");
        assert!(matches!(&plan, FtsQueryPlan::Fts { match_expr, .. }
                if match_expr == "\"he said \"\"hi\"\"\"*"));
    }

    #[test]
    fn leaves_fts_operator_syntax_literal_inside_the_phrase() {
        for q in ["cats OR dogs", "NEAR(a b)", "-excluded", "col:value"] {
            let plan = build_fts_match_expression(q);
            assert_eq!(plan.kind(), "fts", "{q}");
            assert!(
                matches!(&plan, FtsQueryPlan::Fts { match_expr, .. }
                    if *match_expr == format!("\"{q}\"*")),
                "{q}"
            );
        }
    }

    #[test]
    fn keeps_punctuation_the_tokenizer_will_discard() {
        let plan = build_fts_match_expression("Mr. Smith");
        assert!(
            matches!(&plan, FtsQueryPlan::Fts { match_expr, .. } if match_expr == "\"Mr. Smith\"*")
        );
    }

    #[test]
    fn falls_back_for_a_query_whose_tokens_are_all_single_characters() {
        let plan = build_fts_match_expression("C++");
        assert!(matches!(
            &plan,
            FtsQueryPlan::Fallback {
                reason: FtsFallbackReason::TokensTooShort,
                like_pattern,
                ..
            } if like_pattern == "%C++%"
        ));
        assert_eq!(plan.kind(), "fallback");
    }

    #[test]
    fn falls_back_for_a_query_with_no_tokens_at_all() {
        let plan = build_fts_match_expression("---");
        assert!(matches!(
            &plan,
            FtsQueryPlan::Fallback {
                reason: FtsFallbackReason::NoTokens,
                ..
            }
        ));
    }

    #[test]
    fn uses_the_index_when_at_least_one_token_is_long_enough() {
        assert_eq!(build_fts_match_expression("walk C++").kind(), "fts");
    }

    #[test]
    fn escapes_the_wildcard_characters_in_the_fallback_pattern() {
        let plan = build_fts_match_expression("%_");
        assert!(matches!(
            &plan,
            FtsQueryPlan::Fallback { like_pattern, .. } if like_pattern == "%\\%\\_%"
        ));
    }

    /// The UTF-16 seam, stated as a test rather than a comment: a lone astral
    /// character is two JS units, so it clears `MIN_USEFUL_TOKEN_LENGTH` — and
    /// a query of two separate astral characters still tokenizes to nothing,
    /// because emoji are neither `\p{L}` nor `\p{N}`.
    #[test]
    fn the_length_gate_counts_utf16_units() {
        // MATHEMATICAL FRAKTUR SMALL A (U+1D51E) — a `\p{L}`, two UTF-16 units.
        let plan = build_fts_match_expression("\u{1D51E}");
        assert_eq!(plan.kind(), "fts");
        assert_eq!(plan.tokens(), ["\u{1D51E}"]);
        // An emoji is `\p{So}`, so it is a separator, not a token.
        let plan = build_fts_match_expression("\u{1F600}\u{1F601}");
        assert!(matches!(
            &plan,
            FtsQueryPlan::Fallback {
                reason: FtsFallbackReason::NoTokens,
                ..
            }
        ));
    }
}
