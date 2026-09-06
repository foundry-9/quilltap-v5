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
//!
//! **`data:` URLs are served locally** (dogfood finding #113, 2026-09-06).
//! Node's `fetch` (undici) implements the Fetch Standard's *data: URL
//! processor*: a `data:` request never touches the network and answers a
//! synthetic `200 OK` whose `content-type` is the URL's serialized MIME type
//! and whose body is the decoded payload — v4 imports
//! `data:image/png;base64,…` and stores it under the basename
//! `png;base64,….webp` (the P4.76 unification review's measured shape).
//! reqwest has no such processor and refuses the scheme, which the route
//! turned into a flat 500. The arm below is that processor, transcribed from
//! the standard and pinned against six vectors measured on Node 24.

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
            // The Fetch Standard's scheme dispatch: a `data:` URL (the URL
            // parser lowercases the scheme, so `DATA:` is the same URL) is
            // processed locally and never sent. A malformed payload is
            // "failure" — a network error, i.e. the THROWN-fetch `Err` arm.
            if url.len() >= 5 && url[..5].eq_ignore_ascii_case("data:") {
                return process_data_url(&url[5..]).ok_or_else(|| "fetch failed".to_string());
            }
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

// ===========================================================================
// The data: URL processor (Fetch Standard §5.1 "data: URL processor", as
// undici implements it)
// ===========================================================================

/// `input` is everything after the `data:` scheme. Returns `None` on the
/// standard's *failure* (which `fetch` surfaces as a thrown network error).
///
/// Steps, one to one with the standard: split at the first `,` (no comma is
/// failure); the MIME type is the left part with leading/trailing ASCII
/// whitespace stripped; the body is the percent-decoded right part; a MIME
/// type ending in `;` + optional spaces + `base64` (ASCII case-insensitive)
/// forgiving-base64-decodes the body (failure is failure) and drops that
/// suffix; a MIME type starting with `;` is prefixed `text/plain`; an
/// unparseable MIME type becomes `text/plain;charset=US-ASCII`; the response
/// is `200 OK` with the serialized MIME type as its `content-type`.
fn process_data_url(input: &str) -> Option<ImageFetchResponse> {
    let comma = input.find(',')?;
    let mut mime_type = input[..comma].trim_matches(is_ascii_whitespace).to_string();
    let encoded_body = &input[comma + 1..];
    let mut body = percent_decode(encoded_body.as_bytes());
    if let Some(stripped) = strip_base64_suffix(&mime_type) {
        // Forgiving-base64 decode runs over the percent-DECODED body
        // interpreted as text (the standard isomorphic-decodes it first).
        let text = String::from_utf8_lossy(&body).into_owned();
        body = forgiving_base64_decode(&text)?;
        mime_type = stripped;
    }
    if mime_type.starts_with(';') {
        mime_type.insert_str(0, "text/plain");
    }
    let content_type = parse_and_serialize_mime(&mime_type)
        .unwrap_or_else(|| "text/plain;charset=US-ASCII".to_string());
    Some(ImageFetchResponse {
        status: 200,
        status_text: "OK".to_string(),
        content_type,
        bytes: body,
    })
}

/// The standard's ASCII whitespace: TAB, LF, FF, CR, SPACE.
fn is_ascii_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ')
}

/// "`;`, followed by zero or more U+0020 SPACE, followed by an ASCII
/// case-insensitive match for `base64`" at the END of the MIME type → the
/// MIME type with that suffix removed (and the preceding `;` — the standard
/// removes the last `mimeTypeBase64.length + 1` code points... after having
/// trimmed the trailing spaces it also strips).
fn strip_base64_suffix(mime_type: &str) -> Option<String> {
    let lower = mime_type.to_ascii_lowercase();
    let head = lower.strip_suffix("base64")?;
    let head = head.trim_end_matches(' ');
    let head = head.strip_suffix(';')?;
    let keep = head.len();
    Some(mime_type[..keep].to_string())
}

