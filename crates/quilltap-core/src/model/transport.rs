//! The `ProviderTransport` IO boundary (W4.7d) — the seam the sans-IO
//! [`request_builder`](crate::model::request_builder) /
//! [`response_parse`](crate::model::response_parse) /
//! [`decoders`](crate::model::decoders) surfaces plug into. The trait + the pure
//! header/policy helpers are ALWAYS compiled (IO-free); the concrete `reqwest`
//! implementation lives behind the **non-default** `native-transport` cargo
//! feature, so the default core build stays IO-free (it is expected to migrate to
//! the CLI/host crate when one exists).
//!
//! ## v4 baseline (re-surveyed 2026-08-03 at `49769ec4`): bounded requests
//!
//! Until v4 `74ec93b5` the plugins carried NO timeout, abort, or retry of their
//! own — the SDK defaults applied (openai/anthropic SDK `maxRetries: 2`, ~10-min
//! timeout; ollama's raw fetch unbounded), and this module's policy defaults
//! transcribed exactly that. That default cost a measured **622,451 ms** memory
//! recap: a provider accepted the connection and never answered, and the turn sat
//! on "Recalling…" for the full ten minutes with nothing logged (llm_logs rows
//! are only written when a call *finishes*). v4 now bounds every provider request
//! at three levels; this module owns the third.
//!
//! **The nine-SDK collapse.** v4 applies its budget nine different ways (SDK
//! `timeout` options, `AbortSignal`, `httpOptions.timeout`, `timeoutMs`) because
//! it has nine plugin SDKs; v5 has ONE `reqwest` transport, so that matrix
//! collapses to this one [`TransportPolicy`] — **except the streaming
//! distinction, which is real and is carried, not collapsed**:
//!
//!   - **Non-streaming** ([`ProviderTransport::execute`]): the budget bounds the
//!     WHOLE exchange, body included. That is v4's non-streaming idiom
//!     (`buildRequestAbortSignal` on ollama's `fetch`, `timeoutMs` on the
//!     openrouter SDK, `httpOptions.timeout` on google) — the answer is one JSON
//!     blob, so waiting for all of it is the request.
//!   - **Streaming** ([`ProviderTransport::execute_stream`]): the budget bounds
//!     **time-to-response-headers only**; once the headers land the body streams
//!     unbounded. That is v4's raw-fetch idiom verbatim — an `AbortController`
//!     armed before `fetch` and `clearTimeout`-ed in a `finally` once the
//!     response resolves (openrouter `provider.ts:597`, ollama `:188`) — and the
//!     semantics the OpenAI/Anthropic SDK timeouts already have. A whole-exchange
//!     ceiling here would truncate a long generation mid-answer. (One v4 corner
//!     is deliberately NOT carried: google streaming applies an EXPLICIT budget
//!     as `httpOptions.timeout`, which bounds the whole request — v4 avoids
//!     truncation there only by never defaulting it. v5 is headers-only
//!     unconditionally, strictly safer, and no v5 streaming caller sets a
//!     budget today.)
//!
//! Retries follow v4's `buildSdkRequestOptions` contract: a caller-supplied
//! budget is a **ceiling on a single attempt**, so a request that carries one
//! never retries (three attempts at the requested budget would spend three times
//! what the caller agreed to). See [`TransportPolicy::with_request_budget`].
//!
//! None of this is oracle-checkable — a timeout is wall-clock behavior no NDJSON
//! corpus can observe (the canned providers never stall), so the proofs are
//! unit-tier (this module's tests) per the P4.15 falsifiability ruling.
//!
//! ## Per-provider construction (v4)
//!
//! Every provider sends `User-Agent: Quilltap/<version>` ([`quilltap_user_agent`]);
//! openrouter also sends `HTTP-Referer` (`BASE_URL` or `http://localhost:3000`) +
//! `X-Title: Quilltap` ([`transport_headers`]). The `baseUrl`
//! [`rewrite_localhost_url`](crate::provider_manifest::rewrite_localhost_url) is
//! applied at CONSTRUCTION time (ported in W4.7a; the host injects the gateway).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// v4 `getQuilltapUserAgent()` — `Quilltap/<version>` (the host injects the
/// version string; the core has no build-version constant of its own).
pub fn quilltap_user_agent(version: &str) -> String {
    format!("Quilltap/{version}")
}

