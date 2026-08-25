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
//! P4.d24 (v4 `e8a49597`): the five `list-*`/`run-no-character-*` cases on the
//! fixture's perspective rooms drive `operator_character_ids`/`prefer_operator`.
//! They exist because the original `CHAT` is STRUCTURALLY BLIND to that change —
//! its user-controlled participant plays CHAR_A, who is also first in stored
//! order, so the new preference and the old `sightings[0]` agree on every row.
//! Regenerating this family at `e8a49597` over the old fixture stayed green; the
//! corpus, not the port, was the thing that had to move.
//!
//! Generate the oracle (v4 @ e8a49597, Node 24 — mirror to /tmp; jest ignores
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
/// P4.d24 — the five operator-perspective rooms (see the fixture builder).
const CHAT_LLM_LED: &str = "c1000000-0000-4000-8000-000000000002";
const CHAT_TWO_OWN: &str = "c1000000-0000-4000-8000-000000000003";
const CHAT_ALL_LLM: &str = "c1000000-0000-4000-8000-000000000004";
const CHAT_SOLO: &str = "c1000000-0000-4000-8000-000000000005";
const CHAT_REMOVED: &str = "c1000000-0000-4000-8000-000000000006";
const CHAR_A: &str = "a1000000-0000-4000-8000-00000000000a";
const CHAR_B: &str = "a1000000-0000-4000-8000-00000000000b";
const CHAR_C: &str = "a1000000-0000-4000-8000-00000000000c";
/// P4.D35: the group tier the store dump reads back.
const GROUP: &str = "a2000000-0000-4000-8000-0000000000aa";
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
                multi_character_prefill: None,
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
                // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
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

/// v4's middleware answers an uncaught `ZodError` with `{error: 'Validation
/// error', details: [...issues]}`. The `details` issue array is the standing
/// project-wide deferral (the P4.6ay-unit-12 / groups / wardrobe precedent), so
/// v5's envelope carries the sentence alone.
///
/// This does NOT just drop the key: it first pins that the sentence the deferral
/// leaves behind is exactly `Validation error`. A route that answered some OTHER
/// top-level string with a `details` array would fail here rather than pass
/// silently.
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

