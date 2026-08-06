//! Tier-2 differential test: the `conversation_chunks` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-conversation-chunks-fixture.ts), run the SAME
//! create / update / delete / clearEmbeddings op sequence from the committed
//! spec, dump the `conversation_chunks` table canonically, and assert the
//! post-op state is identical.
//!
//! P4.D25 — `clearEmbeddingsForChat`'s `olderThan` age guard (v4 `f7cc887b`).
//! Three `clearEmbeddings` ops close its arms: the guard ON (only rows STRICTLY
//! older than the cutoff go — the row stamped exactly at it and the row after it
//! are both spared), a second identical pass (0 rows), and the guard OFF. The
//! per-op row COUNTS are diffed first, because they are the direct observable of
//! the guard; a corpus-shape assertion pins them at `[1, 0, 2]` so a corpus that
//! stopped sparing anything cannot pass silently.
//!
//! A cold-tiered row's `updatedAt` is MINTED (v4 stamps `new Date()`; this side
//! injects `CLEAR_NOW_ISO`), so a timestamp absent from the committed spec is
//! placeholdered on both sides. Every pinned id and timestamp still compares
//! exactly — which is what makes "the guard SPARED this row" an assertion.
//!
//! A BLOB case (after `help_docs`): the `embedding` Float32 buffer is exercised
//! on insert (a non-empty `Vec<f32>` → the storage blob via
//! `embedding_blob::float32_to_blob`), as NULL (`None`/empty → SQL NULL), and —
//! the banked behavior — through an update that does NOT name the embedding
//! field, which must leave the BLOB untouched. The canonical dump renders BLOBs
//! as lowercase hex on both sides, so the deterministic Float32 buffer
//! (`[0.5,-0.25,0.75,0.125]`, since v4 dd0d9ff5 int8-quantized to
//! `eb0101040000000683c13b55d67f15`) compares bit-exact. It also banks REAL
//! `interchangeIndex` (integer-valued cells dump
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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quilltap_core::db::conversation_chunks::{CcCreate, CcUpdate, CcUpsert, CreateOptions};
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
    /// Bug 17 (`62ab1bc8`) — v4 `upsert(input)` on an EXISTING `(chatId,
    /// interchangeIndex)`. The three corpus ops close the embedding arms:
    /// supplied embedding wins / content change NULLs the stale vector /
    /// identical content preserves it. The update arm mints only `updatedAt`
    /// (v4 `new Date()`; v5 injects `UPSERT_NOW_ISO`), normalized on both sides.
    #[serde(rename = "upsert")]
    Upsert { data: CreateData },
    #[serde(rename = "delete")]
    Delete { id: String },
    /// P4.D25 — v4 `clearEmbeddingsForChat` with its new optional `olderThan`
    /// age guard. `older_than: None` in the spec means "call it with no cutoff".
    #[serde(rename = "clearEmbeddings")]
    ClearEmbeddings {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "olderThan")]
        older_than: Option<String>,
    },
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

/// The `now` stamp the Rust port injects into `clear_embeddings_for_chat`.
/// Deliberately NOT in the committed spec: v4 mints its own `new Date()` there,
/// so this value is v5-side only and must normalize to `<ts>` like the oracle's.
const CLEAR_NOW_ISO: &str = "2026-06-06T06:06:06.606Z";

/// The `now` the Rust port injects into `upsert`'s update arm (Bug 17). Like
/// `CLEAR_NOW_ISO`, deliberately NOT in the committed spec — v4's `_update` mints
/// `new Date()` there — so both sides' `updatedAt` normalize to `<ts>`. (The
/// `new_id` is unused: the corpus upserts all hit an EXISTING row.)
const UPSERT_NOW_ISO: &str = "2026-06-07T07:07:07.707Z";
const UPSERT_NEW_ID: &str = "00000000-0000-4000-8000-0000000000ff";

