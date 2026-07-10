//! The chat-creation progress bus ("The Green Room") — v4's
//! `lib/chat/creation-progress.ts` (D6).
//!
//! `POST /api/v1/chats` does a lot of slow, blocking work before it returns
//! (resolving the cast, the per-character LLM wardrobe step, compiling identity
//! stacks, backfilling continuation history, seeding the opening scene). v4
//! narrates that to a status dialog over a side-channel keyed by a
//! client-generated `progressId`.
//!
//! In v5 the frames are [`api::EventPayload::CreationProgress`](crate::api::EventPayload)
//! variants riding the one global `/api/events` stream, scope-tagged by
//! `progress_id` (D3). Live subscribers see each frame as the driver emits it;
//! a subscriber that connects a tick late (the dialog opens its reader around
//! the instant the POST fires) replays the buffered backlog — the
//! [`CreationProgressBus`] is the core-adjacent buffer that makes that possible
//! (200-frame cap, replay-on-subscribe, a 60 s TTL after the terminal `done`).
//!
//! ## The transport choice (documented, per the work order)
//!
//! v4 buffers per-`progressId` channels and a dedicated SSE route replays one
//! channel. v5 has ONE global `/api/events` stream, so the transport does
//! **replay-on-subscribe**: a new stream connection first drains
//! [`CreationProgressBus::active_snapshot`] (every not-yet-expired channel's
//! buffered frames) before attaching to the live broadcast. A `progressId`
//! query filter is unnecessary — chat creation is short-lived and single-user,
//! so replaying every in-flight channel's backlog is both correct and cheap.
//!
//! ## v4's un-emitted `error` frame (faithful)
//!
//! v4's create handler never actually calls `failCreationProgress` — a hard
//! failure leaves the channel to its TTL rather than posting a terminal `error`
//! frame. The [`CreationProgressEmitter::fail`] method is ported for
//! completeness (v4 defines it), but the `handleCreate` spine does not call it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::api::types::{Event, EventPayload};
use crate::clock::now_unix_ms;

/// Cap the per-channel buffer — a creation flow emits a few dozen frames at most.
const MAX_BUFFER: usize = 200;
/// Keep a finished channel around briefly so a late reconnect still resolves.
const CLEANUP_TTL_MS: i64 = 60_000;

/// One resolved garment in a slot preview (v4 `OutfitPreviewEntry`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutfitPreviewEntry {
    pub id: String,
    pub title: String,
    pub is_composite: bool,
}

/// The decided four-slot outfit rendered read-only in the dialog (v4
/// `OutfitPreviewSlots`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutfitPreviewSlots {
    pub top: Vec<OutfitPreviewEntry>,
    pub bottom: Vec<OutfitPreviewEntry>,
    pub footwear: Vec<OutfitPreviewEntry>,
    pub accessories: Vec<OutfitPreviewEntry>,
}

/// A `log`-frame severity (v4 `level?: 'info' | 'warn' | 'error'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// The `kind`-tagged creation-progress frame (v4 `CreationProgressEvent`). Each
/// carries a `ts` (ms). Serializes to v4's SSE frame shape; the [`Event`]
/// envelope adds the `progressId` scope tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CreationProgressFrame {
    Status {
        message: String,
        ts: i64,
    },
    Log {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<LogLevel>,
        ts: i64,
    },
    #[serde(rename_all = "camelCase")]
    WardrobeStart {
        character_id: String,
        character_name: String,
        ts: i64,
    },
    #[serde(rename_all = "camelCase")]
    WardrobeResult {
        character_id: String,
        character_name: String,
        slots: OutfitPreviewSlots,
        ts: i64,
    },
    Done {
        ts: i64,
    },
    Error {
        message: String,
        ts: i64,
    },
}

impl CreationProgressFrame {
    /// v4's terminal frames (`done` / `error`) — after either, the channel is
    /// finished and further publishes are dropped.
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            CreationProgressFrame::Done { .. } | CreationProgressFrame::Error { .. }
        )
    }
}

/// One channel's buffered backlog. `finished_at_ms` is set once a terminal
/// frame lands, arming the TTL.
struct Channel {
    buffer: Vec<CreationProgressFrame>,
    finished_at_ms: Option<i64>,
}

/// The core-adjacent replay buffer (v4's `globalThis` channel map). Buffers
/// each `progressId`'s frames so a late subscriber replays the backlog;
/// finished channels are pruned a TTL after their terminal frame.
#[derive(Default)]
pub struct CreationProgressBus {
    channels: Mutex<HashMap<String, Channel>>,
}

