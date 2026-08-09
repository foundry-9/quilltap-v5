//! Tier-3 differential: the **Brahma Console orchestrator**
//! (`quilltap_core::services::brahma_console::orchestrator::handle_brahma_console_message`
//! — v4 `handleBrahmaConsoleMessage` + `processBrahmaResponse`,
//! lib/services/brahma-console/orchestrator.service.ts), the multi-turn,
//! transcript-persisting console.
//!
//! Both sides pin the model boundaries identically: the streamed calls by the
//! exact `provider|model|temperature|messages` key (the jest oracle scripts each
//! real call and RECORDS the key — so the system-prompt bytes, the tool-mode gate,
//! and the tool-result threading are proven by the replay), and tool detection by
//! the raw-response `marker`. Tool EXECUTION is REAL on both sides: `run_sql` runs
//! an actual SELECT over the fixture through the real `BuiltInToolRunner`, its
//! byte-exact result threading into the continuation stream AND persisting as a
//! TOOL row. Per case, the emitted frame sequence (the [`RecordingSink`] trace) and
//! the persisted message rows (projected to the stable columns; minted ids /
//! timestamps dropped) are diffed against the oracle.
//!
//! Corpus: a plain no-tool final; a native `run_sql` turn; a text-block tool turn;
//! the `submit_final_response` arm; the duplicate-call stuck guard. (The agent-turn
//! cap is now an operator-set instance setting — `resolve_brahma_max_agent_turns`,
//! default 50, over a fixture with no `brahmaConsole` key — proven by the
//! `loop_bound_forces_a_final_answer_at_the_operator_cap` unit test in the
//! orchestrator's own `tests`, mirroring the one-shot engine. Its default-50 value
//! also rides the recorded system-prompt bytes here — `build_agent_mode_instructions(50)`
//! — so this oracle must be regenerated at v4 `6452e2c3`+ (P4.D57).)
//!
//! Generate the oracle (Node 24, from the v4 checkout — the oracle lives under
//! `.claude/`, which jest ignores, so mirror it to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
//!   cd ~/source/quilltap-server
//!   mkdir -p /tmp/brahma-orch/cases /tmp/brahma-orch/fixtures
//!   cp $W/harness/oracle/cases/brahma-orchestrator-tier3.test.ts /tmp/brahma-orch/cases/
//!   cp $W/harness/oracle/fixtures/brahma-orchestrator-tier3.json /tmp/brahma-orch/fixtures/
//!   QT_FIXTURE_BRAHMA_MAIN=$W/crates/quilltap-web/tests/fixtures/brahma-main.db \
//!   QT_FIXTURE_BRAHMA_MOUNT=$W/crates/quilltap-web/tests/fixtures/brahma-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-brahma-orch.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots /tmp/brahma-orch/cases -- brahma-orchestrator-tier3
//! Run:
//!   QT_ORACLE_BRAHMA_ORCH=/tmp/oracle-brahma-orch.ndjson \
//!     cargo test -p quilltap-harness --test brahma_orchestrator_tier3_equivalence

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::brahma_console::orchestrator::{
    handle_brahma_console_message, BrahmaConsoleSendOptions, BrahmaSendDeps,
};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::message_finalizer::NoCostTracking;
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::tool_execution::ToolCall;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::Value;

const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

// ---------------------------------------------------------------------------
// Spec (harness/oracle/fixtures/brahma-orchestrator-tier3.json).
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
struct CaseW {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    content: String,
    /// P4.D60 (Bug 47): per-case Brahma turn budget written to instance_settings
    /// before the case runs (default 50 = the fixture's absent-setting value).
    #[serde(default, rename = "maxAgentTurns")]
    max_agent_turns: Option<i64>,
}

#[derive(Deserialize)]
struct Spec {
    detection: HashMap<String, Vec<DetectionCall>>,
    cases: Vec<CaseW>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/brahma-orchestrator-tier3.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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
    reasoning: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    usage: Option<UsageW>,
    #[serde(default)]
    raw_response: Option<Value>,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
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
        let usage = c.usage.as_ref().map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            // Unused by the orchestrator (v4 folds only prompt/completion into
            // `totalUsage`); kept consistent so the chunk is well-formed.
            total_tokens: u.prompt_tokens + u.completion_tokens,
        });
        let mut chunk = StreamChunk::done(usage);
        chunk.raw_response = c.raw_response.clone();
        return Ok(chunk);
    }
    if let Some(r) = &c.reasoning {
        // A reasoning ("thinking") chunk: empty content + cumulative reasoning.
        let mut chunk = StreamChunk::content("");
        chunk.reasoning_content = Some(r.clone());
        return Ok(chunk);
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue of sequences).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, VecDeque<Vec<StreamChunkResult>>>>,
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, VecDeque<Vec<StreamChunkResult>>> = HashMap::new();
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

// ---------------------------------------------------------------------------
// Normalization.
// ---------------------------------------------------------------------------

