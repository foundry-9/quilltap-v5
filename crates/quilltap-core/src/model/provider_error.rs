//! The TEXT-side structured refusal (P4.118 — P4.D225's named gap): the value
//! v4's text-provider plugin THREW on a non-2xx response, rebuilt from the
//! status and the raw body so v4's refusal classifier sees the same fields.
//!
//! v5 has ONE HTTP boundary for every text provider (`ReqwestTransport`,
//! `model/transport.rs`), and it renders every non-2xx as `HTTP {status}:
//! {raw body}`. That message is what every other reader keeps reading — the
//! failover ladder after its first step, the WARN lines, the user-facing error
//! — and it does NOT change here (§S.5 of the P4.118 order). What v4 has and v5
//! lacked is the SDK error's STRUCTURE: an OpenAI/Azure `content_filter` or a
//! Z.AI `1301` rides on `APIError.code` / `APIError.error.code`, and v4's
//! classifier reads those codes FIRST (`refusal.ts` `collectCodes`), so a
//! benign-worded coded 400 reroutes on v4 and fell through v5's `/\b4\d\d\b/`
//! rule to no failover at all. [`text_http_refusal`] reconstructs that thrown
//! value as a [`RefusalError`] — the refusal SIDE the stream and completion
//! errors carry (`StreamError::refusal`, `CompletionError::refusal`).
//!
//! Per v4 plugin (the SDKs at `97b25fc53`, recorded against the REAL plugins
//! by `harness/oracle/providers/record-text-errors.mjs`):
//!
//! | provider | v4's thrown value |
//! |---|---|
//! | OPENAI, OPENAI_COMPATIBLE (Azure arrives here), DEEPSEEK, NANOGPT, Z_AI, GROK | `openai` 7.23.0 `APIError`: `makeStatusError` wraps a JSON body whose `.error == null` as `{error: body}`; `code` = that inner `error.code`, and `.error` IS the inner value, so both classifier slots read the same code; `makeMessage` renders the message |
//! | ANTHROPIC | `@anthropic-ai/sdk` 0.115.0 `APIError`: `.error` is the WHOLE body (no wrap), there is no `.code` — only `body.code` reaches the nested slot; `type` is never read |
//! | GOOGLE | `@google/genai` 1.52.0 `ApiError {name, status}` whose message is `JSON.stringify(body)` — or, for a non-JSON content type, of a synthesized `{error:{message, code, status: statusText}}` |
//! | OPENROUTER | the raw Chat Completions `fetch` path: `new Error(\`OpenRouter API error: ${status} - ${text}\`)` |
//! | OLLAMA | raw `fetch`: `new Error(\`Ollama API error: ${status} ${text}\`)` |
//!
//! Three recorded approximations (the transport keeps no headers — option (a)
//! of the order's §B, the zero-churn one):
//! - GOOGLE's content-type branch is decided by whether the body PARSES as
//!   JSON, and its synthesized `statusText` is the canonical reason phrase.
//!   A JSON body served as `text/plain` therefore renders differently (the
//!   `google_json_as_text_plain` row, pinned both ways in the family).
//! - GOOGLE's converse: a body that is NOT JSON served AS `application/json`.
//!   `@google/genai` calls `response.json()` on the content type and throws a
//!   bare `SyntaxError` (V8's parse message, no `status`, no `ApiError`) —
//!   which has no status digits, so v4's trigger reads `provider-error` and
//!   fails over; v5 sees a body that does not parse, synthesizes the
//!   `ApiError` with `status`, and its `HTTP 4xx:` bytes meet the 4xx rule.
//!   The `google_html_as_json` / `google_empty_body_as_json` rows, pinned
//!   both ways (the `97b25fc53` unification review).
//! - OPENROUTER is always the fetch path. v4 streams a no-tools request, and
//!   sends a no-image request, through `@openrouter/sdk`, whose errors are its
//!   own schema-validated classes; v5 never runs that SDK (the deliberate
//!   divergence `streaming_provider.rs` documents), and the SDK-path rows are
//!   pinned as the standing divergence.

use serde_json::{Map, Value};

use crate::model::provider_io::ProviderKind;
use crate::model::transport::TransportError;
use crate::pascal::js_value::json_stringify;
use crate::services::dangerous_content::refusal::{code_string, RefusalError};

