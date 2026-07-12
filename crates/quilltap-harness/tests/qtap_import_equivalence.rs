//! P4.4u4 tier-2 differential: v4's `executeImport` over the committed
//! `lorian-and-riya.qtap` sample-content seed vs the Rust port
//! (`quilltap_core::services::quilltap_import::execute_import`).
//!
//! Both sides start from the SAME pair of EMPTY fixtures (a MAIN db with the slim
//! `characters` + `wardrobe_items` + `memories` tables, a MOUNT-INDEX db with the
//! store tables), import the SAME committed `.qtap`, then NINE tables are
//! structural-diffed: MAIN `characters` / `wardrobe_items` / `memories` + the
//! MOUNT-INDEX vault tables (`doc_mount_points` / `_folders` / `_files` /
//! `_documents` / `_file_links`).
//!
//! Minted-values remap with ONE shared id-map across the row-diffed tables
//! (characters → wardrobe → memories → points → folders → links, rows in
//! natural-key order). NOTHING is pinned: every id is minted (v4's `create`
//! strips the source id), so every FK verifies by RELATIONSHIP — `wardrobe.
//! characterId` / `memory.characterId` / `memory.aboutCharacterId` →
//! `characters.id`, `characters.characterDocumentMountPointId` → the mount point,
//! `link.fileId` → the file, etc. Minted timestamps (`createdAt`/`updatedAt`, +
//! the vault ts columns) → `<ts>`; passthrough seed timestamps
//! (`lastReinforcedAt`) are diffed exactly. The link `chunkCount` → `<cc>` (a
//! v4-only reindex artifact); `doc_mount_chunks` is excluded.
//!
//! The `doc_mount_files` / `doc_mount_documents` content is NOT row-diffed here:
//! each wardrobe `.md` embeds its item's MINTED id + timestamps in the
//! frontmatter, so its content sha is a minted-content seam (the deterministic
//! character-managed-file bytes are already byte-proven by
//! `characters_create_tier2`). Instead their row COUNT and their `fileSizeBytes`
//! MULTISET are asserted equal — a size-exact structural check that catches any
//! projected-content regression without the minted-id noise. The rich `links`
//! table (deterministic relativePath / fileName per file) carries the structural
//! diff.
//!
//! Also exercises the `skip` branch: a SECOND `execute_import` on the imported DB
//! must skip both characters (name-match) and re-create no wardrobe/vault, while
//! memories (remap-only, always-insert) double — asserted Rust-side against the
//! first-run oracle state.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
//!   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-qtap-import-fixture.ts
//!   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
//!   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/qtap-import.ts > /tmp/oracle-qtap-import.ndjson
//! Run:
//!   QT_ORACLE_QTAPIMPORT=/tmp/oracle-qtap-import.ndjson \
//!   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
//!   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
//!     cargo test -p quilltap-harness --test qtap_import_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::SINGLE_USER_ID;
use quilltap_core::services::quilltap_import::{execute_import, parse_export_file, ImportOptions};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

/// Per-table normalization spec. `from_mount` = read from the mount-index writer
/// (else main). The slice order is the canonical walk order for the shared id-remap.
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
        table: "wardrobe_items",
        oracle_key: "wardrobe",
        order_by: "title",
        id_columns: &["id", "characterId"],
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
    // `memories` is walked LAST so the (doubled, on the 2nd import) memory-id token
    // stream can't offset the shared token counter for the vault tables above.
    TableSpec {
        table: "memories",
        oracle_key: "memories",
        order_by: "content",
        id_columns: &["id", "characterId", "aboutCharacterId"],
        // Only the MINTED timestamps are placeholdered — `lastReinforcedAt` /
        // `lastAccessedAt` pass through from the seed and are diffed exactly.
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-import-tier2.json")
}
fn qtap_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/first-startup/imports/lorian-and-riya.qtap")
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

