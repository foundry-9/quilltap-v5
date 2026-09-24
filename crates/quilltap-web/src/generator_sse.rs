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
//!
//! ## An opt-in failure tail (the Scenario Builder only)
//!
//! Once the stream is committed, the dispatch's result has nowhere to go: the
//! generators and the swipe carry their outcome in their own frames, so the
//! pump drops it. The Scenario Builder's v4 route has ONE more arm — a thrown
//! run enqueues `{"error":"The Host could not complete the enquiry."}` after
//! whatever the run already streamed (`route.ts:156-163`). A caller that wants
//! that passes [`stream_frames_committed_on`]'s `tail`: it receives the committed
//! run's result after the drain and may answer ONE last object, framed like
//! every other. Every other caller passes no tail, and its bytes are
//! unchanged.
//!
//! ## An opt-in commit point (the Scenario Builder only, P4.115)
//!
//! Step 2's rule — "a failure before any frame is a JSON refusal" — is right
//! for the generators and the swipe, whose runs cannot fail between their
//! refusals and their first frame in a way v4 would stream. The Scenario
//! Builder's v4 route returns its `ReadableStream` the moment its refusals pass
//! (`route.ts:178-184`), so its headers go out before any frame, and a failure
//! after that point is the in-stream error frame, never a JSON 500. A caller
//! that can tell when that point is reached passes
//! [`stream_frames_committed_on`]'s `accepted` future: when it resolves, the
//! race commits the stream at once (the response head goes out with no frame
//! yet) and every later outcome rides the pump and the tail. Every other
//! caller passes none, and its race is exactly as before.

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

/// Which payload family a re-framer forwards, and where its inner v4 object
/// sits. A plain fn pointer so the two families share one pump without a
/// generic parameter reaching every caller.
///
/// P4.D207 widened this from a hard-coded `GeneratorProgress` match: the
/// streamed swipe re-frames the same way onto the same wire, and duplicating
/// the pump would have duplicated the two `RecvError` arms and their reasoning
/// with it.
pub type FrameOf = for<'a> fn(&'a EventPayload) -> Option<&'a serde_json::Value>;

/// The optional failure tail (module header): given a COMMITTED run's result,
/// answer one last frame object, or `None` for none.
pub type Tail<T> = Box<dyn FnOnce(T) -> Option<serde_json::Value> + Send>;

/// The character generators' payload (`p4.9k`).
pub fn generator_frame(payload: &EventPayload) -> Option<&serde_json::Value> {
    match payload {
        EventPayload::GeneratorProgress(p) => Some(&p.event),
        _ => None,
    }
}

/// The streamed swipe's payload (P4.D207, v4 `f564b0de3`).
pub fn swipe_frame(payload: &EventPayload) -> Option<&serde_json::Value> {
    match payload {
        EventPayload::SwipeProgress(p) => Some(&p.frame),
        _ => None,
    }
}

// === P4.D217 ===
/// The Scenario Builder's payload (P4.D217, v4 `d1c06cd9d`).
pub fn scenario_builder_frame(payload: &EventPayload) -> Option<&serde_json::Value> {
    match payload {
        EventPayload::ScenarioBuilderProgress(p) => Some(&p.frame),
        _ => None,
    }
}
// === end P4.D217 ===

/// Pull the inner v4 object out of an [`Event`] iff it is `frame_of`'s family
/// AND carries `progress_id`.
fn matching<'a>(
    ev: &'a Event,
    progress_id: &str,
    frame_of: FrameOf,
) -> Option<&'a serde_json::Value> {
    if ev.progress_id.as_deref() != Some(progress_id) {
        return None;
    }
    frame_of(&ev.payload)
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
    stream_frames(events, progress_id, generator_frame, dispatch, outcome).await
}

/// Re-frame ONE run of any `progress_id`-tagged Event family as v4's SSE
/// stream. `dispatch` MUST be un-polled on entry (see the module header,
/// step 1); `outcome` maps its result to the non-stream refusal body a failure
/// BEFORE the first frame answers.
pub async fn stream_frames<F, T>(
    events: &broadcast::Sender<Event>,
    progress_id: String,
    frame_of: FrameOf,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
) -> AxumResponse
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // 1. Subscribe BEFORE the dispatch future is polled.
    stream_generator_from(
        events.subscribe(),
        progress_id,
        frame_of,
        dispatch,
        outcome,
        None,
    )
    .await
}

