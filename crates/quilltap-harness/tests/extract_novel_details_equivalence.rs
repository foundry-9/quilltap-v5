//! Tier-1 differential test #24 (Wave 5 / B15): extractNovelDetails — exact
//! ordered-array equality against the v4 oracle. Exercises sentence-initial
//! skip, stop-word filtering, all four date formats, currency, numbers-with-
//! units (including the trailing-`\b` quirk that drops `100%`), CamelCase,
//! acronyms, case-insensitive dedup, existing-content suppression, punctuation
//! stripping, the length>1 filter, and JS `\s` (U+00A0) splitting/matching.
//!
//! Re-aimed at P4.D289 (pin `83334b9a5`): v4 `8af764514` moved the function,
//! byte-identical, to `lib/memory/admission/novel-details.ts`; v5 moved its
//! port verbatim to `crate::admission::novel_details` (the crate-root
//! `memory_gate::extract_novel_details` is the forwarding shim). Red-first:
//! the re-aimed oracle against the UNCHANGED test was value-GREEN (15 / 15).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/extract-novel-details.ts" > /tmp/oracle-extract-novel-details.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_EXTRACT_NOVEL_DETAILS=/tmp/oracle-extract-novel-details.ndjson \
//!     cargo test -p quilltap-harness --test extract_novel_details_equivalence -- --nocapture

use quilltap_core::admission::extract_novel_details;
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

    // Corpus drift guard: the case's exact row count.
    assert_eq!(count, 15, "oracle row count");
    eprintln!("OK: extract-novel-details matched oracle ({count} rows).");
}
