//! P4.D186 tier-1 item 5 — the held chain-complete frame AT THE WIRE.
//!
//! v4 `31436bae4` (bug 137) adds `heldUserTurn` to `encodeChainCompleteEvent`'s
//! `data`, and `finishHeldUserTurn` is the only one of its six callers that
//! passes it. v4's encoder writes `data: ${JSON.stringify({ chainComplete: true,
//! ...data })}\n\n`, so the key ORDER on the wire is the literal's order:
//! `chainComplete, reason, nextSpeakerId, chainDepth, paused, heldUserTurn`.
//!
//! The core-side serde pins (`chat_events.rs`) prove the payload's own bytes.
//! What they cannot see is the trip through `quilltap-web::events` — v5's chat
//! frames ride the one `Event` channel rather than v4's per-request stream, and
//! a transport that re-serialized through a map, filtered unknown keys, or
//! dropped an `Option` would be invisible to every core test. This test pins
//! both directions: the held frame arrives with all six keys in v4's order under
//! the `chatId` scope tag, and a NON-held chain-complete carries no
//! `heldUserTurn` key at all.
//!
//! Run:
//!   cargo test -p quilltap-web --test chain_complete_held_turn_wire

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::api::types::Event;
use quilltap_core::services::chat_events::{ChainCompletePayload, ChatEvent};
use serde_json::Value;

/// Read `data:` frames off the SSE stream until one satisfies `want`, returning
/// its RAW payload bytes (the byte order is the point) and its parsed form.
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
async fn the_held_chain_complete_reaches_api_events_with_v4s_six_keys() {
    let base = common::materialize_fixture_instance();
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // Subscribe FIRST — chat frames have no replay buffer.
    let events = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let host = state.host().expect("a booted host");
    // `finish_held_user_turn`'s frame, published exactly as the spine publishes it.
    let _ = host.core().event_sender().send(Event::chat(
        "chat-held",
        ChatEvent::chain_complete(ChainCompletePayload {
            reason: "paused".to_string(),
            next_speaker_id: None,
            chain_depth: 0,
            paused: Some(true),
            held_user_turn: Some(true),
        }),
    ));

    let (payload, frame) = next_matching(events, |v| v.get("chainComplete").is_some())
        .await
        .expect("a chainComplete frame within 10s — does /api/events filter chat frames?");

    assert_eq!(
        payload,
        r#"{"chatId":"chat-held","chainComplete":true,"reason":"paused","nextSpeakerId":null,"chainDepth":0,"paused":true,"heldUserTurn":true}"#,
        "v4's `{{ chainComplete: true, ...data }}` key order under the chatId scope tag"
    );
    assert_eq!(frame["heldUserTurn"], Value::Bool(true));
    assert_eq!(frame["paused"], Value::Bool(true));
    assert_eq!(frame["chainDepth"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chain_complete_that_was_not_held_carries_no_held_user_turn_key() {
    let base = common::materialize_fixture_instance();
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let events = client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let host = state.host().expect("a booted host");
    // The chain's own paused stop (P4.D160) — `paused: true`, no `heldUserTurn`.
    let _ = host.core().event_sender().send(Event::chat(
        "chat-chain",
        ChatEvent::chain_complete(ChainCompletePayload {
            reason: "paused".to_string(),
            next_speaker_id: None,
            chain_depth: 0,
            paused: Some(true),
            held_user_turn: None,
        }),
    ));

    let (payload, frame) = next_matching(events, |v| v.get("chainComplete").is_some())
        .await
        .expect("a chainComplete frame within 10s");

    assert_eq!(
        payload,
        r#"{"chatId":"chat-chain","chainComplete":true,"reason":"paused","nextSpeakerId":null,"chainDepth":0,"paused":true}"#,
        "the key must be ABSENT, not `false` — five of v4's six emit sites omit it"
    );
    assert!(
        frame.get("heldUserTurn").is_none(),
        "heldUserTurn leaked onto a frame that was not held: {frame}"
    );
}
