//! Tier-1 differential: v4's `sendMessageSchema` refines (P4.D372, v4
//! `db6190b44`, pin `624d0d25c`) against `quilltap_core::api::chat_send::
//! chat_send_refine` — the `ChatSend` dispatch arm's gate.
//!
//! The oracle `safeParse`s v4's REAL schema over a 288-row grid of `content` ×
//! `targetParticipantIds` × `isEstablishedFact` × `fileIds` ×
//! `pendingToolResults`. Per row: v4 succeeds exactly when v5's gate passes,
//! and when v4 refuses, v5's ONE message is v4's FIRST issue's message.
//!
//! **`SEND_REFINE_ISSUE_LIST` (NAMED divergence, R3), both ways.** Zod
//! collects every issue (up to three here: whispered → needs words →
//! content/files/tools) and v4's route renders them all as `{error:
//! 'Validation error', details: […]}`; v5's `ChatSend` arm answers the FIRST
//! issue's message as its 400 body — the standing P4.6a shape, extended in
//! v4's precedence. Pinned: the measured count of multi-issue rows
//! ([`MULTI_ISSUE_ROWS`]) and, on each, v4's ORDER (the first issue is v5's
//! answer). Retires if a later order renders Zod issue bodies at the dispatch
//! edge.
//!
//! Red-first: the U+FEFF / U+3000 rows (JS `.trim()` empties them; Rust's
//! `str::trim` kept U+FEFF — R-L6's convergence of the existing content arm).
//!
//! Generate the oracle (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   $N/npx tsx $V5W/harness/oracle/cases/send-message-schema.ts \
//!     > /tmp/oracle-send-message-schema.ndjson
//! Run:
//!   QT_ORACLE_SEND_SCHEMA=/tmp/oracle-send-message-schema.ndjson \
//!     cargo test -p quilltap-harness --test send_message_schema_equivalence -- --nocapture

use quilltap_core::api::chat_send::chat_send_refine;
use quilltap_core::api::types::PendingToolResult;
use serde_json::Value;

/// The rows on which v4 reports MORE than one issue (measured at the pin):
/// v5 answers only the first of them.
const MULTI_ISSUE_ROWS: usize = 35;

#[test]
fn send_message_schema_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_SEND_SCHEMA") else {
        eprintln!("SKIP: QT_ORACLE_SEND_SCHEMA unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 288, "the grid");
    let mut failures = Vec::new();
    let mut multi = 0usize;
    for (i, r) in rows.iter().enumerate() {
        let input = &r["input"];
        let content = input.get("content").and_then(Value::as_str).unwrap_or("");
        let targets: Option<Vec<String>> = input
            .get("targetParticipantIds")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect());
        let files: Vec<String> = input
            .get("fileIds")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        let tools: Vec<PendingToolResult> = input
            .get("pendingToolResults")
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .unwrap_or_default();
        let flag = input.get("isEstablishedFact").and_then(Value::as_bool);
        let got = chat_send_refine(content, targets.as_deref(), &files, &tools, flag);
        let issues = r["issues"].as_array().unwrap();
        if issues.len() > 1 {
            multi += 1;
        }
        let want = issues.first().and_then(|i| i["message"].as_str());
        if got != want {
            failures.push(format!("row {i} {input}: v5 {got:?} vs v4 first {want:?}"));
        }
        if r["success"].as_bool().unwrap() != got.is_none() {
            failures.push(format!("row {i}: success mismatch"));
        }
    }
    eprintln!(
        "send_message_schema_equivalence: {} rows, {multi} with several issues (SEND_REFINE_ISSUE_LIST)",
        rows.len()
    );
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        multi, MULTI_ISSUE_ROWS,
        "SEND_REFINE_ISSUE_LIST moved — re-measure"
    );
}
