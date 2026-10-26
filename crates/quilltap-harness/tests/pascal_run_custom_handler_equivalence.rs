//! Real-DB differential (P4.6ay unit 4, the handler half):
//! `tools::run_custom::execute_run_custom_tool` against v4's REAL
//! `executeRunCustomTool`, over the committed `pascal-run-custom-{main,mount}.db`
//! fixture. Each case runs on a fresh copy; the returned model output + the
//! posted `chat_messages` system rows are diffed. Rolls are `min === max` (no
//! draw), so the FixedBytes source is never consumed.
//!
//! P4.D169: a case may pin the run's ONE clock reading with `nowMs`, which the
//! jest oracle freezes `Date.now()` to and RECORDS on the row — this side then
//! reads the instant off the row rather than a constant, so the two cannot be
//! edited out of step. Required for any case that gates on, renders or WRITES a
//! progression; omitted (and unchanged) on every clock-independent case.
//!
//! P4.6bd: the `llm-consult-resolved` case is PROFILE-BEARING — both sides
//! insert the same connection profile through their real repos, the consult
//! RESOLVES through a recorded canned completion (the `tier3-completion-oracle`
//! keying), and the persisted `CUSTOM_TOOL_CONSULT` `llm_logs` row is diffed
//! alongside the Pascal message.
//!
//! P4.D305 (v4 `362426c8f`): the deal now runs through the ONE run service
//! (`pascal::run_service`), and this family is the extraction's proof — its
//! pre-existing rows are unchanged. Each row also carries v4's log lines on
//! the run-custom path (the service's `Custom tool run refused` / `Custom tool
//! dealt` / `state cascade unavailable`, the handler's `Custom tool run
//! failed` / `Custom tool run completed`), captured through the transports
//! splice and compared against v5's `global_capture` lines narrowed to the
//! same set; and every posted row's `pascalMeta` TEXT is compared as bytes,
//! so its key order is checked (`Map ==` is order-blind). Two plant rows:
//! `run-llm-chat-unreadable` (the cascade's chat read fails SILENTLY — v4's
//! `findById` is a fallback) and `run-llm-unposted` (every insert aborts).
//!
//! Generate the oracle (Node 24; the v4 checkout at the oracle baseline —
//! jest ignores `.claude/`, so mirror the case file to /tmp; see the case-file
//! header). Self-shielding: the fixture DBs AND the `.meta.json` sidecar the
//! oracle reads are copied to /tmp first, so the committed fixtures are never
//! run against directly (the oracle's cases write) and the sidecar travels
//! with the copy:
//!   cd ~/source/quilltap-server
//!   M=/tmp/qt-pascal-mirror; mkdir -p $M/cases $M/fixtures $M/db
//!   cp <V5W>/harness/oracle/cases/pascal-run-custom-handler.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db $M/db/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db.meta.json $M/db/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db $M/db/
//!   QT_FIXTURE_PASCAL_MAIN=$M/db/pascal-run-custom-main.db \
//!   QT_FIXTURE_PASCAL_MOUNT=$M/db/pascal-run-custom-mount.db \
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
use quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use quilltap_core::services::mount_index::converters::RefusingTextExtractor;
use quilltap_core::services::mount_index::store_file::{store_mount_file, StoreFileInput};
use quilltap_core::tools::rng::FixedBytes;
use quilltap_core::tools::run_custom::{
    execute_run_custom_tool_with_consult, RunCustomToolContext,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// P4.D169: Pascal's run entrance now takes its clock from the caller.
/// 2026-08-29T12:00:00Z.
const PASCAL_NOW_MS: i64 = 1_788_004_800_000;

/// The instant one case runs at: its own `nowMs` when it pins one, otherwise
/// the shared default. See `PASCAL_NOW_MS`.
fn case_now_ms(row: &Value) -> i64 {
    row.get("nowMs")
        .and_then(Value::as_i64)
        .unwrap_or(PASCAL_NOW_MS)
}

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const CHAR_A: &str = "a1000000-0000-4000-8000-00000000000a";
const CHAR_B: &str = "a1000000-0000-4000-8000-00000000000b";
const CHAR_C: &str = "a1000000-0000-4000-8000-00000000000c";
/// P4.D35: the group tier the store dump reads back.
const GROUP: &str = "a2000000-0000-4000-8000-0000000000aa";
/// P4.d19: the character whose vault is broken (no `properties.json`).
const CHAR_D: &str = "a1000000-0000-4000-8000-00000000000d";
const P_A: &str = "e1000000-0000-4000-8000-00000000000a";

/// P4.D319: the per-case RAW-name plant (never baked — R-6): both trims.
const PLANT_PADDED_BERTIE: &[&str] = &[
    r#"UPDATE "characters" SET "name" = '  Bertie  ' WHERE "id" = 'a1000000-0000-4000-8000-00000000000a'"#,
];

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
    #[serde(rename = "vaultD")]
    vault_d: String,
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
    /// P4.D305: SQL planted on the main copy before the open (the oracle's
    /// `plant`, the same statements).
    plant: Option<&'static [&'static str]>,
}

