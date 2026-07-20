//! Real-DB differential (P4.6ay unit 4, the handler half):
//! `tools::run_custom::execute_run_custom_tool` against v4's REAL
//! `executeRunCustomTool`, over the committed `pascal-run-custom-{main,mount}.db`
//! fixture. Each case runs on a fresh copy; the returned model output + the
//! posted `chat_messages` system rows are diffed. Rolls are `min === max` (no
//! draw), so the FixedBytes source is never consumed.
//!
//! P4.6bd: the `llm-consult-resolved` case is PROFILE-BEARING — both sides
//! insert the same connection profile through their real repos, the consult
//! RESOLVES through a recorded canned completion (the `tier3-completion-oracle`
//! keying), and the persisted `CUSTOM_TOOL_CONSULT` `llm_logs` row is diffed
//! alongside the Pascal message.
//!
//! Generate the oracle (v4 @ 616930db, Node 24 — jest ignores `.claude/`, so
//! mirror the case file to /tmp; see the case-file header):
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-pascal-mirror; mkdir -p $M/cases $M/fixtures
//!   cp <V5W>/harness/oracle/cases/pascal-run-custom-handler.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
//!   QT_FIXTURE_PASCAL_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db \
//!   QT_FIXTURE_PASCAL_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-run-custom-handler.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" -- pascal-run-custom-handler
//! Run:
//!   QT_ORACLE_PASCAL_RUN_CUSTOM_HANDLER=/tmp/oracle-pascal-run-custom-handler.ndjson \
//!     cargo test -p quilltap-harness --test pascal_run_custom_handler_equivalence

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::connection_profiles::{CpCreate, CreateOptions};
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::pascal::llm_consult::{CustomToolConsultContext, CustomToolLlmInvoker};
use quilltap_core::tools::rng::FixedBytes;
use quilltap_core::tools::run_custom::{
    execute_run_custom_tool_with_consult, RunCustomToolContext,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const CHAR_A: &str = "a1000000-0000-4000-8000-00000000000a";
const CHAR_B: &str = "a1000000-0000-4000-8000-00000000000b";
const CHAR_C: &str = "a1000000-0000-4000-8000-00000000000c";
const P_A: &str = "e1000000-0000-4000-8000-00000000000a";

#[derive(serde::Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
}

