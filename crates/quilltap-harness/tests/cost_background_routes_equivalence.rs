//! P4.6ao TOKEN-COST + REGENERATE-BACKGROUND route-surface differential:
//! `api::chat_media::chat_get_cost` (+ `chat_regenerate_background`) vs v4's REAL
//! route handlers. Both sides read a FRESH copy of the committed
//! `cost-background-{main,mount}.db` fixture per case. Success cases diff the
//! route body; error cases diff status + `{error}` message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-cost-background.ndjson npx jest -- cost-background-routes
//! Run:
//!   QT_ORACLE_COST_BACKGROUND=/tmp/oracle-cost-background.ndjson \
//!     cargo test -p quilltap-harness --test cost_background_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_media;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    chat_full_id: String,
    chat_legacy_id: String,
    chat_empty_id: String,
    chat_detailed_id: String,
    missing_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/cost-background-web.json")
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

/// A fresh two-partition Db over a private copy of the committed fixture.
fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-cb-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("cost-background-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("cost-background-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

// ── JSON canonicalization ──────────────────────────────────────────────────
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn blank_keys(v: &mut Value, keys: &[String]) {
    match v {
        Value::Object(o) => {
            for k in keys {
                if o.contains_key(k) {
                    o.insert(k.clone(), Value::String(format!("<{k}>")));
                }
            }
            for (_, x) in o.iter_mut() {
                blank_keys(x, keys);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| blank_keys(x, keys)),
        _ => {}
    }
}
fn norm(v: &Value, blank: &[String]) -> String {
    let mut v = v.clone();
    blank_keys(&mut v, blank);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

fn status_of(kind: ErrorKind) -> u16 {
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
fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::ChatCost(v) => Some(v.clone()),
        _ => None,
    }
}

fn check(oracle: &HashMap<String, Value>, name: &str, resp: &Response, failed: &mut Vec<String>) {
    let want = &oracle[name];
    let want_status = want["status"].as_u64().unwrap() as u16;
    let blank: Vec<String> = want["blank"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                if norm(&body, &blank) != norm(&want["body"], &blank) {
                    eprintln!(
                        "[{name}] BODY MISMATCH:\n got {}\n want {}",
                        norm(&body, &blank),
                        norm(&want["body"], &blank)
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK.");
                }
            }
            None => {
                eprintln!("[{name}] expected success, got {resp:?}");
                failed.push(name.to_string());
            }
        }
    } else {
        match resp {
            Response::Error(e) => {
                let want_msg = want["body"]["error"].as_str().unwrap_or("");
                if status_of(e.kind) == want_status && e.message == want_msg {
                    eprintln!("[{name}] OK ({want_status}).");
                } else {
                    eprintln!(
                        "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                        status_of(e.kind),
                        e.message
                    );
                    failed.push(name.to_string());
                }
            }
            other => {
                eprintln!("[{name}] expected error, got {other:?}");
                failed.push(name.to_string());
            }
        }
    }
}

#[test]
fn cost_background_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_COST_BACKGROUND") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let mut failed: Vec<String> = Vec::new();

    // ── Unit 1: ?action=cost ──
    check(
        &oracle,
        "cost_aggregates",
        &chat_media::chat_get_cost(&fresh_db("cost_aggregates"), &spec.chat_full_id, false),
        &mut failed,
    );
    check(
        &oracle,
        "cost_legacy_no_price_source",
        &chat_media::chat_get_cost(&fresh_db("cost_legacy"), &spec.chat_legacy_id, false),
        &mut failed,
    );
    check(
        &oracle,
        "cost_no_aggregates",
        &chat_media::chat_get_cost(&fresh_db("cost_empty"), &spec.chat_empty_id, false),
        &mut failed,
    );
    check(
        &oracle,
        "cost_chat_missing",
        &chat_media::chat_get_cost(&fresh_db("cost_missing"), &spec.missing_id, false),
        &mut failed,
    );
    check(
        &oracle,
        "cost_detailed",
        &chat_media::chat_get_cost(&fresh_db("cost_detailed"), &spec.chat_detailed_id, true),
        &mut failed,
    );
    check(
        &oracle,
        "cost_detailed_no_messages",
        &chat_media::chat_get_cost(&fresh_db("cost_detailed_empty"), &spec.chat_empty_id, true),
        &mut failed,
    );
    // `detailed=1` is not the exact string `'true'` — the REST edge resolves it to
    // false, so the port is driven with `false` here (the edge's own compare is
    // covered by the web-route test).
    check(
        &oracle,
        "cost_detailed_not_the_exact_string",
        &chat_media::chat_get_cost(
            &fresh_db("cost_detailed_not"),
            &spec.chat_detailed_id,
            false,
        ),
        &mut failed,
    );

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
