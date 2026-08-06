//! P4.9H2A EMBEDDING-PROFILES route-surface differential: `api::embedding_profiles::*`
//! vs v4's REAL embedding-profile route handlers, over a FRESH copy of the committed
//! `embedding-profiles-{main,mount}.db` fixture per case. Create/update mint id +
//! timestamps (blanked); refit/reindex/reapply mint a jobId (blanked). The matrix
//! cases diff the post-op `background_jobs` types + `embedding_status` counts — the
//! matrix claim is STATE, not prose.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-ep-routes.ndjson npx jest -- embedding-profiles-routes
//! Run:
//!   QT_ORACLE_EP_ROUTES=/tmp/oracle-ep-routes.ndjson \
//!     cargo test -p quilltap-harness --test embedding_profiles_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::embedding_profiles as ep;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const APIKEY: &str = "ca000000-0000-4000-8000-0000000000c1";
const EP_DEFAULT: &str = "e0000000-0000-4000-8000-000000000001";
const EP_BUILTIN: &str = "e0000000-0000-4000-8000-000000000002";
const EP_TRUNC: &str = "e0000000-0000-4000-8000-000000000003";
const EP_TRUNCFREE: &str = "e0000000-0000-4000-8000-000000000004";
const BOGUS: &str = "e0000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/ep-mgmt.json");
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
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-ep-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("embedding-profiles-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("embedding-profiles-mount.db"), &mount).unwrap();
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

/// v4's `dumpState`: background_jobs types (ordered) + embedding_status counts.
fn dump_state(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut js = conn.prepare("SELECT type FROM background_jobs ORDER BY type")?;
        let jobs: Vec<String> = js
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        let mut ss = conn.prepare(
            "SELECT status, COUNT(*) FROM embedding_status GROUP BY status ORDER BY status",
        )?;
        let status: Vec<Value> = ss
            .query_map([], |r| {
                Ok(json!({ "status": r.get::<_, String>(0)?, "n": r.get::<_, i64>(1)? }))
            })?
            .collect::<Result<_, _>>()?;
        Ok(json!({ "jobs": jobs, "status": status }))
    })
    .unwrap()
}

