//! Tier-1 differential (P4.D347, v4 `9a88f862f`): the name matcher —
//! `buildNameMatcher` / `firstNameMatch` / `lastNameMatch`
//! (`lib/chat/name-mentions.ts`) against `quilltap_core::name_mentions`.
//! Replaces the retired `mentioned_characters_equivalence` (its corpus lives on
//! re-cut against the new contract — case-sensitive, Unicode-letter-bounded,
//! longest token first, UTF-16 indices).
//!
//! Each row carries the names, the text, whether v4 built a matcher, and the
//! first / last `{index, text}`; the Rust side recomputes from the row's input
//! and compares exactly (the match as bytes).
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/name-mentions.ts \
//!     > /tmp/oracle-name-mentions.ndjson
//! Run:
//!   QT_ORACLE_NAME_MENTIONS=/tmp/oracle-name-mentions.ndjson \
//!     cargo test -p quilltap-harness --test name_mentions_equivalence -- --nocapture

use quilltap_core::name_mentions::{build_name_matcher, first_name_match, last_name_match};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Row {
    id: String,
    names: Vec<Option<String>>,
    text: String,
    matcher: bool,
    first: Value,
    last: Value,
}

fn hit(m: Option<quilltap_core::name_mentions::NameMatch>) -> Value {
    match m {
        Some(m) => json!({ "index": m.index, "text": m.text }),
        None => Value::Null,
    }
}

#[test]
fn name_mentions_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_NAME_MENTIONS") else {
        eprintln!("SKIP: set QT_ORACLE_NAME_MENTIONS to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut failures = Vec::new();
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let matcher = build_name_matcher(row.names.iter().map(|n| n.as_deref()));
        let got = json!({
            "matcher": matcher.is_some(),
            "first": hit(matcher.as_ref().and_then(|m| first_name_match(&row.text, m))),
            "last": hit(matcher.as_ref().and_then(|m| last_name_match(&row.text, m))),
        });
        let want = json!({ "matcher": row.matcher, "first": row.first, "last": row.last });
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if g != w {
            failures.push(format!("{}: v4 {w}\n   v5 {g}", row.id));
        }
        count += 1;
    }
    assert!(count >= 30, "oracle file looks empty/partial: {count} rows");
    assert!(
        failures.is_empty(),
        "{} of {count} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: name-mentions matched oracle ({count} rows).");
}
