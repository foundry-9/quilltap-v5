//! Differential: the production streaming composer (P4.1a,
//! `model::streaming_provider::WireStreamingProvider`) — the "free"
//! differential over the committed W4.7b wire fixtures.
//!
//! For every committed wire transcript
//! (`harness/oracle/fixtures/streams/<decoder>/*.wire`), this replays the bytes
//! through the FULL compose path — `StreamParams` → `build_request` → a fake
//! `ProviderTransport` feeding the transcript → the manifest-selected decoder →
//! the `StreamChunk` channel — at whole-buffer AND byte-at-a-time pushes, and
//! diffs the emitted chunk sequence (+ any terminal error) against the same
//! `*.recorded.ndjson` the decoder differential proved (recorded by driving
//! v4's REAL plugin `streamMessage`). The composer must reproduce what the
//! decoders proved, now through the full compose path (decoder selection by
//! manifest, the flavor split, the pump thread, EOF finish).
//!
//! Ollama is replayed at whole-buffer, line-aligned AND byte-at-a-time
//! chunkings. The older caveat here — that v4's plugin split each network read
//! on `\n` with NO cross-read buffer, so byte-at-a-time diverged BY DESIGN —
//! has been stale since v4 bug 35 (`43a1b5b1`) taught the splitter to carry the
//! partial line and the incomplete UTF-8 tail across reads; the decoder
//! differential has replayed all three chunkings since. Measured at P4.D78: the
//! byte chunking is green through the full compose path too, so the exclusion
//! is gone from the code as well as from this comment.
//!
//! Google note: the composer derives `is_thinking_model` from the call's model
//! via the ported v4 predicate (`request_builder::google::is_thinking_model`),
//! which is exactly what v4's plugin did when the fixtures were RECORDED — so
//! the recordings are the truth under the real predicate (the decoder
//! differential's `cases.json` flag is a corpus knob for the decoder-level
//! test; the flag only gates the terminal `finalContent` fallback, inert on
//! every text-streaming wire).
//!
//! No env vars needed — the fixtures + recordings are committed; regenerate
//! with `harness/oracle/providers/regenerate-stream-fixtures.sh` after a v4
//! provider drift.
//!
//! Run:
//!   cargo test -p quilltap-harness --test streaming_composer_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::model::stream::{
    StreamChunk, StreamError, StreamParams, StreamingCompletionProvider,
};
use quilltap_core::model::streaming_provider::WireStreamingProvider;
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::services::dangerous_content::refusal::code_string;
use serde_json::{json, Map, Value};

fn streams_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/streams")
}

/// A parsed oracle case row from a `<provider>.recorded.ndjson`.
struct OracleCase {
    provider: String,
    case: String,
    error: Option<String>,
    chunks: Vec<Value>,
    /// P4.122: the thrown value's classifier fields on an erroring row.
    thrown: Option<Value>,
    /// P4.122 item 6: the ERROR lines v4's plugin logged while the case ran
    /// (`record-stream-fixtures.mjs` `pluginErrorLog`); empty when none.
    plugin_error_log: Vec<Value>,
}

fn load_recorded(decoder: &str) -> Vec<OracleCase> {
    let dir = streams_dir().join(decoder);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".recorded.ndjson") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line).unwrap();
            out.push(OracleCase {
                provider: v["provider"].as_str().unwrap().to_string(),
                case: v["case"].as_str().unwrap().to_string(),
                error: v["error"].as_str().map(|s| s.to_string()),
                chunks: v["chunks"].as_array().cloned().unwrap_or_default(),
                thrown: v.get("thrown").cloned(),
                plugin_error_log: v
                    .get("pluginErrorLog")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
    }
    out
}

fn load_cases(decoder: &str) -> Vec<Value> {
    let path = streams_dir().join(decoder).join("cases.json");
    let text = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn read_wire(decoder: &str, wire: &str) -> Vec<u8> {
    std::fs::read(streams_dir().join(decoder).join(wire)).unwrap()
}

/// The fixture's lowercase provider tag → the canonical registry id.
fn canonical_provider(tag: &str) -> &'static str {
    match tag {
        "anthropic" => "ANTHROPIC",
        "openai" => "OPENAI",
        "grok" => "GROK",
        "google" => "GOOGLE",
        "deepseek" => "DEEPSEEK",
        "z-ai" => "Z_AI",
        "openrouter" => "OPENROUTER",
        "ollama" => "OLLAMA",
        // P4.D83: the `chat_completions_sse` family gained its first
        // OpenAiCompatible cases, and this composer is the fixtures' SECOND
        // consumer — the full-workspace gate is what surfaced that.
        "openai-compatible" => "OPENAI_COMPATIBLE",
        // P4.D101: the SAME class, caught the same way. Adding NanoGPT's five
        // cases to `chat_completions_sse/cases.json` reddened this composer,
        // not the decoder differential — a per-family run would have missed it.
        "nanogpt" => "NANOGPT",
        other => panic!("unknown fixture provider tag {other}"),
    }
}

