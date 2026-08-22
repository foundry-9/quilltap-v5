//! Differential: the five sans-IO stream decoders (wave 4 / W4.7b).
//!
//! For every committed wire transcript, this replays the bytes through the Rust
//! `model::decoders` decoder at THREE chunkings — whole-buffer, per-SSE-frame,
//! and byte-at-a-time — and diffs the normalised `StreamChunk` sequence (+ the
//! terminal `raw_response`) against the NDJSON RECORDED by driving v4's REAL
//! plugin `streamMessage` parser over the same transcript (the fetch-mock
//! recorder in `harness/oracle/providers/record-stream-fixtures.mjs`). Both the
//! transcripts and the recordings are committed; regenerate with
//! `regenerate-stream-fixtures.sh` after a v4 provider drift.
//!
//! The comparison serialises each Rust `StreamChunk` to the exact v4
//! `StreamChunk` JSON shape (camelCase keys, omitted-when-absent, matching what
//! v4's generator yields) and diffs as parsed `serde_json::Value` so object key
//! order is irrelevant. v4 carries a `toolCalls` field on the terminal chunk that
//! the Rust `StreamChunk` deliberately omits (the streaming service reads tool
//! calls only off `rawResponse`, never off the chunk) — that field is excluded
//! from both sides (see `strip_ignored`).
//!
//! One documented, faithful normalisation:
//! - **google `sdkHttpResponse`**: the `@google/genai` SDK injects a transport
//!   `sdkHttpResponse: { headers }` wrapper into the parsed response object; a
//!   sans-IO decoder never sees HTTP headers (the host transport does), so this
//!   artifact is stripped from `rawResponse` on both sides.
//!
//! (v4 bug 35 rewrote the Ollama splitter to buffer across reads + carry an
//! incomplete UTF-8 tail, so ollama is no longer push-boundary-sensitive: it
//! now joins the full three-chunking equivalence — whole, per-line, AND
//! byte-at-a-time — like every other decoder. The old whole-+-per-line-only
//! exclusion for the ported no-buffer bug is gone.)
//!
//! Run:
//!   cargo test -p quilltap-harness --test stream_decoders_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::model::decoders::{
    AnthropicSseDecoder, ChatCompletionsFlavor, ChatCompletionsSseDecoder, DecodeError,
    GooglePartsDecoder, OllamaNdjsonDecoder, ResponsesApiSseDecoder, StreamDecoder,
};
use quilltap_core::model::stream::StreamChunk;
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

