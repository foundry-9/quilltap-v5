//! P4.9I2A HELP-CHATS route-surface differential: `api::help_chats::*` vs v4's
//! REAL `app/api/v1/help-chats/**` route handlers, both over a FRESH copy of the
//! committed `help-chat-{main,mount}.db` fixture per case (baked ids identical →
//! no remap; the create/rename/update-context mutations mint ids + stamps,
//! blanked by the harness).
//!
//! Arms: list for three users (eleven help chats sorted `updatedAt` desc with
//! the salon + brahma chats filtered out; B's one; C's none); eligibility's
//! four arms (A eligible; B help chars but no tool-capable profile; C no help
//! chars) incl. the avatar fallthrough over the linked files; create — the
//! two-character happy path with its six-null echo, the help-disabled-first
//! order, the none-help-enabled 400, **the guard ORDER** (a missing id FIRST
//! answers 404 before the help check; a missing id AFTER a valid one also
//! 404s), and the Zod arms (empty ids, non-uuid, ids wrong type, pageUrl
//! missing / wrong type, empty body); get (incl. NULL `helpPageUrl`, the salon
//! plus the missing 404s, and **v4's userId-less verify** — user B reads A's chat);
//! rename + update-context with verify-then-parse (a bad body on a missing /
//! salon chat is a 404); the envelope 400 on both `?action=` routes; delete;
//! messages; and the send prologue's refusal arms in v4's order. The SYSTEM rows
//! create and update-context write are compared as `messagesAfter` — the chat's
//! `getMessages` projected to `[role, content]` in ROWID order (memory note
//! `a-sorted-dump-cannot-see-position`).
//!
//! Generate the oracle (Node 24, from the v4 checkout — mirror to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   mkdir -p /tmp/help-chats-routes/cases /tmp/help-chats-routes/fixtures
//!   cp $V5W/harness/oracle/cases/help-chats-routes.test.ts /tmp/help-chats-routes/cases/
//!   cp $V5W/harness/oracle/fixtures/help-chat-web.json /tmp/help-chats-routes/fixtures/
//!   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
//!   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-help-chats-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots /tmp/help-chats-routes/cases -- help-chats-routes
//! Run:
//!   QT_ORACLE_HELP_CHATS_ROUTES=/tmp/oracle-help-chats-routes.ndjson \
//!     cargo test -p quilltap-harness --test help_chats_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::help_chats;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::chats_messages_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const USER_A: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const USER_B: &str = "f28e15cd-74f9-4640-9b96-82ac8b619961";
const USER_C: &str = "a38e25de-85fa-4751-ac07-93bd9c72a072";
const C1: &str = "b0000002-0000-4000-8000-000000000001";
const C2: &str = "b0000002-0000-4000-8000-000000000002";
const C3: &str = "b0000002-0000-4000-8000-000000000003";
const H1: &str = "c1000002-0000-4000-8000-000000000001";
const H2: &str = "c1000002-0000-4000-8000-000000000002";
const H3: &str = "c1000002-0000-4000-8000-000000000003";
const SALON: &str = "c1000002-0000-4000-8000-000000000031";
const MISSING: &str = "00000000-0000-4000-8000-0000000000ff";

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
/// Blank the minted / bumped volatile keys wherever they appear (the create's
/// chat + participant ids, every create/update stamp).
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
/// arm whose REST edge answers 201 (v4 `created`); every other `HelpChat` body is 200.
fn status_body(r: &Response, created: bool) -> (u16, Value) {
    match r {
        Response::HelpChat(v) => (if created { 201 } else { 200 }, v.clone()),
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

/// v4's middleware answers an uncaught `ZodError` with `{error: 'Validation
/// error', details: [...]}`; `details` is the standing project-wide deferral,
/// so v5 carries the sentence alone — after PINNING that the sentence IS
/// `Validation error`.
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
        "case '{name}': a ZodError body whose top-level sentence is NOT 'Validation error'"
    );
    json!({ "error": "Validation error" })
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!(
        "qt-help-chats-routes-{}-{}",
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

/// `getMessages(chatId)` projected to `[role, content]` in ROWID order.
fn messages_after(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let rows = db
        .read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .unwrap();
    Value::Array(
        rows.iter()
            .map(|m| {
                let role = m
                    .get("role")
                    .or_else(|| m.get("type"))
                    .cloned()
                    .unwrap_or(Value::Null);
                json!([
                    role,
                    m.get("content")
                        .cloned()
                        .unwrap_or(Value::String(String::new()))
                ])
            })
            .collect(),
    )
}

#[test]
fn help_chats_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_CHATS_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_CHATS_ROUTES (see test header).");
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
    let mut check = |name: &str, resp: &Response, created: bool, after: Option<Value>| {
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
        // `messagesAfter`: present on both sides or neither.
        let want_after = want.get("messagesAfter");
        match (after, want_after) {
            (None, None) => {}
            (Some(g), Some(w)) => {
                if norm(&g) != norm(w) {
                    eprintln!(
                        "[{name}] MESSAGES-AFTER MISMATCH:\n{}",
                        first_diff(&norm(&g), &norm(w))
                    );
                    failed.push(format!("{name}_messages_after"));
                }
            }
            (g, w) => {
                eprintln!(
                    "[{name}] messagesAfter presence differs: rust {} / oracle {}",
                    g.is_some(),
                    w.is_some()
                );
                failed.push(format!("{name}_messages_after_presence"));
            }
        }
    };

    // --- list / eligibility ---
    for (name, user) in [
        ("list_user_a", USER_A),
        ("list_user_b", USER_B),
        ("list_user_c_empty", USER_C),
        ("list_empty_action_lists", USER_A),
    ] {
        let db = fresh_db(&spec, name);
        check(name, &help_chats::help_chat_list(&db, user), false, None);
    }
    for (name, user) in [
        ("eligibility_user_a", USER_A),
        ("eligibility_user_b_no_tool_capable", USER_B),
        ("eligibility_user_c_no_help_chars", USER_C),
    ] {
        let db = fresh_db(&spec, name);
        check(
            name,
            &help_chats::help_chat_eligibility(&db, user),
            false,
            None,
        );
    }
    // The envelope-shaped 400 is the WEB EDGE's (v4 `badRequest(...)` from the
    // route module, before any handler) — reproduce the edge's exact response.
    check(
        "get_unknown_action_400",
        &Response::error(
            ErrorKind::BadRequest,
            "Unknown action: bogus. Available actions: eligibility",
        ),
        false,
        None,
    );

    // --- create ---
    {
        let db = fresh_db(&spec, "create2");
        let r = rt.block_on(help_chats::help_chat_create(
            &db,
            USER_A,
            &json!([C1, C2]),
            &json!("/salon/new-1"),
        ));
        let id = match &r {
            Response::HelpChat(v) => v["chat"]["id"].as_str().map(str::to_string),
            _ => None,
        };
        let after = id.map(|id| messages_after(&db, &id));
        check("create_two_characters", &r, true, after);
    }
    {
        let db = fresh_db(&spec, "createorder");
        let r = rt.block_on(help_chats::help_chat_create(
            &db,
            USER_A,
            &json!([C3, C1]),
            &json!(""),
        ));
        let id = match &r {
            Response::HelpChat(v) => v["chat"]["id"].as_str().map(str::to_string),
            _ => None,
        };
        let after = id.map(|id| messages_after(&db, &id));
        check("create_help_disabled_first_then_enabled", &r, true, after);
    }
    let creates: Vec<(&str, Value, Value)> = vec![
        ("create_none_help_enabled_400", json!([C3]), json!("/x")),
        (
            "create_first_missing_404_before_help_check",
            json!([MISSING, C1]),
            json!("/x"),
        ),
        (
            "create_missing_after_valid_404",
            json!([C1, MISSING]),
            json!("/x"),
        ),
        // The order-measuring row (a missing id + a help-DISABLED partner).
        (
            "create_missing_and_help_disabled_404_not_400",
            json!([MISSING, C3]),
            json!("/x"),
        ),
        ("create_empty_ids_400", json!([]), json!("/x")),
        ("create_non_uuid_400", json!(["nope"]), json!("/x")),
        ("create_ids_wrong_type_400", json!(C1), json!("/x")),
        ("create_page_url_missing_400", json!([C1]), Value::Null),
        ("create_page_url_wrong_type_400", json!([C1]), json!(7)),
        ("create_body_empty_400", Value::Null, Value::Null),
    ];
    for (name, ids, url) in &creates {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(help_chats::help_chat_create(&db, USER_A, ids, url));
        check(name, &r, false, None);
    }

    // --- get ---
    for (name, chat) in [
        ("get_h1", H1),
        ("get_h2_null_page_url", H2),
        ("get_salon_404", SALON),
        ("get_missing_404", MISSING),
        ("get_other_user_still_200", H1),
    ] {
        let db = fresh_db(&spec, name);
        check(name, &help_chats::help_chat_get(&db, chat), false, None);
    }

    // --- rename ---
    let renames: Vec<(&str, &str, Value)> = vec![
        ("rename", H2, json!("Renamed Help")),
        ("rename_empty_title_400", H2, json!("")),
        ("rename_wrong_type_400", H2, json!(5)),
        ("rename_missing_chat_bad_body_404", MISSING, json!(5)),
        ("rename_salon_bad_body_404", SALON, json!("")),
        ("patch_empty_action_renames", H3, json!("Via empty action")),
    ];
    for (name, chat, title) in &renames {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(help_chats::help_chat_rename(&db, chat, title));
        check(name, &r, false, None);
    }

    // --- update-context ---
    {
        let db = fresh_db(&spec, "updctx");
        let r = rt.block_on(help_chats::help_chat_update_context(
            &db,
            H2,
            &json!("/files"),
        ));
        let after = messages_after(&db, H2);
        check("update_context", &r, false, Some(after));
    }
    let updates: Vec<(&str, &str, Value)> = vec![
        ("update_context_empty_400", H2, json!("")),
        ("update_context_wrong_type_400", H2, json!(1)),
        ("update_context_missing_chat_404", MISSING, json!("")),
    ];
    for (name, chat, url) in &updates {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(help_chats::help_chat_update_context(&db, chat, url));
        check(name, &r, false, None);
    }
    check(
        "patch_unknown_action_400",
        &Response::error(
            ErrorKind::BadRequest,
            "Unknown action: bogus. Available actions: update-context",
        ),
        false,
        None,
    );

    // --- delete ---
    for (name, chat) in [
        ("delete_h3", H3),
        ("delete_missing_404", MISSING),
        ("delete_salon_404", SALON),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(help_chats::help_chat_delete(&db, chat));
        check(name, &r, false, None);
    }

    // --- messages ---
    for (name, chat) in [("messages_h1", H1), ("messages_salon_404", SALON)] {
        let db = fresh_db(&spec, name);
        check(
            name,
            &help_chats::help_chat_messages(&db, chat),
            false,
            None,
        );
    }

    // --- the send prologue (verify FIRST, then the schema) — refusal arms only ---
    let sends: Vec<(&str, &str, Value)> = vec![
        ("send_content_wrong_type", H1, json!({ "content": 123 })),
        ("send_content_empty", H1, json!({ "content": "" })),
        ("send_content_missing", H1, json!({})),
        (
            "send_file_ids_string",
            H1,
            json!({ "content": "hi", "fileIds": "x" }),
        ),
        (
            "send_file_ids_bad_uuid",
            H1,
            json!({ "content": "hi", "fileIds": ["not-a-uuid"] }),
        ),
        (
            "send_file_ids_null",
            H1,
            json!({ "content": "hi", "fileIds": null }),
        ),
        (
            "send_missing_chat_bad_body",
            MISSING,
            json!({ "content": "" }),
        ),
        ("send_salon_chat_bad_body", SALON, json!({ "content": 123 })),
    ];
    for (name, chat, body) in &sends {
        let db = fresh_db(&spec, name);
        let resp = help_chats::help_chat_send_prepare(
            &db,
            chat,
            body.get("content").unwrap_or(&Value::Null),
            body.get("fileIds"),
        )
        .err()
        .unwrap_or_else(|| {
            panic!("case '{name}': the prepare PASSED — every arm here is a refusal")
        });
        check(name, &resp, false, None);
    }

    assert_eq!(
        checked,
        oracle.len(),
        "the Rust case list and the oracle disagree: {checked} checked vs {} recorded",
        oracle.len()
    );
    assert!(failed.is_empty(), "help-chats-routes FAILED: {failed:?}");
}
