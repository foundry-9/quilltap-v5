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
use quilltap_core::test_support::captured_with;
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
    /// `None` on a posed TRANSPORT failure (`transport_fetch_throws`, P4.128).
    status: Option<u16>,
    body: Option<String>,
    #[serde(rename = "fetchCalls")]
    fetch_calls: usize,
    outcome: String,
    thrown: Option<Thrown>,
    refusal: Verdict,
    trigger: Option<String>,
    /// P4.128: the ERROR lines v4's plugin logged while the row ran (the
    /// host-bridge logger); absent when it logged none.
    #[serde(rename = "pluginErrorLog", default)]
    plugin_error_log: Vec<PluginLine>,
}

#[derive(Deserialize)]
struct PluginLine {
    message: String,
    context: Value,
    error: Option<String>,
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
    // P4.128's posed TRANSPORT failure: v4 threw the SDK's
    // `APIConnectionError` (`Connection error.`, no status, no code); v5's
    // refusal side models an HTTP failure only (`transport_error_refusal`
    // needs a status), so there is no side to diff. The verdict (not refused)
    // and the trigger (`provider-error`) agree regardless — both are compared
    // and pass on these rows; only the side's ABSENCE is the difference.
    (
        "OPENAI_COMPATIBLE",
        "stream",
        "side",
        &["transport_fetch_throws"],
    ),
    (
        "OPENAI_COMPATIBLE",
        "send",
        "side",
        &["transport_fetch_throws"],
    ),
    ("DEEPSEEK", "stream", "side", &["transport_fetch_throws"]),
    ("DEEPSEEK", "send", "side", &["transport_fetch_throws"]),
    ("NANOGPT", "stream", "side", &["transport_fetch_throws"]),
    ("NANOGPT", "send", "side", &["transport_fetch_throws"]),
    // A non-JSON body served AS `application/json`: `@google/genai` throws a
    // bare `SyntaxError` (no `status`, V8's message); v5 synthesizes the
    // `ApiError` (the third approximation in `model::provider_error`'s doc).
    (
        "GOOGLE",
        "send",
        "message",
        &[
            "google_json_as_text_plain",
            "google_html_as_json",
            "google_empty_body_as_json",
        ],
    ),
    (
        "GOOGLE",
        "send",
        "name",
        &["google_html_as_json", "google_empty_body_as_json"],
    ),
    (
        "GOOGLE",
        "send",
        "status",
        &["google_html_as_json", "google_empty_body_as_json"],
    ),
    (
        "GOOGLE",
        "stream",
        "name",
        &["google_html_as_json", "google_empty_body_as_json"],
    ),
    (
        "GOOGLE",
        "stream",
        "status",
        &["google_html_as_json", "google_empty_body_as_json"],
    ),
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
            "google_empty_body_as_json",
            "google_html_as_json",
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
        &[
            "google_json_as_text_plain",
            "google_html_as_json",
            "google_empty_body_as_json",
        ],
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
            "google_empty_body_as_json",
            "google_html_as_json",
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

/// v5's OWN trigger on each pinned GOOGLE trigger divergence (v4 reads the
/// digit-free `JSON.stringify(body)` / `SyntaxError` message as
/// `provider-error`; v5's `HTTP {status}:` bytes meet the 4xx rule → no
/// trigger, except the 401's `auth`). Absent = `None`.
const V5_GOOGLE_TRIGGER: &[(&str, Option<&str>)] = &[("invalid_api_key_401", Some("auth"))];

fn expected(case: &str, provider: &str, mode: &str, field: &str) -> bool {
    EXPECTED_DIVERGENCES.iter().any(|(p, m, f, cases)| {
        *p == provider
            && *m == mode
            && *f == field
            && (*cases == EVERY_CASE || cases.contains(&case))
    })
}

/// A transport that fails every call with the posed non-2xx, rendered exactly
/// as `ReqwestTransport` renders it, and counts the calls. With no status it
/// poses a TRANSPORT failure (P4.128's `transport_fetch_throws`): reqwest's
/// connect-failure rendering, no status, no body.
struct PosedFailure {
    status: Option<u16>,
    body: String,
    calls: AtomicUsize,
}

impl PosedFailure {
    fn new(status: Option<u16>, body: Option<&str>) -> Self {
        Self {
            status,
            body: body.unwrap_or_default().to_string(),
            calls: AtomicUsize::new(0),
        }
    }