impl TransportError {
    /// The raw response body of a non-2xx failure. The one production
    /// transport (`ReqwestTransport`, both its non-streaming and streaming
    /// arms) renders a non-2xx as `HTTP {status}: {text}` and sets `status`;
    /// a network/timeout failure has no status and no body. The format is
    /// pinned by `http_body_strips_the_transport_prefix` below.
    pub fn http_body(&self) -> Option<&str> {
        let status = self.status?;
        self.message.strip_prefix(&format!("HTTP {status}: "))
    }
}

/// The refusal side for a text HTTP failure: `Some` whenever the transport
/// error is a non-2xx carrying its body and the provider is known.
pub fn transport_error_refusal(provider: &str, error: &TransportError) -> Option<RefusalError> {
    let status = error.status?;
    let body = error.http_body()?;
    text_http_refusal(provider, status, body)
}

/// Rebuild the error v4's `provider` plugin throws for a non-2xx `status`
/// answered with `body` (see the module doc). `None` for an unknown provider.
pub fn text_http_refusal(provider: &str, status: u16, body: &str) -> Option<RefusalError> {
    let kind = ProviderKind::of(provider)?;
    Some(match kind {
        ProviderKind::OpenAi
        | ProviderKind::OpenAiCompatible
        | ProviderKind::DeepSeek
        | ProviderKind::NanoGpt
        | ProviderKind::ZAi
        | ProviderKind::Grok => openai_sdk_error(status, body),
        ProviderKind::Anthropic => anthropic_sdk_error(status, body),
        ProviderKind::Google => google_api_error(status, body),
        ProviderKind::OpenRouter => fetch_error(format!("OpenRouter API error: {status} - {body}")),
        ProviderKind::Ollama => fetch_error(format!("Ollama API error: {status} {body}")),
    })
}

/// The SDKs' `safeJSON(text)`: `JSON.parse`, `undefined` on a throw.
fn safe_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// JS truthiness of a parsed JSON value.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `value?.[key]` — only an object has own properties a JSON body can carry
/// (an array's or a primitive's `.message` / `.code` / `.error` is
/// `undefined`).
fn prop<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(Value::as_object).and_then(|m| m.get(key))
}

/// `APIError.makeMessage(status, error, message)` — identical in `openai`
/// 7.23.0 and `@anthropic-ai/sdk` 0.115.0:
///
/// ```text
/// const msg = error?.message ? (typeof error.message === 'string' ?
///   error.message : JSON.stringify(error.message))
///   : error ? JSON.stringify(error) : message;
/// if (status && msg) return `${status} ${msg}`;
/// if (status) return `${status} status code (no body)`;
/// if (msg) return msg;
/// return '(no status code or body)';
/// ```
///
/// `status` is `None` on the SDK's mid-stream path (P4.122 — `Stream.
/// fromSSEResponse` throws `new APIError(undefined, …)`), and never 0 where it
/// is set — it is an HTTP status.
fn make_message(status: Option<u16>, error: Option<&Value>, message: Option<&str>) -> String {
    let msg: Option<String> = match prop(error, "message").filter(|m| truthy(m)) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(json_stringify(other)),
        None => match error.filter(|e| truthy(e)) {
            Some(e) => Some(json_stringify(e)),
            None => message.map(str::to_string),
        },
    };
    match (status, msg.filter(|m| !m.is_empty())) {
        (Some(status), Some(m)) => format!("{status} {m}"),
        (Some(status), None) => format!("{status} status code (no body)"),
        (None, Some(m)) => m,
        (None, None) => "(no status code or body)".to_string(),
    }
}

/// `errJSON ? undefined : errText` — the text survives as the message only
/// when the parse produced nothing truthy.
fn err_message<'a>(err_json: Option<&Value>, body: &'a str) -> Option<&'a str> {
    match err_json {
        Some(v) if truthy(v) => None,
        _ => Some(body),
    }
}

/// `openai` 7.23.0: `makeStatusError` then `APIError.generate`, which hands
/// the constructor `errorResponse?.error`. The ONE home of the SDK's `{error:
/// body}` wrap rule — the image dialect's non-2xx helper reads it too (P4.122).
pub(crate) fn openai_sdk_error(status: u16, body: &str) -> RefusalError {
    let err_json = safe_json(body);
    let normalized = openai_sdk_normalized(err_json.as_ref());
    let inner = prop(normalized.as_ref(), "error");
    // `this.code = error?.code`; `this.error = error`, so v4's `collectCodes`
    // reads the SAME value through `record.code` and `record.error.code`.
    let code = prop(inner, "code").and_then(code_string);
    RefusalError {
        message: make_message(Some(status), inner, err_message(err_json.as_ref(), body)),
        code: code.clone(),
        nested_code: code,
        name: Some("Error".to_string()),
        provider_reason: None,
        status: Some(status),
    }
}

