//! P4.6p IMAGE-PROFILES route-surface differential: `api::image_profiles::*` vs
//! v4's REAL image-profile route handlers, over a FRESH copy of the committed
//! groups-projects fixture per case. Create mints id + timestamps (blanked);
//! update mints updatedAt (blanked); the isDefault side effect is verified via a
//! `{name, isDefault}` dump. Error cases assert the Error kind's HTTP status +
//! message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-image-profiles-routes.ndjson npx jest -- image-profiles-routes
//! Run:
//!   QT_ORACLE_IMAGE_ROUTES=/tmp/oracle-image-profiles-routes.ndjson \
//!     cargo test -p quilltap-harness --test image_profiles_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::image_profiles as ip;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const APIKEY: &str = "a0000001-0000-4000-8000-000000000001";
const DIANA: &str = "a1000000-0000-4000-8000-000000000004";
const IP_1: &str = "a6000000-0000-4000-8000-000000000001";
const IP_2: &str = "a6000000-0000-4000-8000-000000000002";
const IP_3: &str = "a6000000-0000-4000-8000-000000000003";
const BOGUS: &str = "a6000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
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
    let scratch = std::env::temp_dir().join(format!("qt-ip-{}-{}", tag, std::process::id()));
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

fn dump_profiles(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare("SELECT name, isDefault FROM image_profiles ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({ "name": r.get::<_, String>(0)?, "isDefault": r.get::<_, i64>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "profiles": rows }))
    })
    .unwrap()
}

#[test]
fn image_profiles_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGE_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_ROUTES (see test header).");
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

    // --- Reads ---
    ok(
        "list_plain",
        &ip::image_profile_list(&fresh_db(&spec, "lp"), &uid, None),
        &[],
        &mut failed,
    );
    ok(
        "list_by_char",
        &ip::image_profile_list(&fresh_db(&spec, "lc"), &uid, Some(DIANA.to_string())),
        &[],
        &mut failed,
    );
    ok(
        "list_providers",
        &ip::image_provider_list(),
        &[],
        &mut failed,
    );
    ok(
        "get",
        &ip::image_profile_get(&fresh_db(&spec, "g"), IP_1),
        &[],
        &mut failed,
    );
    err(
        "get_404",
        &ip::image_profile_get(&fresh_db(&spec, "g4"), BOGUS),
        &mut failed,
    );

    // --- Create ---
    {
        let db = fresh_db(&spec, "c_happy");
        let body = json!({
            "name": "New Imagery",
            "provider": "OPENAI",
            "apiKeyId": APIKEY,
            "baseUrl": "http://127.0.0.1:2/v1",
            "modelName": "  gpt-image-1  ",
            "parameters": { "steps": 25 },
            "isDangerousCompatible": true
        });
        ok(
            "create_happy",
            &rt.block_on(ip::image_profile_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "c_def");
        let body = json!({ "name": "Defaulted", "provider": "OPENAI", "modelName": "gpt-image-1", "isDefault": true });
        ok(
            "create_default_unsets",
            &rt.block_on(ip::image_profile_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
        check_tables("create_default_unsets", &dump_profiles(&db), &mut failed);
    }
    err(
        "create_name_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cnr"),
            &uid,
            json!({}),
        )),
        &mut failed,
    );
    err(
        "create_provider_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cpr"),
            &uid,
            json!({ "name": "X" }),
        )),
        &mut failed,
    );
    err(
        "create_model_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cmr"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI" }),
        )),
        &mut failed,
    );
    err(
        "create_params_not_object",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cpo"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "parameters": [1, 2] }),
        )),
        &mut failed,
    );
    err(
        "create_apikey_404",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "ca4"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "apiKeyId": "00000000-0000-4000-8000-0000000000ff" }),
        )),
        &mut failed,
    );
    err(
        "create_dup_409",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cd4"),
            &uid,
            json!({ "name": "Primary Imagery", "provider": "OPENAI", "modelName": "m" }),
        )),
        &mut failed,
    );

    // --- Update ---
    ok(
        "update_apikey_null",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "uak"),
            &uid,
            IP_1,
            json!({ "apiKeyId": null }),
        )),
        UPDATED,
        &mut failed,
    );
    ok(
        "update_baseurl_empty",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "ube"),
            &uid,
            IP_1,
            json!({ "baseUrl": "" }),
        )),
        UPDATED,
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "uid");
        ok(
            "update_isdefault",
            &rt.block_on(ip::image_profile_update(
                &db,
                &uid,
                IP_2,
                json!({ "isDefault": true }),
            )),
            UPDATED,
            &mut failed,
        );
        check_tables("update_isdefault", &dump_profiles(&db), &mut failed);
    }

    // --- Delete ---
    ok(
        "delete",
        &rt.block_on(ip::image_profile_delete(&fresh_db(&spec, "d"), IP_3)),
        &[],
        &mut failed,
    );
    err(
        "delete_404",
        &rt.block_on(ip::image_profile_delete(&fresh_db(&spec, "d4"), BOGUS)),
        &mut failed,
    );

    assert!(
        failed.is_empty(),
        "image-profiles-routes FAILED: {failed:?}"
    );
}

/// The remaining LLM/IO-coupled actions are loud typed refusals this round.
/// `imageProfileGenerate` was un-refused in P4.6ai (its loud not-assembled refusal
/// now lives at the engine's `image_generation` seam gate, not this handler — see
/// `image_generate_route_equivalence`); `validate-key` / `list-models` stay armed.
#[test]
fn image_refusal_arms_are_loud() {
    for resp in [
        ip::image_profile_validate_key(),
        ip::image_profile_list_models(),
    ] {
        match resp {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert!(
                    e.message.contains("recognized but not yet available"),
                    "unexpected refusal message: {}",
                    e.message
                );
            }
            _ => panic!("refusal arm returned success"),
        }
    }
}
