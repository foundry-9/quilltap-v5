//! P4.73 — the host transport behind `POST /api/v1/images` (JSON leg).
//!
//! v4's `importImageFromUrl` (`lib/images-v2.ts:269`) makes a bare
//! `fetch(url)` with no timeout, no headers and no redirect policy of its own,
//! then reads `response.ok`, `response.statusText`, the `content-type` header
//! and `response.arrayBuffer()`. Core has no HTTP, so the call rides this seam
//! — the `ReqwestLoraMetadata` precedent (P4.D138 unit 7), with two
//! differences that follow from what v4 does here:
//!
//! * **No timeout.** The LoRA lookup has v4's own explicit 10 s bound to
//!   mirror; this fetch has none, so imposing one would refuse imports v4
//!   completes.
//! * **Bytes, not text.** v4 reads `arrayBuffer()`, so the body crosses the
//!   seam as `Vec<u8>`; a lossy UTF-8 decode would corrupt every image.
//!
//! An `Err` here is v4's THROWN fetch (a network failure), which its route lets
//! escape to the middleware's catch. Every HTTP status — a 404 included — is an
//! `Ok` the core-side caller gates on with `response.ok`.

use std::future::Future;
use std::pin::Pin;

use quilltap_core::api::images::{ImageFetchResponse, ImageImportFetch};

/// The reqwest-backed [`ImageImportFetch`].
pub struct ReqwestImageImportFetch {
    client: reqwest::Client,
}

impl ReqwestImageImportFetch {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ReqwestImageImportFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageImportFetch for ReqwestImageImportFetch {
    fn get(
        &self,
        url: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ImageFetchResponse, String>> + Send + '_>> {
        let url = url.to_string();
        Box::pin(async move {
            let resp = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status().as_u16();
            // v4 reads `response.statusText`, which for a fetch Response is the
            // reason phrase (empty over HTTP/2, where the wire carries none).
            let status_text = resp
                .status()
                .canonical_reason()
                .unwrap_or_default()
                .to_string();
            // v4 `response.headers.get('content-type') || ''` — a header the
            // server omitted is the empty string, which fails the allow-list.
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            // v4 reads the body only AFTER its `ok` gate, but `arrayBuffer()`
            // itself throws on a broken stream — the same thrown-fetch arm.
            let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
            Ok(ImageFetchResponse {
                status,
                status_text,
                content_type,
                bytes,
            })
        })
    }
}
