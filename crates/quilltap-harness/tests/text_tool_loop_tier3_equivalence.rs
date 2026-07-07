//! Tier-3 differential test: the **text-tool loop**
//! (`quilltap_core::services::text_tool_loop` — v4 `runTextToolPass`,
//! lib/services/chat-message/text-tool-loop.service.ts, W4.1f).
//!
//! Both sides drive `runTextToolPass` over the SAME corpus
//! (`text-tool-loop-tier3.json`). The pass writes NOTHING to the DB (its only DB
//! touch is the preserve closure, a no-op here), so there is no fixture — just the
//! three boundaries pinned identically:
//!
//!   1. `streamMessage` — the jest oracle RECORDS the exact
//!      `provider|model|temperature|messages` canned key per continuation (a
//!      `canned` row); this test replays those sequences through a stateful
//!      [`QueuedStreamingProvider`] (per-key queue), so a threaded-slate divergence
//!      (incl. the nudge text, which rides the ledger into the key) surfaces as a
//!      canned-miss;
//!   2. `executeToolCallWithContext` — canned per-call ([`CannedToolRunner`], keyed
//!      by `name|args|callId`), mirroring the oracle;
//!   3. the STRATEGY — simple-json / text-block use the REAL ported strategy
//!      functions; the provider-text-markers cases use the same synthetic
//!      `<<T:name:argsJson>>` strategy ([`SyntheticStrategy`]) as the oracle.
//!
//! Per case the result state (`fullResponse` / `usage` / `cacheUsage` /
//! `rawResponse` / `thoughtSignature` / `threw` / `toolMessages` [name / success /
//! content / callId / anchorOffset / seq] / `generatedImagePaths`), the ordered
//! SSE-event trace, and the per-continuation `stop` sequences (proving strategy
//! stopSequences forwarding) are compared.
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-text-tool-loop.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- text-tool-loop-tier3
//! Run:
//!   QT_ORACLE_TEXT_TOOL_LOOP=/tmp/oracle-text-tool-loop.ndjson \
//!     cargo test -p quilltap-harness --test text_tool_loop_tier3_equivalence

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    canned_stream_key, StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::primary_stream::{
    EffectiveProfile, PreservePartialOnError, StreamingState,
};
use quilltap_core::services::text_tool_loop::{
    run_text_tool_pass, ProviderTextMarkersStrategy, RunTextToolPassOptions, SimpleJsonStrategy,
    TextBlockStrategy, TextToolStrategy,
};
use quilltap_core::services::tool_call_threading::ThreadedMessage;
use quilltap_core::services::tool_execution::{
    canned_tool_key, create_tool_context, CannedToolRunner, GeneratedImage, ToolCall, ToolMessage,
    ToolResult,
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
struct WireMsg {
    role: String,
    content: String,
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
    strategy: String,
    #[serde(rename = "initialFullResponse")]
    initial_full_response: String,
    #[serde(rename = "formattedMessages")]
    formatted_messages: Vec<WireMsg>,
    #[serde(rename = "cannedTools")]
    canned_tools: Vec<CannedToolSpec>,
}

#[derive(Deserialize)]
struct Spec {
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
    #[serde(rename = "thoughtSignature", default)]
    thought_signature: Option<String>,
    #[serde(default)]
    error: Option<String>,
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
        .join("../../harness/oracle/fixtures/text-tool-loop-tier3.json")
}

// ---------------------------------------------------------------------------
// Stateful canned streaming provider (per-key queue + stop recording).
// ---------------------------------------------------------------------------

struct QueuedStreamingProvider {
    queues: Mutex<HashMap<String, std::collections::VecDeque<Vec<StreamChunkResult>>>>,
    /// Stop sequences forwarded on each stream call, in call order (proves the
    /// engine forwards `strategy.stop_sequences()` to the continuation).
    stops: Mutex<Vec<Vec<String>>>,
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
            stops: Mutex::new(Vec::new()),
        }
    }

    fn stops_len(&self) -> usize {
        self.stops.lock().unwrap().len()
    }

    fn stops_since(&self, start: usize) -> Vec<Vec<String>> {
        self.stops.lock().unwrap()[start..].to_vec()
    }
}

