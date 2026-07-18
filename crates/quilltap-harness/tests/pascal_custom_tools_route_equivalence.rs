//! Route differential (P4.6ay unit 7): the custom-tools route
//! (`api::custom_tools::{chat_custom_tools_list, chat_custom_tool_run}`) vs v4's
//! REAL route handlers, over the committed `pascal-run-custom-{main,mount}.db`
//! fixture. GET diffs the merged-per-perspective roster body; POST diffs the
//! `{status, body}` + the posted `chat_messages` system rows (minted id +
//! createdAt normalized positionally). Rolls are `min === max` (deterministic).
//!
//! Generate the oracle (v4 @ d68638b4, Node 24 — mirror to /tmp; jest ignores
//! `.claude/`):
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-pascal-mirror; mkdir -p $M/cases $M/fixtures
//!   cp <V5W>/harness/oracle/cases/pascal-custom-tools-route.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
//!   QT_FIXTURE_PASCAL_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
//!   QT_FIXTURE_PASCAL_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-custom-tools-route.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-custom-tools-route
//! Run:
//!   QT_ORACLE_PASCAL_CUSTOM_TOOLS_ROUTE=/tmp/oracle-pascal-custom-tools-route.ndjson \
//!     cargo test -p quilltap-harness --test pascal_custom_tools_route_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::custom_tools::{chat_custom_tool_run, chat_custom_tools_list};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Map, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn open(case: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-pascal-route-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

/// Map a v5 `Response` into the `{status, body}` the oracle records.
fn status_body(resp: Response) -> (u16, Value) {
    match resp {
        Response::CustomToolsList(v) | Response::CustomToolRun(v) => (200, v),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Forbidden => 403,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Locked => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Fold integral floats → ints; strip a minted uuid `id` + `createdAt` from any
/// message object (positional normalization for the posted rows).
fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, x) in o {
                // `id` (uuid) + `createdAt` (timestamp) are minted per run.
                if (k == "id" || k == "createdAt") && looks_minted(x) {
                    m.insert(k.clone(), Value::String(format!("<{k}>")));
                } else {
                    m.insert(k.clone(), canon(x));
                }
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

/// A minted `id`/`createdAt` is a non-empty string; the fixture's stable vault
/// ids never appear as an `id`/`createdAt` key on a message object.
fn looks_minted(v: &Value) -> bool {
    v.as_str().map(|s| !s.is_empty()).unwrap_or(false)
}

/// The posted system rows (pascal / prospero), reduced + mint-normalized.
fn system_rows(db: &Db) -> Vec<Value> {
    let msgs = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
        .unwrap();
    let rows: Vec<Value> = msgs
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.get("systemSender").map(|s| !s.is_null()).unwrap_or(false))
        .collect();
    let parse_json = |v: Value| -> Value {
        match v {
            Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
            other => other,
        }
    };
    let mut out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let get = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            json!({
                "role": get("role"),
                "content": get("content"),
                "opaqueContent": get("opaqueContent"),
                "participantId": get("participantId"),
                "systemSender": get("systemSender"),
                "systemKind": get("systemKind"),
                "targetParticipantIds": parse_json(get("targetParticipantIds")),
                "pascalMeta": parse_json(get("pascalMeta")),
            })
        })
        .collect();
    out.sort_by_key(|r| {
        r.get("systemKind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });
    out
}

fn oracle_system_rows(row: &Value) -> Vec<Value> {
    row.get("systemRows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let get = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            let parse_json = |v: Value| -> Value {
                match v {
                    Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
                    other => other,
                }
            };
            json!({
                "role": get("role"),
                "content": get("content"),
                "opaqueContent": get("opaqueContent"),
                "participantId": get("participantId"),
                "systemSender": get("systemSender"),
                "systemKind": get("systemKind"),
                "targetParticipantIds": parse_json(get("targetParticipantIds")),
                "pascalMeta": parse_json(get("pascalMeta")),
            })
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn custom_tools_route_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_CUSTOM_TOOLS_ROUTE") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_CUSTOM_TOOLS_ROUTE (see header).");
        return;
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    // (name, POST body or None for GET)
    let cases: Vec<(&str, Option<Value>)> = vec![
        ("list", None),
        (
            "run-coin-as-a",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
        ),
        (
            "run-ansible-hit",
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
        ),
        (
            "run-ansible-miss",
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
        ),
        ("run-no-character", Some(json!({ "tool": "coin" }))),
        (
            "run-private",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "private": true }),
            ),
        ),
        (
            "run-unknown-tool",
            Some(
                json!({ "tool": "nope", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
        ),
        (
            "run-unknown-character",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-0000000000ff" }),
            ),
        ),
        (
            "run-error",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "parameters": { "bad": 1 } }),
            ),
        ),
    ];

    let mut checked = 0usize;
    for (name, body) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        let db = open(name);

        let (status, resp_body, sys) = match body {
            None => {
                let (s, b) = status_body(chat_custom_tools_list(&db, USER, CHAT));
                (s, b, Vec::new())
            }
            Some(b) => {
                let tool = b["tool"].as_str().unwrap().to_string();
                let parameters = b.get("parameters").and_then(Value::as_object).cloned();
                let private = b.get("private").and_then(Value::as_bool);
                let as_character_id = b
                    .get("asCharacterId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let (s, bd) = status_body(
                    chat_custom_tool_run(
                        &db,
                        USER,
                        CHAT,
                        &tool,
                        parameters,
                        private,
                        as_character_id,
                    )
                    .await,
                );
                let rows = system_rows(&db);
                (s, bd, rows)
            }
        };

        assert_eq!(
            status as u64,
            want["status"].as_u64().unwrap(),
            "case '{name}' status"
        );
        assert_eq!(
            canon(&resp_body),
            canon(&want["body"]),
            "case '{name}' body"
        );
        assert_eq!(
            canon(&Value::Array(sys)),
            canon(&Value::Array(oracle_system_rows(want))),
            "case '{name}' system rows"
        );
        checked += 1;
    }

    assert_eq!(checked, cases.len());
    eprintln!("OK: custom-tools route matched oracle ({checked} cases).");
}
