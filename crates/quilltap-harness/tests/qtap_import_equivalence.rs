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
//! (`lastReinforcedAt`) are diffed exactly. The link `chunkCount` diffs EXACTLY
//! since P4.6BK (v5 chunks on write); `doc_mount_chunks` is excluded.
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
//! Also exercises the `skip` branch: a re-import of the seed on the imported DB
//! must skip both characters (name-match) and re-create no wardrobe/vault, while
//! memories (remap-only, always-insert) double — asserted Rust-side against the
//! first-run oracle state.
//!
//! THE BUG-75 LEG (v4 `40d507cc`): both sides additionally import the committed
//! `qtap-import-bug75.qtap` — a character whose wardrobe carries a depth-2
//! composite chain plus one dangling component reference. Item ids are
//! re-minted on import, so stored `componentItemIds` must be REMAPPED to the
//! new sibling ids (created leaf-first) and the dangling reference dropped with
//! v4's exact warning. The size-multiset check is BLIND to this (a 36-char UUID
//! remaps size-neutrally), so the items are read back through each side's REAL
//! vault-overlay reader and diffed by RELATIONSHIP: item ids and every
//! `componentItemIds` element go through one shared token map — a hollow import
//! (old export ids) yields tokens matching no sibling row and diverges loudly.
//!
//! THE BUG-117 LEG (v4 `0b0617fee`): a files-only `.qtap` whose PNG row carries
//! the PRE-TRANSCODE hash — what a pre-4.9.0 exporting instance wrote. The
//! importer took `sha256` from the archive row beside the post-bridge
//! `mimeType`/`size`, so a bitmap the bridge transcoded left a FileEntry that
//! cannot be joined to the mount blob it points at. It runs on an ISOLATED
//! second copy of the fixtures (its two blobs would otherwise change the
//! `doc_mount_files` size multiset above, and those sizes legitimately differ
//! between sharp's WebP and the harness codec's bytes), with the uploads mount
//! PLANTED identically on both sides. The comparand is the WITHIN-TREE boolean
//! `files.sha256 == doc_mount_blobs.sha256`.
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
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
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
fn bug75_qtap_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-import-bug75.qtap")
}

fn bug117_qtap_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-import-bug117.qtap")
}

/// Project a vault-overlaid wardrobe item to the bug-75 comparand fields
/// (mirrors the oracle case's projection).
fn bug75_project(item: &Value) -> Value {
    serde_json::json!({
        "id": item.get("id").cloned().unwrap_or(Value::Null),
        "title": item.get("title").cloned().unwrap_or(Value::Null),
        "types": item.get("types").cloned().unwrap_or(Value::Null),
        "componentItemIds": item.get("componentItemIds").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        "isDefault": item.get("isDefault").cloned().unwrap_or(Value::Bool(false)),
        "replace": item.get("replace").cloned().unwrap_or(Value::Bool(false)),
    })
}

