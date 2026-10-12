//! Tier-2 structural differential — the mount-index case-collision repair
//! (v4 `0a0419f5`, ported in `quilltap_core::db::mount_index_case_repair`).
//!
//! Reads the SAME planted-collision spec the v4 oracle drives
//! (`harness/oracle/fixtures/mount-case-repair-spec.json`), builds an identical
//! in-memory DB per case (the exact tables + legacy case-sensitive indexes v4's
//! `mount-index-case-repair.test.ts` `freshDb()` creates), plants the rows, runs
//! the ported repair, dumps post-repair rows + the two tables' `sqlite_master`
//! index rows, and field-diffs against the oracle NDJSON.
//!
//! Covers the whole corpus: folder subtree + link repair, scoped uniqueness (via
//! the recreated index sql), idempotence, non-ASCII `Ärger`/`ärger`, the
//! tampered-index replacement, link suffix-before-extension, and vault-name
//! trim + keep-oldest.
//!
//! Generate the oracle (Node 24, from the v4 checkout or the round's pin; the
//! `mountpoints-index` rows need v4 `ed8b15b50`+):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/mount-case-repair.ts \
//!     > /tmp/oracle-mount-case-repair.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_CASE_REPAIR=/tmp/oracle-mount-case-repair.ndjson \
//!     cargo test -p quilltap-harness --test mount_case_repair_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated
//! discipline.
//!
//! P4.D274 (v4 `ed8b15b50`, bug 186) extends it with the `mountpoints-index`
//! run — the ported `ensure_mount_point_name_unique_index` (repair → validity →
//! DROP + CREATE the UNIQUE NOCASE name index → INFO) over a preset-valid, a
//! tampered, a colliding, an idempotent and a JS-trim-keyed plant, plus a
//! probe INSERT after the run whose error text must match v4's — and with the
//! log lines of EVERY case: v4's captured `{level, message, fields}` rendered
//! as `LEVEL message k=v …` in v4's key order, against v5's captured lines with
//! the target dropped. The case-repair lines' WARN + camelCase move (R-G) is
//! pinned here.

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::mount_index_case_repair::{
    ensure_folder_nocase_unique_index, ensure_link_nocase_unique_index,
    ensure_mount_point_name_unique_index, repair_mount_point_name_collisions,
    MOUNT_POINT_NAME_NOCASE_INDEX,
};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    run: String,
    #[serde(default)]
    #[serde(rename = "runTwice")]
    run_twice: bool,
    #[serde(default)]
    #[serde(rename = "tamperFolderIndex")]
    tamper_folder_index: bool,
    #[serde(default)]
    folders: Vec<FolderSpec>,
    #[serde(default)]
    links: Vec<LinkSpec>,
    #[serde(default)]
    #[serde(rename = "mountPoints")]
    mount_points: Vec<MountPointSpec>,
    #[serde(default)]
    #[serde(rename = "presetNameIndex")]
    preset_name_index: bool,
    #[serde(default)]
    #[serde(rename = "tamperNameIndex")]
    tamper_name_index: bool,
    #[serde(default)]
    #[serde(rename = "probeInsert")]
    probe_insert: Option<MountPointSpec>,
}

#[derive(Deserialize)]
struct FolderSpec {
    id: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    name: String,
    path: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Deserialize)]
struct LinkSpec {
    id: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "folderId")]
    folder_id: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Deserialize)]
