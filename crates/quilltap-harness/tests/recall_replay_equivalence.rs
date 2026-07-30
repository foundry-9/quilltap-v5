//! Tier-3 differential: the recall-replay verb (P4.d13 — episodic recall §3).
//!
//! Drives v5's REAL `api::recall_replay::recall_replay` (body coercion →
//! settings/anchor resolution → `run_recall_replay` → `search_memories_semantic`
//! TWICE, multiplier loop and all) over a fresh copy of the committed
//! `episodic-recall-{main,mount}.db` fixture per case, against the NDJSON the
//! v4 oracle (`harness/oracle/cases/recall-replay.test.ts`, jest real-DB —
//! ONLY `executeCheapLLMTask` mocked) emitted from the SAME fixture copies.
//!
//! Embeddings run REAL on both sides through the fixture's fitted BUILTIN
//! TF-IDF profile; the canned distill text runs through each side's REAL parse.
//! Floats compare at 1e-12 (the TF-IDF vocab doubles come from the same JSON on
//! both sides; cosines accumulate in doubles over the same stored f32 bits).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; STAGE the case
//! outside `.claude/` — v4's jest ignores those paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   STAGE=/tmp/qt-oracle-stage
//!   mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $W/harness/oracle/cases/recall-replay.test.ts $STAGE/harness/oracle/cases/
//!   cp $W/harness/oracle/fixtures/{recall-replay-cases,episodic-recall}.json \
//!      $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ER_MAIN=$W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db \
//!   QT_FIXTURE_ER_MOUNT=$W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-episodic-recall-fixture.ts
//!   TZ=UTC QT_FIXTURE_ER_MAIN=... QT_FIXTURE_ER_MOUNT=... \
//!   QT_ORACLE_OUT=/tmp/oracle-recall-replay.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- recall-replay
//! Run:
//!   QT_ORACLE_RECALL_REPLAY=/tmp/oracle-recall-replay.ndjson \
//!     cargo test -p quilltap-harness --test recall_replay_equivalence

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use quilltap_core::api::recall_replay::{recall_replay, RecallReplayDriver};
use quilltap_core::api::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::recall_replay::{run_recall_replay, RunRecallReplayInput};
use serde::Deserialize;
use serde_json::Value;

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
    body: Value,
    distill: DistillSpec,
    #[serde(default)]
    delete_chat_settings: bool,
    #[serde(default)]
    delete_connections: bool,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "$nowMs")]
    now_ms: f64,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "userId")]
    user_id: String,
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
    status: i64,
    body: Value,
}

/// A completion provider that always answers the canned text (the distill is
/// the only completion this surface makes) or always fails.
struct CannedDistillProvider {
    response: Option<String>,
}

impl CompletionProvider for CannedDistillProvider {
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
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

/// The harness "host": runs the REAL core module with the canned distill
/// provider + the production builtin-TF-IDF embedding path over the fixture.
struct TestDriver {
    db: Db,
    response: Option<String>,
}

impl RecallReplayDriver for TestDriver {
    fn run(
        &self,
        input: RunRecallReplayInput,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>> {
        Box::pin(async move {
            let completion = CannedDistillProvider {
                response: self.response.clone(),
            };
            let executor = CheapLlmTaskExecutor::new();
            let embedding = ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
                self.db.clone(),
                CannedWireTransport::new(),
            ));
            // TZ=UTC on the oracle side (the process zone v4's resolver reads).
            run_recall_replay(
                &self.db,
                &completion,
                &executor,
                &embedding,
                &input,
                Some("UTC"),
            )
            .await
        })
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn status_of(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        ErrorKind::Internal => 500,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
    }
}

/// Recursive equality with float tolerance (everything else exact).
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
            let ka: Vec<&String> = a.keys().collect();
            let kb: Vec<&String> = b.keys().collect();
            assert_eq!(ka, kb, "{name}: {path} key set/order");
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

#[tokio::test]
async fn recall_replay_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_RECALL_REPLAY") else {
        eprintln!("QT_ORACLE_RECALL_REPLAY not set — skipping recall-replay differential");
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
        "/../../harness/oracle/fixtures/recall-replay-cases.json"
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
        let main = std::env::temp_dir().join(format!("qt-replay-{pid}-{name}-main.db"));
        let mount = std::env::temp_dir().join(format!("qt-replay-{pid}-{name}-mount.db"));
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

        if case.delete_chat_settings {
            db.write(|w| {
                w.main()
                    .connection()
                    .execute("DELETE FROM chat_settings", [])
                    .map_err(quilltap_core::db::DbError::from)
                    .map(|_| ())
            })
            .await
            .expect("delete chat settings");
        }
        if case.delete_connections {
            db.write(|w| {
                w.main()
                    .connection()
                    .execute("DELETE FROM connection_profiles", [])
                    .map_err(quilltap_core::db::DbError::from)
                    .map(|_| ())
            })
            .await
            .expect("delete connections");
        }

        let driver: Arc<dyn RecallReplayDriver> = Arc::new(TestDriver {
            db: db.clone(),
            response: match case.distill.kind.as_str() {
                "json" => Some(case.distill.text.clone().unwrap_or_default()),
                _ => None,
            },
        });

        let response = recall_replay(
            &db,
            Some(&driver),
            &spec.user_id,
            &spec.chat_id,
            case.body.get("turnIndex"),
            case.body.get("characterId"),
            case.body.get("limit"),
            spec.now_ms,
        )
        .await;

        match &response {
            Response::RecallReplay(result) => {
                assert_eq!(oracle.status, 200, "{name}: oracle expected an error");
                // Reparse through serde so both sides share the same
                // float-parse canonicalization (the build-context precedent).
                let text = serde_json::to_string(result).unwrap();
                let got: Value = serde_json::from_str(&text).unwrap();
                assert_value_eq(&got, &oracle.body, "", name);
            }
            Response::Error(e) => {
                assert_eq!(
                    status_of(e.kind),
                    oracle.status,
                    "{name}: status diverges (message: {})",
                    e.message
                );
                let want_error = oracle
                    .body
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                assert_eq!(e.message, want_error, "{name}: error message diverges");
            }
            other => panic!("{name}: unexpected response variant: {other:?}"),
        }

        drop(db);
        let _ = std::fs::remove_file(&main);
        let _ = std::fs::remove_file(&mount);
    }
    println!(
        "recall_replay_equivalence: {} cases green",
        oracle_rows.len()
    );
}