/// Every string literal anywhere in the committed spec — the values BOTH sides
/// pinned.
fn pinned_literals(spec: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                out.insert(s.clone());
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(spec, &mut out);
    out
}

/// Placeholder a cold-tiered row's minted `updatedAt`; leave every pinned
/// timestamp exact.
fn normalize(dump: &mut Value, label: &str, pinned: &BTreeSet<String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        let minted = match obj.get("updatedAt") {
            Some(Value::String(s)) => !pinned.contains(s),
            _ => false,
        };
        if minted {
            obj.insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
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

    // Every timestamp the corpus PINS. A row cold-tiered by
    // `clearEmbeddingsForChat` carries a minted `updatedAt` instead (v4 stamps
    // `new Date()`; this side injects CLEAR_NOW_ISO), so anything not in this
    // set is placeholdered on both dumps — and every pinned stamp still
    // compares exactly, which is what proves the age guard SPARED a row.
    let spec_json: Value = serde_json::from_str(&spec_text).expect("parse fixture spec json");
    let pinned = pinned_literals(&spec_json);

    // Parse the oracle's NDJSON: a `clearResults` line then the `dump` line.
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut want_cleared: Option<Vec<u64>> = None;
    let mut oracle: Option<Value> = None;
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("clearResults") => {
                want_cleared = Some(
                    v["cleared"]
                        .as_array()
                        .expect("cleared array")
                        .iter()
                        .map(|n| n.as_u64().expect("cleared count"))
                        .collect(),
                )
            }
            _ => oracle = Some(v),
        }
    }
    let want_cleared = want_cleared.expect("oracle emitted no clearResults line — regenerate");
    let mut oracle = oracle.expect("oracle emitted no dump line — regenerate");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-cc-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let mut got_cleared: Vec<u64> = Vec::new();
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
                                // The plain `update` op never touches the BLOB.
                                embedding: None,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .unwrap_or_else(|e| panic!("conversation_chunks.update {id}: {e}"));
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Upsert { data } => {
                    repo.upsert(
                        &CcUpsert {
                            chat_id: data.chat_id.clone(),
                            interchange_index: data.interchange_index,
                            content: data.content.clone(),
                            participant_names: data.participant_names.clone(),
                            message_ids: data.message_ids.clone(),
                            embedding: data.embedding.clone(),
                        },
                        UPSERT_NEW_ID,
                        UPSERT_NOW_ISO,
                    )
                    .unwrap_or_else(|e| panic!("conversation_chunks.upsert: {e}"));
                }
                Op::Delete { id } => {
                    let found = repo
                        .delete(id)
                        .unwrap_or_else(|e| panic!("conversation_chunks.delete {id}: {e}"));
                    assert!(found, "delete target {id} not found in fixture");
                }
                Op::ClearEmbeddings {
                    chat_id,
                    older_than,
                } => {
                    let cleared = repo
                        .clear_embeddings_for_chat(chat_id, CLEAR_NOW_ISO, older_than.as_deref())
                        .unwrap_or_else(|e| {
                            panic!("conversation_chunks.clear_embeddings_for_chat {chat_id}: {e}")
                        });
                    got_cleared.push(cleared as u64);
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("conversation_chunks", "id")
        .expect("dump conversation_chunks");

    let _ = std::fs::remove_file(&work);

    // The counts each `clearEmbeddings` returned are the DIRECT observable of
    // v4 f7cc887b's age guard — assert them before the state diff, so a guard
    // that cleared the wrong set names itself.
    assert_eq!(
        got_cleared, want_cleared,
        "clearEmbeddingsForChat row counts diverged"
    );
    // And pin the corpus's own shape: the guard must have SPARED something
    // (a first pass that cleared everything would agree with an unguarded port).
    assert_eq!(
        want_cleared,
        vec![1, 0, 2],
        "the corpus stopped exercising the olderThan guard (on / idempotent / off)"
    );

    normalize(&mut got, "rust", &pinned);
    normalize(&mut oracle, "oracle", &pinned);

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
