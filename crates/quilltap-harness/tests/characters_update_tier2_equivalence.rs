//! Tier-2 differential test: v4's `CharactersRepository.update` (Phase-2, the
//! store-backed capstone sub-unit 4a — the update vault integration).
//!
//! Both sides start from the SAME baked fixture (a character + vault created by
//! v4's real `repos.characters.create`), apply the SAME update ops, then SIX
//! tables are structural-diffed: the main slim `characters` row + the mount-index
//! store tables (`doc_mount_points` / `_folders` / `_files` / `_documents` /
//! `_file_links`). The Rust port drives
//! [`vault_character_update::update_character`] over two writers; v4 drives the
//! real `repos.characters.update` (see the oracle).
//!
//! The character id is read back from the fixture so both sides target the same
//! minted id. Minted-values remap with ONE shared id-map across all six tables
//! (FKs verify by relationship); timestamps → `<ts>`; the link `chunkCount` →
//! `<cc>` (reindex artifact); `doc_mount_chunks` excluded.
//!
//! Banks: markdown routing (`identity.md` rewritten), the **properties.json
//! read-modify-write** (a `title`-only change preserves pronouns/aliases/
//! firstMessage/talkativeness), a DB-only field update (`isFavorite` true→false →
//! the slim `_update`), and a `systemPrompts` reprojection (sweep the old
//! `Prompts/Default.md`, write the new one) on a managed-only update (slim row not
//! re-touched).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-update-fixture.ts
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-update.ts > /tmp/oracle-charupd.ndjson
//! Run:
//!   QT_ORACLE_CHARUPD=/tmp/oracle-charupd.ndjson \
//!   QT_FIXTURE_CHARUPD_MAIN=/tmp/qt-charupd-main.db \
//!   QT_FIXTURE_CHARUPD_MOUNT=/tmp/qt-charupd-mount.db \
//!     cargo test -p quilltap-harness --test characters_update_tier2_equivalence

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
        .join("../../harness/oracle/fixtures/characters-update-tier2.json")
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
fn characters_update_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARUPD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARUPD to the oracle NDJSON (see header).");
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

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared baked fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-charupd-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-charupd-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // Read the baked character id (both sides target the same one).
    let character_id: String = main
        .connection()
        .query_row("SELECT id FROM characters LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("read baked character id");

    // The ops under test.
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

    // Sanity: the properties.json RMW preserved the untouched keys (title changed
    // to "the spy"; pronouns/aliases/firstMessage/talkativeness unchanged).
    let docs_i = TABLES
        .iter()
        .position(|t| t.oracle_key == "documents")
        .unwrap();
    let docs = got[docs_i]["rows"].as_array().unwrap();
    let props_rmw = docs.iter().any(|d| {
        d["content"]
            .as_str()
            .map(|c| {
                c.contains("\"title\": \"the spy\"")
                    && c.contains("\"firstMessage\": \"Well met, traveller.\"")
                    && c.contains("\"talkativeness\": 0.7")
                    && c.contains("\"subject\": \"she\"")
            })
            .unwrap_or(false)
    });
    assert!(
        props_rmw,
        "properties.json RMW did not preserve untouched keys"
    );

    eprintln!("OK: characters update tier-2 matched oracle (6 tables, 2 DBs).");
}
