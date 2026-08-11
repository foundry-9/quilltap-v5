//! The CLI's one HTTP client: a minimal HTTP/1.1 POST to localhost, kept
//! dependency-free (the binary links no async runtime and no HTTP client).
//!
//! Used by the `db characters` archive/rehydrate/export proxies (P4.D66).
//! ⚠ `recall_replay_cmd.rs` still carries its OWN TcpStream POST + chunked
//! decoder (`http_post_json`) — a pre-existing near-duplicate this module was
//! wrongly recorded as replacing; consolidating it is a post-port cleanup
//! (the recall-replay Tier R cases pin its behavior, so the fold must re-run
//! them). A `None` body sends no `Content-Type` and no bytes, which is what
//! v4's bare `fetch(url, { method: 'POST' })` puts on the wire.

use std::io::{Read, Write};

/// POST and return `(status, body)` with the body decoded as UTF-8 (lossy) —
/// the JSON callers.
pub fn post(port: i64, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
    let (status, bytes) = post_bytes(port, path, body)?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

/// POST and return `(status, raw body bytes)`. The error string is the
/// transport reason (v4 surfaces Node's `err.message` in the same slot — a
/// documented wording-only divergence the differential normalizes).
pub fn post_bytes(port: i64, path: &str, body: Option<&str>) -> Result<(u16, Vec<u8>), String> {
    let addr = format!("localhost:{port}");
    let mut stream = std::net::TcpStream::connect(&addr).map_err(|e| e.to_string())?;
    let req = match body {
        Some(body) => format!(
            "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
        None => format!(
            "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    };
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    // The head is ASCII; the body may be arbitrary bytes (a `.qtap` export),
    // so it is split off positionally and never decoded here.
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split.saturating_sub(4)]).into_owned();
    let head = head.as_str();
    let mut payload_bytes = raw[split..].to_vec();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    // Minimal chunked-transfer decode (axum may chunk when no length is set).
    if head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.contains("chunked"))
    {
        let mut decoded: Vec<u8> = Vec::new();
        let mut rest = payload_bytes.as_slice();
        while let Some(nl) = rest.windows(2).position(|w| w == b"\r\n") {
            let size = std::str::from_utf8(&rest[..nl])
                .ok()
                .and_then(|s| usize::from_str_radix(s.trim(), 16).ok())
                .unwrap_or(0);
            if size == 0 {
                break;
            }
            let start = nl + 2;
            match rest.get(start..start + size) {
                Some(chunk) => decoded.extend_from_slice(chunk),
                None => break,
            }
            rest = rest.get(start + size + 2..).unwrap_or(&[]);
        }
        payload_bytes = decoded;
    }
    Ok((status, payload_bytes))
}
