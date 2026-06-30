//! Tier-2 differential: the character vault MANAGED-FIELDS WRITE projection
//! (`writeCharacterVaultManagedFields`).
//!
//! Both v4 and the Rust port run the SAME projection sequence against a copy of the
//! same seeded fixture, then dump the doc-store tables. v4's post-write
//! `reindexSingleFile` runs (database-backed stores chunk with no model); its only
//! divergence — the link `chunkCount` and the `doc_mount_chunks` rows — is
//! pinned/excluded here, exactly as the groups/projects/wardrobe store-backed tests
//! do. The Rust port leaves `chunkCount` at its DDL default and writes no chunks.
//!
//! Five tables (`doc_mount_points` / `_files` / `_documents` / `_file_links` /
//! `_folders`) diffed in the minted-values remap form with a shared cross-table
//! id-map (so document.fileId, link.fileId/folderId/mountPointId, folder.parentId
//! verify by relationship). The store `mountPointId` is the one pinned id.
//!
//! Banks: a full create (properties.json, the five markdown files,
//! physical-description.md + physical-prompts.json, a Prompts/ filename collision
//! `Default Voice.md`/`Default Voice-1.md`, two Scenarios), then a reproject that
//! sweeps the dropped prompt + both old scenarios, clears physicalDescription
//! (physical-* files PERSIST — v4 skips, does not delete), and renders
//! `talkativeness: 1` (the integer-valued-float seam, end-to-end through the sha).
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-vault-character-write-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-character-write-fixture.ts
//!   QT_FIXTURE_VAULT_CHARACTER_WRITE=/tmp/qt-vault-character-write-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-character-write.ts \
//!     > /tmp/oracle-vault-character-write.ndjson
//! Run:
//!   QT_ORACLE_VAULT_CHARACTER_WRITE=/tmp/oracle-vault-character-write.ndjson \
//!   QT_FIXTURE_VAULT_CHARACTER_WRITE=/tmp/qt-vault-character-write-fixture.db \
//!     cargo test -p quilltap-harness --test vault_character_write_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_character_write::{
    write_character_vault_managed_fields, CharacterVaultWriteInput,
};
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
    character: Value,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        pin_chunk_count: true,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-character-write-tier2.json")
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));
        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[test]
fn vault_character_write_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_CHARACTER_WRITE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_CHARACTER_WRITE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VAULT_CHARACTER_WRITE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_VAULT_CHARACTER_WRITE to the seed fixture .db.");
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
    .expect("parse oracle");

    let store_id = "57c0de00-0000-4000-8000-0000000000d1";

    let work = std::env::temp_dir().join(format!("qt-vcw-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    {
        let links = writer.doc_mount_file_links();
        let docs = writer.doc_mount_documents();
        for (i, op) in spec.ops.iter().enumerate() {
            let input: CharacterVaultWriteInput = serde_json::from_value(op.character.clone())
                .unwrap_or_else(|e| panic!("deserialize op {i} character: {e}"));
            write_character_vault_managed_fields(&links, &docs, store_id, &input)
                .unwrap_or_else(|e| panic!("write_character_vault_managed_fields op {i}: {e:?}"));
        }
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            writer
                .dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let _ = std::fs::remove_file(&work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Final folder state: the swept Scenarios are gone, the renamed one survives,
    // the dropped Prompts/ collision file is gone, and the physical-* files persist
    // even though op 2 cleared physicalDescription.
    let links_idx = TABLES.iter().position(|t| t.oracle_key == "links").unwrap();
    let link_paths: Vec<String> = got[links_idx]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["relativePath"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        link_paths.iter().any(|p| p == "Scenarios/Rooftop Redux.md"),
        "renamed scenario missing; links: {link_paths:?}"
    );
    assert!(
        !link_paths.iter().any(|p| p == "Scenarios/Workshop.md"),
        "swept scenario should be gone; links: {link_paths:?}"
    );
    assert!(
        !link_paths.iter().any(|p| p == "Prompts/Default Voice-1.md"),
        "dropped collision prompt should be swept; links: {link_paths:?}"
    );
    assert!(
        link_paths.iter().any(|p| p == "physical-prompts.json"),
        "physical files should persist after clear; links: {link_paths:?}"
    );

    eprintln!("OK: vault character managed-fields write projection matched oracle (5 tables).");
}
