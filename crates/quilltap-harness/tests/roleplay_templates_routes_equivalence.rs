//! P4.6p ROLEPLAY-TEMPLATES route-surface differential: `api::roleplay_templates::*`
//! vs v4's REAL roleplay-template route handlers. Both sides read a FRESH copy of
//! the committed groups-projects fixture per case; built-in ids (minted at fixture
//! build) are resolved from the fixture and identical on both sides. Create mints
//! id + timestamps (blanked); update mints updatedAt (blanked). Error cases assert
//! the Error kind's HTTP status + message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-roleplay-templates-routes.ndjson npx jest -- roleplay-templates-routes
//! Run:
//!   QT_ORACLE_ROLEPLAY_ROUTES=/tmp/oracle-roleplay-templates-routes.ndjson \
//!     cargo test -p quilltap-harness --test roleplay_templates_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::roleplay_templates as rt;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const RT_USER_1: &str = "a4000000-0000-4000-8000-000000000001";
const RT_USER_2: &str = "a4000000-0000-4000-8000-000000000002";
const BOGUS: &str = "a4000000-0000-4000-8000-0000000000ff";

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
        ErrorKind::Locked => 503,
        ErrorKind::Internal => 500,
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-rt-{}-{}", tag, std::process::id()));
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

fn builtin_id(db: &Db, name: &str) -> String {
    db.read_main(|m| {
        m.query_row(
            "SELECT id FROM roleplay_templates WHERE isBuiltIn = 1 AND name = ?1",
            rusqlite::params![name],
            |r| r.get::<_, String>(0),
        )
        .map_err(Into::into)
    })
    .expect("resolve built-in id")
}

#[test]
fn roleplay_templates_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ROLEPLAY_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_ROLEPLAY_ROUTES (see test header).");
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

    let rt_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    // Resolve built-in ids (stable across copies).
    let standard = builtin_id(&fresh_db(&spec, "resolve"), "Standard");
    let quilltap_rp = builtin_id(&fresh_db(&spec, "resolve2"), "Quilltap RP");

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
        &rt::roleplay_template_list(&fresh_db(&spec, "list"), &uid),
        &[],
        &mut failed,
    );
    ok(
        "get_user",
        &rt::roleplay_template_get(&fresh_db(&spec, "gu"), RT_USER_1),
        &[],
        &mut failed,
    );
    ok(
        "get_regen",
        &rt::roleplay_template_get(&fresh_db(&spec, "gr"), RT_USER_2),
        &[],
        &mut failed,
    );
    ok(
        "get_builtin",
        &rt::roleplay_template_get(&fresh_db(&spec, "gb"), &standard),
        &[],
        &mut failed,
    );
    err(
        "get_missing",
        &rt::roleplay_template_get(&fresh_db(&spec, "gm"), BOGUS),
        &mut failed,
    );

    // --- Create ---
    {
        let db = fresh_db(&spec, "c_happy");
        let body = json!({
            "name": "Fresh Template",
            "description": "  A fresh one.  ",
            "systemPrompt": "  Write freshly.  ",
            "narrationDelimiters": "*",
            "delimiters": [
                { "kind": "wrap", "name": "Emph", "buttonName": "Em", "delimiters": "~", "style": "qt-rp-emph" },
                { "kind": "linePrefix", "name": "OOC", "buttonName": "O", "marker": "// ", "style": "qt-rp-ooc" }
            ]
        });
        ok(
            "create_happy",
            &rt_rt.block_on(rt::roleplay_template_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "c_arr");
        let body = json!({
            "name": "Bracketed",
            "systemPrompt": "Use brackets.",
            "narrationDelimiters": ["[", "]"],
            "delimiters": [
                { "kind": "wrap", "name": "Nar", "buttonName": "N", "delimiters": ["[", "]"], "style": "qt-rp-nar" }
            ]
        });
        ok(
            "create_narration_array",
            &rt_rt.block_on(rt::roleplay_template_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
    }
    err(
        "create_name_required",
        &rt_rt.block_on(rt::roleplay_template_create(
            &fresh_db(&spec, "c_nn"),
            &uid,
            json!({}),
        )),
        &mut failed,
    );
    err(
        "create_sysprompt_required",
        &rt_rt.block_on(rt::roleplay_template_create(
            &fresh_db(&spec, "c_sp"),
            &uid,
            json!({ "name": "X" }),
        )),
        &mut failed,
    );
    err(
        "create_narration_required",
        &rt_rt.block_on(rt::roleplay_template_create(
            &fresh_db(&spec, "c_nr"),
            &uid,
            json!({ "name": "X", "systemPrompt": "Y" }),
        )),
        &mut failed,
    );
    err(
        "create_dup",
        &rt_rt.block_on(rt::roleplay_template_create(
            &fresh_db(&spec, "c_dup"),
            &uid,
            json!({ "name": "Bracket Prose", "systemPrompt": "Y", "narrationDelimiters": "*" }),
        )),
        &mut failed,
    );

    // --- Update ---
    ok(
        "update_happy",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_h"),
            &uid,
            RT_USER_1,
            json!({ "name": "Renamed Prose", "systemPrompt": "  Trimmed.  " }),
        )),
        UPDATED,
        &mut failed,
    );
    err(
        "update_builtin_403",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_b"),
            &uid,
            &standard,
            json!({ "name": "Nope" }),
        )),
        &mut failed,
    );
    err(
        "update_dup",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_d"),
            &uid,
            RT_USER_2,
            json!({ "name": "Bracket Prose" }),
        )),
        &mut failed,
    );
    ok(
        "update_regen_delims",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_rd"),
            &uid,
            RT_USER_1,
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "delimiters": [{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q" }] }),
        )),
        UPDATED,
        &mut failed,
    );
    ok(
        "update_regen_narration",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_rn"),
            &uid,
            RT_USER_1,
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "narrationDelimiters": "|" }),
        )),
        UPDATED,
        &mut failed,
    );
    ok(
        "update_null_description",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_nd"),
            &uid,
            RT_USER_1,
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "description": null }),
        )),
        UPDATED,
        &mut failed,
    );
    err(
        "update_missing_required",
        &rt_rt.block_on(rt::roleplay_template_update(
            &fresh_db(&spec, "u_mr"),
            &uid,
            RT_USER_1,
            json!({ "delimiters": [{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q" }] }),
        )),
        &mut failed,
    );

    // --- Delete ---
    ok(
        "delete_happy",
        &rt_rt.block_on(rt::roleplay_template_delete(
            &fresh_db(&spec, "d_h"),
            RT_USER_2,
        )),
        &[],
        &mut failed,
    );
    err(
        "delete_builtin_403",
        &rt_rt.block_on(rt::roleplay_template_delete(
            &fresh_db(&spec, "d_b"),
            &quilltap_rp,
        )),
        &mut failed,
    );
    err(
        "delete_missing",
        &rt_rt.block_on(rt::roleplay_template_delete(&fresh_db(&spec, "d_m"), BOGUS)),
        &mut failed,
    );

    assert!(
        failed.is_empty(),
        "roleplay-templates-routes FAILED: {failed:?}"
    );
}
