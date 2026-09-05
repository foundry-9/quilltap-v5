//! P4.9I2A tier-1 differential — the help-chat context resolver
//! (`quilltap_core::services::help_chat::context_resolver` vs v4's REAL
//! `resolveHelpContentForUrl` / `resolveAllHelpContentForUrl`,
//! `lib/help-chat/context-resolver.ts`).
//!
//! Both sides read the SAME committed corpus (`harness/oracle/fixtures/
//! help-context-resolver.json`: named document SETS + `{set, url}` rows) — the
//! oracle mocks `@/lib/help-search` to the set, exactly as v4's own unit test
//! does; this side hands the set to the pure resolver. Per row: the primary
//! `{matchType, id}` (or null) and the full `resolveAll` list. Covers all six
//! strategies, the query-specificity tie (stable sort → document order), the
//! pattern segment-count tie, prefix longest-wins (whole-url UTF-16 length), the
//! JS `split('?')` third-part drop, `?` with an empty query, `URLSearchParams`
//! `+`/`%20`/repeated-key semantics, `'*'` as an INPUT, the empty string, a
//! no-match set (null), an empty set (null), and the duplicate-wildcard quirk
//! (a wildcard primary pushed twice — v4's id-vs-url dedup never fires).
//!
//! Generate (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TMPO=/tmp/qt-help-resolver-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/help-context-resolver.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/help-context-resolver.json $TMPO/fixtures/
//!   QT_ORACLE_OUT=/tmp/oracle-help-resolver.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots $TMPO/cases -- help-context-resolver
//! Run:
//!   QT_ORACLE_HELP_RESOLVER=/tmp/oracle-help-resolver.ndjson \
//!     cargo test -p quilltap-harness --test help_context_resolver_equivalence

use std::collections::HashMap;

use quilltap_core::services::help_chat::context_resolver::{
    match_url_pattern, resolve_all_help_content_for_url, resolve_help_content_for_url,
};
use quilltap_core::services::help_chat::HelpDocument;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Clone)]
struct DocW {
    id: String,
    slug: String,
    title: String,
    path: String,
    url: String,
    content: String,
}
#[derive(Deserialize)]
struct UrlRow {
    set: String,
    url: String,
}
#[derive(Deserialize)]
struct Spec {
    sets: HashMap<String, Vec<DocW>>,
    urls: Vec<UrlRow>,
}

fn spec_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/help-context-resolver.json")
}

fn to_docs(set: &[DocW]) -> Vec<HelpDocument> {
    set.iter()
        .map(|d| HelpDocument {
            id: d.id.clone(),
            slug: d.slug.clone(),
            title: d.title.clone(),
            path: d.path.clone(),
            url: d.url.clone(),
            content: d.content.clone(),
        })
        .collect()
}

#[test]
fn help_context_resolver_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_RESOLVER") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_RESOLVER (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let mut oracle: HashMap<(String, String), Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(
            (
                v["set"].as_str().unwrap().to_string(),
                v["url"].as_str().unwrap().to_string(),
            ),
            v,
        );
    }
    assert_eq!(oracle.len(), spec.urls.len(), "oracle rows vs corpus urls");
    assert!(
        spec.urls.len() >= 25,
        "the corpus floor (order: ≥ 25 URL rows)"
    );

    let mut failed = Vec::new();
    for row in &spec.urls {
        let docs = to_docs(&spec.sets[&row.set]);
        let primary = resolve_help_content_for_url(&row.url, &docs);
        let all = resolve_all_help_content_for_url(&row.url, &docs);
        let got = json!({
            "primary": primary.as_ref().map(|p| json!({"matchType": p.match_type.as_str(), "id": p.doc_id})),
            "all": all.iter().map(|c| json!({"matchType": c.match_type.as_str(), "id": c.doc_id})).collect::<Vec<_>>(),
        });
        let want = &oracle[&(row.set.clone(), row.url.clone())];
        let want = json!({ "primary": want["primary"], "all": want["all"] });
        if got != want {
            eprintln!(
                "[{}::{:?}] MISMATCH\n  GOT : {got}\n  WANT: {want}",
                row.set, row.url
            );
            failed.push(format!("{}::{}", row.set, row.url));
        }
    }
    assert!(
        failed.is_empty(),
        "help-context-resolver FAILED: {failed:?}"
    );

    // `matchUrlPattern` on its own (v4's match-url-pattern.test.ts cases).
    assert!(match_url_pattern(
        "/aurora/:id/edit",
        "/aurora/abc-123/edit"
    ));
    assert!(!match_url_pattern("/aurora/:id/edit", "/aurora/abc-123"));
    assert!(match_url_pattern("/salon/:id", "/salon/"));
    assert!(!match_url_pattern("/salon", "/salon/x"));
}
