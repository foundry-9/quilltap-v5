//! Tier-2 differential test: the `terminal_sessions` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-terminal-sessions-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `terminal_sessions` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks a clean strings-plus-nullables shape (NO boolean, NO JSON column):
//! three nullable string columns (`label`, `exitedAt`, `transcriptPath`) and one
//! nullable REAL-affinity unbounded-int column (`exitCode`, bound `Option<f64>`;
//! an integer-valued cell like 137 dumps back as the JSON integer 137 via
//! js_number_to_json).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ts-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-terminal-sessions-fixture.ts
//!   QT_FIXTURE_TS=/tmp/qt-ts-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/terminal-sessions-tier2.ts \
//!     > /tmp/oracle-ts.ndjson
//! Run:
//!   QT_ORACLE_TERMINAL_SESSIONS=/tmp/oracle-ts.ndjson \
//!   QT_FIXTURE_TERMINAL_SESSIONS=/tmp/qt-ts-fixture.db \
//!     cargo test -p quilltap-harness --test terminal_sessions_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::terminal_sessions::{CreateOptions, TsCreate, TsUpdate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(default)]
    label: Option<String>,
    shell: String,
    cwd: String,
    #[serde(rename = "startedAt")]
    started_at: String,
    #[serde(default, rename = "exitedAt")]
    exited_at: Option<String>,
    #[serde(default, rename = "exitCode")]
    exit_code: Option<f64>,
    #[serde(default, rename = "transcriptPath")]
    transcript_path: Option<String>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct UpdateData {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default, rename = "startedAt")]
    started_at: Option<String>,
    #[serde(default, rename = "exitedAt")]
    exited_at: Option<String>,
    #[serde(default, rename = "exitCode")]
    exit_code: Option<f64>,
    #[serde(default, rename = "transcriptPath")]
    transcript_path: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/terminal-sessions-tier2.json")
}

#[test]
fn terminal_sessions_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TERMINAL_SESSIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TERMINAL_SESSIONS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TERMINAL_SESSIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_TERMINAL_SESSIONS to the seed fixture .db (see test header)."
            );
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-ts-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.terminal_sessions();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &TsCreate {
                            chat_id: data.chat_id.clone(),
                            label: data.label.clone(),
                            shell: data.shell.clone(),
                            cwd: data.cwd.clone(),
                            started_at: data.started_at.clone(),
                            exited_at: data.exited_at.clone(),
                            exit_code: data.exit_code,
                            transcript_path: data.transcript_path.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("terminal_sessions.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &TsUpdate {
                                label: data.label.clone(),
                                shell: data.shell.clone(),
                                cwd: data.cwd.clone(),
                                started_at: data.started_at.clone(),
                                exited_at: data.exited_at.clone(),
                                exit_code: data.exit_code,
                                transcript_path: data.transcript_path.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("terminal_sessions.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("terminal_sessions.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("terminal_sessions", "id")
        .expect("dump terminal_sessions");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: terminal_sessions tier-2 matched oracle ({n} rows).");
}