/// [`stream_frames`] with a failure `tail` and an opt-in commit point (module
/// header): once the stream is committed the run's result goes to `tail`
/// after the drain, and the object it answers (if any) is the stream's LAST
/// frame; when `accepted` resolves before any frame or result, the stream
/// commits then — headers out, no frame yet. `outcome` still answers a result
/// that arrives BEFORE `accepted` (a refusal).
pub async fn stream_frames_committed_on<F, T>(
    events: &broadcast::Sender<Event>,
    progress_id: String,
    frame_of: FrameOf,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
    tail: Tail<T>,
    accepted: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
) -> AxumResponse
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // 1. Subscribe BEFORE the dispatch future is polled.
    stream_generator_from_committed_on(
        events.subscribe(),
        progress_id,
        frame_of,
        dispatch,
        outcome,
        Some(tail),
        accepted,
    )
    .await
}

/// The body of [`stream_generator`], taking the subscription it already made.
///
/// Split out for one reason: a caller that holds `&Sender` keeps the channel
/// open by construction, so the two `RecvError::Closed` arms are unreachable
/// through the public entry point and could not be pinned. Taking the receiver
/// by value lets [`tests`] drop the last sender and drive them.
async fn stream_generator_from<F, T>(
    rx: broadcast::Receiver<Event>,
    progress_id: String,
    frame_of: FrameOf,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
    tail: Option<Tail<T>>,
) -> AxumResponse
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // No commit point: the race below can only end on a frame, a result, or
    // the channel closing — every caller but the opt-in one.
    stream_generator_from_committed_on(
        rx,
        progress_id,
        frame_of,
        dispatch,
        outcome,
        tail,
        Box::pin(std::future::pending()),
    )
    .await
}

/// [`stream_generator_from`] with the opt-in commit point (module header).
async fn stream_generator_from_committed_on<F, T>(
    mut rx: broadcast::Receiver<Event>,
    progress_id: String,
    frame_of: FrameOf,
    dispatch: F,
    outcome: impl Fn(T) -> Result<(), (StatusCode, serde_json::Value)> + Send + 'static,
    tail: Option<Tail<T>>,
    mut accepted: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
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
                    if let Some(inner) = matching(&ev, &progress_id, frame_of) {
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
            // The opt-in commit point, polled BEFORE the dispatch: a run
            // accepted and then failed in the same tick is a committed stream
            // whose failure rides the tail, never a refusal.
            () = &mut accepted => break None,
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
            if let Some(inner) = matching(&ev, &progress_id, frame_of) {
                pending.push(frame(inner));
            }
        }
        // The opt-in commit point can fire INSIDE the dispatch's own poll
        // (the Scenario Builder accepts, then its driver fails in the same
        // tick), so the race above saw the result first. A run that was
        // accepted is a committed stream whatever it produced: its result
        // rides the tail, never the refusal body. (Always pending for every
        // caller without a commit point.)
        let committed = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(accepted.as_mut().poll(cx).is_ready())
        })
        .await;
        if pending.is_empty() && !committed {
            if let Err((status, body)) = outcome(out) {
                return (
                    status,
                    [("content-type", "application/json")],
                    body.to_string(),
                )
                    .into_response();
            }
        } else if let Some(last) = tail.and_then(|t| t(out)) {
            // Frames were emitted (or the run was accepted), so the stream is
            // committed after all: the result rides the tail, exactly as on the
            // pump's path below.
            pending.push(frame(&last));
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
        let mut result = None;
        loop {
            tokio::select! {
                biased;
                ev = rx.recv() => match ev {
                    Ok(ev) => {
                        if let Some(inner) = matching(&ev, &progress_id, frame_of) {
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
                out = &mut dispatch => {
                    result = Some(out);
                    break;
                }
            }
        }
        // Drain what landed before the run resolved, then close (v4's
        // `controller.close()` after the runner's promise settles).
        let mut client_gone = false;
        while let Ok(ev) = rx.try_recv() {
            if let Some(inner) = matching(&ev, &progress_id, frame_of) {
                if tx.send(Ok(frame(inner))).await.is_err() {
                    client_gone = true;
                    break;
                }
            }
        }
        // The opt-in tail runs whether or not the client is still there (v4
        // logs its failure line either way; only the enqueue is skipped).
        if let (Some(tail), Some(out)) = (tail, result) {
            if let Some(last) = tail(out) {
                if !client_gone {
                    let _ = tx.send(Ok(frame(&last))).await;
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
                stream_generator_from(
                    rx,
                    "p1".to_string(),
                    generator_frame,
                    dispatch,
                    no_refusal,
                    None,
                )
                .await
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
                let resp = stream_generator_from(
                    rx,
                    "p1".to_string(),
                    generator_frame,
                    dispatch,
                    no_refusal,
                    None,
                )
                .await;
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
