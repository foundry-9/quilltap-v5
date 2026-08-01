//! Tier-3 differential test: the **Brahma one-shot console**
//! (`quilltap_core::services::brahma_console::run_brahma_query` — v4
//! `runBrahmaQuery`, lib/services/brahma-console/one-shot.service.ts), the
//! isolated operator console the Carina engine invokes when the answerer is
//! Brahma (closing the `RunBrahmaConsole` seam left by W4.5).
//!
//! Both sides pin the model boundaries identically: the streamed calls by the
//! exact `provider|model|temperature|messages` key (the jest oracle scripts each
//! real call and RECORDS the key it answered — so the system-prompt bytes incl.
//! `BRAHMA_SQL_PROMPT` + the tool instructions, and the tool-result threading, are
//! proven by the replay), and tool detection by the raw-response `marker`. Tool
//! EXECUTION is REAL on both sides: `run_sql` runs an actual SELECT over the
//! fixture through the real `BuiltInToolRunner`, its byte-exact result threading
//! into the continuation stream (a divergence → a canned-miss → an `llm-failed`
//! result diverging from v4). The console never persists, so the diff is the
//! `BrahmaConsoleResult` per case (no table dumps).
//!
//! Cases: no-profile; the two api-key detail strings; a plain answer; submit via
//! tool args AND via the raw-text fallback; empty → 'empty response'; a `run_sql`
//! iteration (real SELECT + continuation); and the duplicate-call stuck-loop
//! guard (the byte-exact nudge is proven by the 4th continuation's canned key).
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout — the
//! oracle lives under `.claude/`, which jest ignores, so mirror it to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-brahma-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-brahma-mount.db \
//!     $N/npx tsx $W/harness/oracle/fixtures/build-brahma-console-fixture.ts
//!   mkdir -p /tmp/brahma-oracle/cases /tmp/brahma-oracle/fixtures
//!   cp $W/harness/oracle/cases/brahma-console-tier3.test.ts /tmp/brahma-oracle/cases/
//!   cp $W/harness/oracle/fixtures/brahma-console-tier3.json /tmp/brahma-oracle/fixtures/
//!   QT_FIXTURE_BRAHMA_MAIN=/tmp/qt-brahma-main.db QT_FIXTURE_BRAHMA_MOUNT=/tmp/qt-brahma-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-brahma.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/brahma-oracle/cases" -- brahma-console-tier3
//! Run:
//!   QT_ORACLE_BRAHMA=/tmp/oracle-brahma.ndjson \
//!   QT_FIXTURE_BRAHMA_MAIN=/tmp/qt-brahma-main.db QT_FIXTURE_BRAHMA_MOUNT=/tmp/qt-brahma-mount.db \
//!     cargo test -p quilltap-harness --test brahma_console_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::brahma_console::{run_brahma_query, BrahmaQueryDeps};
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::tool_execution::ToolCall;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Spec (harness/oracle/fixtures/brahma-console-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectionCall {
    name: String,
    arguments: Value,
    #[serde(default)]
    call_id: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    user_id: String,
    question: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    chat_id: String,
    detection: HashMap<String, Vec<DetectionCall>>,
    cases: Vec<CaseW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/brahma-console-tier3.json")
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChunkW {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    raw_response: Option<Value>,
}

#[derive(Deserialize)]
struct CannedStreamW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    sequences: Vec<Vec<ChunkW>>,
}

fn to_completion_messages(m: &[CannedMsgW]) -> Vec<CompletionMessage> {
    m.iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                "tool" => CompletionRole::Tool,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect()
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if c.done == Some(true) {
        let mut chunk = StreamChunk::done(None::<StreamUsage>);
        chunk.raw_response = c.raw_response.clone();
        return Ok(chunk);
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue of sequences).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        for row in rows {
            let messages = to_completion_messages(&row.messages);
            let key = canned_stream_key(&row.provider, &row.model, row.temperature, &messages);
            let q = queues.entry(key).or_default();
            for seq in &row.sequences {
                q.push_back(seq.iter().map(chunk_to_result).collect());
            }
        }
        Self {
            queues: Mutex::new(queues),
        }
    }
}
impl StreamingCompletionProvider for QueuedStreamingProvider {
    fn stream_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        let key = canned_stream_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        );
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                None => vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, {} msgs)",
                    params.model,
                    params.messages.len(),
                )))],
            }
        };
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1) + 1);
            for item in sequence {
                let _ = tx.send(item).await;
            }
            rx
        }
    }
}

// ---------------------------------------------------------------------------
// The canned tool-call detector (keyed by the raw response's `marker`).
// ---------------------------------------------------------------------------

struct MarkerDetector {
    by_marker: HashMap<String, Vec<ToolCall>>,
}
impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        raw_response
            .get("marker")
            .and_then(Value::as_str)
            .and_then(|m| self.by_marker.get(m))
            .map(|calls| {
                calls
                    .iter()
                    .map(|c| ToolCall {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                        call_id: c.call_id.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A throwaway [`SelfInventoryEnv`] (the console never calls `self_inventory`).
fn dummy_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: String::new(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// Project a `BrahmaConsoleResult` into v4's `{ ok, answer? , detail? }` JSON.
fn result_to_json(r: &quilltap_core::services::carina_query::BrahmaConsoleResult) -> Value {
    if r.ok {
        json!({ "ok": true, "answer": r.answer })
    } else {
        json!({ "ok": false, "detail": r.detail })
    }
}

#[tokio::test]
async fn brahma_console_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_BRAHMA") else {
        eprintln!("SKIP: set QT_ORACLE_BRAHMA to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_BRAHMA_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_BRAHMA_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_BRAHMA_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_BRAHMA_MOUNT to the seed mount .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: HashMap<String, Value> = HashMap::new();
    let mut oracle_streams: Vec<CannedStreamW> = Vec::new();
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.insert(v["call"].as_str().unwrap().to_string(), v["result"].clone());
            }
            Some("cannedStream") => {
                oracle_streams.push(serde_json::from_value(v).expect("parse cannedStream"))
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-brahma-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-brahma-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let streaming = QueuedStreamingProvider::from_oracle(&oracle_streams);

    let mut by_marker: HashMap<String, Vec<ToolCall>> = HashMap::new();
    for (marker, calls) in &spec.detection {
        by_marker.insert(
            marker.clone(),
            calls
                .iter()
                .map(|c| ToolCall {
                    name: c.name.clone(),
                    arguments: c.arguments.clone(),
                    call_id: c.call_id.clone(),
                })
                .collect(),
        );
    }
    let detector = MarkerDetector { by_marker };

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // The REAL tool runner — `run_sql` executes an actual SELECT over the fixture.
    let runner = BuiltInToolRunner::new(db.clone(), dummy_env());

    for case in &spec.cases {
        let deps = BrahmaQueryDeps {
            db: &db,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            model_supports_native_tools: true,
        };
        let result = run_brahma_query(&deps, &case.user_id, &spec.chat_id, &case.question).await;

        let got = result_to_json(&result);
        let want = oracle_results
            .get(&case.name)
            .unwrap_or_else(|| panic!("oracle missing result for {}", case.name))
            .clone();
        assert_eq!(got, want, "{}: result diverges", case.name);
    }

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
}
