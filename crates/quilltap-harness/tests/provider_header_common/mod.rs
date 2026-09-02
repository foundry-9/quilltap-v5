//! Shared outbound-header capture for the provider wire differentials (P4.44
//! item 3, generalized by P4.47 (B) when the google-wire family gained the same
//! pin).
//!
//! `apply_auth` is `pub(crate)`, so a test cannot call it directly — and
//! reimplementing it here would pin the TEST's copy rather than production. So
//! the helper drives the production line instead: build a plain
//! [`CompletionParams`], call the public `execute_completion` with a transport
//! that stashes the request and answers a stub body, and read
//! `TransportRequest.headers`. That IS
//! `build_request → transport_headers → apply_auth`
//! (`completion_provider.rs:140-141`), which is where User-Agent / HTTP-Referer /
//! X-Title / the api key actually land — none of them are on `built.headers`
//! alone. The parse result is ignored; the request is captured before any parse.
//!
//! Headers never depend on the body or the stream flag, so ONE call per provider
//! serves every recorded row; callers memoize.
//!
//! `#[allow(dead_code)]` — each integration-test binary uses a different subset.
#![allow(dead_code)]

use std::collections::HashMap;

use quilltap_core::model::completion::{CompletionMessage, CompletionParams};
use quilltap_core::model::completion_provider::execute_completion;
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};

struct HeaderCapture {
    seen: std::sync::Mutex<Option<TransportRequest>>,
}

impl ProviderTransport for HeaderCapture {
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        *self.seen.lock().unwrap() = Some(request.clone());
        Box::pin(async move {
            Ok(TransportResponse {
                status: 200,
                body: b"{}".to_vec(),
            })
        })
    }
    fn execute_stream<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        Box::pin(async move {
            Err(TransportError {
                message: "unused".to_string(),
                status: None,
            })
        })
    }
}

/// The whole `TransportRequest` v5 would send for `provider` — headers AND the
/// post-`apply_auth` url. The url matters for a `query`-auth provider, where the
/// api key rides in the url instead of a header.
pub fn v5_transport_request(
    rt: &tokio::runtime::Runtime,
    provider: &str,
    api_key: &str,
) -> TransportRequest {
    let params = CompletionParams {
        messages: vec![CompletionMessage::user("hi")],
        model: "model".to_string(),
        temperature: Some(0.5),
        max_tokens: Some(1000),
        strict_max_tokens: false,
        top_p: None,
        cache_key: None,
        profile_parameters: None,
        attachments: Vec::new(),
        request_timeout_ms: None,
    };
    let cap = HeaderCapture {
        seen: std::sync::Mutex::new(None),
    };
    let policy = TransportPolicy::default();
    let _ = rt.block_on(execute_completion(
        &cap,
        provider,
        None,
        api_key,
        &params,
        &policy,
        "Quilltap/TEST",
        None,
        // P4.71: no container gateway on the header-pin path (`base_url` is
        // None here anyway, so the rewrite is a no-op either way).
        None,
    ));
    let seen = cap.seen.lock().unwrap().clone();
    seen.unwrap_or_else(|| panic!("{provider}: execute_completion made no transport call"))
}

/// v5's REAL outbound headers for `provider`, names lowercased (HTTP header
/// names are case-insensitive; both sides fold before comparing).
pub fn v5_headers(rt: &tokio::runtime::Runtime, provider: &str) -> HashMap<String, String> {
    v5_transport_request(rt, provider, "test-api-key")
        .headers
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect()
}

/// Fold the version-bearing User-Agent and the auth secret to placeholders, so
/// the pin is on the header NAME + scheme, not v4's build version or the key.
/// `name` is already lowercased.
pub fn normalize_header(name: &str, value: &str) -> String {
    match name {
        "user-agent" => {
            assert!(
                value.starts_with("Quilltap/"),
                "unexpected User-Agent {value:?}"
            );
            "Quilltap/<v>".to_string()
        }
        "authorization" => {
            assert!(
                value.starts_with("Bearer "),
                "unexpected Authorization {value:?}"
            );
            "Bearer <key>".to_string()
        }
        "x-api-key" | "x-goog-api-key" => "<key>".to_string(),
        _ => value.to_string(),
    }
}
