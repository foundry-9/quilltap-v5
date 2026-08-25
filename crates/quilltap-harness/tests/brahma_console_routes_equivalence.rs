//! P4.9I1A BRAHMA-CONSOLE CRUD route-surface differential: `api::brahma::*` vs
//! v4's REAL dedicated brahma-console route handlers. Both sides read a FRESH copy
//! of the committed `brahma-{main,mount}.db` fixture per case (baked ids identical
//! → no remap; the create-minted id + the create/rename/set-model minted
//! timestamps are blanked).
//!
//! v5 has no per-verb HTTP status on the dispatch `Response::BrahmaConsole`
//! payload (create's 201 is set at the `quilltap-web` REST edge, not the dispatch
//! layer), so the differential encodes the v4-faithful status per case and diffs
//! that + the body; the error arms' status comes straight from the `ErrorKind`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-brahma-routes.ndjson npx jest -- brahma-console-routes
//! Run:
//!   QT_ORACLE_BRAHMA_ROUTES=/tmp/oracle-brahma-routes.ndjson \
//!     cargo test -p quilltap-harness --test brahma_console_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::brahma;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const USER_A: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const USER_B: &str = "f28e15cd-74f9-4640-9b96-82ac8b619961";
const P2: &str = "c0000001-0000-4000-8000-000000000002";
const CHAT_A: &str = "c1000000-0000-4000-8000-00000000000a";
const CHAT_B: &str = "c1000000-0000-4000-8000-00000000000b";
const CHAT_C: &str = "c1000000-0000-4000-8000-00000000000c";
const CHAT_SALON: &str = "c1000000-0000-4000-8000-00000000000d";
const MISSING: &str = "00000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/brahma-console-web.json")
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
/// Blank the minted / bumped volatile keys (create id + create/rename/set-model
/// timestamps) wherever they appear; both sides differ legitimately there.
fn blank_volatile(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for k in ["id", "createdAt", "updatedAt", "lastMessageAt"] {
                if o.contains_key(k) {
                    o.insert(k.to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_volatile(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_volatile),
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
    blank_volatile(&mut v);
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
            for j in i.saturating_sub(2)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}"));
            return ctx;
        }
    }
    "(identical)".to_string()
}

