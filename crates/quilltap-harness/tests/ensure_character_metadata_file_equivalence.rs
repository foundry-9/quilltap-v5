//! Differential test: `ensure_character_metadata_file` (the lazy fact-sheet seed).
//!
//! Drives the REAL `ensure_character_metadata_file` over the SHARED
//! vault-read-overlay fixture (its stores already hold the three states — a store
//! with NO metadata.json, one with a VALID sheet, one with an INVALID sheet) and
//! compares `{ created, fileContent }` per store to v4's oracle.
//!
//! Pins the existence-not-parse contract: an absent file is seeded to `{}` (true);
//! a present file — VALID OR INVALID — is left byte-for-byte untouched (false).
//! A fat-fingered sheet is never "healed" into an empty one.
//!
//! Build the oracle (Node 24, from the v4 checkout / pinned worktree), AFTER
//! building the vault-read-overlay fixture:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
//!     $N/node --import tsx <V5>/harness/oracle/cases/ensure-metadata-file.ts \
//!     > /tmp/oracle-ensure-metadata-file.ndjson
//! Run:
//!   QT_ORACLE_ENSURE_METADATA_FILE=/tmp/oracle-ensure-metadata-file.ndjson \
//!   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
//!     cargo test -p quilltap-harness --test ensure_character_metadata_file_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::character_vault::ensure_character_metadata_file;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::Writer;
use serde::Deserialize;

const METADATA_JSON_PATH: &str = "metadata.json";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    stores: Vec<StoreCase>,
}

#[derive(Deserialize)]
struct StoreCase {
    label: String,
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
}

#[derive(Deserialize, PartialEq, Debug)]
struct OracleRow {
    label: String,
    created: bool,
    #[serde(rename = "fileContent")]
    file_content: Option<String>,
}

#[derive(Deserialize)]
struct Oracle {
    rows: Vec<OracleRow>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/ensure-metadata-file.json")
}

#[test]
fn ensure_character_metadata_file_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ENSURE_METADATA_FILE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_ENSURE_METADATA_FILE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VAULT_READ_OVERLAY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_VAULT_READ_OVERLAY to the seed fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    // Fresh copy so the shared seed fixture stays pristine (ensure WRITES to it).
    let work = std::env::temp_dir().join(format!("qt-ensure-meta-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut got: Vec<OracleRow> = Vec::new();
    for store in &spec.stores {
        let created = ensure_character_metadata_file(writer.connection(), &store.mount_point_id)
            .unwrap_or_else(|e| panic!("ensure [{}]: {e}", store.label));
        let file_content = DocMountDocumentsRepository::new(writer.connection())
            .find_by_mount_point_and_path(&store.mount_point_id, METADATA_JSON_PATH)
            .expect("read metadata.json");
        got.push(OracleRow {
            label: store.label.clone(),
            created,
            file_content,
        });
    }

    let _ = std::fs::remove_file(&work);

    assert_eq!(got.len(), oracle.rows.len(), "store count diverged");
    for (g, w) in got.iter().zip(oracle.rows.iter()) {
        assert_eq!(g, w, "ensure store [{}] diverged", w.label);
    }
    assert!(
        got.len() >= 3,
        "expected all three states, saw {}",
        got.len()
    );
    eprintln!(
        "OK: ensure_character_metadata_file matched oracle on {} stores.",
        got.len()
    );
}