/// Serialise a Rust `StreamChunk` to the exact v4 `StreamChunk` JSON shape
/// (camelCase, omitted-when-absent) — identical to the decoder differential's.
fn chunk_to_oracle(c: &StreamChunk) -> Value {
    let mut m = Map::new();
    m.insert("content".into(), json!(c.content));
    m.insert("done".into(), json!(c.done));
    if let Some(u) = &c.usage {
        m.insert(
            "usage".into(),
            json!({
                "promptTokens": u.prompt_tokens,
                "completionTokens": u.completion_tokens,
                "totalTokens": u.total_tokens,
            }),
        );
    }
    if let Some(ar) = &c.attachment_results {
        let failed: Vec<Value> = ar
            .failed
            .iter()
            .map(|f| json!({ "id": f.id, "error": f.error }))
            .collect();
        m.insert(
            "attachmentResults".into(),
            json!({ "sent": ar.sent, "failed": failed }),
        );
    }
    if let Some(raw) = &c.raw_response {
        m.insert("rawResponse".into(), raw.clone());
    }
    if let Some(rpu) = &c.raw_provider_usage {
        m.insert("rawProviderUsage".into(), rpu.clone());
    }
    if let Some(cu) = &c.cache_usage {
        let mut cm = Map::new();
        if let Some(v) = cu.cached_tokens {
            cm.insert("cachedTokens".into(), json!(v));
        }
        if let Some(v) = cu.cache_discount {
            cm.insert("cacheDiscount".into(), json!(v));
        }
        if let Some(v) = cu.cache_creation_input_tokens {
            cm.insert("cacheCreationInputTokens".into(), json!(v));
        }
        if let Some(v) = cu.cache_read_input_tokens {
            cm.insert("cacheReadInputTokens".into(), json!(v));
        }
        m.insert("cacheUsage".into(), Value::Object(cm));
    }
    if let Some(ts) = &c.thought_signature {
        m.insert("thoughtSignature".into(), json!(ts));
    }
    if let Some(rc) = &c.reasoning_content {
        m.insert("reasoningContent".into(), json!(rc));
    }
    Value::Object(m)
}

/// Strip fields outside the decoder's semantic output (the decoder
/// differential's two documented normalisations: v4's chunk `toolCalls` and
/// google's SDK `sdkHttpResponse` transport wrapper).
fn strip_ignored(mut v: Value) -> Value {
    if let Value::Object(ref mut m) = v {
        m.remove("toolCalls");
        if let Some(Value::Object(raw)) = m.get_mut("rawResponse") {
            raw.remove("sdkHttpResponse");
        }
    }
    v
}

/// A transport replaying scripted byte frames.
struct ReplayTransport {
    frames: Vec<Vec<u8>>,
    seen: Mutex<Option<TransportRequest>>,
}

impl ProviderTransport for ReplayTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        Box::pin(async move { Err(TransportError::connect("non-streaming not scripted")) })
    }
    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        *self.seen.lock().unwrap() = Some(request.clone());
        let frames = self.frames.clone();
        Box::pin(async move {
            let (tx, rx) = tokio::sync::mpsc::channel(frames.len().max(1));
            for f in frames {
                let _ = tx.send(Ok(f)).await;
            }
            Ok(rx)
        })
    }
}

fn params(model: &str) -> StreamParams {
    StreamParams {
        messages: vec![quilltap_core::model::stream::StreamMessage::user(
            "composer replay",
        )],
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
        request_timeout_ms: None,
    }
}

/// Drive the composer over `pieces` and split the drained items into
/// (chunks, first error).
async fn drive_composer(
    provider: &str,
    model: &str,
    pieces: Vec<Vec<u8>>,
) -> (Vec<StreamChunk>, Option<StreamError>) {
    let transport = ReplayTransport {
        frames: pieces,
        seen: Mutex::new(None),
    };
    let mut keys = HashMap::new();
    keys.insert(provider.to_string(), "synthetic-key".to_string());
    let composer = WireStreamingProvider::new(
        transport,
        keys,
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );
    let mut rx = composer
        .stream_message(provider, None, &params(model))
        .await;
    let mut chunks = Vec::new();
    let mut error: Option<StreamError> = None;
    while let Some(item) = rx.recv().await {
        match item {
            Ok(c) => chunks.push(c),
            Err(e) => {
                error = Some(e);
                // The composer stops after an error item (v4's generator
                // throws) — the channel closes right after.
            }
        }
    }
    (chunks, error)
}

