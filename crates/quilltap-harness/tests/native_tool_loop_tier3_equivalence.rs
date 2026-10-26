//! Tier-3 differential test: the **native tool loop**
//! (`quilltap_core::services::native_tool_loop` — v4 `runNativeToolLoop`,
//! lib/services/chat-message/native-tool-loop.service.ts, W4.1e).
//!
//! Both sides drive the loop over the SAME corpus
//! (`native-tool-loop-tier3.json`) against a MAIN-db-only fixture (the loop's only
//! DB write is the agent-mode `agentTurnCount` bump), pinning the three boundaries
//! identically:
//!
//!   1. `streamMessage` — the jest oracle RECORDS the exact
//!      `provider|model|temperature|messages` canned key it answered per iteration
//!      (a `canned` row); this test replays those sequences through a stateful
//!      [`QueuedStreamingProvider`] (per-key queue), so a threaded-slate divergence
//!      surfaces as a canned-miss;
//!   2. `detectToolCallsInResponse` — the REAL provider parse (W4.7c): the Rust
//!      [`RegistryToolCallDetector`] over the corpus's REAL anthropic rawResponses
//!      (`content[]` tool_use blocks), mirroring the oracle's real Anthropic
//!      plugin `parseToolCalls`;
//!   3. `executeToolCallWithContext` — canned per-call results
//!      ([`CannedToolRunner`], keyed by `name|args|callId`), mirroring the oracle.
//!
//! Per case the ordered SSE-event trace, the result state
//! (`fullResponse` / `toolMessages` [name / success / content / callId /
//! anchorOffset / seq] / `generatedImagePaths`), and finally the `chats` dump
//! (the deterministic `agentTurnCount` writes) are compared. Cases: a callId
//! single-iteration + continuation (native threading), a no-callId batch (text
//! fallback), a truncation-guard reject, agent-mode submit (real-work replace,
//! no-work preserve, ghost-wrap reject), and a multi-iteration → force-final
//! (agentTurnCount diffed).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-native-tool-loop-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/native-tool-loop-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/native-tool-loop-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ntl.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-native-tool-loop-fixture.ts
//!   QT_FIXTURE_NTL=/tmp/qt-ntl.db QT_ORACLE_OUT=/tmp/oracle-native-tool-loop.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- native-tool-loop-tier3
//! Run:
//!   QT_ORACLE_NATIVE_TOOL_LOOP=/tmp/oracle-native-tool-loop.ndjson \
//!   QT_FIXTURE_NTL=/tmp/qt-ntl.db \
//!     cargo test -p quilltap-harness --test native_tool_loop_tier3_equivalence

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
use quilltap_core::services::agent_mode::{AgentModeSource, ResolvedAgentMode};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::native_tool_loop::{
    run_native_tool_loop, RegistryToolCallDetector, RunNativeToolLoopOptions,
};
use quilltap_core::services::primary_stream::{
    EffectiveProfile, PreservePartialOnError, StreamingState,
};
use quilltap_core::services::tool_call_threading::ThreadedMessage;
use quilltap_core::services::tool_execution::{
    canned_tool_key, create_tool_context, CannedToolRunner, ToolCall, ToolMessage, ToolResult,
};
use quilltap_core::services::turn_control;
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::{json, Value};

// P4.121: shared `llm_logs` materialize/dump helpers (the W4.10b common module).
mod common;

// ---------------------------------------------------------------------------
// Spec.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
struct CannedToolSpec {
    name: String,
    arguments: Value,
    #[serde(rename = "callId", default)]
    call_id: Option<String>,
    success: bool,
    result: Value,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize, Clone)]
struct CaseSpec {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "characterName")]
    character_name: String,
    provider: String,
    model: String,
    temperature: f64,
    #[serde(rename = "agentMode")]
    agent_mode: AgentModeSpec,
    #[serde(rename = "initialFullResponse")]
    initial_full_response: String,
    #[serde(rename = "initialRawResponse")]
    initial_raw_response: Value,
    #[serde(rename = "formattedMessages")]
    formatted_messages: Vec<WireMsg>,
    #[serde(rename = "cannedTools")]
    canned_tools: Vec<CannedToolSpec>,
}

#[derive(Deserialize, Clone)]
struct AgentModeSpec {
    enabled: bool,
    #[serde(rename = "maxTurns")]
    max_turns: i64,
}

