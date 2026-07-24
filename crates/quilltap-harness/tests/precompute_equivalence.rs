//! Tier-3 differential: the proactive pre-compute distill (P4.19 —
//! `proactiveRecallTask`).
//!
//! Drives v5's REAL `quilltap_core::services::pre_compute::proactive_recall_task`
//! (windowing → cheap-LLM keyword distill parse → `search_memories_semantic`,
//! recall-context multiplier loop and all) over a fresh copy of the committed
//! `episodic-recall-{main,mount}.db` fixture per case, against the NDJSON the v4
//! oracle (`harness/oracle/cases/precompute.test.ts`, jest real-DB — ONLY
//! `executeCheapLLMTask` mocked, driving the REAL `runPreContextPreCompute`)
//! emitted from the SAME fixture copies.
//!
//! Embeddings run REAL on both sides through the fixture's fitted BUILTIN TF-IDF
//! profile; the canned distill text runs through each side's REAL parse. Floats
//! compare at 1e-12. Elowen is the searched character (7 memories); Pip has none
//! (the search-empty-signals-flow arm). The whole outcome is diffed:
//! `preSearchedMemories` (id/order + score/effectiveWeight/rawWeight/
//! recallAdjustment — the recall-context multipliers prove the assembled search
//! invocation transitively) and `recallSignals` (the parsed distill).
//!
//! Cases: never-spoke / continue-mode-empty-window / extraction-fail → both
//! `null`; spoke-then-window / dangerous-reroute / retrospective-multi-probe /
//! multi-character → memories + signals; search-empty → `null` memories + signals
//! (signals still flow). The 10-cap + the pure windowing/guard cases are the unit
//! specs in `pre_compute.rs`; the uncensored reroute is the tier-1 differentialed
//! `resolve_uncensored_cheap_llm_selection`.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; STAGE the case
//! outside `.claude/` — v4's jest ignores those paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   STAGE=/tmp/qt-oracle-stage
//!   mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $W/harness/oracle/cases/precompute.test.ts $STAGE/harness/oracle/cases/
//!   cp $W/harness/oracle/fixtures/{precompute-cases,episodic-recall}.json \
//!      $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ER_MAIN=$W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db \
//!   QT_FIXTURE_ER_MOUNT=$W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-episodic-recall-fixture.ts
//!   TZ=UTC QT_FIXTURE_ER_MAIN=... QT_FIXTURE_ER_MOUNT=... \
//!   QT_ORACLE_OUT=/tmp/oracle-precompute.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- precompute
//! Run:
//!   QT_ORACLE_PRECOMPUTE=/tmp/oracle-precompute.ndjson \
//!     cargo test -p quilltap-harness --test precompute_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::cheap_llm::{CheapLlmSelection, DangerousContentSettings};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::memory_service::SemanticSearchResult;
use quilltap_core::services::pre_compute::{proactive_recall_task, ProactiveRecallInput};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const EPS: f64 = 1e-12;

#[derive(Deserialize)]
struct DistillSpec {
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    name: String,
    character_id: String,
    character_name: String,
    character_participant_id: String,
    present_about_character_ids: Vec<String>,
    is_continue_mode: bool,
    content: String,
    chat: Value,
    #[serde(default)]
    danger_settings: Option<Value>,
    existing_messages: Vec<Value>,
    distill: DistillSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    #[serde(rename = "$nowMs")]
    now_ms: f64,
    user_id: String,
    cheap_selection: Value,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct FixtureSpec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    #[serde(rename = "preSearchedMemories")]
    pre_searched_memories: Value,
    #[serde(rename = "recallSignals")]
    recall_signals: Value,
}

/// A completion provider that always answers the canned distill text or always
/// fails (the distill is the only completion the proactive task makes).
struct CannedDistillProvider {
    response: Option<String>,
}

impl CompletionProvider for CannedDistillProvider {
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let response = self.response.clone();
        async move {
            match response {
                Some(text) => Ok(CompletionResponse {
                    content: text,
                    usage: Some(CompletionUsage {
                        prompt_tokens: 100,
                        completion_tokens: 20,
                        total_tokens: 120,
                    }),
                    finish_reason: Some("stop".to_string()),
                }),
                None => Err(CompletionError::new("canned failure")),
            }
        }
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// Normalize v5's `SemanticSearchResult` list to the oracle's emitted shape.
fn normalize_memories(results: &[SemanticSearchResult]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|r| {
                let recall_adjustment = match &r.recall_adjustment {
                    Some(a) => json!({
                        "multiplier": a.multiplier,
                        "fired": a.fired,
                        "blendedBefore": a.blended_before,
                        "blendedAfter": a.blended_after,
                    }),
                    None => Value::Null,
                };
                json!({
                    "id": r.memory.get("id").cloned().unwrap_or(Value::Null),
                    "score": r.score,
                    "usedEmbedding": r.used_embedding,
                    "effectiveWeight": r.effective_weight,
                    "rawWeight": r.raw_weight,
                    "recallAdjustment": recall_adjustment,
                })
            })
            .collect(),
    )
}

