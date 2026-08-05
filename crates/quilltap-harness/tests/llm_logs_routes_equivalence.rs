//! P4.6ar LLM-LOGS route-surface differential: `api::llm_logs::*` (over the
//! `db::llm_logs` reads) vs v4's REAL route handlers. Both sides read a FRESH
//! copy of the committed `inspector-{main,mount,llm}.db` family per case. Success
//! cases diff the route body; error cases diff status + `{error}` message.
//!
//! The body diff is byte-meaningful: the log objects carry v4's exact key set —
//! a NULL column is ABSENT (v4's hydrate maps it to `undefined` and
//! `JSON.stringify` omits it), while a stored `null` inside a JSON column (e.g.
//! `response.error`) is PRESENT as `null`.
//!
//! `check` normalizes key ORDER before comparing (so a mismatch reports the
//! values, not the ordering), which would leave the schema-field-order claim
//! unproven on its own. [`check_key_order`] closes that: it compares the RAW key
//! sequence of the Rust body against the oracle's raw `JSON.stringify` bytes,
//! nested objects included. This works because quilltap-core builds `serde_json`
//! with `preserve_order` — `Value::Object` is an `IndexMap`, so `to_value` and
//! the REST edge's `to_string()` both carry the struct's declared field order
//! onto the wire.
//!
//! ⚠️ The env var is `QT_ORACLE_LLM_LOGS_ROUTES`, NOT `QT_ORACLE_LLM_LOGS` —
//! that shorter name belongs to `llm_logs_tier2_equivalence`. The two collided
//! until P4.D49, which meant a full-workspace gate carrying one family's env
//! block fed the OTHER family's NDJSON to this test; because both skip when
//! unset, the collision only ever surfaced when someone ran both at once.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-llm-logs.ndjson npx jest -- llm-logs-routes
//! Run:
//!   QT_ORACLE_LLM_LOGS_ROUTES=/tmp/oracle-llm-logs.ndjson \
//!     cargo test -p quilltap-harness --test llm_logs_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::llm_logs::{self, LlmLogsListParams};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
    character_id: String,
    chat_id: String,
    missing_id: String,
    missing_chat_id: String,
    missing_character_id: String,
    missing_message_id: String,
    messages: Vec<IdOnly>,
    logs: Vec<IdOnly>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/inspector-web.json")
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

