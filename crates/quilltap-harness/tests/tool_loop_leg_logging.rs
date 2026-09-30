//! P4.121 (dogfood #129) — every Salon tool-loop leg writes its own
//! `CHAT_MESSAGE` row, the way v4's ONE `streamMessage` funnel does.
//!
//! The byte-level row equivalence (per-leg content, usage, cache usage, request
//! hashes, `characterId` presence) is the tier-3 families' job —
//! `native_tool_loop_tier3` and `text_tool_loop_tier3` now diff their `llm_logs`
//! rows against v4's REAL funnel. This file pins the two things a corpus of
//! fixed profiles and a `none()` log context cannot see:
//!
//!   1. the row is written under `state.effective_profile` — the UNDERSTUDY after
//!      a failover (v4 `streaming.effectiveProfile`) — not the loop's own
//!      `provider` option; and
//!   2. the run's id rides the row (`LogContext`), so an autonomous room's budget
//!      (`get_total_token_usage_for_run`, read by `enclave::step`) charges every
//!      loop leg, `budgetExcludeCacheHits` honoured.
//!
//! Self-contained: no oracle, no env vars — it cannot SKIP.

use std::collections::HashMap;

use quilltap_core::db::llm_logs::LLMLogsRepository;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{CompletionMessage, CompletionRole};
use quilltap_core::model::stream::{
    CannedStreamingProvider, StreamCacheUsage, StreamChunk, StreamParams, StreamUsage,
};
use quilltap_core::services::agent_mode::{AgentModeSource, ResolvedAgentMode};
use quilltap_core::services::chat_events::RecordingSink;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::native_tool_loop::{
    run_native_tool_loop, RunNativeToolLoopOptions, ToolCallDetector,
};
use quilltap_core::services::primary_stream::{
    EffectiveProfile, PreservePartialOnError, StreamingState,
};
use quilltap_core::services::text_tool_loop::{
    run_text_tool_pass, RunTextToolPassOptions, TextBlockStrategy,
};
use quilltap_core::services::tool_call_threading::ThreadedMessage;
use quilltap_core::services::tool_execution::{
    create_tool_context, CannedToolRunner, ToolCall, ToolResult,
};
use serde_json::{json, Value};

mod common;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "10000000-0000-4000-8000-000000000001";
const CHAT: &str = "30000000-0000-4000-8000-000000000001";
const CHARACTER: &str = "20000000-0000-4000-8000-000000000001";
const MESSAGE: &str = "50000000-0000-4000-8000-000000000001";
const RUN: &str = "70000000-0000-4000-8000-000000000001";
const UNDERSTUDY: &str = "60000000-0000-4000-8000-0000000000aa";

struct MapDetector(HashMap<String, Vec<ToolCall>>);
impl ToolCallDetector for MapDetector {
    fn detect(&self, raw: &Value, _provider: &str) -> Vec<ToolCall> {
        self.0.get(&raw.to_string()).cloned().unwrap_or_default()
    }
}

fn params() -> StreamParams {
    StreamParams {
        messages: Vec::new(),
        model: "m".into(),
        temperature: Some(0.7),
        max_tokens: None,
        top_p: None,
        tools: Some(json!([{ "function": { "name": "noop" } }])),
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
        request_timeout_ms: None,
    }
}

fn preserver() -> PreservePartialOnError {
    PreservePartialOnError::new(
        CHAT,
        CHARACTER,
        "Friday",
        vec![],
        "pp-1",
        None,
        MESSAGE,
        vec![],
    )
}

fn state(full_response: &str, raw: Option<Value>) -> StreamingState {
    StreamingState {
        full_response: full_response.into(),
        raw_response: raw,
        effective_profile: Some(EffectiveProfile {
            id: UNDERSTUDY.into(),
            name: "Understudy".into(),
            provider: "ANTHROPIC".into(),
            model_name: "understudy-model".into(),
            base_url: None,
        }),
        effective_api_key: "understudy-key".into(),
        ..Default::default()
    }
}

