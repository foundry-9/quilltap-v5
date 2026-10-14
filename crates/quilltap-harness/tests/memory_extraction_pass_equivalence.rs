//! P4.D288 unit 3 — tier-1 differential: `services::queue_service::pass_of`
//! (contract C1-6) against v4's REAL `memoryExtractionPassOf`
//! (`lib/background-jobs/queue-service.ts:116-121`) at the `83334b9a5` pin:
//! every flag combination, JS truthiness on each flag and on `pass`, and R-X's
//! unknown pass returned AS IS (`Err(raw)` — a string or any truthy value).
//!
//! Regenerate + run (a pure tsx oracle):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-memory-extraction-pass.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/memory-extraction-pass.ts \
//!     > /tmp/oracle-memory-extraction-pass.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_EXTRACTION_PASS=/tmp/oracle-memory-extraction-pass.ndjson \
//!     cargo test -p quilltap-harness --test memory_extraction_pass_equivalence -- --nocapture

use quilltap_core::services::queue_service::pass_of;
use serde_json::Value;

#[test]
fn memory_extraction_pass_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_EXTRACTION_PASS") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_EXTRACTION_PASS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle");
    let mut failures = Vec::new();
    let (mut rows, mut unknown) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        rows += 1;
        let got = match pass_of(&row["payload"]) {
            Ok(p) => Value::String(p.as_str().to_string()),
            Err(raw) => {
                unknown += 1;
                raw
            }
        };
        if got != row["pass"] {
            failures.push(format!("{}: v5 {got} v4 {}", row["label"], row["pass"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        rows >= 25 && unknown >= 5,
        "the corpus shrank: {rows} rows, {unknown} unknown"
    );
    eprintln!("OK: pass_of matched v4 over {rows} rows ({unknown} unknown passes).");
}
