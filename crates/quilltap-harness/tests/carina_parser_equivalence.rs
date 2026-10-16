//! Tier-1 differential test: Carina inline-query parser (chat-orchestration
//! wave 1.5).
//!
//! Covers `parseCarinaQuery` / `parse_carina_query` and (P4.D305, v4
//! `362426c8f`) `stripCarinaQueryLines` / `strip_carina_query_lines` — rows of
//! `kind: "strip"`; a row with no `kind` is a parse row. Pure functions — exact
//! equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/carina-parser.ts \
//!     > /tmp/oracle-carina-parser.ndjson
//! Run:
//!   QT_ORACLE_CARINA_PARSER=/tmp/oracle-carina-parser.ndjson cargo test -p quilltap-harness --test carina_parser_equivalence

use quilltap_core::carina_parser::{parse_carina_query, strip_carina_query_lines, CarinaQuery};
use serde::Deserialize;

#[derive(Deserialize)]
struct WireQuery {
    #[serde(rename = "characterName")]
    character_name: String,
    whisper: bool,
    question: String,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    #[serde(default)]
    kind: Option<String>,
    content: Option<String>,
    out: Option<serde_json::Value>,
}

#[test]
fn carina_parser_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CARINA_PARSER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CARINA_PARSER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut count, mut strips) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        match row.kind.as_deref() {
            None | Some("parse") => {
                let got = parse_carina_query(row.content.as_deref());
                let want = row.out.map(|w| {
                    let w: WireQuery = serde_json::from_value(w).unwrap();
                    CarinaQuery {
                        character_name: w.character_name,
                        whisper: w.whisper,
                        question: w.question,
                    }
                });
                assert_eq!(got, want, "case '{}'", row.id);
            }
            Some("strip") => {
                let content = row
                    .content
                    .unwrap_or_else(|| panic!("strip case '{}' has no content", row.id));
                let want = row
                    .out
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| panic!("strip case '{}' has no string out", row.id));
                assert_eq!(
                    strip_carina_query_lines(&content),
                    want,
                    "strip case '{}'",
                    row.id
                );
                strips += 1;
            }
            Some(other) => panic!("case '{}': unknown row kind {other:?}", row.id),
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    assert!(
        strips > 0,
        "no strip rows — the 362426c8f corpus is missing"
    );
    eprintln!("OK: carina-parser matched oracle ({count} rows, {strips} strip rows).");
}
