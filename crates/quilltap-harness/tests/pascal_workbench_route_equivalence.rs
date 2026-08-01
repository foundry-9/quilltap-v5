//! Workbench ROUTE differential (P4.6ay unit 12): the four `/api/v1/custom-tools`
//! legs (`api::custom_tools::{custom_tools_library, custom_tools_destinations,
//! custom_tool_preview, custom_tool_audit}`) vs v4's REAL route handlers, over
//! the committed `workbench-{main,mount}.db` fixture. Diffs `{status, body}`.
//!
//! **The corpus lives in `harness/oracle/fixtures/workbench-route-cases.json`
//! and is read by BOTH sides**, so the jest oracle and this test cannot drift
//! apart on a definition or a character reference.
//!
//! **The simulate strategy (decided at round planning, recorded here).** v4's
//! `simulateOutcomes` draws through the crypto directly with no seam, so a
//! cross-language draw-for-draw diff is impossible. The corpus therefore uses
//! DETERMINISTIC definitions — `min === max` ranges, which short-circuit without
//! drawing a single byte — so `runs`/`hits`/`share`/min/max/mean are exact on
//! both sides. `AUDIT_RUNS` (10_000) applies identically either way. Stochastic
//! spread is covered instead by the v5-side statistical unit tests in
//! `pascal::custom_tools`, mirroring v4's own `custom-tools-simulate.test.ts`.
//!
//! P4.6bd: the `preview-live-consult` corpus case exercises the `{live:true}`
//! bench arm through the ASSEMBLED seam — profile-bearing (both sides insert
//! the shared consult profile through their real repos), the consult resolves
//! through oracle-recorded canned completions, and the persisted
//! `CUSTOM_TOOL_CONSULT` `llm_logs` row is diffed per case.
//!
//! Generate the oracle (v4 @ 616930db, Node 24 — mirror to /tmp; jest ignores
//! `.claude/`):
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-workbench-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures
//!   cp <V5W>/harness/oracle/cases/pascal-workbench-route.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/workbench.json \
//!      <V5W>/harness/oracle/fixtures/workbench-route-cases.json $M/fixtures/
//!   ln -sfn $PWD/node_modules $M/node_modules
//!   QT_FIXTURE_WORKBENCH_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/workbench-main.db \
//!   QT_FIXTURE_WORKBENCH_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-workbench-route.ndjson \
//!     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=300000 -- pascal-workbench-route
//! Run:
//!   QT_ORACLE_PASCAL_WORKBENCH_ROUTE=/tmp/oracle-pascal-workbench-route.ndjson \
//!     cargo test -p quilltap-harness --test pascal_workbench_route_equivalence

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::custom_tools::{
    custom_tool_audit, custom_tool_preview, custom_tools_destinations, custom_tools_library,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::connection_profiles::{CpCreate, CreateOptions};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::pascal::llm_consult::ProviderConsultRunner;
use serde::Deserialize;
use serde_json::{json, Value};

/// The fixture's user (`harness/oracle/fixtures/workbench.json`). Reached by
/// the `{live:true}` arm (the P4.6bd `preview-live-consult` case).
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/workbench-route-cases.json")
}

