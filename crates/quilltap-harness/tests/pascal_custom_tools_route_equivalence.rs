//! Route differential (P4.6ay unit 7): the custom-tools route
//! (`api::custom_tools::{chat_custom_tools_list, chat_custom_tool_run}`) vs v4's
//! REAL route handlers, over the committed `pascal-run-custom-{main,mount}.db`
//! fixture. GET diffs the merged-per-perspective roster body; POST diffs the
//! `{status, body}` + the posted `chat_messages` system rows (minted id +
//! createdAt normalized positionally). Rolls are `min === max` (deterministic).
//!
//! P4.6bd: the `run-oracle-consult-resolved` case is PROFILE-BEARING — both
//! sides insert the same connection profile through their real repos, the
//! consult RESOLVES through a recorded canned completion (the
//! `tier3-completion-oracle` keying), and the persisted `CUSTOM_TOOL_CONSULT`
//! `llm_logs` row is diffed alongside the posted system rows.
//!
//! Generate the oracle (v4 @ 616930db, Node 24 — mirror to /tmp; jest ignores
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

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::custom_tools::{chat_custom_tool_run, chat_custom_tools_list};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::connection_profiles::{CpCreate, CreateOptions};
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::pascal::llm_consult::ProviderConsultRunner;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn open(case: &str, profile: bool) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-pascal-route-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm_logs = scratch.join("llm-logs.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    // A fresh llm-logs partition per case (P4.6bd): the resolved consult's
    // CUSTOM_TOOL_CONSULT row is part of the diff.
    common::materialize_llm_logs(&llm_logs, PEPPER);
    if profile {
        insert_consult_profile(&main);
    }
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm_logs),
        },
        PEPPER,
    )
    .expect("open db")
}

/// The one profile a `profile: true` case inserts — field-for-field the
/// oracle's `CONSULT_PROFILE` (both sides go through their real repos).
fn insert_consult_profile(main: &std::path::Path) {
    let writer = Writer::open_writable(main, PEPPER).expect("open main for profile insert");
    writer
        .connection_profiles()
        .create(
            &CpCreate {
                user_id: USER.to_string(),
                name: "Consult Canned".into(),
                provider: "Anthropic".into(),
                transport: "api".into(),
                courier_delta_mode: false,
                api_key_id: None,
                base_url: None,
                model_name: "consult-canned-model".into(),
                parameters: json!({}),
                is_default: true,
                is_cheap: true,
                allow_web_search: false,
                use_native_web_search: false,
                allow_tool_use: false,
                pseudo_tool_mode: "auto".into(),
                model_class: None,
                max_context: None,
                max_tokens: None,
                is_dangerous_compatible: false,
                supports_image_upload: false,
                tags: Vec::new(),
                sort_index: 0.0,
                total_tokens: 0.0,
                total_prompt_tokens: 0.0,
                total_completion_tokens: 0.0,
                message_count: 0.0,
            },
            &CreateOptions {
                id: "cc000000-0000-4000-8000-000000000001".into(),
                created_at: "2026-03-01T00:00:00.000Z".into(),
                updated_at: "2026-03-01T00:00:00.000Z".into(),
            },
        )
        .expect("insert consult profile");
}

// The recorded canned-completion rows the oracle emits per case
// (`tier3-completion-oracle`: replaying EXACTLY what v4's mock answered turns
// any Rust prompt/selection/temperature divergence into a loud canned-miss).
#[derive(Deserialize)]
struct CannedMsg {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct CannedUsage {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}
#[derive(Deserialize)]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsg>,
    response: String,
    usage: CannedUsage,
}

/// Build the per-case canned provider from the oracle's recorded rows.
fn canned_provider(want: &Value) -> CannedCompletionProvider {
    let rows: Vec<CannedRow> = want
        .get("canned")
        .cloned()
        .map(|v| serde_json::from_value(v).expect("parse canned rows"))
        .unwrap_or_default();
    let mut provider = CannedCompletionProvider::new();
    for row in &rows {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "user" => CompletionRole::User,
                    "assistant" => CompletionRole::Assistant,
                    other => panic!("unexpected role {other}"),
                },
                content: m.content.clone(),
            })
            .collect();
        provider = provider.with_response(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            row.response.clone(),
            Some(CompletionUsage {
                prompt_tokens: row.usage.prompt_tokens,
                completion_tokens: row.usage.completion_tokens,
                total_tokens: row.usage.total_tokens,
            }),
        );
    }
    provider
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
                ErrorKind::Unprocessable => 422,
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

    // (name, POST body or None for GET, profile-bearing)
    let cases: Vec<(&str, Option<Value>, bool)> = vec![
        ("list", None, false),
        (
            "run-coin-as-a",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-ansible-hit",
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-ansible-miss",
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
            false,
        ),
        ("run-no-character", Some(json!({ "tool": "coin" })), false),
        (
            "run-private",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "private": true }),
            ),
            false,
        ),
        (
            "run-unknown-tool",
            Some(
                json!({ "tool": "nope", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-unknown-character",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-0000000000ff" }),
            ),
            false,
        ),
        (
            "run-error",
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "parameters": { "bad": 1 } }),
            ),
            false,
        ),
        // The 616930db consult through the CHAT entrance — the third of the
        // three pascalMeta.llm writers.
        (
            "run-oracle-consult",
            Some(
                json!({ "tool": "oracle", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        // P4.6bd: the consult RESOLVES through the CHAT entrance — the inserted
        // profile carries the ladder, the oracle-recorded canned rows answer
        // 'YES', the `eq: 'YES'` outcome fires, and the CUSTOM_TOOL_CONSULT
        // llm-log row is diffed.
        (
            "run-oracle-consult-resolved",
            Some(
                json!({ "tool": "oracle", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            true,
        ),
        // P4.d10 `$state`: the manual-run entrance cascade, scoped to
        // `asCharacterId`'s own groups.
        (
            "run-stateful-as-a",
            Some(
                json!({ "tool": "stateful", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-stateful-as-b",
            Some(
                json!({ "tool": "stateful", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
            false,
        ),
    ];

    // Declared on BOTH sides, so a case added to the oracle and forgotten here
    // would pass silently on a smaller set.
    assert_eq!(
        cases.len(),
        oracle.len(),
        "the Rust case list and the oracle disagree: {} vs {}",
        cases.len(),
        oracle.len()
    );

    let mut checked = 0usize;
    for (name, body, profile) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        let db = open(name, *profile);

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
                // The REAL consult seam, exactly as the engine passes it since
                // P4.6bd: a `ProviderConsultRunner` over the canned provider —
                // the handler builds the real `CustomToolLlmInvoker` through
                // it. Profile-free cases stop at v4's `no connection profiles
                // are configured`; the `profile: true` case resolves through
                // the oracle-recorded canned rows.
                let runner = ProviderConsultRunner {
                    completion: canned_provider(want),
                };
                let (s, bd) = status_body(
                    chat_custom_tool_run(
                        &db,
                        USER,
                        CHAT,
                        &tool,
                        parameters,
                        private,
                        as_character_id,
                        Some(&runner),
                    )
                    .await,
                );
                let rows = system_rows(&db);
                (s, bd, rows)
            }
        };

        // The persisted llm_logs rows (P4.6bd): the resolved consult writes ONE
        // CUSTOM_TOOL_CONSULT row; every other case pins an empty dump.
        let got_logs = common::dump_llm_logs(&db);
        let want_logs = common::oracle_llm_logs(&want["llmLogs"]);
        assert_eq!(got_logs, want_logs, "case '{name}' llm_logs rows");

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