/// The default `BASE_URL` v4 falls back to for openrouter's `HTTP-Referer`.
pub const DEFAULT_BASE_URL: &str = "http://localhost:3000";

/// Build the outgoing header set for a provider request: the manifest/auth
/// headers already on `built_headers`, plus `User-Agent` on EVERY provider and
/// (for `OPENROUTER`) `HTTP-Referer` + `X-Title: Quilltap`. `base_url_env` is v4's
/// `process.env.BASE_URL` (the host passes it; `None` → [`DEFAULT_BASE_URL`]).
pub fn transport_headers(
    provider: &str,
    built_headers: &[(String, String)],
    user_agent: &str,
    base_url_env: Option<&str>,
) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = built_headers.to_vec();
    headers.push(("User-Agent".to_string(), user_agent.to_string()));
    if provider == "OPENROUTER" {
        headers.push((
            "HTTP-Referer".to_string(),
            base_url_env.unwrap_or(DEFAULT_BASE_URL).to_string(),
        ));
        headers.push(("X-Title".to_string(), "Quilltap".to_string()));
    }
    headers
}

/// v4 plugin-utils 2.2.18 `DEFAULT_REQUEST_TIMEOUT_MS` — the budget for a request
/// whose caller expressed no preference. Half the SDK default: "long enough for a
/// slow non-streaming generation, short enough that a silently-stalled endpoint
/// fails in minutes rather than tens of minutes". v4 applies it through
/// `OpenAICompatibleProvider`'s client default (600 s → 300 s in `74ec93b5`),
/// which governs effectively every v4 provider.
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 300_000;

/// Host-policy knobs for a transport call. The defaults are v4's no-caller-budget
/// defaults after `74ec93b5`: [`DEFAULT_REQUEST_TIMEOUT_MS`] with the SDKs'
/// `maxRetries: 2` (plugin-utils `buildSdkClientOptions`'s uncapped arm). A retry
/// re-sends the SAME built request. What `timeout` *measures* differs by call
/// shape — whole exchange on [`ProviderTransport::execute`], time-to-headers on
/// [`ProviderTransport::execute_stream`] (module header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportPolicy {
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            max_retries: 2,
        }
    }
}

impl TransportPolicy {
    /// Apply a caller's per-request budget (v4
    /// [`LLMParams.requestTimeoutMs`][crate::model::completion::CompletionParams::request_timeout_ms],
    /// plugin-types 2.5.5) to this policy — the collapse of v4 plugin-utils'
    /// `buildSdkRequestOptions` + `buildSdkClientOptions` onto v5's one transport.
    ///
    /// A set, positive budget is a **ceiling on a single attempt**: it replaces
    /// the timeout AND drops `max_retries` to 0, because "three attempts at the
    /// requested budget would spend three times what the caller agreed to"
    /// (v4's own words). `None` or a non-positive value means "the caller named
    /// no budget" and leaves this policy untouched — v4's
    /// `typeof requested === 'number' && requested > 0` guard.
    /// Apply a PROVIDER-side default budget — the one a connection profile names
    /// for a provider that offers the setting (Ollama's
    /// `request_timeout_seconds`, P4.D83 / v4 `d89babc4`).
    ///
    /// Unlike [`with_request_budget`](Self::with_request_budget) this replaces
    /// the timeout ONLY: it is a better default, not a caller's ceiling, so the
    /// retry count stands. v4's shape exactly — the profile's number is the
    /// `defaultMs` argument to `resolveRequestTimeoutMs`, which a
    /// caller-supplied `requestTimeoutMs` still overrides. Compose in that order
    /// (`.with_provider_default_timeout(…).with_request_budget(…)`) and the
    /// precedence is v4's.
    #[must_use]
    pub fn with_provider_default_timeout(self, timeout_ms: Option<u64>) -> Self {
        match timeout_ms {
            Some(ms) if ms > 0 => Self {
                timeout: Duration::from_millis(ms),
                ..self
            },
            _ => self,
        }
    }

