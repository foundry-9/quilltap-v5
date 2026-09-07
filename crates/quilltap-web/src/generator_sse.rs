//! The character-generator SSE re-framer (`p4.9k`, P4.9K0, §B.1).
//!
//! v4 streams each generator as its own HTTP response of `data: <JSON>\n\n`
//! frames. All three handlers are the same nine lines —
//! `app/api/v1/characters/[id]/handlers/post.ts:113-140` (the optimizer),
//! `app/api/v1/characters/handlers/post.ts:548-575` (the wizard),
//! `app/api/v1/system/tools/route.ts:1223-1252` (the AI import):
//!
//! ```text
//! controller.enqueue(encoder.encode(`data: ${JSON.stringify(event)}\n\n`))  // per event
//! controller.close()                                                        // after the runner resolves
//! headers: Content-Type: text/event-stream, Cache-Control: no-cache, Connection: keep-alive
//! ```
//!
//! No `event:` names, no `id:` field, no retry, no comment keep-alives — this
//! stream is deliberately NOT `/api/events` (which prefixes an `id:` and sends
//! `: keep-alive`), because these bytes must match v4's byte for byte.
//!
//! ## The mechanism
//!
//! The generator runs behind the dispatch boundary and narrates through
//! [`GeneratorProgressEmitter`](quilltap_core::services::generator_progress),
//! which publishes each v4 progress object as an
//! `Event::GeneratorProgress` on the engine's ONE broadcast. This function
//! turns "a subscription on one `progressId`" plus "a dispatch future" back
//! into v4's stream:
//!
//! 1. **Subscribe FIRST**, before the dispatch future is polled at all. A Rust
//!    future does nothing until awaited, so passing it in un-polled is what
//!    makes this safe — a frame emitted synchronously by the runner cannot
//!    predate the subscription. Swapping the two loses frames, which
//!    [`tests::a_frame_emitted_before_the_dispatch_resolves_is_not_lost`] pins.
//! 2. Race the dispatch against the first matching frame. If the dispatch
//!    resolves with an ERROR and no frame has been seen, answer the dispatch
//!    envelope (v4's route-level `badRequest` / Zod refusal answers JSON with a
//!    status, never a stream — §B.1). Otherwise the stream has begun and the
//!    outcome rides the `terminal` payload the response carries.
//! 3. Forward every `GeneratorProgress` whose `progressId` matches, then close
//!    after DRAINING what was emitted before the dispatch resolved.
//!
//! The forwarded bytes are `event`'s alone — the payload the generator lane
//! built, serialized straight back out with `preserve_order` intact. The
//! envelope's `type` / `progressId` / `generator` are v5 transport and never
//! reach this wire.

use std::future::Future;

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::types::{Event, EventPayload};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;

/// v4's three response headers, in v4's order.
const SSE_HEADERS: [(&str, &str); 3] = [
    ("content-type", "text/event-stream"),
    ("cache-control", "no-cache"),
    ("connection", "keep-alive"),
];

/// One `data: …` frame, v4's encoding exactly.
fn frame(event: &serde_json::Value) -> Vec<u8> {
    format!("data: {event}\n\n").into_bytes()
}

/// Pull the inner v4 progress object out of an [`Event`] iff it is a
/// generator frame for `progress_id`.
fn matching<'a>(ev: &'a Event, progress_id: &str) -> Option<&'a serde_json::Value> {
    if ev.progress_id.as_deref() != Some(progress_id) {
        return None;
    }
    match &ev.payload {
        EventPayload::GeneratorProgress(p) => Some(&p.event),
        _ => None,
    }
}

/// Re-frame one generator run as v4's SSE stream. `dispatch` MUST be un-polled
/// on entry (see the module header, step 1); `outcome` maps its result to the
/// non-stream refusal body a failure BEFORE the first frame answers.
pub async fn stream_generator<F, T>(
    events: &broadcast::Sender<Event>,
    progress_id: String,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
) -> AxumResponse
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // 1. Subscribe BEFORE the dispatch future is polled.
    let mut rx = events.subscribe();

    let (tx, body_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(64);
    let mut dispatch = Box::pin(dispatch);
    let mut pending: Vec<Vec<u8>> = Vec::new();

    // 2. Race the run against the first frame, so a refusal that produced no
    //    frame can still answer with a status + body rather than a stream.
    let finished = loop {
        tokio::select! {
            biased;
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    if let Some(inner) = matching(&ev, &progress_id) {
                        pending.push(frame(inner));
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break None,
            },
            out = &mut dispatch => break Some(out),
        }
        // A frame has arrived: the stream is committed, stop racing.
        if !pending.is_empty() {
            break None;
        }
    };

    if let Some(out) = finished {
        // The run ended before any frame. Drain whatever it emitted in the same
        // tick, then either refuse (no frame at all) or answer a short stream.
        while let Ok(ev) = rx.try_recv() {
            if let Some(inner) = matching(&ev, &progress_id) {
                pending.push(frame(inner));
            }
        }
        if pending.is_empty() {
            if let Err((status, body)) = outcome(out) {
                return (
                    status,
                    [("content-type", "application/json")],
                    body.to_string(),
                )
                    .into_response();
            }
        }
        for f in pending {
            let _ = tx.send(Ok(f)).await;
        }
        drop(tx);
        return (
            StatusCode::OK,
            SSE_HEADERS,
            Body::from_stream(ReceiverStream::new(body_rx)),
        )
            .into_response();
    }

    // 3. The stream is committed: hand the body back now and keep forwarding
    //    until the dispatch resolves, then drain and close.
    tokio::spawn(async move {
        for f in pending {
            if tx.send(Ok(f)).await.is_err() {
                return; // client went away
            }
        }
        loop {
            tokio::select! {
                biased;
                ev = rx.recv() => match ev {
                    Ok(ev) => {
                        if let Some(inner) = matching(&ev, &progress_id) {
                            if tx.send(Ok(frame(inner))).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                },
                _ = &mut dispatch => break,
            }
        }
        // Drain what landed before the run resolved, then close (v4's
        // `controller.close()` after the runner's promise settles).
        while let Ok(ev) = rx.try_recv() {
            if let Some(inner) = matching(&ev, &progress_id) {
                if tx.send(Ok(frame(inner))).await.is_err() {
                    return;
                }
            }
        }
    });

    (
        StatusCode::OK,
        SSE_HEADERS,
        Body::from_stream(ReceiverStream::new(body_rx)),
    )
        .into_response()
}