/// The (status, body) a `Response` maps to at the web edge. `created` marks the
/// arm whose REST edge answers 201 (v4 `created`); every other `BrahmaConsole`
/// body is 200.
fn status_body(r: &Response, created: bool) -> (u16, Value) {
    match r {
        Response::BrahmaConsole(v) => (if created { 201 } else { 200 }, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                // The store-unavailable refusal (P4.23) — v4's deliberate
                // contextful 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// v4's middleware answers an uncaught `ZodError` with `{error: 'Validation
/// error', details: [...issues]}`; the `details` array is the standing
/// project-wide deferral, so v5 carries the sentence alone. This does not just
/// drop the key — it first PINS that the sentence left behind is exactly
/// `Validation error`, so a route answering some other string with a details
/// array would fail here rather than pass silently.
fn drop_zod_details(name: &str, want_body: &Value) -> Value {
    let Some(o) = want_body.as_object() else {
        return want_body.clone();
    };
    if !o.contains_key("details") {
        return want_body.clone();
    }
    assert_eq!(
        o.get("error").and_then(Value::as_str),
        Some("Validation error"),
        "case '{name}': a ZodError body whose top-level sentence is NOT \
         'Validation error' — the details deferral must not hide that"
    );
    json!({ "error": "Validation error" })
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-brahma-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("brahma-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("brahma-mount.db"), &mount).unwrap();
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
fn brahma_console_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_BRAHMA_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_BRAHMA_ROUTES (see test header).");
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    let mut checked = 0usize;
    let mut check = |name: &str, resp: &Response, created: bool| {
        checked += 1;
        let (status, body) = status_body(resp, created);
        let want = &oracle[name];
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        let want_body = drop_zod_details(name, &want["body"]);
        if norm(&body) != norm(&want_body) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want_body))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK ({status}).");
        }
    };

    // --- Reads (no mutation; still fresh copies for uniformity) ---
    {
        let db = fresh_db(&spec, "list");
        check("list", &brahma::brahma_console_list(&db, USER_A), false);
    }
    {
        let db = fresh_db(&spec, "listb");
        check(
            "list_other_user",
            &brahma::brahma_console_list(&db, USER_B),
            false,
        );
    }
    {
        let db = fresh_db(&spec, "get");
        check(
            "get",
            &brahma::brahma_console_get(&db, USER_A, CHAT_A),
            false,
        );
    }
    {
        let db = fresh_db(&spec, "getsalon");
        check(
            "get_salon_404",
            &brahma::brahma_console_get(&db, USER_A, CHAT_SALON),
            false,
        );
    }
    {
        let db = fresh_db(&spec, "getmissing");
        check(
            "get_missing_404",
            &brahma::brahma_console_get(&db, USER_A, MISSING),
            false,
        );
    }
    {
        let db = fresh_db(&spec, "getother");
        check(
            "get_other_user_404",
            &brahma::brahma_console_get(&db, USER_B, CHAT_A),
            false,
        );
    }
    {
        let db = fresh_db(&spec, "messages");
        check(
            "get_messages",
            &brahma::brahma_console_messages(&db, USER_A, CHAT_A),
            false,
        );
    }

    // --- Create ---
    {
        let db = fresh_db(&spec, "createdef");
        let r = rt.block_on(brahma::brahma_console_create(&db, USER_A, None));
        check("create_default", &r, true);
    }
    {
        let db = fresh_db(&spec, "createprof");
        let r = rt.block_on(brahma::brahma_console_create(&db, USER_A, Some(&json!(P2))));
        check("create_with_profile", &r, true);
    }
    {
        let db = fresh_db(&spec, "createbad");
        let r = rt.block_on(brahma::brahma_console_create(
            &db,
            USER_A,
            Some(&json!(MISSING)),
        ));
        check("create_bad_profile", &r, false);
    }

    // --- Rename / set-model / delete ---
    {
        let db = fresh_db(&spec, "rename");
        let r = rt.block_on(brahma::brahma_console_rename(
            &db,
            USER_A,
            CHAT_B,
            &json!("Renamed Console"),
        ));
        check("rename", &r, false);
    }
    {
        let db = fresh_db(&spec, "setmodel");
        let r = rt.block_on(brahma::brahma_console_set_model(
            &db,
            USER_A,
            CHAT_A,
            &json!(P2),
        ));
        check("set_model", &r, false);
    }
    {
        let db = fresh_db(&spec, "setmodelbad");
        let r = rt.block_on(brahma::brahma_console_set_model(
            &db,
            USER_A,
            CHAT_A,
            &json!(MISSING),
        ));
        check("set_model_bad_profile", &r, false);
    }
    {
        let db = fresh_db(&spec, "delete");
        let r = rt.block_on(brahma::brahma_console_delete(&db, USER_A, CHAT_C));
        check("delete", &r, false);
    }

    // --- P4.60: the create / rename / set-model bodies ---
    {
        let db = fresh_db(&spec, "createwrongtype");
        let r = rt.block_on(brahma::brahma_console_create(&db, USER_A, Some(&json!(7))));
        check("create_profile_wrong_type", &r, false);
    }
    {
        let db = fresh_db(&spec, "createempty");
        let r = rt.block_on(brahma::brahma_console_create(&db, USER_A, Some(&json!(""))));
        check("create_profile_empty", &r, false);
    }
    {
        let db = fresh_db(&spec, "createnull");
        let r = rt.block_on(brahma::brahma_console_create(
            &db,
            USER_A,
            Some(&Value::Null),
        ));
        check("create_profile_null", &r, false);
    }
    {
        let db = fresh_db(&spec, "renamewrongtype");
        let r = rt.block_on(brahma::brahma_console_rename(
            &db,
            USER_A,
            CHAT_B,
            &json!(5),
        ));
        check("rename_title_wrong_type", &r, false);
    }
    {
        let db = fresh_db(&spec, "renameempty");
        let r = rt.block_on(brahma::brahma_console_rename(
            &db,
            USER_A,
            CHAT_B,
            &json!(""),
        ));
        check("rename_title_empty", &r, false);
    }
    {
        // The guard-ORDER arm: the verify runs before the schema, so a bad body
        // on a chat that does not exist is a 404.
        let db = fresh_db(&spec, "renamemissing");
        let r = rt.block_on(brahma::brahma_console_rename(
            &db,
            USER_A,
            MISSING,
            &json!(5),
        ));
        check("rename_missing_chat_bad_body", &r, false);
    }
    {
        let db = fresh_db(&spec, "setmodelwrongtype");
        let r = rt.block_on(brahma::brahma_console_set_model(
            &db,
            USER_A,
            CHAT_A,
            &json!(5),
        ));
        check("set_model_profile_wrong_type", &r, false);
    }
    {
        let db = fresh_db(&spec, "setmodelnotuuid");
        let r = rt.block_on(brahma::brahma_console_set_model(
            &db,
            USER_A,
            CHAT_A,
            &json!("nope"),
        ));
        check("set_model_profile_not_uuid", &r, false);
    }

    // --- P4.60: the send body, through v4's own guard ORDER ---
    // `brahma_send_prepare` is what the dispatch arm runs: `verifyBrahmaChat`
    // FIRST, `sendMessageSchema` second. The two 404 arms below carry a body
    // that would ALSO have failed — they are the proof that the order holds.
    {
        let sends: Vec<(&str, &str, Value)> = vec![
            ("send_content_wrong_type", CHAT_A, json!({ "content": 123 })),
            ("send_content_empty", CHAT_A, json!({ "content": "" })),
            ("send_content_missing", CHAT_A, json!({})),
            (
                "send_file_ids_string",
                CHAT_A,
                json!({ "content": "hi", "fileIds": "x" }),
            ),
            (
                "send_file_ids_bad_uuid",
                CHAT_A,
                json!({ "content": "hi", "fileIds": ["not-a-uuid"] }),
            ),
            (
                "send_file_ids_element_number",
                CHAT_A,
                json!({ "content": "hi", "fileIds": [1] }),
            ),
            (
                "send_file_ids_null",
                CHAT_A,
                json!({ "content": "hi", "fileIds": null }),
            ),
            (
                "send_missing_chat_bad_body",
                MISSING,
                json!({ "content": "" }),
            ),
            (
                "send_salon_chat_bad_body",
                CHAT_SALON,
                json!({ "content": 123 }),
            ),
        ];
        for (name, chat, body) in &sends {
            let db = fresh_db(&spec, name);
            let resp = brahma::brahma_send_prepare(
                &db,
                chat,
                USER_A,
                body.get("content").unwrap_or(&Value::Null),
                body.get("fileIds"),
            )
            .err()
            .unwrap_or_else(|| {
                panic!("case '{name}': the prepare PASSED — every arm here is a refusal")
            });
            check(name, &resp, false);
        }
    }

    // Declared on BOTH sides: a case added to the oracle and forgotten here
    // would otherwise pass silently on a smaller set.
    assert_eq!(
        checked,
        oracle.len(),
        "the Rust case list and the oracle disagree: {checked} checked vs {} recorded",
        oracle.len()
    );
    assert!(
        failed.is_empty(),
        "brahma-console-routes FAILED: {failed:?}"
    );
}