#[derive(serde::Deserialize)]
struct Meta {
    #[serde(rename = "vaultA")]
    vault_a: String,
    #[serde(rename = "vaultB")]
    vault_b: String,
    #[serde(rename = "vaultC")]
    vault_c: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// One test case, mirroring the oracle's corpus.
struct Case {
    name: &'static str,
    character_id: Option<&'static str>,
    vault: Option<String>,
    input: Value,
    /// P4.6bd: insert the shared consult profile before the run (mirrors the
    /// oracle's `profile: true` — the same row through both sides' real repos).
    profile: bool,
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

/// The one profile a `profile: true` case inserts — field-for-field the
/// oracle's `CONSULT_PROFILE` (both sides go through their real repos).
fn insert_consult_profile(main: &std::path::Path, pepper: &str, user_id: &str) {
    let writer = Writer::open_writable(main, pepper).expect("open main for profile insert");
    writer
        .connection_profiles()
        .create(
            &CpCreate {
                user_id: user_id.to_string(),
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

/// The system-sender rows (pascal / prospero), minted id + createdAt normalized
/// positionally, reduced to the fields the differential pins.
fn system_rows(msgs: &Value) -> Vec<Value> {
    let mut rows: Vec<Value> = msgs
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.get("systemSender").map(|s| !s.is_null()).unwrap_or(false))
        .collect();
    // Deterministic order (one system row per case, but be robust).
    rows.sort_by_key(|r| {
        r.get("systemKind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });
    rows.into_iter()
        .map(|r| {
            let get = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            // pascalMeta / targetParticipantIds are stored as JSON text — parse so
            // the diff is structural (key order irrelevant).
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

/// v4's output object, undefined fields omitted.
fn oracle_output(row: &Value) -> Value {
    row.get("output").cloned().unwrap_or(Value::Null)
}

/// Serialize a v5 output the way v4's `RunCustomToolOutput` serializes (omit the
/// `None` fields).
fn v5_output(out: &quilltap_core::tools::run_custom::RunCustomToolOutput) -> Value {
    let mut m = Map::new();
    m.insert("success".into(), json!(out.success));
    if let Some(t) = &out.tool {
        m.insert("tool".into(), json!(t));
    }
    if let Some(v) = out.value {
        m.insert("value".into(), json!(v));
    }
    if let Some(s) = &out.state {
        m.insert("state".into(), json!(s));
    }
    if let Some(msg) = &out.message {
        m.insert("message".into(), json!(msg));
    }
    if let Some(w) = out.whispered {
        m.insert("whispered".into(), json!(w));
    }
    if let Some(e) = &out.error {
        m.insert("error".into(), json!(e));
    }
    Value::Object(m)
}

fn canon(v: &Value) -> Value {
    // Fold integral floats to ints so `0.7` etc. compare across serializers.
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), canon(x))).collect()),
        _ => v.clone(),
    }
}

#[test]
fn run_custom_handler_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_RUN_CUSTOM_HANDLER") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_RUN_CUSTOM_HANDLER (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/pascal-run-custom.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("pascal-run-custom-main.db.meta.json"))
            .unwrap(),
    )
    .unwrap();

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

    let cases = vec![
        Case {
            name: "gated-hit",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "ansible" }),
            profile: false,
        },
        Case {
            name: "gated-miss",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "ansible" }),
            profile: false,
        },
        Case {
            name: "gated-empty",
            character_id: Some(CHAR_C),
            vault: Some(meta.vault_c.clone()),
            input: json!({ "tool": "ansible" }),
            profile: false,
        },
        Case {
            name: "public-coin",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin" }),
            profile: false,
        },
        Case {
            name: "whispered-default",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "whispered" }),
            profile: false,
        },
        Case {
            name: "private-override",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin", "private": true }),
            profile: false,
        },
        Case {
            name: "public-override",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "whispered", "private": false }),
            profile: false,
        },
        Case {
            name: "unknown-tool",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "nonexistent" }),
            profile: false,
        },
        Case {
            name: "validation-fail",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "notTool": 1 }),
            profile: false,
        },
        Case {
            name: "run-error",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin", "parameters": { "bad": 1 } }),
            profile: false,
        },
        // The 616930db consult, end to end over the real invoker.
        Case {
            name: "llm-consult-no-profiles",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "oracle" }),
            profile: false,
        },
        Case {
            name: "llm-consult-private",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "oracle", "private": true }),
            profile: false,
        },
        // P4.6bd: the consult RESOLVES — the inserted profile carries the
        // ladder, the canned provider answers 'YES' (the oracle-recorded key),
        // the `eq: 'YES'` outcome row fires, and the CUSTOM_TOOL_CONSULT
        // llm-log row is diffed. The tier-1 proof of the wire.
        Case {
            name: "llm-consult-resolved",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "oracle" }),
            profile: true,
        },
        // P4.d10 `$state`: the entrance cascade, scoped to the rolling
        // character's own groups (hit for CHAR_A in Table Stakes; fallback for
        // group-less CHAR_B).
        Case {
            name: "state-cascade-hit",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "stateful" }),
            profile: false,
        },
        Case {
            name: "state-no-group-falls-back",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "stateful" }),
            profile: false,
        },
    ];

    // The corpus is declared on BOTH sides, so a case added to the oracle and
    // forgotten here would pass silently on a smaller set. Pin the count.
    assert_eq!(
        cases.len(),
        oracle.len(),
        "the Rust case list and the oracle disagree: {} vs {}",
        cases.len(),
        oracle.len()
    );

    let mut checked = 0usize;
    for case in &cases {
        let want = oracle
            .get(case.name)
            .unwrap_or_else(|| panic!("oracle missing case '{}'", case.name));

        let scratch =
            std::env::temp_dir().join(format!("qt-pascal-h-{}-{}", std::process::id(), case.name));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main = scratch.join("main.db");
        let mount = scratch.join("mount.db");
        let llm_logs = scratch.join("llm-logs.db");
        std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
        std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
        // A fresh llm-logs partition per case (P4.6bd): the resolved consult's
        // CUSTOM_TOOL_CONSULT row is part of the diff, and the empty dump pins
        // that no other case logs.
        common::materialize_llm_logs(&llm_logs, &spec.test_pepper_base64);
        if case.profile {
            insert_consult_profile(&main, &spec.test_pepper_base64, &spec.user_id);
        }
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: Some(llm_logs),
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");

        let ctx = RunCustomToolContext {
            user_id: spec.user_id.clone(),
            chat_id: CHAT.to_string(),
            character_id: case.character_id.map(str::to_string),
            character_mount_point_id: case.vault.clone(),
            character_ids: Some(vec![
                CHAR_A.to_string(),
                CHAR_B.to_string(),
                CHAR_C.to_string(),
            ]),
            project_id: None,
            caller_participant_id: Some(P_A.to_string()),
        };
        let mut rng = FixedBytes::new(vec![]);
        // The REAL consult invoker, exactly as v4's handler builds one. For the
        // profile-free cases the resolution stops at v4's `no connection
        // profiles are configured` (proving the invoker is wired past v5's
        // would-be "no LLM invoker" arm); for `profile: true` cases the ladder
        // resolves and the oracle-RECORDED canned rows answer — any prompt/
        // selection/temperature divergence is a loud canned-miss.
        let completion = canned_provider(want);
        let invoker = CustomToolLlmInvoker::new(
            &db,
            &completion,
            CustomToolConsultContext {
                user_id: ctx.user_id.clone(),
                chat_id: Some(ctx.chat_id.clone()),
            },
        );
        let (out, _posted) = rt.block_on(execute_run_custom_tool_with_consult(
            &db,
            &ctx,
            &case.input,
            &mut rng,
            Some(&invoker),
        ));

        // min === max never draws.
        assert_eq!(rng.consumed(), 0, "case '{}' consumed bytes", case.name);

        let msgs = db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .unwrap();
        // The persisted llm_logs rows (P4.6bd): the resolved consult writes ONE
        // CUSTOM_TOOL_CONSULT row; every other case pins an empty dump.
        let got_logs = common::dump_llm_logs(&db);
        let want_logs = common::oracle_llm_logs(&want["llmLogs"]);
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        assert_eq!(got_logs, want_logs, "case '{}' llm_logs rows", case.name);

        assert_eq!(
            canon(&v5_output(&out)),
            canon(&oracle_output(want)),
            "case '{}' output",
            case.name
        );
        assert_eq!(
            canon(&Value::Array(system_rows(&msgs))),
            canon(&Value::Array(system_rows(want))),
            "case '{}' posted messages",
            case.name
        );
        checked += 1;
    }

    assert_eq!(checked, cases.len());
    eprintln!("OK: run_custom handler matched oracle ({checked} cases).");
}
