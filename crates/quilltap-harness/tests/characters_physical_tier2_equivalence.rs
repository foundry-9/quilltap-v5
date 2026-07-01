//! Tier-2 differential test: the physicalDescription-via-update seam of v4's
//! `CharactersRepository.update` (Phase-2, the store-backed capstone).
//!
//! Proves the ALREADY-PORTED physical-md/physical-json descriptor branch of
//! [`vault_character_update::apply_document_store_write_overlay`]: a non-null
//! `physicalDescription` update writes `physical-description.md` (fullDescription) +
//! `physical-prompts.json` (`render_physical_prompts_json`, the five-key variant),
//! strips the key from the DB-bound patch, and — because no slim column remains —
//! skips the slim `_update` so the slim row's `updatedAt` is NOT bumped.
//!
//! Both sides start from the SAME baked fixture (a character + vault created by
//! v4's real `repos.characters.create` with NO physicalDescription), apply the
//! SAME physicalDescription update, then SIX tables are structural-diffed (the main
//! slim `characters` row + the mount-index store tables). Minted-values remap with
//! ONE shared id-map; timestamps → `<ts>`; the link `chunkCount` → `<cc>`;
//! `doc_mount_chunks` excluded.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARPHYS_MAIN=/tmp/qt-charphys-main.db \
//!   QT_FIXTURE_CHARPHYS_MOUNT=/tmp/qt-charphys-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-physical-fixture.ts
//!   QT_FIXTURE_CHARPHYS_MAIN=/tmp/qt-charphys-main.db \
//!   QT_FIXTURE_CHARPHYS_MOUNT=/tmp/qt-charphys-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-physical.ts > /tmp/oracle-charphys.ndjson
//! Run:
//!   QT_ORACLE_CHARPHYS=/tmp/oracle-charphys.ndjson \
//!   QT_FIXTURE_CHARPHYS_MAIN=/tmp/qt-charphys-main.db \
//!   QT_FIXTURE_CHARPHYS_MOUNT=/tmp/qt-charphys-mount.db \
//!     cargo test -p quilltap-harness --test characters_physical_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_character_update::update_character;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    patch: Map<String, Value>,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "characters",
        oracle_key: "characters",
        order_by: "name",
        id_columns: &["id", "characterDocumentMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
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
        from_mount: true,
        pin_chunk_count: true,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-physical-tier2.json")
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
fn characters_physical_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARPHYS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARPHYS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARPHYS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARPHYS_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARPHYS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARPHYS_MOUNT to the mount fixture .db (header).");
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

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-charphys-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-charphys-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    let character_id: String = main
        .connection()
        .query_row("SELECT id FROM characters LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("read baked character id");

    for op in &spec.ops {
        update_character(
            main.connection(),
            mount.connection(),
            &character_id,
            &op.patch,
        )
        .unwrap_or_else(|e| panic!("update_character: {e}"));
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { &mount } else { &main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

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

    // Sanity: the two physical files were written with the new content — the
    // five-key physical-prompts.json (renderPhysicalPromptsJson) and the
    // fullDescription body.
    let docs_i = TABLES
        .iter()
        .position(|t| t.oracle_key == "documents")
        .unwrap();
    let docs = got[docs_i]["rows"].as_array().unwrap();
    let phys_md = docs.iter().any(|d| {
        d["content"]
            .as_str()
            .map(|c| c.contains("Tall, with ink-stained fingers and grey eyes."))
            .unwrap_or(false)
    });
    let phys_json = docs.iter().any(|d| {
        d["content"]
            .as_str()
            .map(|c| {
                c.contains("\"headAndShoulders\": \"a woman with grey eyes, ink on her cheek\"")
                    && c.contains("\"complete\":")
            })
            .unwrap_or(false)
    });
    assert!(phys_md, "physical-description.md was not written");
    assert!(
        phys_json,
        "physical-prompts.json (five-key) was not written"
    );

    eprintln!(
        "OK: characters physicalDescription-via-update tier-2 matched oracle (6 tables, 2 DBs)."
    );
}