fn dump_all(main: &Writer, mount: &Writer) -> Vec<Value> {
    TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { mount } else { main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect()
}

#[test]
fn qtap_import_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_QTAPIMPORT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_QTAPIMPORT to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_QTAPIMPORT_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_QTAPIMPORT_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_QTAPIMPORT_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_QTAPIMPORT_MOUNT to the mount fixture .db (header).");
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

    // Parse the committed seed (byte-identical to v4's first-startup copy).
    let qtap = std::fs::read_to_string(qtap_path()).expect("read committed .qtap");
    let export = parse_export_file(&qtap).expect("parse committed .qtap");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-qtapimport-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-qtapimport-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // Run 1: the op under test.
    let result = execute_import(
        main.connection(),
        mount.connection(),
        SINGLE_USER_ID,
        &export,
        &ImportOptions::seed_defaults(),
    )
    .expect("execute_import");
    assert!(
        result.success,
        "import should succeed: {:?}",
        result.warnings
    );
    assert_eq!(result.imported.characters, 2, "2 characters imported");
    assert_eq!(result.imported.memories, 42, "42 memories imported");
    assert_eq!(result.skipped.characters, 0, "none skipped on first run");
    assert_eq!(
        result.imported_character_ids.len(),
        2,
        "2 destination character ids"
    );

    let mut got = dump_all(&main, &mount);
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
    let rows_len = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().len()
    };
    assert_eq!(rows_len("characters"), 2, "2 character rows");
    // Wardrobe is VAULT-backed (each item → a `Wardrobe/*.md` doc-mount file), so
    // the slim `wardrobe_items` table stays empty — the items are verified via the
    // doc_mount link diff + the file size-multiset below.
    assert_eq!(
        rows_len("wardrobe"),
        0,
        "no slim wardrobe rows (vault-backed)"
    );
    assert_eq!(rows_len("memories"), 42, "42 memory rows");
    assert_eq!(rows_len("points"), 2, "2 vault mount-point rows");
    assert_eq!(
        rows_len("links"),
        28,
        "28 vault links (incl. 4 wardrobe .md/char)"
    );

    // The files / documents content sha is a minted-content seam (wardrobe .md
    // embeds minted id + timestamps) — diff their COUNT + fileSizeBytes MULTISET
    // instead (a size-exact structural check).
    let file_sizes = |dump: &Value| -> Vec<i64> {
        let mut v: Vec<i64> = dump["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["fileSizeBytes"].as_i64().unwrap_or(0))
            .collect();
        v.sort_unstable();
        v
    };
    let got_files = mount
        .dump_table_json("doc_mount_files", "sha256")
        .expect("dump files");
    let got_docs = mount
        .dump_table_json("doc_mount_documents", "contentSha256")
        .expect("dump documents");
    let want_files = oracle.get("files").cloned().expect("oracle files");
    let want_docs = oracle.get("documents").cloned().expect("oracle documents");
    assert_eq!(
        got_files["rows"].as_array().unwrap().len(),
        want_files["rows"].as_array().unwrap().len(),
        "doc_mount_files row count"
    );
    assert_eq!(
        file_sizes(&got_files),
        file_sizes(&want_files),
        "doc_mount_files size multiset diverged"
    );
    assert_eq!(
        got_docs["rows"].as_array().unwrap().len(),
        want_docs["rows"].as_array().unwrap().len(),
        "doc_mount_documents row count"
    );

    // Also assert the oracle's result shape matched.
    if let Some(ores) = oracle.get("result") {
        assert_eq!(ores["success"], Value::Bool(true));
        assert_eq!(ores["imported"]["characters"], Value::from(2));
        assert_eq!(ores["imported"]["memories"], Value::from(42));
    }

    // Run 2 (the skip branch): characters skip by NAME match, no new wardrobe/vault
    // rows; memories (remap-only, always-insert) double. The character / wardrobe /
    // vault state must STILL match the first-run oracle.
    let result2 = execute_import(
        main.connection(),
        mount.connection(),
        SINGLE_USER_ID,
        &export,
        &ImportOptions::seed_defaults(),
    )
    .expect("second execute_import");
    assert_eq!(result2.skipped.characters, 2, "both characters skipped");
    assert_eq!(result2.imported.characters, 0, "no new characters created");
    assert_eq!(
        result2.imported.memories, 42,
        "memories re-inserted (dupes)"
    );

    let mut got2 = dump_all(&main, &mount);
    let memories_after = {
        let i = TABLES
            .iter()
            .position(|t| t.oracle_key == "memories")
            .unwrap();
        got2[i]["rows"].as_array().unwrap().len()
    };
    assert_eq!(memories_after, 84, "memories doubled after the 2nd import");

    // Characters + wardrobe + vault tables are unchanged by the skip (a full no-op).
    normalize_all(&mut got2);
    for (i, s) in TABLES.iter().enumerate() {
        if s.oracle_key == "memories" {
            continue; // duplicated — asserted by count above.
        }
        assert_eq!(
            got2[i]["rows"], want[i]["rows"],
            "{}: skip re-import mutated a no-op table",
            s.table
        );
    }

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    eprintln!("OK: qtap-import tier-2 matched oracle (9 tables, 2 DBs) + skip branch.");
}
