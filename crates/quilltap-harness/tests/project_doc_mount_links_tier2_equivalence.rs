//! Tier-2 differential test: the `project_doc_mount_links` repo — a mount-index
//! sibling-DB repo of Phase 2, a near-clone of the `group_character_members`
//! pilot.
//!
//! Structural DB diff. Both sides start from the SAME seed fixture, which is the
//! mount-index sibling DB (`quilltap-mount-index.db`), not the main DB. The Rust
//! `Writer` is partition-agnostic — `open_writable` opens that file by path
//! exactly as it opens a main DB — so this test is shaped identically to the
//! main-DB tier-2 tests; only the fixture differs. Both run the SAME
//! create + update + delete op sequence from the committed spec, dump the
//! `project_doc_mount_links` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides → zero normalization.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-pdml-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-project-doc-mount-links-fixture.ts
//!   QT_FIXTURE_PROJECT_DOC_MOUNT_LINKS=/tmp/qt-pdml-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/project-doc-mount-links-tier2.ts \
//!     > /tmp/oracle-pdml.ndjson
//! Run:
//!   QT_ORACLE_PROJECT_DOC_MOUNT_LINKS=/tmp/oracle-pdml.ndjson \
//!   QT_FIXTURE_PROJECT_DOC_MOUNT_LINKS=/tmp/qt-pdml-fixture.db \
//!     cargo test -p quilltap-harness --test project_doc_mount_links_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::project_doc_mount_links::{CreateOptions, PdmlCreate, PdmlUpdate};
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
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
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
    #[serde(rename = "projectId")]
    project_id: Option<String>,
    #[serde(rename = "mountPointId")]
    mount_point_id: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/project-doc-mount-links-tier2.json")
}

#[test]
fn project_doc_mount_links_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROJECT_DOC_MOUNT_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PROJECT_DOC_MOUNT_LINKS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PROJECT_DOC_MOUNT_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_PROJECT_DOC_MOUNT_LINKS to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-pdml-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port. The Writer opens the
    // mount-index fixture by path — no special "mount-index writer" needed.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.project_doc_mount_links();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => repo
                    .create(
                        &PdmlCreate {
                            project_id: data.project_id.clone(),
                            mount_point_id: data.mount_point_id.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("project_doc_mount_links.create"),
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &PdmlUpdate {
                                project_id: data.project_id.clone(),
                                mount_point_id: data.mount_point_id.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("project_doc_mount_links.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let removed = repo.delete(id).expect("project_doc_mount_links.delete");
                    assert!(removed, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("project_doc_mount_links", "id")
        .expect("dump project_doc_mount_links");

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
    eprintln!("OK: project_doc_mount_links tier-2 matched oracle ({n} rows).");
}
