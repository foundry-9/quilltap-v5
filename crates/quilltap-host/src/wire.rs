//! The host HTTP client behind the core's W4.7f wire seams (P4.1a deliverable
//! 2): a `reqwest` [`WireTransport`] (async — image dialects, OpenAI
//! moderation) and a `reqwest::blocking` [`SyncWireTransport`] (the Serper
//! web-search boundary, whose tool path is sync).
//!
//! Both reproduce the seam contract exactly (see `quilltap_core::model::wire`):
//! a completed HTTP exchange — **any** status, 2xx or not — is `Ok(WireResponse)`
//! (the raw-fetch dialects inspect the status themselves); only a
//! transport-level failure (DNS, connect, timeout) is `Err(message)`. The v4
//! SDK-throw disposition for non-2xx responses is produced by the *dialects*
//! (they inspect the status), not manufactured here.
//!
//! ## The blocking client never runs on a tokio runtime thread
//!
//! `reqwest::blocking` panics when created **or driven** inside a tokio runtime
//! context (it block_on's its own inner runtime). The sync seam is called from
//! tool handlers that may be running on the host's runtime, so every
//! [`BlockingWireTransport::send`] executes on a spawned dedicated thread and
//! joins — the client itself is created lazily ON such a thread and reused
//! (`OnceLock`; `reqwest::blocking::Client` is `Send + Sync`).
//!
//! The core's feature-gated `ReqwestTransport` (the `ProviderTransport` impl
//! the streaming composer / `execute_completion` use) is NOT moved here — a
//! candidate migration once the CLI/host layering settles (noted in
//! `model/transport.rs`).

use std::sync::OnceLock;
use std::time::Duration;

use quilltap_core::model::image_bytes::{FetchedImageBytes, ImageBytesFetch};
use quilltap_core::model::wire::{SyncWireTransport, WireResponse, WireTransport};

/// Build a `WireResponse` from a completed `reqwest` response (any status).
fn wire_response(status: u16, status_text: String, body: String) -> WireResponse {
    WireResponse {
        status,
        status_text,
        body,
    }
}

/// The async host wire: one pooled [`reqwest::Client`].
pub struct ReqwestWireTransport {
    client: reqwest::Client,
    timeout: Option<Duration>,
}

impl ReqwestWireTransport {
    /// A transport with a fresh pooled client and no per-request timeout
    /// (v4's raw `fetch` has none at this tier — the dialects' callers own
    /// their own timing).
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout: None,
        }
    }

    /// Set a per-request timeout (host policy).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl Default for ReqwestWireTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl WireTransport for ReqwestWireTransport {
    async fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String> {
        let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut rb = self.client.request(m, url);
        if let Some(t) = self.timeout {
            rb = rb.timeout(t);
        }
        for (k, v) in headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        if !body.is_empty() {
            rb = rb.body(body.to_string());
        }
        let resp = rb.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let status_text = resp
            .status()
            .canonical_reason()
            .unwrap_or_default()
            .to_string();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(wire_response(status, status_text, text))
    }
}

/// The host image-download seam (v4 `ca22ec45`): a bare GET for a provider's
/// short-lived image URL, over the same pooled client as the async wire.
///
/// v4 issues `fetch(img.url)` with **no init object at all** — no auth header,
/// no user agent, no accept. The URL is a signed link the provider just handed
/// us, so nothing is added here either. As with the wire seam, a completed
/// exchange is `Ok` at ANY status (the caller checks `response.ok` itself) and
/// only a transport-level failure is `Err`.
pub struct ReqwestImageBytes {
    client: reqwest::Client,
    timeout: Option<Duration>,
}

impl ReqwestImageBytes {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout: None,
        }
    }

    /// Set a per-request timeout (host policy; v4's raw `fetch` has none).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl Default for ReqwestImageBytes {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageBytesFetch for ReqwestImageBytes {
    async fn fetch(&self, url: &str) -> Result<FetchedImageBytes, String> {
        let mut rb = self.client.get(url);
        if let Some(t) = self.timeout {
            rb = rb.timeout(t);
        }
        let resp = rb.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        Ok(FetchedImageBytes {
            status,
            content_type,
            bytes: bytes.to_vec(),
        })
    }
}

/// The lazily-created, thread-owned blocking client. Created on a dedicated
/// (non-runtime) thread the first time it is needed, then shared (`Client` is
/// `Send + Sync`); every request is still *driven* off-runtime.
fn blocking_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::blocking::Client::new)
}

/// Run `f` on a dedicated thread and join (the blocking client cannot run on a
/// tokio runtime thread). A panic on the worker surfaces as an `Err`.
pub(crate) fn run_off_runtime<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    std::thread::Builder::new()
        .name("qt-blocking-http".to_string())
        .spawn(f)
        .map_err(|e| format!("spawn failed: {e}"))?
        .join()
        .map_err(|_| "blocking HTTP worker panicked".to_string())
}

/// The sync host wire (Serper web search): `reqwest::blocking` on a dedicated
/// thread per call.
pub struct BlockingWireTransport {
    timeout: Option<Duration>,
}