const MSG_COLS: &[&str] = &[
    "role",
    "content",
    "provider",
    "modelName",
    "promptTokens",
    "completionTokens",
    "tokenCount",
    "reasoningContent",
];

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

/// Blank the minted `messageId` on a done frame (both sides mint a fresh uuid).
fn blank_frame(mut v: Value) -> Value {
    if let Value::Object(o) = &mut v {
        if o.contains_key("messageId") {
            o.insert("messageId".to_string(), Value::String("<messageId>".into()));
        }
    }
    canon_numbers(&mut v);
    v
}

fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

/// Project a persisted message Value to the stable diff columns (minted id +
/// createdAt dropped; the transcript order is deterministic per send).
fn project_message(m: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for col in MSG_COLS {
        out.insert(col.to_string(), m.get(*col).cloned().unwrap_or(Value::Null));
    }
    let mut v = Value::Object(out);
    canon_numbers(&mut v);
    v
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {i}\n  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}

#[tokio::test]
async fn brahma_orchestrator_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_BRAHMA_ORCH") else {
        eprintln!("SKIP: set QT_ORACLE_BRAHMA_ORCH to the oracle NDJSON (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle_text = std::fs::read_to_string(&oracle_path).unwrap();

    let mut oracle_events: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_messages: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_streams: Vec<CannedStreamW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        match v.get("kind").and_then(Value::as_str) {
            Some("events") => {
                oracle_events.insert(
                    v["call"].as_str().unwrap().to_string(),
                    v["events"].as_array().cloned().unwrap_or_default(),
                );
            }
            Some("messages") => {
                oracle_messages.insert(
                    v["call"].as_str().unwrap().to_string(),
                    v["rows"].as_array().cloned().unwrap_or_default(),
                );
            }
            Some("cannedStream") => {
                oracle_streams.push(serde_json::from_value(v).unwrap());
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Fresh copies so the committed seed stays pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-brahma-orch-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-brahma-orch-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(fixtures_dir().join("brahma-main.db"), &work_main).unwrap();
    std::fs::copy(fixtures_dir().join("brahma-mount.db"), &work_mount).unwrap();

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
        PEPPER,
    )
    .unwrap();

    let runner = BuiltInToolRunner::new(db.clone(), dummy_env());
    let mut failed: Vec<String> = Vec::new();

    for case in &spec.cases {
        // Per-case budget override (only the Bug-47 salvage cases set it); the
        // committed fixture has no instance_settings table, so create it first.
        // Every other case reads the absent setting → default 50.
        if let Some(budget) = case.max_agent_turns {
            db.write(move |w| {
                w.main().connection().execute_batch(
                    "CREATE TABLE IF NOT EXISTS \"instance_settings\" \
                     (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
                )?;
                quilltap_core::db::instance_settings::set_brahma_console_settings(
                    w.main().connection(),
                    budget,
                )
                .map(|_| ())
            })
            .await
            .unwrap();
        }

        let sink = RecordingSink::new();
        let mut cost = NoCostTracking;
        let mut deps = BrahmaSendDeps {
            db: &db,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            cost: &mut cost,
            model_supports_native_tools: true,
        };
        let opts = BrahmaConsoleSendOptions {
            content: case.content.clone(),
            file_ids: Vec::new(),
        };
        let result =
            handle_brahma_console_message(&mut deps, &sink, USER, &case.chat_id, &opts).await;
        if let Err(e) = &result {
            eprintln!("[{}] orchestrator errored: {e}", case.name);
            failed.push(format!("{}_error", case.name));
            continue;
        }

        // Frames.
        let got_frames: Vec<Value> = sink.events_json().into_iter().map(blank_frame).collect();
        let want_frames: Vec<Value> = oracle_events
            .get(&case.name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(blank_frame)
            .collect();
        let got_f = norm(&Value::Array(got_frames));
        let want_f = norm(&Value::Array(want_frames));
        if got_f != want_f {
            eprintln!(
                "[{}] FRAMES MISMATCH:\n{}",
                case.name,
                first_diff(&got_f, &want_f)
            );
            failed.push(format!("{}_frames", case.name));
        } else {
            eprintln!(
                "[{}] frames OK ({} frames).",
                case.name,
                sink.events().len()
            );
        }

        // Persisted messages.
        let cid = case.chat_id.clone();
        let messages = db
            .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .unwrap();
        let got_rows: Vec<Value> = messages
            .iter()
            .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
            .map(project_message)
            .collect();
        let want_rows: Vec<Value> = oracle_messages
            .get(&case.name)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(project_message)
            .collect();
        let got_m = norm(&Value::Array(got_rows));
        let want_m = norm(&Value::Array(want_rows));
        if got_m != want_m {
            eprintln!(
                "[{}] MESSAGES MISMATCH:\n{}",
                case.name,
                first_diff(&got_m, &want_m)
            );
            failed.push(format!("{}_messages", case.name));
        } else {
            eprintln!("[{}] messages OK.", case.name);
        }
    }

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    assert!(
        failed.is_empty(),
        "brahma-orchestrator tier-3 FAILED: {failed:?}"
    );
}