struct MountPointSpec {
    id: String,
    name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Node `path.posix.basename` for a forward-slash path.
fn posix_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn fresh_db() -> Connection {
    let db = Connection::open_in_memory().expect("open in-memory db");
    db.execute_batch(
        "CREATE TABLE doc_mount_points (\
           id TEXT PRIMARY KEY, name TEXT NOT NULL, createdAt TEXT NOT NULL);\
         CREATE TABLE doc_mount_folders (\
           id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL, parentId TEXT, name TEXT NOT NULL, \
           path TEXT NOT NULL, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
         CREATE TABLE doc_mount_file_links (\
           id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL, \
           fileName TEXT NOT NULL, folderId TEXT, createdAt TEXT NOT NULL);\
         CREATE UNIQUE INDEX \"idx_doc_mount_folders_mp_parent_name\" \
           ON \"doc_mount_folders\" (\"mountPointId\", COALESCE(\"parentId\", ''), \"name\");\
         CREATE UNIQUE INDEX \"idx_doc_mount_file_links_mp_path\" \
           ON \"doc_mount_file_links\" (\"mountPointId\", \"relativePath\");",
    )
    .expect("fresh_db DDL");
    db
}

fn dump_rows(db: &Connection, sql: &str, cols: &[&str]) -> Vec<Value> {
    let mut stmt = db.prepare(sql).unwrap();
    let rows = stmt
        .query_map([], |row| {
            let mut obj = Map::new();
            for (i, col) in cols.iter().enumerate() {
                let v: Option<String> = row.get(i)?;
                obj.insert(
                    (*col).to_string(),
                    v.map(Value::String).unwrap_or(Value::Null),
                );
            }
            Ok(Value::Object(obj))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// v4's captured line → `LEVEL message k=v …` (v4's key order; a string
/// value verbatim, anything else as JSON).
fn render_v4_line(v: &Value) -> String {
    let mut out = format!(
        "{} {}",
        v["level"].as_str().unwrap().to_uppercase(),
        v["message"].as_str().unwrap()
    );
    for (k, val) in v["fields"].as_object().unwrap() {
        let rendered = match val {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={rendered}"));
    }
    out
}

/// v5's captured line (`LEVEL target message k=v …`) with the target dropped.
fn drop_target(line: &str) -> String {
    let (level, rest) = line.split_once(' ').unwrap();
    let (_target, rest) = rest.split_once(' ').unwrap();
    format!("{level} {rest}")
}

fn run_case(spec: &Spec, c: &Case) -> (Value, Vec<String>) {
    let db = fresh_db();
    let mp = &spec.mount_point_id;

    if c.preset_name_index {
        db.execute_batch(&format!(
            "CREATE UNIQUE INDEX \"{MOUNT_POINT_NAME_NOCASE_INDEX}\" \
               ON \"doc_mount_points\" (\"name\" COLLATE NOCASE)"
        ))
        .unwrap();
    }
    if c.tamper_name_index {
        db.execute_batch(&format!(
            "CREATE INDEX \"{MOUNT_POINT_NAME_NOCASE_INDEX}\" ON \"doc_mount_points\" (\"name\")"
        ))
        .unwrap();
    }

    if c.tamper_folder_index {
        db.execute_batch(
            "DROP INDEX IF EXISTS \"idx_doc_mount_folders_mp_parent_name\";\
             CREATE INDEX \"idx_doc_mount_folders_mp_parent_name_nocase\" \
               ON \"doc_mount_folders\" (\"mountPointId\");",
        )
        .unwrap();
    }

    for f in &c.folders {
        db.execute(
            "INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![f.id, mp, f.parent_id, f.name, f.path, f.created_at],
        )
        .unwrap();
    }
    for l in &c.links {
        db.execute(
            "INSERT INTO doc_mount_file_links (id, mountPointId, relativePath, fileName, folderId, createdAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![l.id, mp, l.relative_path, posix_basename(&l.relative_path), l.folder_id, l.created_at],
        )
        .unwrap();
    }
    for m in &c.mount_points {
        db.execute(
            "INSERT INTO doc_mount_points (id, name, createdAt) VALUES (?1, ?2, ?3)",
            params![m.id, m.name, m.created_at],
        )
        .unwrap();
    }

    let run_once = || match c.run.as_str() {
        "folder" => ensure_folder_nocase_unique_index(&db).unwrap(),
        "link" => ensure_link_nocase_unique_index(&db).unwrap(),
        "mountpoints" => {
            repair_mount_point_name_collisions(&db).unwrap();
        }
        "mountpoints-index" => ensure_mount_point_name_unique_index(&db).unwrap(),
        other => panic!("unknown run kind: {other}"),
    };
    let ((), lines) = quilltap_core::test_support::captured_with(|| {
        run_once();
        if c.run_twice {
            run_once();
        }
    });

    let probe_error = c.probe_insert.as_ref().map(|m| {
        db.execute(
            "INSERT INTO doc_mount_points (id, name, createdAt) VALUES (?1, ?2, ?3)",
            params![m.id, m.name, m.created_at],
        )
        .err()
        .map(|e| e.to_string())
    });

    let folders = dump_rows(
        &db,
        "SELECT id, parentId, name, path FROM doc_mount_folders ORDER BY id",
        &["id", "parentId", "name", "path"],
    );
    let links = dump_rows(
        &db,
        "SELECT id, relativePath, fileName FROM doc_mount_file_links ORDER BY id",
        &["id", "relativePath", "fileName"],
    );
    let mount_points = dump_rows(
        &db,
        "SELECT id, name FROM doc_mount_points ORDER BY id",
        &["id", "name"],
    );
    let indexes = dump_rows(
        &db,
        "SELECT name, sql FROM sqlite_master WHERE type = 'index' \
           AND tbl_name IN ('doc_mount_folders', 'doc_mount_file_links', 'doc_mount_points') \
           AND sql IS NOT NULL ORDER BY name",
        &["name", "sql"],
    );

    let mut out = json!({
        "case": c.name,
        "folders": folders,
        "links": links,
        "mountPoints": mount_points,
        "indexes": indexes,
    });
    if let Some(probe) = probe_error {
        out["probeError"] = json!(probe);
    }
    (out, lines.iter().map(|l| drop_target(l)).collect())
}

#[test]
fn mount_case_repair_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MOUNT_CASE_REPAIR") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_CASE_REPAIR to the oracle NDJSON (see header).");
        return;
    };

    let spec_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/mount-case-repair-spec.json");
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("read spec")).unwrap();

    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = v["case"].as_str().expect("oracle case name").to_string();
        oracle.insert(name, v);
    }

