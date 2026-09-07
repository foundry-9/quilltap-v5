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
//!
//! ## Recorded v5-only divergences (P4.85 item 2)
//!
//! v4's stream is a `ReadableStream` created inside the handler and fed by the
//! runner's own `onProgress` callback, so it can neither lag nor close while
//! the run executes. v5's rides ONE shared broadcast, which can do both — two
//! states with no v4 counterpart at all. Neither is reachable by a user today
//! (the broadcast's capacity is 1024 and its sender lives for the engine's
//! life, so `Closed` means the engine is shutting down), but a silent arm is
//! how a real fault becomes invisible (the #103/#110/#116 class), so all three
//! SAY SO at `warn`:
//!
//! * `Lagged(n)` — frames were dropped; the stream continues, short.
//! * `Closed` in the pre-commit race (`arm = "pre-commit"`) — the loop leaves
//!   with no frame and no dispatch result, so the caller commits to a stream
//!   that carries nothing: the client gets **200 with v4's three SSE headers
//!   and an EMPTY body**, and the run keeps executing on the engine. Pinned by
//!   [`generator_sse_wire`]'s `a_closed_channel_before_any_frame_…` test.
//! * `Closed` in the pump (`arm = "pump"`) — the committed stream ends early,
//!   after whatever `try_recv` can still drain.

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
    stream_generator_from(events.subscribe(), progress_id, dispatch, outcome).await
}

/// The body of [`stream_generator`], taking the subscription it already made.
///
/// Split out for one reason: a caller that holds `&Sender` keeps the channel
/// open by construction, so the two `RecvError::Closed` arms are unreachable
/// through the public entry point and could not be pinned. Taking the receiver
/// by value lets [`tests`] drop the last sender and drive them.
async fn stream_generator_from<F, T>(
    mut rx: broadcast::Receiver<Event>,
    progress_id: String,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
) -> AxumResponse
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
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
                // A recorded v5-only divergence: v4's per-route ReadableStream
                // cannot lose a frame, this broadcast can (capacity 1024). Say so.
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(
                        dropped = n,
                        progress_id = %progress_id,
                        "[generator_sse] event stream lagged; generator frames dropped"
                    );
                    continue;
                }
                // A second recorded v5-only divergence (see the module header):
                // v4's per-route `ReadableStream` cannot close while its run is
                // executing, this broadcast can. Engine shutdown is the only
                // way to reach it today, and it is not silent.
                Err(RecvError::Closed) => {
                    tracing::warn!(
                        progress_id = %progress_id,
                        arm = "pre-commit",
                        "[generator_sse] event channel closed before the run \
                         resolved; answering an empty stream"
                    );
                    break None;
                }
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
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(
                            dropped = n,
                            progress_id = %progress_id,
                            "[generator_sse] event stream lagged; generator frames dropped"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => {
                        tracing::warn!(
                            progress_id = %progress_id,
                            arm = "pump",
                            "[generator_sse] event channel closed mid-run; \
                             closing the stream early"
                        );
                        break;
                    }
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

// ---------------------------------------------------------------------------
// P4.85 item 2 — the two `RecvError::Closed` arms
// ---------------------------------------------------------------------------

/// The two arms with no v4 counterpart at all, and therefore no differential:
/// v4's per-route `ReadableStream` is fed by the runner's own callback and
/// cannot close under a running run. They live here rather than in
/// `tests/generator_sse_wire.rs` because reaching them means dropping the last
/// `Sender`, which a caller holding `&Sender` cannot do —
/// [`stream_generator_from`] is private, so its tests must be in-crate.
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use quilltap_core::api::types::GeneratorKind;
    use serde_json::json;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn no_refusal(r: Result<(), ()>) -> Result<(), (StatusCode, serde_json::Value)> {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    }

    /// **The pre-commit race.** The channel is already closed when the loop
    /// takes its first `recv`, so it leaves with no frame AND no dispatch
    /// result. This is the MEASURED client shape: the caller commits to a
    /// stream anyway — **200, v4's three SSE headers, an empty body** — while
    /// the run itself is still executing on the engine. Written down because
    /// it is not obvious from the code and nothing else records it.
    ///
    /// Mutation: delete the `tracing::warn!` in that arm → the first assert
    /// fails; the response asserts below then still pass, which is the point
    /// (the shape is silent-safe, the silence is what was wrong).
    #[test]
    fn a_closed_channel_before_any_frame_warns_and_answers_an_empty_stream() {
        let rt = rt();
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(async {
                // The only sender is dropped before the subscription is used.
                let rx = {
                    let (tx, _) = broadcast::channel::<Event>(64);
                    tx.subscribe()
                };
                // A dispatch that never resolves: only the closed channel can
                // end the race, which is what makes this arm the one measured.
                let dispatch = async {
                    std::future::pending::<()>().await;
                    Ok::<(), ()>(())
                };
                stream_generator_from(rx, "p1".to_string(), dispatch, no_refusal).await
            })
        });
        let warn = lines
            .iter()
            .find(|l| l.contains("event channel closed before the run resolved"))
            .unwrap_or_else(|| panic!("the pre-commit Closed arm is SILENT: {lines:#?}"));
        assert!(warn.starts_with("WARN "), "{warn}");
        assert!(warn.contains("progress_id=p1"), "{warn}");
        assert!(warn.contains("arm=pre-commit"), "{warn}");

        assert_eq!(resp.status(), StatusCode::OK);
        for (name, value) in SSE_HEADERS {
            assert_eq!(
                resp.headers().get(name).unwrap().to_str().unwrap(),
                value,
                "the committed stream still carries v4's {name}"
            );
        }
        let body = rt.block_on(to_bytes(resp.into_body(), 1 << 20)).unwrap();
        assert!(body.is_empty(), "the empty-stream shape: {body:?}");
    }

    /// **The pump.** One frame commits the stream, then the last sender goes
    /// away while the run is still going: the pump ends the body early rather
    /// than hanging, and says so. The frame that DID land is still delivered.
    ///
    /// Mutation: delete the `tracing::warn!` in that arm → the `find` panics.
    #[test]
    fn a_closed_channel_mid_run_warns_and_closes_the_committed_stream() {
        let rt = rt();
        let (resp, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(async {
                let (tx, _) = broadcast::channel::<Event>(64);
                let rx = tx.subscribe();
                let emitter = tx.clone();
                drop(tx); // `emitter` is now the only sender
                let dispatch = async move {
                    let _ = emitter.send(Event::generator_progress(
                        "p1",
                        GeneratorKind::Optimizer,
                        json!({"type": "start"}),
                    ));
                    drop(emitter); // …and now there are none
                    std::future::pending::<()>().await;
                    Ok::<(), ()>(())
                };
                let resp =
                    stream_generator_from(rx, "p1".to_string(), dispatch, no_refusal).await;
                // Draining the body drives the spawned pump to its Closed arm.
                let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
                String::from_utf8(bytes.to_vec()).unwrap()
            })
        });
        assert_eq!(resp, "data: {\"type\":\"start\"}\n\n");
        let warn = lines
            .iter()
            .find(|l| l.contains("event channel closed mid-run"))
            .unwrap_or_else(|| panic!("the pump's Closed arm is SILENT: {lines:#?}"));
        assert!(warn.starts_with("WARN "), "{warn}");
        assert!(warn.contains("progress_id=p1"), "{warn}");
        assert!(warn.contains("arm=pump"), "{warn}");
    }
}