    fn fail(&self, request: &TransportRequest) -> TransportError {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.status {
            Some(status) => TransportError::http(status, &self.body),
            None => TransportError::connect(posed_connect_failure(&request.url)),
        }
    }
}

/// `reqwest::Error`'s `Display` for a refused connection — what
/// `ReqwestTransport` hands up for v4's `fetch failed`.
fn posed_connect_failure(url: &str) -> String {
    format!("error sending request for url ({url})")
}

impl ProviderTransport for PosedFailure {
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        let e = self.fail(request);
        Box::pin(async move { Err(e) })
    }

    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        let e = self.fail(request);
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
    /// Every tracing line the call emitted on the caller thread (the
    /// pre-stream arm and the whole completion path run there).
    lines: Vec<String>,
}

fn run_stream(rt: &tokio::runtime::Runtime, row: &Row) -> V5Outcome {
    let provider = WireStreamingProvider::new(
        PosedFailure::new(row.status, row.body.as_deref()),
        SingleKey(String::new()),
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );
    let (items, lines) = captured_with(|| {
        rt.block_on(async {
            let mut rx = provider
                .stream_message(&row.provider, None, &stream_params(&row.provider))
                .await;
            let mut out = Vec::new();
            while let Some(item) = rx.recv().await {
                out.push(item);
            }
            out
        })
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
        lines,
    }
}

fn run_send(rt: &tokio::runtime::Runtime, row: &Row, vision: bool) -> V5Outcome {
    let transport = PosedFailure::new(row.status, row.body.as_deref());
    let params = completion_params(&row.provider, vision);
    let (result, lines) = captured_with(|| {
        rt.block_on(execute_completion(
            &transport,
            &row.provider,
            None,
            "",
            &params,
            &TransportPolicy::default(),
            "Quilltap/test",
            None,
            None,
        ))
    });
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
        lines,
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
        // A pinned GOOGLE trigger divergence also pins v5's OWN value: a
        // regression from one wrong value to another must not hide inside
        // the carve-out (the `97b25fc53` unification review).
        if row.provider == "GOOGLE" {
            let want_v5 = V5_GOOGLE_TRIGGER
                .iter()
                .find(|(c, _)| *c == row.case)
                .map(|(_, t)| *t)
                .unwrap_or(None);
            assert_eq!(
                v5.trigger.as_deref(),
                want_v5,
                "{}: v5's trigger on a pinned GOOGLE divergence moved (v4 {:?})",
                label(row),
                row.trigger
            );
        }
        out.push((
            "trigger",
            format!("v4 {:?} | v5 {:?}", row.trigger, v5.trigger),
        ));
    }
    out
}

/// The three openai-SDK plugins whose `streamMessage` / `sendMessage` wrap
/// the SDK call in a `try … catch { this.logger.error('<Name> API error in
/// <method>', { context, baseUrl }, error); throw }` (P4.122 item 6 ported the
/// MID-stream half; P4.128 the pre-stream and `sendMessage` halves).
const CATCH_LINE_PROVIDERS: &[&str] = &["OPENAI_COMPATIBLE", "DEEPSEEK", "NANOGPT"];

