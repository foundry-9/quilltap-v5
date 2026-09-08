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
//!
//! **P4.85 item 9** closes that gap for the two streaming edges: the tests at
//! the end of this file read the optimizer and wizard tier-3 oracles, replay
//! each recorded run's OWN frames through the re-framer, and compare the body
//! to v4's `rawSse` byte for byte. The oracles used to decode the stream into
//! `events` and throw the framing away.
//!
//! Run:
//!   QT_ORACLE_CHARACTER_OPTIMIZER=/tmp/oracle-character-optimizer.ndjson \
//!   QT_ORACLE_CHARACTER_WIZARD=/tmp/oracle-character-wizard.ndjson \
//!     cargo test -p quilltap-web --test generator_sse_wire

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

// ---------------------------------------------------------------------------
// P4.85 item 9 — the re-framer's bytes against v4's RECORDED stream
// ---------------------------------------------------------------------------

/// One oracle row's `{name, rawSse, events}`. Every other field is the tier-3
/// families' business.
#[derive(serde::Deserialize)]
struct SseRow {
    name: String,
    #[serde(rename = "rawSse")]
    raw_sse: Option<String>,
    events: Vec<Value>,
}

/// Replay one recorded run's frames through the re-framer and hand back the
/// response body plus the three headers.
///
/// The dispatch yields after its first emit so the stream COMMITS on that frame
/// and the rest ride the pump — v4's shape, and v5's production shape too (a
/// real runner awaits its model calls, so it cannot resolve inside one poll).
async fn replay(events: &[Value], kind: GeneratorKind) -> (AxumResponse, String) {
    let (tx, _rx) = broadcast::channel::<Event>(8192);
    let emitter = tx.clone();
    let frames: Vec<Value> = events.to_vec();
    let dispatch = async move {
        let mut first = true;
        for ev in frames {
            emit(&emitter, "p1", kind, ev);
            if std::mem::take(&mut first) {
                tokio::task::yield_now().await;
            }
        }
        Ok::<(), ()>(())
    };
    let resp = stream_generator(&tx, "p1".to_string(), dispatch, |r: Result<(), ()>| {
        r.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "no"})))
    })
    .await;
    let headers = format!(
        "{}|{}|{}",
        header(&resp, "content-type"),
        header(&resp, "cache-control"),
        header(&resp, "connection")
    );
    (resp, headers)
}

/// The whole of item 9 for one family: every recorded case that v4 answered as
/// a stream is replayed through the re-framer and its body compared to v4's
/// `rawSse` BYTE FOR BYTE — no normalization at all, because the input events
/// and the expected bytes come from the SAME oracle row, so anything v4 minted
/// is identical on both sides. What is left over is the framing, which is the
/// port: `data: <JSON>\n\n` per event, no `event:` name, no `id:`, no
/// keep-alives, and v4's three headers.
async fn item9(env: &str, kind: GeneratorKind, floor: usize) {
    let Ok(path) = std::env::var(env) else {
        eprintln!("SKIP: set {env} (see the tier-3 family's header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut driven = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: SseRow = serde_json::from_str(line).unwrap();
        let Some(want) = row.raw_sse.as_deref() else {
            // v4 answered a JSON refusal (the 404 / Zod arms), which the
            // families already diff; there is no stream to compare.
            continue;
        };
        let (resp, headers) = replay(&row.events, kind).await;
        assert_eq!(resp.status(), StatusCode::OK, "{}", row.name);
        assert_eq!(
            headers, "text/event-stream|no-cache|keep-alive",
            "{} carried the wrong headers",
            row.name
        );
        let got = body_string(resp).await;
        assert_eq!(
            got, want,
            "{}: the re-framed bytes are not v4's recorded stream",
            row.name
        );
        driven += 1;
    }
    assert!(
        driven >= floor,
        "{env} offered only {driven} streamed cases (floor {floor}) — a corpus \
         that stopped streaming would make this vacuous"
    );
    eprintln!("item9[{env}]: {driven} recorded streams matched byte for byte");
}

/// The optimizer edge (`POST /api/v1/characters/{id}?action=optimize-stream`).
#[tokio::test]
async fn the_optimizer_stream_is_v4s_recorded_bytes() {
    item9(
        "QT_ORACLE_CHARACTER_OPTIMIZER",
        GeneratorKind::Optimizer,
        20,
    )
    .await;
}

/// The AI-import edge (`POST /api/v1/system/tools?action=ai-import-stream`) —
/// P4.86's one named deferral, landed at the generator follow-ups round's
/// unification: the lane proved the FRAMING half against v4's `rawSse` inside
/// its own family but owned no `quilltap-web` file for the three headers.
/// The same re-framer serves all three generator edges, so the same replay
/// proves this one's bytes AND its `text/event-stream` / `no-cache` /
/// `keep-alive` headers (v4 `system/tools/route.ts:1223-1252`). The oracle
/// streams 37 of its 39 rows at `2f4254b42`.
#[tokio::test]
async fn the_ai_import_stream_is_v4s_recorded_bytes() {
    item9("QT_ORACLE_AI_IMPORT", GeneratorKind::AiImport, 30).await;
}

/// The wizard edge (`POST /api/v1/characters?action=ai-wizard-stream`).
#[tokio::test]
async fn the_wizard_stream_is_v4s_recorded_bytes() {
    // The floor was 3 at the lane close; the §3 unification review raised it
    // to the corpus's shape (28 streamed rows at `2f4254b42`) so a corpus that
    // shrank to a handful of streams cannot leave this vacuous.
    item9("QT_ORACLE_CHARACTER_WIZARD", GeneratorKind::Wizard, 20).await;
}
