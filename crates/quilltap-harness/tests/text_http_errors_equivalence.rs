//! Tier-1 wire differential: the TEXT-side structured refusal (P4.118 — the
//! gap P4.D225's header and the `acadcc7cd` unification named: a text
//! provider's HTTP error `code` never reached v4's `provider-code` evidence on
//! v5, because no production text path attached a `RefusalError`).
//!
//! The oracle (`harness/oracle/providers/record-text-errors.mjs`) drives the
//! TEN real v4 text plugins' `streamMessage` and `sendMessage` at the pin with
//! only `global.fetch` mocked to a posed non-2xx (`fixtures/text-http-errors/
//! cases.json`), and records the value each plugin THREW — the fields v4's
//! classifier reads off it — plus v4's own `classifyRefusal({error})` and
//! `classifyFallbackTrigger(error)` over it.
//!
//! This test poses the SAME status + body as the `TransportError` v5's one
//! HTTP boundary produces (`HTTP {status}: {body}`) under the REAL v5
//! compositions — `WireStreamingProvider::stream_message` and
//! `execute_completion` — and diffs:
//! 1. the refusal SIDE the production reconstruction attached
//!    (`StreamError::refusal` / `CompletionError::refusal`) against v4's thrown
//!    fields (`message`, `code`, `error.code`, `name`, `status`);
//! 2. `classify_refusal` over that side against v4's verdict (refused,
//!    evidence, detail — the trail's bytes);
//! 3. `classify_fallback_trigger` over the error as the Salon
//!    (`FallbackError::from_stream_error`) and the cheap path (the message plus
//!    the side, `cheap_llm_exec.rs`) hand it over, against v4's trigger;
//! 4. that the v5 message bytes are UNCHANGED — `HTTP {status}: {body}` on
//!    every row (§S.5: only the side carries v4's rendering).
//!
//! Row mapping: every provider's `stream`/`send` ↔ v5's streaming / completion
//! path, except OPENROUTER, whose v5 paths are always the raw Chat Completions
//! `fetch` wire — v4's `stream_tools` and `send_vision` modes. v4's OpenRouter
//! `stream`/`send` rows run `@openrouter/sdk`, which v5 never runs (the
//! standing deliberate divergence `streaming_provider.rs` records): those rows
//! are diffed against v5 too, and every difference is pinned BOTH WAYS in
//! [`EXPECTED_DIVERGENCES`] — as are the trigger differences that are v5's
//! unchanged message bytes, not the side (the ladder after its first step
//! reads the message, by mandate).
//!
//! RED-FIRST (measured on the unwired tree, the side `None` at every site):
//! see the lane record — every `provider-code` row answered `refused: false`.
//!
//! Regenerate the oracle (Node 24, from a PINNED v4 worktree — ledger §5.1;
//! the corpus is committed, so this is only needed when the SDKs or plugins
//! move):
//!   PIN=/tmp/qt-v4-pin-<order>-<sha>   # built per ledger §5.1
//!   V4="$PIN" bash harness/oracle/providers/regenerate-text-errors.sh
//! Run (reads the COMMITTED corpus; no env var needed, none honoured):
//!   cargo test -p quilltap-harness --test text_http_errors_equivalence -- --nocapture

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use quilltap_core::llm_fallback::{classify_fallback_trigger, FallbackError};
use quilltap_core::model::completion::{
    CompletionAttachment, CompletionError, CompletionMessage, CompletionParams,
};
use quilltap_core::model::completion_provider::execute_completion;
use quilltap_core::model::stream::{
    StreamError, StreamMessage, StreamParams, StreamingCompletionProvider,
};
use quilltap_core::model::streaming_provider::{SingleKey, WireStreamingProvider};
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::services::dangerous_content::refusal::{
    classify_refusal, code_string, RefusalError, RefusalInput,
};
use serde::Deserialize;
use serde_json::Value;

const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../harness/oracle/fixtures/text-http-errors/text-http-errors.recorded.ndjson"
);

#[derive(Deserialize)]
struct Thrown {
    message: String,
    name: Option<String>,
    code: Option<Value>,
    #[serde(rename = "errorCode")]
    error_code: Option<Value>,
    #[serde(rename = "providerReason")]
    provider_reason: Option<String>,
    status: Option<u16>,
}