fn assert_matches(
    oracle: &OracleCase,
    chunking: &str,
    chunks: &[StreamChunk],
    err: &Option<StreamError>,
) {
    match (&oracle.error, err) {
        (Some(want), Some(got)) => {
            assert_eq!(
                &got.message, want,
                "{}/{} [{}]: error message mismatch",
                oracle.provider, oracle.case, chunking
            );
            // P4.122: the pump's mid-stream arms carry the refusal side the
            // decoder read (both of them — the EOF-flush frame reaches the
            // FINISH arm). v4's thrown `code` must arrive on the side.
            let thrown = oracle
                .thrown
                .as_ref()
                .expect("an erroring row records `thrown`");
            let want_code = thrown.get("code").and_then(code_string);
            let want_nested = thrown.get("errorCode").and_then(code_string);
            let label = format!("{}/{} [{}]", oracle.provider, oracle.case, chunking);
            match got.refusal.as_deref() {
                Some(side) => {
                    assert_eq!(side.message, thrown["message"].as_str().unwrap(), "{label}");
                    assert_eq!(side.code, want_code, "{label}: side code");
                    assert_eq!(side.nested_code, want_nested, "{label}: side nested code");
                    assert_eq!(
                        side.name.as_deref(),
                        thrown["name"].as_str(),
                        "{label}: name"
                    );
                    assert_eq!(
                        side.status.map(u64::from),
                        thrown.get("status").and_then(Value::as_u64),
                        "{label}: side status"
                    );
                }
                None => assert!(
                    want_code.is_none() && want_nested.is_none(),
                    "{label}: v4 threw a coded error ({thrown}) but the pump attached no side"
                ),
            }
        }
        (None, None) => {}
        (Some(_), None) => panic!(
            "{}/{} [{}]: oracle errored but the composer did not",
            oracle.provider, oracle.case, chunking
        ),
        (None, Some(e)) => panic!(
            "{}/{} [{}]: the composer errored ({}) but the oracle did not",
            oracle.provider, oracle.case, chunking, e.message
        ),
    }
    assert_eq!(
        chunks.len(),
        oracle.chunks.len(),
        "{}/{} [{}]: chunk count {} != oracle {}",
        oracle.provider,
        oracle.case,
        chunking,
        chunks.len(),
        oracle.chunks.len()
    );
    for (i, (rust, want)) in chunks.iter().zip(oracle.chunks.iter()).enumerate() {
        let got = strip_ignored(chunk_to_oracle(rust));
        let want = strip_ignored(want.clone());
        assert_eq!(
            got,
            want,
            "{}/{} [{}]: chunk {} mismatch\n got: {}\nwant: {}",
            oracle.provider,
            oracle.case,
            chunking,
            i,
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&want).unwrap()
        );
    }
}

/// P4.122 item 6 — the plugin catch line, which the pump logs at its
/// mid-stream arms: the captured ERROR lines at the streaming-provider target
/// must be exactly v4's recorded `pluginErrorLog`, field for field — so every
/// OPENAI_COMPATIBLE / DEEPSEEK / NANOGPT throw logs ONE `… API error in
/// streamMessage` line, a Google tail throw logs ONE `Error streaming from
/// Google Gemini API` line, and every other row (Z.AI, the Responses plugins,
/// OpenRouter, a clean stream) logs none.
///
/// P4.151 C: selected by LEVEL + TARGET, not by the substring `"API error in
/// streamMessage"` — the old filter could not see a Google catch line at all,
/// so the six clean Google rows' silence leg could not fail, and a Google
/// erroring row PANICKED on the `baseUrl` unwrap instead of comparing. v4's
/// line is rendered provider-neutrally, the rule copied from
/// `text_http_errors_equivalence.rs` (`render_v4_line`, not edited there):
/// the message, then every `context` key in v4's order (`preserve_order`),
/// then the logger's third-argument `error` when it is a string.
fn assert_catch_lines(oracle: &OracleCase, chunking: &str, lines: &[String]) {
    const PREFIX: &str = "ERROR quilltap::model::streaming_provider ";
    let got: Vec<&String> = lines.iter().filter(|l| l.starts_with(PREFIX)).collect();
    let want: Vec<String> = oracle
        .plugin_error_log
        .iter()
        .map(|e| {
            let mut out = format!("{PREFIX}{}", e["message"].as_str().unwrap());
            if let Some(ctx) = e["context"].as_object() {
                for (k, v) in ctx {
                    out.push_str(&format!(" {k}={}", render_value(v)));
                }
            }
            if let Some(err) = e["error"].as_str() {
                out.push_str(&format!(" error={err}"));
            }
            out
        })
        .collect();
    assert_eq!(
        got.iter().map(|l| l.as_str()).collect::<Vec<_>>(),
        want.iter().map(String::as_str).collect::<Vec<_>>(),
        "{}/{} [{}]: the plugin catch line",
        oracle.provider,
        oracle.case,
        chunking
    );
}