    #[must_use]
    pub fn with_request_budget(self, request_timeout_ms: Option<i64>) -> Self {
        match request_timeout_ms {
            Some(ms) if ms > 0 => Self {
                timeout: Duration::from_millis(ms as u64),
                max_retries: 0,
            },
            _ => self,
        }
    }
}

/// A request ready for the transport (the [`BuiltRequest`](crate::model::request_builder::BuiltRequest)
/// with headers finalized + the body serialized to bytes, and the api key + the
/// canonical provider id attached for header/error context).
#[derive(Clone, Debug)]
pub struct TransportRequest {
    pub provider: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The (plaintext) API key; the transport injects it per the manifest `auth`
    /// scheme. SYNTHETIC in tests.
    pub api_key: String,
}

/// A non-streaming transport response.
#[derive(Clone, Debug)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl TransportResponse {
    /// The body parsed as JSON (for [`response_parse`](crate::model::response_parse)).
    pub fn json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// A transport error (a network/timeout/abort failure, or a non-2xx status). The
/// message feeds [`handle_provider_error`](crate::services::llm_errors::handle_provider_error).
#[derive(Clone, Debug)]
pub struct TransportError {
    pub message: String,
    /// The HTTP status, when the failure was a non-2xx response.
    pub status: Option<u16>,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TransportError {}

/// A boxed future the `dyn`-injected transport returns.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One byte frame off a streaming response (fed to the W4.7b decoders).
pub type StreamBytes = Result<Vec<u8>, TransportError>;

/// The IO boundary. The host provides the concrete impl (the feature-gated
/// [`reqwest` impl](ReqwestTransport), or a CLI/host-crate one later); the
/// engine holds it as `dyn ProviderTransport`.
pub trait ProviderTransport: Send + Sync {
    /// Issue a non-streaming request (retry/timeout per `policy`). `policy.timeout`
    /// bounds the WHOLE exchange here — the answer is one body, so waiting for all
    /// of it is the request (module header).
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>>;

    /// Issue a streaming request, returning a channel of byte frames the decoders
    /// consume. (The default impl is provided by the concrete transport; the trait
    /// keeps it explicit so a non-streaming-only host can `unimplemented!` it.)
    ///
    /// `policy.timeout` bounds **time-to-response-headers only**; the body then
    /// streams unbounded (v4's `clearTimeout`-in-`finally` idiom — module header).
    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>>;
}

#[cfg(feature = "native-transport")]
pub use native::ReqwestTransport;

#[cfg(feature = "native-transport")]
mod native {
    //! The concrete `reqwest` transport (behind `native-transport`). Integration-
    //! smoke tier — no differential (the wire builders/decoders/parsers ARE
    //! differential-checked; this only moves bytes).

    use super::*;
    use reqwest::Client;

    /// A `reqwest`-backed [`ProviderTransport`]. Holds one pooled [`Client`].
    pub struct ReqwestTransport {
        client: Client,
    }

    impl ReqwestTransport {
        /// A transport with a fresh pooled client.
        pub fn new() -> Self {
            Self {
                client: Client::new(),
            }
        }

        /// The request WITHOUT any deadline attached — each caller applies the
        /// deadline its own shape needs (`execute` bounds the whole exchange with
        /// reqwest's own `.timeout()`; `execute_stream` bounds only the `send()`).
        fn build(&self, request: &TransportRequest) -> reqwest::RequestBuilder {
            let method = reqwest::Method::from_bytes(request.method.as_bytes())
                .unwrap_or(reqwest::Method::POST);
            let mut rb = self
                .client
                .request(method, &request.url)
                .body(request.body.clone());
            for (k, v) in &request.headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            rb
        }
    }