impl CreationProgressBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a frame to a channel and arm the TTL on a terminal frame. A no-op
    /// after the channel has finished (v4 `publishCreationProgress` /
    /// `finishCreationProgress` both bail when `ch.finished`).
    pub fn publish(&self, id: &str, frame: &CreationProgressFrame, now_ms: i64) {
        if id.is_empty() {
            return;
        }
        let mut map = self.channels.lock().unwrap();
        let ch = map.entry(id.to_string()).or_insert_with(|| Channel {
            buffer: Vec::new(),
            finished_at_ms: None,
        });
        if ch.finished_at_ms.is_some() {
            return;
        }
        ch.buffer.push(frame.clone());
        // v4 splices the oldest overflow off the front.
        if ch.buffer.len() > MAX_BUFFER {
            let overflow = ch.buffer.len() - MAX_BUFFER;
            ch.buffer.drain(0..overflow);
        }
        if frame.is_terminal() {
            ch.finished_at_ms = Some(now_ms);
        }
    }

    /// The buffered backlog for one channel (v4's subscribe replay).
    pub fn replay(&self, id: &str) -> Vec<CreationProgressFrame> {
        self.channels
            .lock()
            .unwrap()
            .get(id)
            .map(|ch| ch.buffer.clone())
            .unwrap_or_default()
    }

    /// Prune channels whose TTL has elapsed (finished more than
    /// `CLEANUP_TTL_MS` ago), then return every remaining channel's backlog as
    /// `(progress_id, frame)` pairs — the replay-on-subscribe snapshot the
    /// transport prepends to a new `/api/events` stream. Best-effort: pruning
    /// happens lazily on access (no timer), matching v4's "leave it to the TTL"
    /// intent without a scheduler in the core.
    pub fn active_snapshot(&self, now_ms: i64) -> Vec<(String, CreationProgressFrame)> {
        let mut map = self.channels.lock().unwrap();
        map.retain(|_, ch| match ch.finished_at_ms {
            Some(finished) => now_ms - finished < CLEANUP_TTL_MS,
            None => true,
        });
        let mut out = Vec::new();
        for (id, ch) in map.iter() {
            for frame in &ch.buffer {
                out.push((id.clone(), frame.clone()));
            }
        }
        out
    }

    /// Test-only: drop every channel.
    #[cfg(test)]
    pub fn reset(&self) {
        self.channels.lock().unwrap().clear();
    }
}

/// A per-id emitter handed to the creation flow (v4 `CreationProgressEmitter`).
/// When no `progress_id` was supplied it is entirely inert, so `handleCreate`
/// never has to branch on whether progress is being tracked. When active it
/// fans each frame out to BOTH the live broadcast (so a connected subscriber
/// sees it immediately) AND the [`CreationProgressBus`] (so a late subscriber
/// replays it).
#[derive(Clone)]
pub struct CreationProgressEmitter {
    inner: Option<Active>,
}

#[derive(Clone)]
struct Active {
    progress_id: String,
    bus: Arc<CreationProgressBus>,
    events: broadcast::Sender<Event>,
}

impl CreationProgressEmitter {
    /// The no-op emitter (v4 `NOOP_EMITTER`) — `progressId` absent.
    pub fn inert() -> Self {
        CreationProgressEmitter { inner: None }
    }

    /// An active emitter for a `progress_id`, fanning out to the bus + the
    /// engine's event broadcast.
    pub fn active(
        progress_id: impl Into<String>,
        bus: Arc<CreationProgressBus>,
        events: broadcast::Sender<Event>,
    ) -> Self {
        CreationProgressEmitter {
            inner: Some(Active {
                progress_id: progress_id.into(),
                bus,
                events,
            }),
        }
    }

    /// Build from an optional `progress_id`: `None`/empty → inert (v4
    /// `createCreationProgressEmitter`).
    pub fn from_id(
        progress_id: Option<&str>,
        bus: Arc<CreationProgressBus>,
        events: broadcast::Sender<Event>,
    ) -> Self {
        match progress_id {
            Some(id) if !id.is_empty() => Self::active(id, bus, events),
            _ => Self::inert(),
        }
    }

    fn emit(&self, frame: CreationProgressFrame) {
        let Some(active) = &self.inner else {
            return;
        };
        active
            .bus
            .publish(&active.progress_id, &frame, now_unix_ms());
        // Best-effort live fan-out; a lagging/absent subscriber is fine.
        let _ = active.events.send(Event {
            chat_id: None,
            room_id: None,
            progress_id: Some(active.progress_id.clone()),
            payload: EventPayload::CreationProgress(frame),
        });
    }

    pub fn status(&self, message: impl Into<String>) {
        self.emit(CreationProgressFrame::Status {
            message: message.into(),
            ts: now_unix_ms(),
        });
    }

    pub fn log(&self, message: impl Into<String>, level: Option<LogLevel>) {
        self.emit(CreationProgressFrame::Log {
            message: message.into(),
            level,
            ts: now_unix_ms(),
        });
    }

    pub fn wardrobe_start(
        &self,
        character_id: impl Into<String>,
        character_name: impl Into<String>,
    ) {
        self.emit(CreationProgressFrame::WardrobeStart {
            character_id: character_id.into(),
            character_name: character_name.into(),
            ts: now_unix_ms(),
        });
    }

