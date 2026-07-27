//! P4.9E3B per-message reattribute differential:
//! `services::message_reattribute` vs v4's REAL
//! `POST /api/v1/messages/[id]?action=reattribute` over a FRESH copy of the
//! committed `chat-dialogs-{main,mount}.db` fixture per case. Response body +
//! the post-mutation dumps (SR_CHAT chat row + messages + every memory) are
//! diffed; nothing on this path mints a wall-clock value (v4's `update({})`
//! PRESERVES the chat's `updatedAt`, and message rows carry none), so there is
//! no normalization at all.
//!
//! The `reattribute_bad_uuid` case carries v4's Zod `details` array, which
//! v5's error envelope does not model (the standing P4.6bb deferral) —
//! asserted in both directions, then stripped.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-message-reattribute.ndjson npx jest -- chat-dialogs-reattribute
//! Run:
//!   QT_ORACLE_MESSAGE_REATTRIBUTE=/tmp/oracle-message-reattribute.ndjson \
//!     cargo test -p quilltap-harness --test message_reattribute_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::message_reattribute::message_reattribute;
use serde::Deserialize;
use serde_json::{json, Value};

const SR_CHAT: &str = "c2000000-0000-4000-8000-000000000003";
const R1: &str = "d5000000-0000-4000-8000-000000000001";
const R4: &str = "d5000000-0000-4000-8000-000000000004";
const P3_PIP: &str = "e2000000-0000-4000-8000-000000000012";
const P3_VERA: &str = "e2000000-0000-4000-8000-000000000013";
const P_NORA_EXPORT: &str = "e2000000-0000-4000-8000-000000000001";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";

/// The bad-uuid case carries v4's Zod `details` (asserted, then stripped).
const VALIDATION_DETAILS_GAP: &[&str] = &["reattribute_bad_uuid"];

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
    let scratch = std::env::temp_dir().join(format!("qt-ra-{}-{}", tag, std::process::id()));
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
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The oracle's `readTables` mirror (memories reduced to the delete-visible
/// columns).
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

#[test]
fn message_reattribute_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_MESSAGE_REATTRIBUTE") else {
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

    for (name, message_id, participant, dump) in [
        ("reattribute_with_memories", R4, P3_PIP, true),
        ("reattribute_no_memories", R1, P3_VERA, true),
        ("reattribute_target_not_in_chat", R4, P_NORA_EXPORT, true),
        ("reattribute_message_missing", MISSING_ID, P3_PIP, false),
        ("reattribute_bad_uuid", R4, "not-a-uuid", false),
    ] {
        let db = fresh_db(&spec, name);
        let r = rt.block_on(message_reattribute(
            &db,
            &spec.user_id,
            message_id,
            participant,
        ));
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            continue;
        };
        let (status, body) = status_body(&r);
        if u64::from(status) != want["status"].as_u64().unwrap_or(0) {
            eprintln!("[{name}] STATUS {status} != {}", want["status"]);
            failed.push(format!("{name}_status"));
        }
        let mut want_body = want["body"].clone();
        if VALIDATION_DETAILS_GAP.contains(&name) {
            let v4_details_ok = want_body
                .get("details")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            let v5_has_details = body.get("details").is_some();
            if !v4_details_ok || v5_has_details {
                eprintln!(
                    "[{name}] the recorded validation-details gap no longer holds \
                     (v4 details present={v4_details_ok}, v5 details present={v5_has_details})"
                );
                failed.push(format!("{name}_details_gap"));
            }
            if let Some(o) = want_body.as_object_mut() {
                o.remove("details");
            }
        }
        if norm(&body) != norm(&want_body) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want_body))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }
        if dump {
            let got_tables = dump_tables(&db);
            let want_tables = want["tables"].clone();
            if norm(&got_tables) != norm(&want_tables) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&norm(&got_tables), &norm(&want_tables))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        }
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(
        failed.is_empty(),
        "message-reattribute mismatches: {failed:?}"
    );
}