    impl Default for ReqwestTransport {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ProviderTransport for ReqwestTransport {
        fn execute<'a>(
            &'a self,
            request: &'a TransportRequest,
            policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
            Box::pin(async move {
                // A retry re-sends the SAME built request (v4 SDK maxRetries).
                // `max_retries` is 0 whenever the caller set a per-request budget
                // (`TransportPolicy::with_request_budget`) — a ceiling must not be
                // spent three times over.
                let mut last: Option<TransportError> = None;
                for _ in 0..=policy.max_retries {
                    // Non-streaming: reqwest's own `.timeout()` bounds the whole
                    // exchange, body included — v4's `buildRequestAbortSignal`.
                    match self.build(request).timeout(policy.timeout).send().await {
                        Ok(resp) => {
                            let status = resp.status().as_u16();
                            let body = resp.bytes().await.map(|b| b.to_vec()).unwrap_or_default();
                            if (200..300).contains(&status) {
                                return Ok(TransportResponse { status, body });
                            }
                            let text = String::from_utf8_lossy(&body).to_string();
                            last = Some(TransportError {
                                message: format!("HTTP {status}: {text}"),
                                status: Some(status),
                            });
                        }
                        Err(e) => {
                            last = Some(TransportError {
                                message: e.to_string(),
                                status: None,
                            });
                        }
                    }
                }
                Err(last.unwrap_or(TransportError {
                    message: "transport failed".to_string(),
                    status: None,
                }))
            })
        }

