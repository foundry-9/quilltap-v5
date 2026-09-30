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

use quilltap_core::llm_fallback::{classify_fallback_trigger, FallbackError};
use quilltap_core::model::decoders::{
    AnthropicSseDecoder, ChatCompletionsFlavor, ChatCompletionsSseDecoder, DecodeError,
    GooglePartsDecoder, OllamaNdjsonDecoder, ResponsesApiSseDecoder, StreamDecoder,
};
use quilltap_core::model::stream::StreamChunk;
use quilltap_core::model::streaming_provider::decode_stream_error;
use quilltap_core::services::dangerous_content::refusal::{
    classify_refusal, code_string, RefusalError, RefusalInput,
};
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
    /// P4.122: on a row whose generator THREW, the fields v4's classifier
    /// duck-types off the thrown value (`record-stream-fixtures.mjs`
    /// `thrownFields`), and v4's two verdicts over it. `None` on every row
    /// that did not throw.
    thrown: Option<Value>,
    refusal: Option<Value>,
    trigger: Option<String>,
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
                refusal: v.get("refusal").cloned(),
                trigger: v.get("trigger").and_then(Value::as_str).map(str::to_string),
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
            assert_thrown_matches(oracle, chunking, got);
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

/// P4.122 — the thrown value's STRUCTURE, not just its message: the refusal
/// side the decoder attached (the fields v4's classifier reads off the value
/// the SDK threw), and v4's two verdicts over it against v5's classifier over
/// the `StreamError` the production pump builds from this `DecodeError`
/// (`decode_stream_error`) — handed to the fallback engine exactly as the
/// Salon hands it (`FallbackError::from_stream_error`).
fn assert_thrown_matches(oracle: &OracleCase, chunking: &str, got: &DecodeError) {
    let label = format!("{}/{} [{}]", oracle.provider, oracle.case, chunking);
    let thrown = oracle
        .thrown
        .as_ref()
        .unwrap_or_else(|| panic!("{label}: an erroring row must record `thrown`"));
    let want_code = thrown.get("code").and_then(code_string);
    let want_nested = thrown.get("errorCode").and_then(code_string);
    let want_status = thrown
        .get("status")
        .and_then(Value::as_u64)
        .map(|s| s as u16);
    let want_reason = thrown
        .get("providerReason")
        .and_then(Value::as_str)
        .map(str::to_string);
    match got.refusal.as_deref() {
        Some(side) => {
            assert_eq!(
                side.message,
                thrown["message"].as_str().unwrap(),
                "{label}: side message"
            );
            assert_eq!(side.code, want_code, "{label}: side code");
            assert_eq!(side.nested_code, want_nested, "{label}: side nested code");
            assert_eq!(
                side.name.as_deref(),
                thrown["name"].as_str(),
                "{label}: side name"
            );
            assert_eq!(side.status, want_status, "{label}: side status");
            assert_eq!(
                side.provider_reason, want_reason,
                "{label}: side providerReason"
            );
        }
        // No side: v4's thrown value must carry nothing the classifier ranks
        // on beyond its message (the Anthropic `error` event, whose whole
        // event is the message and which has no `code`).
        None => assert!(
            want_code.is_none()
                && want_nested.is_none()
                && want_status.is_none()
                && want_reason.is_none(),
            "{label}: v4 threw a structured error ({thrown}) but v5 attached no side"
        ),
    }
    let stream_err = decode_stream_error(got.clone());
    let side_or_message = stream_err
        .refusal
        .as_deref()
        .cloned()
        .unwrap_or_else(|| RefusalError::message_only(stream_err.message.clone()));
    let verdict = classify_refusal(RefusalInput {
        error: Some(&side_or_message),
        ..Default::default()
    });
    let want = oracle
        .refusal
        .as_ref()
        .expect("an erroring row records `refusal`");
    assert_eq!(
        verdict.refused,
        want["refused"].as_bool().unwrap(),
        "{label}: refused"
    );
    assert_eq!(
        verdict.evidence.map(|e| e.as_str().to_string()).as_deref(),
        want.get("evidence").and_then(Value::as_str),
        "{label}: evidence"
    );
    assert_eq!(
        verdict.detail.as_deref(),
        want.get("detail").and_then(Value::as_str),
        "{label}: detail"
    );
    let trigger = classify_fallback_trigger(FallbackError::from_stream_error(&stream_err))
        .map(|t| t.as_str().to_string());
    assert_eq!(trigger, oracle.trigger, "{label}: fallback trigger");
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
            // P4.D225: Grok's raw `content` is its own `extractTextFromResponse`.
            "responses_api_sse" if spec["provider"].as_str() == Some("grok") => {
                drive(ResponsesApiSseDecoder::grok(), &pieces)
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
    // P4.D105 — the NanoGPT streaming cache-usage coverage shape, read off v4's
    // RECORDED final chunk (never off v5's decode), so a corpus that lost a
    // vector cannot leave the feature untested and still pass green.
    let (
        mut nanogpt_cache_anthropic,
        mut nanogpt_cache_write_only,
        mut nanogpt_cache_openai_dialect,
        mut nanogpt_cache_excluded,
        mut nanogpt_rpu_null,
        mut nanogpt_rpu_object,
    ) = (false, false, false, false, false, false);
    for spec in &cases {
        let provider = spec["provider"].as_str().unwrap();
        let case = spec["case"].as_str().unwrap();
        let oracle = recorded
            .iter()
            .find(|o| o.provider == provider && o.case == case)
            .unwrap_or_else(|| panic!("no recorded oracle for {provider}/{case}"));
        // An erroring row has no final chunk to read the cache shape off.
        if provider == "nanogpt" && oracle.error.is_none() {
            let final_chunk = oracle.chunks.last().expect("a final chunk");
            let cache = final_chunk.get("cacheUsage");
            let read = cache
                .and_then(|c| c.get("cacheReadInputTokens"))
                .and_then(Value::as_i64);
            let written = cache
                .and_then(|c| c.get("cacheCreationInputTokens"))
                .and_then(Value::as_i64);
            match (read, written) {
                (Some(_), Some(_)) => nanogpt_cache_anthropic = true,
                (None, Some(_)) => nanogpt_cache_write_only = true,
                (Some(_), None) => nanogpt_cache_openai_dialect = true,
                (None, None) => {}
            }
            // v4 `rawProviderUsage: (usage ?? null)` — the KEY is always
            // present on NanoGPT's final chunk; both arms must be exercised.
            match final_chunk.get("rawProviderUsage") {
                Some(Value::Null) => nanogpt_rpu_null = true,
                Some(Value::Object(u)) => {
                    nanogpt_rpu_object = true;
                    // The house rule, against v4's own bytes: the recorded
                    // promptTokens is the RAW prompt_tokens minus the read.
                    let wire_prompt = u.get("prompt_tokens").and_then(Value::as_i64).unwrap_or(0);
                    let recorded_prompt =
                        final_chunk["usage"]["promptTokens"].as_i64().unwrap_or(0);
                    assert_eq!(
                        recorded_prompt,
                        (wire_prompt - read.unwrap_or(0)).max(0),
                        "nanogpt/{case}: v4's recorded promptTokens is not \
                         max(0, prompt_tokens - cacheRead) — the house rule moved"
                    );
                    if read.unwrap_or(0) > 0 && recorded_prompt < wire_prompt {
                        nanogpt_cache_excluded = true;
                    }
                }
                other => panic!(
                    "nanogpt/{case}: v4 recorded rawProviderUsage={other:?} — the field is \
                     supposed to be present on every final chunk, as the usage object or null"
                ),
            }
        }
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
    if decoder == "chat_completions_sse" {
        assert!(
            nanogpt_cache_anthropic,
            "corpus lost the nanogpt Anthropic-dialect stream case (read AND write)"
        );
        assert!(
            nanogpt_cache_write_only,
            "corpus lost the nanogpt write-only stream case"
        );
        assert!(
            nanogpt_cache_openai_dialect,
            "corpus lost the nanogpt OpenAI-dialect (cached_tokens) stream case"
        );
        assert!(
            nanogpt_cache_excluded,
            "corpus lost the nanogpt stream case where the cache read actually \
             REDUCES promptTokens — the house rule is untested"
        );
        assert!(
            nanogpt_rpu_null,
            "corpus lost the nanogpt no-usage-frame case — nothing pins \
             `rawProviderUsage: null` against an absent key"
        );
        assert!(
            nanogpt_rpu_object,
            "corpus lost every nanogpt case carrying a usage frame"
        );
    }
    // P4.122 — the mid-stream error-frame coverage, read off v4's RECORDED
    // rows. The SDK flavours throw on every coded/uncoded frame (4 providers
    // × before/after/flat-event/uncoded, + the EOF-flush frame + Z.AI's
    // numeric 1301 = 18); OpenRouter's raw path throws on NONE of its five
    // (the divergence-by-design, pinned from v4's side); the Responses
    // plugins throw on `event: error` (before/after) and an unnamed
    // `data.error` frame, and NOT on `response.failed` (the convergence).
    let recorded_throws = |pred: &dyn Fn(&OracleCase) -> bool| {
        recorded
            .iter()
            .filter(|o| pred(o))
            .map(|o| o.error.is_some())
            .collect::<Vec<_>>()
    };
    if decoder == "chat_completions_sse" {
        let sdk = recorded_throws(&|o| {
            o.provider != "openrouter"
                && o.case.contains("midstream")
                && !o.case.ends_with("error-null")
        });
        assert_eq!(sdk.len(), 18, "the SDK-flavour error-frame rows moved");
        assert!(
            sdk.iter().all(|t| *t),
            "an SDK error-frame row stopped throwing on v4"
        );
        let or = recorded_throws(&|o| o.provider == "openrouter" && o.case.contains("midstream"));
        assert_eq!(
            or.len(),
            5,
            "the OpenRouter raw-path error-frame rows moved"
        );
        assert!(
            or.iter().all(|t| !*t),
            "v4's OpenRouter raw path now throws on an error frame"
        );
        let null = recorded_throws(&|o| o.case.ends_with("midstream-error-null"));
        assert_eq!(null.len(), 5);
        assert!(
            null.iter().all(|t| !*t),
            "`{{\"error\":null}}` now throws on v4"
        );
    }
    if decoder == "responses_api_sse" {
        let throws = recorded_throws(&|o| {
            o.case.contains("responses-event-error") || o.case.contains("responses-data-error")
        });
        assert_eq!(throws.len(), 6);
        assert!(
            throws.iter().all(|t| *t),
            "a Responses error frame stopped throwing on v4"
        );
        let failed = recorded_throws(&|o| o.case.ends_with("responses-failed"));
        assert_eq!(failed.len(), 2);
        assert!(
            failed.iter().all(|t| !*t),
            "v4 now throws on `response.failed` — the convergence moved"
        );
    }
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
