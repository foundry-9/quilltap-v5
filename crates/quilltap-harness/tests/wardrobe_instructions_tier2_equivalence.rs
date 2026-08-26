//! P4.D119 tier-2 differential — the dressing-instructions cascade and file
//! helpers (v4 `b86bb1a5`, `lib/wardrobe/wardrobe-instructions.ts` →
//! `quilltap_core::wardrobe_instructions`).
//!
//! Both sides run the SAME two phases over a fresh copy of the committed
//! `wardrobe-instructions-{main,mount}.db` pair, reading the SAME case lists out
//! of `harness/oracle/fixtures/wardrobe-instructions.json`, so neither side can
//! silently partial-pass (the shared-corpus rule): the Rust side asserts its row
//! count against the oracle NDJSON's.
//!
//! Phase 1 (`cascadeCases`) compares the resolve/read RESULT. v4's own unit
//! suite pins the probe ORDER through a mocked `readVaultTextFile`; a real-DB
//! oracle cannot see that, so the dedupe-then-code-unit-sort is made observable
//! instead — two mounts in the same tier both carry a file and the SORT decides
//! which content comes back.
//!
//! Phase 2 (`writeOps`) compares the five mount-index tables after each write,
//! so the bytes on disk (trimmed, no trailing newline, no frontmatter), the
//! folder ensure on the write path only, and the file's ABSENCE after a clear
//! are all comparands rather than a return value.
//!
//! ⚠ The `wardrobe-instructions-{main,mount}.db` pair is COMMITTED. Rebuilding
//! it mints fresh mount/file ids, so it is rebuilt only deliberately — and this
//! family's oracle is then regenerated with it. The builder (kept OUT of the
//! runnable recipe below, which must never write inside the repository) is:
//! `harness/oracle/fixtures/build-wardrobe-instructions-fixture.ts`, run from the
//! v4 checkout with `QT_FIXTURE_WI_MAIN` / `QT_FIXTURE_WI_MOUNT` pointed at the
//! two committed files (its own header carries the invocation).
//!
//! Generate the oracle (Node 24, from the v4 checkout or a pinned worktree —
//! this lane pins at `d25dacc1`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WI_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-main.db \
//!   QT_FIXTURE_WI_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-instructions-cascade.ts \
//!     > /tmp/oracle-wardrobe-instructions-cascade.ndjson
//!
//! Run:
//!   QT_ORACLE_WARDROBE_INSTRUCTIONS_CASCADE=/tmp/oracle-wardrobe-instructions-cascade.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_instructions_tier2_equivalence
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::wardrobe_instructions::{
    read_wardrobe_instructions_file, resolve_wardrobe_instructions,
    write_wardrobe_instructions_file, WARDROBE_INSTRUCTIONS_PATH,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "generalMountPointId")]
    general_mount_point_id: String,
    #[serde(rename = "extraStores")]
    extra_stores: Vec<ExtraStore>,
    #[serde(rename = "cascadeCases")]
    cascade_cases: Vec<Value>,
    #[serde(rename = "writeOps")]
    write_ops: Vec<Value>,
}

#[derive(Deserialize)]
struct ExtraStore {
    label: String,
    id: String,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
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
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-instructions.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
fn labels_of(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn wardrobe_instructions_cascade_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WARDROBE_INSTRUCTIONS_CASCADE") else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_INSTRUCTIONS_CASCADE to the oracle NDJSON (see header)."
        );
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert_eq!(
        oracle_rows.len(),
        spec.cascade_cases.len() + spec.write_ops.len(),
        "oracle row count must equal the shared corpus's case count"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("wardrobe-instructions-main.db"), &main).expect("copy main");
    std::fs::copy(
        fixtures_dir().join("wardrobe-instructions-mount.db"),
        &mount,
    )
    .expect("copy mount");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    // --- label → minted mount id (the oracle's own resolution, mirrored) ----
    let mut labels: HashMap<String, String> = HashMap::new();
    db.read_main(|conn| {
        let one = |sql: &str, id: &str| -> Option<String> {
            conn.query_row(sql, [id], |r| r.get::<_, Option<String>>(0))
                .ok()
                .flatten()
        };
        labels.insert(
            "charA".into(),
            one(
                "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                &spec.character_id,
            )
            .expect("character vault mount"),
        );
        labels.insert(
            "project".into(),
            one(
                "SELECT officialMountPointId FROM projects WHERE id = ?1",
                &spec.project_id,
            )
            .expect("project official store"),
        );
        labels.insert(
            "group".into(),
            one(
                "SELECT officialMountPointId FROM groups WHERE id = ?1",
                &spec.group_id,
            )
            .expect("group official store"),
        );
        Ok(())
    })
    .expect("resolve labels");
    labels.insert("general".into(), spec.general_mount_point_id.clone());
    for st in &spec.extra_stores {
        labels.insert(st.label.clone(), st.id.clone());
    }
    let by_id: HashMap<String, String> =
        labels.iter().map(|(k, v)| (v.clone(), k.clone())).collect();
    let all_mounts: Vec<String> = labels.values().cloned().collect();