fn open(case: &str, profile: bool) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-workbench-route-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm_logs = scratch.join("llm-logs.db");
    std::fs::copy(fixtures_dir().join("workbench-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("workbench-mount.db"), &mount).unwrap();
    // A fresh llm-logs partition per case (P4.6bd): the live consult's
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
///
/// The committed workbench fixture predates any profile-bearing case and has
/// NEITHER a `connection_profiles` NOR a `chat_settings` table (the consult
/// reads both); v4's `initializeDatabase` materializes missing tables on open
/// (its `ensureCollection` DDL), so the v5 side replays the SAME captured DDL
/// from `fresh_schema.json` (the D23 artifact — never hand-written) before
/// inserting. No `chat_settings` ROW is seeded — both sides read none and take
/// the default cheap-LLM config, exactly as v4 does over its empty table.
fn insert_consult_profile(main: &std::path::Path) {
    let writer = Writer::open_writable(main, PEPPER).expect("open main for profile insert");
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-core/src/services/provisioning/fresh_schema.json"),
        )
        .expect("read fresh_schema.json"),
    )
    .expect("parse fresh_schema.json");
    for stmt in schema["main"].as_array().expect("main DDL list") {
        let s = stmt.as_str().unwrap();
        if s.contains("connection_profiles") || s.contains("chat_settings") {
            writer
                .connection()
                .execute_batch(s)
                .expect("materialize consult-read tables");
        }
    }
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
// (`tier3-completion-oracle` keying — replayed exactly, so any prompt/
// selection/temperature divergence is a loud canned-miss).
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

fn status_body(resp: Response) -> (u16, Value) {
    match resp {
        Response::CustomToolsLibrary(v)
        | Response::CustomToolsDestinations(v)
        | Response::CustomToolPreview(v)
        | Response::CustomToolAudit(v) => (200, v),
        Response::Error(e) => {
            let status = match e.kind {
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
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Collapse a JSON-PARSE failure reason to its prefix — unit 2's documented
/// seam, inherited by the library leg. See `pascal_workbench_equivalence`.
fn canon_reason(v: &Value) -> Value {
    match v.as_str() {
        Some(s) if s.starts_with("is not valid JSON:") => {
            Value::String("is not valid JSON:".into())
        }
        _ => v.clone(),
    }
}

/// Fold integral floats → ints so JS's single number type and Rust's agree.
fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                // `details` is v4's raw Zod issue list on a BODY rejection. v5
                // emits the envelope without it — the standing P4.6bb
                // error-envelope `details` deferral, not this drift's doing —
                // so it is dropped on BOTH sides here. The STATUS and the
                // `error` sentence, which are the contract the SPA reads, stay
                // fully compared.
                .filter(|(k, _)| k.as_str() != "details")
                .map(|(k, x)| {
                    (
                        k.clone(),
                        if k == "reason" {
                            canon_reason(x)
                        } else {
                            canon(x)
                        },
                    )
                })
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// Resolve a case's `metadata` — either given inline, or a `metadataCharacter`
/// reference the corpus resolves to a `{ characterId }` object (exactly as the
/// oracle's `bodyFor` does).
/// P4.D35 — every character fact sheet the workbench fixture carries, read back
/// after a bench case. See the assertion's comment for why this exists.
///
/// The fixture has no chat, so a `state.*` effect on this route has no cascade
/// at all; the sheets are the only stores a preview could reach, and they are
/// exactly what must not move.
fn dump_bench_stores(db: &Db, characters: &Value) -> Value {
    use quilltap_core::db::characters_read;
    let mut metadata = serde_json::Map::new();
    if let Some(map) = characters.as_object() {
        for (label, id) in map {
            let Some(id) = id.as_str() else { continue };
            let sheet = db
                .read_main(|main| {
                    db.read_mount_index(|mount| {
                        Ok(characters_read::find_by_id(main, mount, id)
                            .ok()
                            .flatten()
                            .and_then(|c| c.get("metadata").cloned())
                            .unwrap_or(Value::Null))
                    })
                })
                .unwrap_or(Value::Null);
            metadata.insert(label.clone(), sheet);
        }
    }
    serde_json::json!({ "metadata": metadata })
}

fn metadata_for(case: &Value, characters: &Value) -> Option<Value> {
    if let Some(m) = case.get("metadata") {
        return Some(m.clone());
    }
    let key = case.get("metadataCharacter")?.as_str()?;
    let id = characters.get(key)?.clone();
    Some(json!({ "characterId": id }))
}

#[test]
fn workbench_route_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PASCAL_WORKBENCH_ROUTE") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_WORKBENCH_ROUTE (see header).");
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

    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).unwrap()).unwrap();
    let definitions = &corpus["definitions"];
    let characters = &corpus["characters"];
    let cases = corpus["cases"].as_array().expect("cases array");

    let mut checked = 0usize;
    let mut gate_verdicts: Vec<serde_json::Value> = Vec::new();
    let mut statuses: Vec<u16> = Vec::new();

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let profile = case.get("profile").and_then(Value::as_bool) == Some(true);
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing case '{name}'"));
        let db = open(name, profile);

        let (status, body) = match case["method"].as_str().unwrap() {
            "GET" => match case.get("action").and_then(Value::as_str) {
                Some("destinations") => status_body(custom_tools_destinations(&db)),
                _ => status_body(custom_tools_library(&db)),
            },
            _ => {
                let definition = definitions[case["definition"].as_str().unwrap()].clone();
                let params = case.get("params").cloned();
                let metadata = metadata_for(case, characters);
                match case["action"].as_str().unwrap() {
                    // The consult seam made preview async (it may pose a
                    // consult). §B: the bench oracle rides both bodies; an
                    // explicit `null` is a distinct arm from an omitted field,
                    // so the corpus value is forwarded as-is. P4.6bd: the seam
                    // is passed the way the engine passes it — a runner over
                    // the oracle-recorded canned completions; scripted/fail
                    // cases never touch it, and only the profile-bearing
                    // `preview-live-consult` case resolves through it (no
                    // differential ever spends a real LLM call).
                    "preview" => {
                        let runner = ProviderConsultRunner {
                            completion: canned_provider(want),
                        };
                        status_body(
                            tokio::runtime::Builder::new_current_thread()
                                .build()
                                .expect("a current-thread runtime")
                                .block_on(custom_tool_preview(
                                    &db,
                                    &definition,
                                    params.as_ref(),
                                    case.get("private").and_then(Value::as_bool),
                                    metadata.as_ref(),
                                    case.get("state"),
                                    case.get("llm"),
                                    USER,
                                    Some(&runner),
                                )),
                        )
                    }
                    _ => status_body(custom_tool_audit(
                        &db,
                        &definition,
                        params.as_ref(),
                        metadata.as_ref(),
                        case.get("state"),
                        case.get("llm"),
                    )),
                }
            }
        };

        // The persisted llm_logs rows (P4.6bd): the live consult writes ONE
        // CUSTOM_TOOL_CONSULT row (chatId null — a bench run belongs to no
        // room); every other case pins an empty dump.
        let got_logs = common::dump_llm_logs(&db);
        let want_logs = common::oracle_llm_logs(&want["llmLogs"]);
        assert_eq!(got_logs, want_logs, "case '{name}' llm_logs rows");

        assert_eq!(
            status as u64,
            want["status"].as_u64().unwrap(),
            "case '{name}' status"
        );
        assert_eq!(canon(&body), canon(&want["body"]), "case '{name}' body");
        // P4.D35: "the bench computes, never applies" is a claim about ABSENCE,
        // and only a state diff can carry it — a body assertion alone would
        // still pass if the preview quietly wrote. Dumped on EVERY case, so a
        // preview that started writing is caught wherever it happened.
        let got_stores = dump_bench_stores(&db, characters);
        assert_eq!(
            canon(&got_stores),
            canon(&want["stores"]),
            "case '{name}': the bench must have written nothing"
        );
        // P4.d19 §2(b): the preview's `gate` key — present ONLY when the
        // definition gates, and `withheldBy` ABSENT (not null) when available.
        if let Some(gate) = body.get("data").unwrap_or(&body).get("gate") {
            gate_verdicts.push(gate.clone());
        }
        statuses.push(status);
        checked += 1;
    }

    // Coverage the corpus was built for: a thinner corpus (or one whose arms
    // stopped firing) must fail loudly rather than pass on fewer branches.
    assert_eq!(checked, cases.len());
    assert!(statuses.contains(&200), "a success arm");
    assert!(statuses.contains(&400), "an invalid-definition arm");
    assert!(statuses.contains(&404), "an unknown-character arm");
    assert!(
        statuses.contains(&422),
        "a run-refusal / broken-vault arm (v5's first 422)"
    );
    // §2(b) again, as coverage: both clauses must be seen withholding, and an
    // available verdict must be seen carrying NO `withheldBy` key at all. An
    // ungated preview contributes nothing here, which is the other half of the
    // claim — the key is absent, so it never reaches this vector.
    assert!(
        gate_verdicts
            .iter()
            .any(|v| v["available"] == true && v.get("withheldBy").is_none()),
        "an available verdict with withheldBy absent"
    );
    for clause in ["availableWhen", "withheldWhen"] {
        assert!(
            gate_verdicts
                .iter()
                .any(|v| v["available"] == false && v["withheldBy"] == clause),
            "a verdict withheld by {clause}"
        );
    }
    eprintln!("OK: workbench route matched oracle ({checked} cases).");
}