    let mut checked = 0;
    for c in &spec.cases {
        let (got, got_lines) = run_case(&spec, c);
        let mut want = oracle
            .get(&c.name)
            .unwrap_or_else(|| panic!("no oracle line for case {}", c.name))
            .clone();
        let want_lines: Vec<String> = want
            .as_object_mut()
            .unwrap()
            .shift_remove("lines")
            .expect("oracle lines (regenerate at the P4.D274 pin)")
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_line)
            .collect();
        assert_eq!(got_lines, want_lines, "case {} log lines diverged", c.name);
        let want = &want;
        assert_eq!(
            &got, want,
            "case {} diverged\n  got:  {}\n  want: {}",
            c.name, got, want
        );
        checked += 1;
    }
    assert_eq!(checked, spec.cases.len(), "not every case ran");
    assert!(checked >= 12, "expected >= 12 cases, ran {checked}");

    // The naming leaf — next_unique_mount_point_name (case-insensitive + trimmed).
    // Same NAMING_CASES (inputs + order) as the oracle.
    let naming_cases: &[(&[&str], &str)] = &[
        (&["Lore"], "lore"),
        (&["Lore", "LORE (2)"], "lore"),
        (&["Other"], "Lore"),
        (&["  My Vault  "], "my vault"),
        (&[], "Fresh"),
    ];
    let got_names: Vec<Value> = naming_cases
        .iter()
        .map(|(taken, desired)| {
            let set: std::collections::HashSet<String> =
                taken.iter().map(|s| s.to_string()).collect();
            Value::String(
                quilltap_core::db::ensure_official_store::next_unique_mount_point_name(
                    &set, desired,
                ),
            )
        })
        .collect();
    let want_names = oracle
        .get("naming")
        .and_then(|v| v.get("results"))
        .and_then(Value::as_array)
        .expect("oracle naming results");
    assert_eq!(
        &got_names, want_names,
        "next_unique_mount_point_name diverged\n  got:  {got_names:?}\n  want: {want_names:?}"
    );

    eprintln!("OK: mount_case_repair: {checked} cases (rows, indexes, probe errors, log lines) + naming leaf matched oracle");
}
