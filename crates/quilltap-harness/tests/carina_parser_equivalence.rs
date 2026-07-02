//! Tier-1 differential test: Carina inline-query parser (chat-orchestration
//! wave 1.5).
//!
//! Covers `parseCarinaQuery` / `parse_carina_query`. Pure function — exact
//! equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/carina-parser.ts \
//!     > /tmp/oracle-carina-parser.ndjson
//! Run:
//!   QT_ORACLE_CARINA_PARSER=/tmp/oracle-carina-parser.ndjson cargo test -p quilltap-harness --test carina_parser_equivalence

use quilltap_core::carina_parser::{parse_carina_query, CarinaQuery};
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
    content: Option<String>,
    out: Option<WireQuery>,
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

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = parse_carina_query(row.content.as_deref());
        let want = row.out.map(|w| CarinaQuery {
            character_name: w.character_name,
            whisper: w.whisper,
            question: w.question,
        });
        assert_eq!(got, want, "case '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: carina-parser matched oracle ({count} rows).");
}
