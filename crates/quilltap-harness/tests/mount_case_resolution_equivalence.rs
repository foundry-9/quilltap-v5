//! Tier-2 differential — the two case-preserving/adopting link upsert sites of
//! v4 `0a0419f5` that `doc_mount_file_links_tier2` does NOT cover:
//! `link_blob_content` (case-PRESERVING, like the document path) and
//! `link_filesystem_file` (case-ADOPTING — the filesystem is the source of
//! truth). Drives the ported repo over a fresh copy of the doc-mount-file-links
//! tier-2 fixture with the SAME sequences the oracle
//! (`harness/oracle/cases/mount-case-resolution.ts`) drives against v4's real
//! repo, then compares the deterministic casing columns (link relativePath /
//! fileName, folder path, file / blob row counts).
//!
//! Generate the oracle (Node 24, from the v4 checkout at `d68638b4`), AFTER
//! building the shared fixture:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_MOUNT_CASE_RES=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/mount-case-resolution.ts \
//!     > /tmp/oracle-mount-case-resolution.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_CASE_RES=/tmp/oracle-mount-case-resolution.ndjson \
//!   QT_FIXTURE_MOUNT_CASE_RES=/tmp/qt-dmfl-fixture.db \
//!     cargo test -p quilltap-harness --test mount_case_resolution_equivalence -- --nocapture

use quilltap_core::db::doc_mount_file_links::{LinkBlobInput, LinkFilesystemFileInput};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

const STORE_ID: &str = "5700000e-0000-4000-8000-0000000000d5";
const TEST_PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

fn project(dump: &Value, cols: &[&str]) -> Vec<Value> {
    dump["rows"]
        .as_array()
        .expect("rows array")
        .iter()
        .map(|r| {
            let mut obj = serde_json::Map::new();
            for c in cols {
                obj.insert((*c).to_string(), r[*c].clone());
            }
            Value::Object(obj)
        })
        .collect()
}

#[test]
fn mount_case_resolution_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MOUNT_CASE_RES") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_CASE_RES to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_MOUNT_CASE_RES") else {
        eprintln!("SKIP: set QT_FIXTURE_MOUNT_CASE_RES to the seed fixture .db (see header).");
        return;
    };

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .trim(),
    )
    .expect("parse oracle");

    let work = std::env::temp_dir().join(format!("qt-mount-case-res-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).expect("copy fixture");

    let writer = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
    {
        let repo = writer.doc_mount_file_links();

        // Blob case-PRESERVING: ART/logo.png onto Art/Logo.png updates in place,
        // keeps the stored casing; the Art folder is reused for the ART segment.
        let blob = |rel: &str, name: &str, bytes: &str| LinkBlobInput {
            mount_point_id: STORE_ID.to_string(),
            relative_path: rel.to_string(),
            file_name: name.to_string(),
            file_type: None,
            original_file_name: name.to_string(),
            original_mime_type: "image/png".to_string(),
            stored_mime_type: "image/png".to_string(),
            sha256: "0".repeat(64),
            data: bytes.as_bytes().to_vec(),
            description: None,
            conversion_status: None,
            extracted_text: None,
            extracted_text_sha256: None,
            extraction_status: None,
        };
        repo.link_blob_content(&blob("Art/Logo.png", "Logo.png", "first-logo-bytes"))
            .expect("blob A");
        repo.link_blob_content(&blob(
            "ART/logo.png",
            "logo.png",
            "second-logo-bytes-differ",
        ))
        .expect("blob B");

        // Filesystem case-ADOPTING: Notes/draft.md onto Notes/Draft.md updates in
        // place and ADOPTS the scanned casing.
        let fsfile = |rel: &str, name: &str, modified: &str| LinkFilesystemFileInput {
            mount_point_id: STORE_ID.to_string(),
            relative_path: rel.to_string(),
            file_name: name.to_string(),
            file_type: "markdown".to_string(),
            sha256: "a".repeat(64),
            file_size_bytes: 12.0,
            last_modified: modified.to_string(),
            source: None,
            conversion_status: None,
            conversion_error: None,
            plain_text_length: None,
            chunk_count: None,
            allow_embed: None,
            allow_character_read: None,
            allow_character_write: None,
        };
        repo.link_filesystem_file(&fsfile(
            "Notes/Draft.md",
            "Draft.md",
            "2024-05-01T00:00:00.000Z",
        ))
        .expect("fs A");
        repo.link_filesystem_file(&fsfile(
            "Notes/draft.md",
            "draft.md",
            "2024-05-02T00:00:00.000Z",
        ))
        .expect("fs B");
    }

    let links_dump = writer
        .dump_table_json("doc_mount_file_links", "relativePath")
        .expect("dump links");
    let folders_dump = writer
        .dump_table_json("doc_mount_folders", "path")
        .expect("dump folders");
    let files_dump = writer
        .dump_table_json("doc_mount_files", "sha256")
        .expect("dump files");
    let blobs_dump = writer
        .dump_table_json("doc_mount_blobs", "fileId")
        .expect("dump blobs");
    let _ = std::fs::remove_file(&work);

    let got = json!({
        "links": project(&links_dump, &["relativePath", "fileName"]),
        "folders": project(&folders_dump, &["path"]),
        "fileCount": files_dump["rows"].as_array().unwrap().len(),
        "blobCount": blobs_dump["rows"].as_array().unwrap().len(),
    });

    assert_eq!(
        got, oracle,
        "mount-case-resolution diverged from v4\n  rust:   {got}\n  oracle: {oracle}"
    );
    eprintln!("mount_case_resolution: matched oracle (blob case-preserving + fs case-adopting)");
}
