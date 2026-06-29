//! Tier-2 differential test: the `conversation_annotations` repo (Phase-2, after
//! `folders`, `tags`, `text_replacement_rules`, and `prompt_templates`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-conversation-annotations-fixture.ts), run the
//! SAME create / update / delete op sequence from the committed spec, dump the
//! `conversation_annotations` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! This banks a REAL-affinity unbounded-int column (`messageIndex` — REAL, not
//! INTEGER, because the Zod field is `.min(0)` with no `.max()`; bound as `f64`,
//! the integer-valued cell collapses back to an integer in the canonical dump)
//! and a nullable UUID column (`sourceMessageId`, exercised null and non-null).
//! No conflict detection here, so no expectThrow path.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ca-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-conversation-annotations-fixture.ts
//!   QT_FIXTURE_CA=/tmp/qt-ca-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/conversation-annotations-tier2.ts \
//!     > /tmp/oracle-ca.ndjson
//! Run:
//!   QT_ORACLE_CONV_ANNOTATIONS=/tmp/oracle-ca.ndjson \
//!   QT_FIXTURE_CONV_ANNOTATIONS=/tmp/qt-ca-fixture.db \
//!     cargo test -p quilltap-harness --test conversation_annotations_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::conversation_annotations::{CaCreate, CaUpdate, CreateOptions};
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
    #[serde(rename = "messageIndex")]
    message_index: f64,
    #[serde(default, rename = "sourceMessageId")]
    source_message_id: Option<String>,
    #[serde(rename = "characterName")]
    character_name: String,
    content: String,
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
    content: Option<String>,
    #[serde(default, rename = "characterName")]
    character_name: Option<String>,
    #[serde(default, rename = "messageIndex")]
    message_index: Option<f64>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/conversation-annotations-tier2.json")
}

#[test]
fn conversation_annotations_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CONV_ANNOTATIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONV_ANNOTATIONS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CONV_ANNOTATIONS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CONV_ANNOTATIONS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-ca-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.conversation_annotations();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CaCreate {
                            chat_id: data.chat_id.clone(),
                            message_index: data.message_index,
                            source_message_id: data.source_message_id.clone(),
                            character_name: data.character_name.clone(),
                            content: data.content.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("conversation_annotations.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CaUpdate {
                                content: data.content.clone(),
                                character_name: data.character_name.clone(),
                                message_index: data.message_index,
                                updated_at: data.updated_at.clone(),
                                ..Default::default()
                            },
                        )
                        .expect("conversation_annotations.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("conversation_annotations.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("conversation_annotations", "id")
        .expect("dump conversation_annotations");

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
    eprintln!("OK: conversation_annotations tier-2 matched oracle ({n} rows).");
}
