//! The typed chat **event** vocabulary (Phase-3 Unit-3 wave 3) — the v5 form of
//! v4's SSE single-key JSON events (`status`, `content`, `reasoning`, `done`,
//! …). Per `docs/developer/porting/api-boundary.md` these are `Event` variants
//! on the one push channel; the enum grows one variant per landed service (no
//! speculative enumeration) and migrates to the boundary module when Phase 4
//! builds the transports.
//!
//! ## The vocabulary so far
//!
//! This first slice enumerates ONLY the events the primary-stream / recovery /
//! provider-failover services actually emit (`primary-stream.service.ts`,
//! `recovery.service.ts`, `provider-failover.service.ts`):
//!
//! | v4 SSE frame | `ChatEvent` variant | Emitted by |
//! |---|---|---|
//! | `{status:{stage,message,characterName?,characterId?}}` | [`ChatEvent::Status`] | pre-stream / streaming / retry / rerouting status |
//! | `{content}` | [`ChatEvent::Content`] | each content delta |
//! | `{reasoning}` | [`ChatEvent::Reasoning`] | live "thinking" (cumulative) |
//! | `{done:true, messageId, usage, cacheUsage, attachmentResults, toolsExecuted}` | [`ChatEvent::Done`] | recovery / static-fallback close |
//!
//! Every other v4 SSE frame (`turnStart`, `turnComplete`, `chainComplete`,
//! `carinaAnswer`, `confirmationResult`, `pendingExternalTurn`, `error`,
//! keep-alive) belongs to a service not yet ported and is deliberately absent —
//! it lands with its emitter.
//!
//! ## Byte-fidelity with v4's SSE payloads
//!
//! Each variant serializes (via serde) to the SAME single-key JSON object v4's
//! `encode*Event` helper builds — so the tier-3 differential can decode both the
//! Rust [`RecordingSink`] trace and v4's recorded SSE `data:` frames to JSON and
//! compare byte-for-byte. The field names / omission rules match v4 exactly:
//! `characterName` / `characterId` on a status are `skip_serializing_if` (v4
//! omits them when the caller passes `undefined`), and the whole `done` payload
//! spreads its fields at the top level next to `done: true` (v4's
//! `{ done: true, ...data }`), with `usage`/`cacheUsage`/`attachmentResults`
//! always present as explicit `null` on the recovery paths.
//!
//! ## The sink seam
//!
//! [`EventSink`] is the v5 form of v4's `controller` + `encoder` pair: v4
//! enqueues encoded bytes onto a `ReadableStreamDefaultController`; here a
//! service pushes typed [`ChatEvent`]s at a sink. The single method `emit` is
//! infallible from the caller's view — v4's `safeEnqueue` swallows a closed
//! controller and `controller.enqueue` in the hot path is fire-and-forget, so a
//! sink that can no longer deliver simply drops (it must never surface an error
//! that would divert the stream's control flow). Visibility filtering (which
//! subscriber may see which event) is a Phase-4 boundary concern, above this
//! seam.
//!
//! [`RecordingSink`] captures the ordered emission for the differential; a real
//! transport sink (Tauri emitter / axum SSE) is a Phase-4 adapter.

use serde::Serialize;

/// A UI-feedback status update (v4 `encodeStatusEvent`). `stage` is a free-form
/// lifecycle label (`sending` / `streaming` / `retrying` / `rerouting`);
/// `message` is the human string. `character_*` are omitted when absent, matching
/// v4's `undefined` fields dropped by `JSON.stringify`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
}

/// Token usage on a done event (v4 `TokenUsage`). Every field is
/// `skip_serializing_if` so an object with only the fields v4 set renders the
/// same shape — but on the recovery paths the whole `usage` is `null`, never a
/// partial object, so this only matters for a future non-null done.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
}

/// Cache usage on a done event (v4 `CacheUsage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneCacheUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,
}

/// The `done` event payload (v4 `encodeDoneEvent`'s `data`). The recovery paths
/// emit it with `usage` / `cache_usage` / `attachment_results` all `null` and
/// `tools_executed: false`; the fields that v4 leaves off entirely on these
/// callsites (`turn`, `provider`, `reasoningContent`, …) are absent here — they
/// arrive with the finalizer service that sets them.
///
/// Serializes flat next to `done: true` (see [`ChatEvent`]), so a serialized
/// [`ChatEvent::Done`] is byte-identical to v4's `{ done: true, ...data }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    /// The persisted message id (v4 always sets it on the recovery done events).
    pub message_id: Option<String>,
    /// `null` on the recovery paths.
    pub usage: Option<DoneUsage>,
    /// `null` on the recovery paths.
    pub cache_usage: Option<DoneCacheUsage>,
    /// `null` on the recovery paths (kept `Value` so a future attachment shape
    /// slots in without a new type; the recovery paths only ever emit `null`).
    pub attachment_results: Option<serde_json::Value>,
    /// `false` on the recovery paths (no tool loop ran).
    pub tools_executed: bool,
}