/// P4.d24 — corpus-SHAPE assertions over the ORACLE, not over v5.
///
/// The point of the perspective rooms is that they discriminate; a fixture
/// rebuild that quietly flattened one (a room losing its user-controlled
/// participant, a gate stopping gating, `activeTypingParticipantId` not
/// persisting) would leave every row agreeing with `sightings[0]` again and this
/// family would go on passing — the exact silence that let v4's bug survive a
/// differential-verified port. So the witnesses below assert what v4's OWN
/// output must still contain before any diffing happens.
///
/// Each row is `(case, tool, expected asCharacterId, expected characterLabel)`.
fn assert_perspective_witnesses(oracle: &HashMap<String, Value>) {
    const WITNESSES: &[(&str, &str, &str, Option<&str>)] = &[
        // The bug's shape: the shared row runs as the OPERATOR's character even
        // though an LLM character leads the cast, and wears no label.
        ("list-llm-led", "coin", CHAR_B, None),
        // …while the gate the operator's character fails falls back to whoever
        // does pass, and NAMES them.
        ("list-llm-led", "secure_line", CHAR_A, Some("Bertie")),
        // `activeTypingParticipantId` beats stored order (CHAR_A is first).
        ("list-two-own", "coin", CHAR_B, None),
        // …and the operator's OTHER character is still preferred over the LLM
        // cast for a gate the active one fails — so no label.
        ("list-two-own", "secure_line", CHAR_A, None),
        // Nobody user-controlled: stored order, and the row says so.
        ("list-all-llm", "stateful", CHAR_B, Some("Jeeves")),
        // One character in the room: fall back, but nothing to disambiguate.
        ("list-solo", "coin", CHAR_A, None),
        // `removed` is not a candidate, `silent` is.
        ("list-removed-operator", "coin", CHAR_C, None),
        (
            "list-removed-operator",
            "secure_line",
            CHAR_A,
            Some("Bertie"),
        ),
    ];

    for (case, tool, as_character_id, label) in WITNESSES {
        let row = oracle
            .get(*case)
            .unwrap_or_else(|| panic!("oracle missing case '{case}'"));
        let tools = row["body"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("case '{case}' body has no tools array"));
        let listing = tools
            .iter()
            .find(|t| t.get("name").and_then(Value::as_str) == Some(*tool))
            .unwrap_or_else(|| {
                panic!("case '{case}': the oracle no longer lists '{tool}' — the room has drifted")
            });
        assert_eq!(
            listing.get("asCharacterId").and_then(Value::as_str),
            Some(*as_character_id),
            "case '{case}' tool '{tool}': the oracle's perspective moved"
        );
        assert_eq!(
            listing.get("characterLabel").and_then(Value::as_str),
            *label,
            "case '{case}' tool '{tool}': the oracle's characterLabel moved"
        );
    }
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

    // (name, chat id, POST body or None for GET, profile-bearing)
    let cases: Vec<(&str, &str, Option<Value>, bool)> = vec![
        ("list", CHAT, None, false),
        (
            "run-coin-as-a",
            CHAT,
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-ansible-hit",
            CHAT,
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-ansible-miss",
            CHAT,
            Some(
                json!({ "tool": "ansible", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
            false,
        ),
        (
            "run-no-character",
            CHAT,
            Some(json!({ "tool": "coin" })),
            false,
        ),
        (
            "run-private",
            CHAT,
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "private": true }),
            ),
            false,
        ),
        (
            "run-unknown-tool",
            CHAT,
            Some(
                json!({ "tool": "nope", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-unknown-character",
            CHAT,
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-0000000000ff" }),
            ),
            false,
        ),
        (
            "run-error",
            CHAT,
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "parameters": { "bad": 1 } }),
            ),
            false,
        ),
        // The 616930db consult through the CHAT entrance — the third of the
        // three pascalMeta.llm writers.
        (
            "run-oracle-consult",
            CHAT,
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
            CHAT,
            Some(
                json!({ "tool": "oracle", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            true,
        ),
        // P4.d10 `$state`: the manual-run entrance cascade, scoped to
        // `asCharacterId`'s own groups.
        (
            "run-stateful-as-a",
            CHAT,
            Some(
                json!({ "tool": "stateful", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-stateful-as-b",
            CHAT,
            Some(
                json!({ "tool": "stateful", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
            false,
        ),
        // -------------------------------------------------------------------
        // P4.d24 (v4 `e8a49597`) — the operator-perspective rooms.
        // -------------------------------------------------------------------
        // The bug's own shape: CHAR_A leads the cast, the operator plays CHAR_B.
        ("list-llm-led", CHAT_LLM_LED, None, false),
        // The run action's `asCharacterId`-less fallback in the same room.
        (
            "run-no-character-llm-led",
            CHAT_LLM_LED,
            Some(json!({ "tool": "ansible" })),
            false,
        ),
        // `activeTypingParticipantId` names the operator's SECOND character.
        ("list-two-own", CHAT_TWO_OWN, None, false),
        (
            "run-no-character-two-own",
            CHAT_TWO_OWN,
            Some(json!({ "tool": "ansible" })),
            false,
        ),
        // Nobody user-controlled: stored order, and shared rows labelled.
        ("list-all-llm", CHAT_ALL_LLM, None, false),
        // One character, LLM-controlled: fall back, but UNLABELLED.
        ("list-solo", CHAT_SOLO, None, false),
        // `removed` is not a candidate; `silent` still is.
        ("list-removed-operator", CHAT_REMOVED, None, false),
        // -------------------------------------------------------------------
        // P4.D35 — side effects through the MANUAL entrance.
        // -------------------------------------------------------------------
        (
            "run-ledger-as-a",
            CHAT,
            Some(json!({
                "tool": "ledger",
                "asCharacterId": "a1000000-0000-4000-8000-00000000000a",
                "parameters": { "entry": "brass" }
            })),
            false,
        ),
        (
            "run-ledger-as-b",
            CHAT,
            Some(
                json!({ "tool": "ledger", "asCharacterId": "a1000000-0000-4000-8000-00000000000b" }),
            ),
            false,
        ),
        // THE asymmetry: a run nobody made writes to nobody's fact sheet.
        (
            "run-ledger-no-character",
            CHAT,
            Some(json!({ "tool": "ledger" })),
            false,
        ),
        (
            "run-sealed-tally",
            CHAT,
            Some(
                json!({ "tool": "sealed_tally", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        // -------------------------------------------------------------------
        // P4.60 — the wrong-type-collapse arms. `handleRun` calls
        // `runSchema.parse` UNCAUGHT, so every refusal here is the middleware's
        // flat 400 `Validation error`; the schema's own sentences (including
        // `'A tool name is required'`) live only in the deferred `details`.
        // The passing arms are the other two poles the corpus needs: an explicit
        // `null` where the key is `nullish()`, and an unknown key `z.object`
        // strips.
        // -------------------------------------------------------------------
        (
            "run-tool-wrong-type",
            CHAT,
            Some(json!({ "tool": 123, "asCharacterId": "a1000000-0000-4000-8000-00000000000a" })),
            false,
        ),
        (
            "run-tool-empty",
            CHAT,
            Some(json!({ "tool": "", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" })),
            false,
        ),
        (
            "run-tool-missing",
            CHAT,
            Some(json!({ "asCharacterId": "a1000000-0000-4000-8000-00000000000a" })),
            false,
        ),
        (
            "run-parameters-wrong-type",
            CHAT,
            Some(
                json!({ "tool": "coin", "parameters": "nope", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-parameters-array",
            CHAT,
            Some(
                json!({ "tool": "coin", "parameters": [1], "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-parameters-null",
            CHAT,
            Some(
                json!({ "tool": "coin", "parameters": null, "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-private-wrong-type",
            CHAT,
            Some(
                json!({ "tool": "coin", "private": "yes", "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-private-null",
            CHAT,
            Some(
                json!({ "tool": "coin", "private": null, "asCharacterId": "a1000000-0000-4000-8000-00000000000a" }),
            ),
            false,
        ),
        (
            "run-as-character-wrong-type",
            CHAT,
            Some(json!({ "tool": "coin", "asCharacterId": 42 })),
            false,
        ),
        (
            "run-as-character-null",
            CHAT,
            Some(json!({ "tool": "coin", "asCharacterId": null })),
            false,
        ),
        (
            "run-ledger-as-empty-string",
            CHAT,
            Some(json!({ "tool": "ledger", "asCharacterId": "" })),
            false,
        ),
        (
            "run-unknown-key",
            CHAT,
            Some(
                json!({ "tool": "coin", "asCharacterId": "a1000000-0000-4000-8000-00000000000a", "bogus": 1 }),
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

    assert_perspective_witnesses(&oracle);

    let mut checked = 0usize;
    for (name, chat, body, profile) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        let db = open(name, *profile);

        let (status, resp_body, sys) = match body {
            None => {
                let (s, b) = status_body(chat_custom_tools_list(&db, USER, chat));
                (s, b, Vec::new())
            }
            Some(b) => {
                // P4.60: the raw body goes through the REAL `runSchema` port the
                // web edge calls. Reading the keys HERE with `as_str`/`as_bool`/
                // `as_object` — what this leg used to do — mirrored the edge's own
                // wrong-type collapse and would make every new arm below vacuous.
                match quilltap_core::api::custom_tools::parse_run_body(b) {
                    Err(resp) => {
                        let (s, bd) = status_body(resp);
                        (s, bd, system_rows(&db))
                    }
                    Ok(parsed) => {
                        // The REAL consult seam, exactly as the engine passes it
                        // since P4.6bd: a `ProviderConsultRunner` over the canned
                        // provider — the handler builds the real
                        // `CustomToolLlmInvoker` through it. Profile-free cases
                        // stop at v4's `no connection profiles are configured`;
                        // the `profile: true` case resolves through the
                        // oracle-recorded canned rows.
                        let runner = ProviderConsultRunner {
                            completion: canned_provider(want),
                        };
                        let (s, bd) = status_body(
                            chat_custom_tool_run(
                                &db,
                                USER,
                                chat,
                                &parsed.tool,
                                parsed.parameters,
                                parsed.private,
                                parsed.as_character_id,
                                Some(&runner),
                            )
                            .await,
                        );
                        let rows = system_rows(&db);
                        (s, bd, rows)
                    }
                }
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
            canon(&drop_zod_details(name, &want["body"])),
            "case '{name}' body"
        );
        assert_eq!(
            canon(&Value::Array(sys)),
            canon(&Value::Array(oracle_system_rows(want))),
            "case '{name}' system rows"
        );
        // P4.D35: WHERE the writes landed, not merely what `pascalMeta` claims.
        // A GET case's oracle `stores` is null, and so is the dump it is
        // compared against — the read path writes nothing, which is itself
        // worth asserting.
        let got_stores = if body.is_some() {
            common::dump_pascal_stores(&db, chat, GROUP, [CHAR_A, CHAR_B, CHAR_C])
        } else {
            Value::Null
        };
        assert_eq!(
            canon(&got_stores),
            canon(&want["stores"]),
            "case '{name}' state tiers + fact sheets after the run"
        );
        checked += 1;
    }

    assert_eq!(checked, cases.len());
    eprintln!("OK: custom-tools route matched oracle ({checked} cases).");
}
