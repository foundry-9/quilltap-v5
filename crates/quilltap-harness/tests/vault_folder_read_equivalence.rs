//! Read-differential test: the vault read overlay's directory-listing load
//! (`DocMountDocumentsRepository::find_many_by_mount_points_in_folder`).
//!
//! Both v4 and the Rust port READ the SAME pre-seeded mount-index fixture, so
//! every minted id/timestamp is identical on both sides — the returned rows
//! compare EXACTLY (no normalization), only sorted by (mountPointId,
//! relativePath) since the read has no defined order. Covers the IN-clause across
//! two stores, the non-recursive single-level + extension filtering (a top-level
//! file, a nested file, and a wrong-extension file are all excluded), and the
//! empty-mount-point short-circuit.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-vault-folder-read-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-folder-read-fixture.ts
//!   QT_FIXTURE_VAULT_FOLDER_READ=/tmp/qt-vault-folder-read-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-folder-read.ts \
//!     > /tmp/oracle-vault-folder-read.ndjson
//! Run:
//!   QT_ORACLE_VAULT_FOLDER_READ=/tmp/oracle-vault-folder-read.ndjson \
//!   QT_FIXTURE_VAULT_FOLDER_READ=/tmp/qt-vault-folder-read-fixture.db \
//!     cargo test -p quilltap-harness --test vault_folder_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    label: String,
    #[serde(rename = "mountPointIds")]
    mount_point_ids: Vec<String>,
    folder: String,
    extension: String,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    rows: Vec<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-folder-read-tier2.json")
}

/// A returned row as a sortable JSON object (the six overlay-consumed fields).
fn row_to_value(d: &quilltap_core::db::doc_mount_documents::VaultFolderDoc) -> Value {
    json!({
        "content": d.content,
        "mountPointId": d.mount_point_id,
        "relativePath": d.relative_path,
        "fileName": d.file_name,
        "createdAt": d.created_at,
        "updatedAt": d.updated_at,
    })
}

/// Sort rows by (mountPointId, relativePath) — the read has no defined order.
fn sort_key(v: &Value) -> (String, String) {
    (
        v["mountPointId"].as_str().unwrap_or_default().to_string(),
        v["relativePath"].as_str().unwrap_or_default().to_string(),
    )
}

#[test]
fn vault_folder_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_FOLDER_READ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_FOLDER_READ to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VAULT_FOLDER_READ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_VAULT_FOLDER_READ to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle: Vec<OracleRow> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-vfr-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for q in &spec.queries {
        let repo = writer.doc_mount_documents();
        let got_docs = repo
            .find_many_by_mount_points_in_folder(&q.mount_point_ids, &q.folder, &q.extension)
            .unwrap_or_else(|e| panic!("[{}] read: {e}", q.label));
        let mut got: Vec<Value> = got_docs.iter().map(row_to_value).collect();
        got.sort_by_key(sort_key);

        let oracle_row = oracle
            .iter()
            .find(|r| r.label == q.label)
            .unwrap_or_else(|| panic!("oracle missing query {}", q.label));
        let mut want = oracle_row.rows.clone();
        want.sort_by_key(sort_key);

        assert_eq!(got, want, "[{}] folder-read rows diverged", q.label);
    }
    let _ = std::fs::remove_file(&work);

    eprintln!(
        "OK: vault folder-read matched oracle on {} queries.",
        spec.queries.len()
    );
}
