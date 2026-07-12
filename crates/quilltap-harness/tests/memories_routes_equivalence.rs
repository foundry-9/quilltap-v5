//! P4.6s MEMORIES route-surface differential: `api::memories::*` vs v4's REAL
//! memories route handlers. Both sides read a FRESH copy of the committed
//! memories-{main,mount}.db fixture per case (baked ids identical → no remap).
//! Reads carry the full body; error arms are compared as HTTP status + message
//! (the kind→status recipe). Mutation / job-enqueue arms are added by the later
//! P4.6s commits.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-memories-routes.ndjson npx jest -- memories-routes
//! Run:
//!   QT_ORACLE_MEMORIES_ROUTES=/tmp/oracle-memories-routes.ndjson \
//!     cargo test -p quilltap-harness --test memories_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::memories;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const MNEMO: &str = "b1000000-0000-4000-8000-000000000001";
const ORLA: &str = "b1000000-0000-4000-8000-000000000002";
const CHAT_SALON: &str = "c1000000-0000-4000-8000-000000000001";
const MSG_S1: &str = "d1000000-0000-4000-8000-000000000001";
const MSG_S2A: &str = "d1000000-0000-4000-8000-000000000002";
const MEM_TAGGED_2: &str = "b2000000-0000-4000-8000-0000000000a1";
const MISSING: &str = "00000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memories-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// v4 `lib/api/responses.ts` kind→status.
fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Locked => 423,
        ErrorKind::Internal => 500,
    }
}

fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-mem-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("memories-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("memories-mount.db"), &mount).unwrap();
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

#[test]
fn memories_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_MEMORIES_ROUTES") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
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
    let uid = spec.user_id.clone();

    // A 200-body check (baked ids identical → no blanking).
    let check_body = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0);
        if want_status != 200 {
            eprintln!("[{name}] expected an error arm; use check_error");
            failed.push(name.to_string());
            return;
        }
        let got = response_data(got);
        let want = &rec["body"];
        if norm(&got) != norm(want) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got), &norm(want))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    // An error-arm check: HTTP status + message (v4 `{error}` body).
    let check_error = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
        let want_msg = rec["body"]["error"].as_str().unwrap_or("");
        match got {
            Response::Error(e) => {
                let got_status = status_of(e.kind);
                if got_status != want_status || e.message != want_msg {
                    eprintln!(
                        "[{name}] MISMATCH: got {got_status} {:?} / want {want_status} {want_msg:?}",
                        e.message
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK ({got_status}).");
                }
            }
            other => {
                eprintln!("[{name}] expected Error, got {:?}", response_data(other));
                failed.push(name.to_string());
            }
        }
    };

    // --- List (paginated) ---
    {
        let db = fresh_db(&spec, "lp");
        check_body(
            "list_paginated",
            &memories::memory_list(&db, MNEMO, None, None, None, None, None, Some(20), Some(0)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lp2");
        check_body(
            "list_paginated_p2",
            &memories::memory_list(&db, MNEMO, None, None, None, None, None, Some(20), Some(20)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpi");
        check_body(
            "list_paginated_importance_asc",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                None,
                Some("importance"),
                Some("asc"),
                Some(15),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lps");
        check_body(
            "list_paginated_search",
            &memories::memory_list(
                &db,
                MNEMO,
                Some("smuggler"),
                None,
                None,
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpm");
        check_body(
            "list_paginated_minImportance",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                Some(0.7),
                None,
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpsrc");
        check_body(
            "list_paginated_source_manual",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                Some("MANUAL"),
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    // --- List (legacy) ---
    {
        let db = fresh_db(&spec, "ll");
        check_body(
            "list_legacy",
            &memories::memory_list(&db, ORLA, None, None, None, None, None, None, None),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lls");
        check_body(
            "list_legacy_search",
            &memories::memory_list(
                &db,
                MNEMO,
                Some("barometer"),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lmc");
        check_error(
            "list_missing_character",
            &memories::memory_list(&db, MISSING, None, None, None, None, None, None, None),
            &mut failed,
        );
    }
    // --- Item GET ---
    {
        let db = fresh_db(&spec, "get");
        check_body(
            "get",
            &rt.block_on(memories::memory_get(&db, MEM_TAGGED_2)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "getm");
        check_error(
            "get_missing",
            &rt.block_on(memories::memory_get(&db, MISSING)),
            &mut failed,
        );
    }
    // --- Count by chat ---
    {
        let db = fresh_db(&spec, "cbc");
        check_body(
            "count_by_chat",
            &memories::memory_count_by_chat(&db, CHAT_SALON),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "cbcm");
        check_error(
            "count_by_chat_missing",
            &memories::memory_count_by_chat(&db, MISSING),
            &mut failed,
        );
    }
    // --- By message ---
    {
        let db = fresh_db(&spec, "bms");
        check_body(
            "by_message_swipe",
            &memories::memory_by_message(&db, &uid, MSG_S2A),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bm1");
        check_body(
            "by_message_single",
            &memories::memory_by_message(&db, &uid, MSG_S1),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bmm");
        check_error(
            "by_message_missing",
            &memories::memory_by_message(&db, &uid, MISSING),
            &mut failed,
        );
    }
    // --- Character memory counts ---
    {
        let db = fresh_db(&spec, "cc");
        check_body(
            "character_counts",
            &memories::memory_character_counts(&db, &uid),
            &mut failed,
        );
    }

    assert!(failed.is_empty(), "mismatched cases: {failed:?}");
}
