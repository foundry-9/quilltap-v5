//! P4.6f characters action-verbs differential: the thin action handlers
//! (`api::characters::{character_favorite, character_toggle_controlled_by,
//! character_toggle_carina, character_set_default_partner, character_avatar,
//! character_add_tag, character_remove_tag}`) vs v4's REAL
//! `characters/[id]/handlers/post.ts`. Each case runs on a FRESH copy of the
//! committed characters fixture; the response echo AND the post-op slim
//! character row (`find_by_id_raw`) are diffed.
//!
//! Two transport-shape facts are folded in: (a) success bodies are compared, not
//! HTTP status (the dispatch envelope maps every success to 200 — v4's 201 for
//! add-tag is a REST concern); (b) v4's error body `{error: msg}` maps to the
//! dispatch `Error{kind, message}` — for guard failures the test asserts the
//! kind's HTTP status matches v4's status and the message matches `body.error`.
//! Normalized: the op-minted `updatedAt` and the read-time-minted
//! `physicalDescription.{createdAt,updatedAt}`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-characters-actions.ndjson npx jest -- characters-actions
//! Run:
//!   QT_ORACLE_CHARACTERS_ACTIONS=/tmp/oracle-characters-actions.ndjson \
//!     cargo test -p quilltap-harness --test characters_actions_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
const CLEO: &str = "a1000000-0000-4000-8000-000000000003";
const DAX_IMG: &str = "f0000002-0000-4000-8000-000000000002";
const TAG_MYSTERY: &str = "70000002-0000-4000-8000-000000000002";
const TAG_ADVENTURE: &str = "70000001-0000-4000-8000-000000000001";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/characters.json")
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
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

/// Blank the op-minted `updatedAt` on a slim/overlaid character object plus the
/// read-time-minted `physicalDescription.{createdAt,updatedAt}` (the established
/// char normalization). Recurses into `character`/`data` wrappers.
fn strip_char_ts(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        if o.contains_key("updatedAt") {
            o.insert("updatedAt".into(), Value::String("<ts>".into()));
        }
        if let Some(pd) = o
            .get_mut("physicalDescription")
            .and_then(Value::as_object_mut)
        {
            pd.remove("createdAt");
            pd.remove("updatedAt");
        }
    }
}
fn strip_payload_ts(v: &mut Value) {
    // The payload is `{ body: {...}, raw: {...} }`; the character sits under
    // body.character or body.data.
    if let Some(o) = v.as_object_mut() {
        if let Some(raw) = o.get_mut("raw") {
            strip_char_ts(raw);
        }
        if let Some(body) = o.get_mut("body").and_then(Value::as_object_mut) {
            for key in ["character", "data"] {
                if let Some(c) = body.get_mut(key) {
                    strip_char_ts(c);
                }
            }
        }
    }
}

fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::NotFound => 404,
        ErrorKind::Locked => 503,
        ErrorKind::Internal => 500,
    }
}

/// Open a fresh Db over a copy of the committed fixture.
fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-char-actions-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
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
fn characters_actions_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHARACTERS_ACTIONS") else {
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
    let uid = spec.user_id.clone();

    // Each case: (name, closure producing the Response over a fresh Db).
    // The closure runs the verb; the harness then re-reads the raw row itself.
    let mut failed: Vec<String> = Vec::new();

    // Helper: run a success case (compare body echo + raw row).
    let mut run_success = |name: &str, resp: Response, db: &Db| {
        let want = &oracle[name];
        // Error kind on a success case is an immediate failure.
        if let Response::Error(e) = &resp {
            eprintln!("[{name}] expected success, got error: {}", e.message);
            failed.push(name.to_string());
            return;
        }
        let mut got = serde_json::json!({
            "body": response_data(&resp),
            "raw": read_raw(db, ARIA),
        });
        let mut wnt = serde_json::json!({
            "body": want["body"].clone(),
            "raw": want["raw"].clone(),
        });
        strip_payload_ts(&mut got);
        strip_payload_ts(&mut wnt);
        if norm(&got) != norm(&wnt) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got), &norm(&wnt))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };

    {
        let db = fresh_db(&spec, "favorite");
        let r = rt.block_on(characters::character_favorite(&db, &uid, ARIA));
        run_success("favorite", r, &db);
    }
    {
        let db = fresh_db(&spec, "tcb");
        let r = rt.block_on(characters::character_toggle_controlled_by(&db, &uid, ARIA));
        run_success("toggle_controlled_by", r, &db);
    }
    {
        let db = fresh_db(&spec, "tc");
        let r = rt.block_on(characters::character_toggle_carina(&db, &uid, ARIA));
        run_success("toggle_carina", r, &db);
    }
    {
        let db = fresh_db(&spec, "spv");
        let r = rt.block_on(characters::character_set_default_partner(
            &db,
            &uid,
            ARIA,
            Some(CLEO),
        ));
        run_success("set_partner_valid", r, &db);
    }
    {
        let db = fresh_db(&spec, "spc");
        let r = rt.block_on(characters::character_set_default_partner(
            &db, &uid, ARIA, None,
        ));
        run_success("set_partner_clear", r, &db);
    }
    {
        let db = fresh_db(&spec, "avset");
        let r = rt.block_on(characters::character_avatar(&db, &uid, ARIA, Some(DAX_IMG)));
        run_success("avatar_set", r, &db);
    }
    {
        let db = fresh_db(&spec, "avclr");
        let r = rt.block_on(characters::character_avatar(&db, &uid, ARIA, None));
        run_success("avatar_clear", r, &db);
    }
    {
        let db = fresh_db(&spec, "addtag");
        let r = rt.block_on(characters::character_add_tag(&db, &uid, ARIA, TAG_MYSTERY));
        run_success("add_tag", r, &db);
    }
    {
        let db = fresh_db(&spec, "rmtag");
        let r = rt.block_on(characters::character_remove_tag(
            &db,
            &uid,
            ARIA,
            TAG_ADVENTURE,
        ));
        run_success("remove_tag", r, &db);
    }

    // Guard-failure cases: compare error kind's HTTP status + message.
    let mut run_error = |name: &str, resp: Response| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        let want_msg = want["body"]["error"].as_str().unwrap_or("");
        match &resp {
            Response::Error(e) => {
                if http_for(e.kind) != want_status || e.message != want_msg {
                    eprintln!(
                        "[{name}] MISMATCH: got {}/{:?} want {}/{}",
                        http_for(e.kind),
                        e.message,
                        want_status,
                        want_msg
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK.");
                }
            }
            _ => {
                eprintln!("[{name}] expected error, got success");
                failed.push(name.to_string());
            }
        }
    };

    {
        let db = fresh_db(&spec, "llm_fail");
        let resp = rt.block_on(characters::character_set_default_partner(
            &db,
            &uid,
            ARIA,
            Some(BRAM),
        ));
        run_error("set_partner_llm_fail", resp);
    }
    {
        let db = fresh_db(&spec, "self_fail");
        let resp = rt.block_on(characters::character_set_default_partner(
            &db,
            &uid,
            ARIA,
            Some(ARIA),
        ));
        run_error("set_partner_self_fail", resp);
    }

    assert!(failed.is_empty(), "characters-actions FAILED: {failed:?}");
}

/// Read the slim character row (v4 `findByIdRaw`) for the write verification.
fn read_raw(db: &Db, id: &str) -> Value {
    let id = id.to_string();
    db.read_main(move |main| quilltap_core::db::characters_read::find_by_id_raw(main, &id))
        .unwrap()
        .unwrap_or(Value::Null)
}
