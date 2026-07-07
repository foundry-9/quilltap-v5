//! Differential: the per-provider request-envelope builders + the four
//! `RequestTransform` hooks (wave 4 / W4.7c part 2).
//!
//! For every committed line, this reconstructs the [`RequestInput`] from the
//! recorded `input` params, runs the Rust `model::request_builder`, and diffs the
//! built body / url / method byte-for-byte against the request envelope RECORDED
//! by intercepting the outgoing `fetch` of v4's REAL plugin `streamMessage`
//! (`record-request-envelopes.mjs`) — the SDK / raw fetch sends
//! `JSON.stringify(body)` verbatim, so the body is a byte comparison.
//!
//! Covers every `RequestTransform` branch: anthropic (plain + sampling-rejected +
//! thinking + caching + tools/stop + tool-roundtrip), openai (first-call vs
//! chained + reasoning-model + cache-retention), deepseek (reasoning echo + thinking
//! strip + profile params), plus the plain providers (z-ai, openrouter, ollama,
//! grok). Google's genai-SDK wire framing is deferred to the transport; its request
//! LOGIC is verified in `request_builder_google_equivalence`.

use quilltap_core::model::request_builder::{
    build_request, RequestInput, RequestMessage, ToolCallMsg,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson")
}

fn registry_id(plugin_name: &str) -> String {
    plugin_name.to_uppercase().replace('-', "_")
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn message_from_json(m: &Value) -> RequestMessage {
    let tool_calls = m
        .get("toolCalls")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|tc| ToolCallMsg {
                    id: tc
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    type_: tc
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("function")
                        .to_string(),
                    name: tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    arguments: tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    RequestMessage {
        role: m
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        content: m
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        tool_call_id: opt_str(m, "toolCallId"),
        tool_calls,
        reasoning_content: opt_str(m, "reasoningContent"),
        thought_signature: opt_str(m, "thoughtSignature"),
        cache_control: m.get("cacheControl").cloned(),
        name: opt_str(m, "name"),
    }
}

fn input_from_json(input: &Value) -> RequestInput {
    let messages = input
        .get("messages")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(message_from_json).collect())
        .unwrap_or_default();
    let stop = input.get("stop").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect()
    });
    RequestInput {
        model: input
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        messages,
        temperature: input.get("temperature").and_then(Value::as_f64),
        max_tokens: input.get("maxTokens").and_then(Value::as_i64),
        top_p: input.get("topP").and_then(Value::as_f64),
        stop,
        tools: input.get("tools").and_then(Value::as_array).cloned(),
        tool_choice: input.get("toolChoice").cloned(),
        response_format: input.get("responseFormat").cloned(),
        web_search_enabled: input
            .get("webSearchEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        profile_parameters: input.get("profileParameters").cloned(),
        cache_key: opt_str(input, "cacheKey"),
        previous_response_id: opt_str(input, "previousResponseId"),
        strict_max_tokens: input
            .get("strictMaxTokens")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        stream: true,
    }
}

#[test]
fn request_builder_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed request-envelope NDJSON");
    let mut rows = 0usize;
    let mut providers = std::collections::HashSet::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let plugin = row["provider"].as_str().unwrap();
        let case = row["case"].as_str().unwrap();
        let provider = registry_id(plugin);
        providers.insert(provider.clone());
        rows += 1;

        let input = input_from_json(&row["input"]);
        let built = build_request(&provider, &input)
            .unwrap_or_else(|e| panic!("{provider}/{case}: build failed: {e}"));

        // Body byte-exact (the transforms live here).
        let want_body = row["body"].as_str().unwrap();
        let got_body = built.body_string();
        assert_eq!(
            got_body, want_body,
            "\n{provider}/{case} BODY diverged\n  got:  {got_body}\n  want: {want_body}\n"
        );

        // Method + url.
        assert_eq!(
            built.method,
            row["method"].as_str().unwrap(),
            "{provider}/{case} method"
        );
        assert_eq!(
            built.url,
            row["url"].as_str().unwrap(),
            "{provider}/{case} url"
        );
    }

    assert!(rows >= 25, "expected a substantial corpus, got {rows}");
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
    ] {
        assert!(providers.contains(p), "corpus missing provider {p}");
    }
}