/// P4.D329: the mirror rows that need a REAL bag (v4's `loadInventoryContext`;
/// on this branch C2's stand-in refuses, so every gate on `inventory`
/// declines and every read takes the refusal). MEASURED. A carved row that
/// turns GREEN fails the run (lift it).
const RED_UNTIL_UNION: &[&str] = &[
    "measure-twice-carrier-runs",
    "measure-twice-probe-kind-name",
    "measure-twice-probe-title-twin",
    "measure-twice-probe-walkie",
    "measure-twice-probe-spyglass-withheld",
    "inventory-read-message-door",
    "inventory-read-default-door",
    "inventory-read-prompt-door",
    "inventory-read-none-named",
];

/// Regenerated at the pin `322a5488e`, the rows whose `Available:` list v4's
/// vault-only custom tools moved (a vault's tools are its own character's
/// only) — P4.D332's to port; green at the union. MEASURED: the five
/// pre-existing rows plus the two mirror rows whose answer is a refusal.
const P4D332_RESIDUE: &[&str] = &[
    "unknown-tool",
    "gate-available-missing-key-unrunnable",
    "gate-available-empty-sheet-unrunnable",
    "gate-withheld-when-holds-unrunnable",
    "progress-gate-running-span-unrunnable",
    "measure-twice-other-instance-withheld",
    "measure-twice-nobody-withheld",
];

/// P4.D305: the log lines this family pins (the oracle's `PINNED_MESSAGES`).
const PINNED_MESSAGES: [&str; 6] = [
    "Custom tool run: state cascade unavailable, defaulting to {}",
    "Custom tool run refused",
    "Custom tool dealt",
    "Custom tool run failed",
    "Custom tool run completed",
    // P4.D333 (v4 `322a5488e`): the table-first order announces itself.
    "Custom tool table dealt ahead of the consult",
];

/// P4.D333: the oracle's `NARRATOR_TOOL_TEXT`, byte for byte — a consult-
/// bearing tool whose table does not test the consult (bug 257's table-first
/// order). Written into the case's vault on the per-run copy through v5's
/// real `store_mount_file`, as v4's case writes it through `storeMountFile`.
const NARRATOR_TOOL_TEXT: &str = r#"{"name":"narrator","description":"Narrate a scan.","parameters":{"layers":{"type":"number","default":95}},"roll":{"min":1,"max":1},"llm":{"prompt":"Write a report. State: {{outcome.state}}, row {{outcome.index}}. Conclusion to reach: {{outcome.brief}}","errorMessage":"The link falters."},"outcomes":[{"when":{"roll":{"gte":{"$param":"layers"}}},"message":"{{llm}}","state":"success","brief":"Confirmed at {{params.layers}} layers."},{"when":true,"message":"{{llm}} [{{outcome.state}}]","state":"failure","brief":"Not found."}]}"#;

/// P4.D333: `(case, relative path, text)` — the per-case tool plants.
const TOOL_PLANTS: &[(&str, &str, &str)] = &[
    (
        "llm-consult-table-first",
        "Tools/narrator.tool.json",
        NARRATOR_TOOL_TEXT,
    ),
    (
        "llm-consult-table-first-no-profiles",
        "Tools/narrator.tool.json",
        NARRATOR_TOOL_TEXT,
    ),
];