/// ERROR lines v4's OTHER plugins log on these rows that v5 has NOT ported
/// (measured at the `97b25fc53` pin by P4.128's logger bridge; outside that
/// order's mandate — a named follow-up). Pinned BOTH ways: a NEW v4 line
/// fails the per-row check in [`diff_catch_lines`]; an entry v4 stops
/// emitting fails the exercised-count assert in the test; and a v5 PORT of
/// one fails the absence check (v5's whole capture, not only its catch-line
/// filter — the `97b25fc53` smalls unification's catch: the first shape
/// noticed a new v4 line and nothing else). `(provider, v4 mode, message)`.
const UNPORTED_PLUGIN_ERROR_LINES: &[(&str, &str, &str)] = &[
    ("GOOGLE", "send", "Error calling Google Gemini API"),
    ("GOOGLE", "stream", "Error streaming from Google Gemini API"),
    ("OLLAMA", "send", "Ollama API error response"),
    ("OLLAMA", "send", "Ollama sendMessage failed"),
    ("OLLAMA", "stream", "Ollama streaming API error"),
    ("OLLAMA", "stream", "Ollama streamMessage failed"),
    ("OPENROUTER", "send_vision", "OpenRouter API error"),
    ("OPENROUTER", "stream_tools", "OpenRouter API error"),
    (
        "OPENROUTER",
        "stream_tools",
        "Error in streamViaChatCompletions",
    ),
];

/// P4.128 — the plugin catch line, diffed against v4's recorded
/// `pluginErrorLog` field for field: v5's captured `… API error in …` lines
/// must be exactly v4's (a silence leg on every row where v4 logged none),
/// and every OTHER v4 plugin ERROR line must be a pinned unported one. Returns
/// the number of v4 catch lines on the row, or the mismatch.
fn diff_catch_lines(
    row: &Row,
    v5: &V5Outcome,
    unported_seen: &mut BTreeSet<(String, String, String)>,
) -> Result<usize, String> {
    for (_, _, msg) in UNPORTED_PLUGIN_ERROR_LINES {
        assert!(
            !v5.lines.iter().any(|l| l.contains(msg)),
            "{}: v5 now emits an UNPORTED_PLUGIN_ERROR_LINES message ({msg:?}) — port it as a \
             diffed catch line and retire its entry",
            label(row)
        );
    }
    let (method, target) = if row.mode.starts_with("stream") {
        ("streamMessage", "quilltap::model::streaming_provider")
    } else {
        ("sendMessage", "quilltap::model::completion_provider")
    };
    let got: Vec<&str> = v5
        .lines
        .iter()
        .map(String::as_str)
        .filter(|l| {
            l.contains(" API error in streamMessage") || l.contains(" API error in sendMessage")
        })
        .collect();
    let mut want: Vec<String> = Vec::new();
    for e in &row.plugin_error_log {
        if CATCH_LINE_PROVIDERS.contains(&row.provider.as_str())
            && e.message.ends_with(&format!(" API error in {method}"))
        {
            want.push(format!(
                "ERROR {target} {} context={} baseUrl={} error={}",
                e.message,
                e.context["context"].as_str().unwrap(),
                e.context["baseUrl"].as_str().unwrap(),
                e.error.as_deref().expect("the SDK threw an Error"),
            ));
        } else {
            let triple = (row.provider.as_str(), row.mode.as_str(), e.message.as_str());
            assert!(
                UNPORTED_PLUGIN_ERROR_LINES.contains(&triple),
                "{}: v4 logged an ERROR line no pin names: {:?}",
                label(row),
                e.message
            );
            unported_seen.insert((
                triple.0.to_string(),
                triple.1.to_string(),
                triple.2.to_string(),
            ));
        }
    }
    let want: Vec<&str> = want.iter().map(String::as_str).collect();
    if got == want {
        Ok(want.len())
    } else {
        Err(format!(
            "{}: the plugin catch line\n  v5: {got:?}\n  v4: {want:?}",
            label(row)
        ))
    }
}

