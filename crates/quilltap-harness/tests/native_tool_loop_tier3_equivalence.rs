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
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ntl.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-native-tool-loop-fixture.ts
//!   QT_FIXTURE_NTL=/tmp/qt-ntl.db QT_ORACLE_OUT=/tmp/oracle-native-tool-loop.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- native-tool-loop-tier3
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
use quilltap_core::services::agent_mode::ResolvedAgentMode;
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
use serde::Deserialize;
use serde_json::{json, Value};

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

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
}

impl QueuedStreamingProvider {
    fn from_oracle(rows: &[CannedRowW]) -> Self {
        let mut queues: HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>> =
            HashMap::new();
        for row in rows {
            let messages: Vec<CompletionMessage> = row
                .messages
                .iter()
                .map(|m| CompletionMessage {
                    role: match m.role.as_str() {
                        "system" => CompletionRole::System,
                        "user" => CompletionRole::User,
                        "assistant" => CompletionRole::Assistant,
                        "tool" => CompletionRole::Tool,
                        other => panic!("unexpected role {other}"),
                    },
                    content: m.content.clone(),
                })
                .collect();
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
        let sequence: Vec<StreamChunkResult> = {
            let mut queues = self.queues.lock().unwrap();
            match queues.get_mut(&key).and_then(|q| q.pop_front()) {
                Some(seq) => seq,
                None => vec![Err(StreamError::new(format!(
                    "no canned stream queued for key ({provider}, model {}, temperature {:?}, {} msgs)",
                    params.model,
                    params.temperature,
                    params.messages.len(),
                )))],
            }
        };
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1));
            for item in sequence {
                let _ = tx.send(item).await;
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
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    let work = std::env::temp_dir().join(format!("qt-ntl-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let provider = QueuedStreamingProvider::from_oracle(&oracle_canned);
    let db = Db::open(DbPaths::main_only(work.clone()), &spec.test_pepper_base64)
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
            format!("{}-msg", c.chat_id),
        );
        let mut state = StreamingState {
            full_response: c.initial_full_response.clone(),
            effective_profile: Some(EffectiveProfile {
                id: "prof-1".into(),
                provider: c.provider.clone(),
                model_name: c.model.clone(),
                base_url: None,
            }),
            effective_api_key: "test-key".into(),
            raw_response: Some(c.initial_raw_response.clone()),
            ..Default::default()
        };
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
        };
        let formatted_messages: Vec<ThreadedMessage> = c
            .formatted_messages
            .iter()
            .map(|m| ThreadedMessage {
                role: m.role.clone(),
                content: m.content.clone(),
                name: None,
                thought_signature: None,
                reasoning_content: None,
                tool_call_id: None,
                tool_calls: None,
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

        run_native_tool_loop(
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
                },
                provider: c.provider.clone(),
                base_url: None,
                formatted_messages,
                base_params,
                tool_context,
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut generated_image_paths,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("loop {} failed: {}", c.name, e.message));

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

    // chats dump: the deterministic agentTurnCount writes (no normalization —
    // both sides start from the same fixture bytes; only agentTurnCount changes).
    let got_chats = db
        .read_main(|conn| quilltap_core::db::dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    drop(db);
    let _ = std::fs::remove_file(&work);

    let want_chats = oracle_chats.expect("oracle chats dump");
    assert_eq!(
        got_chats["rows"], want_chats["rows"],
        "chats rows diverge\n  rust:   {}\n  oracle: {}",
        got_chats["rows"], want_chats["rows"]
    );

    eprintln!(
        "OK: native-tool-loop tier-3 matched oracle ({} cases).",
        spec.cases.len()
    );
}
