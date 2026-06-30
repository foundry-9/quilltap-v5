//! Tier-2 differential test: v4's `CharactersRepository` array / sub-array ops
//! (Phase-2, the store-backed capstone sub-unit 4b).
//!
//! Both sides start from the SAME baked fixture (a character + vault created by
//! v4's real `repos.characters.create`, with one baked systemPrompt, one scenario,
//! one partnerLink), apply the SAME op sequence, then SIX tables are
//! structural-diffed: the main slim `characters` row + the mount-index store tables
//! (`doc_mount_points` / `_folders` / `_files` / `_documents` / `_file_links`). The
//! Rust port drives [`vault_character_arrays`] over two writers; v4 drives the real
//! repository methods (see the oracle).
//!
//! The id-taking prompt/scenario ops carry a `targetName` / `targetTitle`; each
//! side resolves it to the current item's id via `find_by_id` (the read overlay)
//! right before the op — the id is path-derived, so both sides agree.
//!
//! Minted-values remap with ONE shared id-map across all six tables (FKs verify by
//! relationship); timestamps → `<ts>`; the link `chunkCount` → `<cc>` (reindex
//! artifact); `doc_mount_chunks` excluded.
//!
//! Banks: addSystemPrompt (default-demote + non-default), updateSystemPrompt
//! (rename → sweep + content), setDefaultSystemPrompt, deleteSystemPrompt (deletes
//! the default → survivor promotion), addScenario / updateScenario / removeScenario,
//! addPartnerLink / removePartnerLink (slim column), and the
//! setFavorite / setControlledBy / setCanBeCarina setters.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-arrays-fixture.ts
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-arrays.ts > /tmp/oracle-chararr.ndjson
//! Run:
//!   QT_ORACLE_CHARARR=/tmp/oracle-chararr.ndjson \
//!   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
//!   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
//!     cargo test -p quilltap-harness --test characters_arrays_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_character_arrays as arr;
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
#[serde(rename_all = "camelCase")]
struct Op {
    op: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    target_name: Option<String>,
    #[serde(default)]
    target_title: Option<String>,
    #[serde(default)]
    data: Option<Map<String, Value>>,
    #[serde(default)]
    partner_id: Option<String>,
    #[serde(default)]
    value: Option<Value>,
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
        .join("../../harness/oracle/fixtures/characters-arrays-tier2.json")
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

/// Resolve a `systemPrompts` / `scenarios` item id by its name/title via the read
/// overlay (mirrors the oracle's `findById`-based resolution).
fn resolve_item_id(
    main: &Writer,
    mount: &Writer,
    character_id: &str,
    array_key: &str,
    name_key: &str,
    name_value: &str,
) -> String {
    let character = arr::find_by_id(main.connection(), mount.connection(), character_id)
        .unwrap_or_else(|e| panic!("find_by_id during resolve: {e}"))
        .unwrap_or_else(|| panic!("character {character_id} vanished during resolve"));
    character
        .get(array_key)
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|i| i.get(name_key).and_then(Value::as_str) == Some(name_value))
        })
        .and_then(|i| i.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("{array_key} item not found for {name_key}={name_value}"))
}

fn run_op(main: &Writer, mount: &Writer, character_id: &str, op: &Op) {
    let cid = character_id;
    let m = main.connection();
    let mo = mount.connection();
    match op.op.as_str() {
        "addSystemPrompt" => {
            arr::add_system_prompt(
                m,
                mo,
                cid,
                op.name.as_deref().expect("name"),
                op.content.as_deref().expect("content"),
                op.is_default.expect("isDefault"),
            )
            .expect("add_system_prompt");
        }
        "updateSystemPrompt" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "systemPrompts",
                "name",
                op.target_name.as_deref().expect("targetName"),
            );
            arr::update_system_prompt(m, mo, cid, &id, op.data.as_ref().expect("data"))
                .expect("update_system_prompt");
        }
        "setDefaultSystemPrompt" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "systemPrompts",
                "name",
                op.target_name.as_deref().expect("targetName"),
            );
            arr::set_default_system_prompt(m, mo, cid, &id).expect("set_default_system_prompt");
        }
        "deleteSystemPrompt" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "systemPrompts",
                "name",
                op.target_name.as_deref().expect("targetName"),
            );
            arr::delete_system_prompt(m, mo, cid, &id).expect("delete_system_prompt");
        }
        "addScenario" => {
            arr::add_scenario(
                m,
                mo,
                cid,
                op.title.as_deref().expect("title"),
                op.content.as_deref().expect("content"),
            )
            .expect("add_scenario");
        }
        "updateScenario" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "scenarios",
                "title",
                op.target_title.as_deref().expect("targetTitle"),
            );
            arr::update_scenario(m, mo, cid, &id, op.data.as_ref().expect("data"))
                .expect("update_scenario");
        }
        "removeScenario" => {
            let id = resolve_item_id(
                main,
                mount,
                cid,
                "scenarios",
                "title",
                op.target_title.as_deref().expect("targetTitle"),
            );
            arr::remove_scenario(m, mo, cid, &id).expect("remove_scenario");
        }
        "addPartnerLink" => {
            arr::add_partner_link(
                m,
                mo,
                cid,
                op.partner_id.as_deref().expect("partnerId"),
                op.is_default.expect("isDefault"),
            )
            .expect("add_partner_link");
        }
        "removePartnerLink" => {
            arr::remove_partner_link(m, mo, cid, op.partner_id.as_deref().expect("partnerId"))
                .expect("remove_partner_link");
        }
        "setFavorite" => {
            arr::set_favorite(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_bool).expect("value"),
            )
            .expect("set_favorite");
        }
        "setControlledBy" => {
            arr::set_controlled_by(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_str).expect("value"),
            )
            .expect("set_controlled_by");
        }
        "setCanBeCarina" => {
            arr::set_can_be_carina(
                m,
                mo,
                cid,
                op.value.as_ref().and_then(Value::as_bool).expect("value"),
            )
            .expect("set_can_be_carina");
        }
        other => panic!("unknown op: {other}"),
    }
}

#[test]
fn characters_arrays_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARARR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARARR to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARARR_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARARR_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARARR_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARARR_MOUNT to the mount fixture .db (header).");
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
    let main_work = std::env::temp_dir().join(format!("qt-chararr-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-chararr-mount-rust-{pid}.db"));
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

    for op in &spec.ops {
        run_op(&main, &mount, &character_id, op);
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

    eprintln!("OK: characters arrays tier-2 matched oracle (6 tables, 2 DBs, 13 ops).");
}
