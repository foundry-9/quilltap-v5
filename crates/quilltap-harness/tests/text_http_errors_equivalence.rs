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
//! P4.141 widened the corpus past the non-2xx: a posed TRANSPORT failure on
//! all ten providers (`transport_fetch_throws`), a HANG under a 50 ms
//! `requestTimeoutMs` (`transport_hang` — the SDKs' real
//! `APIConnectionTimeoutError` and the raw-`fetch` plugins' abort errors), and
//! five posed 2xx bodies (`ok_*`). The posed exchange follows each case's
//! `cases.json` entry, and the family diffs, per row:
//! - the OUTCOME (v4 `ok` ↔ v5 `Ok`, v4 `thrown` ↔ v5 `Err`);
//! - on a non-2xx, the refusal side (above) and v5's `HTTP {status}:` bytes;
//!   on a status-less row, v5's transport bytes (the RULED P4.128 shape — only
//!   the catch line carries v4's text) and the transport KIND; on a thrown 2xx
//!   row, v5's message against v4's thrown `message` (no v5 transport text
//!   exists there, so the message IS v4's);
//! - the verdict and the trigger, through the Salon's and the cheap path's own
//!   hand-overs (`FallbackError::from_stream_error` /
//!   `FallbackError::from_completion_error`);
//! - EVERY ERROR and WARN line v5 logged on the two model targets against v4's
//!   whole `pluginErrorLog` / `pluginWarnLog`, rendered from v4's `context`
//!   in key order plus the logger's third-argument `error`;
//! - the request count, 1-for-1 on status and 2xx rows; recorded and NOT
//!   compared on a status-less row (the 2026-07-23 provider-I/O ruling — the
//!   SDKs' retries are the transport's, not the port's contract).
//!
//! P4.150 widened it again: six GOOGLE-only 2xx cases (one mode each, through
//! the real `@google/genai` 1.52.0) for `extractTextFromResponse`'s `No parts
//! found in Google response candidate` WARN on both paths, the stream path's
//! `No candidates…` WARN, and the `content.text` fallback — measured REACHABLE
//! (the SDK keeps the key); every answered row now compares v4's
//! `okResult.content`; an HTTP row asserts the `Http` kind, a thrown 2xx
//! asserts NO kind; and both transport arms record the policy budget, which
//! must be the row's `requestTimeoutMs` (the send path through the production
//! `quilltap_host::spine::completion_send_policy`).
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
use std::sync::Mutex;

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
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportErrorKind, TransportPolicy,
    TransportRequest, TransportResponse,
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
/// The posed exchanges the corpus was recorded from (P4.141 — a case's
/// `transport` and `requestTimeoutMs` are not on its rows).
const CASES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../harness/oracle/fixtures/text-http-errors/cases.json"
);

#[derive(Deserialize)]
struct CaseSpec {
    case: String,
    transport: Option<String>,
    #[serde(rename = "requestTimeoutMs")]
    request_timeout_ms: Option<i64>,
}

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
    /// P4.141: the WARN lines (same bridge, no third argument).
    #[serde(rename = "pluginWarnLog", default)]
    plugin_warn_log: Vec<PluginLine>,
    /// What v4 answered on an `ok` row (`{content}` for a send, `{chunks,
    /// content}` for a stream) — P4.150 D2 compares the CONTENT.
    #[serde(rename = "okResult", default)]
    ok_result: Option<Value>,
}

