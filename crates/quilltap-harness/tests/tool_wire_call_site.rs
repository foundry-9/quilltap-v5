//! P4.13 unit 3 — the tool-linkage CALL-SITE pin (dogfood finding #25's
//! verification leg, modeled on P4.11 unit 7's byte pin in
//! `completion_provider.rs`).
//!
//! Why this test exists: `request_builder_equivalence` feeds the builders a
//! corpus that already contains well-formed tool messages (proving the builders
//! right), and the native-tool-loop tier-3 mocks the provider and asserts the
//! loop's internal `ThreadedMessage` slate (proving the loop right). **Neither
//! executes the conversion between them** — which is exactly where #25 lived:
//! the `[{role, content}]` flattening discarded every tool-call field before
//! any builder ran, on every provider, invisibly to both differentials.
//!
//! So this test drives the REAL `run_native_tool_loop` through the REAL
//! `WireStreamingProvider` (request builder → transport) with only the
//! transport faked, and asserts on the BYTES the transport is handed for the
//! re-stream after a tool result is threaded — per provider family:
//!
//!   - chat-completions (DEEPSEEK / Z_AI / OLLAMA / OPENAI_COMPATIBLE /
//!     OPENROUTER-streaming): `role:"tool"` + `tool_call_id`, and the assistant
//!     turn carrying its `tool_calls`;
//!   - responses API (OPENAI / GROK): `function_call_output` with the right
//!     `call_id`, and the assistant `function_call` item;
//!   - ANTHROPIC: `tool_result` with a NON-EMPTY `tool_use_id`, and the
//!     assistant `tool_use` block;
//!   - GOOGLE: the `functionResponse` part named for the tool, and the model
//!     turn's `functionCall` part.
//!
//! The unit-8 riders are asserted here too: the DeepSeek / Z.AI
//! `reasoning_content` echo on the tool-call turn, and the Google
//! `thoughtSignature` riding the model turn's text part.
//!
//! No oracle env is needed — this is a structural regression pin over shapes
//! already pinned byte-exactly by `request_builder_equivalence`'s recorded
//! corpus; it runs in every plain `cargo test`.

use std::sync::Mutex;

use quilltap_core::db::runtime::Db;
use quilltap_core::model::stream::StreamParams;
use quilltap_core::model::streaming_provider::{ProviderKeySource, WireStreamingProvider};
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::services::agent_mode::{AgentModeSource, ResolvedAgentMode};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::native_tool_loop::{
    run_native_tool_loop, RunNativeToolLoopOptions, ToolCallDetector,
};
use quilltap_core::services::primary_stream::{
    EffectiveProfile, PreservePartialOnError, StreamingState,
};
use quilltap_core::services::tool_call_threading::ThreadedMessage;
use quilltap_core::services::tool_execution::{
    create_tool_context, CannedToolRunner, ToolCall, ToolMessage, ToolResult,
};
use serde_json::{json, Value};

/// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const CALL_ID: &str = "call_wire_1";
const TOOL_NAME: &str = "roll_dice";

/// Records every streaming request the loop hands the transport; each stream is
/// returned EMPTY (channel closed immediately) — the capture happens before any
/// decode, and every decoder's `finish()` is clean on empty input, so the loop
/// simply ends after the re-stream.
struct RecordingStreamTransport {
    requests: Mutex<Vec<TransportRequest>>,
}

