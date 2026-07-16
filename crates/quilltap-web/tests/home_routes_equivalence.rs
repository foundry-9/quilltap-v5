//! P4.6au HOME-DASHBOARD differential: `api::system::system_home` (over
//! `services::home::get_home_data`) vs v4's REAL exported `getHomeData` +
//! the REAL `GET /api/v1/system/home` route handler. Both sides read a FRESH
//! copy of the committed `home-{main,mount}.db` family per case; the mutation
//! cases replay the oracle's raw SQL on their own copy before the read.
//!
//! Every timestamp in the fixture is PINNED, so the payload diffs EXACT — the
//! `check` normalizes key ORDER only (so a mismatch reports values, not
//! ordering), and [`check_key_order`] separately pins the raw key sequence of
//! the richest payload (`route_primary`) against v4's `JSON.stringify` bytes —
//! that is what proves the DTO field order AND the omit-vs-null splits
//! (`defaultImageId`/`url` omitted when falsy vs `character`/`defaultImage`/
//! `storyBackgroundUrl` explicit null; project `description`/`color`/`icon`
//! present-vs-absent pass-throughs).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-home.ndjson npx jest -- home-routes
//! Run:
//!   QT_ORACLE_HOME=/tmp/oracle-home.ndjson \
//!     cargo test -p quilltap-web --test home_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::system::system_home;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mutations {
    help_flip_chat_id: String,
    null_lm_chat_id: String,
    touch_file_id: String,
    touch_file_to: String,
    reproject_chat_id: String,
    reproject_to_project_id: String,
    clear_bg_chat_id: String,
    orphan_bg_file_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    empty_user_id: String,
    missing_user_id: String,
    mutations: Mutations,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/home-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
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
    let scratch = std::env::temp_dir().join(format!("qt-home-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("home-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("home-mount.db"), &mount).unwrap();
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

/// Replay one oracle mutation (the same raw SQL the oracle ran on ITS copy).
fn mutate(db: &Db, sql: &'static str, params: Vec<String>) {
    db.write_blocking(move |ws| {
        let bound: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        ws.main().connection().execute(sql, bound.as_slice())?;
        Ok(())
    })
    .expect("mutation SQL");
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
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
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
        Response::SystemHome(v) => Some(v.clone()),
        _ => None,
    }
}

