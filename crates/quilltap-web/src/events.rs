//! `GET /api/events` — the one global SSE stream per client (D3).
//!
//! Each engine [`Event`] writes as v4's frame encoding — `data: <JSON>\n\n`,
//! no `event:` names — preceded by an incrementing `id:` field (a future
//! `Last-Event-ID` replay rides it; **no replay buffer this round** — a
//! broadcast lag (drop-oldest) is the client's resync signal, per D3
//! best-effort). The keep-alive is the SSE comment `: keep-alive\n\n` (v4
//! `encodeKeepAlive`) every [`KEEP_ALIVE_INTERVAL`] (v4 pins no cadence on
//! the send stream; 15 s per the work order).
//!
//! ## Green-Room replay-on-subscribe (D6)
//!
//! A new subscriber first drains the [`CreationProgressBus`](quilltap_core::api)
//! snapshot — every not-yet-expired creation channel's buffered frames as
//! `creationProgress` [`Event`]s — BEFORE attaching to the live broadcast, so a
//! creation dialog that opens its reader a tick after the create POST fires
//! still replays the backlog. The subscription is taken BEFORE the snapshot so
//! no frame slips between the two.

use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Event, QuilltapCore};
use quilltap_core::clock::now_unix_ms;
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;

use crate::state::SharedState;

pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

pub async fn events(State(state): State<SharedState>) -> AxumResponse {
    let Some(host) = state.host() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "server failed to start").into_response();
    };
    // Subscribe BEFORE snapshotting so no creation frame slips between the two.
    let mut rx = host.core().subscribe();
    let backlog: Vec<Event> = host
        .core()
        .creation_progress_bus()
        .active_snapshot(now_unix_ms())
        .into_iter()
        .map(|(progress_id, frame)| Event::creation_progress(progress_id, frame))
        .collect();

    let (tx, body_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(64);
    tokio::spawn(async move {
        let mut next_id: u64 = 1;
        // Replay the Green-Room backlog first (D6), using the same frame
        // encoding as the live stream below.
        for ev in backlog {
            if let Ok(json) = serde_json::to_string(&ev) {
                let frame = format!("id: {next_id}\ndata: {json}\n\n");
                next_id += 1;
                if tx.send(Ok(frame.into_bytes())).await.is_err() {
                    return; // client went away before the live stream
                }
            }
        }
        let mut keep_alive = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        keep_alive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The interval fires immediately once; skip that first tick.
        keep_alive.tick().await;
        loop {
            tokio::select! {
                event = rx.recv() => match event {
                    Ok(ev) => {
                        let json = match serde_json::to_string(&ev) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        let frame = format!("id: {next_id}\ndata: {json}\n\n");
                        next_id += 1;
                        if tx.send(Ok(frame.into_bytes())).await.is_err() {
                            break; // client went away
                        }
                    }
                    // Lagged: events were dropped (drop-oldest). Keep going —
                    // the client treats the id gap as its resync signal.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                },
                _ = keep_alive.tick() => {
                    if tx.send(Ok(b": keep-alive\n\n".to_vec())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    (
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-cache"),
            ("connection", "keep-alive"),
        ],
        Body::from_stream(ReceiverStream::new(body_rx)),
    )
        .into_response()
}
