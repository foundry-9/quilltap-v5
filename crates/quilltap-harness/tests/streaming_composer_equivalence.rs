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
//! Ollama respects the W4.7b push-boundary caveat: v4's plugin splits each
//! network read on `\n` with NO cross-read buffer (a ported bug), so ollama is
//! replayed at whole-buffer + line-aligned chunkings only (the byte-at-a-time
//! lossy bug-parity is asserted in the decoder's own unit tests).
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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quilltap_core::model::stream::{StreamChunk, StreamParams, StreamingCompletionProvider};
use quilltap_core::model::streaming_provider::WireStreamingProvider;
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::model::CompletionMessage;
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
        messages: vec![CompletionMessage::user("composer replay")],
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
    }
}

/// Drive the composer over `pieces` and split the drained items into
/// (chunks, first error).
async fn drive_composer(
    provider: &str,
    model: &str,
    pieces: Vec<Vec<u8>>,
) -> (Vec<StreamChunk>, Option<String>) {
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
    let mut error: Option<String> = None;
    while let Some(item) = rx.recv().await {
        match item {
            Ok(c) => chunks.push(c),
            Err(e) => {
                error = Some(e.message);
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
    err: &Option<String>,
) {
    match (&oracle.error, err) {
        (Some(want), Some(got)) => assert_eq!(
            got, want,
            "{}/{} [{}]: error message mismatch",
            oracle.provider, oracle.case, chunking
        ),
        (None, None) => {}
        (Some(_), None) => panic!(
            "{}/{} [{}]: oracle errored but the composer did not",
            oracle.provider, oracle.case, chunking
        ),
        (None, Some(e)) => panic!(
            "{}/{} [{}]: the composer errored ({e}) but the oracle did not",
            oracle.provider, oracle.case, chunking
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

/// Whole-buffer + byte-at-a-time (SSE wires).
fn chunkings_sse(wire: &[u8]) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    vec![
        ("whole", vec![wire.to_vec()]),
        ("byte", wire.iter().map(|b| vec![*b]).collect()),
    ]
}

/// Whole-buffer + line-aligned (ollama NDJSON — the ported no-buffer bug makes
/// byte-at-a-time diverge from v4 BY DESIGN).
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
    vec![("whole", vec![wire.to_vec()]), ("per-line", lines)]
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
        for (chunking, pieces) in chunkings {
            let (chunks, err) = rt.block_on(drive_composer(provider, model, pieces));
            assert_matches(oracle, chunking, &chunks, &err);
        }
        checked += 1;
    }
    assert_eq!(
        checked,
        recorded.len(),
        "{decoder}: {checked} cases in cases.json but {} recorded",
        recorded.len()
    );
    eprintln!("OK: composer/{decoder} — {checked} case(s) × 2 chunkings match v4.");
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
