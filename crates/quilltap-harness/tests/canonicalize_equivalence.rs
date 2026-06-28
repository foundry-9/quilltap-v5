//! Tier-1 differential test #28 (Wave 6 / B19): tool canonicalization —
//! byte-exact serialized equality against the v4 oracle. The oracle emits
//! `JSON.stringify(result)` as a string; we compare `serde_json::to_string` of
//! the Rust output against it, so key ORDER (deep parameter sort + tool-name
//! array sort) is what's verified, not just structure.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/canonicalize.ts \
//!     > /tmp/oracle-canonicalize.ndjson
//! Run:
//!   QT_ORACLE_CANONICALIZE=/tmp/oracle-canonicalize.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::canonicalize::{
    canonicalize_universal_tool, canonicalize_universal_tools, UniversalTool,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "one")]
    One {
        id: String,
        input: UniversalTool,
        out: String,
    },
    #[serde(rename = "many")]
    Many {
        id: String,
        input: Vec<UniversalTool>,
        out: String,
    },
}

#[test]
fn canonicalize_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CANONICALIZE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CANONICALIZE to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::One { id, input, out } => {
                let got = serde_json::to_string(&canonicalize_universal_tool(&input)).unwrap();
                assert_eq!(got, out, "canonicalizeUniversalTool '{id}'");
            }
            Row::Many { id, input, out } => {
                let got = serde_json::to_string(&canonicalize_universal_tools(&input)).unwrap();
                assert_eq!(got, out, "canonicalizeUniversalTools '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: canonicalize matched oracle ({count} rows).");
}