#[derive(Deserialize)]
struct PluginLine {
    message: String,
    /// v4's context object, in v4's key order (`preserve_order`).
    context: Value,
    #[serde(default)]
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
    // (P4.128 pinned the six `transport_fetch_throws` rows' missing refusal
    // SIDE here. P4.141 scopes the side comparand to the rows that HAVE one by
    // design — a non-2xx: `transport_error_refusal` models an HTTP failure
    // only, so a status-less or 2xx row is diffed on its message, verdict,
    // trigger and lines instead, and those six entries retired with the
    // scoping.)
    //
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
    // P4.141: the Google plugin's catch line logs the thrown value's message
    // as its `error`, so the three approximated messages above surface on
    // the line too — the same divergence, one field over.
    (
        "GOOGLE",
        "send",
        "lines",
        &[
            "google_empty_body_as_json",
            "google_html_as_json",
            "google_json_as_text_plain",
        ],
    ),
    (
        "GOOGLE",
        "stream",
        "lines",
        &[
            "google_empty_body_as_json",
            "google_html_as_json",
            "google_json_as_text_plain",
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
    // P4.141's posed 2xx bodies through `@openrouter/sdk` (v4's no-tools
    // stream / no-image send): the SDK schema-validates the body and throws
    // `ResponseValidationError` (or V8's `JSON.parse` text) where v5's raw
    // wire answers — the same `openrouter-sdk` class.
    (
        "OPENROUTER",
        "send",
        "outcome",
        &[
            "ok_choice_no_message",
            "ok_choices_empty",
            "ok_empty_object",
        ],
    ),
    (
        "OPENROUTER",
        "stream",
        "outcome",
        &[
            "ok_choice_no_message",
            "ok_choices_empty",
            "ok_empty_body_json",
            "ok_empty_object",
            "ok_non_json",
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
/// The [`EXPECTED_DIVERGENCES`] marker for "every non-2xx case of the
/// corpus" (the response rows — see the both-ways check).
const EVERY_CASE: &[&str] = &["*"];

/// v5's OWN trigger on each pinned GOOGLE trigger divergence (v4 reads the
/// digit-free `JSON.stringify(body)` / `SyntaxError` message as
/// `provider-error`; v5's `HTTP {status}:` bytes meet the 4xx rule → no
/// trigger, except the 401's `auth`). Absent = `None`.
const V5_GOOGLE_TRIGGER: &[(&str, Option<&str>)] = &[("invalid_api_key_401", Some("auth"))];

fn expected(row: &Row, field: &str) -> bool {
    let non_2xx = row.status.is_some_and(|s| !(200..300).contains(&s));
    EXPECTED_DIVERGENCES.iter().any(|(p, m, f, cases)| {
        *p == row.provider
            && *m == row.mode
            && *f == field
            && ((*cases == EVERY_CASE && non_2xx) || cases.contains(&row.case.as_str()))
    })
}

/// The exchange a row poses (P4.141 — the case's `transport` first, else its
/// status): a non-2xx, a refused connection, a deadline, or a 2xx body.
#[derive(Clone, Debug)]
enum Posed {
    Http { status: u16, body: String },
    Connect,
    Timeout,
    Ok2xx(Vec<u8>),
}

impl Posed {
    fn for_row(row: &Row, spec: &CaseSpec) -> Self {
        match spec.transport.as_deref() {
            Some("fetch-throws") => Posed::Connect,
            Some("hang") => Posed::Timeout,
            Some(other) => panic!("{}: unknown posed transport {other:?}", label(row)),
            None => {
                let status = row
                    .status
                    .unwrap_or_else(|| panic!("{}: a response row has a status", label(row)));
                let body = row.body.clone().unwrap_or_default();
                if (200..300).contains(&status) {
                    Posed::Ok2xx(body.into_bytes())
                } else {
                    Posed::Http { status, body }
                }
            }
        }
    }

    fn is_statusless(&self) -> bool {
        matches!(self, Posed::Connect | Posed::Timeout)
    }
}

/// A transport answering every call with the row's posed exchange, rendered
/// exactly as `ReqwestTransport` renders it, and counting the calls.
struct PosedTransport {
    posed: Posed,
    calls: AtomicUsize,
    /// The URL of the last request the composer handed over.
    url: Mutex<String>,
    /// The `TransportPolicy.timeout` the composer handed over on the last call
    /// — BOTH arms record it (P4.150 D3: `execute` used to ignore `policy`, so
    /// the row's `requestTimeoutMs` reaching the NON-streaming transport was
    /// unproven).
    timeout: Mutex<Option<std::time::Duration>>,
}

impl PosedTransport {
    fn new(posed: Posed) -> Self {
        Self {
            posed,
            calls: AtomicUsize::new(0),
            url: Mutex::new(String::new()),
            timeout: Mutex::new(None),
        }
    }

    fn seen_url(&self) -> String {
        self.url.lock().unwrap().clone()
    }

    fn seen_timeout(&self) -> Option<std::time::Duration> {
        *self.timeout.lock().unwrap()
    }

    fn saw(&self, request: &TransportRequest, policy: &TransportPolicy) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.url.lock().unwrap() = request.url.clone();
        *self.timeout.lock().unwrap() = Some(policy.timeout);
    }
}

/// `reqwest::Error`'s `Display` for a request that never got a response — a
/// refused connection AND reqwest's own whole-exchange timeout render alike
/// (`reqwest-0.12.28/src/error.rs:231,267-269`); only the KIND tells them
/// apart (P4.141).
fn posed_connect_failure(url: &str) -> String {
    format!("error sending request for url ({url})")
}

impl ProviderTransport for PosedTransport {
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        self.saw(request, policy);
        let out = match &self.posed {
            Posed::Http { status, body } => Err(TransportError::http(*status, body)),
            Posed::Connect => Err(TransportError::connect(posed_connect_failure(&request.url))),
            Posed::Timeout => Err(TransportError::timeout(posed_connect_failure(&request.url))),
            Posed::Ok2xx(body) => Ok(TransportResponse {
                status: 200,
                body: body.clone(),
            }),
        };
        Box::pin(async move { out })
    }

    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        self.saw(request, policy);
        let posed = self.posed.clone();
        let url = request.url.clone();
        // The streaming arm's deadline is the time-to-headers budget — the
        // POLICY's, so a hang row also proves the row's `requestTimeoutMs`
        // reached the transport.
        let budget = policy.timeout.as_millis();
        Box::pin(async move {
            match posed {
                Posed::Http { status, body } => Err(TransportError::http(status, body)),
                Posed::Connect => Err(TransportError::connect(posed_connect_failure(&url))),
                Posed::Timeout => Err(TransportError::headers_timeout(budget)),
                Posed::Ok2xx(body) => {
                    let (tx, rx) = tokio::sync::mpsc::channel(1);
                    if !body.is_empty() {
                        let _ = tx.send(Ok(body)).await;
                    }
                    Ok(rx)
                }
            }
        })
    }
}

fn model_for(provider: &str) -> &'static str {
    if provider == "GOOGLE" {
        "gemini-2.5-flash"
    } else {
        "test-model"
    }
}

/// The recorder's `TOOL` (the OpenRouter `stream_tools` mode's tool — v4's
/// `hasTools` raw-path gate).
fn recorder_tool() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "lookup",
            "description": "Look something up.",
            "parameters": {
                "type": "object",
                "properties": { "q": { "type": "string" } },
                "required": ["q"]
            }
        }
    })
}

