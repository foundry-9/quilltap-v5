//! Tier-2 differential test: the `chat_documents` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-chat-documents-fixture.ts), run the SAME create
//! / update / delete op sequence from the committed spec, dump the
//! `chat_documents` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks a plain (non-Taggable) AbstractBaseRepository with an
//! all-text-plus-one-boolean shape: two required TEXT strings (chatId,
//! filePath), an enum TEXT column (scope), two nullable TEXT columns
//! (mountPoint, displayTitle), and one boolean (isActive -> INTEGER 0/1). No
//! number, JSON, or BLOB columns.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cd-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-documents-fixture.ts
//!   QT_FIXTURE_CD=/tmp/qt-cd-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-documents-tier2.ts \
//!     > /tmp/oracle-cd.ndjson
//! Run:
//!   QT_ORACLE_CHAT_DOCUMENTS=/tmp/oracle-cd.ndjson \
//!   QT_FIXTURE_CHAT_DOCUMENTS=/tmp/qt-cd-fixture.db \
//!     cargo test -p quilltap-harness --test chat_documents_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chat_documents::{CdCreate, CdUpdate, CreateOptions};
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
    #[serde(rename = "filePath")]
    file_path: String,
    scope: String,
    #[serde(default, rename = "mountPoint")]
    mount_point: Option<String>,
    #[serde(default, rename = "displayTitle")]
    display_title: Option<String>,
    #[serde(rename = "isActive")]
    is_active: bool,
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
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "filePath")]
    file_path: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default, rename = "mountPoint")]
    mount_point: Option<String>,
    #[serde(default, rename = "displayTitle")]
    display_title: Option<String>,
    #[serde(default, rename = "isActive")]
    is_active: Option<bool>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-documents-tier2.json")
}

#[test]
fn chat_documents_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_DOCUMENTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_DOCUMENTS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHAT_DOCUMENTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CHAT_DOCUMENTS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-cd-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_documents();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CdCreate {
                            chat_id: data.chat_id.clone(),
                            file_path: data.file_path.clone(),
                            scope: data.scope.clone(),
                            mount_point: data.mount_point.clone(),
                            display_title: data.display_title.clone(),
                            is_active: data.is_active,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("chat_documents.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CdUpdate {
                                chat_id: data.chat_id.clone(),
                                file_path: data.file_path.clone(),
                                scope: data.scope.clone(),
                                mount_point: data.mount_point.clone(),
                                display_title: data.display_title.clone(),
                                is_active: data.is_active,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("chat_documents.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("chat_documents.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("chat_documents", "id")
        .expect("dump chat_documents");

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
    eprintln!("OK: chat_documents tier-2 matched oracle ({n} rows).");
}