#[derive(Deserialize)]
struct Verdict {
    refused: bool,
    evidence: Option<String>,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct Row {
    case: String,
    provider: String,
    mode: String,
    status: u16,
    body: String,
    #[serde(rename = "fetchCalls")]
    fetch_calls: usize,
    outcome: String,
    thrown: Option<Thrown>,
    refusal: Verdict,
    trigger: Option<String>,
}

/// Every (case, provider, v4 mode, field) where v5 differs from the recorded
/// v4 row, BY DESIGN. Pinned both ways: a divergence that disappears is as red
/// as a new one. Two classes:
/// - `openrouter-sdk`: v4's `@openrouter/sdk` path (modes `stream` / `send`),
///   which v5 never runs.
/// - `message-bytes`: v5's `trigger`, read by the fallback ladder's later
///   rules over v5's UNCHANGED `HTTP {status}: {body}` message where v4 reads
///   its own rendering (the order's §S.5; the OpenRouter 403 shape is the
///   order's Tier-3 item 10, awaiting a ruling).
///
/// Each entry is `(provider, v4 mode, field, cases)`; [`EVERY_CASE`] means all
/// of the corpus's cases diverge on that field (and each must).
///
/// Read by class: OPENROUTER `stream`/`send` are the SDK path — its own error
/// classes (`ResponseValidationError`, `ForbiddenResponseError`,
/// `OpenRouterDefaultError`), messages without status digits, `status` set,
/// and `error.code` = the numeric code the body carries (so a numeric `1301`
/// IS a provider-code on v4's SDK path, and a `content_policy_violation` body
/// is NOT a message-pattern hit there); every trigger row is that same message
/// against v5's `HTTP {status}:` bytes. GOOGLE's triggers are v4's
/// `JSON.stringify(body)` message — no status digits — meeting v5's
/// `HTTP {status}:` rule; `google_json_as_text_plain` is the content-type
/// branch the transport cannot see (the module doc of `model::provider_error`).
const EXPECTED_DIVERGENCES: &[(&str, &str, &str, &[&str])] = &[
    ("GOOGLE", "send", "message", &["google_json_as_text_plain"]),
    (
        "GOOGLE",
        "send",
        "trigger",
        &[
            "anthropic_typed_body",
            "benign_null_code",
            "code_is_bool",
            "code_is_number_zero",
            "error_empty_message",
            "error_is_false",
            "error_is_string",
            "error_null_flat",
            "error_without_message",
            "flat_content_filter",
            "float_in_body",
            "invalid_api_key_401",
            "invalid_prompt",
            "json_array",
            "json_null",
            "json_number",
            "json_string",
            "message_not_a_string",
            "mixed_case_code",
            "oai_content_filter",
            "ollama_error_string",
            "zai_1301_long_message",
            "zai_1301_numeric",
            "zai_1301_string",
        ],
    ),
    (
        "GOOGLE",
        "stream",
        "message",
        &["google_json_as_text_plain"],
    ),
    (
        "GOOGLE",
        "stream",
        "trigger",
        &[
            "anthropic_typed_body",
            "benign_null_code",
            "code_is_bool",
            "code_is_number_zero",
            "error_empty_message",
            "error_is_false",
            "error_is_string",
            "error_null_flat",
            "error_without_message",
            "flat_content_filter",
            "float_in_body",
            "invalid_api_key_401",
            "invalid_prompt",
            "json_array",
            "json_null",
            "json_number",
            "json_string",
            "message_not_a_string",
            "mixed_case_code",
            "oai_content_filter",
            "ollama_error_string",
            "zai_1301_long_message",
            "zai_1301_numeric",
            "zai_1301_string",
        ],
    ),
    ("OPENROUTER", "send", "message", EVERY_CASE),
    ("OPENROUTER", "send", "name", EVERY_CASE),
    ("OPENROUTER", "send", "status", EVERY_CASE),
    (
        "OPENROUTER",
        "send",
        "nested_code",
        &[
            "code_is_number_zero",
            "google_invalid_argument",
            "openrouter_moderation_403",
            "zai_1301_numeric",
        ],
    ),
    (
        "OPENROUTER",
        "send",
        "verdict",
        &["content_policy_violation", "zai_1301_numeric"],
    ),
    (
        "OPENROUTER",
        "send",
        "trigger",
        &[
            "anthropic_top_level_code",
            "anthropic_typed_body",
            "azure_content_filter",
            "benign_null_code",
            "code_is_bool",
            "code_is_number_zero",
            "content_policy_violation",
            "error_empty_message",
            "error_is_false",
            "error_is_string",
            "error_null_flat",
            "error_without_message",
            "flat_content_filter",
            "float_in_body",
            "google_invalid_argument",
            "invalid_api_key_401",
            "invalid_prompt",
            "json_array",
            "json_null",
            "json_number",
            "json_string",
            "message_not_a_string",
            "mixed_case_code",
            "model_not_found_404",
            "oai_content_filter",
            "ollama_error_string",
            "openrouter_moderation_403",
            "zai_1301_long_message",
            "zai_1301_numeric",
            "zai_1301_string",
        ],
    ),
    ("OPENROUTER", "stream", "message", EVERY_CASE),
    ("OPENROUTER", "stream", "name", EVERY_CASE),
    ("OPENROUTER", "stream", "status", EVERY_CASE),
    (
        "OPENROUTER",
        "stream",
        "nested_code",
        &[
            "code_is_number_zero",
            "google_invalid_argument",
            "openrouter_moderation_403",
            "zai_1301_numeric",
        ],
    ),
    (
        "OPENROUTER",
        "stream",
        "verdict",
        &["content_policy_violation", "zai_1301_numeric"],
    ),
    (
        "OPENROUTER",
        "stream",
        "trigger",
        &[
            "anthropic_top_level_code",
            "anthropic_typed_body",
            "azure_content_filter",
            "benign_null_code",
            "code_is_bool",
            "code_is_number_zero",
            "content_policy_violation",
            "error_empty_message",
            "error_is_false",
            "error_is_string",
            "error_null_flat",
            "error_without_message",
            "flat_content_filter",
            "float_in_body",
            "google_invalid_argument",
            "invalid_api_key_401",
            "invalid_prompt",
            "json_array",
            "json_null",
            "json_number",
            "json_string",
            "message_not_a_string",
            "mixed_case_code",
            "model_not_found_404",
            "oai_content_filter",
            "ollama_error_string",
            "openrouter_moderation_403",
            "zai_1301_long_message",
            "zai_1301_numeric",
            "zai_1301_string",
        ],
    ),
];
/// The [`EXPECTED_DIVERGENCES`] marker for "every case of the corpus".
const EVERY_CASE: &[&str] = &["*"];

fn expected(case: &str, provider: &str, mode: &str, field: &str) -> bool {
    EXPECTED_DIVERGENCES.iter().any(|(p, m, f, cases)| {
        *p == provider
            && *m == mode
            && *f == field
            && (*cases == EVERY_CASE || cases.contains(&case))
    })
}

/// A transport that fails every call with the posed non-2xx, rendered exactly
/// as `ReqwestTransport` renders it, and counts the calls.
struct PosedFailure {
    status: u16,
    body: String,
    calls: AtomicUsize,
}

impl PosedFailure {
    fn new(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            calls: AtomicUsize::new(0),
        }
    }

