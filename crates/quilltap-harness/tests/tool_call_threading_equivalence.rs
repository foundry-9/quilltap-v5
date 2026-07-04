//! Tier-1 differential test: tool-call message threading (wave 4 / W4.1b.2).
//!
//! Covers `buildAssistantToolCallMessage` / `build_assistant_tool_call_message`
//! and `buildToolResultMessages` / `build_tool_result_messages`. Pure functions;
//! we compare the serialized message(s) as `serde_json::Value` so the JS
//! `JSON.stringify`-drops-`undefined` semantics (mapped to serde
//! `skip_serializing_if = None`) are exercised structurally.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-call-threading.ts \
//!     > /tmp/oracle-tool-call-threading.ndjson
//! Run:
//!   QT_ORACLE_TOOL_CALL_THREADING=/tmp/oracle-tool-call-threading.ndjson \
//!     cargo test -p quilltap-harness --test tool_call_threading_equivalence

use quilltap_core::services::tool_call_threading::{
    build_assistant_tool_call_message, build_tool_result_messages, DetectedToolCall, ToolMessage,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct WireDetectedCall {
    name: String,
    arguments: Value,
    #[serde(rename = "callId")]
    call_id: Option<String>,
}

#[derive(Deserialize)]
struct WireToolMessage {
    #[serde(rename = "toolName")]
    tool_name: String,
    content: String,
    #[serde(rename = "callId")]
    call_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct WireOpts {
    #[serde(rename = "reasoningContent")]
    reasoning_content: Option<String>,
    #[serde(rename = "thoughtSignature")]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    kind: String,
    #[serde(rename = "toolCalls", default)]
    tool_calls: Vec<WireDetectedCall>,
    #[serde(rename = "currentResponse", default)]
    current_response: String,
    #[serde(default)]
    opts: Option<WireOpts>,
    #[serde(rename = "toolMessages", default)]
    tool_messages: Vec<WireToolMessage>,
    out: Value,
}

#[test]
fn tool_call_threading_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TOOL_CALL_THREADING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TOOL_CALL_THREADING to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got: Value = match row.kind.as_str() {
            "assistant" => {
                let calls: Vec<DetectedToolCall> = row
                    .tool_calls
                    .iter()
                    .map(|c| DetectedToolCall {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                        call_id: c.call_id.clone(),
                    })
                    .collect();
                let opts = row.opts.as_ref();
                let msg = build_assistant_tool_call_message(
                    &calls,
                    &row.current_response,
                    opts.and_then(|o| o.reasoning_content.as_deref()),
                    opts.and_then(|o| o.thought_signature.as_deref()),
                );
                serde_json::to_value(&msg).unwrap()
            }
            "result" => {
                let msgs: Vec<ToolMessage> = row
                    .tool_messages
                    .iter()
                    .map(|m| ToolMessage {
                        tool_name: m.tool_name.clone(),
                        content: m.content.clone(),
                        call_id: m.call_id.clone(),
                        ..Default::default()
                    })
                    .collect();
                serde_json::to_value(build_tool_result_messages(&msgs)).unwrap()
            }
            other => panic!("unknown kind '{other}' in case '{}'", row.id),
        };
        assert_eq!(got, row.out, "case '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: tool-call-threading matched oracle ({count} rows).");
}
