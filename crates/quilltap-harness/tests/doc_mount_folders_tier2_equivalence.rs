//! Tier-2 differential test: the `doc_mount_folders` repo — a **mount-index
//! sibling-DB repo** of Phase 2 (after the pilot `group_character_members`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture, which is the
//! mount-index sibling DB (`quilltap-mount-index.db`), not the main DB. The Rust
//! `Writer` is partition-agnostic — `open_writable` opens that file by path
//! exactly as it opens a main DB — so this test is shaped identically to the
//! main-DB tier-2 tests; only the fixture differs. Both run the SAME
//! create + update + delete op sequence from the committed spec, dump the
//! `doc_mount_folders` table canonically, and assert the post-op state is
//! identical. The corpus banks the nullable-UUID `parentId` column: the root
//! folder dumps `parentId: null`, the children dump it as a UUID. Ids and
//! timestamps are pinned on both sides → zero normalization.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmf-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-folders-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_FOLDERS=/tmp/qt-dmf-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-folders-tier2.ts \
//!     > /tmp/oracle-dmf.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_FOLDERS=/tmp/oracle-dmf.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_FOLDERS=/tmp/qt-dmf-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_folders_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_folders::{CreateOptions, DmfCreate, DmfUpdate};
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
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    name: String,
    path: String,
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
    #[serde(rename = "mountPointId")]
    mount_point_id: Option<String>,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    name: Option<String>,
    path: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-folders-tier2.json")
}

#[test]
fn doc_mount_folders_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_FOLDERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DOC_MOUNT_FOLDERS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_FOLDERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_DOC_MOUNT_FOLDERS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-dmf-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port. The Writer opens the
    // mount-index fixture by path — no special "mount-index writer" needed.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.doc_mount_folders();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => repo
                    .create(
                        &DmfCreate {
                            mount_point_id: data.mount_point_id.clone(),
                            parent_id: data.parent_id.clone(),
                            name: data.name.clone(),
                            path: data.path.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("doc_mount_folders.create"),
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &DmfUpdate {
                                mount_point_id: data.mount_point_id.clone(),
                                parent_id: data.parent_id.clone(),
                                name: data.name.clone(),
                                path: data.path.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("doc_mount_folders.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let removed = repo.delete(id).expect("doc_mount_folders.delete");
                    assert!(removed, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("doc_mount_folders", "id")
        .expect("dump doc_mount_folders");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: doc_mount_folders tier-2 matched oracle ({n} rows).");
}
