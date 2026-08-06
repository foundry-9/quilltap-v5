//! Differential: the per-provider request-envelope builders + the four
//! `RequestTransform` hooks (wave 4 / W4.7c part 2).
//!
//! For every committed line, this reconstructs the [`RequestInput`] from the
//! recorded `input` params, runs the Rust `model::request_builder`, and diffs the
//! built body / url / method byte-for-byte against the request envelope RECORDED
//! by intercepting the outgoing `fetch` of v4's REAL plugin — the SDK / raw fetch
//! sends `JSON.stringify(body)` verbatim, so the body is a byte comparison.
//!
//! **Both halves (P4.11).** Each line carries a `mode`: `"stream"` drives v4's
//! `streamMessage`, `"send"` its `sendMessage`, and the mode becomes
//! `RequestInput.stream`. A line with no `mode` predates P4.11 and means
//! `"stream"`. Recording only the streaming half is how dogfood #23 — every
//! builder hard-coding `stream: true`, so every non-streaming caller in
//! production got an SSE body it could not parse — survived a
//! differential-verified port; the coverage assertion at the bottom is what
//! keeps a (provider, mode) pair from going missing again.
//!
//! A line may instead carry `refused: true`: v4's plugin (or its SDK) rejected
//! the params CLIENT-SIDE and made no request. Those diff against
//! `BuildError::ProviderRefused` — a refusal is a contract, not a gap.
//!
//! Covers every `RequestTransform` branch: anthropic (plain + sampling-rejected +
//! thinking + caching + tools/stop + tool-roundtrip), openai (first-call vs
//! chained + reasoning-model + cache-retention), deepseek (reasoning echo + thinking
//! strip + profile params), plus the plain providers (z-ai, openrouter, ollama,
//! grok). Google's genai-SDK wire framing is deferred to the transport; its request
//! LOGIC is verified in `request_builder_google_equivalence`.
//!
//! The corpus is committed
//! (`harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson`);
//! no env var is needed to run — the family runs in every plain `cargo test`.
//! Regenerate the corpus (Node 24, only after a v4 provider drift) with
//! `harness/oracle/providers/regenerate-request-envelopes.sh`.