fn stream_params(row: &Row, spec: &CaseSpec) -> StreamParams {
    StreamParams {
        messages: vec![StreamMessage::user("hi")],
        model: model_for(&row.provider).into(),
        temperature: None,
        max_tokens: Some(64),
        top_p: None,
        tools: (row.mode == "stream_tools").then(|| Value::Array(vec![recorder_tool()])),
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
        request_timeout_ms: spec.request_timeout_ms,
    }
}

// The recorder's 1×1 PNG (the OpenRouter `send_vision` mode's attachment).
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn completion_params(row: &Row, spec: &CaseSpec, vision: bool) -> CompletionParams {
    CompletionParams {
        messages: vec![CompletionMessage::user(if vision {
            "what is this"
        } else {
            "hi"
        })],
        model: model_for(&row.provider).into(),
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
        request_timeout_ms: spec.request_timeout_ms,
    }
}

/// What v5 produced for one posed exchange.
struct V5Outcome {
    /// `None` when v5 answered (`Ok` / a stream that ended clean).
    error: Option<V5Error>,
    calls: usize,
    /// The URL the transport was handed (the status-less rows' message pin).
    url: String,
    /// The policy budget the transport was handed (P4.150 D3).
    timeout: Option<std::time::Duration>,
    /// The answered content on an `Ok` (a send's `content`; a stream's chunk
    /// contents concatenated) — P4.150 D2.
    content: Option<String>,
    /// Every tracing line the call emitted (the pre-stream arm and the whole
    /// completion path run on the caller thread; the pump logs under the
    /// caller's dispatcher).
    lines: Vec<String>,
}

