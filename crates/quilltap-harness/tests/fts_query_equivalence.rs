//! Tier-1 differential: the Rust FTS5 query translator must agree with v4's
//! REAL `lib/database/repositories/fts-query.ts`, query by query, on every
//! corpus row.
//!
//! This module decides **what the search bar finds**. An index path taken where
//! v4 takes the fallback (or the reverse) is not a slower answer — it is a
//! DIFFERENT answer, because token-prefix semantics and substring semantics
//! disagree about `sidewalk`, `café` and `C++`. So the comparand is the whole
//! plan, not just its `kind`: the `match` expression byte for byte, the
//! fallback `likePattern` byte for byte, the `reason` word, and the token list.
//!
//! ## The two seams this family exists to measure
//!
//! 1. **`\p{L}\p{N}` across two Unicode engines.** v4 splits on
//!    `/[^\p{L}\p{N}]+/u` under V8's ICU tables; v5 uses the `regex` crate's.
//!    The corpus carries Greek, Arabic-Indic digits, a Roman numeral (`Nl`), a
//!    vulgar fraction (`No`), CJK, an astral Fraktur letter, emoji, and the
//!    same word both precomposed (`école`) and decomposed (`e` + U+0301 —
//!    which tokenizes as TWO tokens, because a combining mark is `Mn`, not a
//!    letter). Measured agreement on every row.
//! 2. **`MIN_USEFUL_TOKEN_LENGTH` is UTF-16.** `tokenLengths` is carried in the
//!    oracle row for exactly this reason: `𝔞` is ONE `char` and TWO JS units,
//!    so a `chars().count()` port would call it too short where v4 calls it
//!    long enough. The row pins the measurement, not just the verdict.
//!
//! ## Regenerating
//!
//! ```bash
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx /Users/csebold/source/quilltap-v5/harness/oracle/cases/fts-query.ts \
//!     /Users/csebold/source/quilltap-v5/harness/oracle/fixtures/fts-query.json \
//!     > /tmp/oracle-fts-query.ndjson
//!   QT_ORACLE_FTS_QUERY=/tmp/oracle-fts-query.ndjson \
//!     cargo test -p quilltap-harness --test fts_query_equivalence -- --nocapture
//! ```
//!
//! With `QT_ORACLE_FTS_QUERY` unset the differential SKIPs (and says so); the
//! corpus self-checks below always run.

use std::path::PathBuf;

use quilltap_core::db::fts_query::{
    build_fts_match_expression, escape_like_pattern, tokenize_like_unicode61, FtsQueryPlan,
};
use quilltap_core::jsstr::utf16_len;
use serde::Deserialize;

/// One oracle row — v4's three functions over one query.
#[derive(Debug, Deserialize)]
struct OracleRow {
    query: String,
    tokens: Vec<String>,
    #[serde(rename = "tokenLengths")]
    token_lengths: Vec<usize>,
    escaped: String,
    plan: OraclePlan,
}

