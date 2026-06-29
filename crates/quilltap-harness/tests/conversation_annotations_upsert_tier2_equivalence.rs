//! Tier-2 differential test: the `conversation_annotations` repo's `upsert`
//! method, in the MINTED-VALUES (remap) form.
//!
//! `upsert(input)` finds ONE existing row by the unique key (chatId,
//! messageIndex, characterName); if found it `_update`s ONLY content +
//! sourceMessageId (re-minting updatedAt, preserving id+createdAt); else it
//! `_create`s a new row, minting id + createdAt + updatedAt to the same now.
//! Because the op mints its own id + timestamps, NOTHING is pinned: both v4 and
//! the Rust port independently mint random UUIDs and wall-clock timestamps, so
//! the raw dumps cannot match. They are reconciled by normalizing only the
//! legitimately nondeterministic fields, then structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`content`) order — identical
//!     on both sides because each row's content is a distinct input, not
//!     generated. Walking that order, each `id` value gets a first-seen canonical
//!     token (`ID_0`, `ID_1`, …). (Unlike `folders`, there is no FK-to-a-minted-id
//!     column here, so only `id` is remapped; `chatId` / `sourceMessageId` are
//!     deterministic inputs and MUST match exactly — they are not remapped.)
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. The
//!     `folders` create-invariant `createdAt == updatedAt` is NOT asserted here:
//!     the update path legitimately re-mints `updatedAt` to a later instant than
//!     the preserved `createdAt`, so the two differ for the updated rows.
//!
//! The SAME normalization runs over both the oracle dump and the Rust dump (one
//! implementation, here), so the remap is provably consistent — the oracle stays
//! a raw emitter. Both upsert paths (create + update) are exercised; the nullable
//! `sourceMessageId` is banked both null and non-null.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ca-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-conversation-annotations-upsert-fixture.ts
//!   QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT=/tmp/qt-ca-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/conversation-annotations-upsert-tier2.ts \
//!     > /tmp/oracle-ca-upsert.ndjson
//! Run:
//!   QT_ORACLE_CONVERSATION_ANNOTATIONS_UPSERT=/tmp/oracle-ca-upsert.ndjson \
//!   QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT=/tmp/qt-ca-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test conversation_annotations_upsert_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::conversation_annotations::CaUpsertInput;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

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
    #[serde(rename = "upsert")]
    Upsert { input: UpsertInput },
}

#[derive(Deserialize)]
struct UpsertInput {
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

/// Columns that hold a generated id. `upsert` only mints the PK; `chatId` /
/// `sourceMessageId` are deterministic inputs (NOT remapped).
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/update time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/conversation-annotations-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen `id` remap
/// over the rows in their given (content-sorted) order, then timestamp
/// placeholdering. No `createdAt == updatedAt` assertion — the update path
/// legitimately re-mints `updatedAt`.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));

    let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));

        for col in ID_COLUMNS {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn conversation_annotations_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CONVERSATION_ANNOTATIONS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONVERSATION_ANNOTATIONS_UPSERT to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CONVERSATION_ANNOTATIONS_UPSERT to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-ca-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert sequence through the Rust port, minting our own
    // ids/timestamps.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.conversation_annotations();
        for op in &spec.ops {
            let Op::Upsert { input } = op;
            repo.upsert(&CaUpsertInput {
                chat_id: input.chat_id.clone(),
                message_index: input.message_index,
                source_message_id: input.source_message_id.clone(),
                character_name: input.character_name.clone(),
                content: input.content.clone(),
            })
            .expect("conversation_annotations.upsert");
        }
    }

    let mut got = writer
        .dump_table_json("conversation_annotations", "content")
        .expect("dump conversation_annotations");

    let _ = std::fs::remove_file(&work);

    // One normalization, applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "remapped row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    // Sanity: the final state has the expected row count (2 seed + 2 created via
    // the two CREATE upserts; the two UPDATE upserts hit existing rows).
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        4,
        "expected four final rows (2 seed + 2 created)"
    );

    // Guard against a no-op normalization: the `id` column must be a token now.
    let m: &Map<String, Value> = rows[0].as_object().unwrap();
    assert!(
        m["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: conversation_annotations upsert tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