    let clear_all = |mounts: Vec<String>| {
        db.write_blocking(move |w| {
            let mi = w.mount_index().expect("mount writer");
            let links = mi.doc_mount_file_links();
            for mp in &mounts {
                links.delete_database_document(mp, WARDROBE_INSTRUCTIONS_PATH)?;
            }
            Ok(())
        })
        .expect("clear");
    };
    let seed_file = |mp: String, content: String| {
        db.write_blocking(move |w| {
            let mi = w.mount_index().expect("mount writer");
            let links = mi.doc_mount_file_links();
            links.ensure_folder_path(&mp, "Wardrobe")?;
            links.write_database_document(&mp, WARDROBE_INSTRUCTIONS_PATH, &content)?;
            Ok(())
        })
        .expect("seed");
    };
    let set_general = |value: Option<String>| {
        db.write_blocking(move |w| {
            let conn = w.main().connection();
            match value {
                Some(v) => conn.execute(
                    "INSERT OR REPLACE INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2)",
                    rusqlite::params!["generalMountPointId", v],
                )?,
                None => conn.execute(
                    "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
                    rusqlite::params!["generalMountPointId"],
                )?,
            };
            Ok(())
        })
        .expect("general setting");
    };

    // --- phase 1 ------------------------------------------------------------
    for (i, c) in spec.cascade_cases.iter().enumerate() {
        let name = s(c, "name").expect("case name");
        clear_all(all_mounts.clone());
        if let Some(seed) = c.get("seed").and_then(Value::as_object) {
            for (label, content) in seed {
                seed_file(
                    labels[label].clone(),
                    content.as_str().unwrap_or_default().to_string(),
                );
            }
        }
        let unprovision = c
            .get("unprovisionGeneral")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if unprovision {
            set_general(None);
        }

        let got: Value = db
            .read_main(|main| {
                db.read_mount_index(|mnt| {
                    Ok(match s(c, "kind").as_deref() {
                        Some("resolve") => {
                            // The three tri-state character spellings the corpus
                            // carries: absent/null → None, `""` → Some("") (v4's
                            // JS-truthiness skip), a label → its mount id.
                            let character: Option<String> = match c.get("character") {
                                None | Some(Value::Null) => None,
                                Some(Value::String(l)) if l.is_empty() => Some(String::new()),
                                Some(Value::String(l)) => Some(labels[l].clone()),
                                other => panic!("bad character spelling: {other:?}"),
                            };
                            let groups: Vec<String> = labels_of(c, "groups")
                                .iter()
                                .map(|l| labels[l].clone())
                                .collect();
                            let projects: Vec<String> = labels_of(c, "projects")
                                .iter()
                                .map(|l| labels[l].clone())
                                .collect();
                            match resolve_wardrobe_instructions(
                                main,
                                mnt,
                                character.as_deref(),
                                &groups,
                                &projects,
                            ) {
                                Some(hit) => json!({
                                    "content": hit.content,
                                    "tier": hit.tier.as_str(),
                                    "mount": by_id.get(&hit.mount_point_id)
                                        .cloned()
                                        .unwrap_or(hit.mount_point_id),
                                }),
                                None => Value::Null,
                            }
                        }
                        Some("read") => {
                            let mp = &labels[&s(c, "mount").expect("mount label")];
                            match read_wardrobe_instructions_file(mnt, mp) {
                                Some(v) => Value::String(v),
                                None => Value::Null,
                            }
                        }
                        other => panic!("unknown case kind {other:?}"),
                    })
                })
            })
            .expect("run case");

        if unprovision {
            set_general(Some(spec.general_mount_point_id.clone()));
        }

        let want = &oracle_rows[i];
        assert_eq!(
            s(want, "name").as_deref(),
            Some(name.as_str()),
            "case {i}: corpus order diverged from the oracle"
        );
        assert_eq!(
            &got,
            want.get("result").unwrap_or(&Value::Null),
            "{name}: cascade result diverged"
        );
    }

