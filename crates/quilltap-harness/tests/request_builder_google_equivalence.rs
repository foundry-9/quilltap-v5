//! Differential: the Google request LOGIC (wave 4 / W4.7c part 2).
//!
//! The genai SDK reframes `{model, contents, config}` into the wire body (a
//! transport concern, W4.7d), so this diffs the Quilltap-side request LOGIC the
//! SDK consumes, against v4's REAL plugin (`record-google-request.mjs`):
//!
//!   - `formatMessagesForGoogle` → `contents` / `systemInstruction` /
//!     `shouldDisableTools` (the `thoughtSignature` round-trip + the
//!     thinking-model tool-disable rule), called directly on the plugin;
//!   - `sanitizeSchemaForGoogle` (module-private in v4) → verified through the
//!     `tools` array as it lands ON THE WIRE (the SDK passes `functionDeclarations`
//!     through faithfully), compared against the effective tools this builder
//!     would send.

use quilltap_core::model::request_builder::google::{build_tools, format_messages_for_google};
use quilltap_core::model::request_builder::{RequestInput, RequestMessage, ToolCallMsg};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/request-envelopes/google-request.recorded.ndjson")
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
    RequestInput {
        model: input
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        messages: input
            .get("messages")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(message_from_json).collect())
            .unwrap_or_default(),
        temperature: input.get("temperature").and_then(Value::as_f64),
        max_tokens: input.get("maxTokens").and_then(Value::as_i64),
        top_p: input.get("topP").and_then(Value::as_f64),
        tools: input.get("tools").and_then(Value::as_array).cloned(),
        web_search_enabled: input
            .get("webSearchEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ..Default::default()
    }
}

#[test]
fn google_request_logic_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed google request NDJSON");
    let mut rows = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let case = row["case"].as_str().unwrap();
        let input = input_from_json(&row["input"]);
        rows += 1;

        let (tools, has_tools) = build_tools(&input);
        let fm = format_messages_for_google(&input.messages, &input.model, has_tools);

        // contents byte-exact (the thoughtSignature round-trip + batching).
        let got_contents = serde_json::to_string(&Value::Array(fm.contents.clone())).unwrap();
        let want_contents = serde_json::to_string(&row["contents"]).unwrap();
        assert_eq!(
            got_contents, want_contents,
            "\ngoogle/{case} CONTENTS diverged\n  got:  {got_contents}\n  want: {want_contents}\n"
        );

        // systemInstruction + shouldDisableTools.
        let got_sys = fm
            .system_instruction
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null);
        assert_eq!(
            got_sys, row["systemInstruction"],
            "google/{case} systemInstruction"
        );
        assert_eq!(
            Value::Bool(fm.should_disable_tools),
            row["shouldDisableTools"],
            "google/{case} shouldDisableTools"
        );

        // Sanitizer: the EFFECTIVE tools (None when disabled / absent) vs the wire.
        let effective = if has_tools && !fm.should_disable_tools {
            Value::Array(tools)
        } else {
            Value::Null
        };
        let got_tools = serde_json::to_string(&effective).unwrap();
        let want_tools = serde_json::to_string(&row["wireTools"]).unwrap();
        assert_eq!(
            got_tools, want_tools,
            "\ngoogle/{case} TOOLS (sanitizer) diverged\n  got:  {got_tools}\n  want: {want_tools}\n"
        );
    }

    assert!(rows >= 5, "expected the google corpus, got {rows}");
}
