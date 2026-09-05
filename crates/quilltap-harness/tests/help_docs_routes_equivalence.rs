//! P4.9I2A HELP-DOCS route-surface differential: `api::help_docs::*` vs v4's
//! REAL `app/api/v1/help-docs` route handlers, both over a FRESH copy of the
//! committed `help-chat-{main,mount}.db` fixture per case (baked ids identical
//! → no remap; nothing here mutates).
//!
//! Arms: list; the unknown-action AND empty-action fallthroughs to the list
//! (this route is one of v4's DEFAULT-SERVING `?action=` shapes — memory note
//! `v4-has-three-action-dispatch-shapes`); chat-count for two users (A: eleven
//! help + one brahma chat filtered out, one salon counted; C: zero); get by id,
//! by slug, and the 404; and search over the fixture's 17 REAL synced docs — a
//! title+content hit, a content-only hit, case-insensitivity, no hit, the
//! one-char short-circuit, ONE astral char (two UTF-16 units — NOT
//! short-circuited), padding trimmed, a common word with many hits, a word from
//! inside a code fence, `q` absent (→ `''`) and `q` empty.
//!
//! v5 has no per-verb HTTP status on `Response::HelpDocs`, so the differential
//! encodes the status per case (200 / the error kinds) and diffs that + the body.
//!
//! Generate the oracle (Node 24, from the v4 checkout — mirror to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   mkdir -p /tmp/help-docs-routes/cases /tmp/help-docs-routes/fixtures
//!   cp $V5W/harness/oracle/cases/help-docs-routes.test.ts /tmp/help-docs-routes/cases/
//!   cp $V5W/harness/oracle/fixtures/help-chat-web.json /tmp/help-docs-routes/fixtures/
//!   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
//!   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-help-docs-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots /tmp/help-docs-routes/cases -- help-docs-routes
//! Run:
//!   QT_ORACLE_HELP_DOCS_ROUTES=/tmp/oracle-help-docs-routes.ndjson \
//!     cargo test -p quilltap-harness --test help_docs_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::help_docs;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const USER_A: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const USER_C: &str = "a38e25de-85fa-4751-ac07-93bd9c72a072";
/// The fixture's minted id for `help/brahma-console.md` (`help-chat-main.db.meta.json`).
const BRAHMA_DOC_ID: &str = "d1c8c363-e1c4-48fa-90fb-da0c4229dd9f";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/help-chat-web.json")
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
            return format!("  line {i}\n  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}

fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::HelpDocs(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!(
        "qt-help-docs-routes-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("help-chat-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("help-chat-mount.db"), &mount).unwrap();
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
fn help_docs_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_DOCS_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_DOCS_ROUTES (see test header).");
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
    let mut checked = 0usize;
    let mut check = |name: &str, resp: &Response| {
        checked += 1;
        let (status, body) = status_body(resp);
        let want = &oracle[name];
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK ({status}).");
        }
    };

    // The list, and the two fallthroughs (the edge resolves `?action=bogus` /
    // `?action=` to the same `HelpDocsList` verb — asserted here by dispatching
    // the verb the edge would pick; the edge's selection is pinned by the web
    // wire test).
    for name in [
        "list",
        "list_unknown_action_falls_through",
        "list_empty_action_falls_through",
    ] {
        let db = fresh_db(&spec, name);
        check(name, &help_docs::help_docs_list(&db));
    }
    {
        let db = fresh_db(&spec, "cca");
        check(
            "chat_count_user_a",
            &help_docs::help_docs_chat_count(&db, USER_A),
        );
    }
    {
        let db = fresh_db(&spec, "ccc");
        check(
            "chat_count_user_c",
            &help_docs::help_docs_chat_count(&db, USER_C),
        );
    }
    {
        let db = fresh_db(&spec, "getid");
        check("get_by_id", &help_docs::help_doc_get(&db, BRAHMA_DOC_ID));
    }
    {
        let db = fresh_db(&spec, "getslug");
        check(
            "get_by_slug",
            &help_docs::help_doc_get(&db, "brahma-console"),
        );
    }
    {
        let db = fresh_db(&spec, "getmiss");
        check(
            "get_missing_404",
            &help_docs::help_doc_get(&db, "no-such-doc"),
        );
    }
    let searches: Vec<(&str, Option<&str>)> = vec![
        ("search_title_and_content", Some("Brahma")),
        ("search_content_only", Some("wildcard")),
        ("search_case_insensitive", Some("SALON")),
        ("search_none", Some("zzqx-nothing-here")),
        ("search_one_char_short_circuit", Some("a")),
        ("search_one_astral_char", Some("😀")),
        ("search_padded_trims", Some("  taboo  ")),
        ("search_common_word_many_hits", Some("the")),
        ("search_fenced_code_word", Some("quilltap")),
        ("search_q_absent", None),
        ("search_q_empty", Some("")),
    ];
    for (name, q) in &searches {
        let db = fresh_db(&spec, name);
        check(name, &help_docs::help_docs_search(&db, *q));
    }

    assert_eq!(
        checked,
        oracle.len(),
        "the Rust case list and the oracle disagree: {checked} checked vs {} recorded",
        oracle.len()
    );
    assert!(failed.is_empty(), "help-docs-routes FAILED: {failed:?}");
}
