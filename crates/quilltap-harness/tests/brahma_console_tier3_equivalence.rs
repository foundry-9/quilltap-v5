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
//! iteration (real SELECT + continuation); the duplicate-call stuck-loop guard
//! (the byte-exact nudge is proven by the 4th continuation's canned key); the
//! agent-turn-budget salvage; and a scripted mid-stream provider throw
//! (`stream_error_mid_turn`, P4.79 — v4's `for await` propagates it straight out
//! of `runBrahmaQuery`, which has no try/catch of its own; the oracle records
//! the shape v4's real caller, `answerAsBrahma`, converts an uncaught throw
//! into, matching v5's own "never throws" `fail(...)` idiom already used for
//! the api-key/tool-build failure paths above).
//!
//! ## Fixture vintage (P4.89)
//!
//! This family does NOT read the committed `brahma-{main,mount}.db` pair: the
//! recipe below BUILDS `/tmp/qt-brahma-{main,mount}.db` fresh from
//! `build-brahma-console-fixture.ts` through v4's real repositories, so its
//! schema is always the pin's `generateDDL` vintage by construction. It shares
//! only the file NAME with the committed pair. P4.89's in-place widen of that
//! pair therefore cannot reach this family; it was regenerated and re-run at v4
//! `ffb6b3119` alongside its two siblings and stayed green.
//!
//! ## P4.D216 — the shared one-shot loop (v4 `d1c06cd9d`)
//!
//! v4 moved the loop into `lib/services/agent-loop/one-shot-loop.ts`
//! (`runOneShotToolLoop`); v5 moved it into
//! `services::agent_loop::one_shot_loop`, `run_brahma_query` now its thin
//! wrapper. Two growths:
//!
//! - **Log lines, every case.** The oracle records the `OneShotToolLoop` and
//!   `BrahmaOneShot` services' lines (`{ level, message, context }`); this side
//!   captures its tracing events STRUCTURALLY (level, message, each field by
//!   name) from the loop module and the console module, and the two lists must
//!   agree line for line — so a missing, extra, reordered or re-fielded line is
//!   a red, and every case is its own silence leg. v4 `d1c06cd9d` added FIVE
//!   lines under the `Brahma one-shot` label (`: starting`, `: aborted between
//!   turns`, `: aborted mid-stream`, `: tool turn`, `: finished`) and a `turns`
//!   field on `produced an empty answer`; v5 had also never emitted the
//!   pre-existing `stuck in tool-call loop` WARN, the empty-answer DEBUG or the
//!   no-profile DEBUG.
//! - **Loop-direct arms** (`spec.loopCases`) call v4's REAL `runOneShotToolLoop`
//!   and v5's `run_one_shot_tool_loop` directly — the abort seam (between turns
//!   and per chunk mid-stream), `onReasoning`'s REPLACE semantics, the usage
//!   sum, `toolsExecuted`, the default label, the propagated throw, and the log
//!   TYPE (the oracle records `opts.logType ?? 'CHAT_MESSAGE'` for every stream
//!   call that reaches its terminal chunk, which is where v4's real
//!   `streamMessage` writes its row; this side reads the rows its real writer
//!   put in the llm-logs partition under the arm's own chat id).
//!
//! The pre-existing corpus is byte-identical between the `00c290c9a` and
//! `d1c06cd9d` pins (measured): the refactor is neutral on every result and
//! every canned-stream key; only the log lines and the loop arms move.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout — the
//! oracle lives under `.claude/`, which jest ignores, so mirror it to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-brahma-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-brahma-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-brahma-console-fixture.ts
//!   mkdir -p /tmp/brahma-oracle/cases /tmp/brahma-oracle/fixtures
//!   cp $V5W/harness/oracle/cases/brahma-console-tier3.test.ts /tmp/brahma-oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/brahma-console-tier3.json /tmp/brahma-oracle/fixtures/
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::agent_loop::one_shot_loop::{
    run_one_shot_tool_loop, BuiltTools, NoopSink, OneShotLoopDeps, OneShotLoopResult,
    RunOneShotToolLoopOptions,
};
use quilltap_core::services::brahma_console::{run_brahma_query, BrahmaQueryDeps};
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::tool_execution::ToolCall;
use quilltap_core::services::tool_execution::{StatusContext, ToolExecutionContext};
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
    /// P4.D60 (Bug 47): per-case Brahma turn budget written to instance_settings
    /// before the case runs (absent = the default 50).
    #[serde(default, rename = "maxAgentTurns")]
    max_agent_turns: Option<i64>,
}

