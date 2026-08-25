//! Tier-2 differential for the GROUP WARDROBE CRUD (P4.D112, v4 `d7263f39`):
//! `api::groups::group_wardrobe_{list,create,get,update,delete}` vs v4's REAL
//! `app/api/v1/groups/[id]/wardrobe/route.ts` + `[itemId]/route.ts` handlers
//! (through the real `createContextParamsHandler` middleware — the schema 400s
//! are ITS flat `{error: 'Validation error'}` envelope, the standing
//! project-wide `details` deferral).
//!
//! Both sides run each case against a FRESH copy of the SAME baked fixture
//! pair — the wardrobe-transfers fixture, whose group already carries a
//! provisioned official store, a `Wardrobe/` folder, and the Household Livery
//! item — then dump the seven mount-index tables (the transfers family's
//! shared-id-map remap form). Bodies diff after per-case `normalize` dot-paths
//! blank the minted values, so every fixture-baked id compares LITERALLY.
//!
//! v5 serves this family DISPATCH-ONLY (the project-wardrobe precedent): the
//! v4 REST URLs get no quilltap-web edge, so success statuses (200 vs the
//! create 201) live at the transport and are not comparable here; error
//! statuses ARE compared via the ErrorKind mapping.
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   TMPO=/tmp/qt-group-wardrobe-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5/harness/oracle/cases/group-wardrobe.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5/harness/oracle/fixtures/group-wardrobe.json" "$TMPO/harness/oracle/fixtures/"
//!   cp "$V5/harness/oracle/fixtures/wardrobe-transfers-tier2.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WTR_MAIN=/tmp/qt-gw-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-gw-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-transfers-fixture.ts
//!   QT_FIXTURE_GW_MAIN=/tmp/qt-gw-main.db QT_FIXTURE_GW_MOUNT=/tmp/qt-gw-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-group-wardrobe.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- group-wardrobe
//! Run:
//!   QT_ORACLE_GROUP_WARDROBE=/tmp/oracle-group-wardrobe.ndjson \
//!   QT_FIXTURE_GW_MAIN=/tmp/qt-gw-main.db QT_FIXTURE_GW_MOUNT=/tmp/qt-gw-mount.db \
//!     cargo test -p quilltap-harness --test group_wardrobe_routes_equivalence
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::api::groups;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "itemId")]
    item_id: String,
    cases: Vec<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/group-wardrobe.json")
}

fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked | ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

/// Blank one dot-path (array indices are numeric segments) to `"<norm>"`.
fn blank_path(v: &mut Value, path: &str) {
    let mut cur = v;
    let segs: Vec<&str> = path.split('.').collect();
    for (i, seg) in segs.iter().enumerate() {
        let last = i == segs.len() - 1;
        if last {
            match cur {
                Value::Object(o) => {
                    if o.contains_key(*seg) {
                        o.insert((*seg).to_string(), Value::String("<norm>".to_string()));
                    }
                }
                Value::Array(a) => {
                    if let Ok(idx) = seg.parse::<usize>() {
                        if let Some(slot) = a.get_mut(idx) {
                            *slot = Value::String("<norm>".to_string());
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        let next = match cur {
            Value::Object(o) => o.get_mut(*seg),
            Value::Array(a) => seg.parse::<usize>().ok().and_then(|idx| a.get_mut(idx)),
            _ => None,
        };
        match next {
            Some(n) => cur = n,
            None => return,
        }
    }
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

// ── The seven-table remap machinery (the transfers family's form) ───────────

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    sort_ref_cols: &'static [&'static str],
    ts_columns: &'static [&'static str],
    content_column: Option<&'static str>,
    sha_column: Option<&'static str>,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        sort_ref_cols: &[],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        content_column: None,
        sha_column: None,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        content_column: None,
        sha_column: None,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        sort_ref_cols: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: Some("sha256"),
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        sort_ref_cols: &["fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: Some("content"),
        sha_column: Some("contentSha256"),
    },
    TableSpec {
        table: "project_doc_mount_links",
        oracle_key: "projectLinks",
        order_by: "createdAt",
        id_columns: &["id", "projectId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
    },
    TableSpec {
        table: "group_doc_mount_links",
        oracle_key: "groupLinks",
        order_by: "createdAt",
        id_columns: &["id", "groupId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
    },
];

fn uuid_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    })
}

fn iso_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap())
}

