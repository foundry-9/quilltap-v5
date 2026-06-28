//! Tier-1 differential test #26 (Wave 6 / B17): parseLegacyEmbeddingText —
//! recovering pre-BLOB JSON-text embeddings, exact structural / float equality
//! against the v4 oracle. The integer-keyed-object cases pin JS `Object.values`
//! ascending-numeric key ordering.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/legacy-embedding.ts \
//!     > /tmp/oracle-legacy-embedding.ndjson
//! Run:
//!   QT_ORACLE_LEGACY_EMBEDDING=/tmp/oracle-legacy-embedding.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::embedding_blob::parse_legacy_embedding_text;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    id: String,
    input: String,
    out: Option<Vec<f64>>,
}

#[test]
fn legacy_embedding_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_LEGACY_EMBEDDING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_LEGACY_EMBEDDING to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = parse_legacy_embedding_text(&row.input);
        match (&got, &row.out) {
            (None, None) => {}
            (Some(g), Some(e)) => {
                assert_eq!(g.len(), e.len(), "length '{}'", row.id);
                for (i, (gv, ev)) in g.iter().zip(e.iter()).enumerate() {
                    assert!(
                        (gv - ev).abs() < 1e-12,
                        "value '{}' idx {i}: got {gv} want {ev}",
                        row.id
                    );
                }
            }
            _ => panic!(
                "some/none mismatch '{}': got {got:?} want {:?}",
                row.id, row.out
            ),
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: legacy-embedding matched oracle ({count} rows).");
}