impl BlockingWireTransport {
    /// No per-request timeout by default (v4's raw `fetch` has none here).
    pub fn new() -> Self {
        Self { timeout: None }
    }

    /// Set a per-request timeout (host policy).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl Default for BlockingWireTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncWireTransport for BlockingWireTransport {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String> {
        let method = method.to_string();
        let url = url.to_string();
        let headers = headers.to_vec();
        let body = body.to_string();
        let timeout = self.timeout;
        run_off_runtime(move || {
            let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
            let mut rb = blocking_client().request(m, &url);
            if let Some(t) = timeout {
                rb = rb.timeout(t);
            }
            for (k, v) in &headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            if !body.is_empty() {
                rb = rb.body(body);
            }
            let resp = rb.send().map_err(|e| e.to_string())?;
            let status = resp.status().as_u16();
            let status_text = resp
                .status()
                .canonical_reason()
                .unwrap_or_default()
                .to_string();
            let text = resp.text().map_err(|e| e.to_string())?;
            Ok(wire_response(status, status_text, text))
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A one-shot canned HTTP responder on a loopback socket.
    fn spawn_responder(status_line: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status_line}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let seen = String::from_utf8_lossy(&buf[..n]).to_string();
            stream.write_all(response.as_bytes()).unwrap();
            seen
        });
        (format!("http://{addr}"), handle)
    }

    /// A one-shot canned responder that also sets a `content-type`.
    fn spawn_responder_ct(
        status_line: &str,
        content_type: &str,
        body: &str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status_line}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let seen = String::from_utf8_lossy(&buf[..n]).to_string();
            stream.write_all(response.as_bytes()).unwrap();
            seen
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn async_wire_returns_non_2xx_as_ok() {
        let (base, handle) = spawn_responder("429 Too Many Requests", "{\"err\":true}");
        let t = ReqwestWireTransport::new();
        let resp = t
            .send(
                "POST",
                &format!("{base}/v1/thing"),
                &[("x-test".to_string(), "1".to_string())],
                "{\"a\":1}",
            )
            .await
            .expect("a completed exchange is Ok even when non-2xx");
        assert_eq!(resp.status, 429);
        assert_eq!(resp.status_text, "Too Many Requests");
        assert_eq!(resp.body, "{\"err\":true}");
        let seen = handle.join().unwrap();
        assert!(seen.starts_with("POST /v1/thing"));
        assert!(seen.contains("x-test: 1"));
        assert!(seen.ends_with("{\"a\":1}"));
    }

    /// The blocking wire works when called FROM a tokio runtime thread (the
    /// dedicated-thread guard — a naive blocking call here would panic).
    #[tokio::test]
    async fn blocking_wire_is_safe_on_a_runtime_thread() {
        let (base, _handle) = spawn_responder("200 OK", "{\"ok\":true}");
        let t = BlockingWireTransport::new();
        let resp = t.send("GET", &base, &[], "").expect("loopback GET");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "{\"ok\":true}");
    }

    /// The image-download seam: a completed exchange at ANY status is `Ok`, the
    /// content type comes back raw, and the request carries NO headers of ours.
    #[tokio::test]
    async fn image_bytes_seam_is_a_bare_get() {
        let (base, handle) = spawn_responder_ct("200 OK", "image/webp; charset=binary", "PNGBYTES");
        let f = ReqwestImageBytes::new();
        let got = f.fetch(&format!("{base}/x.png")).await.expect("download");
        assert_eq!(got.status, 200);
        assert_eq!(
            got.content_type.as_deref(),
            Some("image/webp; charset=binary")
        );
        assert_eq!(got.bytes, b"PNGBYTES".to_vec());
        let seen = handle.join().unwrap();
        assert!(
            seen.starts_with("GET /x.png"),
            "a bare fetch is a GET: {seen}"
        );
        for banned in ["authorization:", "Authorization:", "x-goog-api-key"] {
            assert!(
                !seen.contains(banned),
                "the download must send no auth: {seen}"
            );
        }
    }

    /// A non-2xx download is still `Ok` — the Z.AI dialect turns it into v4's
    /// `Failed to download Z.AI image: HTTP {status}` itself.
    #[tokio::test]
    async fn image_bytes_seam_returns_non_2xx_as_ok() {
        let (base, _h) = spawn_responder_ct("404 Not Found", "text/plain", "nope");
        let got = ReqwestImageBytes::new()
            .fetch(&format!("{base}/missing.png"))
            .await
            .expect("a completed exchange is Ok even at 404");
        assert_eq!(got.status, 404);
    }

    #[tokio::test]
    async fn async_wire_network_failure_is_err() {
        // A port nothing listens on: connection refused → transport Err.
        let t = ReqwestWireTransport::new().with_timeout(Duration::from_secs(2));
        let err = t
            .send("GET", "http://127.0.0.1:9/api", &[], "")
            .await
            .unwrap_err();
        assert!(!err.is_empty());
    }
}