        fn execute_stream<'a>(
            &'a self,
            request: &'a TransportRequest,
            policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>>
        {
            Box::pin(async move {
                // Streaming: bound how long the provider may take to *start*
                // answering, not how long it takes to finish. `send()` resolves at
                // the response headers, so timing out around it — and NOT arming
                // reqwest's whole-exchange `.timeout()` — is v4's raw-fetch idiom
                // (`AbortController` armed before `fetch`, `clearTimeout` in the
                // `finally` once the headers land). The body below streams
                // unbounded: a long generation must never be cut off mid-answer.
                let sent = tokio::time::timeout(policy.timeout, self.build(request).send())
                    .await
                    .map_err(|_| TransportError {
                        message: format!(
                            "provider did not send response headers within {}ms",
                            policy.timeout.as_millis()
                        ),
                        status: None,
                    })?;
                let resp = sent.map_err(|e| TransportError {
                    message: e.to_string(),
                    status: None,
                })?;
                let status = resp.status().as_u16();
                if !(200..300).contains(&status) {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(TransportError {
                        message: format!("HTTP {status}: {text}"),
                        status: Some(status),
                    });
                }
                let (tx, rx) = tokio::sync::mpsc::channel::<StreamBytes>(32);
                let mut stream = resp;
                tokio::spawn(async move {
                    loop {
                        match stream.chunk().await {
                            Ok(Some(bytes)) => {
                                if tx.send(Ok(bytes.to_vec())).await.is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx
                                    .send(Err(TransportError {
                                        message: e.to_string(),
                                        status: None,
                                    }))
                                    .await;
                                break;
                            }
                        }
                    }
                });
                Ok(rx)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_shape() {
        assert_eq!(quilltap_user_agent("0.0.128"), "Quilltap/0.0.128");
    }

    #[test]
    fn openrouter_gets_referer_and_title() {
        let base = vec![("content-type".to_string(), "application/json".to_string())];
        let h = transport_headers(
            "OPENROUTER",
            &base,
            "Quilltap/1",
            Some("https://app.example"),
        );
        assert!(h
            .iter()
            .any(|(k, v)| k == "User-Agent" && v == "Quilltap/1"));
        assert!(h
            .iter()
            .any(|(k, v)| k == "HTTP-Referer" && v == "https://app.example"));
        assert!(h.iter().any(|(k, v)| k == "X-Title" && v == "Quilltap"));
    }

    #[test]
    fn openrouter_referer_defaults_to_localhost() {
        let h = transport_headers("OPENROUTER", &[], "Quilltap/1", None);
        assert!(h
            .iter()
            .any(|(k, v)| k == "HTTP-Referer" && v == DEFAULT_BASE_URL));
    }

    #[test]
    fn non_openrouter_only_gets_user_agent() {
        let h = transport_headers("ANTHROPIC", &[], "Quilltap/1", None);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0, "User-Agent");
    }

    /// P4.D42 / v4 `74ec93b5` + plugin-utils 2.2.18: the no-caller-budget default
    /// dropped from the SDKs' ten minutes to five (`DEFAULT_REQUEST_TIMEOUT_MS`),
    /// keeping `maxRetries: 2` (`buildSdkClientOptions`'s uncapped arm).
    #[test]
    fn default_policy_matches_v4s_no_caller_budget_default() {
        let p = TransportPolicy::default();
        assert_eq!(p.max_retries, 2);
        assert_eq!(p.timeout, Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS));
        assert_eq!(p.timeout, Duration::from_secs(300));
    }

    /// v4 `buildSdkRequestOptions`: a caller-supplied budget is a ceiling on ONE
    /// attempt, so it both replaces the timeout and disables retries.
    #[test]
    fn a_request_budget_caps_the_timeout_and_forbids_retrying_past_it() {
        let p = TransportPolicy::default().with_request_budget(Some(40_000));
        assert_eq!(p.timeout, Duration::from_millis(40_000));
        assert_eq!(
            p.max_retries, 0,
            "three attempts at the requested budget would spend 3x what the caller agreed to"
        );
    }

    /// v4's `typeof requested === 'number' && requested > 0` guard: absent or
    /// non-positive means "no preference" and leaves the policy alone.
    #[test]
    fn an_absent_or_nonpositive_budget_leaves_the_policy_untouched() {
        let base = TransportPolicy::default();
        for none in [None, Some(0), Some(-1)] {
            let p = base.with_request_budget(none);
            assert_eq!(p, base, "budget {none:?} must not change the policy");
        }
    }

    /// The two call shapes read `timeout` differently (module header), so the
    /// wall-clock proofs need a real socket. Feature-gated with the concrete
    /// transport; `cargo test --workspace` compiles core with `native-transport`
    /// (quilltap-host requires it, and cargo unifies features across the
    /// invocation), so these run in the ordinary gate.
    #[cfg(feature = "native-transport")]
    mod native_deadlines {
        use super::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        /// What the fake provider socket does once a request arrives.
        #[derive(Clone, Copy)]
        enum Behavior {
            /// Accept the connection and never answer — the `74ec93b5` incident.
            NeverAnswers,
            /// Send headers at once, then dribble the body out over `chunks *
            /// gap_ms`, well past the policy budget.
            SlowButFlowing { chunks: usize, gap_ms: u64 },
            /// Answer 500 immediately, counting how many times we were asked.
            AlwaysFails,
        }

        /// A one-shot-per-connection HTTP server on an ephemeral port. Returns the
        /// base URL and the connection counter.
        async fn spawn(behavior: Behavior) -> (String, Arc<AtomicUsize>) {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let addr = listener.local_addr().expect("addr");
            let hits = Arc::new(AtomicUsize::new(0));
            let counter = hits.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::spawn(async move {
                        // Drain whatever the client sends; we never need to parse
                        // it, only to let the request finish arriving.
                        let mut buf = [0u8; 4096];
                        let _ = sock.read(&mut buf).await;
                        match behavior {
                            Behavior::NeverAnswers => {
                                // Hold the socket open, silently, forever.
                                std::future::pending::<()>().await;
                            }
                            Behavior::SlowButFlowing { chunks, gap_ms } => {
                                let _ = sock
                                    .write_all(
                                        b"HTTP/1.1 200 OK\r\n\
                                          Content-Type: text/event-stream\r\n\
                                          Transfer-Encoding: chunked\r\n\r\n",
                                    )
                                    .await;
                                let _ = sock.flush().await;
                                for i in 0..chunks {
                                    tokio::time::sleep(Duration::from_millis(gap_ms)).await;
                                    let frame = format!("data: {i}\n\n");
                                    let _ = sock
                                        .write_all(
                                            format!("{:x}\r\n{frame}\r\n", frame.len()).as_bytes(),
                                        )
                                        .await;
                                    let _ = sock.flush().await;
                                }
                                let _ = sock.write_all(b"0\r\n\r\n").await;
                                let _ = sock.flush().await;
                            }
                            Behavior::AlwaysFails => {
                                let _ = sock
                                    .write_all(
                                        b"HTTP/1.1 500 Internal Server Error\r\n\
                                          Content-Length: 4\r\n\r\nboom",
                                    )
                                    .await;
                                let _ = sock.flush().await;
                            }
                        }
                    });
                }
            });
            (format!("http://{addr}/v1/chat"), hits)
        }

