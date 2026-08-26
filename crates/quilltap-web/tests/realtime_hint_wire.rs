//! P4.D124 — the invalidation hint, end to end over the real transport.
//!
//! `quilltap-core`'s capture tests pin every publish point against the engine
//! broadcast. What THIS pins is everything downstream of it, which none of them
//! can see:
//!
//!   1. The host actually ARMS the bus at boot (`Host::start`). Nothing else
//!      fails if it does not — every publish just becomes the documented no-op,
//!      silently, forever.
//!   2. The `Event` reaches `GET /api/events` and is written as an SSE frame.
//!   3. The wire BYTES are §Shared contract §B.2's: `{"v":1,"topic":"jobs",
//!      "at":<ms>}`, key order included, with no `chatId`/`roomId`/`progressId`
//!      and no `id` on a collection-wide hint.
//!
//! ⚠ The bus is a process-GLOBAL armed by whichever host booted last, so this
//! file keeps to ONE server (integration tests are one binary per file, so no
//! sibling file can race it).
//!
//! Run:
//!   cargo test -p quilltap-web --test realtime_hint_wire

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};

/// Read SSE `data:` payloads off the stream until `want` matches one, or time
/// out. Returns the matching payload.
async fn next_matching(
    resp: reqwest::Response,
    want: impl Fn(&Value) -> bool,
) -> Option<(String, Value)> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        let chunk = match tokio::time::timeout(Duration::from_secs(10), stream.next()).await {
            Ok(Some(Ok(bytes))) => bytes,
            _ => break,
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let frame = buf[..end].to_string();
            buf.drain(..end + 2);
            for line in frame.lines() {
                if let Some(payload) = line.strip_prefix("data: ") {
                    if let Ok(v) = serde_json::from_str::<Value>(payload) {
                        if want(&v) {
                            return Some((payload.to_string(), v));
                        }
                    }
                }
            }
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn an_enqueue_puts_a_jobs_hint_on_the_event_stream() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // Subscribe FIRST, so the hint cannot land before anyone is listening.
    let events = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);

    // ── The SSE exposure survey, held mechanically (P4.D124 tier-2 item 6) ──
    //
    // v4's origin worry is WebSocket-specific because browsers do not apply
    // CORS to upgrades. v5's hints ride `GET /api/events`, and EventSource IS
    // CORS-governed — so the question is whether this router hands out
    // permission. It does not: `quilltap-web` installs no CORS layer at all
    // (only `TraceLayer` and `DefaultBodyLimit`), so the response carries no
    // `Access-Control-Allow-Origin` and a cross-origin EventSource is blocked
    // by the browser. Asserted rather than merely surveyed, so the day someone
    // adds a permissive layer this fails and the decision gets made
    // deliberately.
    for header in [
        "access-control-allow-origin",
        "access-control-allow-credentials",
    ] {
        assert!(
            events.headers().get(header).is_none(),
            "/api/events must not hand out CORS permission ({header})"
        );
    }

    // Give the stream a moment to actually attach to the broadcast.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Enqueue through the REAL route.
    let resp = client
        .post(format!("http://{addr}/api/v1/system/jobs"))
        .header("content-type", "application/json")
        .body(json!({ "type": "MEMORY_HOUSEKEEPING", "payload": {} }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "the enqueue itself");

    let (payload, hint) = next_matching(events, |v| v.get("topic").is_some())
        .await
        .expect("a realtime hint within 10s — is the bus armed at boot?");

    // §B.2, byte for byte: v, topic, at — in that order, with `id` absent on a
    // collection-wide hint and no scope tag of any kind.
    let at = hint["at"].as_i64().expect("`at` is a number");
    assert_eq!(
        payload,
        format!(r#"{{"v":1,"topic":"jobs","at":{at}}}"#),
        "the hint's wire bytes"
    );
    // …and `at` is a plausible server clock, not a zero or a placeholder.
    assert!(
        at > 1_700_000_000_000,
        "`at` should be a real ms clock: {at}"
    );

    // §B.5: the frame is discriminable as a hint by carrying BOTH keys.
    assert_eq!(hint["v"], 1);
    assert_eq!(hint["topic"], "jobs");
    for absent in ["id", "chatId", "roomId", "progressId", "type"] {
        assert!(
            hint.get(absent).is_none(),
            "a collection-wide hint must not carry `{absent}`"
        );
    }
}
