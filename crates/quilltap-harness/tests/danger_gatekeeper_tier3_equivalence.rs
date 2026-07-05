//! W4.2 differential #2: the **CHAT_DANGER_CLASSIFICATION job runner** (v4
//! `handleChatDangerClassification` + `gatekeeper.service.ts`), tier-3 → tier-2.
//!
//! Drives the ported `handle_chat_danger_classification` over a seeded fixture
//! with BOTH model boundaries canned — the cheap-LLM classify path
//! ([`CannedCompletionProvider`], keys replayed from the oracle) and the
//! moderation-provider path (a canned [`ModerationProvider`] mirroring the
//! oracle's `moderationProviderRegistry` mock, incl. a provider failure) — then
//! diffs `chats` + `chat_messages` against v4's REAL handler. Minted timestamps
//! are sentinel-aware (`updatedAt` / `dangerClassifiedAt` stay at the seed
//! `2020` sentinel on the moderation/skip paths — proving no bump — and
//! placeholder to `<ts>` when the LLM path's system-event token aggregate mints
//! a fresh value); the minted system-event id + createdAt are placeholdered.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-danger-gatekeeper.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-gatekeeper-fixture.ts
//!   TMPO=/tmp/qt-danger-oracle; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/danger-gatekeeper.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/danger-gatekeeper.json $TMPO/fixtures/
//!   QT_FIXTURE_GATEKEEPER=/tmp/qt-danger-gatekeeper.db QT_ORACLE_OUT=/tmp/oracle-danger-gatekeeper.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- danger-gatekeeper
//! Run:
//!   QT_ORACLE_DANGER_GATEKEEPER=/tmp/oracle-danger-gatekeeper.ndjson \
//!   QT_FIXTURE_GATEKEEPER=/tmp/qt-danger-gatekeeper.db \
//!     cargo test -p quilltap-harness --test danger_gatekeeper_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    canned_completion_key, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole, CompletionUsage,
};
use quilltap_core::services::dangerous_content::gatekeeper::{
    ModerationCategoryScore, ModerationOutcome, ModerationProvider, ModerationResult,
};
use quilltap_core::services::dangerous_content::gatekeeper_job::{
    handle_chat_danger_classification, ChatDangerClassificationJob, NoDangerAnnouncer,
};
use serde::Deserialize;
use serde_json::Value;

const SEED_TS: &str = "2020-01-01T00:00:00.000Z";

// --- spec ---
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userA")]
    user_a: String,
    #[serde(rename = "userB")]
    user_b: String,
    #[serde(rename = "moderationResults")]
    moderation_results: HashMap<String, Value>,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct CaseSpec {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    user: String,
    #[serde(rename = "connectionProfileId")]
    connection_profile_id: String,
}

// --- canned completion provider (replays the oracle-recorded keys) ---
#[derive(Deserialize)]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsg>,
    response: String,
    usage: CannedUsage,
}
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

struct CannedCompletion(HashMap<String, CompletionResponse>);
impl CompletionProvider for CannedCompletion {
    fn send_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let key = canned_completion_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        );
        let result = self
            .0
            .get(&key)
            .cloned()
            .ok_or_else(|| CompletionError::new(format!("no canned completion for key {key}")));
        async move { result }
    }
}

fn role_of(s: &str) -> CompletionRole {
    match s {
        "system" => CompletionRole::System,
        "user" => CompletionRole::User,
        "assistant" => CompletionRole::Assistant,
        "tool" => CompletionRole::Tool,
        other => panic!("unknown role {other}"),
    }
}

// --- canned moderation provider (mirrors the oracle's registry mock) ---
enum ModOutcome {
    Result(ModerationResult),
    Fail,
}
struct CannedModeration(HashMap<String, ModOutcome>);
impl ModerationProvider for CannedModeration {
    fn moderate(
        &self,
        content: &str,
        _user_id: &str,
        _settings: &quilltap_core::db::chat_settings::DangerousContentSettings,
        _chat_id: Option<&str>,
    ) -> impl std::future::Future<Output = ModerationOutcome> + Send {
        // Key on the `token=<case>` marker in the classification content.
        let token = content
            .split("token=")
            .nth(1)
            .map(|s| s.split_whitespace().next().unwrap_or("").to_string());
        let outcome = match token.as_deref().and_then(|t| self.0.get(t)) {
            Some(ModOutcome::Result(r)) => ModerationOutcome::Moderated {
                result: r.clone(),
                provider_name: "OPENAI".to_string(),
            },
            Some(ModOutcome::Fail) => {
                ModerationOutcome::Failed("moderation provider exploded".into())
            }
            None => ModerationOutcome::NotAvailable,
        };
        async move { outcome }
    }
}