/// The error `openai` 7.23.0 THROWS from inside a 200 stream (P4.122):
/// `Stream.fromSSEResponse` answers an `event: error` frame with `new
/// APIError(undefined, data?.error ?? data, undefined, headers)` and any other
/// frame whose `data.error` is truthy with `new APIError(undefined,
/// data.error, …)`. `error` is that ALREADY-INNER value, passed as-is — no
/// `{error: body}` wrap on this path (a FLAT `event: error` payload is itself
/// the error object) — and `status` is `undefined`, so the message is v4's
/// bare `msg` (or `'(no status code or body)'`) and the side carries no
/// status. `this.code = error?.code` and `this.error = error`, so the
/// classifier's two code slots read the same value, as on the non-2xx path.
pub fn openai_stream_error(error: &Value) -> RefusalError {
    let code = prop(Some(error), "code").and_then(code_string);
    RefusalError {
        message: make_message(None, Some(error), None),
        code: code.clone(),
        nested_code: code,
        name: Some("Error".to_string()),
        provider_reason: None,
        status: None,
    }
}

/// `makeStatusError`'s normalization of the parsed body: `error && typeof
/// error === 'object' && error.error == null ? { error } : error` — an array
/// is an object too, and `[].error` is undefined.
fn openai_sdk_normalized(err_json: Option<&Value>) -> Option<Value> {
    match err_json {
        Some(v @ (Value::Object(_) | Value::Array(_)))
            if prop(Some(v), "error").is_none_or(Value::is_null) =>
        {
            let mut wrap = Map::new();
            wrap.insert("error".to_string(), v.clone());
            Some(Value::Object(wrap))
        }
        other => other.cloned(),
    }
}

/// The RAW `APIError.code` `openai` 7.23.0 sets for a non-2xx `body` — the
/// normalized inner error's `code`, any JSON type (`this.code =
/// error?.code`). The image plugins' moderation mappers read it raw (Z.AI
/// stringifies a number; OpenAI/xAI test for a string), so it is exposed
/// beside [`openai_sdk_error`], through the SAME wrap rule (P4.122 (B)).
pub(crate) fn openai_sdk_raw_code(body: &str) -> Option<Value> {
    let normalized = openai_sdk_normalized(safe_json(body).as_ref());
    prop(prop(normalized.as_ref(), "error"), "code").cloned()
}

/// `@anthropic-ai/sdk` 0.115.0: no normalization, `const error =
/// errorResponse` — the WHOLE body — and no `code` property at all.
fn anthropic_sdk_error(status: u16, body: &str) -> RefusalError {
    let err_json = safe_json(body);
    RefusalError {
        message: make_message(
            Some(status),
            err_json.as_ref(),
            err_message(err_json.as_ref(), body),
        ),
        code: None,
        // `record.error.code` = `body.code` — never `error.type` (do not map it).
        nested_code: prop(err_json.as_ref(), "code").and_then(code_string),
        name: Some("Error".to_string()),
        provider_reason: None,
        status: Some(status),
    }
}

/// `@google/genai` 1.52.0 `throwErrorIfNotOK`. The JSON branch is taken when
/// the body parses (the module doc's approximation of `content-type`).
fn google_api_error(status: u16, body: &str) -> RefusalError {
    let error_body = safe_json(body).unwrap_or_else(|| {
        serde_json::json!({
            "error": {
                "message": body,
                "code": status,
                "status": canonical_reason(status),
            }
        })
    });
    let message = json_stringify(&error_body);
    if (400..600).contains(&status) {
        RefusalError {
            message,
            name: Some("ApiError".to_string()),
            status: Some(status),
            ..Default::default()
        }
    } else {
        // `throw new Error(errorMessage)` — no status.
        fetch_error(message)
    }
}

/// A plugin's own `new Error(...)` over a raw `fetch` response: a message and
/// nothing else a classifier reads.
fn fetch_error(message: String) -> RefusalError {
    RefusalError {
        message,
        name: Some("Error".to_string()),
        ..Default::default()
    }
}

