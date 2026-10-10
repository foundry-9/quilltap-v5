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
//! P4.D267 (v4 `f7d8064be` / `3f7320138` / `d58548051`, pin `01a83539d`): the
//! R7 harness fields (`memoryBudget`, strict `tuning`, pinned `signals`,
//! `asOf`) — each case body is decoded through the REAL dispatch `Request`
//! (so a JSON `null` reaches the route as v4 sees it); a validation 400's
//! `details` are byte-compared; the distill-call count is compared (a pinned
//! case makes none); the route's two `[Chats v1]` lines, the RecallReplay
//! service's three and the search's `[Memory]` lines are compared (v4's
//! transports splice / v5's `global_capture`). Each copy gets the tier
//! migration on both sides (v4 `applyMemoryTiersMigration`, v5
//! `ensure_memory_tiers_columns` — idempotent on the widened pair), then the
//! case's `plantSql`.
//!
//! Generate the oracle (Node 24, from the v4 checkout; STAGE the case outside
//! `.claude/` — v4's jest ignores those paths), against /tmp COPIES of the
//! COMMITTED episodic-recall fixture. Do NOT run
//! `build-episodic-recall-fixture.ts` pointed at the committed `.db` files —
//! that rebuilds them in place, mints fresh UUIDs, and invalidates every
//! episodic family (exactly the P4.D32 sweep overwrite); a fixture rebuild is
//! a deliberate act that re-runs every consumer, never part of oracle regen:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-recall-replay-stage
//!   mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures $STAGE/db
//!   cp $V5W/harness/oracle/cases/recall-replay.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/recall-replay-cases.json $STAGE/harness/oracle/fixtures/
//!   cp $V5W/harness/oracle/fixtures/episodic-recall.json $STAGE/harness/oracle/fixtures/
//!   cp $V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts $STAGE/harness/oracle/fixtures/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db  $STAGE/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db $STAGE/db/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_ER_MAIN=$STAGE/db/episodic-recall-main.db \
//!   QT_FIXTURE_ER_MOUNT=$STAGE/db/episodic-recall-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-recall-replay.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "harness/oracle/cases/recall-replay\.test\.ts$"
//! Run:
//!   QT_ORACLE_RECALL_REPLAY=/tmp/oracle-recall-replay.ndjson \
//!     cargo test -p quilltap-harness --test recall_replay_equivalence

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering};

use quilltap_core::api::recall_replay::{recall_replay, RecallReplayArgs, RecallReplayDriver};
use quilltap_core::api::{ErrorKind, Request, Response};
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
    #[serde(default)]
    plant_sql: Option<String>,
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
    #[serde(rename = "distillCalls")]
    distill_calls: usize,
    lines: Vec<Value>,
}

/// The lines this family pins (v4's exact messages), each with the v5 target
/// that stands for v4's logger (R-M: the RecallReplay service logger's
/// `{service, module}` context is the target).
const PINNED_LINES: [(&str, &str); 8] = [
    ("[Chats v1] Recall replay requested", "quilltap::chats"),
    ("[Chats v1] Recall replay failed", "quilltap::chats"),
    (
        "Recall replay using pinned signals",
        "quilltap::recall_replay",
    ),
    ("Recall replay corpus cutoff", "quilltap::recall_replay"),
    ("Recall replay complete", "quilltap::recall_replay"),
    (
        "[Memory] TF-IDF embeddings: recall boost gate off",
        "quilltap::memories",
    ),
    (
        "[Memory] Specific entity anchors chosen",
        "quilltap::memories",
    ),
    ("[Memory] Background reservation (R6)", "quilltap::memories"),
];