    // --- phase 2 ------------------------------------------------------------
    clear_all(all_mounts.clone());
    for (j, op) in spec.write_ops.iter().enumerate() {
        let name = s(op, "name").expect("op name");
        let mp = labels[&s(op, "mount").expect("mount label")].clone();
        if let Some(pre) = s(op, "preSeed") {
            seed_file(mp.clone(), pre);
        }
        match s(op, "kind").as_deref() {
            Some("write") => {
                let instructions: Option<String> = match op.get("instructions") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(v)) => Some(v.clone()),
                    other => panic!("bad instructions spelling: {other:?}"),
                };
                let mp2 = mp.clone();
                db.write_blocking(move |w| {
                    let mi = w.mount_index().expect("mount writer");
                    let links: DocMountFileLinksRepository = mi.doc_mount_file_links();
                    write_wardrobe_instructions_file(&links, &mp2, instructions.as_deref())
                })
                .unwrap_or_else(|e| panic!("{name}: write failed: {e}"));
            }
            Some("read") => {}
            other => panic!("unknown write-op kind {other:?}"),
        }

        let want = &oracle_rows[spec.cascade_cases.len() + j];
        assert_eq!(
            s(want, "name").as_deref(),
            Some(name.as_str()),
            "write op {j}: corpus order diverged from the oracle"
        );

        let got_result: Value = match s(op, "kind").as_deref() {
            Some("write") => json!({ "wrote": true }),
            _ => db
                .read_mount_index(|mnt| {
                    Ok(match read_wardrobe_instructions_file(mnt, &mp) {
                        Some(v) => Value::String(v),
                        None => Value::Null,
                    })
                })
                .expect("read back"),
        };
        assert_eq!(
            &got_result,
            want.get("result").unwrap_or(&Value::Null),
            "{name}: write-phase result diverged"
        );

        let mut got: Vec<Value> = TABLES
            .iter()
            .map(|t| {
                db.read_mount_index(|mnt| {
                    quilltap_core::db::dump_table_json_conn(mnt, t.table, t.order_by)
                })
                .unwrap_or_else(|e| panic!("dump {}: {e}", t.table))
            })
            .collect();
        let mut expected: Vec<Value> = TABLES
            .iter()
            .map(|t| {
                want.get("tables")
                    .and_then(|v| v.get(t.oracle_key))
                    .cloned()
                    .unwrap_or_else(|| panic!("{name}: oracle missing dump for {}", t.oracle_key))
            })
            .collect();
        normalize_all(&mut got);
        normalize_all(&mut expected);
        for (i, t) in TABLES.iter().enumerate() {
            assert_eq!(
                got[i]["columns"], expected[i]["columns"],
                "{name}/{}: column set / order",
                t.table
            );
            assert_eq!(
                got[i]["rows"], expected[i]["rows"],
                "{name}/{}: row state diverged\n  rust:   {}\n  oracle: {}",
                t.table, got[i]["rows"], expected[i]["rows"]
            );
        }
    }

    eprintln!(
        "OK: wardrobe-instructions cascade + helpers matched oracle ({} cases, {} write ops).",
        spec.cascade_cases.len(),
        spec.write_ops.len()
    );
}

/// v4's third write-helper unit arm: **only** a `NOT_FOUND` delete failure is
/// swallowed — anything else rethrows. v4 reaches it by mocking
/// `deleteDatabaseDocument` to reject with an `IO_ERROR`; over a real database
/// no INPUT can produce that, so the fault is injected the way this repo already
/// injects unreachable catch arms (`force-a-swallowed-catch-by-breaking-the-table`):
/// drop the links table on a scratch copy and clear.
///
/// The positive half (a clear against a mount with no such file succeeds) is the
/// corpus's `clear_when_already_absent_is_a_noop` op, so this test only has to
/// pin that the swallow is NARROW.
#[test]
fn a_non_not_found_delete_failure_is_not_swallowed() {
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("wardrobe-instructions-main.db"), &main).expect("copy main");
    std::fs::copy(
        fixtures_dir().join("wardrobe-instructions-mount.db"),
        &mount,
    )
    .expect("copy mount");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    let general = spec.general_mount_point_id.clone();
    // Baseline: the clear succeeds while the table is intact.
    let g = general.clone();
    db.write_blocking(move |w| {
        let links = w
            .mount_index()
            .expect("mount writer")
            .doc_mount_file_links();
        write_wardrobe_instructions_file(&links, &g, None)
    })
    .expect("clearing an absent file is a no-op");

    let g = general.clone();
    let err = db
        .write_blocking(move |w| {
            let mi = w.mount_index().expect("mount writer");
            mi.connection()
                .execute("DROP TABLE doc_mount_file_links", [])?;
            let links = mi.doc_mount_file_links();
            write_wardrobe_instructions_file(&links, &g, None)
        })
        .expect_err("a broken links table must NOT be swallowed as NOT_FOUND");
    let msg = err.to_string();
    assert!(
        msg.contains("doc_mount_file_links"),
        "the underlying failure must reach the caller, got: {msg}"
    );
}
