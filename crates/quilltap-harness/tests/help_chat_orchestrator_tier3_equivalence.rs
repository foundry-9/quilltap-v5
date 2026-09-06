//! Tier-3 differential: the **help-chat orchestrator**
//! (`quilltap_core::services::help_chat::orchestrator::handle_help_chat_message` —
//! v4 `handleHelpChatMessage` + `processHelpResponse` + `triggerAsyncTasks`,
//! lib/services/help-chat/orchestrator.service.ts).
//!
//! Both sides pin the model boundaries identically: the streamed calls by the
//! exact `provider|model|temperature|messages` canned key (the jest oracle scripts
//! each real call and RECORDS the key over the messages the plugins would send — an
//! id-less `tool` row filtered out on both sides, see the orchestrator module doc),
//! and tool detection by the raw-response `marker`. Tool EXECUTION is REAL on both
//! sides (`help_search` runs the real keyword fallback over the fixture's 17 docs;
//! `help_navigate` is pure). Per case: the emitted frame sequence, the chat's
//! persisted message rows (stable columns), and the chat's `background_jobs` rows
//! (the async tail's MEMORY_EXTRACTION enqueue — the context-summary check is the
//! no-op seam on both sides).
//!
//! Corpus (11 cases, each into its OWN fixture chat): single character with a NULL
//! `helpPageUrl` (→ `/`); two characters over a transcript carrying a TOOL row
//! (`turnStart`/`turnComplete`/`chainComplete`, no `skipped` key); a native
//! `help_search` turn; a native `help_navigate` turn; a text-block turn (profile
//! P3); the duplicate-call guard (three identical calls → the nudge); the
//! `submit_final_response` arm; the JSON-text fallback; the 10-turn cap; a
//! participant whose profile's api key is dangling (`API key not found` as a
//! per-participant `error` frame, the OTHER participant still answering); and user
//! B's chat with NO `chat_settings` row (the async tail suppressed — no job).
//!
//! Generate (Node 24, from the v4 checkout — mirror to /tmp; jest ignores .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   mkdir -p /tmp/help-orch/cases /tmp/help-orch/fixtures
//!   cp $V5W/harness/oracle/cases/help-chat-orchestrator-tier3.test.ts /tmp/help-orch/cases/
//!   cp $V5W/harness/oracle/fixtures/help-chat-orchestrator-tier3.json /tmp/help-orch/fixtures/
//!   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
//!   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-help-orch.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots /tmp/help-orch/cases -- help-chat-orchestrator-tier3
//! Run:
//!   QT_ORACLE_HELP_ORCH=/tmp/oracle-help-orch.ndjson \
//!     cargo test -p quilltap-harness --test help_chat_orchestrator_tier3_equivalence

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
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::help_chat::orchestrator::{
    handle_help_chat_message, HelpChatSendOptions, HelpSendDeps, NoHelpContextSummaryCheck,
};
use quilltap_core::services::message_finalizer::NoCostTracking;
use quilltap_core::services::native_tool_loop::ToolCallDetector;
use quilltap_core::services::tool_execution::ToolCall;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Value};

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
    #[serde(default)]
    user: Option<String>,
    content: String,
}
#[derive(Deserialize)]
struct Users {
    #[serde(rename = "A")]
    a: String,
    #[serde(rename = "B")]
    b: String,
}
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    users: Users,
    detection: HashMap<String, Vec<DetectionCall>>,
    cases: Vec<CaseW>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/help-chat-orchestrator-tier3.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

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
    usage: Option<UsageW>,
    #[serde(default)]
    raw_response: Option<Value>,
    /// A scripted mid-stream throw (the oracle's `streamMessage` mock throws
    /// `new Error(error)` after the chunks before it) — replayed as an `Err`
    /// chunk so the Rust loop meets the same failure at the same point.
    #[serde(default)]
    error: Option<String>,
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
    if let Some(e) = &c.error {
        return Err(StreamError::new(e.clone()));
    }
    if c.done == Some(true) {
        let usage = c.usage.as_ref().map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.prompt_tokens + u.completion_tokens,
        });
        let mut chunk = StreamChunk::done(usage);
        chunk.raw_response = c.raw_response.clone();
        return Ok(chunk);
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
}

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, VecDeque<Vec<StreamChunkResult>>>>,
    misses: Mutex<Vec<String>>,
    /// Served sequences that ran to `done` (no scripted throw) — the number
    /// of `CHAT_MESSAGE` rows v4's `streamMessage` logger would have written.
    completed_streams: Mutex<i64>,
}
impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedStreamW]) -> Self {
        let mut queues: HashMap<String, VecDeque<Vec<StreamChunkResult>>> = HashMap::new();
        for row in rows {
            let key = canned_stream_key(
                &row.provider,
                &row.model,
                row.temperature,
                &to_completion_messages(&row.messages),
            );
            let q = queues.entry(key).or_default();
            for seq in &row.sequences {
                q.push_back(seq.iter().map(chunk_to_result).collect());
            }
        }
        Self {
            queues: Mutex::new(queues),
            misses: Mutex::new(Vec::new()),
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
                None => {
                    // Name the miss with the LAST message so a divergence is legible.
                    let last = params
                        .messages
                        .last()
                        .map(|m| format!("{}: {:.120}", m.role_str(), m.content()))
                        .unwrap_or_default();
                    self.misses.lock().unwrap().push(format!(
                        "({provider}, {}, {} msgs) last={last:?}",
                        params.model,
                        params.messages.len()
                    ));
                    // Debugging aid: `QT_HELP_ORCH_DUMP_DIR=<dir>` writes each
                    // missed call's messages so the system-prompt bytes can be
                    // diffed against the oracle's recorded `cannedStream` rows.
                    if let Ok(dir) = std::env::var("QT_HELP_ORCH_DUMP_DIR") {
                        let n = self.misses.lock().unwrap().len();
                        let dump: Vec<Value> = params
                            .messages
                            .iter()
                            .map(|m| json!({ "role": m.role_str(), "content": m.content() }))
                            .collect();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(
                            format!("{dir}/miss-{n}.json"),
                            serde_json::to_string_pretty(&dump).unwrap(),
                        );
                    }
                    vec![Err(StreamError::new(format!(
                        "no canned stream queued for key ({provider}, model {}, {} msgs)",
                        params.model,
                        params.messages.len(),
                    )))]
                }
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

struct MarkerDetector {
    by_marker: HashMap<String, Vec<ToolCall>>,
}
impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        raw_response
            .get("marker")
            .and_then(Value::as_str)
            .and_then(|m| self.by_marker.get(m))
            .cloned()
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

const MSG_COLS: &[&str] = &[
    "role",
    "content",
    "participantId",
    "provider",
    "modelName",
    "promptTokens",
    "completionTokens",
    "tokenCount",
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
/// Blank the minted `messageId` on done/turnComplete frames (both sides mint).
fn blank_frame(mut v: Value) -> Value {
    if let Value::Object(o) = &mut v {
        if let Some(Value::String(s)) = o.get("messageId") {
            if !s.is_empty() {
                o.insert("messageId".to_string(), Value::String("<messageId>".into()));
            }
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
async fn help_chat_orchestrator_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_ORCH") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_ORCH to the oracle NDJSON (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle_text = std::fs::read_to_string(&oracle_path).unwrap();

    let mut oracle_events: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_messages: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_jobs: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_streams: Vec<CannedStreamW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        let call = || v["call"].as_str().unwrap().to_string();
        match v.get("kind").and_then(Value::as_str) {
            Some("events") => {
                oracle_events.insert(call(), v["events"].as_array().cloned().unwrap_or_default());
            }
            Some("messages") => {
                oracle_messages.insert(call(), v["rows"].as_array().cloned().unwrap_or_default());
            }
            Some("jobs") => {
                oracle_jobs.insert(call(), v["jobs"].as_array().cloned().unwrap_or_default());
            }
            Some("cannedStream") => oracle_streams.push(serde_json::from_value(v).unwrap()),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    assert_eq!(
        oracle_events.len(),
        spec.cases.len(),
        "oracle cases vs corpus"
    );

    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-help-orch-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-help-orch-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(fixtures_dir().join("help-chat-main.db"), &work_main).unwrap();
    std::fs::copy(fixtures_dir().join("help-chat-mount.db"), &work_mount).unwrap();

    let streaming = QueuedStreamingProvider::from_oracle(&oracle_streams);
    let by_marker: HashMap<String, Vec<ToolCall>> = spec
        .detection
        .iter()
        .map(|(marker, calls)| {
            (
                marker.clone(),
                calls
                    .iter()
                    .map(|c| ToolCall {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                        call_id: c.call_id.clone(),
                    })
                    .collect(),
            )
        })
        .collect();
    let detector = MarkerDetector { by_marker };

    // An llm-logs partition beside the fixture pair, so the per-turn
    // `CHAT_MESSAGE` rows are a comparand-shaped pin (dogfood finding #111:
    // the help loop bypassed `primary_stream`'s logger and a real help turn
    // left NO row where v4 logs every `streamMessage` call). The oracle's mock
    // sits above v4's logger, so the oracle cannot emit these rows — this leg
    // is a v5 pin against the canned-stream count, not a differential.
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
    .unwrap();
    let runner = BuiltInToolRunner::new(db.clone(), dummy_env());
    let summary = NoHelpContextSummaryCheck;
    let mut failed: Vec<String> = Vec::new();

    for case in &spec.cases {
        let user = if case.user.as_deref() == Some("B") {
            spec.users.b.as_str()
        } else {
            spec.users.a.as_str()
        };
        let sink = RecordingSink::new();
        let mut cost = NoCostTracking;
        let mut deps = HelpSendDeps {
            db: &db,
            streaming: &streaming,
            tool_runner: &runner,
            tool_detector: &detector,
            cost: &mut cost,
            summary_check: &summary,
            model_supports_native_tools: true,
        };
        let opts = HelpChatSendOptions {
            content: case.content.clone(),
            file_ids: Vec::new(),
        };
        let result = handle_help_chat_message(&mut deps, &sink, user, &case.chat_id, &opts).await;
        if let Err(e) = &result {
            eprintln!("[{}] orchestrator errored: {e}", case.name);
            failed.push(format!("{}_error", case.name));
            continue;
        }

        // Frames.
        let got_frames: Vec<Value> = sink.events_json().into_iter().map(blank_frame).collect();
        let want_frames: Vec<Value> = oracle_events[&case.name]
            .iter()
            .cloned()
            .map(blank_frame)
            .collect();
        let (g, w) = (
            norm(&Value::Array(got_frames)),
            norm(&Value::Array(want_frames)),
        );
        if g != w {
            eprintln!("[{}] FRAMES MISMATCH:\n{}", case.name, first_diff(&g, &w));
            failed.push(format!("{}_frames", case.name));
        } else {
            eprintln!("[{}] frames OK ({}).", case.name, sink.events().len());
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
        let want_rows: Vec<Value> = oracle_messages[&case.name]
            .iter()
            .map(project_message)
            .collect();
        let (g, w) = (
            norm(&Value::Array(got_rows)),
            norm(&Value::Array(want_rows)),
        );
        if g != w {
            eprintln!("[{}] MESSAGES MISMATCH:\n{}", case.name, first_diff(&g, &w));
            failed.push(format!("{}_messages", case.name));
        } else {
            eprintln!("[{}] messages OK.", case.name);
        }

        // The async tail's enqueue (background_jobs for this chat, rowid order).
        let cid = case.chat_id.clone();
        let got_jobs: Vec<Value> = db
            .read_main(move |c| {
                let mut stmt = c.prepare(
                    "SELECT type, status, userId, payload FROM background_jobs \
                     WHERE json_extract(payload, '$.chatId') = ?1 ORDER BY rowid",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![cid], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows
                    .into_iter()
                    .map(|(t, s, u, p)| {
                        let p: Value = serde_json::from_str(&p).unwrap_or(Value::Null);
                        let mut keys: Vec<String> = p
                            .as_object()
                            .map(|o| o.keys().cloned().collect())
                            .unwrap_or_default();
                        keys.sort();
                        json!({
                            "type": t, "status": s, "userId": u,
                            "payloadKeys": keys,
                            "chatId": p.get("chatId").cloned().unwrap_or(Value::Null),
                            "hasTurnOpener": !p.get("turnOpenerMessageId").map(Value::is_null).unwrap_or(true),
                            "hasAnchor": !p.get("extractionAnchorMessageId").map(Value::is_null).unwrap_or(true),
                            "connectionProfileId": p.get("connectionProfileId").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect())
            })
            .unwrap();
        let (g, w) = (
            norm(&Value::Array(got_jobs)),
            norm(&Value::Array(oracle_jobs[&case.name].clone())),
        );
        if g != w {
            eprintln!("[{}] JOBS MISMATCH:\n{}", case.name, first_diff(&g, &w));
            failed.push(format!("{}_jobs", case.name));
        } else {
            eprintln!("[{}] jobs OK.", case.name);
        }
    }

    let misses = streaming.misses.lock().unwrap().clone();
    if !misses.is_empty() {
        eprintln!("CANNED-STREAM MISSES ({}):", misses.len());
        for m in &misses {
            eprintln!("  {m}");
        }
    }

    // Finding #111's pin: one CHAT_MESSAGE row per completed help turn, with
    // v4's help shape — `messageId` NULL (the loop's `streamMessage` carries
    // none), `characterId` the seat's, `connectionProfileId` the profile's, a
    // measured duration. Mutation: remove the `log_chat_message_call` in
    // `stream_turn` → 0 rows.
    let (rows, null_mid, with_char, with_profile, nonneg_dur): (i64, i64, i64, i64, i64) = db
        .read_llm_logs(|c| {
            Ok(c.query_row(
                "SELECT count(*), coalesce(sum(messageId IS NULL), 0), \
                 coalesce(sum(characterId IS NOT NULL), 0), \
                 coalesce(sum(connectionProfileId IS NOT NULL), 0), \
                 coalesce(sum(durationMs >= 0), 0) \
                 FROM llm_logs WHERE type = 'CHAT_MESSAGE'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?)
        })
        .unwrap();
    // v4 logs on `chunk.done`; a scripted throw logs nothing.
    let expected_chat_message_rows = *streaming.completed_streams.lock().unwrap();
    assert_eq!(
        rows, expected_chat_message_rows,
        "one CHAT_MESSAGE row per completed help turn"
    );
    assert!(rows > 0, "the pin must see at least one turn");
    assert_eq!(null_mid, rows, "the help loop passes no messageId → NULL");
    assert_eq!(with_char, rows);
    assert_eq!(with_profile, rows);
    assert_eq!(nonneg_dur, rows);

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    assert!(
        failed.is_empty(),
        "help-chat-orchestrator tier-3 FAILED: {failed:?}"
    );
}