impl ProviderTransport for RecordingStreamTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        Box::pin(async move {
            Err(TransportError {
                message: "non-streaming not scripted".to_string(),
                status: None,
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        self.requests.lock().unwrap().push(request.clone());
        Box::pin(async move {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        })
    }
}

struct TestKeys;
impl ProviderKeySource for TestKeys {
    fn key_for(&self, _provider: &str) -> Option<String> {
        Some("test-key".to_string())
    }
}

/// Detects one native tool call (WITH a provider call ID) on the marker raw
/// response, nothing anywhere else — so the loop runs exactly one
/// thread-and-re-stream iteration.
struct MarkerDetector;
impl ToolCallDetector for MarkerDetector {
    fn detect(&self, raw_response: &Value, _provider: &str) -> Vec<ToolCall> {
        if raw_response.get("marker").and_then(Value::as_str) == Some("tool-turn") {
            vec![ToolCall {
                name: TOOL_NAME.to_string(),
                arguments: json!({ "sides": 6 }),
                call_id: Some(CALL_ID.to_string()),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Run the real loop for `provider`/`model` and return the single request body
/// the transport received on the post-tool re-stream.
async fn captured_body(provider: &str, model: &str) -> Value {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open_main(dir.path().join("wire.db"), PEPPER).expect("open db");

    let transport = RecordingStreamTransport {
        requests: Mutex::new(Vec::new()),
    };
    let wire = WireStreamingProvider::new(
        transport,
        TestKeys,
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );

    let mut runner = CannedToolRunner::new();
    runner.register(
        &ToolCall {
            name: TOOL_NAME.to_string(),
            arguments: json!({ "sides": 6 }),
            call_id: Some(CALL_ID.to_string()),
        },
        ToolResult {
            tool_name: TOOL_NAME.to_string(),
            success: true,
            result: json!({ "total": 4 }),
            error: None,
            message: None,
            metadata: None,
        },
    );

    let sink = RecordingSink::new();
    let mut preserve = PreservePartialOnError::new(
        "chat-wire",
        "char-1",
        "Byron",
        vec![],
        "pp-1",
        None,
        "msg-1",
    );
    let mut state = StreamingState {
        full_response: "Let me roll.".into(),
        effective_profile: Some(EffectiveProfile {
            id: "prof-1".into(),
            provider: provider.to_string(),
            model_name: model.to_string(),
            base_url: None,
        }),
        effective_api_key: "test-key".into(),
        raw_response: Some(json!({ "marker": "tool-turn" })),
        // The unit-8 riders: the DeepSeek/Z.AI echo + the Gemini signature
        // ride the assistant tool-call turn the loop threads.
        reasoning_content: Some("weighing the dice".into()),
        thought_signature: Some("sig-abc".into()),
        ..Default::default()
    };
    let mut tool_messages: Vec<ToolMessage> = Vec::new();
    let mut generated_image_paths = Vec::new();

    let base_params = StreamParams {
        messages: Vec::new(),
        model: model.to_string(),
        temperature: Some(0.7),
        max_tokens: None,
        top_p: None,
        tools: None,
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
    };
    let formatted_messages = vec![
        ThreadedMessage {
            role: "system".into(),
            content: "You are Byron.".into(),
            name: None,
            thought_signature: None,
            reasoning_content: None,
            tool_call_id: None,
            tool_calls: None,
            cache_control: None,
            attachments: None,
        },
        ThreadedMessage {
            role: "user".into(),
            content: "Roll a die.".into(),
            name: None,
            thought_signature: None,
            reasoning_content: None,
            tool_call_id: None,
            tool_calls: None,
            cache_control: None,
            attachments: None,
        },
    ];
    let tool_context = create_tool_context(
        "chat-wire",
        "user-1",
        "char-1",
        "pp-1",
        None,
        None,
        None,
        None,
        None,
    );

    run_native_tool_loop(
        &db,
        &wire,
        &sink,
        &runner,
        &MarkerDetector,
        &mut preserve,
        RunNativeToolLoopOptions {
            chat_id: "chat-wire".into(),
            character_id: "char-1".into(),
            character_name: "Byron".into(),
            agent_mode: ResolvedAgentMode {
                enabled: false,
                max_turns: 5,
                enabled_source: AgentModeSource::Global,
            },
            provider: provider.to_string(),
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
    .unwrap_or_else(|e| panic!("{provider} loop failed: {}", e.message));

    // Recover the transport back out of the wire provider we moved it into:
    // the recording lives behind the provider; reach it via the request log
    // captured by the closure below.
    let requests = wire_requests(&wire);
    assert_eq!(
        requests.len(),
        1,
        "{provider}: expected exactly one re-stream request, got {}",
        requests.len()
    );
    let body: Value = serde_json::from_slice(&requests[0].body)
        .unwrap_or_else(|e| panic!("{provider}: body not JSON: {e}"));
    body
}

/// Pull the recorded requests back out of the provider (the transport is owned
/// by the `WireStreamingProvider`).
fn wire_requests(
    wire: &WireStreamingProvider<RecordingStreamTransport, TestKeys>,
) -> Vec<TransportRequest> {
    wire.transport_ref().requests.lock().unwrap().clone()
}

/// The chat-completions family: `role:"tool"` + `tool_call_id`, and the
/// assistant turn carrying its `tool_calls` (finding #25's exact loss points).
fn assert_chat_completions_shape(provider: &str, body: &Value, expect_reasoning_echo: bool) {
    let messages = body["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("{provider}: no messages array: {body}"));

    let assistant = messages
        .iter()
        .find(|m| m["role"] == "assistant" && m.get("tool_calls").is_some())
        .unwrap_or_else(|| {
            panic!("{provider}: no assistant turn carrying tool_calls: {messages:?}")
        });
    let tc = &assistant["tool_calls"][0];
    assert_eq!(tc["id"], CALL_ID, "{provider}: wrong tool_calls id");
    assert_eq!(tc["function"]["name"], TOOL_NAME, "{provider}");
    if expect_reasoning_echo {
        assert_eq!(
            assistant["reasoning_content"], "weighing the dice",
            "{provider}: the DeepSeek/Z.AI reasoning echo must ride the tool-call turn"
        );
    }

    let tool_msg = messages
        .iter()
        .find(|m| m["role"] == "tool")
        .unwrap_or_else(|| panic!("{provider}: the tool result never reached the wire (#25)"));
    assert_eq!(
        tool_msg["tool_call_id"], CALL_ID,
        "{provider}: tool result lost its call id"
    );
}

#[tokio::test]
async fn chat_completions_family_threads_the_linkage() {
    for (provider, model, echo) in [
        ("DEEPSEEK", "deepseek-chat", true),
        ("Z_AI", "glm-4.6", true),
        ("OLLAMA", "llama3.2", false),
        ("OPENAI_COMPATIBLE", "some-model", false),
        ("OPENROUTER", "openai/gpt-4o", false),
    ] {
        let body = captured_body(provider, model).await;
        assert_chat_completions_shape(provider, &body, echo);
    }
}

#[tokio::test]
async fn responses_api_family_threads_the_linkage() {
    for (provider, model) in [("OPENAI", "gpt-4o"), ("GROK", "grok-4")] {
        let body = captured_body(provider, model).await;
        let input = body["input"]
            .as_array()
            .unwrap_or_else(|| panic!("{provider}: no input array: {body}"));

        let call = input
            .iter()
            .find(|i| i["type"] == "function_call")
            .unwrap_or_else(|| panic!("{provider}: no assistant function_call item"));
        assert_eq!(call["call_id"], CALL_ID, "{provider}");
        assert_eq!(call["name"], TOOL_NAME, "{provider}");

        let output = input
            .iter()
            .find(|i| i["type"] == "function_call_output")
            .unwrap_or_else(|| panic!("{provider}: the tool result never reached the wire (#25)"));
        assert_eq!(
            output["call_id"], CALL_ID,
            "{provider}: function_call_output lost its call id"
        );
    }
}

#[tokio::test]
async fn anthropic_threads_the_linkage() {
    let body = captured_body("ANTHROPIC", "claude-sonnet-4-5").await;
    let messages = body["messages"].as_array().expect("messages array");

    let tool_use = messages
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .find(|b| b["type"] == "tool_use")
        .expect("ANTHROPIC: no assistant tool_use block");
    assert_eq!(tool_use["id"], CALL_ID);
    assert_eq!(tool_use["name"], TOOL_NAME);

    let tool_result = messages
        .iter()
        .filter_map(|m| m["content"].as_array())
        .flatten()
        .find(|b| b["type"] == "tool_result")
        .expect("ANTHROPIC: the tool result never reached the wire (#25)");
    let tuid = tool_result["tool_use_id"].as_str().unwrap_or_default();
    assert_eq!(
        tuid, CALL_ID,
        "ANTHROPIC: tool_use_id must be the provider call id, never empty (the old malformed arm)"
    );
}

#[tokio::test]
async fn google_threads_the_linkage_and_thought_signature() {
    let body = captured_body("GOOGLE", "gemini-2.5-flash").await;
    let contents = body["contents"].as_array().expect("contents array");

    // The model turn: functionCall part + the thoughtSignature riding the text
    // part (the unit-8 Gemini round-trip).
    let model_turn = contents
        .iter()
        .find(|c| {
            c["role"] == "model"
                && c["parts"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|part| part.get("functionCall").is_some()))
        })
        .expect("GOOGLE: no model turn with a functionCall part");
    let parts = model_turn["parts"].as_array().unwrap();
    let fc = parts
        .iter()
        .find_map(|p| p.get("functionCall"))
        .expect("functionCall part");
    assert_eq!(fc["name"], TOOL_NAME);
    let text_part = parts
        .iter()
        .find(|p| p.get("text").is_some())
        .expect("GOOGLE: the prose text part");
    assert_eq!(
        text_part["thoughtSignature"], "sig-abc",
        "GOOGLE: the thought signature must ride the tool-call turn's text part"
    );

    // The tool result: a functionResponse part named for the tool.
    let fr = contents
        .iter()
        .filter_map(|c| c["parts"].as_array())
        .flatten()
        .find_map(|p| p.get("functionResponse"))
        .expect("GOOGLE: the tool result never reached the wire (#25)");
    assert_eq!(
        fr["name"], TOOL_NAME,
        "GOOGLE: functionResponse must correlate by the tool name"
    );
}