/// The `statusText` an HTTP/1.1 server sends with each 4xx/5xx status (RFC
/// 9110 §15 plus the registered extensions) — what Node's `fetch` hands
/// `@google/genai` as `response.statusText`. An unregistered status has no
/// canonical phrase, so the empty string.
fn canonical_reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        417 => "Expectation Failed",
        418 => "I'm a Teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Entity",
        423 => "Locked",
        424 => "Failed Dependency",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        506 => "Variant Also Negotiates",
        507 => "Insufficient Storage",
        508 => "Loop Detected",
        510 => "Not Extended",
        511 => "Network Authentication Required",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    //! One pin per corpus row class of `harness/oracle/fixtures/
    //! text-http-errors/cases.json`; the bytes are the recorded v4 values
    //! (`text-http-errors.recorded.ndjson`). The family diffs every row.
    use super::*;

    const OAI: &[&str] = &[
        "OPENAI",
        "OPENAI_COMPATIBLE",
        "DEEPSEEK",
        "NANOGPT",
        "Z_AI",
        "GROK",
    ];

    fn r(provider: &str, status: u16, body: &str) -> RefusalError {
        text_http_refusal(provider, status, body).expect("known provider")
    }

    #[test]
    fn http_body_strips_the_transport_prefix() {
        // The exact bytes `ReqwestTransport` writes at both its arms.
        let status = 400;
        let text = r#"{"error":{"code":"content_filter"}}"#;
        let e = TransportError {
            message: format!("HTTP {status}: {text}"),
            status: Some(status),
        };
        assert_eq!(e.http_body(), Some(text));
        let empty = TransportError {
            message: "HTTP 400: ".to_string(),
            status: Some(400),
        };
        assert_eq!(empty.http_body(), Some(""));
        // A network failure has no status, hence no body.
        let net = TransportError {
            message: "HTTP 400: looks like one".to_string(),
            status: None,
        };
        assert_eq!(net.http_body(), None);
        // A message in another shape is not guessed at.
        let other = TransportError {
            message: "provider did not send response headers within 5ms".to_string(),
            status: Some(400),
        };
        assert_eq!(other.http_body(), None);
    }

    #[test]
    fn openai_nested_code_fills_both_slots() {
        for p in OAI {
            let e = r(
                p,
                400,
                r#"{"error":{"message":"Filtered.","type":"invalid_request_error","param":null,"code":"content_filter"}}"#,
            );
            assert_eq!(e.message, "400 Filtered.");
            assert_eq!(e.code.as_deref(), Some("content_filter"));
            assert_eq!(e.nested_code.as_deref(), Some("content_filter"));
            assert_eq!(e.name.as_deref(), Some("Error"));
            assert_eq!(e.status, Some(400));
        }
    }

    #[test]
    fn openai_flat_body_is_wrapped_by_make_status_error() {
        let e = r(
            "OPENAI_COMPATIBLE",
            400,
            r#"{"code":"content_filter","message":"Flat body, benign words."}"#,
        );
        assert_eq!(e.message, "400 Flat body, benign words.");
        assert_eq!(e.code.as_deref(), Some("content_filter"));
        // `{error: null, …}` counts as "no error" too.
        let n = r(
            "OPENAI",
            400,
            r#"{"error":null,"message":"Null error, flat code.","code":"safety"}"#,
        );
        assert_eq!(n.message, "400 Null error, flat code.");
        assert_eq!(n.code.as_deref(), Some("safety"));
    }

    #[test]
    fn zai_numeric_and_string_1301() {
        let s = r("Z_AI", 400, r#"{"error":{"code":"1301","message":"x"}}"#);
        let n = r("Z_AI", 400, r#"{"error":{"code":1301,"message":"x"}}"#);
        assert_eq!(s.code.as_deref(), Some("1301"));
        assert_eq!(n.code.as_deref(), Some("1301"));
        assert_eq!(n.message, "400 x");
    }

    #[test]
    fn grok_string_error_is_json_quoted_and_codeless() {
        let e = r(
            "GROK",
            400,
            r#"{"code":"Client specified an invalid argument","error":"Content violates usage guidelines."}"#,
        );
        assert_eq!(e.message, r#"400 "Content violates usage guidelines.""#);
        assert_eq!(e.code, None);
        assert_eq!(e.nested_code, None);
    }

    #[test]
    fn openai_message_shapes() {
        // A non-string message is stringified.
        let e = r(
            "OPENAI",
            400,
            r#"{"error":{"message":{"detail":"structured"},"code":"content_filter"}}"#,
        );
        assert_eq!(e.message, r#"400 {"detail":"structured"}"#);
        // No message → the whole inner error, key order kept.
        let e = r(
            "OPENAI",
            400,
            r#"{"error":{"code":"content_filter","param":null}}"#,
        );
        assert_eq!(e.message, r#"400 {"code":"content_filter","param":null}"#);
        // An EMPTY message is falsy → the whole inner error.
        let e = r("OPENAI", 400, r#"{"error":{"message":"","code":"safety"}}"#);
        assert_eq!(e.message, r#"400 {"message":"","code":"safety"}"#);
        // `error: false` is not wrapped, is falsy, and carries no code.
        let e = r("OPENAI", 400, r#"{"error":false,"code":"content_filter"}"#);
        assert_eq!(e.message, "400 status code (no body)");
        assert_eq!(e.code, None);
    }

    #[test]
    fn openai_non_object_bodies() {
        // Non-JSON: the text is the message.
        assert_eq!(
            r("NANOGPT", 400, "Bad Request: upstream refused").message,
            "400 Bad Request: upstream refused"
        );
        // Empty: no body.
        assert_eq!(r("DEEPSEEK", 400, "").message, "400 status code (no body)");
        // `null` parses falsy, so the TEXT survives as the message.
        assert_eq!(r("OPENAI", 400, "null").message, "400 null");
        // A truthy primitive swallows the text and has no `.error`.
        assert_eq!(r("OPENAI", 400, "42").message, "400 status code (no body)");
        assert_eq!(
            r("OPENAI", 400, r#""oops""#).message,
            "400 status code (no body)"
        );
        // An array is wrapped; its elements' codes are never read.
        let a = r("OPENAI", 400, r#"[{"code":"content_filter"}]"#);
        assert_eq!(a.message, r#"400 [{"code":"content_filter"}]"#);
        assert_eq!(a.code, None);
    }

    #[test]
    fn js_number_rendering_in_a_stringified_body() {
        let e = r(
            "GOOGLE",
            400,
            r#"{"error":{"message":"n","code":"x","n":1.50,"big":1e21,"tiny":0.0000001}}"#,
        );
        assert_eq!(
            e.message,
            r#"{"error":{"message":"n","code":"x","n":1.5,"big":1e+21,"tiny":1e-7}}"#
        );
    }

    #[test]
    fn non_string_codes_through_code_string() {
        let zero = r(
            "OPENAI",
            422,
            r#"{"error":{"message":"Zero code.","code":0}}"#,
        );
        assert_eq!(zero.code.as_deref(), Some("0"));
        assert_eq!(zero.message, "422 Zero code.");
        let b = r(
            "OPENAI",
            400,
            r#"{"error":{"message":"Bool code.","code":true}}"#,
        );
        assert_eq!(b.code, None);
    }

    #[test]
    fn anthropic_whole_body_error_has_no_code() {
        let e = r(
            "ANTHROPIC",
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"Output blocked by content filtering policy"},"request_id":"req_011"}"#,
        );
        assert_eq!(
            e.message,
            r#"400 {"type":"error","error":{"type":"invalid_request_error","message":"Output blocked by content filtering policy"},"request_id":"req_011"}"#
        );
        assert_eq!(e.code, None);
        assert_eq!(e.nested_code, None);
        // Only a TOP-LEVEL `code` reaches the nested slot.
        let t = r(
            "ANTHROPIC",
            400,
            r#"{"type":"error","code":"content_filter","error":{"type":"invalid_request_error","message":"prompt is too long"}}"#,
        );
        assert_eq!(t.code, None);
        assert_eq!(t.nested_code.as_deref(), Some("content_filter"));
        // No wrap: a flat body's own message renders.
        let f = r(
            "ANTHROPIC",
            400,
            r#"{"code":"content_filter","message":"Flat."}"#,
        );
        assert_eq!(f.message, "400 Flat.");
        assert_eq!(r("ANTHROPIC", 400, "42").message, "400 42");
        assert_eq!(r("ANTHROPIC", 400, "").message, "400 status code (no body)");
    }

    #[test]
    fn google_stringifies_the_body_or_synthesizes_one() {
        let e = r(
            "GOOGLE",
            400,
            r#"{"error":{"code":400,"message":"Request contains an invalid argument.","status":"INVALID_ARGUMENT"}}"#,
        );
        assert_eq!(
            e.message,
            r#"{"error":{"code":400,"message":"Request contains an invalid argument.","status":"INVALID_ARGUMENT"}}"#
        );
        assert_eq!(e.name.as_deref(), Some("ApiError"));
        assert_eq!(e.status, Some(400));
        assert_eq!(e.code, None);
        assert_eq!(e.nested_code, None);
        assert_eq!(
            r("GOOGLE", 400, "").message,
            r#"{"error":{"message":"","code":400,"status":"Bad Request"}}"#
        );
        assert_eq!(
            r("GOOGLE", 403, "nope").message,
            r#"{"error":{"message":"nope","code":403,"status":"Forbidden"}}"#
        );
    }

    #[test]
    fn fetch_plugins_render_their_own_error() {
        let o = r("OPENROUTER", 403, r#"{"error":{"code":403}}"#);
        assert_eq!(
            o.message,
            r#"OpenRouter API error: 403 - {"error":{"code":403}}"#
        );
        assert_eq!(o.status, None);
        assert_eq!(o.code, None);
        assert_eq!(o.nested_code, None);
        assert_eq!(
            r("OPENROUTER", 400, "").message,
            "OpenRouter API error: 400 - "
        );
        let l = r("OLLAMA", 400, r#"{"error":"invalid options: bogus"}"#);
        assert_eq!(
            l.message,
            r#"Ollama API error: 400 {"error":"invalid options: bogus"}"#
        );
        assert_eq!(l.status, None);
        assert_eq!(r("OLLAMA", 400, "").message, "Ollama API error: 400 ");
    }

    /// P4.122 — `makeMessage`'s two NO-status branches, through the
    /// mid-stream builder (`APIError(undefined, error)`; bytes from
    /// `openai` 7.23.0 `core/error.js`, recorded in the stream corpora).
    #[test]
    fn openai_stream_error_has_no_status_and_no_wrap() {
        let e = openai_stream_error(&serde_json::json!({"code":"content_filter","message":"X"}));
        assert_eq!(e.message, "X");
        assert_eq!(e.code.as_deref(), Some("content_filter"));
        assert_eq!(e.nested_code.as_deref(), Some("content_filter"));
        assert_eq!(e.name.as_deref(), Some("Error"));
        assert_eq!(e.status, None);
        // No message → the whole error, stringified (msg only).
        let e = openai_stream_error(&serde_json::json!({"code":1301}));
        assert_eq!(e.message, r#"{"code":1301}"#);
        assert_eq!(e.code.as_deref(), Some("1301"));
        // Neither → `(no status code or body)`.
        for v in [
            serde_json::json!(null),
            serde_json::json!(false),
            serde_json::json!(""),
        ] {
            let e = openai_stream_error(&v);
            assert_eq!(e.message, "(no status code or body)");
            assert_eq!(e.code, None);
        }
        // A string error is JSON-quoted and codeless; NO wrap, so an inner
        // `error` key is just a key.
        assert_eq!(
            openai_stream_error(&serde_json::json!("boom")).message,
            r#""boom""#
        );
        let e = openai_stream_error(&serde_json::json!({"error":{"code":"content_filter"}}));
        assert_eq!(e.message, r#"{"error":{"code":"content_filter"}}"#);
        assert_eq!(e.code, None);
    }

    #[test]
    fn make_message_four_branches() {
        let m = serde_json::json!({"message":"m"});
        assert_eq!(make_message(Some(400), Some(&m), None), "400 m");
        assert_eq!(
            make_message(Some(400), None, None),
            "400 status code (no body)"
        );
        assert_eq!(make_message(None, Some(&m), None), "m");
        assert_eq!(make_message(None, None, None), "(no status code or body)");
        assert_eq!(make_message(None, None, Some("text")), "text");
    }

    #[test]
    fn unknown_provider_has_no_side() {
        assert!(text_http_refusal("NOT_A_PROVIDER", 400, "{}").is_none());
    }

    #[test]
    fn transport_error_refusal_needs_a_status_and_the_prefix() {
        let coded = TransportError {
            message: r#"HTTP 400: {"error":{"code":"content_filter","message":"m"}}"#.to_string(),
            status: Some(400),
        };
        let side = transport_error_refusal("OPENAI", &coded).expect("coded 400");
        assert_eq!(side.code.as_deref(), Some("content_filter"));
        let net = TransportError {
            message: "connection refused".to_string(),
            status: None,
        };
        assert!(transport_error_refusal("OPENAI", &net).is_none());
    }
}