#[test]
fn embedding_profiles_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EP_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_EP_ROUTES (see test header).");
        return;
    };
    let spec = spec();
    let uid = spec.user_id.clone();
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
    const JOBID: &[&str] = &["jobId"];

    // ── reads ──────────────────────────────────────────────────────────────
    ok(
        "list",
        &ep::embedding_profile_list(&fresh_db(&spec, "list"), &uid),
        &[],
        &mut failed,
    );
    ok(
        "get",
        &ep::embedding_profile_get(&fresh_db(&spec, "get"), EP_DEFAULT),
        &[],
        &mut failed,
    );
    ok(
        "get_builtin",
        &ep::embedding_profile_get(&fresh_db(&spec, "getb"), EP_BUILTIN),
        &[],
        &mut failed,
    );
    err(
        "get_404",
        &ep::embedding_profile_get(&fresh_db(&spec, "get404"), BOGUS),
        &mut failed,
    );

    // ── create ─────────────────────────────────────────────────────────────
    ok(
        "create_happy",
        &rt.block_on(ep::embedding_profile_create(
            &fresh_db(&spec, "ch"),
            &uid,
            json!({ "name": "New External", "provider": "OPENAI", "apiKeyId": APIKEY, "modelName": "text-embedding-3-small", "dimensions": 1536 }),
        )),
        CREATED,
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "cd");
        let r = rt.block_on(ep::embedding_profile_create(
            &db,
            &uid,
            json!({ "name": "New Default", "provider": "OPENAI", "modelName": "text-embedding-3-small", "isDefault": true }),
        ));
        ok("create_default_triggers_reindex", &r, CREATED, &mut failed);
        check_tables(
            "create_default_triggers_reindex",
            &dump_state(&db),
            &mut failed,
        );
    }
    err(
        "create_dup_409",
        &rt.block_on(ep::embedding_profile_create(
            &fresh_db(&spec, "cdup"),
            &uid,
            json!({ "name": "OpenAI Default", "provider": "OPENAI", "modelName": "x" }),
        )),
        &mut failed,
    );
    err(
        "create_missing_name_400",
        &rt.block_on(ep::embedding_profile_create(
            &fresh_db(&spec, "cmn"),
            &uid,
            json!({ "provider": "OPENAI", "modelName": "x" }),
        )),
        &mut failed,
    );
    err(
        "create_bad_dimensions_400",
        &rt.block_on(ep::embedding_profile_create(
            &fresh_db(&spec, "cbd"),
            &uid,
            json!({ "name": "Z", "provider": "OPENAI", "modelName": "x", "dimensions": -5 }),
        )),
        &mut failed,
    );
    err(
        "create_apikey_404",
        &rt.block_on(ep::embedding_profile_create(
            &fresh_db(&spec, "cak"),
            &uid,
            json!({ "name": "Z", "provider": "OPENAI", "modelName": "x", "apiKeyId": BOGUS }),
        )),
        &mut failed,
    );

    // ── update: the trigger matrix ───────────────────────────────────────────
    let matrix = |name: &str, id: &str, body: Value, failed: &mut Vec<String>| {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(ep::embedding_profile_update(&db, &uid, id, body));
        ok(name, &r, UPDATED, failed);
        check_tables(name, &dump_state(&db), failed);
    };
    matrix(
        "update_default_model_full_reindex",
        EP_DEFAULT,
        json!({ "modelName": "text-embedding-3-large" }),
        &mut failed,
    );
    matrix(
        "update_builtin_became_default_refit",
        EP_BUILTIN,
        json!({ "isDefault": true }),
        &mut failed,
    );
    matrix(
        "update_became_default_full_reindex",
        EP_TRUNC,
        json!({ "isDefault": true, "truncateToDimensions": 256 }),
        &mut failed,
    );
    matrix(
        "update_default_narrow_reapply",
        EP_DEFAULT,
        json!({ "truncateToDimensions": 512 }),
        &mut failed,
    );
    matrix(
        "update_default_widen_reindex",
        EP_DEFAULT,
        json!({ "truncateToDimensions": 3000 }),
        &mut failed,
    );
    matrix(
        "update_nondefault_model_no_job",
        EP_TRUNC,
        json!({ "modelName": "text-embedding-3-small" }),
        &mut failed,
    );
    matrix(
        "update_default_normalizeL2_only_no_job",
        EP_DEFAULT,
        json!({ "normalizeL2": false }),
        &mut failed,
    );
    matrix(
        "update_clear_apikey",
        EP_DEFAULT,
        json!({ "apiKeyId": null }),
        &mut failed,
    );
    err(
        "update_dup_409",
        &rt.block_on(ep::embedding_profile_update(
            &fresh_db(&spec, "udup"),
            &uid,
            EP_TRUNC,
            json!({ "name": "OpenAI Default" }),
        )),
        &mut failed,
    );
    err(
        "update_404",
        &rt.block_on(ep::embedding_profile_update(
            &fresh_db(&spec, "u404"),
            &uid,
            BOGUS,
            json!({ "name": "X" }),
        )),
        &mut failed,
    );

    // ── delete ───────────────────────────────────────────────────────────────
    ok(
        "delete",
        &rt.block_on(ep::embedding_profile_delete(
            &fresh_db(&spec, "del"),
            EP_TRUNCFREE,
        )),
        &[],
        &mut failed,
    );
    err(
        "delete_404",
        &rt.block_on(ep::embedding_profile_delete(
            &fresh_db(&spec, "del404"),
            BOGUS,
        )),
        &mut failed,
    );

    // ── refit ────────────────────────────────────────────────────────────────
    {
        let db = fresh_db(&spec, "refit");
        let r = rt.block_on(ep::embedding_profile_refit(&db, &uid, EP_BUILTIN));
        ok("refit_builtin", &r, JOBID, &mut failed);
        check_tables("refit_builtin", &dump_state(&db), &mut failed);
    }
    err(
        "refit_non_builtin_400",
        &rt.block_on(ep::embedding_profile_refit(
            &fresh_db(&spec, "refitnb"),
            &uid,
            EP_DEFAULT,
        )),
        &mut failed,
    );
    err(
        "refit_404",
        &rt.block_on(ep::embedding_profile_refit(
            &fresh_db(&spec, "refit404"),
            &uid,
            BOGUS,
        )),
        &mut failed,
    );

    // ── reindex ──────────────────────────────────────────────────────────────
    let reindex = |name: &str, id: &str, scope: Option<&str>, failed: &mut Vec<String>| {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(ep::embedding_profile_reindex(
            &db,
            &uid,
            id,
            scope.map(str::to_string),
        ));
        ok(name, &r, JOBID, failed);
        check_tables(name, &dump_state(&db), failed);
    };
    reindex("reindex_all", EP_DEFAULT, Some("all"), &mut failed);
    // Legacy no-body call == scope None == 'all'.
    reindex("reindex_legacy_no_body", EP_DEFAULT, None, &mut failed);
    reindex(
        "reindex_mismatched",
        EP_TRUNC,
        Some("mismatched-dim"),
        &mut failed,
    );
    err(
        "reindex_mismatched_no_target_400",
        &rt.block_on(ep::embedding_profile_reindex(
            &fresh_db(&spec, "rmnt"),
            &uid,
            EP_BUILTIN,
            Some("mismatched-dim".to_string()),
        )),
        &mut failed,
    );
    err(
        "reindex_bad_scope_400",
        &rt.block_on(ep::embedding_profile_reindex(
            &fresh_db(&spec, "rbs"),
            &uid,
            EP_DEFAULT,
            Some("nonsense".to_string()),
        )),
        &mut failed,
    );

    // ── reapply ──────────────────────────────────────────────────────────────
    {
        let db = fresh_db(&spec, "reapply");
        let r = rt.block_on(ep::embedding_profile_reapply(&db, &uid, EP_TRUNC));
        ok("reapply_has_trunc", &r, JOBID, &mut failed);
        check_tables("reapply_has_trunc", &dump_state(&db), &mut failed);
    }
    err(
        "reapply_no_trunc_400",
        &rt.block_on(ep::embedding_profile_reapply(
            &fresh_db(&spec, "rant"),
            &uid,
            EP_TRUNCFREE,
        )),
        &mut failed,
    );
    err(
        "reapply_404",
        &rt.block_on(ep::embedding_profile_reapply(
            &fresh_db(&spec, "ra404"),
            &uid,
            BOGUS,
        )),
        &mut failed,
    );

    assert!(failed.is_empty(), "route differential failures: {failed:?}");
    eprintln!("OK: embedding_profiles routes matched oracle.");
}