/// A v4 context value as the capture renders a `%`-sigil field: a string
/// unquoted, anything else as its JSON text (copied from
/// `text_http_errors_equivalence.rs`'s `render_value`).
fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whole-buffer + byte-at-a-time (SSE wires).
fn chunkings_sse(wire: &[u8]) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    vec![
        ("whole", vec![wire.to_vec()]),
        ("byte", wire.iter().map(|b| vec![*b]).collect()),
    ]
}

/// Whole-buffer + line-aligned + byte-at-a-time (ollama NDJSON). v4 bug 35's
/// buffered splitter makes the decoder push-boundary-insensitive, so all three
/// agree with v4's single line-aligned recording.
fn chunkings_ndjson(wire: &[u8]) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let mut lines = Vec::new();
    let mut start = 0;
    for i in 0..wire.len() {
        if wire[i] == b'\n' {
            lines.push(wire[start..i + 1].to_vec());
            start = i + 1;
        }
    }
    if start < wire.len() {
        lines.push(wire[start..].to_vec());
    }
    vec![
        ("whole", vec![wire.to_vec()]),
        ("per-line", lines),
        ("byte", wire.iter().map(|b| vec![*b]).collect()),
    ]
}

fn run_decoder_fixture(decoder: &str) {
    let cases = load_cases(decoder);
    let recorded = load_recorded(decoder);
    assert!(!recorded.is_empty(), "no recorded cases for {decoder}");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut checked = 0;
    let mut chunkings_run = 0usize;
    // P4.D78 — a composition-level witness that reasoning survives the FULL
    // compose path (build → transport → manifest-selected decoder → the pump
    // thread → the chunk channel), not just the decoder in isolation. Counted
    // off the chunks the COMPOSER emitted; asserted non-zero for ollama below.
    let mut reasoning_cases = 0usize;
    for spec in &cases {
        let tag = spec["provider"].as_str().unwrap();
        let case = spec["case"].as_str().unwrap();
        let model = spec["model"].as_str().unwrap_or("model");
        let provider = canonical_provider(tag);
        let oracle = recorded
            .iter()
            .find(|o| o.provider == tag && o.case == case)
            .unwrap_or_else(|| panic!("no recorded oracle for {tag}/{case}"));

        let wire = read_wire(decoder, spec["wire"].as_str().unwrap());
        let chunkings = if decoder == "ollama_ndjson" {
            chunkings_ndjson(&wire)
        } else {
            chunkings_sse(&wire)
        };
        let mut saw_reasoning_here = false;
        chunkings_run = chunkings.len();
        for (chunking, pieces) in chunkings {
            let ((chunks, err), lines) = quilltap_core::test_support::captured_with(|| {
                rt.block_on(drive_composer(provider, model, pieces))
            });
            assert_catch_lines(oracle, chunking, &lines);
            if chunks.iter().any(|c| {
                c.reasoning_content
                    .as_deref()
                    .is_some_and(|r| !r.is_empty())
            }) {
                saw_reasoning_here = true;
            }
            assert_matches(oracle, chunking, &chunks, &err);
        }
        if saw_reasoning_here {
            reasoning_cases += 1;
        }
        checked += 1;
    }
    if decoder == "ollama_ndjson" {
        // The reasoning corpus can only prove the wiring while it is present:
        // a regenerated cases.json that lost the thinking vectors would leave
        // every other assertion here green with the feature untested.
        assert!(
            reasoning_cases >= 4,
            "composer/ollama_ndjson saw reasoning on only {reasoning_cases} case(s) —              the thinking vectors are missing from the corpus, or reasoning stopped              reaching the composer's chunk channel"
        );
    }
    assert_eq!(
        checked,
        recorded.len(),
        "{decoder}: {checked} cases in cases.json but {} recorded",
        recorded.len()
    );
    eprintln!(
        "OK: composer/{decoder} — {checked} case(s) × {chunkings_run} chunking(s) match v4{}.",
        if reasoning_cases > 0 {
            format!(" ({reasoning_cases} carrying reasoning end to end)")
        } else {
            String::new()
        }
    );
}

#[test]
fn composer_chat_completions_sse_matches_v4() {
    run_decoder_fixture("chat_completions_sse");
}

#[test]
fn composer_responses_api_sse_matches_v4() {
    run_decoder_fixture("responses_api_sse");
}

#[test]
fn composer_anthropic_sse_matches_v4() {
    run_decoder_fixture("anthropic_sse");
}

#[test]
fn composer_google_parts_matches_v4() {
    run_decoder_fixture("google_parts");
}

#[test]
fn composer_ollama_ndjson_matches_v4() {
    run_decoder_fixture("ollama_ndjson");
}