/// P4.D216: a loop-direct arm (see the oracle header for the trip hooks).
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LoopCaseW {
    name: String,
    user_id: String,
    chat_id: String,
    user_message: String,
    max_agent_turns: i64,
    #[serde(default)]
    log_label: Option<String>,
    #[serde(default)]
    log_type: Option<String>,
    #[serde(default)]
    abort_on_reasoning: Option<String>,
    #[serde(default)]
    abort_on_marker: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    chat_id: String,
    detection: HashMap<String, Vec<DetectionCall>>,
    cases: Vec<CaseW>,
    loop_profile: Value,
    loop_system_prompt: String,
    loop_cases: Vec<LoopCaseW>,
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
    /// A scripted mid-stream provider throw (P4.79's `stream_error_mid_turn`) —
    /// v4's `for await` propagates it out of `runBrahmaQuery` itself (no
    /// internal try/catch there).
    #[serde(default)]
    error: Option<String>,
    /// P4.D216: a reasoning delta.
    #[serde(default)]
    reasoning_content: Option<String>,
    /// P4.D216: usage on the terminal chunk.
    #[serde(default)]
    usage: Option<UsageW>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
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
            role: CompletionRole::from_v4_wire(m.role.as_str()).unwrap_or(CompletionRole::User),
            content: m.content.clone(),
        })
        .collect()
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if let Some(e) = &c.error {
        return Err(StreamError::new(e.clone()));
    }
    if c.done == Some(true) {
        let mut chunk = StreamChunk::done(c.usage.as_ref().map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }));
        chunk.raw_response = c.raw_response.clone();
        return Ok(chunk);
    }
    if let Some(rc) = &c.reasoning_content {
        return Ok(StreamChunk {
            reasoning_content: Some(rc.clone()),
            ..Default::default()
        });
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue of sequences).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
    /// Served sequences that ran to `done` (no scripted throw) — the number of
    /// `CHAT_MESSAGE` rows v4's `streamMessage` logger would have written
    /// (dogfood finding #111's other half — the one-shot engine wrote none where
    /// v4 logs every call).
    completed_streams: Mutex<i64>,
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
            completed_streams: Mutex::new(0),
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
                Some(seq) => {
                    if !seq.iter().any(Result::is_err) {
                        *self.completed_streams.lock().unwrap() += 1;
                    }
                    seq
                }
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
    /// P4.D216: the between-turns trip — `(marker, flag)`; detection runs AFTER
    /// the loop's mid-stream abort check, so the next check is the loop top.
    abort_on: Mutex<Option<(String, Arc<AtomicBool>)>>,
}
impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        let marker = raw_response.get("marker").and_then(Value::as_str);
        if let (Some(m), Some((trip, flag))) = (marker, self.abort_on.lock().unwrap().as_ref()) {
            if m == trip {
                flag.store(true, Ordering::SeqCst);
            }
        }
        marker
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

// ---------------------------------------------------------------------------
// P4.D216: a STRUCTURAL tracing capture (level, message, fields by name), so a
// v4 `{ level, message, context }` line compares field for field.
// ---------------------------------------------------------------------------

/// The two v5 module targets whose lines the oracle's two services map to:
/// the loop (`OneShotToolLoop`) and the console (`BrahmaOneShot`).
const CAPTURED_TARGETS: &[&str] = &[
    "quilltap_core::services::agent_loop::one_shot_loop",
    "quilltap_core::services::brahma_console",
];

#[derive(Debug, Clone, PartialEq)]
struct Line {
    level: String,
    message: String,
    fields: Vec<(String, String)>,
}

struct LineVisitor {
    message: String,
    fields: Vec<(String, String)>,
}
impl tracing::field::Visit for LineVisitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.message = format!("{v:?}");
        } else {
            self.fields.push((f.name().to_string(), format!("{v:?}")));
        }
    }
}

