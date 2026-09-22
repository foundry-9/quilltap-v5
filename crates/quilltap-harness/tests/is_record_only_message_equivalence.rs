//! Tier-1 differential test: v4's `isRecordOnlyMessage` (P4.106 item 4 — the
//! P4.D205 gap "the six `isRecordOnlyMessage` unit cases unpinned").
//!
//! The predicate is the ONE gate `buildMessageContext` applies before anything
//! else touches the history: it strips the Host's `inform` record (the passage
//! itself reaches the model as its own system block) and every Commonplace
//! Book whisper bar `relevant-conversations`. v5's twin is
//! `quilltap_core::services::message_context::is_record_only_message`.
//!
//! Both sides read the SAME committed corpus
//! (`harness/oracle/fixtures/is-record-only-message.json` — v4's six shipped
//! cases' ten inputs plus eleven edge rows); the oracle drives v4's REAL
//! export and records `{ id, input, output }`; this test parses each input
//! through `WhisperMessage::from_value` (the production parse) and compares
//! the boolean, exact.
//!
//! Generate the oracle (from the v4 checkout — a pinned worktree per the drift
//! ledger's §5.1 when HEAD is past the baseline):
//!   cd ~/source/quilltap-server
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   PATH=$N:$PATH npx tsx ~/source/quilltap-v5/harness/oracle/cases/is-record-only-message.ts \
//!     > /tmp/oracle-is-record-only-message.ndjson
//! Run:
//!   QT_ORACLE_IS_RECORD_ONLY_MESSAGE=/tmp/oracle-is-record-only-message.ndjson \
//!     cargo test -p quilltap-harness --test is_record_only_message_equivalence

use std::path::PathBuf;

use quilltap_core::services::message_context::{is_record_only_message, WhisperMessage};
use serde_json::Value;

fn corpus_ids() -> Vec<String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/is-record-only-message.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn is_record_only_message_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_IS_RECORD_ONLY_MESSAGE") else {
        println!("SKIP: QT_ORACLE_IS_RECORD_ONLY_MESSAGE unset — generate the oracle (see header)");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // The oracle must cover the whole corpus, in order — a truncated run is
    // not a green.
    let oracle_ids: Vec<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        oracle_ids,
        corpus_ids(),
        "oracle rows do not cover the corpus"
    );

    let mut failures = Vec::new();
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        let expected = row["output"]
            .as_bool()
            .unwrap_or_else(|| panic!("{id}: oracle output is not a boolean"));
        let got = is_record_only_message(&WhisperMessage::from_value(&row["input"]));
        if got != expected {
            failures.push(format!(
                "{id}: v4 {expected}, v5 {got} (input {})",
                row["input"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} rows differ:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
    println!("is_record_only_message: {} rows equal", rows.len());
}
