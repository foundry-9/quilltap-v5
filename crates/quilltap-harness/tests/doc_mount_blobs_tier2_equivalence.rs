//! Tier-2 differential test: `doc_mount_blobs` (the binary byte-store, step 8).
//!
//! Both sides run the SAME `upsertByFileId` sequence (from the committed spec)
//! against the SAME mount-index fixture (which seeds the parent `doc_mount_files`
//! rows the blob FK requires), then `doc_mount_blobs` is structural-diffed with
//! the `data` BLOB as lowercase hex (bit-exact, mirrors `help_docs` /
//! `doc_mount_chunks`). `upsertByFileId` mints `id` + timestamps, so this is the
//! minted-values remap form: `id` → first-seen token, timestamps → `<ts>`;
//! `fileId` is the pinned seeded parent id and `sha256` / `sizeBytes` /
//! `storedMimeType` / `data` are deterministic content (compared directly).
//!
//! The corpus banks: a fresh insert, an overwrite-in-place on a repeat `fileId`
//! (same row id, new bytes/sha/size/mime, `createdAt` preserved), the
//! **sha-recompute rule** (every op passes an all-zero advisory sha — the stored
//! sha must be `sha256(data)`), and a non-UTF-8 binary payload (a PNG header +
//! `deadbeef`) round-tripping through the BLOB column.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-blobs-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-blobs-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_BLOBS=/tmp/qt-blobs-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-blobs-tier2.ts > /tmp/oracle-blobs.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_BLOBS=/tmp/oracle-blobs.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_BLOBS=/tmp/qt-blobs-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_blobs_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_blobs::UpsertBlobInput;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    #[serde(rename = "fileId")]
    file_id: String,
    #[serde(rename = "dataHex")]
    data_hex: String,
    sha256: String,
    #[serde(rename = "storedMimeType")]
    stored_mime_type: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-blobs-tier2.json")
}

/// Remap the minted `id`; placeholder timestamps. `fileId` is pinned (seeded
/// parent), `sha256` / `sizeBytes` / `storedMimeType` / `data` are deterministic.
fn normalize(dump: &mut Value) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump has rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row is object");
        if let Some(Value::String(raw)) = obj.get("id") {
            let next = format!("ID_{}", id_map.len());
            let token = id_map.entry(raw.clone()).or_insert(next).clone();
            obj.insert("id".into(), Value::String(token));
        }
        for col in ["createdAt", "updatedAt"] {
            if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert(col.into(), Value::String("<ts>".into()));
            }
        }
    }
}

#[test]
fn doc_mount_blobs_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_BLOBS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_DOC_MOUNT_BLOBS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_BLOBS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_DOC_MOUNT_BLOBS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-blobs-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture: {e}"));
    {
        let repo = writer.doc_mount_blobs();
        for op in &spec.ops {
            let data = hex_decode(&op.data_hex);
            repo.upsert_by_file_id(&UpsertBlobInput {
                file_id: op.file_id.clone(),
                sha256: op.sha256.clone(),
                stored_mime_type: op.stored_mime_type.clone(),
                data,
            })
            .expect("upsert_by_file_id");
        }
    }

    let mut got = writer
        .dump_table_json("doc_mount_blobs", "fileId")
        .expect("dump");
    let _ = std::fs::remove_file(&work);
    let mut want = oracle.clone();

    normalize(&mut got);
    normalize(&mut want);

    assert_eq!(got["columns"], want["columns"], "column set / order");
    assert_eq!(
        got["rows"], want["rows"],
        "doc_mount_blobs row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], want["rows"]
    );

    let rows = got["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "2 blob rows (FILE_A overwritten + FILE_B)");

    // The sha-recompute rule: the stored sha is sha256(data), never the all-zero
    // advisory. So no row carries the advisory zeros.
    for r in rows {
        assert_ne!(
            r["sha256"],
            Value::String("0".repeat(64)),
            "stored sha must be recomputed, not the advisory value"
        );
    }
    // The binary payload round-tripped through the BLOB (FILE_B = the PNG bytes).
    let file_b = rows
        .iter()
        .find(|r| r["fileId"] == Value::String("f11e0000-0000-4000-8000-00000000000b".into()))
        .expect("FILE_B row");
    assert_eq!(
        file_b["data"],
        Value::String("89504e470d0a1a0a0000000d49484452deadbeef".into()),
        "binary BLOB round-trip"
    );

    eprintln!("OK: doc_mount_blobs tier-2 matched oracle (binary byte-store).");
}

/// Decode an even-length lowercase-hex string to bytes (test-local; the corpus
/// hex is well-formed).
fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}
