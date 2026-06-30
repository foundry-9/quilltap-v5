//! Tier-2 differential test: v4's `scaffoldCharacterMount` (Phase-2, mount-index
//! DB, the store-backed capstone sub-unit 3a).
//!
//! Both sides start from the SAME mount-index fixture (a seeded database-backed
//! character mount point + the materialized store tables), run the SAME op —
//! scaffold that mount point — then FIVE mount-index tables are structural-diffed:
//! `doc_mount_points` (the unchanged seed), `doc_mount_folders` (7),
//! `doc_mount_files` (3, deduped), `doc_mount_documents` (3), and
//! `doc_mount_file_links` (8). The Rust port calls
//! [`character_vault::scaffold_character_mount`]; v4 drives the real
//! `scaffoldCharacterMount` (see the oracle).
//!
//! Minted-values remap with ONE shared id-map: a single first-seen-token map is
//! built by walking all tables in a fixed order (points → folders → files →
//! documents → links, rows in natural-key order). So every cross-table FK —
//! `folder.mountPointId`/`link.mountPointId` → the store, `link.fileId` →
//! `file.id`, `document.fileId` → `file.id` — verifies by RELATIONSHIP. The seeded
//! `mountPointId` is pinned, so it maps to the same token on both sides.
//! Timestamps → `<ts>`; the link `chunkCount` → `<cc>` (a v4-only
//! `reindexSingleFile` artifact the Rust storage primitive does not rebuild);
//! `doc_mount_chunks` is excluded entirely.
//!
//! Banks: the seven folders, the six blank `.md` files deduped to ONE
//! file/document row (six distinct links), and the two seeded JSON files with
//! their FIXED default content (`properties.json` + the four-key
//! `physical-prompts.json`).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SCAFFOLD_MAIN=/tmp/qt-scaffold-main.db \
//!   QT_FIXTURE_SCAFFOLD_MOUNT=/tmp/qt-scaffold-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-scaffold-fixture.ts
//!   QT_FIXTURE_SCAFFOLD_MAIN=/tmp/qt-scaffold-main.db \
//!   QT_FIXTURE_SCAFFOLD_MOUNT=/tmp/qt-scaffold-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-scaffold.ts > /tmp/oracle-scaffold.ndjson
//! Run:
//!   QT_ORACLE_SCAFFOLD=/tmp/oracle-scaffold.ndjson \
//!   QT_FIXTURE_SCAFFOLD_MOUNT=/tmp/qt-scaffold-mount.db \
//!     cargo test -p quilltap-harness --test characters_scaffold_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::character_vault::scaffold_character_mount;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
}

/// Per-table normalization spec. `oracle_key` = the JSON key the oracle emits it
/// under. The slice order here is the canonical walk order for the shared id-remap.
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
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
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
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-scaffold-tier2.json")
}

/// Remap id columns (shared map), placeholder timestamps, pin `chunkCount`.
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
fn characters_scaffold_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SCAFFOLD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SCAFFOLD to the oracle NDJSON (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_SCAFFOLD_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_SCAFFOLD_MOUNT to the mount-index fixture .db (header)."
            );
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

    // Fresh copy so the shared seed fixture stays pristine.
    let pid = std::process::id();
    let mount_work = std::env::temp_dir().join(format!("qt-scaffold-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // The op under test: scaffold the seeded mount point.
    scaffold_character_mount(mount.connection(), &spec.mount_point_id)
        .unwrap_or_else(|e| panic!("scaffold: {e}"));

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            mount
                .dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
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

    // Sanity: the corpus produced the expected shape.
    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(
        rows("points").len(),
        1,
        "1 mount-point row (unchanged seed)"
    );
    assert_eq!(rows("folders").len(), 7, "7 top-level folders");
    assert_eq!(
        rows("files").len(),
        3,
        "3 deduped file rows (blank, props, physical)"
    );
    assert_eq!(rows("documents").len(), 3, "3 document rows");
    assert_eq!(rows("links").len(), 8, "8 links (6 blank md + 2 json)");

    // The two seeded JSON files carry the FIXED default content.
    let docs = rows("documents");
    let props_default = "{\n  \"pronouns\": null,\n  \"aliases\": [],\n  \"title\": \"\",\n  \"firstMessage\": \"\",\n  \"talkativeness\": 0.5\n}";
    let physical_default =
        "{\n  \"short\": null,\n  \"medium\": null,\n  \"long\": null,\n  \"complete\": null\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(props_default.into())),
        "default properties.json content not found; documents: {docs:?}"
    );
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(physical_default.into())),
        "default physical-prompts.json content not found"
    );
    // The six blank markdown files dedup to the empty-string document.
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String("".into())),
        "empty blank-markdown document `\"\"` not found"
    );

    eprintln!("OK: characters scaffold tier-2 matched oracle (5 mount-index tables).");
}
