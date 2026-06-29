//! Tier-2 differential test: the `groups` STORE-BACKED pilot (document-store
//! overlay slice, build steps 2-3).
//!
//! Both sides run the SAME create/update sequence (from the committed spec)
//! against the SAME pair of fixtures (a MAIN db with the slim `groups` table + a
//! MOUNT-INDEX db with the store tables), then SEVEN tables are structural-diffed:
//! the main slim `groups` row plus `doc_mount_points` / `doc_mount_files` /
//! `doc_mount_documents` / `doc_mount_file_links` / `doc_mount_folders` /
//! `group_doc_mount_links`. The Rust port drives [`GroupsRepository`] over two
//! writers (one per DB); v4 drives the real `repos.groups` (see the oracle).
//!
//! This is the minted-values remap form, extended across two databases with ONE
//! shared id-map: a single first-seen-token map is built by walking all tables in
//! a fixed order (points → groups → files → documents → links → folders →
//! groupLinks, rows in natural-key order). So every cross-DB / cross-table FK —
//! `groups.officialMountPointId` → `doc_mount_points.id`, `link.fileId` →
//! `file.id`, `link.mountPointId` → the store, `groupLink.groupId` → the group —
//! verifies by RELATIONSHIP without pinning a literal id. Timestamps →
//! `<ts>`; the link `chunkCount` → `<cc>` (a v4-only `reindexSingleFile`
//! artifact the Rust storage primitive does not rebuild — see the oracle header);
//! `doc_mount_chunks` is excluded entirely.
//!
//! The corpus banks: the 5-step create (slim row + provision + four files +
//! overlay re-read), `properties.json` byte-exact (both keys + the empty bag),
//! a store-only update (`description`/`color` rewritten, the slim row's
//! `updatedAt` NOT bumped) with a properties read-modify-write that PRESERVES the
//! untouched `icon`, a DB-only update (`name` → slim row bumped, store untouched),
//! dedup-by-sha (`"{}"` shared by three links across two stores; `""` shared by
//! two), and orphan-on-rewrite (the pre-update content rows persist).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-groups-tier2-fixture.ts
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/groups-tier2.ts > /tmp/oracle-groups.ndjson
//! Run:
//!   QT_ORACLE_GROUPS=/tmp/oracle-groups.ndjson \
//!   QT_FIXTURE_GROUPS_MAIN=/tmp/qt-groups-main.db \
//!   QT_FIXTURE_GROUPS_MOUNT=/tmp/qt-groups-mount.db \
//!     cargo test -p quilltap-harness --test groups_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::groups::{GroupCreateInput, GroupCreateOptions, GroupsRepository};
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
    Create { label: String, input: CreateInput },
    #[serde(rename = "update")]
    Update {
        label: String,
        patch: Map<String, Value>,
    },
}

