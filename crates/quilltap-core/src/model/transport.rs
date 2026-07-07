//! The `ProviderTransport` IO boundary (W4.7d) — the seam the sans-IO
//! [`request_builder`](crate::model::request_builder) /
//! [`response_parse`](crate::model::response_parse) /
//! [`decoders`](crate::model::decoders) surfaces plug into. The trait + the pure
//! header/policy helpers are ALWAYS compiled (IO-free); the concrete `reqwest`
//! implementation lives behind the **non-default** `native-transport` cargo
//! feature, so the default core build stays IO-free (it is expected to migrate to
//! the CLI/host crate when one exists).
//!
//! ## v4 baseline (survey-confirmed): no timeout / abort / retry at the provider
//! ## tier
//!
//! v4's plugins carry NO timeout, abort, or retry of their own — the SDK defaults
//! apply (openai/anthropic SDK `maxRetries: 2`, ~10-min timeout; ollama's raw
//! fetch is unbounded; cancellation is only stop-iterating the generator). The v5
//! trait therefore carries timeout/abort/retry as HOST-policy knobs
//! ([`TransportPolicy`]) whose defaults match that observable behavior (a retry
//! re-sends the SAME built request). None of this is oracle-checkable — transport
//! is integration-smoke tier, no differential.
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

/// Host-policy knobs for a transport call. Defaults match the SDKs' observable
/// behavior (openai/anthropic SDK `maxRetries: 2`, ~10-min timeout). A retry
/// re-sends the SAME built request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportPolicy {
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            max_retries: 2,
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
    /// Issue a non-streaming request (retry/timeout per `policy`).
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>>;

    /// Issue a streaming request, returning a channel of byte frames the decoders
    /// consume. (The default impl is provided by the concrete transport; the trait
    /// keeps it explicit so a non-streaming-only host can `unimplemented!` it.)
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

        fn build(
            &self,
            request: &TransportRequest,
            policy: &TransportPolicy,
        ) -> reqwest::RequestBuilder {
            let method = reqwest::Method::from_bytes(request.method.as_bytes())
                .unwrap_or(reqwest::Method::POST);
            let mut rb = self
                .client
                .request(method, &request.url)
                .timeout(policy.timeout)
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
                let mut last: Option<TransportError> = None;
                for _ in 0..=policy.max_retries {
                    match self.build(request, policy).send().await {
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
                let resp =
                    self.build(request, policy)
                        .send()
                        .await
                        .map_err(|e| TransportError {
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

    #[test]
    fn default_policy_matches_sdk_observable_behavior() {
        let p = TransportPolicy::default();
        assert_eq!(p.max_retries, 2);
        assert_eq!(p.timeout, Duration::from_secs(600));
    }
}