/// Load the `cases.json` for a decoder as a map of case → spec Value.
fn load_cases(decoder: &str) -> Vec<Value> {
    let path = streams_dir().join(decoder).join("cases.json");
    let text = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn read_wire(decoder: &str, wire: &str) -> Vec<u8> {
    std::fs::read(streams_dir().join(decoder).join(wire)).unwrap()
}

/// Serialise a Rust `StreamChunk` to the exact v4 `StreamChunk` JSON shape
/// (camelCase, omitted-when-absent) so it can be diffed against the recording.
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
        // v4 emits only the fields a given provider populates; serialise the
        // present (Some) subset, camelCased.
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

/// Strip fields that are not part of the decoder's semantic output:
/// - `toolCalls` on a chunk (v4 carries it; the consumer ignores it, reading
///   tool calls off `rawResponse` — the Rust `StreamChunk` omits it by design).
/// - `sdkHttpResponse` inside `rawResponse` (a `@google/genai` transport wrapper
///   a sans-IO decoder never sees).
fn strip_ignored(mut v: Value) -> Value {
    if let Value::Object(ref mut m) = v {
        m.remove("toolCalls");
        if let Some(Value::Object(raw)) = m.get_mut("rawResponse") {
            raw.remove("sdkHttpResponse");
        }
    }
    v
}

/// Drive a decoder over `wire` at a given chunk size (0 = whole buffer,
/// usize::MAX handled by caller as per-frame). Returns (chunks, error?).
fn drive<D: StreamDecoder>(
    mut d: D,
    pieces: &[Vec<u8>],
) -> (Vec<StreamChunk>, Option<DecodeError>) {
    let mut out = Vec::new();
    let mut err = None;
    for p in pieces {
        match d.push(p) {
            Ok(cs) => out.extend(cs),
            Err(e) => {
                if err.is_none() {
                    err = Some(e);
                }
            }
        }
    }
    match d.finish() {
        Ok(cs) => out.extend(cs),
        Err(e) => {
            if err.is_none() {
                err = Some(e);
            }
        }
    }
    (out, err)
}

/// The three chunkings for an SSE/text wire: whole, per-SSE-frame (split on the
/// blank-line `\n\n` boundary, keeping it), byte-at-a-time.
fn chunkings_sse(wire: &[u8]) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let whole = vec![wire.to_vec()];
    // per-frame: split after each "\n\n".
    let mut frames = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + 1 < wire.len() {
        if wire[i] == b'\n' && wire[i + 1] == b'\n' {
            frames.push(wire[start..i + 2].to_vec());
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < wire.len() {
        frames.push(wire[start..].to_vec());
    }
    let bytes: Vec<Vec<u8>> = wire.iter().map(|b| vec![*b]).collect();
    vec![("whole", whole), ("per-frame", frames), ("byte", bytes)]
}

/// Chunkings for ollama NDJSON: whole + per-line + byte-at-a-time. v4 bug 35's
/// buffered splitter (line carry + incomplete-UTF-8 carry) makes ollama
/// push-boundary-insensitive, so the byte chunking now agrees with v4's single
/// line-aligned recording — no exclusion.
fn chunkings_ndjson(wire: &[u8]) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let whole = vec![wire.to_vec()];
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
    let bytes: Vec<Vec<u8>> = wire.iter().map(|b| vec![*b]).collect();
    vec![("whole", whole), ("per-line", lines), ("byte", bytes)]
}

/// Compare the driven (chunks, error) against the oracle case, for one chunking.
fn assert_matches(
    oracle: &OracleCase,
    chunking: &str,
    chunks: &[StreamChunk],
    err: &Option<DecodeError>,
    ignore_last_raw_sdk: bool,
) {
    let _ = ignore_last_raw_sdk;
    // Error parity.
    match (&oracle.error, err) {
        (Some(want), Some(got)) => {
            assert_eq!(
                &got.message, want,
                "{}/{}: error message mismatch",
                oracle.provider, oracle.case
            );
        }
        (None, None) => {}
        (Some(_), None) => panic!(
            "{}/{} [{}]: oracle errored but Rust did not",
            oracle.provider, oracle.case, chunking
        ),
        (None, Some(e)) => panic!(
            "{}/{} [{}]: Rust errored ({}) but oracle did not",
            oracle.provider, oracle.case, chunking, e.message
        ),
    }
    // Chunk sequence parity.
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

/// Build the right decoder for a case and run all chunkings against the oracle.
fn run_decoder_case(decoder: &str, spec: &Value, oracle: &OracleCase) {
    let wire = read_wire(decoder, spec["wire"].as_str().unwrap());
    let is_ndjson = decoder == "ollama_ndjson";
    let chunkings = if is_ndjson {
        chunkings_ndjson(&wire)
    } else {
        chunkings_sse(&wire)
    };

    for (chunking, pieces) in chunkings {
        let (chunks, err) = match decoder {
            "chat_completions_sse" => {
                let flavor = match spec["flavor"].as_str().unwrap() {
                    "DeepSeek" => ChatCompletionsFlavor::DeepSeek,
                    "ZAi" => ChatCompletionsFlavor::ZAi,
                    "OpenRouterRaw" => ChatCompletionsFlavor::OpenRouterRaw,
                    "OpenAiCompatible" => ChatCompletionsFlavor::OpenAiCompatible,
                    "NanoGpt" => ChatCompletionsFlavor::NanoGpt,
                    other => panic!("unknown flavor {other}"),
                };
                drive(ChatCompletionsSseDecoder::new(flavor), &pieces)
            }
            "responses_api_sse" => drive(ResponsesApiSseDecoder::new(), &pieces),
            "anthropic_sse" => drive(AnthropicSseDecoder::new(), &pieces),
            "google_parts" => {
                let thinking = spec["isThinkingModel"].as_bool().unwrap_or(false);
                drive(GooglePartsDecoder::new(thinking), &pieces)
            }
            "ollama_ndjson" => {
                let model = spec["model"].as_str().unwrap_or("model");
                drive(OllamaNdjsonDecoder::new(model), &pieces)
            }
            other => panic!("unknown decoder {other}"),
        };
        assert_matches(oracle, chunking, &chunks, &err, decoder == "google_parts");
    }
}

fn run_decoder(decoder: &str) {
    let cases = load_cases(decoder);
    let recorded = load_recorded(decoder);
    assert!(!recorded.is_empty(), "no recorded cases for {decoder}");
    let mut checked = 0;
    for spec in &cases {
        let provider = spec["provider"].as_str().unwrap();
        let case = spec["case"].as_str().unwrap();
        let oracle = recorded
            .iter()
            .find(|o| o.provider == provider && o.case == case)
            .unwrap_or_else(|| panic!("no recorded oracle for {provider}/{case}"));
        run_decoder_case(decoder, spec, oracle);
        checked += 1;
    }
    assert_eq!(
        checked,
        recorded.len(),
        "{decoder}: {} cases in cases.json but {} recorded",
        checked,
        recorded.len()
    );
    eprintln!("OK: {decoder} — {checked} case(s) × 2–3 chunkings match v4.");
}

#[test]
fn chat_completions_sse_matches_v4() {
    run_decoder("chat_completions_sse");
}

#[test]
fn responses_api_sse_matches_v4() {
    run_decoder("responses_api_sse");
}

#[test]
fn anthropic_sse_matches_v4() {
    run_decoder("anthropic_sse");
}

#[test]
fn google_parts_matches_v4() {
    run_decoder("google_parts");
}

#[test]
fn ollama_ndjson_matches_v4() {
    run_decoder("ollama_ndjson");
}