fn chunk_to_result(c: &ChunkW) -> StreamChunkResult {
    if let Some(msg) = &c.error {
        return Err(StreamError::new(msg.clone()));
    }
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
            thought_signature: c.thought_signature.clone(),
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
        self.stops.lock().unwrap().push(params.stop.clone());
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
// ---------------------------------------------------------------------------
// Result serialization.
// ---------------------------------------------------------------------------

fn js_num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.007e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn usage_value(u: Option<StreamUsage>) -> Value {
    match u {
        Some(u) => json!({
            "promptTokens": u.prompt_tokens,
            "completionTokens": u.completion_tokens,
            "totalTokens": u.total_tokens,
        }),
        None => Value::Null,
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
async fn text_tool_loop_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_TEXT_TOOL_LOOP") else {
        eprintln!("SKIP: set QT_ORACLE_TEXT_TOOL_LOOP to the oracle NDJSON (see header).");
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
    let mut oracle_stops: HashMap<String, Value> = HashMap::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.insert(v["case"].as_str().unwrap().into(), v["result"].clone());
            }
            Some("events") => {
                oracle_events.insert(v["case"].as_str().unwrap().into(), v["events"].clone());
            }
            Some("stops") => {
                oracle_stops.insert(v["case"].as_str().unwrap().into(), v["stopPerCall"].clone());
            }
            Some("canned") => oracle_canned.push(serde_json::from_value(v).expect("parse canned")),
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    let provider = QueuedStreamingProvider::from_oracle(&oracle_canned);

    // A throwaway encrypted Db for the (no-op) preserve path.
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    let db_path = std::env::temp_dir().join(format!("qt-ttl-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    let db = Db::open_main(&db_path, PEPPER).expect("open db");

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
            let _ = canned_tool_key(&t.name, &t.arguments, t.call_id.as_deref());
        }
    }

    for c in &spec.cases {
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
            ..Default::default()
        };
        let mut tool_messages: Vec<ToolMessage> = Vec::new();
        let mut generated_image_paths: Vec<GeneratedImage> = Vec::new();

        let base_params = StreamParams {
            messages: Vec::new(),
            model: c.model.clone(),
            temperature: Some(c.temperature),
            max_tokens: None,
            top_p: None,
            tools: None,
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

        let simple = SimpleJsonStrategy;
        let text_block = TextBlockStrategy;
        // W4.7c: the provider-text-markers cases now use the REAL ported strategy
        // (the DeepSeek plugin's composite XML detect/parse/strip), matching the
        // oracle's real-plugin wiring — no longer the synthetic `<<T:…>>` scanner.
        let provider_strategy = ProviderTextMarkersStrategy::built_in("DEEPSEEK");
        let strategy: &dyn TextToolStrategy = match c.strategy.as_str() {
            "simple-json" => &simple,
            "text-block" => &text_block,
            "provider" => &provider_strategy,
            other => panic!("unknown strategy {other}"),
        };

        let stops_start = provider.stops_len();
        let result = run_text_tool_pass(
            &db,
            &provider,
            &sink,
            &runner,
            strategy,
            &mut preserve,
            RunTextToolPassOptions {
                chat_id: c.chat_id.clone(),
                character_id: c.character_id.clone(),
                character_name: c.character_name.clone(),
                provider: c.provider.clone(),
                base_url: None,
                formatted_messages,
                base_params,
                continuation_tools: Some(Value::Array(Vec::new())),
                continuation_use_native_web_search: false,
                tool_context,
                state: &mut state,
                tool_messages: &mut tool_messages,
                generated_image_paths: &mut generated_image_paths,
            },
        )
        .await;
        let threw = result.is_err();

        let got_result = json!({
            "fullResponse": state.full_response,
            "usage": usage_value(state.usage),
            "cacheUsage": Value::Null,
            "rawResponse": state.raw_response.clone().unwrap_or(Value::Null),
            "thoughtSignature": state.thought_signature.clone().map(Value::String).unwrap_or(Value::Null),
            "threw": threw,
            "toolMessages": tool_messages.iter().map(tool_message_value).collect::<Vec<_>>(),
            "generatedImagePaths": generated_image_paths.iter().map(|_: &GeneratedImage| Value::Null).collect::<Vec<_>>(),
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

        let got_stops = json!(provider.stops_since(stops_start));
        let want_stops = oracle_stops
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle stops missing for {}", c.name));
        assert_eq!(
            &got_stops, want_stops,
            "stop-sequences mismatch for case {}\n  rust:   {got_stops}\n  oracle: {want_stops}",
            c.name
        );
    }

    eprintln!(
        "OK: text-tool-loop tier-3 matched oracle ({} cases).",
        spec.cases.len()
    );
}
