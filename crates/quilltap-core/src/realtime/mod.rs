//! The realtime invalidation subsystem (v4 `lib/realtime/**`, `f3892158d`).
//!
//! A hint says *which slice of server state changed*, never *what it changed
//! to*. The HTTP API stays the single source of truth for the data itself; a
//! hint only says when to look again. That keeps the client's reconnect story
//! trivial ("invalidate everything and refetch") and means there is no second
//! serialization schema to drift from the REST responses.
//!
//! ## The one mechanism divergence from v4, and why
//!
//! v4 adds a second WebSocket at `/api/v1/system/realtime/stream` and
//! multiplexes hints over it. **v5 has no such endpoint and will not grow
//! one.** The hints ride v5's EXISTING [`Event`](crate::api::Event) channel —
//! the engine broadcast, which reaches the browser as SSE
//! (`quilltap-web`'s `GET /api/events`) and the desktop shell as
//! `quilltap://event` (`quilltap-tauri`'s event pump). This is mandated by the
//! locked transport-agnostic boundary: *"Streaming only ever on the `Event`
//! channel"* (CLAUDE.md, phase-4.md). A second socket would put a second
//! streaming transport above the boundary, which the boundary exists to
//! forbid — and it would have to be built twice, once per host.
//!
//! Everything v4's socket carries, the Event channel already carries: one
//! stream per client, server→client only, best-effort with a documented
//! resync. The parts of v4's wire that are WS-protocol rather than feature —
//! `RealtimeClientMessageSchema` (`{type:'ping'}` → `pong`) and
//! `REALTIME_STREAM_PATH` — do not port: SSE needs no app-level ping (v5's
//! stream keep-alives every 15 s, `events.rs`), and the Tauri pump is
//! in-process.
//!
//! ## Modules
//!
//! * [`types`] — the hint on the wire, and the six topics.
//! * [`bus`] — the publish chokepoint and its 250 ms trailing-edge coalescing.
//! * [`job_topics`] — the PURE job-type/write-batch → topic computation.

pub mod bus;
pub mod job_topics;
pub mod types;