fn parse_moderation(spec: &Spec) -> HashMap<String, ModOutcome> {
    let mut map = HashMap::new();
    for (name, v) in &spec.moderation_results {
        if v.as_str() == Some("fail") {
            map.insert(name.clone(), ModOutcome::Fail);
        } else {
            let flagged = v.get("flagged").and_then(Value::as_bool).unwrap_or(false);
            let categories = v
                .get("categories")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|c| ModerationCategoryScore {
                            category: c
                                .get("category")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .into(),
                            score: c.get("score").and_then(Value::as_f64).unwrap_or(0.0),
                        })
                        .collect()
                })
                .unwrap_or_default();
            map.insert(
                name.clone(),
                ModOutcome::Result(ModerationResult {
                    flagged,
                    categories,
                }),
            );
        }
    }
    map
}

// --- numeric + timestamp normalization ---
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if n.is_f64() {
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f.abs() < 9.007e15 {
                        *v = Value::Number(serde_json::Number::from(f as i64));
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

/// Sentinel-aware timestamp placeholder on the chats row: `updatedAt` /
/// `dangerClassifiedAt` that differ from the `2020` seed sentinel are minted →
/// `<ts>` (proving the bump); equal-to-seed values stay (proving no bump).
fn normalize_chats(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            for col in ["updatedAt", "dangerClassifiedAt"] {
                let minted = obj
                    .get(col)
                    .and_then(Value::as_str)
                    .map(|s| s != SEED_TS)
                    .unwrap_or(false);
                if minted {
                    obj.insert(col.into(), Value::String("<ts>".into()));
                }
            }
        }
        canon_numbers(row);
    }
}

/// The minted system-event `id` + `createdAt` are placeholdered (not referenced
/// elsewhere); every other column (chatId / type / systemEventType / description
/// / token counts / provider / modelName) is diffed exactly.
fn normalize_messages(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            if obj.contains_key("id") {
                obj.insert("id".into(), Value::String("<id>".into()));
            }
            if obj.get("createdAt").map(|v| !v.is_null()) == Some(true) {
                obj.insert("createdAt".into(), Value::String("<ts>".into()));
            }
        }
        canon_numbers(row);
    }
}

fn oracle_table<'a>(lines: &'a [Value], table: &str) -> &'a Value {
    lines
        .iter()
        .find(|v| v.get("table").and_then(Value::as_str) == Some(table))
        .unwrap_or_else(|| panic!("oracle missing table {table}"))
}

#[tokio::test]
async fn danger_gatekeeper_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DANGER_GATEKEEPER") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_GATEKEEPER to the tier-3 NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_GATEKEEPER") else {
        eprintln!("SKIP: set QT_FIXTURE_GATEKEEPER to the seed fixture .db (see header).");
        return;
    };

    let spec_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/danger-gatekeeper.json");
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse gatekeeper spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Build the canned completion provider from the recorded rows + collect the
    // two table dumps.
    let mut completions: HashMap<String, CompletionResponse> = HashMap::new();
    let mut oracle_tables: Vec<Value> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("canned") => {
                let row: CannedRow = serde_json::from_value(v).expect("parse canned row");
                let messages: Vec<CompletionMessage> = row
                    .messages
                    .iter()
                    .map(|m| CompletionMessage {
                        role: role_of(&m.role),
                        content: m.content.clone(),
                    })
                    .collect();
                let key =
                    canned_completion_key(&row.provider, &row.model, row.temperature, &messages);
                completions.insert(
                    key,
                    CompletionResponse {
                        content: row.response,
                        usage: Some(CompletionUsage {
                            prompt_tokens: row.usage.prompt_tokens,
                            completion_tokens: row.usage.completion_tokens,
                            total_tokens: row.usage.total_tokens,
                        }),
                    },
                );
            }
            Some("table") => oracle_tables.push(v),
            _ => {}
        }
    }

    let completion = CannedCompletion(completions);
    let moderation = CannedModeration(parse_moderation(&spec));
    let uid = |u: &str| {
        if u == "A" {
            spec.user_a.clone()
        } else {
            spec.user_b.clone()
        }
    };

    let work = std::env::temp_dir().join(format!(
        "qt-danger-gatekeeper-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for c in &spec.cases {
        let job = ChatDangerClassificationJob {
            id: format!("job-{}", c.name),
            user_id: uid(&c.user),
            chat_id: c.chat_id.clone(),
            connection_profile_id: c.connection_profile_id.clone(),
        };
        handle_chat_danger_classification(&db, &moderation, &completion, &NoDangerAnnouncer, &job)
            .await
            .unwrap_or_else(|e| panic!("gatekeeper {}: {e:?}", c.name));
    }

    let mut got_chats = db
        .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    let mut got_messages = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);
    let _ = std::fs::remove_file(&work);

    let mut want_chats = oracle_table(&oracle_tables, "chats").clone();
    let mut want_messages = oracle_table(&oracle_tables, "chat_messages").clone();

    {
        let g = got_chats
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        normalize_chats(g);
        let w = want_chats
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        normalize_chats(w);
        assert_eq!(g, w, "chats rows diverge");
    }
    {
        let g = got_messages
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        normalize_messages(g);
        let w = want_messages
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        normalize_messages(w);
        assert_eq!(g, w, "chat_messages rows diverge");
    }

    eprintln!("OK: danger gatekeeper chats + chat_messages matched oracle.");
}