/// v4's captured line as v5's capture rig renders it: `LEVEL target message
/// k=v …`, `module` dropped (it is the target), arrays / objects as `…Json`,
/// numbers as JS prints them, `null` as the house `null`.
fn expected_line(line: &Value) -> String {
    let message = line["message"].as_str().unwrap();
    let target = PINNED_LINES
        .iter()
        .find(|(m, _)| *m == message)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("unpinned oracle line {message}"));
    let mut out = format!(
        "{} {target} {message}",
        line["level"].as_str().unwrap().to_ascii_uppercase()
    );
    for (k, v) in line["fields"].as_object().unwrap() {
        if k == "module" {
            continue;
        }
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            Value::Array(_) | Value::Object(_) => {
                out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            Value::Number(n) => out.push_str(&format!(
                " {k}={}",
                quilltap_core::pascal::js_value::number_to_string(n.as_f64().unwrap())
            )),
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

/// A completion provider that always answers the canned text (the distill is
/// the only completion this surface makes) or always fails.
struct CannedDistillProvider {
    response: Option<String>,
    calls: Arc<AtomicUsize>,
}

impl CompletionProvider for CannedDistillProvider {
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
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
                    attachment_results: None,
                    cache_usage: None,
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
    calls: Arc<AtomicUsize>,
}

impl RecallReplayDriver for TestDriver {
    fn run(
        &self,
        input: RunRecallReplayInput,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + '_>> {
        Box::pin(async move {
            let completion = CannedDistillProvider {
                response: self.response.clone(),
                calls: self.calls.clone(),
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
        // The store-unavailable refusal (P4.23) — v4's deliberate
        // contextful 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
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
    quilltap_core::test_support::global_capture::install();
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

        // Fresh copies (the search bumps lastAccessedAt), in a per-case scratch
        // dir deleted (journals included) when it drops at the end of the case.
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-replay-{name}-"))
            .tempdir()
            .expect("tempdir");
        let main = scratch.path().join("main.db");
        let mount = scratch.path().join("mount.db");
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

        // §R.3: the tier migration on the copy (a no-op on the widened pair),
        // then the case's plant — identically on the oracle side.
        db.write(|w| {
            quilltap_core::test_support::ensure_memory_tiers_columns(w.main().connection());
            Ok(())
        })
        .await
        .expect("ensure the tier columns");
        if let Some(sql) = case.plant_sql.clone() {
            db.write(move |w| {
                w.main()
                    .connection()
                    .execute_batch(&sql)
                    .map_err(quilltap_core::db::DbError::from)
            })
            .await
            .expect("plant");
        }
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

        let calls = Arc::new(AtomicUsize::new(0));
        let driver: Arc<dyn RecallReplayDriver> = Arc::new(TestDriver {
            db: db.clone(),
            response: match case.distill.kind.as_str() {
                "json" => Some(case.distill.text.clone().unwrap_or_default()),
                _ => None,
            },
            calls: calls.clone(),
        });

        // The body through the REAL dispatch decode — the carriers the arm
        // hands the route (`double_option` keeps a JSON `null` present).
        let mut wire = serde_json::json!({"type": "chatRecallReplay", "chatId": spec.chat_id});
        for (k, v) in case.body.as_object().unwrap() {
            wire[k] = v.clone();
        }
        let Request::ChatRecallReplay {
            turn_index,
            character_id,
            limit,
            memory_budget,
            tuning,
            signals,
            as_of,
            ..
        } = serde_json::from_value::<Request>(wire).expect("decode the case body")
        else {
            panic!("{name}: not a chatRecallReplay");
        };
        let (response, lines) =
            quilltap_core::test_support::global_capture::capture_async(recall_replay(
                &db,
                Some(&driver),
                &spec.user_id,
                &spec.chat_id,
                RecallReplayArgs {
                    turn_index: turn_index.as_ref(),
                    character_id: character_id.as_ref(),
                    limit: limit.as_ref(),
                    memory_budget: memory_budget.as_ref(),
                    tuning: tuning.as_ref().map(Option::as_ref),
                    signals: signals.as_ref().map(Option::as_ref),
                    as_of: as_of.as_ref().map(Option::as_ref),
                },
                spec.now_ms,
            ))
            .await;

        assert_eq!(
            calls.load(Ordering::SeqCst),
            oracle.distill_calls,
            "{name}: distill calls"
        );
        let got_lines: Vec<&str> = lines
            .iter()
            .map(String::as_str)
            .filter(|l| {
                PINNED_LINES.iter().any(|(m, t)| {
                    l.split_once(' ')
                        .is_some_and(|(_, rest)| rest.starts_with(&format!("{t} {m}")))
                })
            })
            .collect();
        let want_lines: Vec<String> = oracle.lines.iter().map(expected_line).collect();
        assert_eq!(
            got_lines,
            want_lines.iter().map(String::as_str).collect::<Vec<_>>(),
            "{name}: captured lines"
        );

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
                // A validation 400's issues, byte for byte (key order is a
                // comparand — serde_json's `Map` equality is order-blind).
                let got_details = e
                    .details
                    .as_ref()
                    .map(|d| serde_json::to_string(d).unwrap());
                let want_details = oracle
                    .body
                    .get("details")
                    .map(|d| serde_json::to_string(d).unwrap());
                assert_eq!(got_details, want_details, "{name}: details diverge");
            }
            other => panic!("{name}: unexpected response variant: {other:?}"),
        }

        drop(db);
    }
    println!(
        "recall_replay_equivalence: {} cases green",
        oracle_rows.len()
    );
}