/// The oracle's `PLANT_CHAT_UNREADABLE` — `chats.title` rebuilt as a generated
/// column whose read throws, on BOTH sides' reads.
const PLANT_CHAT_UNREADABLE: &[&str] = &[
    r#"ALTER TABLE "chats" DROP COLUMN "title""#,
    r#"ALTER TABLE "chats" ADD COLUMN "title" TEXT AS (json("id")) VIRTUAL"#,
];
/// The oracle's `PLANT_POST_FAILS` — every `chat_messages` insert aborts.
const PLANT_POST_FAILS: &[&str] = &[
    r#"CREATE TRIGGER "p4d305_no_posts" BEFORE INSERT ON "chat_messages" BEGIN SELECT RAISE(ABORT, 'planted: no posts'); END"#,
];

/// The posted system rows' raw `pascalMeta` TEXT, in row order — compared as
/// BYTES so the key order (`invokedBy`, `triggerPhrase`,
/// `callerParticipantId`) is checked.
fn pascal_meta_texts(msgs: &Value) -> Vec<Value> {
    msgs.get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.get("pascalMeta").is_some_and(|m| !m.is_null()))
        .map(|r| r["pascalMeta"].clone())
        .collect()
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
                role: CompletionRole::from_v4_wire(m.role.as_str())
                    .unwrap_or_else(|| panic!("unexpected role {}", m.role.as_str())),
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
                multi_character_prefill: None,
                model_class: None,
                fallback_profile_id: None,
                allow_tier_fallback: false,
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
    // The capture rig must own this binary's subscriber before any callsite is
    // reached (memory note `capture-rig-writer-thread`).
    quilltap_core::test_support::global_capture::install();
    // P4.133 OUT-OF-MANDATE (dogfood #133): the oracle mocks
    // `getApiKeyForCheapLLMSelection` to a constant; the twin (v5 now resolves
    // the cheap selection's own key — and the Concierge classifier's — and
    // refuses without one).
    let _canned_key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
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
            plant: None,
        },
        Case {
            name: "gated-miss",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "ansible" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "gated-empty",
            character_id: Some(CHAR_C),
            vault: Some(meta.vault_c.clone()),
            input: json!({ "tool": "ansible" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "public-coin",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "whispered-default",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "whispered" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "private-override",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin", "private": true }),
            profile: false,
            plant: None,
        },
        Case {
            name: "public-override",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "whispered", "private": false }),
            profile: false,
            plant: None,
        },
        Case {
            name: "unknown-tool",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "nonexistent" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "validation-fail",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "notTool": 1 }),
            profile: false,
            plant: None,
        },
        Case {
            name: "run-error",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin", "parameters": { "bad": 1 } }),
            profile: false,
            plant: None,
        },
        // The 616930db consult, end to end over the real invoker.
        Case {
            name: "llm-consult-no-profiles",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "oracle" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "llm-consult-private",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "oracle", "private": true }),
            profile: false,
            plant: None,
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
            plant: None,
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
            plant: None,
        },
        // ---- P4.d19: the availability gate, end-to-end through the handler --
        // The gated definitions live in the GENERAL store, so only the invoker's
        // own fact sheet differs between these rows. A withheld tool is ABSENT
        // from the roster, so the handler answers exactly as it does for a name
        // the model invented.
        Case {
            name: "gate-available-runs",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "secure_line" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "gate-available-missing-key-unrunnable",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "secure_line" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "gate-available-empty-sheet-unrunnable",
            character_id: Some(CHAR_C),
            vault: Some(meta.vault_c.clone()),
            input: json!({ "tool": "secure_line" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "gate-withheld-when-holds-unrunnable",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "novice_aid" }),
            profile: false,
            plant: None,
        },
        // The same empty sheet that fails `availableWhen` above satisfies no
        // `withheldWhen`, so CHAR_C IS dealt this one.
        Case {
            name: "gate-withheld-when-empty-sheet-runs",
            character_id: Some(CHAR_C),
            vault: Some(meta.vault_c.clone()),
            input: json!({ "tool": "novice_aid" }),
            profile: false,
            plant: None,
        },
        // ---- P4.d19: the fact-sheet read MOVED ahead of the roster ---------
        // With no definition in hand, only an explicit `private: true` whispers.
        Case {
            name: "vault-broken-private-true",
            character_id: Some(CHAR_D),
            vault: Some(meta.vault_d.clone()),
            input: json!({ "tool": "coin", "private": true }),
            profile: false,
            plant: None,
        },
        Case {
            name: "vault-broken-private-false",
            character_id: Some(CHAR_D),
            vault: Some(meta.vault_d.clone()),
            input: json!({ "tool": "whispered", "private": false }),
            profile: false,
            plant: None,
        },
        // THE arm whose behavior CHANGED: `whispered` declares
        // defaultVisibility:'whisper', and this failure is PUBLIC now.
        Case {
            name: "vault-broken-private-absent-whisper-default",
            character_id: Some(CHAR_D),
            vault: Some(meta.vault_d.clone()),
            input: json!({ "tool": "whispered" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "vault-broken-unknown-tool",
            character_id: Some(CHAR_D),
            vault: Some(meta.vault_d.clone()),
            input: json!({ "tool": "nonexistent" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "state-no-group-falls-back",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "stateful" }),
            profile: false,
            plant: None,
        },
        // ---- P4.D35: side effects, end-to-end through the LLM entrance ----
        Case {
            name: "effects-all-tiers",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "ledger", "parameters": { "entry": "brass" } }),
            profile: false,
            plant: None,
        },
        Case {
            name: "effects-no-group-skips-the-group-write",
            character_id: Some(CHAR_B),
            vault: Some(meta.vault_b.clone()),
            input: json!({ "tool": "ledger" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "effects-precedence-under-sealed-odds",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "sealed_tally" }),
            profile: false,
            plant: None,
        },
        // ---- P4.D169: progressions, end to end through the LLM entrance ----
        // The clock each of these runs at is taken from its ORACLE row, which
        // is the value the jest side froze `Date.now()` to — so the two sides
        // cannot disagree about the instant by construction.
        Case {
            name: "progress-gate-finished-span-runs",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "recharged" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "progress-gate-running-span-unrunnable",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "gestating" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "progress-effect-writes-the-span",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "recharge" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "progress-effect-creates-a-progression",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "kindle" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "progress-effect-writes-under-a-different-clock",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "recharge" }),
            profile: false,
            plant: None,
        },
        // ---- P4.D305: the run service's fail-soft arms ---------------------
        // The cascade's chat read fails SILENTLY (a fallback read — v4's
        // DEBUG `state cascade unavailable` is NOT reached this way); the deal
        // lands against `{}`; the post's chat check answers None.
        Case {
            name: "run-llm-chat-unreadable",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin" }),
            profile: false,
            plant: Some(PLANT_CHAT_UNREADABLE),
        },
        // The chat reads, every insert aborts: the handler's unposted arm.
        Case {
            name: "run-llm-unposted",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "coin" }),
            profile: false,
            plant: Some(PLANT_POST_FAILS),
        },
        // ---- P4.D319 (v4 `a05c6f755`): `{{character}}` + `announceAsCharacter`
        // in the trigger room (the room comes from the oracle row's `room`).
        // The handler passes no name, so the service reads the RAW row.
        Case {
            name: "announced-dresser-raw-name",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "announced_dresser" }),
            profile: false,
            plant: None,
        },
        Case {
            name: "announced-dresser-padded-raw-name",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "announced_dresser" }),
            profile: false,
            plant: Some(PLANT_PADDED_BERTIE),
        },
        Case {
            name: "announced-dresser-regression-spell",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "spell" }),
            profile: false,
            plant: None,
        },
        // ---- P4.D333 (v4 `322a5488e`, bug 257): the table dealt FIRST ------
        // The narrator tool is planted per case (`TOOL_PLANTS`). Resolving:
        // `pascalMeta.llm.prompt` and the CUSTOM_TOOL_CONSULT row carry the
        // TOLD verdict, and the run logs the table-first line.
        Case {
            name: "llm-consult-table-first",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "narrator" }),
            profile: true,
            plant: None,
        },
        // No profile: dealt first all the same, the consult fails soft.
        Case {
            name: "llm-consult-table-first-no-profiles",
            character_id: Some(CHAR_A),
            vault: Some(meta.vault_a.clone()),
            input: json!({ "tool": "narrator" }),
            profile: false,
            plant: None,
        },
    ];
    // === P4.D329: v4 `cad017599`'s seven `it`s MIRRORED over the widened pair
    // (`pascal-run-custom-inventory-pascal.ts`), plus the run service's bag
    // read by door (R-15). The room comes from the oracle row's `room`. ===
    let inventory_case = |name: &'static str, who: Option<&'static str>, tool: &str| Case {
        name,
        character_id: who,
        vault: match who {
            Some(CHAR_A) => Some(meta.vault_a.clone()),
            Some(CHAR_B) => Some(meta.vault_b.clone()),
            _ => None,
        },
        input: json!({ "tool": tool }),
        profile: false,
        plant: None,
    };
    let mut cases = cases;
    cases.extend([
        inventory_case("measure-twice-carrier-runs", Some(CHAR_A), "measure_twice"),
        inventory_case(
            "measure-twice-other-instance-withheld",
            Some(CHAR_B),
            "measure_twice",
        ),
        inventory_case("measure-twice-nobody-withheld", None, "measure_twice"),
        inventory_case(
            "measure-twice-probe-kind-name",
            Some(CHAR_A),
            "probe_resonator",
        ),
        inventory_case(
            "measure-twice-probe-title-twin",
            Some(CHAR_A),
            "probe_resonator_title",
        ),
        inventory_case("measure-twice-probe-walkie", Some(CHAR_A), "probe_walkie"),
        inventory_case(
            "measure-twice-probe-spyglass-withheld",
            Some(CHAR_A),
            "probe_spyglass",
        ),
        inventory_case(
            "inventory-read-message-door",
            Some(CHAR_A),
            "reads_in_message",
        ),
        inventory_case(
            "inventory-read-default-door",
            Some(CHAR_A),
            "reads_in_default",
        ),
        inventory_case(
            "inventory-read-prompt-door",
            Some(CHAR_A),
            "reads_in_prompt",
        ),
        inventory_case("inventory-read-none-named", Some(CHAR_A), "spell"),
        inventory_case("inventory-read-nobody-rolled", None, "reads_in_message"),
    ]);

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
    // P4.D169's floors. Each was ZERO before this lane widened the corpus, and
    // the family was green at zero: the entrance threaded a clock and a sheet
    // that no case read.
    let mut saw_progress_gate_withheld = false;
    let mut saw_progress_write = false;
    let mut saw_progress_create = false;
    let mut logged_rows = 0usize;
    let (mut failed, mut red_green): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    // P4.D333: the bug-257 tool plants applied, and the table-first lines pinned.
    let mut plants_applied = 0usize;
    let mut table_first_lines = 0usize;
    // P4.D329: the oracle's inventory lines across the mirror rows (each is
    // compared on its row; a carved row stops at its first difference).
    let inventory_lines_in_oracle: usize = oracle
        .iter()
        .filter(|(n, _)| n.starts_with("measure-twice-") || n.starts_with("inventory-read-"))
        .map(|(_, r)| {
            r.get("inventoryLines")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .sum();
    for case in &cases {
        // The coverage floors read the ORACLE row only, so they are taken
        // before the compare (a carved row still counts toward them).
        {
            let want = oracle
                .get(case.name)
                .unwrap_or_else(|| panic!("oracle missing case '{}'", case.name));
            // `updatedAt` is stamped only by the applier, and `kettle` exists in no
            // fixture — so each is the signature of a write that actually landed in
            // a real vault rather than merely being reported in `applied`.
            let vault_a = want["stores"]["metadata"]["A"]
                .get("progressions")
                .cloned()
                .unwrap_or(Value::Null);
            if vault_a
                .get("cannon")
                .and_then(|c| c.get("updatedAt"))
                .is_some()
            {
                saw_progress_write = true;
            }
            if vault_a.get("kettle").is_some() {
                saw_progress_create = true;
            }
            if want["output"]["error"]
                .as_str()
                .is_some_and(|e| e.contains("gestating"))
            {
                saw_progress_gate_withheld = true;
            }
        }
        let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let want = oracle
                .get(case.name)
                .unwrap_or_else(|| panic!("oracle missing case '{}'", case.name));

            let scratch = std::env::temp_dir().join(format!(
                "qt-pascal-h-{}-{}",
                std::process::id(),
                case.name
            ));
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
            if let Some((_, path, text)) =
                TOOL_PLANTS.iter().find(|(name, _, _)| *name == case.name)
            {
                let vault = case.vault.as_deref().expect("a tool plant needs a vault");
                let main_w = Writer::open_writable(&main, &spec.test_pepper_base64)
                    .expect("open main for the tool plant");
                let mount_w = Writer::open_writable(&mount, &spec.test_pepper_base64)
                    .expect("open mount for the tool plant");
                let mut input = StoreFileInput::new(vault, path, text.as_bytes().to_vec());
                input.original_mime_type = Some("application/json".into());
                input.transcode_images = false;
                input.extract_text = false;
                input.enqueue_embedding = false;
                store_mount_file(
                    main_w.connection(),
                    mount_w.connection(),
                    &input,
                    &RefusingTextExtractor,
                    &RefusingWebpTranscoder,
                )
                .unwrap_or_else(|e| panic!("case '{}' tool plant: {e:?}", case.name));
                plants_applied += 1;
            }
            if let Some(plant) = case.plant {
                let writer = Writer::open_writable(&main, &spec.test_pepper_base64)
                    .expect("open main for the plant");
                for sql in plant {
                    writer
                        .connection()
                        .execute_batch(sql)
                        .unwrap_or_else(|e| panic!("case '{}' plant {sql}: {e}", case.name));
                }
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

            // P4.D319: a case may run in another room (the oracle row's `room`).
            let room = &want["room"];
            let chat_id = room["chatId"].as_str().unwrap_or(CHAT).to_string();
            let ctx = RunCustomToolContext {
                user_id: spec.user_id.clone(),
                chat_id: chat_id.clone(),
                character_id: case.character_id.map(str::to_string),
                character_mount_point_id: case.vault.clone(),
                project_id: if room.is_null() {
                    None
                } else {
                    room["projectId"].as_str().map(str::to_string)
                },
                caller_participant_id: Some(
                    room["callerParticipantId"]
                        .as_str()
                        .unwrap_or(P_A)
                        .to_string(),
                ),
                // P4.D169: a case may pin its own instant (`nowMs`), and the jest
                // oracle freezes `Date.now()` to the same value for it. Every
                // clock-independent case takes the shared default, which is what
                // the pre-progression rows have always run against.
                now_ms: case_now_ms(want),
                // The oracle's handler context carries no announcement set (v4's
                // `pendingWardrobeAnnouncements` is the executor's), so neither
                // does this one.
                pending_wardrobe_announcements: None,
                pending_wardrobe_history: None,
                pending_inventory_announcements: None,
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
            let ((out, _posted), lines) =
                rt.block_on(quilltap_core::test_support::global_capture::capture_async(
                    execute_run_custom_tool_with_consult(
                        &db,
                        &ctx,
                        &case.input,
                        &mut rng,
                        Some(&invoker),
                    ),
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
            // P4.D35: WHERE the writes landed, not merely what `pascalMeta` claims.
            let got_stores = if want["stores"].is_null() {
                Value::Null
            } else {
                common::dump_pascal_stores(&db, &chat_id, GROUP, [CHAR_A, CHAR_B, CHAR_C])
            };
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
            assert_eq!(
                canon(&got_stores),
                canon(&want["stores"]),
                "case '{}' state tiers + fact sheets after the run",
                case.name
            );
            // P4.D305: the posted `pascalMeta` as BYTES (key order included).
            assert_eq!(
                pascal_meta_texts(&msgs),
                pascal_meta_texts(want),
                "case '{}' pascalMeta bytes",
                case.name
            );
            // P4.D305: v4's pinned lines, line for line, against v5's capture.
            let want_lines: Vec<String> = want["logs"]
                .as_array()
                .unwrap_or_else(|| panic!("case '{}' oracle row has no `logs`", case.name))
                .iter()
                .map(|l| common::v4_log_as_capture_line("quilltap::pascal", l))
                .collect();
            let got_lines =
                common::pinned_capture_lines(&lines, "quilltap::pascal", &PINNED_MESSAGES);
            assert_eq!(got_lines, want_lines, "case '{}' log lines", case.name);
            if !want_lines.is_empty() {
                logged_rows += 1;
            }
            // P4.D329: a mirror row's inventory lines — the roster's bag DEBUGs and
            // the run service's WARN whole (null fields omitted: `tracing` records
            // no `None`), P4.D328's `[InventoryContext] Loaded` by tag only.
            if case.name.starts_with("measure-twice-") || case.name.starts_with("inventory-read-") {
                let want_inv: Vec<String> = want
                .get("inventoryLines")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|l| match l.get("fields") {
                    None => format!("TAG {}", l["message"].as_str().unwrap_or("")),
                    Some(fields) => {
                        let fields: Map<String, Value> = fields
                            .as_object()
                            .unwrap()
                            .iter()
                            .filter(|(_, v)| !v.is_null())
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        common::v4_log_as_capture_line(
                            "quilltap::pascal",
                            &json!({ "level": l["level"], "message": l["message"], "fields": fields }),
                        )
                    }
                })
                .collect();
                let got_inv: Vec<String> = lines
                .iter()
                .filter_map(|l| {
                    let rest = l.split_once(' ')?.1.split_once(' ')?.1;
                    if rest.starts_with("[InventoryContext] Loaded") {
                        Some("TAG [InventoryContext] Loaded".to_string())
                    } else if (rest.starts_with("Custom-tool gates:") && rest.contains("inventory"))
                        || rest.starts_with("Custom tool run: the rolling character's inventory could not be read")
                    {
                        Some(l.clone())
                    } else {
                        None
                    }
                })
                .collect();
                assert_eq!(got_inv, want_inv, "case '{}' inventory lines", case.name);
            }
        }));
        if let Some(want) = oracle.get(case.name) {
            table_first_lines += want["logs"].as_array().map_or(0, |l| {
                l.iter()
                    .filter(|x| x["message"] == "Custom tool table dealt ahead of the consult")
                    .count()
            });
        }
        checked += 1;
        let red = RED_UNTIL_UNION.contains(&case.name) || P4D332_RESIDUE.contains(&case.name);
        match (walked.is_ok(), red) {
            (false, false) => failed.push(case.name),
            (true, true) => red_green.push(case.name),
            _ => {}
        }
    }

    assert!(failed.is_empty(), "cases differ from v4: {failed:?}");
    assert!(
        red_green.is_empty(),
        "carved cases turned GREEN (P4.D328's loader / P4.D332's roster landed?) — lift them: {red_green:?}"
    );
    assert!(
        inventory_lines_in_oracle >= 20,
        "only {inventory_lines_in_oracle} inventory lines in the mirror rows"
    );
    assert_eq!(checked, cases.len());
    assert!(
        saw_progress_gate_withheld,
        "no case was refused a tool gated on a span still RUNNING"
    );
    assert!(
        saw_progress_write,
        "no case's progress effect actually landed in the character's vault"
    );
    assert!(
        saw_progress_create,
        "no case CREATED a progression nobody had authored"
    );
    assert!(logged_rows > 0, "no case carried a pinned log line");
    assert_eq!(
        plants_applied,
        TOOL_PLANTS.len(),
        "a tool plant went unapplied"
    );
    assert!(
        table_first_lines >= 2,
        "the table-first line went missing ({table_first_lines} pinned)"
    );
    eprintln!("OK: run_custom handler matched oracle ({checked} cases).");
}