/// Remap the bug-75 items' ids AND `componentItemIds` elements through ONE
/// shared token map (title-sorted walk). An element that maps to no sibling
/// row id keeps its raw UUID — which is exactly how a hollow import shows up.
fn bug75_normalize(items: &mut [Value]) {
    let mut map: HashMap<String, String> = HashMap::new();
    for it in items.iter() {
        if let Some(id) = it.get("id").and_then(Value::as_str) {
            let next = format!("W_{}", map.len());
            map.entry(id.to_string()).or_insert(next);
        }
    }
    for it in items.iter_mut() {
        let obj = it.as_object_mut().expect("bug75 item object");
        let token = obj
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| map.get(id).cloned());
        if let Some(token) = token {
            obj.insert("id".to_string(), Value::String(token));
        }
        if let Some(Value::Array(refs)) = obj.get_mut("componentItemIds") {
            for r in refs.iter_mut() {
                if let Value::String(s) = r {
                    if let Some(tok) = map.get(s.as_str()) {
                        *r = Value::String(tok.clone());
                    }
                }
            }
        }
    }
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
        None,
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

    // The Bug-75 leg: import the committed composite fixture (see header).
    let bug75_qtap =
        std::fs::read_to_string(bug75_qtap_path()).expect("read committed bug75 .qtap");
    let bug75_export = parse_export_file(&bug75_qtap).expect("parse bug75 .qtap");
    let bug75_result = execute_import(
        main.connection(),
        mount.connection(),
        SINGLE_USER_ID,
        &bug75_export,
        &ImportOptions::seed_defaults(),
        None,
    )
    .expect("bug75 execute_import");
    assert!(
        bug75_result.success,
        "bug75 import should succeed: {:?}",
        bug75_result.warnings
    );
    assert_eq!(
        bug75_result.imported_character_ids.len(),
        1,
        "1 bug75 destination character id"
    );
    let bram_id = bug75_result.imported_character_ids[0].clone();

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
    assert_eq!(
        rows_len("characters"),
        3,
        "2 seed character rows + the bug75 character"
    );
    // Wardrobe is VAULT-backed (each item → a `Wardrobe/*.md` doc-mount file), so
    // the slim `wardrobe_items` table stays empty — the items are verified via the
    // doc_mount link diff + the file size-multiset below.
    assert_eq!(
        rows_len("wardrobe"),
        0,
        "no slim wardrobe rows (vault-backed)"
    );
    assert_eq!(rows_len("memories"), 42, "42 memory rows");
    assert_eq!(rows_len("points"), 3, "3 vault mount-point rows");

    // The link TOTAL is deliberately not pinned to a literal. It is two halves and
    // only one of them is ours: the `Wardrobe/*.md` links come from the committed
    // `.qtap` seed (4 items per character) and are fixed, while every other link is
    // a file from v4's own `scaffoldCharacterMount` list — which GROWS. It gained
    // `metadata.json` in 4.8.0 (v4 `8bc43333`, squashed into `d68638b4`), taking the
    // total 28 → 30, and the hand-written 28 then went red for a v4 feature v5 had
    // already ported (P4.6az) — a false alarm the row-for-row diff above had already
    // cleared. So the scaffold half is asserted by its SHAPE instead: one link per
    // scaffold file per imported character. The diff above is what proves the port;
    // this block only catches a degenerate corpus.
    let links = {
        let i = TABLES.iter().position(|t| t.oracle_key == "links").unwrap();
        got[i]["rows"].as_array().unwrap()
    };
    let (wardrobe, scaffold): (Vec<&str>, Vec<&str>) = links
        .iter()
        .map(|r| r["relativePath"].as_str().unwrap_or_default())
        .partition(|p| p.starts_with("Wardrobe/"));
    assert_eq!(
        wardrobe.len(),
        14,
        "4 wardrobe `.md` × 2 seed characters + 6 bug75 items: {wardrobe:?}"
    );
    let mut distinct_wardrobe = wardrobe.clone();
    distinct_wardrobe.sort_unstable();
    distinct_wardrobe.dedup();
    assert_eq!(
        distinct_wardrobe.len(),
        wardrobe.len(),
        "wardrobe item titles collided into one path: {wardrobe:?}"
    );
    let mut scaffold_counts: HashMap<&str, usize> = HashMap::new();
    for p in &scaffold {
        *scaffold_counts.entry(p).or_default() += 1;
    }
    let mut offenders: Vec<String> = scaffold_counts
        .iter()
        .filter(|(_, n)| **n != 3)
        .map(|(p, n)| format!("{p} ×{n}"))
        .collect();
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "every vault scaffold file should appear once per imported character: {offenders:?}"
    );
    assert!(
        scaffold_counts.contains_key("properties.json"),
        "the vault keystone `properties.json` is missing from the links"
    );
    assert!(
        scaffold_counts.len() >= 10,
        "only {} distinct vault scaffold files — the corpus looks degenerate",
        scaffold_counts.len()
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

    // The Bug-75 comparand: read Bram's items back through v5's REAL
    // vault-overlay reader and diff by relationship against v4's read (see
    // header). Warnings diff EXACTLY — the dangling reference must produce
    // v4's sentence, byte for byte.
    let obug75 = oracle.get("bug75").expect(
        "oracle missing the bug75 section — regenerate the NDJSON from a case file that has the Bug-75 leg",
    );
    let oracle_warnings: Vec<String> = obug75["warnings"]
        .as_array()
        .expect("bug75.warnings")
        .iter()
        .map(|w| w.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        bug75_result.warnings, oracle_warnings,
        "bug75 import warnings diverged"
    );
    assert!(
        oracle_warnings
            .iter()
            .any(|w| w.contains("component item(s) not present in the import")),
        "the corpus lost its dangling-reference arm: {oracle_warnings:?}"
    );

    let docs = quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository::new(
        mount.connection(),
    );
    let mut got_items: Vec<Value> = quilltap_core::db::wardrobe_read::find_by_character_id(
        main.connection(),
        &docs,
        &bram_id,
        true,
    )
    .expect("read bram wardrobe")
    .iter()
    .map(bug75_project)
    .collect();
    got_items.sort_by(|a, b| {
        a["title"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["title"].as_str().unwrap_or_default())
    });
    let mut want_items: Vec<Value> = obug75["items"].as_array().expect("bug75.items").clone();
    assert_eq!(
        got_items.len(),
        6,
        "6 bug75 wardrobe items expected: {got_items:?}"
    );
    bug75_normalize(&mut got_items);
    bug75_normalize(&mut want_items);
    assert_eq!(
        got_items, want_items,
        "bug75 componentItemIds relationship state diverged"
    );
    // Belt and braces: the composite chain actually RESOLVED — every remaining
    // component reference is a token of a sibling row (a hollow import leaves
    // raw UUIDs here).
    for it in &got_items {
        for r in it["componentItemIds"].as_array().unwrap() {
            let s = r.as_str().unwrap_or_default();
            assert!(
                s.starts_with("W_"),
                "unresolved component reference survived the import: {s} in {it}"
            );
        }
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
        None,
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

    // ── P4.D152 / bug 117 (v4 `0b0617fee`) — an ISOLATED second pair ──────────
    //
    // See the oracle case's comment: the importer took `sha256` from the ARCHIVE
    // row beside the post-bridge `mimeType`/`size`, so a bitmap the bridge
    // transcoded left a FileEntry that cannot be joined to the mount blob it
    // points at. The hash STRING cannot cross the trees (v4 stores real sharp
    // WebP), so the comparand is the WITHIN-TREE boolean, and the import runs on
    // a SECOND copy of the fixtures so its two blobs cannot disturb the
    // `doc_mount_files` size multiset the main diff asserts.
    //
    // The uploads mount is PLANTED, identically to the oracle's plant — fixture
    // scaffolding, not ported code: the shared fixtures are empty and
    // `write_user_upload_to_mount_store` refuses without a
    // `userUploadsMountPointId` pointing at a database-backed store.
    {
        let main2_path = std::env::temp_dir().join(format!("qt-qtapimport-b117-main-{pid}.db"));
        let mount2_path = std::env::temp_dir().join(format!("qt-qtapimport-b117-mount-{pid}.db"));
        let _ = std::fs::remove_file(&main2_path);
        let _ = std::fs::remove_file(&mount2_path);
        std::fs::copy(&main_fixture, &main2_path).unwrap_or_else(|e| panic!("copy main2: {e}"));
        std::fs::copy(&mount_fixture, &mount2_path).unwrap_or_else(|e| panic!("copy mount2: {e}"));
        let main2 = Writer::open_writable(&main2_path, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open main2: {e}"));
        let mount2 = Writer::open_writable(&mount2_path, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open mount2: {e}"));

        const UPLOADS_MP: &str = "aaaaaaaa-0000-4000-8000-00000000dd01";
        const TS: &str = "2026-02-28T17:11:10.563Z";
        mount2
            .connection()
            .execute(
                "INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, createdAt, updatedAt) \
                 VALUES (?1, 'Quilltap Uploads', '', 'database', 'documents', ?2, ?2)",
                rusqlite::params![UPLOADS_MP, TS],
            )
            .expect("plant uploads mount");
        main2
            .connection()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
            )
            .expect("plant instance_settings");
        main2
            .connection()
            .execute(
                "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('userUploadsMountPointId', ?1)",
                rusqlite::params![UPLOADS_MP],
            )
            .expect("plant uploads setting");

        let b117_qtap =
            std::fs::read_to_string(bug117_qtap_path()).expect("read committed bug117 .qtap");
        let b117_export = parse_export_file(&b117_qtap).expect("parse bug117 .qtap");
        // The byte-CHANGING codec: the not-configured one passes bytes through and
        // would make the boolean vacuously true whichever ORDER ran.
        let codec = quilltap_harness::PrefixingPixelCodec;
        let b117 = execute_import(
            main2.connection(),
            mount2.connection(),
            SINGLE_USER_ID,
            &b117_export,
            &ImportOptions::seed_defaults(),
            Some(&codec),
        )
        .expect("bug117 execute_import");

        let want117 = &oracle["bug117"];
        assert_eq!(
            Value::Bool(b117.success),
            want117["success"],
            "bug117 import success"
        );
        assert_eq!(
            serde_json::to_value(&b117.warnings).unwrap(),
            want117["warnings"],
            "bug117 warnings"
        );
        assert_eq!(
            serde_json::json!(b117.imported.files),
            want117["imported"],
            "bug117 imported file count"
        );

        let got117 = dump_sha_join(&main2, &mount2);
        assert_eq!(got117, want117["shaJoin"], "bug117 sha join");
        // The floor: the transcoded row must be present AND matching, or the arm
        // has stopped asking anything.
        let png = got117
            .as_array()
            .and_then(|rs| {
                rs.iter()
                    .find(|r| r["filename"].as_str() == Some("imported-shot.png"))
                    .cloned()
            })
            .expect("the PNG row");
        assert_eq!(png["mimeType"], serde_json::json!("image/webp"));
        assert_eq!(
            png["shaMatchesBlob"],
            serde_json::json!(true),
            "bug 117: an imported bitmap must record the STORED bytes' hash"
        );

        drop(main2);
        drop(mount2);
        let _ = std::fs::remove_file(&main2_path);
        let _ = std::fs::remove_file(&mount2_path);
    }

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    eprintln!(
        "OK: qtap-import tier-2 matched oracle (9 tables, 2 DBs) + skip branch + bug-117 sha join."
    );
}

/// The bug-117 comparand, in the oracle case's shape: every `files` row with a
/// `mount-blob:` key, joined to its blob through the parsed key.
fn dump_sha_join(main: &Writer, mount: &Writer) -> Value {
    let mut stmt = main
        .connection()
        .prepare(
            "SELECT originalFilename, mimeType, category, isPlainText, storageKey, sha256 \
               FROM files WHERE storageKey LIKE 'mount-blob:%' ORDER BY originalFilename",
        )
        .expect("prepare sha join");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .expect("sha join")
        .collect::<Result<Vec<_>, _>>()
        .expect("sha join rows");
    let mut out = Vec::new();
    for (filename, mime, category, is_plain_text, key, sha) in rows {
        let blob: Option<(String, String)> =
            quilltap_core::services::file_storage::parse_mount_blob_storage_key(&key).and_then(
                |(_, id)| {
                    mount
                        .connection()
                        .query_row(
                            "SELECT sha256, storedMimeType FROM doc_mount_blobs WHERE id = ?1",
                            rusqlite::params![id],
                            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                        )
                        .ok()
                },
            );
        out.push(serde_json::json!({
            "filename": filename,
            "mimeType": mime,
            "category": category,
            "isPlainText": is_plain_text,
            "blobFound": blob.is_some(),
            "storedMimeTypeInBlob": blob.as_ref().map(|(_, m)| m.clone()),
            "shaMatchesBlob": blob.map(|(bs, _)| bs == sha).unwrap_or(false),
        }));
    }
    Value::Array(out)
}
