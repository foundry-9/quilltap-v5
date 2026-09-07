//! P4.9K0 tier-2 item 9 — the `/api/events` pass-through check for the new
//! `generatorProgress` Event family.
//!
//! **Survey finding, held mechanically:** `quilltap-web::events` filters
//! NOTHING. It serializes every [`Event`] the engine broadcasts, so the new
//! family needed no allow-list entry and no K0 fence in that file. That is a
//! fact about today's transport, not a guarantee — the day someone adds a kind
//! filter, this fails and the decision gets made deliberately (the
//! `realtime_hint_wire` precedent).
//!
//! What this pins, which no core-side capture test can see:
//!   1. A `GeneratorProgress` Event survives `GET /api/events` unaltered.
//!   2. Its wire shape is §B.5's — `type` / `progressId` / `generator` /
//!      `event`, with the inner `event` object's own key ORDER intact, which is
//!      the whole reason the SPA hooks can fold it exactly as v4's hooks fold a
//!      parsed SSE line.
//!
//! K0 defines no verb, so the frame is published straight onto the engine's
//! event sender — the same call the K1/K2 emitters will make.
//!
//! Run:
//!   cargo test -p quilltap-web --test generator_progress_events_passthrough

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::api::types::{Event, GeneratorKind};
use serde_json::{json, Value};

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
async fn a_generator_progress_event_reaches_api_events_unfiltered() {
    let base = common::materialize_fixture_instance();
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // Subscribe FIRST — this stream has no replay buffer for generator frames.
    let events = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let host = state.host().expect("a booted host");
    // Deliberately non-alphabetical keys: the order must survive the trip.
    let _ = host.core().event_sender().send(Event::generator_progress(
        "p-k0",
        GeneratorKind::AiImport,
        json!({"type": "step_complete", "step": "basics", "index": 2}),
    ));

    let (payload, frame) = next_matching(events, |v| v.get("progressId").is_some())
        .await
        .expect("a generatorProgress frame within 10s — does /api/events filter kinds?");

    assert_eq!(
        payload,
        r#"{"progressId":"p-k0","type":"generatorProgress","generator":"aiImport","event":{"type":"step_complete","step":"basics","index":2}}"#,
        "the §B.5 wire bytes"
    );
    assert_eq!(frame["type"], "generatorProgress");
    assert_eq!(frame["progressId"], "p-k0");
    assert_eq!(frame["generator"], "aiImport");
    assert_eq!(frame["event"]["step"], "basics");
}