struct V5Error {
    message: String,
    side: Option<RefusalError>,
    trigger: Option<String>,
    transport_kind: Option<TransportErrorKind>,
}

fn run_stream(rt: &tokio::runtime::Runtime, row: &Row, spec: &CaseSpec) -> V5Outcome {
    let posed = Posed::for_row(row, spec);
    let one_item = !matches!(posed, Posed::Ok2xx(_));
    let provider = WireStreamingProvider::new(
        PosedTransport::new(posed),
        SingleKey(String::new()),
        TransportPolicy::default(),
        "Quilltap/test".to_string(),
    );
    let params = stream_params(row, spec);
    let (items, lines) = captured_with(|| {
        rt.block_on(async {
            let mut rx = provider.stream_message(&row.provider, None, &params).await;
            let mut out = Vec::new();
            while let Some(item) = rx.recv().await {
                out.push(item);
            }
            out
        })
    });
    if one_item {
        assert_eq!(
            items.len(),
            1,
            "{}: a pre-stream failure is one item",
            label(row)
        );
    }
    let content = items.iter().all(Result::is_ok).then(|| {
        items
            .iter()
            .filter_map(|i| i.as_ref().ok())
            .map(|c| c.content.as_str())
            .collect::<String>()
    });
    let error = items
        .into_iter()
        .find_map(Result::err)
        .map(|err: StreamError| {
            // The Salon's hand-over (`primary_stream.rs`).
            let trigger = classify_fallback_trigger(FallbackError::from_stream_error(&err))
                .map(|t| t.as_str().to_string());
            V5Error {
                message: err.message.clone(),
                side: err.refusal.as_deref().cloned(),
                trigger,
                transport_kind: stream_transport_kind(&err),
            }
        });
    V5Outcome {
        error,
        calls: provider.transport_ref().calls.load(Ordering::SeqCst),
        url: provider.transport_ref().seen_url(),
        timeout: provider.transport_ref().seen_timeout(),
        content,
        lines,
    }
}

fn run_send(rt: &tokio::runtime::Runtime, row: &Row, spec: &CaseSpec, vision: bool) -> V5Outcome {
    let transport = PosedTransport::new(Posed::for_row(row, spec));
    let params = completion_params(row, spec, vision);
    // The production send composition's policy (`WireCompletionProvider` —
    // the row's `requestTimeoutMs` as THIS call's budget), not the process
    // default the family handed over before P4.150 D3.
    let policy = quilltap_host::spine::completion_send_policy(
        &TransportPolicy::default(),
        &row.provider,
        &params,
    );
    let (result, lines) = captured_with(|| {
        rt.block_on(execute_completion(
            &transport,
            &row.provider,
            None,
            "",
            &params,
            &policy,
            "Quilltap/test",
            None,
            None,
        ))
    });
    let content = result.as_ref().ok().map(|r| r.content.clone());
    let error = result.err().map(|err: CompletionError| {
        // The cheap path's hand-over (`cheap_llm_exec.rs`).
        let trigger =
            classify_fallback_trigger(cheap_path_hand_over(&err)).map(|t| t.as_str().to_string());
        V5Error {
            message: err.message.clone(),
            side: err.refusal.as_deref().cloned(),
            trigger,
            transport_kind: completion_transport_kind(&err),
        }
    });
    V5Outcome {
        error,
        calls: transport.calls.load(Ordering::SeqCst),
        url: transport.seen_url(),
        timeout: transport.seen_timeout(),
        content,
        lines,
    }
}

/// The cheap path's hand-over (`cheap_llm_exec.rs`): the production
/// `FallbackError::from_completion_error` — the message, the refusal side
/// when the completion carried one, and `network` for a network-class
/// transport failure (P4.141).
fn cheap_path_hand_over(err: &CompletionError) -> FallbackError<'_> {
    FallbackError::from_completion_error(err)
}

