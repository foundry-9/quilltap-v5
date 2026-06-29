//! Tier-2 differential test: the document-store STORAGE PRIMITIVE
//! (`writeDatabaseDocument` / `linkDocumentContent` / `ensureLinkFolderId`).
//!
//! Both sides run the SAME write sequence (from the committed spec) against the
//! SAME mount-index fixture, then the resulting FOUR tables — `doc_mount_files`,
//! `doc_mount_documents`, `doc_mount_file_links`, `doc_mount_folders` — are
//! structural-diffed. `linkDocumentContent` mints every id and a single internal
//! `now`, so this is the **minted-values remap form**, but extended across four
//! tables:
//!
//!   - **shared id remap.** A SINGLE first-seen-token map is built by walking all
//!     four dumps in a fixed order (files → documents → links → folders, rows in
//!     natural-key order). So a cross-table FK (e.g. `doc_mount_documents.fileId`
//!     → `doc_mount_files.id`, `doc_mount_file_links.folderId` →
//!     `doc_mount_folders.id`) verifies the RELATIONSHIP without pinning the id.
//!     `mountPointId` is the seeded store id — pinned, identical both sides — so
//!     it is NOT remapped and matches outright.
//!   - **timestamps** → `<ts>` placeholder. The `createdAt == updatedAt` create
//!     invariant is intentionally NOT asserted: an op that rewrites a path
//!     upsert-updates its link (refreshing `updatedAt`/`lastModified` while
//!     preserving `createdAt`), so the two legitimately differ.
//!
//! The corpus exercises: a fresh JSON + markdown write, subfolder creation,
//! dedup-by-sha (a second path with byte-identical content reuses one file + one
//! document row), link upsert-in-place (rewriting a path), and the markdown
//! frontmatter policy cascade (`character_read: false` → all `allow*` = 0),
//! verified against v4's real yaml-based `policyFromContent`.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-file-links-tier2.ts \
//!     > /tmp/oracle-dmfl.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_FILE_LINKS=/tmp/oracle-dmfl.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_FILE_LINKS=/tmp/qt-dmfl-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_file_links_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    store: Store,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Store {
    id: String,
}

#[derive(Deserialize)]
struct Op {
    #[serde(rename = "relativePath")]
    relative_path: String,
    content: String,
}

/// Per-table (natural-key order_by, minted-id columns, timestamp columns). The
/// table walk order here is the canonical order the shared id-remap follows.
struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_file_links",
        order_by: "relativePath",
        // mountPointId is the pinned seeded store id — NOT remapped.
        id_columns: &["id", "fileId", "folderId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
    },
    TableSpec {
        table: "doc_mount_folders",
        order_by: "path",
        id_columns: &["id", "parentId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-file-links-tier2.json")
}

/// Normalize one dump's `rows` in place against the SHARED id-map: first-seen id
/// remap over the listed id columns, then timestamp placeholdering. The map is
/// shared across tables (passed by &mut) so cross-table FKs resolve to the same
/// tokens. Rows are already in natural-key order (the dump sorted them), identical
/// on both sides.
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
    }
}

/// Normalize all four dumps with one shared id-map, walking tables in `TABLES`
/// order so the first-seen token assignment is identical on both sides.
fn normalize_all(dumps: &mut [Value; 4]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[test]
fn doc_mount_file_links_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_FILE_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DOC_MOUNT_FILE_LINKS to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_FILE_LINKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_DOC_MOUNT_FILE_LINKS to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-dmfl-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME write sequence through the Rust port (minting our own ids/ts).
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.doc_mount_file_links();
        for op in &spec.ops {
            repo.write_database_document(&spec.store.id, &op.relative_path, &op.content)
                .expect("write_database_document");
        }
    }

    let mut got: [Value; 4] = std::array::from_fn(|i| {
        writer
            .dump_table_json(TABLES[i].table, TABLES[i].order_by)
            .unwrap_or_else(|e| panic!("dump {}: {e}", TABLES[i].table))
    });
    let _ = std::fs::remove_file(&work);

    let mut want: [Value; 4] = std::array::from_fn(|i| {
        oracle
            .get(oracle_key(TABLES[i].table))
            .cloned()
            .unwrap_or_else(|| panic!("oracle missing dump for {}", TABLES[i].table))
    });

    // One normalization, applied to both sides with a shared cross-table id-map.
    normalize_all(&mut got);
    normalize_all(&mut want);

    for i in 0..4 {
        let table = TABLES[i].table;
        assert_eq!(got[i]["table"], want[i]["table"], "{table}: table name");
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{table}: column set / order"
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{table}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the corpus produced the expected shape and the remap fired.
    let files = got[0]["rows"].as_array().expect("files rows");
    let docs = got[1]["rows"].as_array().expect("documents rows");
    let links = got[2]["rows"].as_array().expect("links rows");
    let folders = got[3]["rows"].as_array().expect("folders rows");
    assert_eq!(files.len(), 5, "expected 5 deduped file rows");
    assert_eq!(docs.len(), 5, "expected 5 document rows");
    assert_eq!(links.len(), 5, "expected 5 link rows");
    assert_eq!(folders.len(), 1, "expected 1 folder row (Knowledge)");

    // The dedup invariant: alias.md and the FIRST description.md write share content,
    // so two link rows reference the same file id (post-remap token).
    let alias = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("alias.md".into()))
        .expect("alias.md link");
    assert!(
        alias["fileId"].as_str().unwrap().starts_with("ID_"),
        "fileId was not remapped"
    );

    // The policy cascade: secret.md (character_read:false) → all allow* = 0.
    let secret = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("secret.md".into()))
        .expect("secret.md link");
    assert_eq!(secret["allowEmbed"], Value::from(0), "secret allowEmbed");
    assert_eq!(
        secret["allowCharacterRead"],
        Value::from(0),
        "secret allowCharacterRead"
    );
    assert_eq!(
        secret["allowCharacterWrite"],
        Value::from(0),
        "secret allowCharacterWrite"
    );

    // A permissive file keeps all flags 1 (sanity on the default path).
    let props = links
        .iter()
        .find(|r| r["relativePath"] == Value::String("properties.json".into()))
        .expect("properties.json link");
    assert_eq!(props["allowEmbed"], Value::from(1), "properties allowEmbed");

    eprintln!("OK: doc_mount_file_links storage-primitive tier-2 matched oracle (4 tables).");
}

/// Map a table name to the JSON key the oracle emits it under.
fn oracle_key(table: &str) -> &'static str {
    match table {
        "doc_mount_files" => "files",
        "doc_mount_documents" => "documents",
        "doc_mount_file_links" => "links",
        "doc_mount_folders" => "folders",
        other => panic!("unknown table {other}"),
    }
}
