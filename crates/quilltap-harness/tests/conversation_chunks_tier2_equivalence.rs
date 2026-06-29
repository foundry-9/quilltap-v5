//! Tier-2 differential test: the `conversation_chunks` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-conversation-chunks-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `conversation_chunks` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! A BLOB case (after `help_docs`): the `embedding` Float32 buffer is exercised
//! on insert (a non-empty `Vec<f32>` → little-endian f32 bytes via
//! `embedding_blob::float32_to_blob`), as NULL (`None`/empty → SQL NULL), and —
//! the banked behavior — through an update that does NOT name the embedding
//! field, which must leave the BLOB untouched. The canonical dump renders BLOBs
//! as lowercase hex on both sides, so the deterministic Float32 buffer
//! (`[0.5,-0.25,0.75,0.125]` → `0000003f000080be0000403f0000003e`) compares
//! bit-exact. It also banks REAL `interchangeIndex` (integer-valued cells dump
//! as JSON integers) and the JSON array columns `participantNames`/`messageIds`.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cc-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-conversation-chunks-fixture.ts
//!   QT_FIXTURE_CC=/tmp/qt-cc-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/conversation-chunks-tier2.ts \
//!     > /tmp/oracle-cc.ndjson
//! Run:
//!   QT_ORACLE_CONVERSATION_CHUNKS=/tmp/oracle-cc.ndjson \
//!   QT_FIXTURE_CONVERSATION_CHUNKS=/tmp/qt-cc-fixture.db \
//!     cargo test -p quilltap-harness --test conversation_chunks_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::conversation_chunks::{CcCreate, CcUpdate, CreateOptions};
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
    #[serde(rename = "interchangeIndex")]
    interchange_index: f64,
    content: String,
    #[serde(rename = "participantNames")]
    participant_names: Vec<String>,
    #[serde(rename = "messageIds")]
    message_ids: Vec<String>,
    /// `null` in the spec deserializes to `None`; a JSON number array to a
    /// `Vec<f32>` (f64 literals truncated to f32 — the fixture uses only
    /// exactly-representable values, so the cast is lossless).
    #[serde(default)]
    embedding: Option<Vec<f32>>,
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
    #[serde(default, rename = "interchangeIndex")]
    interchange_index: Option<f64>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default, rename = "participantNames")]
    participant_names: Option<Vec<String>>,
    #[serde(default, rename = "messageIds")]
    message_ids: Option<Vec<String>>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/conversation-chunks-tier2.json")
}

#[test]
fn conversation_chunks_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CONVERSATION_CHUNKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONVERSATION_CHUNKS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CONVERSATION_CHUNKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CONVERSATION_CHUNKS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-cc-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.conversation_chunks();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CcCreate {
                            chat_id: data.chat_id.clone(),
                            interchange_index: data.interchange_index,
                            content: data.content.clone(),
                            participant_names: data.participant_names.clone(),
                            message_ids: data.message_ids.clone(),
                            embedding: data.embedding.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .unwrap_or_else(|e| panic!("conversation_chunks.create {}: {e}", options.id));
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CcUpdate {
                                chat_id: data.chat_id.clone(),
                                interchange_index: data.interchange_index,
                                content: data.content.clone(),
                                participant_names: data.participant_names.clone(),
                                message_ids: data.message_ids.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .unwrap_or_else(|e| panic!("conversation_chunks.update {id}: {e}"));
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo
                        .delete(id)
                        .unwrap_or_else(|e| panic!("conversation_chunks.delete {id}: {e}"));
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("conversation_chunks", "id")
        .expect("dump conversation_chunks");

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
    eprintln!("OK: conversation_chunks tier-2 matched oracle ({n} rows).");
}