#[test]
fn text_http_errors_match_v4s_real_plugins() {
    let text = std::fs::read_to_string(CORPUS).unwrap_or_else(|e| panic!("read {CORPUS}: {e}"));
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("corpus row parses"))
        .collect();
    // 33 cases × (9 providers × 2 modes + OPENROUTER × 4 modes), plus the two
    // GOOGLE-only rows (`google_html_as_json`, `google_empty_body_as_json`:
    // a body that is not JSON served AS `application/json`) × 2 modes, plus
    // P4.128's posed transport failure through the three catch-line plugins
    // × 2 modes.
    assert_eq!(
        rows.len(),
        33 * 22 + 2 * 2 + 3 * 2,
        "corpus row count; regenerate the corpus"
    );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut found: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut unexpected: Vec<String> = Vec::new();
    let mut refused_v4 = 0usize;
    let mut refused_matched = 0usize;
    let mut transport_rows = 0usize;
    let mut catch_lines = 0usize;
    let mut unported_seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut catch_mismatches: Vec<String> = Vec::new();

    for row in &rows {
        assert_eq!(row.outcome, "thrown", "{}: v4 must throw", label(row));
        let v5 = run_v5(&rt, row);
        match row.status {
            Some(status) => {
                // (4) the message bytes do not move.
                assert_eq!(
                    v5.message,
                    format!(
                        "HTTP {}: {}",
                        status,
                        row.body.as_deref().unwrap_or_default()
                    ),
                    "{}: v5's message must stay the transport's bytes (§S.5)",
                    label(row)
                );
                // v4 issued exactly one request per row (no SDK retry on these
                // statuses); so must v5.
                assert_eq!(v5.calls, row.fetch_calls, "{}: request count", label(row));
            }
            None => {
                // A transport failure: v5's message stays the transport's own
                // (RULED at planning — only the catch line's `error` field
                // takes v4's `Connection error.`), and the SDK's retries (1 +
                // `DEFAULT_MAX_RETRIES` 2) are recorded but NOT compared: the
                // 2026-07-23 provider-I/O ruling (v5's retries live inside
                // `ReqwestTransport`, which a posed transport replaces).
                assert!(
                    v5.message.starts_with("error sending request for url ("),
                    "{}: v5's transport message moved: {}",
                    label(row),
                    v5.message
                );
                assert_eq!(row.fetch_calls, 3, "{}: v4's SDK retry count", label(row));
                transport_rows += 1;
            }
        }
        match diff_catch_lines(row, &v5, &mut unported_seen) {
            Ok(n) => catch_lines += n,
            Err(e) => catch_mismatches.push(e),
        }

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
    assert_eq!(transport_rows, 6, "the posed transport-failure rows");
    assert!(
        catch_mismatches.is_empty(),
        "{} row(s) with a catch-line mismatch:\n{}",
        catch_mismatches.len(),
        catch_mismatches.join("\n")
    );
    // P4.128: the three catch-line plugins × 34 cases (33 shared + the
    // transport row) × 2 modes — every one diffed field for field.
    assert_eq!(catch_lines, 3 * 34 * 2, "v4's plugin catch lines diffed");
    let unported_missing: Vec<&(&str, &str, &str)> = UNPORTED_PLUGIN_ERROR_LINES
        .iter()
        .filter(|(p, m, msg)| {
            !unported_seen.contains(&(p.to_string(), m.to_string(), msg.to_string()))
        })
        .collect();
    assert!(
        unported_missing.is_empty(),
        "UNPORTED_PLUGIN_ERROR_LINES entries v4 no longer emits on any row (retire them): {unported_missing:?}"
    );
    assert_eq!(refused_matched, 160, "v5's matching refusal verdicts");
    let mut missing: Vec<String> = Vec::new();
    for (p, m, f, cases) in EXPECTED_DIVERGENCES {
        // "Every case" = every case that HAS a row for that provider/mode
        // (the GOOGLE-only rows have none elsewhere).
        let listed: Vec<&str> = if *cases == EVERY_CASE {
            let cases_here: BTreeSet<&str> = rows
                .iter()
                .filter(|r| r.provider == *p && r.mode == *m)
                .map(|r| r.case.as_str())
                .collect();
            cases_here.into_iter().collect()
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
