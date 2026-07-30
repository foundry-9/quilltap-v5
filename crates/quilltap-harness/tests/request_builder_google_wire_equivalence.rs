//! Differential (W4.7d): the Google genai-SDK config → wire framing — the byte
//! bytes the SDK actually sends, closing the W4.7c deferral.
//!
//! The recorder (`record-request-envelopes.mjs --provider google`) intercepts the
//! outgoing `fetch` of v4's REAL google plugin `streamMessage` and captures the
//! raw request body the genai SDK serialized. This reconstructs the same
//! `RequestInput` and diffs `build_request("GOOGLE", …).body` byte-for-byte
//! against that captured body — proving the reframing (generationConfig split,
//! `{name,args}`→`{args,name}`, systemInstruction wrapper, root key order).
//!
//! **Both modes (P4.11 unit 6).** The fixture now carries a `stream` half
//! (`generateContentStream`) and a `send` half (`generateContent`). The genai
//! wire BODY is byte-identical between them — the whole streaming distinction
//! lives in the URL (`:streamGenerateContent?alt=sse` vs `:generateContent`), so
//! this differential asserts the URL too.
//!
//! Regenerate the fixture (Node 24):
//!   V4=~/source/quilltap-server V5=<repo-root> \
//!     bash <V5>/harness/oracle/providers/regenerate-google-wire.sh
//! (The fixture is committed; no env var needed to run.)

use quilltap_core::model::request_builder::{
    build_request, RequestInput, StreamMessage, ToolCallFunction, ToolCallPayload,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/request-envelopes/google-wire.recorded.ndjson")
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn message_from_json(m: &Value) -> StreamMessage {
    let content = m
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match m.get("role").and_then(Value::as_str).unwrap_or_default() {
        "system" => StreamMessage::system(content),
        "assistant" => StreamMessage::Assistant {
            content,
            tool_calls: m
                .get("toolCalls")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|tc| {
                            // The corpus only records type "function" (v4's
                            // detection emits nothing else); the enum's static
                            // kind makes any other type a loud loader error.
                            assert_eq!(
                                tc.get("type").and_then(Value::as_str),
                                Some("function"),
                                "corpus tool call with a non-function type"
                            );
                            ToolCallPayload {
                                id: tc
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                kind: "function",
                                function: ToolCallFunction {
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
                                },
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            reasoning_content: opt_str(m, "reasoningContent"),
            thought_signature: opt_str(m, "thoughtSignature"),
            cache_control: m.get("cacheControl").cloned(),
        },
        // An id-less tool message is unrepresentable in v5 (the carrying enum
        // requires the call id); a corpus vector carrying one must FAIL the
        // loader loudly rather than be silently reshaped.
        "tool" => StreamMessage::Tool {
            call_id: opt_str(m, "toolCallId")
                .expect("corpus tool message without a toolCallId (unrepresentable in v5)"),
            name: opt_str(m, "name"),
            content,
        },
        _ => StreamMessage::User {
            content,
            cache_control: m.get("cacheControl").cloned(),
            attachments: m
                .get("attachments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        },
    }
}

fn input_from_json(input: &Value, stream: bool) -> RequestInput {
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
        stream,
    }
}

#[test]
fn google_wire_body_matches_recorded() {
    let text = std::fs::read_to_string(corpus_path())
        .unwrap_or_else(|e| panic!("cannot read google wire fixture: {e}"));

    let mut count = 0usize;
    let mut attachment_rows = 0usize;
    let mut modes = std::collections::HashSet::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let case = row.get("case").and_then(Value::as_str).unwrap_or("?");
        let mode = row.get("mode").and_then(Value::as_str).unwrap_or("stream");
        let stream = match mode {
            "stream" => true,
            "send" => false,
            other => panic!("unknown recorded mode {other:?}"),
        };
        modes.insert(mode.to_string());
        let input = input_from_json(&row["input"], stream);
        let built = build_request("GOOGLE", &input).expect("build_request GOOGLE");

        // The recorded body is the exact bytes the SDK sent; compare byte-for-byte.
        let recorded_body: Value =
            serde_json::from_str(row["body"].as_str().expect("recorded body string"))
                .expect("parse recorded body");
        let got = serde_json::to_string(&built.body).unwrap();
        let want = serde_json::to_string(&recorded_body).unwrap();
        assert_eq!(
            got, want,
            "google wire body diverged (case '{case}' [{mode}])"
        );

        // The url is where the streaming split lives for this provider.
        assert_eq!(
            built.url,
            row["url"].as_str().expect("recorded url"),
            "google url diverged (case '{case}' [{mode}])"
        );

        // Attachment results (P4.21): absent = v4 reported nothing.
        let got_results = serde_json::to_value(&built.attachment_results).unwrap();
        match row.get("attachmentResults") {
            Some(want) => assert_eq!(
                &got_results, want,
                "google/{case}[{mode}] attachmentResults diverged"
            ),
            None => assert_eq!(
                got_results,
                serde_json::json!({ "sent": [], "failed": [] }),
                "google/{case}[{mode}] reported attachment results v4 did not"
            ),
        }
        if row["input"]
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|msgs| {
                msgs.iter().any(|m| {
                    m.get("attachments")
                        .and_then(Value::as_array)
                        .is_some_and(|a| !a.is_empty())
                })
            })
        {
            attachment_rows += 1;
        }
        count += 1;
    }

    assert!(count > 0, "google wire fixture looks empty");
    assert!(
        attachment_rows >= 4,
        "the google wire fixture lost its attachment vectors (P4.21), got {attachment_rows}"
    );
    for mode in ["stream", "send"] {
        assert!(
            modes.contains(mode),
            "google wire fixture missing mode {mode}"
        );
    }
    eprintln!("OK: google wire framing matched recorded ({count} cases, both modes).");
}