/// One streamed chat event — a typed `Event` variant on the push channel. Each
/// variant serializes to v4's matching single-key SSE JSON frame (see the module
/// docs): the enum is `untagged` so `Content("hi")` → `{"content":"hi"}` (not a
/// `{"Content":…}` wrapper), and `Done` flattens its payload beside `done: true`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ChatEvent {
    /// `{status:{…}}`
    Status { status: StatusPayload },
    /// `{content:"…"}`
    Content { content: String },
    /// `{reasoning:"…"}` — cumulative thinking text (client replaces, not
    /// appends).
    Reasoning { reasoning: String },
    /// `{done:true, …}` — the payload spreads flat next to `done: true`.
    Done {
        /// Always `true` — present so the serialized object carries the `done`
        /// key (`#[serde(flatten)]` on the payload puts the rest beside it).
        done: DoneBool,
        #[serde(flatten)]
        payload: DonePayload,
    },
}

/// A unit type that always serializes to the JSON literal `true`, so
/// [`ChatEvent::Done`] carries `"done": true` (v4's `{ done: true, ... }`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoneBool;

impl Serialize for DoneBool {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}

impl ChatEvent {
    /// A status event.
    pub fn status(status: StatusPayload) -> Self {
        ChatEvent::Status { status }
    }

    /// A content-delta event.
    pub fn content(content: impl Into<String>) -> Self {
        ChatEvent::Content {
            content: content.into(),
        }
    }

    /// A reasoning (thinking) event carrying the cumulative reasoning text.
    pub fn reasoning(reasoning: impl Into<String>) -> Self {
        ChatEvent::Reasoning {
            reasoning: reasoning.into(),
        }
    }

    /// A done event.
    pub fn done(payload: DonePayload) -> Self {
        ChatEvent::Done {
            done: DoneBool,
            payload,
        }
    }
}

/// The event-sink seam (v4's `controller` + `encoder`). A service pushes typed
/// [`ChatEvent`]s; delivery is fire-and-forget — `emit` never returns an error
/// that could divert the stream's control flow (v4's `safeEnqueue` swallows a
/// closed controller; `controller.enqueue` in the hot path ignores failure).
pub trait EventSink {
    /// Emit one event. A sink that can no longer deliver simply drops it.
    fn emit(&self, event: ChatEvent);
}

/// A recording [`EventSink`] for the tier-3 differential (and self-tests): it
/// appends every emitted event in order. Cheap-clone (shares one `Vec` behind a
/// mutex) so the same sink can be handed by reference to a service and read back
/// afterwards.
#[derive(Clone, Default)]
pub struct RecordingSink {
    events: std::sync::Arc<std::sync::Mutex<Vec<ChatEvent>>>,
}

impl RecordingSink {
    /// A fresh sink with no recorded events.
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot of the events emitted so far, in emission order.
    pub fn events(&self) -> Vec<ChatEvent> {
        self.events.lock().expect("recording sink poisoned").clone()
    }

    /// The events serialized to their SSE-frame JSON (one `Value` per event, in
    /// order) — the form the differential compares against v4's recorded frames.
    pub fn events_json(&self) -> Vec<serde_json::Value> {
        self.events()
            .iter()
            .map(|e| serde_json::to_value(e).expect("event serializes"))
            .collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: ChatEvent) {
        self.events
            .lock()
            .expect("recording sink poisoned")
            .push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_serializes_as_v4_single_key_frame_with_omitted_optionals() {
        let ev = ChatEvent::status(StatusPayload {
            stage: "sending".into(),
            message: "Sending to Friday...".into(),
            tool_name: None,
            character_name: Some("Friday".into()),
            character_id: Some("char-1".into()),
        });
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({
                "status": {
                    "stage": "sending",
                    "message": "Sending to Friday...",
                    "characterName": "Friday",
                    "characterId": "char-1"
                }
            })
        );
    }

    #[test]
    fn content_and_reasoning_are_bare_single_key_frames() {
        assert_eq!(
            serde_json::to_value(ChatEvent::content("Hel")).unwrap(),
            json!({ "content": "Hel" })
        );
        assert_eq!(
            serde_json::to_value(ChatEvent::reasoning("thinking...")).unwrap(),
            json!({ "reasoning": "thinking..." })
        );
    }

    #[test]
    fn done_flattens_payload_beside_done_true_with_explicit_nulls() {
        // The recovery-path done: message id set, usage/cache/attachments null,
        // toolsExecuted false — v4's `{ done: true, ...data }`.
        let ev = ChatEvent::done(DonePayload {
            message_id: Some("msg-9".into()),
            usage: None,
            cache_usage: None,
            attachment_results: None,
            tools_executed: false,
        });
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({
                "done": true,
                "messageId": "msg-9",
                "usage": null,
                "cacheUsage": null,
                "attachmentResults": null,
                "toolsExecuted": false
            })
        );
    }

    #[test]
    fn recording_sink_captures_order() {
        let sink = RecordingSink::new();
        sink.emit(ChatEvent::content("a"));
        sink.emit(ChatEvent::content("b"));
        sink.emit(ChatEvent::reasoning("r"));
        let json = sink.events_json();
        assert_eq!(
            json,
            vec![
                json!({ "content": "a" }),
                json!({ "content": "b" }),
                json!({ "reasoning": "r" }),
            ]
        );
    }
}