#[derive(Deserialize)]
struct CreateInput {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    state: Option<Value>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

/// Per-table normalization spec. `from_mount` = read from the mount-index writer
/// (else the main writer); `oracle_key` = the JSON key the oracle emits it under.
/// The slice order here is the canonical walk order for the shared id-remap.
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
        table: "groups",
        oracle_key: "groups",
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
        table: "group_doc_mount_links",
        oracle_key: "groupLinks",
        order_by: "createdAt",
        id_columns: &["id", "groupId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/groups-tier2.json")
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
fn groups_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_GROUPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_GROUPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_GROUPS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_GROUPS_MAIN to the main fixture .db (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_GROUPS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_GROUPS_MOUNT to the mount-index fixture .db (see header)."
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

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-groups-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-groups-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    {
        let repo = GroupsRepository::new(main.connection(), mount.connection());
        let mut id_by_label: HashMap<String, String> = HashMap::new();
        for op in &spec.ops {
            match op {
                Op::Create { label, input } => {
                    let created = repo
                        .create(
                            &GroupCreateInput {
                                name: input.name.clone(),
                                description: input.description.clone(),
                                instructions: input.instructions.clone(),
                                state: input
                                    .state
                                    .clone()
                                    .unwrap_or_else(|| Value::Object(Map::new())),
                                color: input.color.clone(),
                                icon: input.icon.clone(),
                            },
                            &GroupCreateOptions::default(),
                        )
                        .unwrap_or_else(|e| panic!("create {label}: {e}"));
                    let id = created["id"].as_str().expect("created id").to_string();
                    id_by_label.insert(label.clone(), id);
                }
                Op::Update { label, patch } => {
                    let id = id_by_label
                        .get(label)
                        .unwrap_or_else(|| panic!("update references unknown label {label}"));
                    repo.update(id, patch)
                        .unwrap_or_else(|e| panic!("update {label}: {e}"));
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

    // Sanity: the corpus produced the expected shape.
    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(rows("groups").len(), 2, "2 group rows");
    assert_eq!(rows("points").len(), 2, "2 mount-point rows");
    assert_eq!(rows("files").len(), 7, "7 deduped file rows");
    assert_eq!(rows("documents").len(), 7, "7 document rows");
    assert_eq!(rows("links").len(), 8, "8 link rows (2 stores × 4 files)");
    assert_eq!(rows("folders").len(), 0, "0 folders (all files top-level)");
    assert_eq!(rows("groupLinks").len(), 2, "2 group→store links");

    // The properties read-modify-write PRESERVED the untouched `icon` while
    // changing `color` (Alpha's final properties.json).
    let docs = rows("documents");
    let final_props = "{\n  \"color\": \"#445566\",\n  \"icon\": \"star\"\n}";
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String(final_props.into())),
        "RMW-preserved properties.json not found; documents: {docs:?}"
    );
    // The empty-bag store (Beta) wrote `{}` and the empty description.md (`""`).
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String("{}".into())),
        "empty properties.json `{{}}` not found"
    );
    assert!(
        docs.iter()
            .any(|d| d["content"] == Value::String("".into())),
        "empty markdown file `\"\"` not found"
    );

    eprintln!("OK: groups store-backed tier-2 matched oracle (7 tables, 2 DBs).");
}

/// The keystone asymmetry (v4 `applyOverlayOne` THROWS, `applyOverlay` DROPS): a
/// group with a null `officialMountPointId` (or a store missing `properties.json`)
/// is unavailable. `find_by_id` must error; `find_all` must drop it. This is the
/// engine's documented failure mode mirrored from v4; it is a Rust-side
/// behavioral bank (the write-path differential above is the oracle-verified
/// part) and needs only the fixtures — a storeless row, against the empty store.
#[test]
fn groups_keystone_throw_vs_drop() {
    let (Ok(main_fixture), Ok(mount_fixture), Ok(pepper)) = (
        std::env::var("QT_FIXTURE_GROUPS_MAIN"),
        std::env::var("QT_FIXTURE_GROUPS_MOUNT"),
        // The fixtures are keyed by the committed test pepper.
        Ok::<_, std::env::VarError>("ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=".to_string()),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_GROUPS_MAIN/MOUNT (see header).");
        return;
    };

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-groups-keystone-main-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-groups-keystone-mount-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap();
    std::fs::copy(&mount_fixture, &mount_work).unwrap();

    let main = Writer::open_writable(&main_work, &pepper).unwrap();
    let mount = Writer::open_writable(&mount_work, &pepper).unwrap();

    // A storeless group: null officialMountPointId (the keystone-broken state).
    main.connection()
        .execute(
            "INSERT INTO groups (id, name, officialMountPointId, createdAt, updatedAt) \
             VALUES ('ghost-id', 'Ghost', NULL, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();

    let repo = GroupsRepository::new(main.connection(), mount.connection());

    // find_by_id THROWS (Unavailable).
    let one = repo.find_by_id("ghost-id");
    assert!(
        one.is_err(),
        "find_by_id on a storeless group should error, got {one:?}"
    );

    // find_all DROPS it (no error, row absent).
    let all = repo.find_all().expect("find_all should not error");
    assert!(
        !all.iter()
            .any(|g| g["id"] == Value::String("ghost-id".into())),
        "find_all should drop the storeless group"
    );

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    eprintln!("OK: groups keystone throw-vs-drop asymmetry holds.");
}
