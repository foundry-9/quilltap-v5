//! Differential test: the character fact-sheet (`metadata.json`) round-trip
//! through `apply_document_store_write_overlay`.
//!
//! Drives the REAL write overlay over the shared charupd two-DB fixture (a
//! character + linked vault). Per op it records whether `metadata` survived into
//! the DB-bound patch and the resulting metadata.json bytes, then compares to
//! v4's oracle (which drives the real `applyDocumentStoreWriteOverlay`).
//!
//! Pins the whole-object-REPLACE semantics that make `metadata` unlike every
//! other vault JSON file: a subsequent patch REPLACES the file (dropped keys are
//! gone, never merged); an empty object OR `null` clears it to `{}`; an
//! unmanaged-only patch leaves the file untouched; and a replace overwrites an
//! unparseable file WITHOUT reading it first (the `presetMetadataFile` op).
//!
//! Reuses the charupd fixtures (build-characters-update-fixture.ts). Build the
//! oracle (Node 24, from the v4 checkout / pinned worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     $N/node --import tsx <V5>/harness/oracle/cases/metadata-vault-roundtrip.ts \
//!     > /tmp/oracle-metadata-roundtrip.ndjson
//! Run:
//!   QT_ORACLE_METADATA_ROUNDTRIP=/tmp/oracle-metadata-roundtrip.ndjson \
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     cargo test -p quilltap-harness --test metadata_vault_roundtrip_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::vault_character_update::apply_document_store_write_overlay;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

const METADATA_JSON_PATH: &str = "metadata.json";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    label: String,
    patch: Map<String, Value>,
    #[serde(rename = "presetMetadataFile")]
    preset_metadata_file: Option<String>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct OracleRow {
    label: String,
    #[serde(rename = "dbPatchHasMetadata")]
    db_patch_has_metadata: bool,
    #[serde(rename = "fileContent")]
    file_content: Option<String>,
}

#[derive(Deserialize)]
struct Oracle {
    rows: Vec<OracleRow>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/metadata-vault-roundtrip.json")
}

#[test]
fn metadata_vault_roundtrip_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_METADATA_ROUNDTRIP") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_METADATA_ROUNDTRIP to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARUPD_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARUPD_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARUPD_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARUPD_MOUNT to the mount fixture .db (header).");
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

    // Fresh copies so the shared baked fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-metaroundtrip-main-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-metaroundtrip-mount-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // The baked character id + its vault mount FK (both sides target the same).
    let character_id: String = main
        .connection()
        .query_row("SELECT id FROM characters LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("read baked character id");
    let mount_point_id: String = main
        .connection()
        .query_row(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            [&character_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("read vault fk")
        .expect("baked character has a vault");

    let links = DocMountFileLinksRepository::new(mount.connection());
    let docs = DocMountDocumentsRepository::new(mount.connection());

    let mut got: Vec<OracleRow> = Vec::new();
    for op in &spec.ops {
        if let Some(preset) = &op.preset_metadata_file {
            // Seed a (possibly unparseable) file BEFORE the overlay runs, to prove
            // the whole-object replace overwrites it without reading it first.
            links
                .write_database_document(&mount_point_id, METADATA_JSON_PATH, preset)
                .expect("preset metadata.json");
        }
        let db_patch = apply_document_store_write_overlay(
            main.connection(),
            mount.connection(),
            &character_id,
            &op.patch,
        )
        .unwrap_or_else(|e| panic!("apply_document_store_write_overlay [{}]: {e}", op.label));
        let file_content = docs
            .find_by_mount_point_and_path(&mount_point_id, METADATA_JSON_PATH)
            .expect("read metadata.json");
        got.push(OracleRow {
            label: op.label.clone(),
            db_patch_has_metadata: db_patch.contains_key("metadata"),
            file_content,
        });
    }

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    assert_eq!(
        got.len(),
        oracle.rows.len(),
        "op count diverged (got {}, want {})",
        got.len(),
        oracle.rows.len()
    );
    for (g, w) in got.iter().zip(oracle.rows.iter()) {
        assert_eq!(g, w, "metadata round-trip op [{}] diverged", w.label);
    }
    assert!(
        got.len() >= 7,
        "expected the full op corpus, saw {}",
        got.len()
    );
    eprintln!(
        "OK: metadata vault round-trip matched oracle on {} ops.",
        got.len()
    );
}
