//! Tier-3 differential test: the **Carina query engine**
//! (`quilltap_core::services::carina_query::run_carina_query` — v4
//! `runCarinaQuery`, lib/services/carina/carina.service.ts), the isolated
//! reference-answer engine.
//!
//! Both sides pin the model boundaries identically: the streamed calls by the
//! exact `provider|model|temperature|messages` key (the jest oracle scripts each
//! real call and RECORDS the key it answered; this test replays them through a
//! `QueuedStreamingProvider`, so a prompt/selection/temperature divergence is a
//! canned-miss → an `llm-failed` result diverging from v4's `ok`), the memory
//! recall embedding by the exact question text, tool detection by the raw-response
//! marker, tool execution by `name|args|callId`, and the Brahma console by an
//! injected canned result. Then per case:
//!
//!   1. the `CarinaResult` (ok/err, answerer id/name minted-remapped, the posted
//!      message id `<msg>`-tokenized, the answer text exact);
//!   2. the emitted `carinaAnswer` event (the same posted message);
//!
//! and after all cases, the `chat_messages` / `chats` / `background_jobs` table
//! dumps in the shared-cross-table id-map remap form.
//!
//! Generate the fixtures + oracle output (Node 24, from the v4 checkout — the
//! oracle lives under `.claude/`, which jest ignores, so mirror it to /tmp):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}   # the v5 checkout (or your worktree)
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-carina-query-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-carina-query-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-carina-query-fixture.ts
//!   mkdir -p /tmp/carina-oracle/cases /tmp/carina-oracle/fixtures
//!   cp $V5W/harness/oracle/cases/carina-query-tier3.test.ts /tmp/carina-oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/carina-query-tier3.json /tmp/carina-oracle/fixtures/
//!   QT_FIXTURE_CARINA_MAIN=/tmp/qt-carina-query-main.db QT_FIXTURE_CARINA_MOUNT=/tmp/qt-carina-query-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-carina-query.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "/tmp/carina-oracle/cases" -- carina-query-tier3
//! Run:
//!   QT_ORACLE_CARINA_QUERY=/tmp/oracle-carina-query.ndjson \
//!   QT_FIXTURE_CARINA_MAIN=/tmp/qt-carina-query-main.db QT_FIXTURE_CARINA_MOUNT=/tmp/qt-carina-query-mount.db \
//!     cargo test -p quilltap-harness --test carina_query_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::carina_query::{
    run_carina_query, BrahmaConsoleResult, CarinaQueryDeps, RunBrahmaConsole,
};
use quilltap_core::services::carina_runner::{CarinaResult, RunCarinaQueryOptions};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::tool_execution::{
    CannedToolRunner, ToolCall, ToolMetadata, ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/carina-query-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectionCall {
    name: String,
    arguments: Value,
    #[serde(default)]
    call_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CannedToolW {
    name: String,
    arguments: Value,
    #[serde(default)]
    call_id: Option<String>,
    success: bool,
    result: Value,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrahmaW {
    ok: bool,
    answer: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    character_name: String,
    question: String,
    whisper: bool,
    #[serde(default)]
    asker_participant_id: Option<String>,
    operator_initiated: bool,
    expect_post: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatW {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    now_ms: f64,
    user_id: String,
    chat: ChatW,
    canned_embeddings: HashMap<String, Vec<f32>>,
    detection: HashMap<String, Vec<DetectionCall>>,
    canned_tools: Vec<CannedToolW>,
    brahma: BrahmaW,
    cases: Vec<CaseW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/carina-query-tier3.json")
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

// ---------------------------------------------------------------------------
// The canned Brahma console seam.
// ---------------------------------------------------------------------------

struct CannedBrahma {
    ok: bool,
    answer: String,
    detail: Option<String>,
}
impl RunBrahmaConsole for CannedBrahma {
    fn run(
        &self,
        _user_id: &str,
        _chat_id: &str,
        _question: &str,
    ) -> impl Future<Output = BrahmaConsoleResult> + Send {
        let r = BrahmaConsoleResult {
            ok: self.ok,
            answer: self.answer.clone(),
            detail: self.detail.clone(),
        };
        async move { r }
    }
}

// ---------------------------------------------------------------------------
// Result / event JSON projection + normalization.
// ---------------------------------------------------------------------------

/// v4's `CarinaResult` JSON shape (both sides), built from the Rust enum.
fn carina_result_to_json(r: &CarinaResult) -> Value {
    match r {
        CarinaResult::Ok {
            answer,
            message_id,
            message,
            answerer_id,
            answerer_name,
        } => json!({
            "ok": true,
            "answer": answer,
            "messageId": message_id,
            "message": message.message.clone(),
            "answererId": answerer_id,
            "answererName": answerer_name,
        }),
        CarinaResult::Err { error } => {
            let mut e = serde_json::Map::new();
            e.insert("kind".into(), json!(error.kind.as_str()));
            // v4 `JSON.stringify` drops undefined; keep only present fields.
            if let Some(cn) = &error.character_name {
                e.insert("characterName".into(), json!(cn));
            }
            if let Some(d) = &error.detail {
                e.insert("detail".into(), json!(d));
            }
            json!({ "ok": false, "error": Value::Object(e) })
        }
    }
}

/// Normalize a posted-message object: the minted `id` → `<msg>`, `createdAt` → `<ts>`.
fn normalize_message(msg: &mut Value) {
    if let Some(obj) = msg.as_object_mut() {
        if obj.contains_key("id") {
            obj.insert("id".into(), Value::String("<msg>".into()));
        }
        if obj.contains_key("createdAt") {
            obj.insert("createdAt".into(), Value::String("<ts>".into()));
        }
    }
}

/// Normalize a `CarinaResult` JSON (per-case): the minted `messageId` → `<msg>`
/// and the embedded posted message.
fn normalize_result(result: &mut Value) {
    if let Some(obj) = result.as_object_mut() {
        if obj.get("messageId").map(Value::is_string).unwrap_or(false) {
            obj.insert("messageId".into(), Value::String("<msg>".into()));
        }
        if let Some(msg) = obj.get_mut("message") {
            normalize_message(msg);
        }
    }
}

/// Normalize a `carinaAnswer` event (per-case): the embedded message.
fn normalize_event(event: &mut Value) {
    if let Some(msg) = event
        .as_object_mut()
        .and_then(|o| o.get_mut("carinaAnswer"))
    {
        normalize_message(msg);
    }
}

// ---------------------------------------------------------------------------
// Table normalization — the shared-cross-table id-map remap form.
// ---------------------------------------------------------------------------

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

fn ts_columns(row: &mut serde_json::Map<String, Value>, cols: &[&str]) {
    for col in cols {
        if row.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
            row.insert((*col).to_string(), Value::String("<ts>".to_string()));
        }
    }
}

/// Normalize the three tables in-place. `chat_messages` seeds the shared id-map
/// (its minted `id` → numbered token); `background_jobs.payload.carinaMessageId`
/// re-uses those tokens, its own minted `id` → the constant `<jobid>`, and its
/// rows are re-sorted by their normalized string (the minted `id` order is not
/// stable across sides).
fn normalize_tables(chat_messages: &mut Value, chats: &mut Value, background_jobs: &mut Value) {
    let mut id_map: HashMap<String, String> = HashMap::new();

    if let Some(rows) = chat_messages.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().expect("chat_messages row object");
            if let Some(Value::String(raw)) = obj.get("id") {
                let tok = token_for(&mut id_map, raw);
                obj.insert("id".into(), Value::String(tok));
            }
            ts_columns(obj, &["createdAt", "updatedAt"]);
        }
    }

    if let Some(rows) = chats.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().expect("chats row object");
            ts_columns(obj, &["updatedAt", "lastMessageAt"]);
        }
    }

    if let Some(rows) = background_jobs
        .get_mut("rows")
        .and_then(Value::as_array_mut)
    {
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().expect("background_jobs row object");
            obj.insert("id".into(), Value::String("<jobid>".into()));
            ts_columns(
                obj,
                &[
                    "createdAt",
                    "updatedAt",
                    "scheduledAt",
                    "startedAt",
                    "completedAt",
                ],
            );
            // Remap the payload's carinaMessageId through the shared map.
            if let Some(Value::String(raw)) = obj.get("payload") {
                if let Ok(mut payload) = serde_json::from_str::<Value>(raw) {
                    if let Some(cid) = payload
                        .as_object()
                        .and_then(|p| p.get("carinaMessageId"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                    {
                        let tok = token_for(&mut id_map, &cid);
                        payload
                            .as_object_mut()
                            .unwrap()
                            .insert("carinaMessageId".into(), Value::String(tok));
                    }
                    obj.insert(
                        "payload".into(),
                        Value::String(serde_json::to_string(&payload).unwrap()),
                    );
                }
            }
        }
        // Re-sort by the normalized row string (the minted `id` order diverges).
        rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
    }
}

// ---------------------------------------------------------------------------
// Oracle NDJSON.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn carina_query_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CARINA_QUERY") else {
        eprintln!("SKIP: set QT_ORACLE_CARINA_QUERY to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_CARINA_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_CARINA_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_CARINA_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_CARINA_MOUNT to the seed mount .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: HashMap<String, Value> = HashMap::new();
    let mut oracle_events: HashMap<String, Value> = HashMap::new();
    let mut oracle_streams: Vec<CannedStreamW> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
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
            Some("event") => {
                oracle_events.insert(v["call"].as_str().unwrap().to_string(), v["event"].clone());
            }
            Some("cannedStream") => {
                oracle_streams.push(serde_json::from_value(v).expect("parse cannedStream"))
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-carina-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-carina-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // The providers + seams.
    let streaming = QueuedStreamingProvider::from_oracle(&oracle_streams);

    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }

    let mut runner = CannedToolRunner::new();
    for t in &spec.canned_tools {
        let tc = ToolCall {
            name: t.name.clone(),
            arguments: t.arguments.clone(),
            call_id: t.call_id.clone(),
        };
        runner.register(
            &tc,
            ToolResult {
                tool_name: t.name.clone(),
                success: t.success,
                result: t.result.clone(),
                error: t.error.clone(),
                message: t.message.clone(),
                metadata: None::<ToolMetadata>,
            },
        );
    }

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

    let brahma = CannedBrahma {
        ok: spec.brahma.ok,
        answer: spec.brahma.answer.clone(),
        detail: spec.brahma.detail.clone(),
    };

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    for case in &spec.cases {
        let sink = RecordingSink::new();
        let deps = CarinaQueryDeps {
            db: &db,
            embedding: &embedding,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            sink: &sink,
            brahma: &brahma,
            model_supports_native_tools: false,
            now_ms: spec.now_ms,
        };
        let opts = RunCarinaQueryOptions {
            user_id: spec.user_id.clone(),
            chat_id: spec.chat.id.clone(),
            character_name: case.character_name.clone(),
            question: case.question.clone(),
            whisper: case.whisper,
            asker_participant_id: case.asker_participant_id.clone(),
            operator_initiated: case.operator_initiated,
        };

        let result = run_carina_query(&deps, &opts).await;

        // Result diff.
        let mut got = carina_result_to_json(&result);
        let mut want = oracle_results
            .get(&case.name)
            .unwrap_or_else(|| panic!("oracle missing result for {}", case.name))
            .clone();
        normalize_result(&mut got);
        normalize_result(&mut want);
        assert_eq!(got, want, "{}: result diverges", case.name);

        // Event diff.
        let events = sink.events_json();
        let mut got_event = if case.expect_post {
            assert_eq!(
                events.len(),
                1,
                "{}: expected exactly one carinaAnswer event",
                case.name
            );
            json!({ "carinaAnswer": events[0].get("carinaAnswer").cloned().unwrap_or(Value::Null) })
        } else {
            assert!(
                events.is_empty(),
                "{}: expected no event, got {events:?}",
                case.name
            );
            Value::Null
        };
        let mut want_event = oracle_events
            .get(&case.name)
            .unwrap_or_else(|| panic!("oracle missing event for {}", case.name))
            .clone();
        if !got_event.is_null() {
            normalize_event(&mut got_event);
            normalize_event(&mut want_event);
        }
        assert_eq!(
            got_event, want_event,
            "{}: carinaAnswer event diverges",
            case.name
        );
    }

    // Dump + diff the three tables in the shared-id-map remap form.
    let mut got_cm = db
        .read_main(|c| dump_table_json_conn(c, "chat_messages", "content"))
        .expect("dump chat_messages");
    let mut got_ch = db
        .read_main(|c| dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    let mut got_bj = db
        .read_main(|c| dump_table_json_conn(c, "background_jobs", "id"))
        .expect("dump background_jobs");
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    let strip = |mut v: Value| -> Value {
        v.as_object_mut().unwrap().remove("kind");
        v
    };
    let mut want_cm = strip(
        oracle_tables
            .remove("chat_messages")
            .expect("oracle chat_messages"),
    );
    let mut want_ch = strip(oracle_tables.remove("chats").expect("oracle chats"));
    let mut want_bj = strip(
        oracle_tables
            .remove("background_jobs")
            .expect("oracle background_jobs"),
    );

    normalize_tables(&mut got_cm, &mut got_ch, &mut got_bj);
    normalize_tables(&mut want_cm, &mut want_ch, &mut want_bj);

    for (name, got, want) in [
        ("chat_messages", &got_cm, &want_cm),
        ("chats", &got_ch, &want_ch),
        ("background_jobs", &got_bj, &want_bj),
    ] {
        assert_eq!(got["columns"], want["columns"], "{name} column set / order");
        assert_eq!(got["rows"], want["rows"], "{name} rows diverge");
    }
}
