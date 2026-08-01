//! P4.6p GLOBAL MOUNT-POINTS route-surface differential: `api::mount_points::*` vs
//! v4's REAL mount-point route handlers, over a FRESH copy of the committed
//! groups-projects fixture per case. Create mints id + timestamps (blanked); patch
//! mints updatedAt (blanked). The scaffold + cascade side effects are verified via
//! folder-path / per-table-count dumps. Error cases assert the Error kind's HTTP
//! status + message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-mount-points-routes.ndjson npx jest -- mount-points-routes
//! Run:
//!   QT_ORACLE_MOUNT_ROUTES=/tmp/oracle-mount-points-routes.ndjson \
//!     cargo test -p quilltap-harness --test mount_points_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::mount_points as mp;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const GAMMA_EXTRA_MP: &str = "b0000000-0000-4000-8000-000000000001";
const MP_INDEXED: &str = "b0000000-0000-4000-8000-0000000000c0";
const BOGUS: &str = "b0000000-0000-4000-8000-0000000000ee";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    #[allow(dead_code)]
    user_id: String,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/groups-projects.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
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
fn blank_keys(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(o) => {
            for k in keys {
                if o.contains_key(*k) {
                    o.insert((*k).to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_keys(x, keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| blank_keys(x, keys)),
        _ => {}
    }
}
fn norm(v: &Value, blank: &[&str]) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_keys(&mut v, blank);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}
fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}
fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-mp-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("groups-projects-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

fn dump_folders(db: &Db, mount_id: &str) -> Value {
    let mp = mount_id.to_string();
    db.read_mount_index(move |conn| {
        let mut stmt = conn
            .prepare("SELECT path FROM doc_mount_folders WHERE mountPointId = ?1 ORDER BY path")?;
        let rows = stmt
            .query_map(rusqlite::params![mp], |r| {
                Ok(json!({ "path": r.get::<_, String>(0)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "folders": rows }))
    })
    .unwrap()
}

fn dump_cascade(db: &Db, mount_id: &str) -> Value {
    let mp = mount_id.to_string();
    db.read_mount_index(move |conn| {
        let count = |sql: &str| -> rusqlite::Result<i64> {
            conn.query_row(sql, rusqlite::params![mp], |r| r.get::<_, i64>(0))
        };
        Ok(json!({
            "chunks": count("SELECT COUNT(*) FROM doc_mount_chunks WHERE mountPointId = ?1")?,
            "files": count("SELECT COUNT(*) FROM doc_mount_files f WHERE EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = f.id AND l.mountPointId = ?1)")?,
            "fileLinks": count("SELECT COUNT(*) FROM doc_mount_file_links WHERE mountPointId = ?1")?,
            "documents": count("SELECT COUNT(*) FROM doc_mount_documents WHERE fileId IN (SELECT fileId FROM doc_mount_file_links WHERE mountPointId = ?1)")?,
            "folders": count("SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId = ?1")?,
            "projectLinks": count("SELECT COUNT(*) FROM project_doc_mount_links WHERE mountPointId = ?1")?,
            "mountPointExists": count("SELECT COUNT(*) FROM doc_mount_points WHERE id = ?1")? > 0,
        }))
    })
    .unwrap()
}

fn created_mount_id(resp: &Response) -> String {
    response_data(resp)
        .get("mountPoint")
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn mount_points_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MOUNT_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_ROUTES (see test header).");
        return;
    };
    let spec = spec();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    let ok = |name: &str, resp: &Response, blank: &[&str], failed: &mut Vec<String>| {
        if let Response::Error(e) = resp {
            eprintln!("[{name}] expected success, got {:?}: {}", e.kind, e.message);
            failed.push(name.to_string());
            return;
        }
        let want = &oracle[name]["body"];
        let got = response_data(resp);
        if norm(&got, blank) != norm(want, blank) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got, blank), &norm(want, blank))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    let check_tables = |name: &str, got: &Value, failed: &mut Vec<String>| {
        let want = &oracle[name]["tables"];
        if norm(got, &[]) != norm(want, &[]) {
            eprintln!(
                "[{name} tables] MISMATCH:\n{}",
                first_diff(&norm(got, &[]), &norm(want, &[]))
            );
            failed.push(format!("{name}_tables"));
        } else {
            eprintln!("[{name} tables] OK.");
        }
    };
    let err = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        let want_msg = want["body"]["error"].as_str().unwrap_or("");
        match resp {
            Response::Error(e) if http_for(e.kind) == want_status && e.message == want_msg => {
                eprintln!("[{name}] OK (err {want_status}).");
            }
            Response::Error(e) => {
                eprintln!(
                    "[{name}] ERR MISMATCH: got {}:'{}' want {}/'{}'",
                    http_for(e.kind),
                    e.message,
                    want_status,
                    want_msg
                );
                failed.push(name.to_string());
            }
            _ => {
                eprintln!("[{name}] expected error {want_status}, got success");
                failed.push(name.to_string());
            }
        }
    };

    const CREATED: &[&str] = &["id", "createdAt", "updatedAt"];
    const UPDATED: &[&str] = &["updatedAt"];

    // --- Reads ---
    ok(
        "list",
        &mp::mount_point_list(&fresh_db(&spec, "l")),
        &[],
        &mut failed,
    );
    ok(
        "get",
        &mp::mount_point_get(&fresh_db(&spec, "g"), MP_INDEXED),
        &[],
        &mut failed,
    );
    err(
        "get_404",
        &mp::mount_point_get(&fresh_db(&spec, "g4"), BOGUS),
        &mut failed,
    );

    // --- Create ---
    ok(
        "create_database",
        &rt.block_on(mp::mount_point_create(
            &fresh_db(&spec, "cdb"),
            json!({ "name": "DB Store", "mountType": "database", "storeType": "documents" }),
        )),
        CREATED,
        &mut failed,
    );
    ok(
        "create_filesystem_warning",
        &rt.block_on(mp::mount_point_create(
            &fresh_db(&spec, "cfw"),
            json!({ "name": "FS Store", "mountType": "filesystem", "basePath": "/nonexistent/qt-mp-xyz-98765" }),
        )),
        CREATED,
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "ccs");
        let resp = rt.block_on(mp::mount_point_create(
            &db,
            json!({ "name": "Char Store", "mountType": "database", "storeType": "character" }),
        ));
        ok("create_character_scaffold", &resp, CREATED, &mut failed);
        let id = created_mount_id(&resp);
        check_tables(
            "create_character_scaffold",
            &dump_folders(&db, &id),
            &mut failed,
        );
    }
    err(
        "create_refine_400",
        &rt.block_on(mp::mount_point_create(
            &fresh_db(&spec, "cr4"),
            json!({ "name": "Bad", "mountType": "filesystem", "basePath": "" }),
        )),
        &mut failed,
    );
    // [0a0419f5] case-insensitive store-name clash on create → 409.
    err(
        "create_name_clash",
        &rt.block_on(mp::mount_point_create(
            &fresh_db(&spec, "cnc"),
            json!({ "name": "gamma extra store", "mountType": "database", "storeType": "documents" }),
        )),
        &mut failed,
    );

    // --- Patch ---
    ok(
        "patch_happy",
        &rt.block_on(mp::mount_point_update(
            &fresh_db(&spec, "ph"),
            GAMMA_EXTRA_MP,
            json!({ "name": "Gamma Store Renamed", "enabled": false }),
        )),
        UPDATED,
        &mut failed,
    );
    err(
        "patch_zod_500",
        &rt.block_on(mp::mount_point_update(
            &fresh_db(&spec, "pz"),
            GAMMA_EXTRA_MP,
            json!({ "mountType": "bogus" }),
        )),
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "pf");
        let resp = rt.block_on(mp::mount_point_update(
            &db,
            GAMMA_EXTRA_MP,
            json!({ "storeType": "character" }),
        ));
        ok("patch_scaffold_flip", &resp, UPDATED, &mut failed);
        check_tables(
            "patch_scaffold_flip",
            &dump_folders(&db, GAMMA_EXTRA_MP),
            &mut failed,
        );
    }
    err(
        "patch_404",
        &rt.block_on(mp::mount_point_update(
            &fresh_db(&spec, "p4"),
            BOGUS,
            json!({ "name": "x" }),
        )),
        &mut failed,
    );
    // [0a0419f5] renaming Indexed Store to a case-variant of Gamma Extra Store
    // (another store) → 409; the clash excludes the store itself.
    err(
        "patch_name_clash",
        &rt.block_on(mp::mount_point_update(
            &fresh_db(&spec, "pnc"),
            MP_INDEXED,
            json!({ "name": "GAMMA EXTRA STORE" }),
        )),
        &mut failed,
    );
    // [0a0419f5] renaming a store to a case-variant of its OWN name is allowed.
    ok(
        "patch_name_case_only_self",
        &rt.block_on(mp::mount_point_update(
            &fresh_db(&spec, "pncs"),
            GAMMA_EXTRA_MP,
            json!({ "name": "GAMMA EXTRA STORE" }),
        )),
        UPDATED,
        &mut failed,
    );

    // --- Delete ---
    {
        let db = fresh_db(&spec, "del");
        let resp = rt.block_on(mp::mount_point_delete(&db, MP_INDEXED));
        ok("delete", &resp, &[], &mut failed);
        check_tables("delete", &dump_cascade(&db, MP_INDEXED), &mut failed);
    }
    err(
        "delete_404",
        &rt.block_on(mp::mount_point_delete(&fresh_db(&spec, "d4"), BOGUS)),
        &mut failed,
    );

    assert!(failed.is_empty(), "mount-points-routes FAILED: {failed:?}");
}