fn user_message(content: &str) -> ThreadedMessage {
    ThreadedMessage {
        role: "user".into(),
        content: content.into(),
        name: None,
        thought_signature: None,
        reasoning_content: None,
        tool_call_id: None,
        tool_calls: None,
        cache_control: None,
        attachments: None,
    }
}

fn ok_tool(name: &str, result: Value) -> ToolResult {
    ToolResult {
        tool_name: name.into(),
        success: true,
        result,
        error: None,
        message: None,
        metadata: None,
    }
}

fn rows_for(db: &Db, run: &str, include_cache_hits: bool) -> (f64, f64) {
    let t = db
        .read_llm_logs(|c| {
            Ok(LLMLogsRepository::new(c).get_total_token_usage_for_run(run, include_cache_hits))
        })
        .expect("read totals");
    (t.prompt_tokens, t.total_tokens)
}

#[tokio::test]
async fn native_restream_logs_under_the_understudy_and_charges_the_run() {
    let (db, _dir) = common::open_main_and_llm_logs_db(PEPPER);
    let marker0 = json!({ "marker": "m0" });
    let marker1 = json!({ "marker": "m1" });
    let call = ToolCall {
        name: "state".into(),
        arguments: json!({ "key": "mood" }),
        call_id: Some("c1".into()),
    };
    let mut by_raw = HashMap::new();
    by_raw.insert(marker0.to_string(), vec![call.clone()]);
    by_raw.insert(marker1.to_string(), vec![]);
    let detector = MapDetector(by_raw);
    let mut runner = CannedToolRunner::new();
    runner.register(&call, ok_tool("state", json!({ "mood": "curious" })));

    let cont = vec![
        CompletionMessage::user("hi"),
        CompletionMessage {
            role: CompletionRole::Assistant,
            content: "before ".into(),
        },
        CompletionMessage {
            role: CompletionRole::Tool,
            content: "{\n  \"mood\": \"curious\"\n}".into(),
        },
    ];
    let provider = CannedStreamingProvider::new().with_stream(
        "ANTHROPIC",
        "m",
        Some(0.7),
        &cont,
        vec![
            Ok(StreamChunk::content("after")),
            Ok(StreamChunk {
                done: true,
                usage: Some(StreamUsage {
                    prompt_tokens: 5,
                    completion_tokens: 1,
                    total_tokens: 6,
                }),
                cache_usage: Some(StreamCacheUsage {
                    cache_read_input_tokens: Some(4),
                    ..Default::default()
                }),
                raw_response: Some(marker1.clone()),
                ..Default::default()
            }),
        ],
    );

    let sink = RecordingSink::new();
    let mut preserve = preserver();
    let mut st = state("before ", Some(marker0));
    let mut tool_messages = Vec::new();
    let mut images = Vec::new();
    run_native_tool_loop(
        &db,
        &provider,
        &sink,
        &runner,
        &detector,
        &mut preserve,
        RunNativeToolLoopOptions {
            chat_id: CHAT.into(),
            character_id: CHARACTER.into(),
            character_name: "Friday".into(),
            agent_mode: ResolvedAgentMode {
                enabled: false,
                max_turns: 10,
                enabled_source: AgentModeSource::Global,
            },
            provider: "ANTHROPIC".into(),
            base_url: None,
            formatted_messages: vec![user_message("hi")],
            base_params: params(),
            tool_context: create_tool_context(
                CHAT, USER, CHARACTER, "pp-1", None, None, None, None, None,
            ),
            state: &mut st,
            tool_messages: &mut tool_messages,
            generated_image_paths: &mut images,
            log_context: LogContext {
                autonomous_run_id: Some(RUN.into()),
            },
        },
    )
    .await
    .expect("loop ok");

    let rows = common::dump_llm_logs(&db);
    assert_eq!(rows.len(), 1, "one re-stream, one row: {rows:#?}");
    let r = &rows[0];
    assert_eq!(r["type"], "CHAT_MESSAGE");
    assert_eq!(r["messageId"], MESSAGE);
    assert_eq!(r["chatId"], CHAT);
    assert_eq!(
        r["characterId"], CHARACTER,
        "the native re-stream passes characterId (v4 native-tool-loop.service.ts:340-350)"
    );
    // The understudy's profile, not the loop's `provider` option.
    assert_eq!(r["connectionProfileId"], UNDERSTUDY);
    assert_eq!(r["modelName"], "understudy-model");
    assert_eq!(r["autonomousRunId"], RUN);
    // Only THIS leg's text — `state.full_response` is the whole turn.
    assert_eq!(
        serde_json::from_str::<Value>(r["response"].as_str().unwrap()).unwrap()["content"],
        "after"
    );

    // The budget charge: the leg's own usage, cache reads added back only when
    // the room counts them (`budgetExcludeCacheHits = 0`).
    assert_eq!(rows_for(&db, RUN, false), (5.0, 6.0));
    assert_eq!(rows_for(&db, RUN, true), (9.0, 10.0));
}

