//! P4.6ak TEXT-REPLACEMENTS + CHAT-GET-BACKGROUND route-surface differential:
//! `api::text_replacements::*` + `api::chat_media::chat_get_background` vs v4's
//! REAL route handlers. Both sides read a FRESH copy of the committed
//! `text-replacements-{main,mount}.db` fixture per case. Success cases diff the
//! route body (minted id/createdAt/updatedAt blanked per the oracle's `blank`
//! list); error cases diff status + `{error}` message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-text-replacements.ndjson npx jest -- text-replacements-routes
//! Run:
//!   QT_ORACLE_TEXT_REPLACEMENTS=/tmp/oracle-text-replacements.ndjson \
//!     cargo test -p quilltap-harness --test text_replacements_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_media;
use quilltap_core::api::text_replacements;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
// Pinned ids (shared verbatim with the oracle).
const R1: &str = "1a000000-0000-4000-8000-0000000000a1"; // teh (ci, so0)
const R3: &str = "1a000000-0000-4000-8000-0000000000a3"; // dont (ci, so0)
const MISSING: &str = "99999999-9999-4999-8999-999999999999";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "chatWithBackgroundId")]
    chat_with_background_id: String,
    #[serde(rename = "chatNoBackgroundId")]
    chat_no_background_id: String,
    #[serde(rename = "chatMissingFileId")]
    chat_missing_file_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/text-replacements-web.json")
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
fn fresh_db(tag: &str, seq: usize) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-tr-{}-{}-{}", tag, seq, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("text-replacements-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("text-replacements-mount.db"), &mount).unwrap();
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
/// Replace the named keys (wherever they appear) with a placeholder — minted
/// ids/timestamps that legitimately differ between the two sides.
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
/// The success-body carried by the two P4.6ak Response variants (null for the
/// delete 204).
fn success_body(r: &Response) -> Option<Value> {
    match r {
        Response::TextReplacement(v) | Response::ChatBackground(v) => Some(v.clone()),
        _ => None,
    }
}

#[test]
fn text_replacements_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_TEXT_REPLACEMENTS") else {
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

    let check = |name: &str, resp: &Response, failed: &mut Vec<String>| {
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
    };

    // ── text-replacement rules ──
    check(
        "list",
        &text_replacements::list(&fresh_db("list", 0)),
        &mut failed,
    );
    check(
        "create",
        &rt.block_on(text_replacements::create(
            &fresh_db("create", 0),
            &json!({ "fromText": "recieve", "toText": "receive", "caseSensitive": false, "sortOrder": 5 }),
        )),
        &mut failed,
    );
    check(
        "create_conflict",
        &rt.block_on(text_replacements::create(
            &fresh_db("create_conflict", 0),
            &json!({ "fromText": "TEH", "toText": "the", "caseSensitive": false }),
        )),
        &mut failed,
    );
    check(
        "patch",
        &rt.block_on(text_replacements::update(
            &fresh_db("patch", 0),
            R1,
            &json!({ "toText": "THE", "enabled": false }),
        )),
        &mut failed,
    );
    check(
        "patch_missing",
        &rt.block_on(text_replacements::update(
            &fresh_db("patch_missing", 0),
            MISSING,
            &json!({ "toText": "x" }),
        )),
        &mut failed,
    );
    check(
        "patch_conflict",
        &rt.block_on(text_replacements::update(
            &fresh_db("patch_conflict", 0),
            R3,
            &json!({ "fromText": "adn", "caseSensitive": true }),
        )),
        &mut failed,
    );
    check(
        "delete",
        &rt.block_on(text_replacements::delete(&fresh_db("delete", 0), R1)),
        &mut failed,
    );
    check(
        "delete_missing",
        &rt.block_on(text_replacements::delete(
            &fresh_db("delete_missing", 0),
            MISSING,
        )),
        &mut failed,
    );

    let bulk_rules = json!({ "rules": [
        { "fromText": "alpha", "toText": "ALPHA", "sortOrder": 2 },
        { "fromText": "bravo", "toText": "BRAVO", "caseSensitive": true, "sortOrder": 0 },
        { "fromText": "charlie", "toText": "CHARLIE", "enabled": false, "sortOrder": 1 },
    ] });
    check(
        "bulk_replace",
        &rt.block_on(text_replacements::bulk_replace(
            &fresh_db("bulk", 0),
            &bulk_rules,
        )),
        &mut failed,
    );

    // bulk-replace, THEN list — proves the transactional replace + the resort.
    {
        let db = fresh_db("bulk_then_list", 0);
        let _ = rt.block_on(text_replacements::bulk_replace(
            &db,
            &json!({ "rules": [
                { "fromText": "alpha", "toText": "ALPHA", "sortOrder": 2 },
                { "fromText": "bravo", "toText": "BRAVO", "sortOrder": 0 },
                { "fromText": "charlie", "toText": "CHARLIE", "sortOrder": 1 },
            ] }),
        ));
        check("bulk_then_list", &text_replacements::list(&db), &mut failed);
    }

    // bulk-replace to EMPTY, then list.
    {
        let db = fresh_db("list_empty", 0);
        let _ = rt.block_on(text_replacements::bulk_replace(
            &db,
            &json!({ "rules": [] }),
        ));
        check("list_empty", &text_replacements::list(&db), &mut failed);
    }

    // ── chatGetBackground ──
    check(
        "bg_resolved",
        &chat_media::chat_get_background(
            &fresh_db("bg_resolved", 0),
            &spec.chat_with_background_id,
        ),
        &mut failed,
    );
    check(
        "bg_no_id",
        &chat_media::chat_get_background(&fresh_db("bg_no_id", 0), &spec.chat_no_background_id),
        &mut failed,
    );
    check(
        "bg_file_missing",
        &chat_media::chat_get_background(&fresh_db("bg_missing", 0), &spec.chat_missing_file_id),
        &mut failed,
    );
    check(
        "bg_chat_missing",
        &chat_media::chat_get_background(&fresh_db("bg_chat_missing", 0), MISSING),
        &mut failed,
    );

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
