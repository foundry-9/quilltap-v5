//! The sans-IO HTTP wire seam shared by the W4.7f provider dialects (image
//! generation, OpenAI moderation, Serper web search).
//!
//! In v4 each provider builds a request (SDK or raw `fetch`) and consumes the
//! response. The **request building** and **response parsing** are pure and are
//! ported per surface (`model::image_dialects`, the moderation wire, the Serper
//! wire); the actual byte transport is host-side (W4.7d owns the real HTTP
//! client — no HTTP crate in the core). This module is the injected boundary
//! between the two.
//!
//! Two response dispositions, matching v4:
//!   - `Ok(WireResponse)` — the HTTP call completed with a status + body. The
//!     **raw-fetch** providers (google, openrouter, serper, openai-moderation)
//!     inspect the status themselves, so a non-2xx is still `Ok` here.
//!   - `Err(message)` — a transport-level throw. This is how an **SDK** provider
//!     (openai / grok / z-ai image gen) surfaces an upstream error: the SDK
//!     converts a non-2xx response into a thrown `Error` whose message is the
//!     signal `is_image_moderation_error` matches. The dialect never reconstructs
//!     that message from the raw response — it is the recorded SDK throw.
//!
//! The trait is async and generic-consumed (the model-boundary precedent), so the
//! real provider impls stay generic over `T: WireTransport` with no boxing.

use std::collections::HashMap;
use std::future::Future;

/// One HTTP response the transport observed (v4 `Response`): the status code, the
/// reason phrase (`response.statusText`), and the raw body text. `json()` /
/// `text()` in v4 both derive from `body`. Most dialects read only `status` +
/// `body`; the Serper generic-error string also uses `status_text`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WireResponse {
    pub status: u16,
    pub status_text: String,
    pub body: String,
}

impl WireResponse {
    /// Construct a response with an empty reason phrase (the common case).
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            status_text: String::new(),
            body: body.into(),
        }
    }

    /// v4 `response.ok` — a 2xx status.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The injected HTTP transport (v4's `fetch` / the provider SDK's transport). A
/// `Err(String)` is a transport-level throw (an SDK converting a non-2xx to an
/// exception, or a network error); the message is surfaced verbatim.
pub trait WireTransport: Send + Sync {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> impl Future<Output = Result<WireResponse, String>> + Send;
}

/// The synchronous sibling of [`WireTransport`] — for the web-search boundary,
/// whose [`WebSearchProvider`](crate::tools::web_search::WebSearchProvider) seam
/// is sync (v4's `executeSearch` is awaited, but the v5 tool path is sync). Same
/// two dispositions; production wires a blocking HTTP call.
pub trait SyncWireTransport: Send + Sync {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String>;
}

/// A deterministic [`SyncWireTransport`] for the web-search differential, keyed by
/// the same `METHOD\nURL\nBODY` signature as [`CannedWireTransport`].
#[derive(Clone, Default)]
pub struct CannedSyncWireTransport {
    responses: HashMap<String, WireResponse>,
    throws: HashMap<String, String>,
}

impl CannedSyncWireTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_raw_response(mut self, key: impl Into<String>, resp: WireResponse) -> Self {
        self.responses.insert(key.into(), resp);
        self
    }

    pub fn with_raw_throw(mut self, key: impl Into<String>, message: impl Into<String>) -> Self {
        self.throws.insert(key.into(), message.into());
        self
    }
}

impl SyncWireTransport for CannedSyncWireTransport {
    fn send(
        &self,
        method: &str,
        url: &str,
        _headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String> {
        let key = wire_key(method, url, body);
        if let Some(msg) = self.throws.get(&key) {
            return Err(msg.clone());
        }
        match self.responses.get(&key) {
            Some(r) => Ok(r.clone()),
            None => Err(format!(
                "CannedSyncWireTransport: no canned response for `{key}`"
            )),
        }
    }
}

/// A deterministic [`WireTransport`] for the differentials: returns a fixed
/// [`WireResponse`] (or a transport throw) keyed by the exact request signature
/// `METHOD\nURL\nBODY`. The oracle records that signature and the wire payload it
/// fed the plugin, so a request-building divergence surfaces as a canned miss on
/// both sides. An unregistered request is a surfaced error (a corpus omission).
#[derive(Clone, Default)]
pub struct CannedWireTransport {
    responses: HashMap<String, WireResponse>,
    throws: HashMap<String, String>,
}

/// The canonical request signature a [`CannedWireTransport`] keys on.
pub fn wire_key(method: &str, url: &str, body: &str) -> String {
    format!("{method}\n{url}\n{body}")
}

impl CannedWireTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a wire response for the exact `(method, url, body)`.
    pub fn with_response(
        mut self,
        method: &str,
        url: &str,
        body: &str,
        resp: WireResponse,
    ) -> Self {
        self.responses.insert(wire_key(method, url, body), resp);
        self
    }

    /// Register a wire response under a RAW pre-computed key (`METHOD\nURL\nBODY`).
    pub fn with_raw_response(mut self, key: impl Into<String>, resp: WireResponse) -> Self {
        self.responses.insert(key.into(), resp);
        self
    }

    /// Register a transport throw (the recorded SDK error message) under a RAW key.
    pub fn with_raw_throw(mut self, key: impl Into<String>, message: impl Into<String>) -> Self {
        self.throws.insert(key.into(), message.into());
        self
    }
}

impl WireTransport for CannedWireTransport {
    async fn send(
        &self,
        method: &str,
        url: &str,
        _headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String> {
        let key = wire_key(method, url, body);
        if let Some(msg) = self.throws.get(&key) {
            return Err(msg.clone());
        }
        match self.responses.get(&key) {
            Some(r) => Ok(r.clone()),
            None => Err(format!(
                "CannedWireTransport: no canned response for `{key}`"
            )),
        }
    }
}
