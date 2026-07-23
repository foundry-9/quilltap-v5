//! Tool-call message threading — port of v4
//! `lib/services/chat-message/tool-call-threading.ts`.
//!
//! The single source of truth for how a native tool-use turn is re-threaded into
//! the outgoing message slate so a provider can pair its function-calling blocks
//! on the follow-up stream. Both the Salon's `runNativeToolLoop` and the Brahma
//! Console orchestrator build their per-iteration "assistant turn + tool results"
//! through these helpers so the two loops cannot drift apart. The pairing rule:
//!
//!   - When ANY call in the batch carries a provider call ID, attach the full
//!     `toolCalls` array to the assistant turn and emit each result as a native
//!     `tool`-role message keyed by `toolCallId`.
//!   - Otherwise the assistant turn is content-only and each result is framed as
//!     a `[Tool Result: <name>]` user message.
//!
//! `reasoningContent`/`thoughtSignature` are forwarded for providers (e.g.
//! Anthropic) that require the thinking block to accompany the tool-use turn
//! within the same request. They are request-local continuation state, not
//! durable history.

use serde::Serialize;
use serde_json::Value;

use crate::jsstr::js_trim;
use crate::model::stream::StreamMessage;

/// A tool call as surfaced by v4's `detectToolCallsInResponse` — the subset the
/// threading helpers consume. `arguments` is carried as an order-preserving
/// [`Value`] (a JSON object) so its `JSON.stringify` reproduces v4's insertion
/// order.
#[derive(Debug, Clone)]
pub struct DetectedToolCall {
    pub name: String,
    pub arguments: Value,
    pub call_id: Option<String>,
}

// v4's `ToolMessage` lives once in `chat-message/types.ts` and both this module
// and the tool-execution service import it. The v5 canonical definition lives in
// [`super::tool_execution`]; the threading helpers read only its
// `tool_name`/`content`/`call_id` fields. Re-exported so
// `tool_call_threading::ToolMessage` keeps resolving for existing consumers.
pub use super::tool_execution::ToolMessage;

// The canonical `ToolCallPayload` / `ToolCallFunction` moved to the model layer
// (P4.13 unit 1 — the stream boundary carries them now); re-exported so existing
// consumers keep resolving here.
pub use crate::model::stream::{ToolCallFunction, ToolCallPayload};

/// A provider-agnostic outgoing chat message. `None`-valued optional fields are
/// dropped on serialization, matching JS `JSON.stringify` dropping `undefined`
/// keys.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ThreadedMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(rename = "reasoningContent", skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallPayload>>,
}

/// v4 `JSON.stringify(arguments)`: normalize integer-valued floats to bare
/// integers (JS renders `1.0` as `1`) then serialize with the workspace's
/// preserve-order `serde_json`. String escaping and object/array shaping already
/// match `JSON.stringify`; the number normalization is the only divergence to
/// close over the JSON value space tool arguments occupy.
pub(crate) fn stringify_arguments(arguments: &Value) -> String {
    serde_json::to_string(&normalize_numbers(arguments)).unwrap_or_else(|_| "{}".to_string())
}

