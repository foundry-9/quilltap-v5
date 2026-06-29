//! Tier-2 differential test #1: the `folders` repo (Phase-2 on-ramp pilot).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-folders-fixture.ts), run the SAME create +
//! update op sequence from the committed spec, dump the `folders` table
//! canonically, and assert the post-op state is identical. Ids and timestamps
//! are pinned on both sides, so the dumps must match with zero normalization.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-folders-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-folders-fixture.ts
//!   QT_FIXTURE_FOLDERS=/tmp/qt-folders-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/folders-tier2.ts \
//!     > /tmp/oracle-folders.ndjson
//! Run:
//!   QT_ORACLE_FOLDERS=/tmp/oracle-folders.ndjson \
//!   QT_FIXTURE_FOLDERS=/tmp/qt-folders-fixture.db \
//!     cargo test -p quilltap-harness --test folders_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::folders::{CreateOptions, FolderCreate, FolderUpdate};
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
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    path: String,
    name: String,
    #[serde(rename = "parentFolderId")]
    parent_folder_id: Option<String>,
    #[serde(rename = "projectId")]
    project_id: Option<String>,
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
    name: Option<String>,
    path: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/folders-tier2.json")
}

#[test]
fn folders_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FOLDERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FOLDERS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_FOLDERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_FOLDERS to the seed fixture .db (see test header).");
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
    let work = std::env::temp_dir().join(format!("qt-folders-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let folders = writer.folders();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => folders
                    .create(
                        &FolderCreate {
                            user_id: data.user_id.clone(),
                            path: data.path.clone(),
                            name: data.name.clone(),
                            parent_folder_id: data.parent_folder_id.clone(),
                            project_id: data.project_id.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("folders.create"),
                Op::Update { id, data } => {
                    let found = folders
                        .update(
                            id,
                            &FolderUpdate {
                                name: data.name.clone(),
                                path: data.path.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("folders.update");
                    assert!(found, "update target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("folders", "id")
        .expect("dump folders");

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
    eprintln!("OK: folders tier-2 matched oracle ({n} rows).");
}