fn placeholder_ts_in(s: &str) -> String {
    iso_re().replace_all(s, "<ts>").into_owned()
}

fn blank_uuids_in(s: &str) -> String {
    uuid_re().replace_all(s, "<uuid>").into_owned()
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let mut out = String::new();
    for b in h.finalize() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn norm_sort_key(row: &Value, spec: &TableSpec, id_map: &HashMap<String, String>) -> String {
    let empty = serde_json::Map::new();
    let obj = row.as_object().unwrap_or(&empty);
    let mut key = String::new();
    for col in spec.sort_ref_cols {
        let raw = obj.get(*col).and_then(Value::as_str).unwrap_or("");
        key.push_str(id_map.get(raw).map(String::as_str).unwrap_or(raw));
        key.push('\u{1f}');
    }
    let mut rest: Vec<(&String, String)> = obj
        .iter()
        .filter(|(k, _)| !spec.id_columns.contains(&k.as_str()))
        .map(|(k, v)| (k, v.to_string()))
        .collect();
    rest.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in rest {
        key.push_str(k);
        key.push('\u{1e}');
        key.push_str(&v);
        key.push('\u{1f}');
    }
    key
}

fn normalize_all(dumps: &mut [Value]) {
    for (i, spec) in TABLES.iter().enumerate() {
        if let Some(rows) = dumps[i].get_mut("rows").and_then(Value::as_array_mut) {
            for row in rows.iter_mut() {
                if let Some(obj) = row.as_object_mut() {
                    for (k, v) in obj.iter_mut() {
                        if let Value::String(s) = v {
                            *s = placeholder_ts_in(s);
                            if spec.content_column == Some(k.as_str()) {
                                *s = blank_uuids_in(s);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut file_sha_by_raw_id: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        let Some(cc) = spec.content_column else {
            continue;
        };
        let sc = spec.sha_column.unwrap();
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            let content = obj
                .get(cc)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let recomputed = format!("SHA_{}", sha256_hex(&content));
            if let Some(Value::String(fid)) = obj.get("fileId") {
                file_sha_by_raw_id.insert(fid.clone(), recomputed.clone());
            }
            obj.insert(sc.to_string(), Value::String(recomputed));
        }
    }
    for (i, spec) in TABLES.iter().enumerate() {
        if spec.table != "doc_mount_files" {
            continue;
        }
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            if let Some(Value::String(id)) = obj.get("id") {
                if let Some(sha) = file_sha_by_raw_id.get(id) {
                    obj.insert("sha256".to_string(), Value::String(sha.clone()));
                }
            }
        }
    }

    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap_or_else(|| panic!("{}: no rows array", spec.table));
        rows.sort_by_key(|a| norm_sort_key(a, spec, &id_map));
        for row in rows.iter_mut() {
            let obj = row
                .as_object_mut()
                .unwrap_or_else(|| panic!("{}: row not an object", spec.table));
            for col in spec.id_columns {
                if let Some(Value::String(raw)) = obj.get(*col) {
                    let next = format!("ID_{}", id_map.len());
                    let token = id_map.entry(raw.clone()).or_insert(next).clone();
                    obj.insert((*col).to_string(), Value::String(token));
                }
            }
        }
    }

    for (i, spec) in TABLES.iter().enumerate() {
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            for col in spec.ts_columns {
                if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
                }
            }
        }
    }

    let final_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        if let Some(rows) = dumps[i].get_mut("rows").and_then(Value::as_array_mut) {
            rows.sort_by(|a, b| {
                norm_sort_key(a, spec, &final_map).cmp(&norm_sort_key(b, spec, &final_map))
            });
        }
    }
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

#[test]
fn group_wardrobe_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_GROUP_WARDROBE") else {
        return;
    };
    let Some(main_fixture) = env_or_skip("QT_FIXTURE_GW_MAIN") else {
        return;
    };
    let Some(mount_fixture) = env_or_skip("QT_FIXTURE_GW_MOUNT") else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_by_name: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle line");
            (
                v.get("name").and_then(Value::as_str).unwrap().to_string(),
                v,
            )
        })
        .collect();
    assert_eq!(
        oracle_by_name.len(),
        spec.cases.len(),
        "oracle case count != corpus"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for case in &spec.cases {
        let name = case["name"].as_str().unwrap().to_string();
        let kind = case["kind"].as_str().unwrap();
        let gid = case
            .get("groupId")
            .and_then(Value::as_str)
            .unwrap_or(&spec.group_id)
            .to_string();
        let iid = case
            .get("itemId")
            .and_then(Value::as_str)
            .unwrap_or(&spec.item_id)
            .to_string();
        let body = case.get("body").cloned().unwrap_or(Value::Null);
        let want = oracle_by_name
            .get(&name)
            .unwrap_or_else(|| panic!("oracle missing case {name}"));

        // Fresh copies so the shared seed fixtures stay pristine.
        let pid = std::process::id();
        let scratch = std::env::temp_dir().join(format!("qt-gw-rust-{pid}-{name}"));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
        std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

        let resp = {
            let db = Db::open(
                DbPaths {
                    main: main_work.clone(),
                    mount_index: Some(mount_work.clone()),
                    llm_logs: None,
                },
                &spec.test_pepper_base64,
            )
            .expect("open db");
            match kind {
                "list" => groups::group_wardrobe_list(&db, &gid),
                "create" => rt.block_on(groups::group_wardrobe_create(&db, &gid, body)),
                "get" => groups::group_wardrobe_get(&db, &gid, &iid),
                "update" => rt.block_on(groups::group_wardrobe_update(&db, &gid, &iid, body)),
                "delete" => rt.block_on(groups::group_wardrobe_delete(&db, &gid, &iid)),
                other => panic!("unknown case kind {other}"),
            }
        };

        // Body / error comparison.
        let want_ok = want.get("ok").and_then(Value::as_bool).unwrap_or(false);
        match &resp {
            Response::Error(e) => {
                assert!(
                    !want_ok,
                    "case {name}: rust refused ({}:'{}') but oracle succeeded",
                    http_for(e.kind),
                    e.message
                );
                let want_status = want["status"].as_i64().unwrap();
                let want_msg = want["body"]["error"].as_str().unwrap_or("");
                assert_eq!(
                    (http_for(e.kind), e.message.as_str()),
                    (want_status, want_msg),
                    "case {name}: error shape"
                );
            }
            _ => {
                assert!(want_ok, "case {name}: rust succeeded but oracle refused");
                let mut got_body = response_data(&resp);
                let mut want_body = want["body"].clone();
                if let Some(paths) = case.get("normalize").and_then(Value::as_array) {
                    for p in paths.iter().filter_map(Value::as_str) {
                        blank_path(&mut got_body, p);
                        blank_path(&mut want_body, p);
                    }
                }
                assert_eq!(
                    sorted(&got_body),
                    sorted(&want_body),
                    "case {name}: body diverged\n  rust:   {got_body}\n  oracle: {want_body}"
                );
            }
        }

        // Table dump + remap diff (asserts the error arms wrote NOTHING).
        let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open mount for dump: {e}"));
        let mut got: Vec<Value> = TABLES
            .iter()
            .map(|s| {
                mount
                    .dump_table_json(s.table, s.order_by)
                    .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
            })
            .collect();
        drop(mount);
        let mut wanted: Vec<Value> = TABLES
            .iter()
            .map(|s| {
                want.get("tables")
                    .and_then(|t| t.get(s.oracle_key))
                    .cloned()
                    .unwrap_or_else(|| panic!("oracle missing table {}", s.oracle_key))
            })
            .collect();
        normalize_all(&mut got);
        normalize_all(&mut wanted);
        for (i, s) in TABLES.iter().enumerate() {
            assert_eq!(
                got[i]["columns"], wanted[i]["columns"],
                "case {name} / {}: column set",
                s.table
            );
            assert_eq!(
                got[i]["rows"], wanted[i]["rows"],
                "case {name} / {}: remapped rows diverged\n  rust:   {}\n  oracle: {}",
                s.table, got[i]["rows"], wanted[i]["rows"]
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    eprintln!(
        "OK: group-wardrobe tier-2 matched oracle ({} cases, 7 tables each).",
        spec.cases.len()
    );
}
