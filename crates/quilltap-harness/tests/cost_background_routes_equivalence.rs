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
    user_enabled_id: String,
    user_disabled_id: String,
    user_no_profile_id: String,
    chat_full_id: String,
    chat_legacy_id: String,
    chat_empty_id: String,
    chat_detailed_id: String,
    chat_regen_id: String,
    chat_no_chars_id: String,
    chat_mixed_status_id: String,
    chat_all_left_id: String,
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
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}
fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::ChatCost(v) | Response::ChatMedia(v) => Some(v.clone()),
        _ => None,
    }
}

/// `priority`/`maxAttempts` are `f64` on the Rust row but whole numbers on the
/// wire — render them as JS would (`-1`, not `-1.0`) so the diff compares values,
/// not float formatting.
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

/// Every `background_jobs` row, id-ordered — the unit-2 enqueue proof, in the
/// same shape the oracle emits (`payload` PARSED, so the two sides diff the
/// object, not two different serializations of it).
fn dump_jobs(db: &Db) -> Value {
    let mut rows: Vec<Value> = db
        .read_main(|conn| {
            let jobs = quilltap_core::db::background_jobs::BackgroundJobsRepository::new(conn)
                .find_all()?;
            Ok(jobs
                .into_iter()
                .map(|j| {
                    serde_json::json!({
                        "id": j.id,
                        "userId": j.user_id,
                        "type": j.job_type,
                        "status": j.status,
                        "priority": js_num(j.priority),
                        "maxAttempts": js_num(j.max_attempts),
                        "payload": serde_json::from_str::<Value>(&j.payload)
                            .unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>())
        })
        .expect("read background_jobs");
    rows.sort_by_key(|v| v["id"].as_str().unwrap_or("").to_string());
    Value::Array(rows)
}

/// Diff the `background_jobs` dump against the oracle's, blanking the minted id.
fn check_jobs(oracle: &HashMap<String, Value>, name: &str, db: &Db, failed: &mut Vec<String>) {
    let want = match oracle[name].get("jobs") {
        Some(j) => j.clone(),
        None => return,
    };
    let blank = vec!["id".to_string(), "jobId".to_string()];
    let got = dump_jobs(db);
    if norm(&got, &blank) != norm(&want, &blank) {
        eprintln!(
            "[{name}] JOBS MISMATCH:\n got {}\n want {}",
            norm(&got, &blank),
            norm(&want, &blank)
        );
        failed.push(format!("{name}:jobs"));
    } else {
        eprintln!(
            "[{name}] jobs OK ({} row(s)).",
            got.as_array().unwrap().len()
        );
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

    // ── Unit 2: ?action=regenerate-background ──
    // The settings arm is chosen by the USER the call runs as (v4 reads chat
    // settings by user) — the same three fixture users the oracle signs in as.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for (name, user, chat) in [
        (
            "regen_disabled",
            &spec.user_disabled_id,
            &spec.chat_regen_id,
        ),
        (
            "regen_no_image_profile",
            &spec.user_no_profile_id,
            &spec.chat_regen_id,
        ),
        (
            "regen_no_characters",
            &spec.user_enabled_id,
            &spec.chat_no_chars_id,
        ),
        ("regen_queued", &spec.user_enabled_id, &spec.chat_regen_id),
        // [P4.D146 / v4 70505745a] The presence gate: one participant per status
        // → only active + silent reach the enqueued payload's `characterIds`
        // (the job row is the assertion); nobody present → the REWORDED 400 and
        // no job at all.
        (
            "regen_present_participants_only",
            &spec.user_enabled_id,
            &spec.chat_mixed_status_id,
        ),
        (
            "regen_all_participants_left",
            &spec.user_enabled_id,
            &spec.chat_all_left_id,
        ),
        (
            "regen_chat_missing",
            &spec.user_enabled_id,
            &spec.missing_id,
        ),
    ] {
        let db = fresh_db(name);
        let resp = rt.block_on(chat_media::chat_regenerate_background(&db, user, chat));
        check(&oracle, name, &resp, &mut failed);
        check_jobs(&oracle, name, &db, &mut failed);
    }

    // The dedupe arm: a second call reuses the pending job — the
    // already-in-progress message, ONE job row, and the SAME jobId.
    {
        let db = fresh_db("regen_dedupe");
        let first = rt.block_on(chat_media::chat_regenerate_background(
            &db,
            &spec.user_enabled_id,
            &spec.chat_regen_id,
        ));
        let second = rt.block_on(chat_media::chat_regenerate_background(
            &db,
            &spec.user_enabled_id,
            &spec.chat_regen_id,
        ));
        check(&oracle, "regen_dedupe", &second, &mut failed);
        check_jobs(&oracle, "regen_dedupe", &db, &mut failed);

        // The identity claim the blanked body can't carry (see the oracle case).
        let job_id =
            |r: &Response| success_body(r).and_then(|b| b["jobId"].as_str().map(String::from));
        let (a, b) = (job_id(&first), job_id(&second));
        assert!(a.is_some(), "regen_dedupe: first call produced no jobId");
        let same = a == b;
        let want_same = oracle["regen_dedupe"]["extra"]["sameJobId"]
            .as_bool()
            .expect("oracle extra.sameJobId");
        if same != want_same {
            eprintln!("[regen_dedupe] sameJobId MISMATCH: got {same} / want {want_same}");
            failed.push("regen_dedupe:sameJobId".to_string());
        } else {
            eprintln!("[regen_dedupe] sameJobId OK ({same}).");
        }
    }

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
