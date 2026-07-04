//! Differential test (W4.1d batch 1) — the three help / agent-mode tool handlers
//! (`help_settings`, `help_navigate`, `submit_final_response`).
//!
//! help_navigate + submit_final_response are PURE (tier-1): both sides run the
//! same JSON arg corpus with no DB. help_settings is a READ across five
//! user-scoped main-DB repos: both sides READ a COPY of one v4-baked fixture
//! (chat_settings + connection/embedding/image profiles + roleplay templates),
//! run the same category ops, and the tool Output + formatted string compare
//! EXACTLY — no normalization, because nothing is mutated.
//!
//! Per op the oracle emits `{ output, formatted }`; the Rust port serializes its
//! own Output to a Value and formats via `format_*`, and both are diffed.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_HELP=/tmp/qt-help.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-help-tools-fixture.ts
//!   QT_FIXTURE_HELP=/tmp/qt-help.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-tools.ts > /tmp/oracle-help-tools.ndjson
//! Run:
//!   QT_ORACLE_HELP=/tmp/oracle-help-tools.ndjson \
//!   QT_FIXTURE_HELP=/tmp/qt-help.db \
//!     cargo test -p quilltap-harness --test help_tools_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::Db;
use quilltap_core::tools::help;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userA")]
    user_a: String,
    #[serde(rename = "userB")]
    user_b: String,
    #[serde(rename = "settingsOps")]
    settings_ops: Vec<SettingsOp>,
    #[serde(rename = "navigateOps")]
    navigate_ops: Vec<NavigateOp>,
    #[serde(rename = "submitOps")]
    submit_ops: Vec<SubmitOp>,
}

#[derive(Deserialize)]
struct SettingsOp {
    user: String,
    args: Value,
}
#[derive(Deserialize)]
struct NavigateOp {
    args: Value,
}
#[derive(Deserialize)]
struct SubmitOp {
    #[serde(rename = "chatId")]
    chat_id: String,
    args: Value,
}

#[derive(Deserialize)]
struct OracleOp {
    output: Value,
    formatted: String,
}
#[derive(Deserialize)]
struct Oracle {
    settings: Vec<OracleOp>,
    navigate: Vec<OracleOp>,
    submit: Vec<OracleOp>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/help-tools-tier2.json")
}

/// Compare a single op's Output value + formatted string against the oracle.
fn assert_op(label: &str, i: usize, got_output: &Value, got_formatted: &str, oracle: &OracleOp) {
    assert_eq!(
        got_output,
        &oracle.output,
        "{label} op {i}: output diverged\n  rust:   {}\n  oracle: {}",
        serde_json::to_string(got_output).unwrap(),
        serde_json::to_string(&oracle.output).unwrap()
    );
    assert_eq!(
        got_formatted, oracle.formatted,
        "{label} op {i}: formatted diverged\n  rust:   {got_formatted:?}\n  oracle: {:?}",
        oracle.formatted
    );
}

#[tokio::test]
async fn help_tools_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP") else {
        eprintln!("SKIP: set QT_ORACLE_HELP to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_HELP") else {
        eprintln!("SKIP: set QT_FIXTURE_HELP to the fixture .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    assert_eq!(
        spec.settings_ops.len(),
        oracle.settings.len(),
        "settings op count: spec vs oracle"
    );
    assert_eq!(spec.navigate_ops.len(), oracle.navigate.len());
    assert_eq!(spec.submit_ops.len(), oracle.submit.len());

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-help-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // --- help_settings (DB read) ---
    for (i, op) in spec.settings_ops.iter().enumerate() {
        let user_id = if op.user == "A" {
            &spec.user_a
        } else {
            &spec.user_b
        };
        let out = help::execute_help_settings(&db, user_id, &op.args).await;
        let formatted = help::format_help_settings(&out);
        let got = serde_json::to_value(&out).expect("serialize help_settings output");
        assert_op("help_settings", i, &got, &formatted, &oracle.settings[i]);
    }

    // --- help_navigate (pure) ---
    for (i, op) in spec.navigate_ops.iter().enumerate() {
        let out = help::execute_help_navigate(&spec.user_a, &op.args);
        let formatted = help::format_help_navigate(&out);
        let got = serde_json::to_value(&out).expect("serialize help_navigate output");
        assert_op("help_navigate", i, &got, &formatted, &oracle.navigate[i]);
    }

    // --- submit_final_response (pure) ---
    for (i, op) in spec.submit_ops.iter().enumerate() {
        let out = help::execute_submit_final_response(&op.chat_id, &op.args);
        let formatted = help::format_submit_final_response(&out);
        let got = serde_json::to_value(&out).expect("serialize submit output");
        assert_op(
            "submit_final_response",
            i,
            &got,
            &formatted,
            &oracle.submit[i],
        );
    }

    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: help-tools matched oracle ({} settings, {} navigate, {} submit).",
        spec.settings_ops.len(),
        spec.navigate_ops.len(),
        spec.submit_ops.len()
    );
}