/// A fresh THREE-partition Db over a private copy of the committed fixture — the
/// llm-logs sibling is the point of this surface.
fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-insp-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm = scratch.join("llm.db");
    std::fs::copy(fixtures_dir().join("inspector-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("inspector-mount.db"), &mount).unwrap();
    std::fs::copy(fixtures_dir().join("inspector-llm.db"), &llm).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
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
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}
fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::LlmLog(v) => Some(v.clone()),
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

/// Every object's key sequence, depth-first — the shape `check`'s sorted compare
/// deliberately throws away.
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
/// `JSON.stringify` bytes exactly, not merely carry the same key SET. Reads the
/// oracle's RAW NDJSON line (parsing preserves insertion order under
/// `preserve_order`, so the recorded order survives) and diffs it against the
/// port's. This is what pins `LlmLogRow`'s fields to `LLMLogSchema` order.
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

/// The `extra.afterStatus` claim on the two DELETE cases: the row is really gone
/// (the ack alone would pass vacuously against a no-op delete).
fn check_after_status(
    oracle: &HashMap<String, Value>,
    name: &str,
    resp: &Response,
    failed: &mut Vec<String>,
) {
    let want = oracle[name]["extra"]["afterStatus"].as_u64().unwrap() as u16;
    let got = match resp {
        Response::Error(e) => status_of(e.kind),
        _ => 200,
    };
    if got != want {
        eprintln!("[{name}] afterStatus MISMATCH: got {got} / want {want}");
        failed.push(format!("{name}:afterStatus"));
    } else {
        eprintln!("[{name}] afterStatus OK ({got}).");
    }
}

#[test]
fn llm_logs_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_LLM_LOGS_ROUTES") else {
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
    let other_log_id = &spec.logs[spec.logs.len() - 1].id;

    // A list case: the params exactly as the REST edge would resolve them.
    let list = |name: &str, p: LlmLogsListParams<'_>, failed: &mut Vec<String>| {
        let db = fresh_db(name);
        let resp = llm_logs::llm_logs_list(&db, &spec.user_id, &p);
        check(&oracle, name, &resp, failed);
    };

    // ── The default (recent) branch + the pagination arithmetic ──
    // This one also carries the key-order assertion across the whole envelope and
    // every log in it — including the bare rows, where the absent columns are.
    {
        let db = fresh_db("list_recent_default");
        let resp = llm_logs::llm_logs_list(&db, &spec.user_id, &LlmLogsListParams::default());
        check(&oracle, "list_recent_default", &resp, &mut failed);
        check_key_order(&oracle_raw, "list_recent_default", &resp, &mut failed);
    }
    list(
        "list_recent_limit_2",
        LlmLogsListParams {
            limit: Some("2"),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_recent_limit_above_cap",
        LlmLogsListParams {
            limit: Some("999"),
            ..Default::default()
        },
        &mut failed,
    );
    // The NaN quirk: everything comes back, and `limit` rides the wire as null.
    list(
        "list_recent_garbage_limit",
        LlmLogsListParams {
            limit: Some("garbage"),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_recent_zero_limit",
        LlmLogsListParams {
            limit: Some("0"),
            ..Default::default()
        },
        &mut failed,
    );
    // slice(0, -3) drops from the TAIL.
    list(
        "list_recent_negative_limit",
        LlmLogsListParams {
            limit: Some("-3"),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_recent_offset",
        LlmLogsListParams {
            offset: Some("3"),
            ..Default::default()
        },
        &mut failed,
    );
    // NB `total` here is 3, not 13: the repo's own LIMIT lands BEFORE the route
    // reads `logs.length` for `total`. v4's "total" is the fetched page's size on
    // any repo-limited branch, not the collection's size. Faithful.
    list(
        "list_recent_offset_and_limit",
        LlmLogsListParams {
            offset: Some("2"),
            limit: Some("3"),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_recent_garbage_offset",
        LlmLogsListParams {
            offset: Some("nope"),
            ..Default::default()
        },
        &mut failed,
    );

    // ── The entity branches ──
    list(
        "list_by_message",
        LlmLogsListParams {
            message_id: Some(&spec.messages[1].id),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_message_unknown",
        LlmLogsListParams {
            message_id: Some(&spec.missing_message_id),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_chat",
        LlmLogsListParams {
            chat_id: Some(&spec.chat_id),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_chat_include_messages",
        LlmLogsListParams {
            chat_id: Some(&spec.chat_id),
            include_messages: true,
            ..Default::default()
        },
        &mut failed,
    );
    // `includeMessages=TRUE` is not the exact string, so the edge resolves it to
    // false — driven as false here; the edge's own compare is its concern.
    list(
        "list_by_chat_include_messages_not_exact",
        LlmLogsListParams {
            chat_id: Some(&spec.chat_id),
            include_messages: false,
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_chat_missing",
        LlmLogsListParams {
            chat_id: Some(&spec.missing_chat_id),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_character",
        LlmLogsListParams {
            character_id: Some(&spec.character_id),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_character_missing",
        LlmLogsListParams {
            character_id: Some(&spec.missing_character_id),
            ..Default::default()
        },
        &mut failed,
    );
    // Branch precedence: messageId wins, and the missing chat/character ids never
    // reach their 404 checks.
    list(
        "list_branch_precedence_message_wins",
        LlmLogsListParams {
            message_id: Some(&spec.messages[1].id),
            chat_id: Some(&spec.missing_chat_id),
            character_id: Some(&spec.missing_character_id),
            ..Default::default()
        },
        &mut failed,
    );

    // ── standalone / type ──
    // Expected EMPTY — v4's `$eq: null` lowers to `col = NULL` (see
    // `db::llm_logs::find_standalone`). The corpus has five standalone rows.
    list(
        "list_standalone",
        LlmLogsListParams {
            standalone: true,
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_type",
        LlmLogsListParams {
            log_type: Some("CHAT_MESSAGE"),
            ..Default::default()
        },
        &mut failed,
    );
    list(
        "list_by_type_unknown",
        LlmLogsListParams {
            log_type: Some("NOT_A_REAL_TYPE"),
            ..Default::default()
        },
        &mut failed,
    );

    // ── The item routes ──
    {
        let resp = llm_logs::llm_log_get(&fresh_db("item_get_found"), &spec.logs[0].id);
        check(&oracle, "item_get_found", &resp, &mut failed);
        // The wire-order claim, on the richest single log (every optional column
        // populated + both nested summaries).
        check_key_order(&oracle_raw, "item_get_found", &resp, &mut failed);
    }
    check(
        &oracle,
        "item_get_missing",
        &llm_logs::llm_log_get(&fresh_db("item_get_missing"), &spec.missing_id),
        &mut failed,
    );
    // NO OWNERSHIP CHECK: the session user reads a log owned by someone else.
    check(
        &oracle,
        "item_get_other_user",
        &llm_logs::llm_log_get(&fresh_db("item_get_other"), other_log_id),
        &mut failed,
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for (name, id) in [
        ("item_delete_found", &spec.logs[0].id),
        ("item_delete_other_user", other_log_id),
    ] {
        let db = fresh_db(name);
        let resp = rt.block_on(llm_logs::llm_log_delete(&db, id));
        check(&oracle, name, &resp, &mut failed);
        // The row is really gone, not just acked.
        let after = llm_logs::llm_log_get(&db, id);
        check_after_status(&oracle, name, &after, &mut failed);
    }
    {
        let db = fresh_db("item_delete_missing");
        let resp = rt.block_on(llm_logs::llm_log_delete(&db, &spec.missing_id));
        check(&oracle, "item_delete_missing", &resp, &mut failed);
    }

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
