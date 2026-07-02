//! Tier-2 differential test: the `projects` STORE-BACKED entity (document-store
//! overlay slice, build step 4).
//!
//! Mirrors `groups_tier2_equivalence` — same two-DB store-backed machine, same
//! shared cross-db id-map remap form, seven tables (the slim `projects` row +
//! `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders` +
//! `project_doc_mount_links`). What it adds: the larger **16-key `properties.json`
//! bag** (five Zod-default keys ALWAYS materialized in schema order, the rest
//! `skip_serializing_if`) and the **character-roster operations**
//! (`addToRoster` / `removeFromRoster` / `setAllowAnyCharacter`, each a properties
//! read-modify-write through `update`). v4's reindex `chunkCount` /
//! `doc_mount_chunks` artifact is pinned/excluded, exactly as for groups.
//!
//! The corpus banks: a rich create (roster + color + defaultImageProfileId +
//! backgroundDisplayMode, the optional keys interleaved with the materialized
//! defaults in schema order) and a minimal create (only the five defaults);
//! addToRoster + removeFromRoster (the `characterRoster` array RMW preserving the
//! other 15 keys); setAllowAnyCharacter (a bool RMW); and a DB-only `name` update.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-projects-tier2-fixture.ts
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/projects-tier2.ts > /tmp/oracle-projects.ndjson
//! Run:
//!   QT_ORACLE_PROJECTS=/tmp/oracle-projects.ndjson \
//!   QT_FIXTURE_PROJECTS_MAIN=/tmp/qt-projects-main.db \
//!   QT_FIXTURE_PROJECTS_MOUNT=/tmp/qt-projects-mount.db \
//!     cargo test -p quilltap-harness --test projects_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::projects::{ProjectCreateInput, ProjectCreateOptions, ProjectsRepository};
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
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        label: String,
        input: Map<String, Value>,
    },
    #[serde(rename = "update")]
    Update {
        label: String,
        patch: Map<String, Value>,
    },
    #[serde(rename = "addToRoster")]
    AddToRoster {
        label: String,
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "removeFromRoster")]
    RemoveFromRoster {
        label: String,
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "setAllowAnyCharacter")]
    SetAllowAnyCharacter { label: String, value: bool },
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
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "projects",
        oracle_key: "projects",
        order_by: "name",
        id_columns: &["id", "officialMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
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
        table: "project_doc_mount_links",
        oracle_key: "projectLinks",
        order_by: "createdAt",
        id_columns: &["id", "projectId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/projects-tier2.json")
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

/// Split a flat create input into (name, description, instructions, state, properties).
/// The property bag is everything except the four top-level store/slim fields —
/// exactly what v4's create payload routes to `properties.json`.
fn split_create_input(input: &Map<String, Value>) -> ProjectCreateInput {
    let mut properties = input.clone();
    properties.remove("name");
    properties.remove("description");
    properties.remove("instructions");
    properties.remove("state");
    ProjectCreateInput {
        name: input["name"].as_str().expect("name").to_string(),
        description: input
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        instructions: input
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: input
            .get("state")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        properties: Value::Object(properties),
    }
}

#[test]
fn projects_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROJECTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PROJECTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_PROJECTS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROJECTS_MAIN (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_PROJECTS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_PROJECTS_MOUNT (see header).");
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
    let main_work = std::env::temp_dir().join(format!("qt-projects-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-projects-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    {
        let repo = ProjectsRepository::new(main.connection(), mount.connection());
        let mut id_by_label: HashMap<String, String> = HashMap::new();
        let lookup = |map: &HashMap<String, String>, label: &str| -> String {
            map.get(label)
                .unwrap_or_else(|| panic!("op references unknown label {label}"))
                .clone()
        };
        for op in &spec.ops {
            match op {
                Op::Create { label, input } => {
                    let created = repo
                        .create(&split_create_input(input), &ProjectCreateOptions::default())
                        .unwrap_or_else(|e| panic!("create {label}: {e}"));
                    id_by_label.insert(label.clone(), created["id"].as_str().unwrap().to_string());
                }
                Op::Update { label, patch } => {
                    repo.update(&lookup(&id_by_label, label), patch)
                        .unwrap_or_else(|e| panic!("update {label}: {e}"));
                }
                Op::AddToRoster {
                    label,
                    character_id,
                } => {
                    repo.add_to_roster(&lookup(&id_by_label, label), character_id)
                        .unwrap_or_else(|e| panic!("addToRoster {label}: {e}"));
                }
                Op::RemoveFromRoster {
                    label,
                    character_id,
                } => {
                    repo.remove_from_roster(&lookup(&id_by_label, label), character_id)
                        .unwrap_or_else(|e| panic!("removeFromRoster {label}: {e}"));
                }
                Op::SetAllowAnyCharacter { label, value } => {
                    repo.set_allow_any_character(&lookup(&id_by_label, label), *value)
                        .unwrap_or_else(|e| panic!("setAllowAnyCharacter {label}: {e}"));
                }
            }
        }
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

    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(rows("projects").len(), 2, "2 project rows");
    assert_eq!(rows("points").len(), 2, "2 mount-point rows");
    assert_eq!(rows("projectLinks").len(), 2, "2 project→store links");

    // The minimal project's properties.json = the five materialized defaults,
    // in schema order, with backgroundDisplayMode 'theme' (Beta after the
    // allowAnyCharacter RMW → true).
    let docs = rows("documents");
    let beta_props =
        "{\n  \"allowAnyCharacter\": true,\n  \"characterRoster\": [],\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"backgroundDisplayMode\": \"theme\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(beta_props.into())),
        "Beta materialized-defaults properties.json not found; documents: {docs:?}"
    );
    // Alpha's final roster after add char-2 then remove char-1 = [char-2], with
    // the optional keys (color / defaultImageProfileId / answerConfirmationOverride /
    // backgroundDisplayMode) preserved through the RMWs and interleaved with the
    // defaults in schema order.
    let alpha_props =
        "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": [\n    \"aaaaaaaa-0000-4000-8000-000000000002\"\n  ],\n  \"color\": \"#778899\",\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"defaultImageProfileId\": \"11111111-1111-4111-8111-111111111111\",\n  \"answerConfirmationOverride\": \"ON\",\n  \"backgroundDisplayMode\": \"project\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(alpha_props.into())),
        "Alpha RMW-preserved properties.json not found; documents: {docs:?}"
    );

    eprintln!("OK: projects store-backed tier-2 matched oracle (7 tables, 2 DBs).");
}