        fn request(url: String) -> TransportRequest {
            TransportRequest {
                provider: "DEEPSEEK".to_string(),
                method: "POST".to_string(),
                url,
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                body: b"{}".to_vec(),
                api_key: "synthetic-key".to_string(),
            }
        }

        /// The incident itself, on the non-streaming path: a provider that accepts
        /// the connection and never answers must fail inside the budget rather
        /// than hold the caller for the SDK default.
        #[tokio::test]
        async fn non_streaming_abandons_a_provider_that_never_answers() {
            let (url, _) = spawn(Behavior::NeverAnswers).await;
            let transport = ReqwestTransport::new();
            let policy = TransportPolicy {
                timeout: Duration::from_millis(150),
                max_retries: 0,
            };
            let started = std::time::Instant::now();
            let err = transport
                .execute(&request(url), &policy)
                .await
                .expect_err("a silent provider must not resolve");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the budget must fire, not the SDK default (took {:?})",
                started.elapsed()
            );
            assert!(err.status.is_none(), "a deadline is not an HTTP status");
        }

        /// D3, half one: a stream whose provider never sends headers is aborted at
        /// the budget — the streaming path is bounded, not unbounded.
        #[tokio::test]
        async fn streaming_aborts_a_provider_that_never_sends_headers() {
            let (url, _) = spawn(Behavior::NeverAnswers).await;
            let transport = ReqwestTransport::new();
            let policy = TransportPolicy {
                timeout: Duration::from_millis(150),
                max_retries: 0,
            };
            let started = std::time::Instant::now();
            let err = transport
                .execute_stream(&request(url), &policy)
                .await
                .expect_err("a silent provider must not open a stream");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "took {:?}",
                started.elapsed()
            );
            assert!(
                err.message.contains("response headers"),
                "the abort must name what it measured: {}",
                err.message
            );
        }

        /// D3, half two — the half that makes the first half safe: a provider that
        /// answers promptly and then generates *slowly* must stream to completion,
        /// even though the whole exchange runs far past the budget. Arming
        /// reqwest's whole-request `.timeout()` here (the pre-P4.D42 shape) cuts
        /// this body off mid-answer.
        #[tokio::test]
        async fn streaming_does_not_truncate_a_slow_but_flowing_body() {
            let (url, _) = spawn(Behavior::SlowButFlowing {
                chunks: 6,
                gap_ms: 60,
            })
            .await;
            let transport = ReqwestTransport::new();
            // The whole body takes ~360ms to arrive; the budget is 150ms and must
            // apply to the headers only.
            let policy = TransportPolicy {
                timeout: Duration::from_millis(150),
                max_retries: 0,
            };
            let mut rx = transport
                .execute_stream(&request(url), &policy)
                .await
                .expect("headers arrive at once");
            let mut body = String::new();
            while let Some(frame) = rx.recv().await {
                let bytes = frame.expect("a flowing body must not error");
                body.push_str(&String::from_utf8_lossy(&bytes));
            }
            for i in 0..6 {
                assert!(
                    body.contains(&format!("data: {i}\n\n")),
                    "frame {i} was truncated away; got {body:?}"
                );
            }
        }

        /// v4 `buildSdkRequestOptions`: a budget-bearing call is ONE attempt. The
        /// same failing provider is asked three times under the default policy and
        /// exactly once under a budget.
        #[tokio::test]
        async fn a_budget_bearing_call_never_retries() {
            let (url, hits) = spawn(Behavior::AlwaysFails).await;
            let transport = ReqwestTransport::new();

            let uncapped = TransportPolicy {
                timeout: Duration::from_secs(5),
                max_retries: 2,
            };
            let _ = transport.execute(&request(url.clone()), &uncapped).await;
            assert_eq!(
                hits.load(Ordering::SeqCst),
                3,
                "the uncapped policy retries twice"
            );

            hits.store(0, Ordering::SeqCst);
            let capped = TransportPolicy::default().with_request_budget(Some(5_000));
            let _ = transport.execute(&request(url), &capped).await;
            assert_eq!(
                hits.load(Ordering::SeqCst),
                1,
                "a caller's ceiling must be spent once, not three times"
            );
        }
    }
}
