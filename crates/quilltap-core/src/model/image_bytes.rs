//! The image-download seam (v4 `ca22ec45`).
//!
//! Z.AI's Images API answers with URLs (valid roughly 30 days), not base64 —
//! but every Quilltap consumer (the chat handler, the avatar and background
//! jobs, `tools::generate_image`) reads only the base64 `data`. v4's fix
//! downloads each URL inside the provider, so the response the consumers see is
//! always usable. This module is the injected boundary that download crosses.
//!
//! It is deliberately NOT a variant on
//! [`WireTransport`](crate::model::wire::WireTransport): that seam's
//! [`WireResponse`](crate::model::wire::WireResponse) carries a `String` body
//! and no headers, and an image download needs raw bytes plus the
//! `content-type`. Widening the wire for one caller would touch every dialect;
//! a narrow trait alongside it touches none.
//!
//! Both dispositions match v4's bare `fetch(url)`:
//!   - `Ok(FetchedImageBytes)` — the exchange completed with **any** status.
//!     The caller inspects `status` itself (v4 checks `response.ok`).
//!   - `Err(message)` — a transport-level throw (DNS, connect, timeout),
//!     surfaced verbatim the way v4's rejected `fetch` promise is.

use std::future::Future;

/// One completed image download: the status, the raw `content-type` header as
/// sent (unparsed — the caller applies v4's sniff), and the body bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchedImageBytes {
    pub status: u16,
    /// `response.headers.get('content-type')` — `None` when absent.
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl FetchedImageBytes {
    /// v4 `response.ok` — a 2xx status.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The injected image-download transport. v4 issues a **bare** `fetch(url)`:
/// a GET with no headers at all — no auth, no user agent, no accept. The URL is
/// a short-lived signed link the provider just handed us, so adding headers
/// would be a divergence, not a hardening.
pub trait ImageBytesFetch: Send + Sync {
    fn fetch(&self, url: &str) -> impl Future<Output = Result<FetchedImageBytes, String>> + Send;
}

/// A deterministic [`ImageBytesFetch`] for the differentials and unit tests,
/// keyed by the exact URL. An unregistered URL is a surfaced error (a corpus
/// omission), never a silent empty download.
#[derive(Clone, Default)]
pub struct CannedImageBytes {
    responses: std::collections::HashMap<String, FetchedImageBytes>,
    throws: std::collections::HashMap<String, String>,
}

impl CannedImageBytes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a completed download for `url`.
    pub fn with_response(mut self, url: impl Into<String>, resp: FetchedImageBytes) -> Self {
        self.responses.insert(url.into(), resp);
        self
    }

    /// Register a transport-level throw for `url`.
    pub fn with_throw(mut self, url: impl Into<String>, message: impl Into<String>) -> Self {
        self.throws.insert(url.into(), message.into());
        self
    }
}

impl ImageBytesFetch for CannedImageBytes {
    async fn fetch(&self, url: &str) -> Result<FetchedImageBytes, String> {
        if let Some(msg) = self.throws.get(url) {
            return Err(msg.clone());
        }
        match self.responses.get(url) {
            Some(r) => Ok(r.clone()),
            None => Err(format!("CannedImageBytes: no canned download for `{url}`")),
        }
    }
}
