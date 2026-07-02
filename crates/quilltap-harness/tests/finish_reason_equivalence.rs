//! Tier-1 differential test (chat-orchestration wave 1.6): provider
//! finish-reason extraction. Pure function — exact equality.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/finish-reason.ts \
//!     > /tmp/oracle-finish-reason.ndjson
//! Run:
//!   QT_ORACLE_FINISH_REASON=/tmp/oracle-finish-reason.ndjson \
//!     cargo test -p quilltap-harness --test finish_reason_equivalence

use quilltap_core::finish_reason::extract_finish_reason;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    raw: Value,
    out: Option<String>,
}

#[test]
fn finish_reason_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_FINISH_REASON") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FINISH_REASON to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = extract_finish_reason(&row.raw);
        assert_eq!(got, row.out, "finish-reason '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: finish-reason matched oracle ({count} rows).");
}
