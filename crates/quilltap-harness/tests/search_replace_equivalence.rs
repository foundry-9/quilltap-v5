//! P4.9E3B search-replace differential: `services::search_replace` vs v4's
//! REAL `POST /api/v1/search-replace` route over a FRESH copy of the committed
//! `chat-dialogs-{main,mount}.db` fixture per case. Response body + the
//! post-mutation dumps (SR_CHAT chat row + messages, every memory) are diffed.
//!
//! ## Minted values
//!
//! The execute path re-writes matched memories, minting `updatedAt` (twice —
//! `replaceInMemories` + the post-replace rewrite). The differential compares
//! WHICH rows were touched (the `updatedAt != seed` boolean must agree), then
//! asserts v4's stamp is at-or-after the frozen base and v5's differs from the
//! seed, and only then strips the key — the difference is proven, not
//! normalized away. Message rows and the chat row carry no minted values.
//!
//! ## Two RECORDED-ONLY v4 arms (no v5 counterpart, deliberately)
//!
//! `unknown_action` / `missing_action` are v4 `withActionDispatch` middleware
//! furniture — v5 carries no action string (the §1 verbs ARE the action
//! selection, and no REST edge exists, matching the message-op precedent).
//! Their oracle rows are asserted by shape so upstream copy drift is caught.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-search-replace.ndjson npx jest -- chat-dialogs-search-replace
//! Run:
//!   QT_ORACLE_SEARCH_REPLACE=/tmp/oracle-search-replace.ndjson \
//!     cargo test -p quilltap-harness --test search_replace_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::search_replace::{search_replace, Action};
use serde::Deserialize;
use serde_json::{json, Value};

const NORA: &str = "a2000000-0000-4000-8000-000000000001";
const SR_CHAT: &str = "c2000000-0000-4000-8000-000000000003";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";
/// The oracle's ticking frozen-clock base.
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-dialogs-web.json")
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sr-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
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

fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatDialog(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Internal => 500,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The oracle's `readTables` mirror: SR_CHAT chat row + messages + every memory
/// reduced to the same columns.
fn dump_tables(db: &Db) -> Value {
    let chat = db
        .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, SR_CHAT))
        .unwrap()
        .unwrap_or(Value::Null);
    let messages = Value::Array(
        db.read_main(|c| quilltap_core::db::chats_messages_read::get_messages(c, SR_CHAT))
            .unwrap(),
    );
    let rows = db
        .read_main(quilltap_core::db::memories_read::find_all)
        .unwrap();
    let mut memories: Vec<Value> = rows
        .into_iter()
        .map(|m| {
            json!({
                "id": m.get("id").cloned().unwrap_or(Value::Null),
                "characterId": m.get("characterId").cloned().unwrap_or(Value::Null),
                "chatId": m.get("chatId").cloned().unwrap_or(Value::Null),
                "sourceMessageId": m.get("sourceMessageId").cloned().unwrap_or(Value::Null),
                "content": m.get("content").cloned().unwrap_or(Value::Null),
                "summary": m.get("summary").cloned().unwrap_or(Value::Null),
                "keywords": m.get("keywords").cloned().unwrap_or(Value::Null),
                "embedding": m.get("embedding").cloned().unwrap_or(Value::Null),
                "updatedAt": m.get("updatedAt").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    memories.sort_by_key(|v| {
        v.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });
    json!({ "chat": chat, "messages": messages, "memories": Value::Array(memories) })
}

/// Prove-then-strip the minted memory `updatedAt`s: the touched-row SETS must
/// agree, v4's stamps sit at-or-after the frozen base, v5's differ from the
/// seed. Returns failure labels.
fn reconcile_memory_clocks(name: &str, got: &mut Value, want: &mut Value) -> Vec<String> {
    let mut failed = Vec::new();
    let (Some(g), Some(w)) = (
        got.get_mut("memories").and_then(Value::as_array_mut),
        want.get_mut("memories").and_then(Value::as_array_mut),
    ) else {
        return vec![format!("{name}_memories_shape")];
    };
    if g.len() != w.len() {
        return vec![format!("{name}_memories_len")];
    }
    for (gm, wm) in g.iter_mut().zip(w.iter_mut()) {
        let gid = gm.get("id").and_then(Value::as_str).unwrap_or("");
        let gu = gm
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let wu = wm
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let g_touched = gu != SEED_ISO;
        let w_touched = wu != SEED_ISO;
        if g_touched != w_touched {
            eprintln!(
                "[{name}] memory {gid}: touched-set drift (v5 touched={g_touched}, v4 touched={w_touched})"
            );
            failed.push(format!("{name}_{gid}_touched"));
        }
        if w_touched && wu.as_str() < FROZEN_NOW_ISO {
            eprintln!("[{name}] memory {gid}: v4 stamp {wu:?} before the frozen base");
            failed.push(format!("{name}_{gid}_v4_clock"));
        }
        if g_touched && gu.is_empty() {
            failed.push(format!("{name}_{gid}_v5_clock"));
        }
        gm.as_object_mut().unwrap().remove("updatedAt");
        wm.as_object_mut().unwrap().remove("updatedAt");
    }
    failed
}

#[test]
fn search_replace_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SEARCH_REPLACE") else {
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
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    let chat_scope = json!({ "type": "chat", "chatId": SR_CHAT });
    let char_scope = json!({ "type": "character", "characterId": NORA });

    struct Case<'a> {
        name: &'a str,
        action: Action,
        scope: Value,
        search: &'a str,
        replace: &'a str,
        include_messages: Option<bool>,
        include_memories: Option<bool>,
        dump: bool,
    }
    let cases = [
        Case {
            name: "preview_chat_scope",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
        Case {
            name: "preview_character_scope",
            action: Action::Preview,
            scope: char_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
        Case {
            name: "preview_messages_only",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: Some(false),
            dump: false,
        },
        Case {
            name: "preview_memories_only",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: Some(false),
            include_memories: None,
            dump: false,
        },
        Case {
            name: "preview_no_match",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "gryphon",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
        Case {
            name: "preview_chat_missing",
            action: Action::Preview,
            scope: json!({ "type": "chat", "chatId": MISSING_ID }),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
        Case {
            name: "execute_chat_scope",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
        },
        Case {
            name: "execute_character_scope",
            action: Action::Execute,
            scope: char_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
        },
        Case {
            name: "execute_case_asymmetry",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "Lantern",
            replace: "Beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
        },
        Case {
            name: "execute_memories_only",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: Some(false),
            include_memories: None,
            dump: true,
        },
        Case {
            name: "execute_no_match",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "gryphon",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
        },
        Case {
            name: "execute_invalid_scope",
            action: Action::Execute,
            scope: json!({ "type": "project", "projectId": SR_CHAT }),
            search: "x",
            replace: "y",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
        Case {
            name: "preview_empty_search",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "",
            replace: "y",
            include_messages: None,
            include_memories: None,
            dump: false,
        },
    ];

    for c in cases {
        let db = fresh_db(&spec, c.name);
        let r = rt.block_on(search_replace(
            &db,
            &spec.user_id,
            c.action,
            &c.scope,
            c.search,
            c.replace,
            c.include_messages,
            c.include_memories,
        ));
        driven.insert(c.name.to_string());
        let Some(want) = oracle.get(c.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", c.name));
            continue;
        };
        let (status, body) = status_body(&r);
        if u64::from(status) != want["status"].as_u64().unwrap_or(0) {
            eprintln!("[{}] STATUS {status} != {}", c.name, want["status"]);
            failed.push(format!("{}_status", c.name));
        }
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{}] BODY MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(c.name.to_string());
        } else {
            eprintln!("[{}] body OK.", c.name);
        }
        if c.dump {
            let mut got_tables = dump_tables(&db);
            let mut want_tables = want["tables"].clone();
            failed.extend(reconcile_memory_clocks(
                c.name,
                &mut got_tables,
                &mut want_tables,
            ));
            if norm(&got_tables) != norm(&want_tables) {
                eprintln!(
                    "[{} tables] MISMATCH:\n{}",
                    c.name,
                    first_diff(&norm(&got_tables), &norm(&want_tables))
                );
                failed.push(format!("{}_tables", c.name));
            } else {
                eprintln!("[{} tables] OK.", c.name);
            }
        }
    }

    // The two RECORDED-ONLY middleware arms (module header): assert shape, no
    // v5 drive.
    for (name, want_status, want_error) in [
        ("unknown_action", 400, "Unknown action: rename"),
        (
            "missing_action",
            400,
            "Action parameter required: execute or preview",
        ),
    ] {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            continue;
        };
        let ok = want["status"].as_u64() == Some(want_status)
            && want["body"]["error"].as_str() == Some(want_error);
        if !ok {
            eprintln!(
                "[{name}] the recorded middleware arm no longer holds: {:?}",
                want
            );
            failed.push(format!("{name}_recorded"));
        } else {
            eprintln!("[{name}] recorded v4 arm OK (no v5 counterpart by design).");
        }
        if name == "unknown_action"
            && want["body"]["availableActions"] != json!(["execute", "preview"])
        {
            failed.push(format!("{name}_available_actions"));
        }
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "search-replace mismatches: {failed:?}");
}
