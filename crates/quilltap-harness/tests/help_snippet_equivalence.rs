//! P4.9I2A tier-1 differential — the Guide text search
//! (`quilltap_core::services::help_chat::guide_search::search_documents` —
//! `buildSnippet` + the match mapper + the title-hits-first sort — vs v4's REAL
//! `GET /api/v1/help-docs?action=search` handler over a corpus, with
//! `@/lib/help-search` mocked to the corpus docs and the context middleware
//! collapsed; `buildSnippet` is module-private in v4, so the handler is the unit).
//!
//! Corpus (`harness/oracle/fixtures/help-snippet.json`): ten docs + 18 queries
//! covering the `< 2` short-circuit in UTF-16 units (`"d"`, `" d "` trimmed →
//! empty; one astral char → TWO units, NOT short-circuited), case-insensitivity,
//! the fence-before-furniture regex ORDER, emphasis/heading/pipe/quote stripping,
//! `\s+` collapse incl. tabs/CRLF, the lopsided 30/160 window with both / one /
//! no ellipses, a title-only hit (`snippet: null`), the stable title-hits-first
//! sort over docs whose title-hit order differs from document order, and the
//! lowercase-index-applied-to-original quirk (`İ` → `i̇` shifts the slice by one
//! unit, as v4's does).
//!
//! Generate (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TMPO=/tmp/qt-help-snippet-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/help-snippet.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/help-snippet.json $TMPO/fixtures/
//!   QT_ORACLE_OUT=/tmp/oracle-help-snippet.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots $TMPO/cases -- help-snippet
//! Run:
//!   QT_ORACLE_HELP_SNIPPET=/tmp/oracle-help-snippet.ndjson \
//!     cargo test -p quilltap-harness --test help_snippet_equivalence

use std::collections::HashMap;

use quilltap_core::services::help_chat::guide_search::search_documents;
use quilltap_core::services::help_chat::HelpDocument;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct DocW {
    slug: String,
    title: String,
    content: String,
}
#[derive(Deserialize)]
struct Spec {
    docs: Vec<DocW>,
    queries: Vec<String>,
}

#[test]
fn help_snippet_search_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_SNIPPET") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_SNIPPET (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/help-snippet.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let docs: Vec<HelpDocument> = spec
        .docs
        .iter()
        .enumerate()
        .map(|(i, d)| HelpDocument {
            id: format!("id-{i}"),
            slug: d.slug.clone(),
            title: d.title.clone(),
            path: format!("help/{}.md", d.slug),
            url: "/".to_string(),
            content: d.content.clone(),
        })
        .collect();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["q"].as_str().unwrap().to_string(), v);
    }
    assert_eq!(oracle.len(), spec.queries.len());
    assert!(
        spec.queries.len() * spec.docs.len() >= 20,
        "the corpus floor (order: ≥ 20 rows)"
    );

    let mut failed = Vec::new();
    for q in &spec.queries {
        let want = &oracle[q];
        assert_eq!(
            want["status"].as_u64(),
            Some(200),
            "[{q:?}] v4 answered non-200"
        );
        let got: Vec<Value> = search_documents(&docs, q)
            .iter()
            .map(|m| m.to_value())
            .collect();
        let want_matches = want["matches"].as_array().cloned().unwrap_or_default();
        if got != want_matches {
            let i = got
                .iter()
                .zip(want_matches.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(got.len().min(want_matches.len()));
            eprintln!(
                "[{q:?}] MISMATCH ({} vs {} rows) at {i}:\n  GOT : {}\n  WANT: {}",
                got.len(),
                want_matches.len(),
                got.get(i).map(|v| v.to_string()).unwrap_or_default(),
                want_matches
                    .get(i)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            );
            failed.push(q.clone());
        }
    }
    assert!(failed.is_empty(), "help-snippet FAILED: {failed:?}");
}