use quilltap_core::model::request_builder::{
    build_request, RequestInput, StreamMessage, ToolCallFunction, ToolCallPayload,
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

/// `mode` → `RequestInput.stream`. Absent means the pre-P4.11 streaming vectors.
fn stream_from_mode(row: &Value) -> bool {
    match row.get("mode").and_then(Value::as_str).unwrap_or("stream") {
        "stream" => true,
        "send" => false,
        other => panic!("unknown recorded mode {other:?}"),
    }
}

/// The (provider, case, mode) triples where v4 itself refuses before any HTTP
/// call. Enumerated, never inferred: an unexpected refusal line is a recorder or
/// oracle regression, and a refusal that quietly stops being recorded is exactly
/// the "no case" silence that hid dogfood #23.
const EXPECTED_REFUSALS: &[(&str, &str, &str)] = &[
    // The @openrouter/sdk's ChatToolMessage schema wants `toolCallId`; v4 sends
    // `tool_call_id`, so a tool round-trip fails the SDK's INPUT validation and
    // no request leaves the process. (v4's own defect, carried faithfully.) This
    // is the IMAGE-FREE tool round-trip: v4 bug 31 escapes the image case to the
    // raw vision path, where a tool-role message is fine (the
    // `image-tool-roundtrip` send vector succeeds).
    ("OPENROUTER", "tool-roundtrip", "send"),
    // v4 bug 31 (`43a1b5b1`) RETIRED the two non-streaming vision refusals: an
    // image attachment now routes the send around the SDK to a direct
    // chat-completions POST, so `image-attachment[send]` and
    // `image-attachment-tools[send]` produce bodies, not refusals.
];

#[test]
fn request_builder_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed request-envelope NDJSON");
    let mut rows = 0usize;
    let mut refusals = 0usize;
    let mut pairs = std::collections::HashSet::new();
    // P4.21 attachment-coverage shape (the pre-P4.21 corpus had ZERO attachment
    // vectors — the blind spot that hid dogfood #37; assert the SHAPE, not a
    // hand count, per the corpus-shape-constants-rot rule).
    let mut att_pairs = std::collections::HashSet::new();
    let (mut saw_pdf, mut saw_text_doc, mut saw_no_data, mut saw_multi, mut saw_no_data_failure) =
        (false, false, false, false, false);

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let plugin = row["provider"].as_str().unwrap();
        let case = row["case"].as_str().unwrap();
        let mode = row.get("mode").and_then(Value::as_str).unwrap_or("stream");
        let provider = registry_id(plugin);
        pairs.insert((provider.clone(), mode.to_string()));
        rows += 1;

        let input = input_from_json(&row["input"], stream_from_mode(&row));
        let built = build_request(&provider, &input);

        // Attachment-coverage bookkeeping (over the recorded INPUT, so refusal
        // rows count toward coverage too).
        if let Some(msgs) = row["input"].get("messages").and_then(Value::as_array) {
            for m in msgs {
                let Some(atts) = m.get("attachments").and_then(Value::as_array) else {
                    continue;
                };
                if atts.is_empty() {
                    continue;
                }
                att_pairs.insert((provider.clone(), mode.to_string()));
                if atts.len() >= 2 {
                    saw_multi = true;
                }
                for a in atts {
                    match a.get("mimeType").and_then(Value::as_str) {
                        Some("application/pdf") => saw_pdf = true,
                        Some("text/plain") => saw_text_doc = true,
                        _ => {}
                    }
                    if a.get("data").is_none() {
                        saw_no_data = true;
                    }
                }
            }
        }
        if let Some(failed) = row
            .get("attachmentResults")
            .and_then(|r| r.get("failed"))
            .and_then(Value::as_array)
        {
            if failed
                .iter()
                .any(|f| f.get("error").and_then(Value::as_str) == Some("File data not loaded"))
            {
                saw_no_data_failure = true;
            }
        }

        // A recorded refusal: v4 made no request, so v5 must not build one.
        if row.get("refused").and_then(Value::as_bool) == Some(true) {
            assert!(
                EXPECTED_REFUSALS.contains(&(provider.as_str(), case, mode)),
                "{provider}/{case}[{mode}] recorded a NEW refusal — v4 rejected the \
                 params before any request ({}). Add it to EXPECTED_REFUSALS with \
                 the reason, or fix the recorder.",
                row.get("error").and_then(Value::as_str).unwrap_or("?")
            );
            match built {
                Err(quilltap_core::model::request_builder::BuildError::ProviderRefused(_)) => {}
                Err(e) => panic!("{provider}/{case}[{mode}]: expected ProviderRefused, got {e}"),
                Ok(_) => {
                    panic!("{provider}/{case}[{mode}]: v4 REFUSES these params but v5 built a body")
                }
            }
            refusals += 1;
            continue;
        }

        let built =
            built.unwrap_or_else(|e| panic!("{provider}/{case}[{mode}]: build failed: {e}"));

        // Body byte-exact (the transforms live here).
        let want_body = row["body"].as_str().unwrap();
        let got_body = built.body_string();
        assert_eq!(
            got_body, want_body,
            "\n{provider}/{case}[{mode}] BODY diverged\n  got:  {got_body}\n  want: {want_body}\n"
        );

        // Method + url.
        assert_eq!(
            built.method,
            row["method"].as_str().unwrap(),
            "{provider}/{case}[{mode}] method"
        );
        assert_eq!(
            built.url,
            row["url"].as_str().unwrap(),
            "{provider}/{case}[{mode}] url"
        );

        // Attachment results (P4.21): the recorder keeps the field only when
        // non-empty, so an absent field means v4 reported NOTHING — and v5
        // must report nothing too, not just "we didn't check".
        let got_results = serde_json::to_value(&built.attachment_results).unwrap();
        match row.get("attachmentResults") {
            Some(want) => assert_eq!(
                &got_results, want,
                "{provider}/{case}[{mode}] attachmentResults diverged"
            ),
            None => assert_eq!(
                got_results,
                serde_json::json!({ "sent": [], "failed": [] }),
                "{provider}/{case}[{mode}] reported attachment results v4 did not"
            ),
        }
    }

    assert!(rows >= 25, "expected a substantial corpus, got {rows}");
    assert_eq!(
        refusals,
        EXPECTED_REFUSALS.len(),
        "every enumerated refusal must still be recorded — a vanished refusal line \
         reads as 'no case', which is how a whole mode went unchecked before"
    );

    // Coverage: EVERY provider in BOTH modes. Not an eyeball — the blind spot
    // this closes was precisely a mode nobody noticed was absent.
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
        "OPENAI_COMPATIBLE",
    ] {
        for mode in ["stream", "send"] {
            assert!(
                pairs.contains(&(p.to_string(), mode.to_string())),
                "corpus missing provider {p} in mode {mode}"
            );
        }
    }
    // Every provider must have an attachment vector in BOTH modes — a corpus
    // with zero attachment vectors is exactly how #37 survived green.
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
        "OPENAI_COMPATIBLE",
    ] {
        for mode in ["stream", "send"] {
            assert!(
                att_pairs.contains(&(p.to_string(), mode.to_string())),
                "corpus has no attachment vector for provider {p} in mode {mode}"
            );
        }
    }
    assert!(saw_pdf, "corpus lost its PDF-attachment vector");
    assert!(saw_text_doc, "corpus lost its text-document vectors");
    assert!(saw_no_data, "corpus lost its data-not-loaded vector");
    assert!(saw_multi, "corpus lost its multi-attachment vector");
    assert!(
        saw_no_data_failure,
        "corpus lost the recorded 'File data not loaded' failure arm"
    );

    eprintln!("OK: {rows} request envelopes ({refusals} recorded refusal(s)) matched v4.");
}