#[derive(Deserialize, Clone)]
struct WireMsg {
    role: String,
    content: String,
    /// v4 `msg.name` — the participant name `formatMessagesForProvider` keeps
    /// on a multi-character turn; hashed into `historyTailHash` (P4.128).
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    cases: Vec<CaseSpec>,
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChunkW {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(rename = "rawResponse", default)]
    raw_response: Option<Value>,
    #[serde(default)]
    usage: Option<UsageW>,
    /// P4.D335: the operator's stop lands right after this chunk is read.
    #[serde(rename = "stopTurn", default)]
    stop_turn: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    sequences: Vec<Vec<ChunkW>>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/native-tool-loop-tier3.json")
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue).
// ---------------------------------------------------------------------------

/// One queued canned sequence and its P4.D335 `stopTurn` flag.
type QueuedSequence = (Vec<StreamChunkResult>, bool);

struct QueuedStreamingProvider {
    /// Per key, the queued sequences — each with its P4.D335 `stopTurn` flag
    /// (the operator's stop lands once the sequence has been read whole).
    queues: Mutex<HashMap<String, std::collections::VecDeque<QueuedSequence>>>,
    /// P4.D335: the chat the current case's turn is registered under.
    current_chat: Mutex<String>,
}

impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedRowW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<QueuedSequence>> =
            HashMap::new();
        for row in rows {
            let messages: Vec<CompletionMessage> = row
                .messages
                .iter()
                .map(|m| CompletionMessage {
                    role: CompletionRole::from_v4_wire(m.role.as_str())
                        .unwrap_or_else(|| panic!("unexpected role {}", m.role.as_str())),
                    content: m.content.clone(),
                })
                .collect();
            let key = canned_stream_key(&row.provider, &row.model, row.temperature, &messages);
            let q = queues.entry(key).or_default();
            for seq in &row.sequences {
                q.push_back((
                    seq.iter().map(chunk_to_result).collect(),
                    seq.iter().any(|c| c.stop_turn),
                ));
            }
        }
        Self {
            queues: Mutex::new(queues),
            current_chat: Mutex::new(String::new()),
        }
    }
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if c.done == Some(true) {
        let usage = c.usage.map(|u| StreamUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        return Ok(StreamChunk {
            done: true,
            usage,
            raw_response: c.raw_response.clone(),
            ..Default::default()
        });
    }
    Ok(StreamChunk::content(c.content.clone().unwrap_or_default()))
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
        let chat = self.current_chat.lock().unwrap().clone();
        let (sequence, stop_turn): (Vec<StreamChunkResult>, bool) = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                None => (vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, temperature {:?}, {} msgs)",
                    params.model,
                    params.temperature,
                    params.messages.len(),
                )))], false),
            }
        };
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1));
            for item in sequence {
                let _ = tx.send(item).await;
            }
            if stop_turn {
                // P4.D335: v4's mock aborts the turn when the consumer pulls
                // past the flagged chunk. Twin: hold the sender (the channel
                // stays open), wait until every item was READ, then stop.
                tokio::spawn(async move {
                    while tx.capacity() < tx.max_capacity() {
                        tokio::task::yield_now().await;
                    }
                    turn_control::stop_active_turns(&chat, Some(std::time::Duration::ZERO)).await;
                    drop(tx);
                });
            }
            rx
        }
    }
}

// ---------------------------------------------------------------------------
// Result / event serialization.
// ---------------------------------------------------------------------------

fn js_num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.007e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn tool_message_value(m: &ToolMessage) -> Value {
    json!({
        "toolName": m.tool_name,
        "success": m.success,
        "content": m.content,
        "callId": m.call_id,
        "anchorOffset": m.anchor_offset.map(js_num).unwrap_or(Value::Null),
        "seq": m.seq.map(js_num).unwrap_or(Value::Null),
    })
}