/// The transport kind a stream error carries (P4.141 Tier 1 item 4).
fn stream_transport_kind(err: &StreamError) -> Option<TransportErrorKind> {
    err.transport_kind
}

/// The transport kind a completion error carries (as above).
fn completion_transport_kind(err: &CompletionError) -> Option<TransportErrorKind> {
    err.transport_kind
}

fn label(row: &Row) -> String {
    format!("{}/{}/{}", row.case, row.provider, row.mode)
}

/// Run v5 for a recorded v4 row (see the module doc's row mapping).
fn run_v5(rt: &tokio::runtime::Runtime, row: &Row, spec: &CaseSpec) -> V5Outcome {
    match row.mode.as_str() {
        "stream" | "stream_tools" => run_stream(rt, row, spec),
        "send" => run_send(rt, row, spec, false),
        "send_vision" => run_send(rt, row, spec, true),
        other => panic!("unknown mode {other}"),
    }
}

/// The field-by-field diff of one row; each entry is a divergence NAME.
fn diff_row(row: &Row, posed: &Posed, v5: &V5Outcome) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let v4_thrown = row.outcome == "thrown";
    match (v4_thrown, &v5.error) {
        (true, None) | (false, Some(_)) => {
            out.push((
                "outcome",
                format!(
                    "v4 {} | v5 {}",
                    row.outcome,
                    v5.error
                        .as_ref()
                        .map(|e| format!("thrown {:?}", e.message))
                        .unwrap_or_else(|| "ok".to_string())
                ),
            ));
            return out;
        }
        (false, None) => {
            // P4.150 D2: what v4 ANSWERED — the content (the `content.text`
            // fallback's comparand; every other answered row its silence leg).
            let want = row
                .ok_result
                .as_ref()
                .and_then(|r| r.get("content"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{}: an ok row carries okResult.content", label(row)));
            if v5.content.as_deref() != Some(want) {
                out.push(("content", format!("v4 {want:?} | v5 {:?}", v5.content)));
            }
            return out;
        }
        (true, Some(_)) => {}
    }
    let v5e = v5.error.as_ref().expect("both threw");
    let thrown = row
        .thrown
        .as_ref()
        .unwrap_or_else(|| panic!("{}: v4 threw", label(row)));
    match posed {
        // The refusal SIDE exists by design on a non-2xx only.
        Posed::Http { .. } => match &v5e.side {
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
        },
        // No v5 transport text exists on a thrown 2xx: the message IS v4's.
        Posed::Ok2xx(_) => {
            if v5e.message != thrown.message {
                out.push((
                    "message",
                    format!("v4 {:?} | v5 {:?}", thrown.message, v5e.message),
                ));
            }
        }
        // A status-less row: v5's message stays the transport's own bytes
        // (asserted by the caller); v4's text rides the catch line.
        Posed::Connect | Posed::Timeout => {}
    }
    // The verdict over the side (v4's classifier over the thrown value). With
    // no side v5 classifies the message alone — the pre-P4.118 behaviour.
    let synthesized;
    let input_err = match &v5e.side {
        Some(s) => s,
        None => {
            synthesized = RefusalError::message_only(v5e.message.clone());
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
    if v5e.trigger != row.trigger {
        // A pinned GOOGLE trigger divergence also pins v5's OWN value: a
        // regression from one wrong value to another must not hide inside
        // the carve-out (the `97b25fc53` unification review).
        if row.provider == "GOOGLE" && matches!(posed, Posed::Http { .. }) {
            let want_v5 = V5_GOOGLE_TRIGGER
                .iter()
                .find(|(c, _)| *c == row.case)
                .map(|(_, t)| *t)
                .unwrap_or(None);
            assert_eq!(
                v5e.trigger.as_deref(),
                want_v5,
                "{}: v5's trigger on a pinned GOOGLE divergence moved (v4 {:?})",
                label(row),
                row.trigger
            );
        }
        out.push((
            "trigger",
            format!("v4 {:?} | v5 {:?}", row.trigger, v5e.trigger),
        ));
    }
    out
}

/// The two tracing targets v4's plugin lines land on in v5 (one per
/// composer — `model::plugin_catch_log`).
const TARGETS: [&str; 2] = [
    "quilltap::model::streaming_provider",
    "quilltap::model::completion_provider",
];

/// v5's captured lines at `level` on the two model targets.
fn v5_plugin_lines<'a>(v5: &'a V5Outcome, level: &str) -> Vec<&'a str> {
    v5.lines
        .iter()
        .map(String::as_str)
        .filter(|l| {
            TARGETS
                .iter()
                .any(|t| l.starts_with(&format!("{level} {t} ")))
        })
        .collect()
}

/// A v4 context value as the capture renders a `%`-sigil field: a string
/// unquoted, anything else as its JSON text.
fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// v4's recorded plugin line as v5's capture renders it: `<LEVEL> <target>
/// <message>`, then every `context` key in v4's order, then the logger's
/// third-argument `error`.
fn render_v4_line(level: &str, row: &Row, line: &PluginLine) -> String {
    let target = if row.mode.starts_with("stream") {
        TARGETS[0]
    } else {
        TARGETS[1]
    };
    let mut out = format!("{level} {target} {}", line.message);
    if let Some(ctx) = line.context.as_object() {
        for (k, v) in ctx {
            out.push_str(&format!(" {k}={}", render_value(v)));
        }
    }
    if let Some(e) = &line.error {
        out.push_str(&format!(" error={e}"));
    }
    out
}

/// WARN lines v4's plugins log on these rows that v5 has NOT ported, pinned
/// BOTH ways (an entry v4 stops emitting fails the exercised check; a v5 port
/// fails the absence check). `(provider, v4 mode, message)`. The Responses
/// SSE end-of-stream WARN lives in v5's `responses_api` decoder — outside
/// P4.141's ownership, a named deferral (the lane record).
const UNPORTED_PLUGIN_WARN_LINES: &[(&str, &str, &str)] = &[
    (
        "OPENAI",
        "stream",
        "Stream ended without response.completed event",
    ),
    (
        "GROK",
        "stream",
        "Stream ended without response.completed event",
    ),
];

/// P4.128 / P4.141 — the plugin lines, diffed against v4's recorded
/// `pluginErrorLog` / `pluginWarnLog`: v5's ERROR (WARN) lines on the two
/// model targets must be exactly v4's whole log, in order (a silence leg on
/// every row where v4 logged none). Returns the number of v4 ERROR lines on
/// the row, or the mismatch.
fn diff_lines(
    row: &Row,
    v5: &V5Outcome,
    unported_warn_seen: &mut BTreeSet<(String, String, String)>,
) -> (Result<usize, String>, Result<(), String>) {
    let got: Vec<&str> = v5_plugin_lines(v5, "ERROR");
    let want: Vec<String> = row
        .plugin_error_log
        .iter()
        .map(|e| render_v4_line("ERROR", row, e))
        .collect();
    let errors = if got == want.iter().map(String::as_str).collect::<Vec<_>>() {
        Ok(want.len())
    } else {
        Err(format!(
            "{}: the plugin ERROR lines\n  v5: {got:?}\n  v4: {want:?}",
            label(row)
        ))
    };
    for (_, _, msg) in UNPORTED_PLUGIN_WARN_LINES {
        assert!(
            !v5.lines.iter().any(|l| l.contains(msg)),
            "{}: v5 now emits an UNPORTED_PLUGIN_WARN_LINES message ({msg:?}) — retire its entry",
            label(row)
        );
    }
    let got_warn: Vec<&str> = v5_plugin_lines(v5, "WARN");
    let mut want_warn: Vec<String> = Vec::new();
    for w in &row.plugin_warn_log {
        let triple = (row.provider.as_str(), row.mode.as_str(), w.message.as_str());
        if UNPORTED_PLUGIN_WARN_LINES.contains(&triple) {
            unported_warn_seen.insert((
                triple.0.to_string(),
                triple.1.to_string(),
                triple.2.to_string(),
            ));
        } else {
            want_warn.push(render_v4_line("WARN", row, w));
        }
    }
    let warns = if got_warn == want_warn.iter().map(String::as_str).collect::<Vec<_>>() {
        Ok(())
    } else {
        Err(format!(
            "{}: the plugin WARN lines\n  v5: {got_warn:?}\n  v4: {want_warn:?}",
            label(row)
        ))
    };
    (errors, warns)
}

#[test]
fn text_http_errors_match_v4s_real_plugins() {
    let text = std::fs::read_to_string(CORPUS).unwrap_or_else(|e| panic!("read {CORPUS}: {e}"));
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("corpus row parses"))
        .collect();
    let specs: Vec<CaseSpec> = serde_json::from_str(
        &std::fs::read_to_string(CASES).unwrap_or_else(|e| panic!("read {CASES}: {e}")),
    )
    .expect("cases.json parses");
    let spec_of = |case: &str| -> &CaseSpec {
        specs
            .iter()
            .find(|s| s.case == case)
            .unwrap_or_else(|| panic!("no cases.json entry for {case}"))
    };
    // Re-derived from the rebuilt NDJSON (P4.141): 33 shared non-2xx cases ×
    // 22 rows, the two GOOGLE-only rows × 2 modes, the two status-less cases ×
    // 20 rows (nine providers × 2 modes + OpenRouter's two raw modes — the
    // SDK modes skipped, the recorder's `modes` override), and the five 2xx
    // cases × 22 rows; plus (P4.150 D2) six GOOGLE-only 2xx cases × ONE mode
    // each (the recorder's `modes` override): four `send`, two `stream`.
    assert_eq!(
        rows.len(),
        33 * 22 + 2 * 2 + 2 * 20 + 5 * 22 + 6,
        "corpus row count; regenerate the corpus"
    );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut found: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut unexpected: Vec<String> = Vec::new();
    let mut refused_v4 = 0usize;
    let mut refused_matched = 0usize;
    let mut statusless_rows = 0usize;
    let mut ok_rows = 0usize;
    let mut catch_lines = 0usize;
    let mut unported_warn_seen: BTreeSet<(String, String, String)> = BTreeSet::new();

    for row in &rows {
        let spec = spec_of(&row.case);
        let posed = Posed::for_row(row, spec);
        let v5 = run_v5(&rt, row, spec);
        if row.outcome == "ok" {
            ok_rows += 1;
        }
        if let Some(e) = &v5.error {
            match &posed {
                Posed::Http { status, body } => {
                    // (4) the message bytes do not move.
                    assert_eq!(
                        e.message,
                        format!("HTTP {status}: {body}"),
                        "{}: v5's message must stay the transport's bytes (§S.5)",
                        label(row)
                    );
                    // P4.150 D3: an HTTP failure carries the `Http` kind (every
                    // `TransportError` maps through `.with_transport(kind, …)`)
                    // — asserted only for Connect/Timeout before.
                    assert_eq!(
                        e.transport_kind,
                        Some(TransportErrorKind::Http),
                        "{}: the transport kind",
                        label(row)
                    );
                }
                Posed::Connect | Posed::Timeout => {
                    // v5's message stays the transport's own (RULED at P4.128
                    // planning — only the catch line's `error` takes v4's
                    // text): reqwest's `Display` on the non-streaming arm, the
                    // time-to-headers budget on the streaming one.
                    let want = if row.mode.starts_with("stream") && matches!(posed, Posed::Timeout)
                    {
                        TransportError::headers_timeout(
                            spec.request_timeout_ms
                                .expect("a hang row carries a budget")
                                as u128,
                        )
                        .message
                    } else {
                        posed_connect_failure(&v5.url)
                    };
                    assert_eq!(e.message, want, "{}: v5's transport message", label(row));
                    // The KIND the transport reported travels on the error
                    // (P4.141 Tier 1 item 4) — the posed one.
                    let want_kind = match posed {
                        Posed::Timeout => TransportErrorKind::Timeout,
                        _ => TransportErrorKind::Connect,
                    };
                    assert_eq!(
                        e.transport_kind,
                        Some(want_kind),
                        "{}: the transport kind",
                        label(row)
                    );
                }
                // P4.150 D3: a thrown 2xx (a parse failure) is NOT a transport
                // failure — no kind, or `is_timeout_failure` / the `network`
                // trigger could read it as one.
                Posed::Ok2xx(_) => assert_eq!(
                    e.transport_kind,
                    None,
                    "{}: a 2xx parse error carries no transport kind",
                    label(row)
                ),
            }
        }
        // P4.150 D3: the policy budget the transport saw, on BOTH arms — the
        // row's `requestTimeoutMs` where it carries one, else the default.
        let want_budget = spec.request_timeout_ms.map_or_else(
            || TransportPolicy::default().timeout,
            |ms| std::time::Duration::from_millis(ms as u64),
        );
        assert_eq!(
            v5.timeout,
            Some(want_budget),
            "{}: the transport policy's timeout",
            label(row)
        );
        if posed.is_statusless() {
            statusless_rows += 1;
        } else {
            // One request per row (no SDK retry on these); so must v5.
            assert_eq!(v5.calls, row.fetch_calls, "{}: request count", label(row));
        }

        let mut diffs = diff_row(row, &posed, &v5);
        let (errors, warns) = diff_lines(row, &v5, &mut unported_warn_seen);
        match errors {
            Ok(n) => catch_lines += n,
            Err(e) => diffs.push(("lines", e)),
        }
        if let Err(e) = warns {
            diffs.push(("warn_lines", e));
        }
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
            if !expected(row, field) {
                unexpected.push(format!("{} {field}: {detail}", label(row)));
            }
            found.insert(key);
        }
    }

    eprintln!(
        "text_http_errors: {} rows; v4 refused {refused_v4}, v5 matched {refused_matched}; \
         {statusless_rows} status-less, {ok_rows} ok; {catch_lines} v4 ERROR line(s) diffed; \
         {} divergence(s) found",
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
    assert_eq!(statusless_rows, 40, "the posed status-less rows");
    // v4 answered (did not throw) on 55 rows: the openai-SDK streams, the
    // OpenRouter raw stream, Anthropic's / Ollama's zero-chunk streams and
    // Google's empty-body stream over the 2xx cases, and Google / Ollama /
    // OpenRouter-raw's sends over the JSON shapes — plus (P4.150 D2) all six
    // GOOGLE-only 2xx rows.
    assert_eq!(ok_rows, 61, "v4's answered rows");
    let unported_missing: Vec<&(&str, &str, &str)> = UNPORTED_PLUGIN_WARN_LINES
        .iter()
        .filter(|(p, m, msg)| {
            !unported_warn_seen.contains(&(p.to_string(), m.to_string(), msg.to_string()))
        })
        .collect();
    assert!(
        unported_missing.is_empty(),
        "UNPORTED_PLUGIN_WARN_LINES entries v4 no longer emits on any row (retire them): {unported_missing:?}"
    );
    let mut missing: Vec<String> = Vec::new();
    for (p, m, f, cases) in EXPECTED_DIVERGENCES {
        // "Every case" = every NON-2xx case that has a row for that
        // provider/mode (the GOOGLE-only rows have none elsewhere; P4.141's
        // status-less and 2xx rows carry no refusal side — the marker
        // predates them and means the P4.118 response rows).
        let listed: Vec<&str> = if *cases == EVERY_CASE {
            let cases_here: BTreeSet<&str> = rows
                .iter()
                .filter(|r| r.provider == *p && r.mode == *m)
                .filter(|r| r.status.is_some_and(|s| !(200..300).contains(&s)))
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
    // v4 logged 546 plugin ERROR lines across the corpus; this counts the
    // ones on rows whose lines MATCH (a mismatching row is a "lines"
    // divergence): every one but the six GOOGLE lines of the three pinned
    // content-type approximations (P4.141 — 210 before its units landed).
    assert_eq!(
        catch_lines, 540,
        "v4's plugin ERROR lines diffed and matched"
    );
}