/// v4's `FtsQueryPlan` as it serializes: `kind` plus either `match` or
/// `likePattern` + `reason`.
#[derive(Debug, Deserialize)]
struct OraclePlan {
    kind: String,
    #[serde(rename = "match")]
    match_expr: Option<String>,
    #[serde(rename = "likePattern")]
    like_pattern: Option<String>,
    reason: Option<String>,
    tokens: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Corpus {
    queries: Vec<String>,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/fts-query.json")
}

fn read_corpus() -> Corpus {
    let path = corpus_path();
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("parse fts-query corpus")
}

#[test]
fn fts_query_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FTS_QUERY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FTS_QUERY to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));
    let rows: Vec<OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse oracle row: {e}\n{l}")))
        .collect();

    let corpus = read_corpus();
    assert_eq!(
        rows.len(),
        corpus.queries.len(),
        "oracle row count vs corpus query count — regenerate the oracle"
    );

    for (i, row) in rows.iter().enumerate() {
        let q = &row.query;
        assert_eq!(
            *q, corpus.queries[i],
            "row {i}: oracle query out of step with the corpus"
        );

        // 1) The tokenizer, and the UTF-16 lengths the gate reads.
        let tokens = tokenize_like_unicode61(q);
        assert_eq!(tokens, row.tokens, "row {i} ({q:?}): tokens diverged");
        let lengths: Vec<usize> = tokens.iter().map(|t| utf16_len(t)).collect();
        assert_eq!(
            lengths, row.token_lengths,
            "row {i} ({q:?}): token UTF-16 lengths diverged"
        );

        // 2) The LIKE escape (v5's single `like_escape` home, via the fold).
        assert_eq!(
            escape_like_pattern(q),
            row.escaped,
            "row {i} ({q:?}): escapeLikePattern diverged"
        );

        // 3) The whole plan.
        let plan = build_fts_match_expression(q);
        assert_eq!(
            plan.kind(),
            row.plan.kind,
            "row {i} ({q:?}): plan kind diverged"
        );
        assert_eq!(
            plan.tokens(),
            row.plan.tokens.as_slice(),
            "row {i} ({q:?}): plan tokens diverged"
        );
        match &plan {
            FtsQueryPlan::Fts { match_expr, .. } => {
                assert_eq!(
                    Some(match_expr.as_str()),
                    row.plan.match_expr.as_deref(),
                    "row {i} ({q:?}): MATCH expression diverged"
                );
                assert!(
                    row.plan.like_pattern.is_none() && row.plan.reason.is_none(),
                    "row {i} ({q:?}): v4 emitted a fallback field on an fts plan"
                );
            }
            FtsQueryPlan::Fallback {
                like_pattern,
                reason,
                ..
            } => {
                assert_eq!(
                    Some(like_pattern.as_str()),
                    row.plan.like_pattern.as_deref(),
                    "row {i} ({q:?}): fallback LIKE pattern diverged"
                );
                assert_eq!(
                    Some(reason.as_str()),
                    row.plan.reason.as_deref(),
                    "row {i} ({q:?}): fallback reason diverged"
                );
                assert!(
                    row.plan.match_expr.is_none(),
                    "row {i} ({q:?}): v4 emitted a MATCH on a fallback plan"
                );
            }
        }
    }

    eprintln!("OK: fts_query matched oracle on {} queries.", rows.len());
}

/// The corpus must actually reach both plan arms and both fallback reasons, or
/// a green differential proves less than it looks. Runs with no env var.
#[test]
fn corpus_covers_every_plan_arm() {
    let corpus = read_corpus();
    let mut fts = 0usize;
    let mut no_tokens = 0usize;
    let mut too_short = 0usize;
    let mut astral_token = 0usize;
    for q in &corpus.queries {
        match build_fts_match_expression(q) {
            FtsQueryPlan::Fts { .. } => fts += 1,
            FtsQueryPlan::Fallback { reason, .. } => match reason.as_str() {
                "no-tokens" => no_tokens += 1,
                "tokens-too-short" => too_short += 1,
                other => panic!("unknown reason {other}"),
            },
        }
        if tokenize_like_unicode61(q)
            .iter()
            .any(|t| utf16_len(t) != t.chars().count())
        {
            astral_token += 1;
        }
    }
    assert!(fts > 0 && no_tokens > 0 && too_short > 0, "arm coverage");
    assert!(
        astral_token > 0,
        "the corpus must carry a token whose UTF-16 length differs from its char count, \
         or the MIN_USEFUL_TOKEN_LENGTH seam is untested"
    );
    // A 1,000-character query must be in the corpus: this module has no length
    // gate of its own (the caller's MAX_SEARCH_QUERY_LENGTH is upstream), and a
    // long phrase is exactly where a naive quoting port would break.
    assert!(
        corpus.queries.iter().any(|q| utf16_len(q) >= 1000),
        "the corpus must carry a 1,000-character query"
    );
}