#[tokio::test]
async fn native_tool_loop_tier3_matches_oracle() {
    // P4.D335: the halt INFO is captured through the process-global rig.
    global_capture::install();
    let Ok(oracle_path) = std::env::var("QT_ORACLE_NATIVE_TOOL_LOOP") else {
        eprintln!("SKIP: set QT_ORACLE_NATIVE_TOOL_LOOP to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_NTL") else {
        eprintln!("SKIP: set QT_FIXTURE_NTL to the seed .db (see header).");
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
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_chats: Option<Value> = None;
    let mut oracle_llm_logs: Option<Vec<Value>> = None;
    // P4.D335: v4's `Native tool loop halted…` INFO, per case.
    let mut oracle_halt_logs: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.insert(v["case"].as_str().unwrap().into(), v["result"].clone());
            }
            Some("events") => {
                oracle_events.insert(v["case"].as_str().unwrap().into(), v["events"].clone());
            }
            Some("canned") => oracle_canned.push(serde_json::from_value(v).expect("parse canned")),
            Some("table") => oracle_chats = Some(v),
            Some("llmlogs") => oracle_llm_logs = Some(common::oracle_llm_logs(&v)),
            Some("haltLog") => {
                oracle_halt_logs.insert(v["case"].as_str().unwrap().into(), v["lines"].clone());
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-ntl-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let provider = QueuedStreamingProvider::from_oracle(&oracle_canned);
    // P4.121: a fresh llm-logs partition so every loop leg's CHAT_MESSAGE row
    // lands somewhere we can dump and diff against v4's REAL funnel.
    let ll_work_scratch = tempfile::Builder::new()
        .prefix("qt-ntl-rust-ll-")
        .tempdir()
        .expect("tempdir");
    let ll_work = ll_work_scratch.path().join("ll_work.db");
    common::materialize_llm_logs(&ll_work, &spec.test_pepper_base64);
    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: Some(ll_work.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // One global canned tool runner (unique keys across the corpus).
    let mut runner = CannedToolRunner::new();
    for c in &spec.cases {
        for t in &c.canned_tools {
            let call = ToolCall {
                name: t.name.clone(),
                arguments: t.arguments.clone(),
                call_id: t.call_id.clone(),
            };
            runner.register(
                &call,
                ToolResult {
                    tool_name: t.name.clone(),
                    success: t.success,
                    result: t.result.clone(),
                    error: t.error.clone(),
                    message: t.message.clone(),
                    metadata: None,
                },
            );
            // Silence the unused import in the non-registering path.
            let _ = canned_tool_key(&t.name, &t.arguments, t.call_id.as_deref());
        }
    }

    let mut halts_seen = 0usize;
    for c in &spec.cases {
        let detector = RegistryToolCallDetector::built_in();
        let sink = RecordingSink::new();
        let mut preserve = PreservePartialOnError::new(
            c.chat_id.clone(),
            c.character_id.clone(),
            c.character_name.clone(),
            vec![],
            format!("{}-pp", c.chat_id),
            None,
            // The jest side's `c.chatId.replace(/^3/, '5')` — anchored at the
            // START, as there; `replacen('3', …, 1)` hit the first `3` anywhere.
            c.chat_id
                .strip_prefix('3')
                .map_or_else(|| c.chat_id.clone(), |rest| format!("5{rest}")),
            // P4.D205: this corpus carries no informs.
            vec![],
            None,
        );
        let mut state = StreamingState {
            full_response: c.initial_full_response.clone(),
            effective_profile: Some(EffectiveProfile {
                id: "60000000-0000-4000-8000-000000000001".into(),
                name: "prof-1 profile".into(),
                provider: c.provider.clone(),
                model_name: c.model.clone(),
                base_url: None,
            }),
            effective_api_key: "test-key".into(),
            raw_response: Some(c.initial_raw_response.clone()),
            ..Default::default()
        };
        // P4.D335: every case runs under a registered turn (v4's oracle hands
        // every case a live stop signal); only `stop-between-batches` fires it.
        let registration = turn_control::register_active_turn(&c.chat_id);
        state.turn_control = Some(registration.handle());
        *provider.current_chat.lock().unwrap() = c.chat_id.clone();
        let mut tool_messages: Vec<ToolMessage> = Vec::new();
        let mut generated_image_paths = Vec::new();

        let base_params = StreamParams {
            messages: Vec::new(),
            model: c.model.clone(),
            temperature: Some(c.temperature),
            max_tokens: None,
            top_p: None,
            tools: Some(json!([{ "function": { "name": "noop" } }])),
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: None,
            previous_response_id: None,
            stop: Vec::new(),
            request_timeout_ms: None,
        };
        let formatted_messages: Vec<ThreadedMessage> = c
            .formatted_messages
            .iter()
            .map(|m| ThreadedMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                name: m.name.clone(),
                thought_signature: None,
                reasoning_content: None,
                tool_call_id: None,
                tool_calls: None,
                cache_control: None,
                attachments: None,
            })
            .collect();
        let tool_context = create_tool_context(
            c.chat_id.clone(),
            spec.user_id.clone(),
            c.character_id.clone(),
            format!("{}-pp", c.chat_id),
            None,
            None,
            None,
            None,
            None,
        );

        let (run, lines) = global_capture::capture_async(run_native_tool_loop(
            &db,
            &provider,
            &sink,
            &runner,
            &detector,
            &mut preserve,
            RunNativeToolLoopOptions {
                chat_id: c.chat_id.clone(),
                character_id: c.character_id.clone(),
                character_name: c.character_name.clone(),
                agent_mode: ResolvedAgentMode {
                    enabled: c.agent_mode.enabled,
                    max_turns: c.agent_mode.max_turns,
                    enabled_source: AgentModeSource::Global,
                },
                provider: c.provider.clone(),
                base_url: None,
                formatted_messages,
                base_params,
                tool_context,
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut generated_image_paths,
                log_context: quilltap_core::services::llm_logging::LogContext::none(),
            },
        ))
        .await;
        run.unwrap_or_else(|e| panic!("loop {} failed: {}", c.name, e.message));
        drop(registration);
        // P4.D335: the loop-head halt line, v4's bag (`chatId`, `toolIterations`).
        let want_halt: Vec<String> = oracle_halt_logs
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle haltLog missing for {}", c.name))
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                format!(
                    "INFO quilltap::native_tool_loop {} chat_id={} tool_iterations={}",
                    l["message"].as_str().unwrap(),
                    l["context"]["chatId"].as_str().unwrap(),
                    l["context"]["toolIterations"]
                )
            })
            .collect();
        let got_halt: Vec<String> = lines
            .iter()
            .filter(|l| l.contains("Native tool loop halted"))
            .cloned()
            .collect();
        assert_eq!(got_halt, want_halt, "{}: the loop-head halt INFO", c.name);
        halts_seen += want_halt.len();

        let got_result = json!({
            "fullResponse": state.full_response,
            "toolMessages": tool_messages.iter().map(tool_message_value).collect::<Vec<_>>(),
            "generatedImagePaths": generated_image_paths.iter().map(|_: &quilltap_core::services::tool_execution::GeneratedImage| Value::Null).collect::<Vec<_>>(),
        });
        let want_result = oracle_results
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle result missing for {}", c.name));
        assert_eq!(
            &got_result, want_result,
            "result mismatch for case {}\n  rust:   {got_result}\n  oracle: {want_result}",
            c.name
        );

        let got_events = Value::Array(sink.events_json());
        let want_events = oracle_events
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle events missing for {}", c.name));
        assert_eq!(
            &got_events, want_events,
            "event trace mismatch for case {}\n  rust:   {got_events}\n  oracle: {want_events}",
            c.name
        );
    }

    assert_eq!(
        halts_seen, 1,
        "exactly `stop-between-batches` halts at the loop head"
    );

    // chats dump: the deterministic agentTurnCount writes (no normalization —
    // both sides start from the same fixture bytes; only agentTurnCount changes).
    let got_chats = db
        .read_main(|conn| quilltap_core::db::dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    let got_logs = common::dump_llm_logs(&db);
    drop(db);

    let want_chats = oracle_chats.expect("oracle chats dump");
    assert_eq!(
        got_chats["rows"], want_chats["rows"],
        "chats rows diverge\n  rust:   {}\n  oracle: {}",
        got_chats["rows"], want_chats["rows"]
    );

    // P4.121 (dogfood #129): every loop leg's own CHAT_MESSAGE row, byte-for-byte
    // against v4's real `streamMessage` funnel — per-leg content, usage, cache
    // usage, request-prefix hashes, and `characterId` present on the re-stream
    // but absent on force-final.
    let want_logs = oracle_llm_logs.expect(
        "oracle emitted no llmlogs row — regenerate it (P4.121 relocated the model \
         mock beneath v4's real funnel)",
    );
    for (i, (g, w)) in got_logs.iter().zip(want_logs.iter()).enumerate() {
        if g != w {
            let cols: Vec<&str> = g
                .as_object()
                .unwrap()
                .keys()
                .filter(|k| g[k.as_str()] != w[k.as_str()])
                .map(String::as_str)
                .collect();
            eprintln!("llm_logs row {i} differs in columns {cols:?}");
            for c in &cols {
                eprintln!("  {c}\n    rust:   {}\n    oracle: {}", g[*c], w[*c]);
            }
        }
    }
    assert_eq!(
        got_logs,
        want_logs,
        "llm_logs CHAT_MESSAGE rows diverge (got {} vs oracle {})",
        got_logs.len(),
        want_logs.len()
    );
    assert!(
        got_logs.iter().any(|r| r["characterId"].is_null())
            && got_logs.iter().any(|r| !r["characterId"].is_null()),
        "non-vacuity: the corpus must exercise both a characterId-carrying re-stream \
         and a characterId-less force-final leg"
    );

    eprintln!(
        "OK: native-tool-loop tier-3 matched oracle ({} cases).",
        spec.cases.len()
    );
}
