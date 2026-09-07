//! Wire tests for the P4.9K0 generator SSE re-framer.
//!
//! The expected bytes are transcribed from v4's three identical handlers
//! (`app/api/v1/characters/[id]/handlers/post.ts:113-140`,
//! `app/api/v1/characters/handlers/post.ts:548-575`,
//! `app/api/v1/system/tools/route.ts:1223-1252`):
//! `data: ${JSON.stringify(event)}\n\n` per event, `controller.close()` after
//! the runner resolves, and the three headers. There is no v4 twin for the
//! MECHANISM (v4's SSE is per-route; v5's rides the one Event channel), so this
//! is pinned against v4's recorded framing rather than diffed against a runner.

use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use quilltap_core::api::types::{Event, GeneratorKind};
use quilltap_web::generator_sse::stream_generator;
use serde_json::{json, Value};
use tokio::sync::broadcast;

fn emit(tx: &broadcast::Sender<Event>, progress_id: &str, kind: GeneratorKind, event: Value) {
    let _ = tx.send(Event::generator_progress(progress_id, kind, event));
}

async fn body_string(resp: AxumResponse) -> String {
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf8")
}

fn header(resp: &AxumResponse, name: &str) -> String {
    resp.headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

/// Three frames and a clean resolve: v4's exact bytes and v4's three headers.
#[tokio::test]
async fn three_frames_and_a_resolve_reproduce_v4s_stream() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let emitter = tx.clone();
    let dispatch = async move {
        emit(
            &emitter,
            "p1",
            GeneratorKind::Optimizer,
            json!({"type": "start"}),
        );
        emit(
            &emitter,
            "p1",
            GeneratorKind::Optimizer,
            json!({"type": "substep_complete", "step": "generating"}),
        );
        emit(
            &emitter,
            "p1",
            GeneratorKind::Optimizer,
            json!({"type": "done", "analysis": "a", "suggestions": []}),
        );
        Ok::<(), ()>(())
    };

    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(header(&resp, "content-type"), "text/event-stream");
    assert_eq!(header(&resp, "cache-control"), "no-cache");
    assert_eq!(header(&resp, "connection"), "keep-alive");
    assert_eq!(
        body_string(resp).await,
        concat!(
            "data: {\"type\":\"start\"}\n\n",
            "data: {\"type\":\"substep_complete\",\"step\":\"generating\"}\n\n",
            "data: {\"type\":\"done\",\"analysis\":\"a\",\"suggestions\":[]}\n\n",
        )
    );
}

/// A frame emitted SYNCHRONOUSLY inside the dispatch future still lands: the
/// subscription is taken before the future is polled at all. Swapping those two
/// (awaiting the dispatch before subscribing) empties this body.
#[tokio::test]
async fn a_frame_emitted_before_the_dispatch_resolves_is_not_lost() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let emitter = tx.clone();
    // No `.await` anywhere in the body: the emit happens on the very first poll.
    let dispatch = async move {
        emit(
            &emitter,
            "p1",
            GeneratorKind::Wizard,
            json!({"type": "first"}),
        );
        Ok::<(), ()>(())
    };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;
    assert_eq!(body_string(resp).await, "data: {\"type\":\"first\"}\n\n");
}

/// Another run's frames are not this stream's. Both the `progressId` scope tag
/// and the payload kind gate the forward.
#[tokio::test]
async fn a_frame_for_another_progress_id_is_not_forwarded() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let emitter = tx.clone();
    let dispatch = async move {
        emit(
            &emitter,
            "OTHER",
            GeneratorKind::AiImport,
            json!({"type": "theirs"}),
        );
        emit(
            &emitter,
            "p1",
            GeneratorKind::AiImport,
            json!({"type": "mine"}),
        );
        emit(
            &emitter,
            "OTHER",
            GeneratorKind::AiImport,
            json!({"type": "theirs2"}),
        );
        // A different Event family on the same scope tag is not ours either.
        let _ = emitter.send(Event::creation_progress(
            "p1",
            quilltap_core::services::creation_progress::CreationProgressFrame::Done { ts: 1 },
        ));
        Ok::<(), ()>(())
    };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;
    assert_eq!(body_string(resp).await, "data: {\"type\":\"mine\"}\n\n");
}

/// A refusal BEFORE the run starts (v4's Zod parse / route-level `badRequest`)
/// answers the dispatch envelope with its status — not a stream, and not a
/// 200 with an empty body.
#[tokio::test]
async fn a_dispatch_error_before_any_frame_answers_the_envelope() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let dispatch = async move { Err::<(), &'static str>("Invalid request") };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), &str>| {
        r.map_err(|m| {
            (
                StatusCode::BAD_REQUEST,
                json!({"error": {"kind": "badRequest", "message": m}}),
            )
        })
    })
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(header(&resp, "content-type"), "application/json");
    assert_eq!(
        body_string(resp).await,
        r#"{"error":{"kind":"badRequest","message":"Invalid request"}}"#
    );
}

/// A run that succeeds having narrated nothing is still a STREAM (v4 opens the
/// response before the runner starts and closes it empty) — the refusal arm is
/// reserved for a failed dispatch.
#[tokio::test]
async fn a_silent_success_is_an_empty_stream_not_a_refusal() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let dispatch = async move { Ok::<(), ()>(()) };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(header(&resp, "content-type"), "text/event-stream");
    assert_eq!(body_string(resp).await, "");
}

/// A failure AFTER the first frame keeps the stream (v4 cannot un-send a
/// response either); the outcome rides the run's own terminal `error` frame.
#[tokio::test]
async fn a_failure_after_the_first_frame_still_streams() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let emitter = tx.clone();
    let dispatch = async move {
        emit(
            &emitter,
            "p1",
            GeneratorKind::Optimizer,
            json!({"type": "start"}),
        );
        emit(
            &emitter,
            "p1",
            GeneratorKind::Optimizer,
            json!({"type": "error", "error": "boom"}),
        );
        Err::<(), &'static str>("boom")
    };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), &str>| {
        r.map_err(|m| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": m})))
    })
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        body_string(resp).await,
        concat!(
            "data: {\"type\":\"start\"}\n\n",
            "data: {\"type\":\"error\",\"error\":\"boom\"}\n\n",
        )
    );
}

/// The forwarded bytes are the generator's own object, key order included —
/// deliberately non-alphabetical here, and NOT re-sorted (the
/// `json-column-key-order` rule). The v5 envelope's `type` / `generator` never
/// reach this wire.
#[tokio::test]
async fn the_forwarded_bytes_are_the_inner_event_verbatim() {
    let (tx, _rx) = broadcast::channel::<Event>(64);
    let emitter = tx.clone();
    let dispatch = async move {
        emit(
            &emitter,
            "p1",
            GeneratorKind::Wizard,
            json!({"zeta": 1, "alpha": 2, "type": "step", "nested": {"b": 1, "a": 2}}),
        );
        Ok::<(), ()>(())
    };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;
    assert_eq!(
        body_string(resp).await,
        "data: {\"zeta\":1,\"alpha\":2,\"type\":\"step\",\"nested\":{\"b\":1,\"a\":2}}\n\n"
    );
}