    fn fail(&self) -> TransportError {
        self.calls.fetch_add(1, Ordering::SeqCst);
        TransportError {
            message: format!("HTTP {}: {}", self.status, self.body),
            status: Some(self.status),
        }
    }
}

impl ProviderTransport for PosedFailure {
    fn execute<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        let e = self.fail();
        Box::pin(async move { Err(e) })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        let e = self.fail();
        Box::pin(async move { Err(e) })
    }
}

fn model_for(provider: &str) -> &'static str {
    if provider == "GOOGLE" {
        "gemini-2.5-flash"
    } else {
        "test-model"
    }
}

fn stream_params(provider: &str) -> StreamParams {
    StreamParams {
        messages: vec![StreamMessage::user("hi")],
        model: model_for(provider).into(),
        temperature: None,
        max_tokens: Some(64),
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

// The recorder's 1×1 PNG (the OpenRouter `send_vision` mode's attachment).
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn completion_params(provider: &str, vision: bool) -> CompletionParams {
    CompletionParams {
        messages: vec![CompletionMessage::user(if vision {
            "what is this"
        } else {
            "hi"
        })],
        model: model_for(provider).into(),
        temperature: None,
        max_tokens: Some(64),
        strict_max_tokens: false,
        top_p: None,
        cache_key: None,
        profile_parameters: None,
        attachments: if vision {
            vec![CompletionAttachment {
                id: "a1".into(),
                filename: "x.png".into(),
                mime_type: "image/png".into(),
                data: PNG_1X1.into(),
            }]
        } else {
            Vec::new()
        },
        request_timeout_ms: None,
    }
}

/// What v5 produced for one posed failure.
struct V5Outcome {
    message: String,
    side: Option<RefusalError>,
    trigger: Option<String>,
    calls: usize,
}

fn run_stream(rt: &tokio::runtime::Runtime, row: &Row) -> V5Outcome {
    let provider = WireStreamingProvider::new(
        PosedFailure::new(row.status, &row.body),
        SingleKey(String::new()),
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );
    let items = rt.block_on(async {
        let mut rx = provider
            .stream_message(&row.provider, None, &stream_params(&row.provider))
            .await;
        let mut out = Vec::new();
        while let Some(item) = rx.recv().await {
            out.push(item);
        }
        out
    });
    assert_eq!(
        items.len(),
        1,
        "{}: a pre-stream failure is one item",
        label(row)
    );
    let err: StreamError = items
        .into_iter()
        .next()
        .unwrap()
        .expect_err("a posed non-2xx is an error");
    // The Salon's hand-over (`primary_stream.rs`).
    let trigger = classify_fallback_trigger(FallbackError::from_stream_error(&err))
        .map(|t| t.as_str().to_string());
    V5Outcome {
        message: err.message.clone(),
        side: err.refusal.as_deref().cloned(),
        trigger,
        calls: provider.transport_ref().calls.load(Ordering::SeqCst),
    }
}

fn run_send(rt: &tokio::runtime::Runtime, row: &Row, vision: bool) -> V5Outcome {
    let transport = PosedFailure::new(row.status, &row.body);
    let params = completion_params(&row.provider, vision);
    let result = rt.block_on(execute_completion(
        &transport,
        &row.provider,
        None,
        "",
        &params,
        &TransportPolicy::default(),
        "Quilltap/test",
        None,
        None,
    ));
    let err: CompletionError = match result {
        Ok(_) => panic!("{}: a posed non-2xx must fail", label(row)),
        Err(e) => e,
    };
    // The cheap path's hand-over (`cheap_llm_exec.rs`): the message, plus the
    // side when the completion carried one.
    let fe = FallbackError::message(&err.message);
    let fe = match err.refusal.as_deref() {
        Some(r) => fe.with_refusal(r),
        None => fe,
    };
    let trigger = classify_fallback_trigger(fe).map(|t| t.as_str().to_string());
    V5Outcome {
        message: err.message.clone(),
        side: err.refusal.as_deref().cloned(),
        trigger,
        calls: transport.calls.load(Ordering::SeqCst),
    }
}

fn label(row: &Row) -> String {
    format!("{}/{}/{}", row.case, row.provider, row.mode)
}

/// Run v5 for a recorded v4 row (see the module doc's row mapping).
fn run_v5(rt: &tokio::runtime::Runtime, row: &Row) -> V5Outcome {
    match row.mode.as_str() {
        "stream" | "stream_tools" => run_stream(rt, row),
        "send" => run_send(rt, row, false),
        "send_vision" => run_send(rt, row, true),
        other => panic!("unknown mode {other}"),
    }
}

/// The field-by-field diff of one row; each entry is a divergence NAME.
fn diff_row(row: &Row, v5: &V5Outcome) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let thrown = row
        .thrown
        .as_ref()
        .unwrap_or_else(|| panic!("{}: v4 must have thrown", label(row)));
    match &v5.side {
        None => out.push(("side", "v5 attached no refusal side".to_string())),
        Some(side) => {
            let want_code = thrown.code.as_ref().and_then(code_string);
            let want_nested = thrown.error_code.as_ref().and_then(code_string);
            let fields: [(&'static str, String, String); 6] = [
                ("message", thrown.message.clone(), side.message.clone()),
                ("code", format!("{want_code:?}"), format!("{:?}", side.code)),
                (
                    "nested_code",
                    format!("{want_nested:?}"),
                    format!("{:?}", side.nested_code),
                ),
                (
                    "name",
                    format!("{:?}", thrown.name),
                    format!("{:?}", side.name),
                ),
                (
                    "status",
                    format!("{:?}", thrown.status),
                    format!("{:?}", side.status),
                ),
                (
                    "provider_reason",
                    format!("{:?}", thrown.provider_reason),
                    format!("{:?}", side.provider_reason),
                ),
            ];
            for (name, want, got) in fields {
                if want != got {
                    out.push((name, format!("v4 {want} | v5 {got}")));
                }
            }
        }
    }
    // The verdict over the side (v4's classifier over the thrown value). With
    // no side v5 classifies the message alone — the pre-P4.118 behaviour.
    let synthesized;
    let input_err = match &v5.side {
        Some(s) => s,
        None => {
            synthesized = RefusalError::message_only(v5.message.clone());
            &synthesized
        }
    };
    let verdict = classify_refusal(RefusalInput {
        error: Some(input_err),
        ..Default::default()
    });
    let got_verdict = (
        verdict.refused,
        verdict.evidence.map(|e| e.as_str().to_string()),
        verdict.detail.clone(),
    );
    let want_verdict = (
        row.refusal.refused,
        row.refusal.evidence.clone(),
        row.refusal.detail.clone(),
    );
    if got_verdict != want_verdict {
        out.push((
            "verdict",
            format!("v4 {want_verdict:?} | v5 {got_verdict:?}"),
        ));
    }
    if v5.trigger != row.trigger {
        out.push((
            "trigger",
            format!("v4 {:?} | v5 {:?}", row.trigger, v5.trigger),
        ));
    }
    out
}

#[test]
fn text_http_errors_match_v4s_real_plugins() {
    let text = std::fs::read_to_string(CORPUS).unwrap_or_else(|e| panic!("read {CORPUS}: {e}"));
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("corpus row parses"))
        .collect();
    // 33 cases × (9 providers × 2 modes + OPENROUTER × 4 modes).
    assert_eq!(
        rows.len(),
        33 * 22,
        "corpus row count; regenerate the corpus"
    );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut found: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut unexpected: Vec<String> = Vec::new();
    let mut refused_v4 = 0usize;
    let mut refused_matched = 0usize;

    for row in &rows {
        assert_eq!(row.outcome, "thrown", "{}: v4 must throw", label(row));
        let v5 = run_v5(&rt, row);
        // (4) the message bytes do not move.
        assert_eq!(
            v5.message,
            format!("HTTP {}: {}", row.status, row.body),
            "{}: v5's message must stay the transport's bytes (§S.5)",
            label(row)
        );
        // v4 issued exactly one request per row (no SDK retry on these
        // statuses); so must v5.
        assert_eq!(v5.calls, row.fetch_calls, "{}: request count", label(row));

        let diffs = diff_row(row, &v5);
        if row.refusal.refused {
            refused_v4 += 1;
            if !diffs.iter().any(|(n, _)| *n == "verdict") {
                refused_matched += 1;
            }
        }
        for (field, detail) in diffs {
            let key = (
                row.case.clone(),
                row.provider.clone(),
                row.mode.clone(),
                field.to_string(),
            );
            if !expected(&key.0, &key.1, &key.2, &key.3) {
                unexpected.push(format!("{} {field}: {detail}", label(row)));
            }
            found.insert(key);
        }
    }

    eprintln!(
        "text_http_errors: {} rows; v4 refused {refused_v4}, v5 matched {refused_matched}; {} divergence(s) found",
        rows.len(),
        found.len()
    );
    // Non-vacuity: v4 refuses 162 rows — 154 on `provider-code` (the six
    // openai-SDK providers × 12 coded cases × 2 modes = 144, Anthropic's
    // nested `body.code` on 4 cases × 2 = 8, OpenRouter's SDK path on the
    // numeric `1301` × 2 = 2) and 8 on `message-pattern` (the
    // `content_policy_violation` wording through Anthropic, Google and the two
    // fetch plugins, × 2) — and v5 matches every one bar the two OpenRouter
    // SDK-path `1301` rows pinned above. Before P4.118 wired the side, v5 matched NONE.
    assert_eq!(refused_v4, 162, "v4's refused rows");
    assert_eq!(refused_matched, 160, "v5's matching refusal verdicts");
    let all_cases: BTreeSet<&str> = rows.iter().map(|r| r.case.as_str()).collect();
    let mut missing: Vec<String> = Vec::new();
    for (p, m, f, cases) in EXPECTED_DIVERGENCES {
        let listed: Vec<&str> = if *cases == EVERY_CASE {
            all_cases.iter().copied().collect()
        } else {
            cases.to_vec()
        };
        for c in listed {
            let key = (c.to_string(), p.to_string(), m.to_string(), f.to_string());
            if !found.contains(&key) {
                missing.push(format!("{c}/{p}/{m} {f}"));
            }
        }
    }
    assert!(
        unexpected.is_empty() && missing.is_empty(),
        "{} unexpected divergence(s):\n{}\n{} expected divergence(s) no longer diverge:\n{}",
        unexpected.len(),
        unexpected.join("\n"),
        missing.len(),
        missing.join("\n")
    );
}
