//! Tier-1 differential (W4.7e): the LLM-request cache-prefix hashes. Pure —
//! exact byte equality against v4's `computeRequestPrefixHashes`.
//!
//! Generate the oracle:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/request-prefix-hashes.ts \
//!     > /tmp/oracle-request-prefix-hashes.ndjson
//! Run:
//!   QT_ORACLE_REQUEST_PREFIX_HASHES=/tmp/oracle-request-prefix-hashes.ndjson \
//!     cargo test -p quilltap-harness --test request_prefix_hashes_equivalence

use quilltap_core::cache_prefix_hashes::{compute_request_prefix_hashes, PrefixMessage};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    messages: Vec<PrefixMessage>,
    tools: Option<Vec<Value>>,
    out: Value,
}

#[test]
fn request_prefix_hashes_match_oracle() {
    let path = match std::env::var("QT_ORACLE_REQUEST_PREFIX_HASHES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_REQUEST_PREFIX_HASHES to the oracle NDJSON.");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = compute_request_prefix_hashes(&row.messages, row.tools.as_deref());
        // Serialize to a Value: skip_serializing_if drops absent hashes, matching
        // v4's conditionally-set keys.
        let got_value = serde_json::to_value(&got).unwrap();
        assert_eq!(got_value, row.out, "prefix hashes '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: request-prefix-hashes matched oracle ({count} rows).");
}