struct StructuralCapture(Arc<Mutex<Vec<Line>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for StructuralCapture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        if !CAPTURED_TARGETS.contains(&meta.target()) {
            return;
        }
        let mut v = LineVisitor {
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut v);
        self.0.lock().unwrap().push(Line {
            level: meta.level().to_string().to_lowercase(),
            message: v.message,
            fields: v.fields,
        });
    }
}

/// A v4 context value rendered the way the v5 capture renders the same field:
/// strings bare, numbers/bools as text, a string array as Rust's `Debug` of a
/// `Vec<&str>` (the loop's `tools` goes through [`render_v4_field`]).
fn render_v4(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let strs: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            format!("{strs:?}")
        }
        other => other.to_string(),
    }
}

/// A v4 context FIELD as the v5 capture sees it. The one-shot loop's `tools`
/// (v4 a `string[]`) is logged under the `…Json` convention — the capture sees
/// the RAW name `toolsJson` with the compact JSON array as its value (the file
/// layer strips the suffix and re-parses; the d1c06cd9d unification review).
fn render_v4_field(k: &str, v: &Value) -> (String, String) {
    if k == "tools" && v.is_array() {
        return ("toolsJson".to_string(), v.to_string());
    }
    (k.to_string(), render_v4(v))
}


fn v4_lines(rows: &Value) -> Vec<Line> {
    rows.as_array()
        .map(|a| {
            a.iter()
                .map(|l| Line {
                    level: l["level"].as_str().unwrap_or_default().to_string(),
                    message: l["message"].as_str().unwrap_or_default().to_string(),
                    fields: l["context"]
                        .as_object()
                        .map(|o| o.iter().map(|(k, v)| render_v4_field(k, v)).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `(level, message, sorted fields)`.
type NormLine = (String, String, Vec<(String, String)>);

/// Field ORDER is not part of the comparand (v4's is object-insertion order,
/// v5's is the macro's); the SET of `(name, value)` pairs is.
fn assert_lines_match(case: &str, got: &[Line], want: &[Line], failures: &mut Vec<String>) {
    let norm = |ls: &[Line]| -> Vec<NormLine> {
        ls.iter()
            .map(|l| {
                let mut f = l.fields.clone();
                f.sort();
                (l.level.clone(), l.message.clone(), f)
            })
            .collect()
    };
    if norm(got) != norm(want) {
        failures.push(format!(
            "{case}: log lines diverge\n  v5: {:#?}\n  v4: {:#?}",
            norm(got),
            norm(want)
        ));
    }
}

/// A v5 loop result projected into the oracle's shape.
fn loop_result_to_json(
    r: &Result<
        OneShotLoopResult,
        quilltap_core::services::agent_loop::one_shot_loop::OneShotLoopError,
    >,
) -> Value {
    match r {
        Ok(OneShotLoopResult::Ok {
            answer,
            tools_executed,
            usage,
        }) => json!({
            "ok": true,
            "answer": answer,
            "toolsExecuted": tools_executed,
            "usage": {
                "promptTokens": usage.prompt_tokens,
                "completionTokens": usage.completion_tokens,
                "totalTokens": usage.total_tokens,
            },
        }),
        Ok(OneShotLoopResult::Failed { detail }) => json!({ "ok": false, "detail": detail }),
        Err(e) => json!({ "threw": e.message }),
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
    let mut oracle_logs: HashMap<String, Value> = HashMap::new();
    let mut oracle_loop: HashMap<String, Value> = HashMap::new();
    let mut oracle_loop_streams: Vec<CannedStreamW> = Vec::new();
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
            Some("logs") => {
                oracle_logs.insert(v["call"].as_str().unwrap().to_string(), v["lines"].clone());
            }
            Some("loopResult") => {
                oracle_loop.insert(v["call"].as_str().unwrap().to_string(), v.clone());
            }
            Some("loopCannedStream") => {
                oracle_loop_streams.push(serde_json::from_value(v).expect("parse loopCannedStream"))
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
    let detector = MarkerDetector {
        by_marker,
        abort_on: Mutex::new(None),
    };

    // An llm-logs partition beside the fixture pair, so the per-query
    // `CHAT_MESSAGE` rows are a comparand-shaped pin (dogfood finding #111's
    // other half — the one-shot engine bypassed `primary_stream`'s logger and a
    // real Brahma query left NO row where v4 logs every `streamMessage` call).
    // The oracle's mock sits above v4's logger, so the oracle cannot emit these
    // rows — this leg is a v5 pin against the canned-stream count, not a
    // differential.
    let llm_dir = tempfile::tempdir().unwrap();
    let llm_data = llm_dir.path().join("data");
    std::fs::create_dir_all(&llm_data).unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(
        &llm_data,
        &spec.test_pepper_base64,
    )
    .expect("provision an llm-logs partition");
    let work_llm_logs = llm_data.join("quilltap-llm-logs.db");
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_llm_logs),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // The REAL tool runner — `run_sql` executes an actual SELECT over the fixture.
    let runner = BuiltInToolRunner::new(db.clone(), dummy_env());

    // P4.D216: one structural capture for the WHOLE test, armed before the first
    // callsite is ever hit (a callsite first reached with no subscriber caches
    // "never" — the `global_capture` note), drained per case.
    use tracing_subscriber::layer::SubscriberExt;
    let captured = Arc::new(Mutex::new(Vec::<Line>::new()));
    let _capture_guard = tracing::subscriber::set_default(
        tracing_subscriber::registry().with(StructuralCapture(captured.clone())),
    );
    let mut log_failures: Vec<String> = Vec::new();

    for case in &spec.cases {
        // Per-case budget override (only the Bug-47 salvage case sets it); the
        // committed fixture has no instance_settings table, so create it first.
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

        let got_lines: Vec<Line> = std::mem::take(&mut *captured.lock().unwrap());
        let want_lines = v4_lines(
            oracle_logs
                .get(&case.name)
                .unwrap_or_else(|| panic!("oracle missing logs for {}", case.name)),
        );
        assert_lines_match(&case.name, &got_lines, &want_lines, &mut log_failures);
    }

    // Finding #111's pin: one CHAT_MESSAGE row per completed one-shot stream
    // call (a scripted mid-stream throw logs nothing — v4 logs on `chunk.done`),
    // v4's console shape — `messageId` NULL (the engine carries none) and
    // `characterId` NULL (no character at all), `connectionProfileId` the
    // resolved profile's, a MEASURED duration. Mutations: remove
    // `log_chat_message_call` in `run_stream` → 0 rows; hard-code `durationMs`
    // to 0 → `llm_log_duration_guard` reddens.
    let (rows, null_mid, null_char, with_profile, nonneg_dur): (i64, i64, i64, i64, i64) = db
        .read_llm_logs(|c| {
            Ok(c.query_row(
                "SELECT count(*), coalesce(sum(messageId IS NULL), 0), \
                 coalesce(sum(characterId IS NULL), 0), \
                 coalesce(sum(connectionProfileId IS NOT NULL), 0), \
                 coalesce(sum(durationMs >= 0), 0) \
                 FROM llm_logs WHERE type = 'CHAT_MESSAGE' AND chatId = ?1",
                [&spec.chat_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?)
        })
        .unwrap();
    let expected_chat_message_rows = *streaming.completed_streams.lock().unwrap();
    assert_eq!(
        rows, expected_chat_message_rows,
        "one CHAT_MESSAGE row per completed one-shot stream call"
    );
    assert!(rows > 0, "the pin must see at least one completed turn");
    assert_eq!(
        null_mid, rows,
        "the one-shot engine passes no messageId → NULL"
    );
    assert_eq!(
        null_char, rows,
        "the one-shot engine has no character → characterId NULL"
    );
    assert_eq!(with_profile, rows);
    assert_eq!(nonneg_dur, rows);
    eprintln!("[llm_logs] {rows} CHAT_MESSAGE row(s), expected {expected_chat_message_rows}.");

    // -----------------------------------------------------------------------
    // P4.D216: the loop-direct arms.
    // -----------------------------------------------------------------------
    assert!(
        !spec.loop_cases.is_empty() && oracle_loop.len() == spec.loop_cases.len(),
        "every loop arm must have an oracle row ({} arms, {} rows) — a pre-`d1c06cd9d` \
         oracle emits none",
        spec.loop_cases.len(),
        oracle_loop.len()
    );
    let loop_streaming = QueuedStreamingProvider::from_oracle(&oracle_loop_streams);
    let loop_built = BuiltTools {
        tools: Vec::new(),
        model_supports_native_tools: true,
        use_native_web_search: false,
    };
    let mut loop_failures: Vec<String> = Vec::new();
    for lc in &spec.loop_cases {
        let want = oracle_loop
            .get(&lc.name)
            .unwrap_or_else(|| panic!("oracle missing loop row for {}", lc.name));
        let flag = Arc::new(AtomicBool::new(false));
        *detector.abort_on.lock().unwrap() = lc.abort_on_marker.clone().map(|m| (m, flag.clone()));
        let mut reasoning_calls: Vec<String> = Vec::new();
        let trip_reasoning = lc.abort_on_reasoning.clone();
        let flag_for_cb = flag.clone();
        let mut on_reasoning = |r: &str| {
            reasoning_calls.push(r.to_string());
            if trip_reasoning.as_deref() == Some(r) {
                flag_for_cb.store(true, Ordering::SeqCst);
            }
        };
        let tool_context = ToolExecutionContext {
            chat_id: lc.chat_id.clone(),
            user_id: lc.user_id.clone(),
            operator_surface: true,
            ..Default::default()
        };
        let log_type: Option<&'static str> = match lc.log_type.as_deref() {
            None => None,
            Some("SCENARIO_BUILDER") => {
                Some(quilltap_core::services::llm_logging::log_type::SCENARIO_BUILDER)
            }
            Some(other) => panic!("{}: unmapped logType {other}", lc.name),
        };
        let result = run_one_shot_tool_loop(
            &OneShotLoopDeps {
                db: &db,
                streaming: &loop_streaming,
                tool_runner: &runner,
                tool_detector: &detector,
            },
            RunOneShotToolLoopOptions {
                user_id: &lc.user_id,
                chat_id: &lc.chat_id,
                connection_profile: &spec.loop_profile,
                system_prompt: &spec.loop_system_prompt,
                user_message: &lc.user_message,
                tools: &loop_built,
                tool_context: &tool_context,
                max_agent_turns: lc.max_agent_turns,
                controller: &NoopSink,
                signal: Some(&flag),
                log_type,
                status_context: Some(StatusContext {
                    character_name: "The Host".to_string(),
                    character_id: String::new(),
                }),
                on_reasoning: Some(&mut on_reasoning),
                log_label: lc.log_label.as_deref(),
            },
        )
        .await;
        *detector.abort_on.lock().unwrap() = None;

        let got = loop_result_to_json(&result);
        if got != want["result"] {
            loop_failures.push(format!(
                "{}: result diverges\n  v5: {got}\n  v4: {}",
                lc.name, want["result"]
            ));
        }
        let want_reasoning: Vec<String> =
            serde_json::from_value(want["reasoningCalls"].clone()).expect("reasoningCalls");
        if reasoning_calls != want_reasoning {
            loop_failures.push(format!(
                "{}: onReasoning calls diverge\n  v5: {reasoning_calls:?}\n  v4: {want_reasoning:?}",
                lc.name
            ));
        }
        // The log TYPE of every row the real writer put down for this arm, in
        // write order, against the types v4's `streamMessage` would have logged.
        let chat_id = lc.chat_id.clone();
        let got_types: Vec<String> = db
            .read_llm_logs(move |c| {
                let mut st =
                    c.prepare("SELECT type FROM llm_logs WHERE chatId = ?1 ORDER BY rowid")?;
                let rows = st
                    .query_map([&chat_id], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let want_types: Vec<String> =
            serde_json::from_value(want["loggedTypes"].clone()).expect("loggedTypes");
        if got_types != want_types {
            loop_failures.push(format!(
                "{}: llm_logs types diverge\n  v5: {got_types:?}\n  v4: {want_types:?}",
                lc.name
            ));
        }
        let got_lines: Vec<Line> = std::mem::take(&mut *captured.lock().unwrap());
        assert_lines_match(
            &lc.name,
            &got_lines,
            &v4_lines(&want["lines"]),
            &mut log_failures,
        );
    }
    assert!(loop_failures.is_empty(), "\n{}", loop_failures.join("\n"));
    assert!(log_failures.is_empty(), "\n{}", log_failures.join("\n"));
    eprintln!(
        "[p4d216] {} case(s) + {} loop arm(s): results, log lines and llm_logs types agree.",
        spec.cases.len(),
        spec.loop_cases.len()
    );

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
}