    pub fn wardrobe_result(
        &self,
        character_id: impl Into<String>,
        character_name: impl Into<String>,
        slots: OutfitPreviewSlots,
    ) {
        self.emit(CreationProgressFrame::WardrobeResult {
            character_id: character_id.into(),
            character_name: character_name.into(),
            slots,
            ts: now_unix_ms(),
        });
    }

    /// The terminal `done` frame (v4 `finishCreationProgress`).
    pub fn finish(&self) {
        self.emit(CreationProgressFrame::Done { ts: now_unix_ms() });
    }

    /// The terminal `error` frame (v4 `failCreationProgress`). Ported for
    /// completeness; `handleCreate` never calls it (see the module header).
    pub fn fail(&self, message: impl Into<String>) {
        self.emit(CreationProgressFrame::Error {
            message: message.into(),
            ts: now_unix_ms(),
        });
    }

    /// Whether this emitter is active (has a `progress_id`).
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(msg: &str, ts: i64) -> CreationProgressFrame {
        CreationProgressFrame::Status {
            message: msg.to_string(),
            ts,
        }
    }

    #[test]
    fn empty_id_publishes_nothing() {
        let bus = CreationProgressBus::new();
        bus.publish("", &status("x", 1), 1);
        assert!(bus.replay("").is_empty());
        assert!(bus.active_snapshot(1).is_empty());
    }

    #[test]
    fn buffer_caps_at_max_dropping_oldest() {
        let bus = CreationProgressBus::new();
        for i in 0..(MAX_BUFFER + 5) {
            bus.publish("c", &status(&format!("m{i}"), i as i64), i as i64);
        }
        let replay = bus.replay("c");
        assert_eq!(replay.len(), MAX_BUFFER);
        // The oldest five were spliced off the front; the newest survives.
        match &replay[0] {
            CreationProgressFrame::Status { message, .. } => assert_eq!(message, "m5"),
            other => panic!("unexpected: {other:?}"),
        }
        match replay.last().unwrap() {
            CreationProgressFrame::Status { message, .. } => {
                assert_eq!(message, &format!("m{}", MAX_BUFFER + 4))
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn late_subscriber_replays_backlog() {
        let bus = CreationProgressBus::new();
        bus.publish("c", &status("a", 1), 1);
        bus.publish("c", &status("b", 2), 2);
        let snap = bus.active_snapshot(3);
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|(id, _)| id == "c"));
    }

    #[test]
    fn terminal_frame_finishes_channel_and_drops_further_publishes() {
        let bus = CreationProgressBus::new();
        bus.publish("c", &status("a", 1), 1);
        bus.publish("c", &CreationProgressFrame::Done { ts: 2 }, 2);
        // A publish after done is a no-op.
        bus.publish("c", &status("late", 3), 3);
        let replay = bus.replay("c");
        assert_eq!(replay.len(), 2);
        assert!(matches!(replay[1], CreationProgressFrame::Done { .. }));
    }

    #[test]
    fn finished_channel_pruned_after_ttl() {
        let bus = CreationProgressBus::new();
        bus.publish("c", &status("a", 1), 1);
        bus.publish("c", &CreationProgressFrame::Done { ts: 2 }, 2);
        // Just inside the TTL → still present.
        assert_eq!(bus.active_snapshot(2 + CLEANUP_TTL_MS - 1).len(), 2);
        // Past the TTL → pruned.
        bus.publish("c", &status("a", 1), 1); // no-op (finished)
        assert!(bus.active_snapshot(2 + CLEANUP_TTL_MS).is_empty());
    }

    #[test]
    fn unfinished_channel_never_pruned() {
        let bus = CreationProgressBus::new();
        bus.publish("c", &status("a", 1), 1);
        // A far-future now with no terminal frame keeps the channel.
        assert_eq!(bus.active_snapshot(999_999_999).len(), 1);
    }

    #[test]
    fn inert_emitter_is_a_no_op() {
        let e = CreationProgressEmitter::inert();
        assert!(!e.is_active());
        // Every method is safe to call and does nothing observable.
        e.status("x");
        e.log("y", Some(LogLevel::Warn));
        e.finish();
    }

    #[test]
    fn frame_serializes_to_v4_shape() {
        let f = status("Assembling the cast…", 1700000000000);
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["kind"], "status");
        assert_eq!(v["message"], "Assembling the cast…");
        assert_eq!(v["ts"], 1700000000000i64);

        let log = CreationProgressFrame::Log {
            message: "settled".into(),
            level: Some(LogLevel::Warn),
            ts: 5,
        };
        let lv = serde_json::to_value(&log).unwrap();
        assert_eq!(lv["kind"], "log");
        assert_eq!(lv["level"], "warn");

        let ws = CreationProgressFrame::WardrobeStart {
            character_id: "cid".into(),
            character_name: "Aria".into(),
            ts: 6,
        };
        let wv = serde_json::to_value(&ws).unwrap();
        assert_eq!(wv["kind"], "wardrobe-start");
        assert_eq!(wv["characterId"], "cid");
        assert_eq!(wv["characterName"], "Aria");
    }
}