fn check(oracle: &HashMap<String, Value>, name: &str, resp: &Response, failed: &mut Vec<String>) {
    let want = &oracle[name];
    let want_status = want["status"].as_u64().unwrap() as u16;
    if (200..300).contains(&want_status) {
        match success_body(resp) {
            Some(body) => {
                if norm(&body) != norm(&want["body"]) {
                    eprintln!(
                        "[{name}] BODY MISMATCH:\n got {}\n want {}",
                        norm(&body),
                        norm(&want["body"])
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

/// Every object's key sequence, depth-first — the shape `check`'s sorted
/// compare deliberately throws away.
fn key_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            out.push(format!(
                "{path}: {}",
                o.keys().cloned().collect::<Vec<_>>().join(",")
            ));
            for (k, x) in o.iter() {
                key_paths(x, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// The wire-order claim: the Rust body's key sequence must match v4's
/// `JSON.stringify` bytes exactly, not merely carry the same key SET. Works
/// because quilltap-core builds `serde_json` with `preserve_order`.
fn check_key_order(
    oracle_raw: &HashMap<String, String>,
    name: &str,
    resp: &Response,
    failed: &mut Vec<String>,
) {
    let want_line: Value = serde_json::from_str(&oracle_raw[name]).unwrap();
    let mut want = Vec::new();
    key_paths(&want_line["body"], "", &mut want);
    let mut got = Vec::new();
    key_paths(&success_body(resp).expect("success body"), "", &mut got);
    if got != want {
        eprintln!("[{name}] KEY ORDER MISMATCH:\n got {got:#?}\n want {want:#?}");
        failed.push(format!("{name}:keyOrder"));
    } else {
        eprintln!("[{name}] key order OK ({} objects).", got.len());
    }
}

/// The round's §1 Shared-contract wire shapes (always on — no oracle needed;
/// this is a serialization contract, not a behavior diff): the dispatch verb
/// is `systemHome` with NO parameters, and the success envelope is tagged
/// `systemHome`. Pins the serde attributes against a later rename slip (the
/// P4.6ar wire-contract precedent).
#[test]
fn p4_6au_shared_contract_wire_shapes() {
    use quilltap_core::api::types::Request;
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"type":"systemHome"}"#)
            .expect("systemHome as the SPA sends it"),
        Request::SystemHome
    );
    let resp = Response::SystemHome(serde_json::json!({ "displayName": "Friday" }));
    assert_eq!(
        serde_json::to_string(&resp).unwrap(),
        r#"{"type":"systemHome","data":{"displayName":"Friday"}}"#
    );
}

#[test]
fn home_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_HOME") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    // The RAW lines too — `check_key_order` needs v4's `JSON.stringify` bytes.
    let mut oracle_raw: HashMap<String, String> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        let name = v["name"].as_str().unwrap().to_string();
        oracle_raw.insert(name.clone(), line.to_string());
        oracle.insert(name, v);
    }

    let mut failed: Vec<String> = Vec::new();
    let m = &spec.mutations;

    // ── The route (envelope) cases. v4's route derives `fallbackName` from the
    // session mock (`ctx.user.name` — undefined → null), so the Rust replay is
    // a None fallback; `successResponse(data)` is the raw payload, so the body
    // IS the service body. The primary carries the wire-order claim across the
    // richest full payload (both avatar kinds, tags, the dangling participant,
    // the omit-vs-null splits, all three project sources).
    {
        let db = fresh_db("route_primary");
        let resp = system_home(&db, &spec.user_id, None);
        check(&oracle, "route_primary", &resp, &mut failed);
        check_key_order(&oracle_raw, "route_primary", &resp, &mut failed);
    }
    {
        let db = fresh_db("route_empty");
        let resp = system_home(&db, &spec.empty_user_id, None);
        check(&oracle, "route_empty_user", &resp, &mut failed);
    }

    // ── The displayName ladder + the scoping split, service-level ──
    let service = |name: &str, user: &str, fallback: Option<&str>, failed: &mut Vec<String>| {
        let db = fresh_db(name);
        let resp = system_home(&db, user, fallback);
        check(&oracle, name, &resp, failed);
    };
    service("service_primary", &spec.user_id, None, &mut failed);
    service(
        "service_primary_fallback_ignored",
        &spec.user_id,
        Some("Reginald"),
        &mut failed,
    );
    service(
        "service_empty_user_fallback",
        &spec.empty_user_id,
        Some("Reginald"),
        &mut failed,
    );
    service(
        "service_empty_user_no_fallback",
        &spec.empty_user_id,
        None,
        &mut failed,
    );
    service(
        "service_empty_user_blank_fallback",
        &spec.empty_user_id,
        Some(""),
        &mut failed,
    );
    service(
        "service_missing_user_fallback",
        &spec.missing_user_id,
        Some("Reginald"),
        &mut failed,
    );

    // ── The mutation cases: replay the oracle's raw SQL, then read ──
    let mutated = |name: &str, sql: &'static str, params: Vec<String>, failed: &mut Vec<String>| {
        let db = fresh_db(name);
        mutate(&db, sql, params);
        let resp = system_home(&db, &spec.user_id, None);
        check(&oracle, name, &resp, failed);
    };
    mutated(
        "mutate_help_flip",
        r#"UPDATE "chats" SET "chatType" = ?1 WHERE "id" = ?2"#,
        vec!["help".into(), m.help_flip_chat_id.clone()],
        &mut failed,
    );
    mutated(
        "mutate_null_last_message",
        r#"UPDATE "chats" SET "lastMessageAt" = NULL WHERE "id" = ?1"#,
        vec![m.null_lm_chat_id.clone()],
        &mut failed,
    );
    mutated(
        "mutate_touch_file",
        r#"UPDATE "files" SET "updatedAt" = ?1 WHERE "id" = ?2"#,
        vec![m.touch_file_to.clone(), m.touch_file_id.clone()],
        &mut failed,
    );
    mutated(
        "mutate_reproject_chat",
        r#"UPDATE "chats" SET "projectId" = ?1 WHERE "id" = ?2"#,
        vec![
            m.reproject_to_project_id.clone(),
            m.reproject_chat_id.clone(),
        ],
        &mut failed,
    );
    mutated(
        "mutate_clear_background",
        r#"UPDATE "chats" SET "storyBackgroundImageId" = NULL WHERE "id" = ?1"#,
        vec![m.clear_bg_chat_id.clone()],
        &mut failed,
    );
    mutated(
        "mutate_orphan_background",
        r#"DELETE FROM "files" WHERE "id" = ?1"#,
        vec![m.orphan_bg_file_id.clone()],
        &mut failed,
    );

    // Every oracle case must have been exercised (a renamed case would
    // otherwise pass vacuously).
    let exercised = 14;
    assert_eq!(
        oracle.len(),
        exercised,
        "oracle carries {} cases but the test exercises {exercised}",
        oracle.len()
    );

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
