//! Tier-1 differential test #24 (Wave 5 / B15): extractNovelDetails — exact
//! ordered-array equality against the v4 oracle. Exercises sentence-initial
//! skip, stop-word filtering, all four date formats, currency, numbers-with-
//! units (including the trailing-`\b` quirk that drops `100%`), CamelCase,
//! acronyms, case-insensitive dedup, existing-content suppression, punctuation
//! stripping, the length>1 filter, and JS `\s` (U+00A0) splitting/matching.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/extract-novel-details.ts \
//!     > /tmp/oracle-extract-novel-details.ndjson
//! Run:
//!   QT_ORACLE_EXTRACT_NOVEL_DETAILS=/tmp/oracle-extract-novel-details.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::memory_gate::extract_novel_details;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    id: String,
    candidate: String,
    existing: String,
    out: Vec<String>,
}

#[test]
fn extract_novel_details_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_EXTRACT_NOVEL_DETAILS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_EXTRACT_NOVEL_DETAILS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = extract_novel_details(&row.candidate, &row.existing);
        assert_eq!(got, row.out, "novel '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: extract-novel-details matched oracle ({count} rows).");
}