/// WHATWG percent-decode over bytes: `%XX` with two hex digits → the byte;
/// anything else passes through.
fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            let hi = (input[i + 1] as char).to_digit(16);
            let lo = (input[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// The Infra Standard's *forgiving-base64 decode*: remove ASCII whitespace;
/// if the length is a multiple of 4, strip one or two trailing `=`; a length
/// ≡ 1 (mod 4) is failure; any byte outside the base64 alphabet is failure;
/// then decode without padding.
fn forgiving_base64_decode(data: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let mut s: String = data.chars().filter(|c| !is_ascii_whitespace(*c)).collect();
    if s.len().is_multiple_of(4) {
        if s.ends_with("==") {
            s.truncate(s.len() - 2);
        } else if s.ends_with('=') {
            s.truncate(s.len() - 1);
        }
    }
    if s.len() % 4 == 1 {
        return None;
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return None;
    }
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(s)
        .ok()
}

/// A small MIME-type parse + serialize (the MIME Sniffing Standard's
/// algorithms, reduced to what a `data:` URL can carry): `type/subtype` in
/// HTTP token characters, lowercased; `;name=value` parameters with lowercased
/// names, quoted values unquoted, malformed parameters dropped; serialized
/// with `;` and no whitespace. `None` is the standard's failure.
fn parse_and_serialize_mime(input: &str) -> Option<String> {
    let input = input.trim_matches(is_ascii_whitespace);
    let (essence, rest) = match input.find(';') {
        Some(i) => (&input[..i], &input[i + 1..]),
        None => (input, ""),
    };
    let (ty, sub) = essence.split_once('/')?;
    let sub = sub.trim_end_matches(is_ascii_whitespace);
    let is_token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    };
    if !is_token(ty) || !is_token(sub) {
        return None;
    }
    let mut out = format!("{}/{}", ty.to_ascii_lowercase(), sub.to_ascii_lowercase());
    if rest.is_empty() {
        return Some(out);
    }
    for param in rest.split(';') {
        let param = param.trim_start_matches(is_ascii_whitespace);
        let Some((name, value)) = param.split_once('=') else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        let value = value.trim_end_matches(is_ascii_whitespace);
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if !is_token(&name) || value.is_empty() {
            continue;
        }
        if !value
            .bytes()
            .all(|b| b == b'\t' || (0x20..=0x7E).contains(&b) || b >= 0x80)
        {
            continue;
        }
        out.push(';');
        out.push_str(&name);
        out.push('=');
        out.push_str(value);
    }
    Some(out)
}

#[cfg(test)]
mod data_url_tests {
    //! The six vectors measured on Node 24.13.1 (`fetch(url)` → `status`,
    //! `statusText`, `headers.get('content-type')`, `arrayBuffer()`) on
    //! 2026-09-06 — dogfood finding #113.
    use super::*;

    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    async fn get(url: &str) -> Result<ImageFetchResponse, String> {
        ReqwestImageImportFetch::new().get(url).await
    }

    #[tokio::test]
    async fn a_base64_png_is_a_200_ok_with_the_mediatype_and_the_decoded_bytes() {
        let r = get(&format!("data:image/png;base64,{PNG_B64}"))
            .await
            .unwrap();
        assert_eq!((r.status, r.status_text.as_str()), (200, "OK"));
        assert_eq!(r.content_type, "image/png");
        assert_eq!(r.bytes.len(), 70);
        assert_eq!(&r.bytes[..4], &[137, 80, 78, 71]);
    }

    #[tokio::test]
    async fn parameters_survive_serialization_and_the_scheme_is_case_insensitive() {
        let r = get(&format!("data:image/png;charset=utf-8;base64,{PNG_B64}"))
            .await
            .unwrap();
        assert_eq!(r.content_type, "image/png;charset=utf-8");
        assert_eq!(r.bytes.len(), 70);
        let r = get(&format!("DATA:image/png;base64,{PNG_B64}"))
            .await
            .unwrap();
        assert_eq!(
            (r.status, r.content_type.as_str(), r.bytes.len()),
            (200, "image/png", 70)
        );
        let r = get(&format!("data:image/webp;base64,{PNG_B64}"))
            .await
            .unwrap();
        assert_eq!(r.content_type, "image/webp");
    }

    #[tokio::test]
    async fn a_missing_mediatype_defaults_and_the_body_percent_decodes() {
        let r = get("data:,hello%20world").await.unwrap();
        assert_eq!(r.content_type, "text/plain;charset=US-ASCII");
        assert_eq!(r.bytes, b"hello world");
    }

    #[tokio::test]
    async fn a_malformed_base64_payload_is_a_thrown_fetch() {
        let e = get("data:image/png;base64,!!!notbase64").await.unwrap_err();
        assert_eq!(e, "fetch failed");
        // No comma at all is the standard's failure too.
        assert!(get("data:image/png").await.is_err());
    }
}