#[tokio::test]
async fn text_continuation_logs_under_the_understudy_and_charges_the_run() {
    let (db, _dir) = common::open_main_and_llm_logs_db(PEPPER);
    let initial = "Let me check.[[state key=\"mood\"]][[/state]]";
    let parsed = quilltap_core::services::pseudo_tool::parse_text_blocks_from_response(initial);
    let call = ToolCall {
        name: parsed[0].name.clone(),
        arguments: parsed[0].arguments.clone(),
        call_id: None,
    };
    let mut runner = CannedToolRunner::new();
    runner.register(&call, ok_tool(&call.name, json!({ "mood": "curious" })));
    let tool_content = "{\n  \"mood\": \"curious\"\n}";
    let cont = vec![
        CompletionMessage {
            role: CompletionRole::Assistant,
            content: initial.into(),
        },
        CompletionMessage {
            role: CompletionRole::User,
            content: format!("[Tool Result: {}]\n{}", call.name, tool_content),
        },
    ];
    let provider = CannedStreamingProvider::new().with_stream(
        "ANTHROPIC",
        "m",
        Some(0.7),
        &cont,
        vec![
            Ok(StreamChunk::content("all calm")),
            Ok(StreamChunk {
                done: true,
                usage: Some(StreamUsage {
                    prompt_tokens: 7,
                    completion_tokens: 2,
                    total_tokens: 9,
                }),
                ..Default::default()
            }),
        ],
    );

    let sink = RecordingSink::new();
    let mut preserve = preserver();
    let mut st = state(initial, None);
    let mut tool_messages = Vec::new();
    let mut images = Vec::new();
    run_text_tool_pass(
        &db,
        &provider,
        &sink,
        &runner,
        &TextBlockStrategy,
        &mut preserve,
        RunTextToolPassOptions {
            chat_id: CHAT.into(),
            character_id: CHARACTER.into(),
            character_name: "Friday".into(),
            provider: "ANTHROPIC".into(),
            base_url: None,
            formatted_messages: vec![],
            base_params: params(),
            continuation_tools: None,
            continuation_use_native_web_search: false,
            tool_context: create_tool_context(
                CHAT, USER, CHARACTER, "pp-1", None, None, None, None, None,
            ),
            state: &mut st,
            tool_messages: &mut tool_messages,
            generated_image_paths: &mut images,
            log_context: LogContext {
                autonomous_run_id: Some(RUN.into()),
            },
        },
    )
    .await
    .expect("pass ok");

    let rows = common::dump_llm_logs(&db);
    assert_eq!(rows.len(), 1, "one continuation, one row: {rows:#?}");
    let r = &rows[0];
    assert_eq!(r["messageId"], MESSAGE);
    assert!(
        r["characterId"].is_null(),
        "the text continuation passes NO characterId (v4 text-tool-loop.service.ts:390-400)"
    );
    assert_eq!(r["connectionProfileId"], UNDERSTUDY);
    assert_eq!(r["modelName"], "understudy-model");
    assert_eq!(r["autonomousRunId"], RUN);
    assert_eq!(rows_for(&db, RUN, false), (7.0, 9.0));
}