/// Recursive equality: floats at 1e-12; objects compared by key SET (order-
/// insensitive — this is an internal outcome, not a wire contract); arrays exact.
fn assert_value_eq(got: &Value, want: &Value, path: &str, name: &str) {
    match (got, want) {
        (Value::Number(a), Value::Number(b)) => {
            let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) else {
                assert_eq!(got, want, "{name}: {path} number kind");
                return;
            };
            assert!(
                (a - b).abs() <= EPS,
                "{name}: {path} float diverges: rust={a} oracle={b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut ka: Vec<&String> = a.keys().collect();
            let mut kb: Vec<&String> = b.keys().collect();
            ka.sort();
            kb.sort();
            assert_eq!(ka, kb, "{name}: {path} key set");
            for (k, va) in a {
                assert_value_eq(va, &b[k], &format!("{path}/{k}"), name);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name}: {path} array length");
            for (i, (va, vb)) in a.iter().zip(b).enumerate() {
                assert_value_eq(va, vb, &format!("{path}[{i}]"), name);
            }
        }
        _ => assert_eq!(got, want, "{name}: {path} diverges"),
    }
}

fn selection_from(v: &Value) -> CheapLlmSelection {
    CheapLlmSelection {
        provider: v
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: None,
        connection_profile_id: None,
        is_local: v.get("isLocal").and_then(Value::as_bool).unwrap_or(false),
        profile_parameters: None,
    }
}

fn danger_from(v: Option<&Value>) -> DangerousContentSettings {
    match v {
        Some(d) => DangerousContentSettings {
            mode: d
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("OFF")
                .to_string(),
            uncensored_text_profile_id: d
                .get("uncensoredTextProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        None => DangerousContentSettings {
            mode: "OFF".to_string(),
            uncensored_text_profile_id: None,
        },
    }
}

#[tokio::test]
async fn precompute_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PRECOMPUTE") else {
        eprintln!("QT_ORACLE_PRECOMPUTE not set — skipping precompute differential");
        return;
    };
    let oracle_raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let oracle_rows: Vec<OracleRow> = oracle_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let spec_raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/precompute-cases.json"
    ))
    .expect("read cases spec");
    let spec: Spec = serde_json::from_str(&spec_raw).expect("parse cases spec");
    let fixture_spec: FixtureSpec = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/episodic-recall.json"
        ))
        .expect("read fixture spec"),
    )
    .expect("parse fixture spec");

    assert_eq!(
        spec.cases.len(),
        oracle_rows.len(),
        "corpus/oracle case-count mismatch — regenerate the oracle NDJSON"
    );

    for (case, oracle) in spec.cases.iter().zip(&oracle_rows) {
        assert_eq!(case.name, oracle.name, "case order mismatch");
        let name = &case.name;

        // Fresh copies (the search bumps lastAccessedAt).
        let pid = std::process::id();
        let main = std::env::temp_dir().join(format!("qt-precompute-{pid}-{name}-main.db"));
        let mount = std::env::temp_dir().join(format!("qt-precompute-{pid}-{name}-mount.db"));
        let _ = std::fs::remove_file(&main);
        let _ = std::fs::remove_file(&mount);
        std::fs::copy(fixtures_dir().join("episodic-recall-main.db"), &main).unwrap();
        std::fs::copy(fixtures_dir().join("episodic-recall-mount.db"), &mount).unwrap();

        let db = Db::open(
            DbPaths {
                main: main.clone(),
                mount_index: Some(mount.clone()),
                llm_logs: None,
            },
            &fixture_spec.test_pepper_base64,
        )
        .expect("open fixture copy");

        let completion = CannedDistillProvider {
            response: match case.distill.kind.as_str() {
                "json" => Some(case.distill.text.clone().unwrap_or_default()),
                _ => None,
            },
        };
        let executor = CheapLlmTaskExecutor::new();
        let embedding = ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
            db.clone(),
            CannedWireTransport::new(),
        ));

        let selection = selection_from(&spec.cheap_selection);
        let danger = danger_from(case.danger_settings.as_ref());
        let input = ProactiveRecallInput {
            chat: &case.chat,
            character_id: &case.character_id,
            character_name: &case.character_name,
            character_participant_id: &case.character_participant_id,
            present_about_character_ids: &case.present_about_character_ids,
            is_continue_mode: case.is_continue_mode,
            content: &case.content,
            existing_messages: &case.existing_messages,
            cheap_llm_selection: Some(&selection),
            danger_settings: &danger,
            available_profiles: &[],
            user_id: &spec.user_id,
            chat_id: case.chat.get("id").and_then(Value::as_str).unwrap_or("chat"),
            now_ms: spec.now_ms as i64,
        };

        let mut emit = |_stage: &str, _msg: String| {};
        let outcome =
            proactive_recall_task(&db, &embedding, &completion, &executor, &input, &mut emit).await;

        // Reparse the memories through serde so both sides share the same
        // float-parse canonicalization (the build-context precedent).
        let (mems, sigs) = match outcome {
            Some(o) => (o.memories, o.signals),
            None => (None, None),
        };
        let mut got = Map::new();
        got.insert(
            "preSearchedMemories".to_string(),
            match &mems {
                Some(v) => {
                    let text = serde_json::to_string(&normalize_memories(v)).unwrap();
                    serde_json::from_str(&text).unwrap()
                }
                None => Value::Null,
            },
        );
        got.insert(
            "recallSignals".to_string(),
            match &sigs {
                Some(s) => s.signals_json(),
                None => Value::Null,
            },
        );
        let got = Value::Object(got);

        let want = json!({
            "preSearchedMemories": oracle.pre_searched_memories,
            "recallSignals": oracle.recall_signals,
        });

        assert_value_eq(&got, &want, "", name);
        println!("precompute_equivalence: {name} OK");

        drop(db);
        let _ = std::fs::remove_file(&main);
        let _ = std::fs::remove_file(&mount);
    }
    println!("precompute_equivalence: {} cases green", oracle_rows.len());
}