/// Recursively collapse integer-valued finite floats to JSON integers, mirroring
/// [`crate::db::js_number_to_json`] (which is `pub(crate)`, so re-implemented here
/// over an owned tree).
fn normalize_numbers(v: &Value) -> Value {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.as_i64().is_none()
                    && n.as_u64().is_none()
                    && f.is_finite()
                    && f.fract() == 0.0
                    && f >= i64::MIN as f64
                    && f <= i64::MAX as f64
                {
                    return Value::from(f as i64);
                }
            }
            v.clone()
        }
        Value::Array(a) => Value::Array(a.iter().map(normalize_numbers).collect()),
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            for (k, val) in o {
                m.insert(k.clone(), normalize_numbers(val));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

/// Build the assistant turn that carries a batch of native tool calls. Attaches
/// the `toolCalls` array only when at least one call has a provider call ID;
/// otherwise the turn is content-only. Empty/whitespace prose collapses to `""`.
pub fn build_assistant_tool_call_message(
    tool_calls: &[DetectedToolCall],
    current_response: &str,
    reasoning_content: Option<&str>,
    thought_signature: Option<&str>,
) -> ThreadedMessage {
    let has_call_ids = tool_calls
        .iter()
        .any(|tc| tc.call_id.as_deref().is_some_and(|id| !id.is_empty()));

    let tool_calls_payload = if has_call_ids {
        Some(
            tool_calls
                .iter()
                .filter(|tc| tc.call_id.as_deref().is_some_and(|id| !id.is_empty()))
                .map(|tc| ToolCallPayload {
                    id: tc.call_id.clone().unwrap(),
                    kind: "function",
                    function: ToolCallFunction {
                        name: tc.name.clone(),
                        arguments: stringify_arguments(&tc.arguments),
                    },
                })
                .collect(),
        )
    } else {
        None
    };

    // v4: `currentResponse && currentResponse.trim().length > 0 ? currentResponse : ''`.
    let content = if !js_trim(current_response).is_empty() {
        current_response.to_string()
    } else {
        String::new()
    };

    ThreadedMessage {
        role: "assistant".to_string(),
        content,
        name: None,
        thought_signature: thought_signature.map(str::to_string),
        reasoning_content: reasoning_content.map(str::to_string),
        tool_call_id: None,
        tool_calls: tool_calls_payload,
    }
}

/// Convert one threaded message to the stream boundary's carrying type
/// (P4.13 unit 2) — **lossless**: the tool-call linkage, reasoning content, and
/// thought signature all ride through to the request builders. The old
/// `[{role, content}]` projection at this seam is what dropped the linkage
/// before any builder ran (dogfood finding #25).
pub fn to_stream_message(m: &ThreadedMessage) -> StreamMessage {
    match m.role.as_str() {
        "system" => StreamMessage::System {
            content: m.content.clone(),
        },
        "assistant" => StreamMessage::Assistant {
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone().unwrap_or_default(),
            reasoning_content: m.reasoning_content.clone(),
            thought_signature: m.thought_signature.clone(),
        },
        "tool" => match m.tool_call_id.as_deref().filter(|id| !id.is_empty()) {
            Some(call_id) => StreamMessage::Tool {
                call_id: call_id.to_string(),
                name: m.name.clone(),
                content: m.content.clone(),
            },
            // Unreachable from `build_tool_result_messages` (its id-less arm
            // already emits the user framing below); kept total with the same
            // v4 fallback rule rather than panicking on a hand-built message.
            None => {
                StreamMessage::tool_result_fallback(m.name.as_deref().unwrap_or(""), &m.content)
            }
        },
        _ => StreamMessage::User {
            content: m.content.clone(),
        },
    }
}

/// [`to_stream_message`] over a slate.
pub fn to_stream_messages(messages: &[ThreadedMessage]) -> Vec<StreamMessage> {
    messages.iter().map(to_stream_message).collect()
}

/// Build the per-result messages that follow the assistant tool-call turn. A
/// result with a call ID becomes a native `tool`-role message paired by
/// `toolCallId`; without one it falls back to a `[Tool Result: <name>]` user
/// message. Order is preserved.
pub fn build_tool_result_messages(tool_messages: &[ToolMessage]) -> Vec<ThreadedMessage> {
    tool_messages
        .iter()
        .map(
            |tm| match tm.call_id.as_deref().filter(|id| !id.is_empty()) {
                Some(call_id) => ThreadedMessage {
                    role: "tool".to_string(),
                    content: tm.content.clone(),
                    name: Some(tm.tool_name.clone()),
                    thought_signature: None,
                    reasoning_content: None,
                    tool_call_id: Some(call_id.to_string()),
                    tool_calls: None,
                },
                None => ThreadedMessage {
                    role: "user".to_string(),
                    content: format!("[Tool Result: {}]\n{}", tm.tool_name, tm.content),
                    name: None,
                    thought_signature: None,
                    reasoning_content: None,
                    tool_call_id: None,
                    tool_calls: None,
                },
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_call_ids_content_only() {
        let calls = vec![DetectedToolCall {
            name: "roll".into(),
            arguments: json!({"sides": 6}),
            call_id: None,
        }];
        let msg = build_assistant_tool_call_message(&calls, "  hi  ", None, None);
        assert_eq!(msg.content, "  hi  ");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn whitespace_prose_collapses() {
        let msg = build_assistant_tool_call_message(&[], "   \n\t ", None, None);
        assert_eq!(msg.content, "");
    }

    #[test]
    fn call_id_present_attaches_tool_calls_filtered() {
        let calls = vec![
            DetectedToolCall {
                name: "a".into(),
                arguments: json!({"x": 1}),
                call_id: Some("c1".into()),
            },
            DetectedToolCall {
                name: "b".into(),
                arguments: json!({}),
                call_id: None,
            },
        ];
        let msg = build_assistant_tool_call_message(&calls, "", None, None);
        let tc = msg.tool_calls.unwrap();
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "c1");
        assert_eq!(tc[0].function.arguments, "{\"x\":1}");
    }

    #[test]
    fn results_pair_by_call_id() {
        let msgs = build_tool_result_messages(&[
            ToolMessage {
                tool_name: "search".into(),
                content: "result".into(),
                call_id: Some("c1".into()),
                ..Default::default()
            },
            ToolMessage {
                tool_name: "roll".into(),
                content: "6".into(),
                call_id: None,
                ..Default::default()
            },
        ]);
        assert_eq!(msgs[0].role, "tool");
        assert_eq!(msgs[0].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "[Tool Result: roll]\n6");
    }
}
