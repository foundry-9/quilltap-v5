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

/// The next-speaker `turn` info on a finalizer done event (v4's
/// `NextSpeakerInfo`). Present on the finalizer callsite; absent on the recovery
/// paths. Field order matches v4's `NextSpeakerInfo` literal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneTurn {
    pub next_speaker_id: Option<String>,
    pub reason: String,
    pub cycle_complete: bool,
    pub is_users_turn: bool,
}

/// A three-state field for a JSON key that is either **absent** (omitted from the
/// object), present as `null`, or present as a value — the exact distinction v4's
/// object-spread makes between an omitted key (`x || undefined`) and an explicit
/// `null` (`x || null`). Plain `Option<T>` can only model omit-vs-value; the
/// finalizer's `reasoningContent: x || null` / `reasoningSegments: x` are ALWAYS
/// present (`null` or value) while the recovery paths omit them entirely.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Omittable<T> {
    /// The key is not written at all (v4 `undefined` dropped by `JSON.stringify`).
    #[default]
    Absent,
    /// The key is written as JSON `null`.
    Null,
    /// The key is written with this value.
    Value(T),
}

impl<T: Serialize> Serialize for Omittable<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            // `Absent` is only ever reached through `skip_serializing_if`; if it
            // is serialized directly it must still be a null (defensive).
            Omittable::Absent | Omittable::Null => s.serialize_none(),
            Omittable::Value(v) => v.serialize(s),
        }
    }
}

impl<T> Omittable<T> {
    fn is_absent(&self) -> bool {
        matches!(self, Omittable::Absent)
    }
}

/// The `done` event payload (v4 `encodeDoneEvent`'s `data`). The recovery paths
/// emit it with `usage` / `cache_usage` / `attachment_results` all `null` and
/// `tools_executed: false`, and leave the rest absent; the finalizer callsite
/// fills the full set (`participant_id`, `turn`, `provider`, `model_name`,
/// `is_silent_message`, `reasoning_content`, `reasoning_segments`).
///
/// Field order matches v4's finalizer done literal (`{ done: true, messageId,
/// participantId, usage, cacheUsage, attachmentResults, toolsExecuted, turn,
/// provider, modelName, isSilentMessage, reasoningContent, reasoningSegments }`);
/// the recovery paths omit every `skip_serializing_if` field, producing exactly
/// v4's shorter recovery frame. Construct the recovery form with
/// `DonePayload { message_id, usage, cache_usage, attachment_results,
/// tools_executed, ..Default::default() }`.
///
/// Serializes flat next to `done: true` (see [`ChatEvent`]), so a serialized
/// [`ChatEvent::Done`] is byte-identical to v4's `{ done: true, ...data }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    /// The persisted message id (v4 always sets it on the recovery + finalizer
    /// done events).
    pub message_id: Option<String>,
    /// The responding participant (v4 finalizer `participantId`). Absent on the
    /// recovery paths (v4 leaves it off there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    /// `null` on the recovery paths; the real usage on the finalizer path.
    pub usage: Option<DoneUsage>,
    /// `null` on the recovery paths.
    pub cache_usage: Option<DoneCacheUsage>,
    /// `null` on the recovery paths (kept `Value` so a future attachment shape
    /// slots in without a new type).
    pub attachment_results: Option<serde_json::Value>,
    /// `false` on the recovery paths (no tool loop ran); on the finalizer path,
    /// `toolMessages.length > 0`.
    pub tools_executed: bool,
    /// `true` on the "nothing to add" skip done frame (v4 `handleTurnSkip`'s
    /// `skipped: true`), else ABSENT. Declared right after `tools_executed` so the
    /// skip frame matches v4's `{ …, toolsExecuted, skipped, skippedParticipantId,
    /// provider, modelName }` order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
    /// Participant ID of the character who passed — present only alongside
    /// `skipped`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_participant_id: Option<String>,
    /// The next-speaker info (finalizer only). Absent on recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<DoneTurn>,
    /// The effective provider (finalizer only). Absent on recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The effective model name (finalizer only). Absent on recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// `true` on the Courier parked-placeholder `done` frame (v4
    /// `pendingExternalTurn: true`), else ABSENT. Serializes right after
    /// `model_name` so the frame matches v4's `{ …, provider, modelName,
    /// pendingExternalTurn }` order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_external_turn: Option<bool>,
    /// `true` when the responding participant is silent, else ABSENT — v4's
    /// `isSilentMessage: … === 'silent' || undefined` (never `false`). Absent on
    /// recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_silent_message: Option<bool>,
    /// `true` on the orchestrator's **empty-response** done frame (v4
    /// `emptyResponse: true`), else ABSENT. Only the empty-response terminal branch
    /// of `processMessage` sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_response: Option<bool>,
    /// The empty-response reason string (v4 `emptyResponseReason`) — present only
    /// alongside `empty_response`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_response_reason: Option<String>,
    /// The full reasoning text (finalizer only) — ALWAYS present there as `null`
    /// or a string (`reasoningContent || null`); ABSENT on recovery.
    #[serde(skip_serializing_if = "Omittable::is_absent")]
    pub reasoning_content: Omittable<String>,
    /// The positioned reasoning blocks (finalizer only) — ALWAYS present there as
    /// `null` or an array (`rebasedReasoning`); ABSENT on recovery.
    #[serde(skip_serializing_if = "Omittable::is_absent")]
    pub reasoning_segments: Omittable<Vec<DoneReasoningSegment>>,
}

/// A reasoning segment as it appears in the done event (v4 `ReasoningSegment` —
/// `anchorOffset` / `content` / `seq`). Separate from
/// [`crate::services::primary_stream::ReasoningSegment`] so the event layer owns
/// its serialization shape (camelCase keys in schema order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneReasoningSegment {
    pub anchor_offset: usize,
    pub content: String,
    pub seq: u64,
}

/// One streamed chat event — a typed `Event` variant on the push channel. Each
/// variant serializes to v4's matching single-key SSE JSON frame (see the module
/// docs): the enum is `untagged` so `Content("hi")` → `{"content":"hi"}` (not a
/// `{"Content":…}` wrapper), and `Done` flattens its payload beside `done: true`.
// `Eq` is intentionally NOT derived: the `CarinaAnswer` variant carries a
// `serde_json::Value` (which is not `Eq`). `PartialEq` is enough for the tests /
// differential (they compare via `events_json()` anyway).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ChatEvent {
    /// `{status:{…}}`
    Status { status: StatusPayload },
    /// `{content:"…"}`
    Content { content: String },
    /// `{reasoning:"…"}` — cumulative thinking text (client replaces, not
    /// appends).
    Reasoning { reasoning: String },
    /// `{carinaAnswer: <message>}` — a Carina reference-answer message posted
    /// mid-turn (v4 `encodeCarinaAnswerEvent`). The value is the full posted
    /// `MessageEvent` object (kept as a raw `Value` — the message shape is owned
    /// by the carina writer, which produces it).
    CarinaAnswer {
        #[serde(rename = "carinaAnswer")]
        carina_answer: serde_json::Value,
    },
    /// `{confirmationResult: {…}}` — the answer-confirmation state for a
    /// just-streamed message (v4 `encodeConfirmationResultEvent`). The finalizer
    /// emits it whenever a verdict (incl. `null`) was resolved — the user-driven
    /// skip path emits `confirmed: null`; the active path (wave 4) emits the real
    /// verdict (and the replacement `content` on a revision).
    ConfirmationResult {
        #[serde(rename = "confirmationResult")]
        confirmation_result: ConfirmationResultPayload,
    },
    /// `{turnStart: true, participantId, characterName, chainDepth}` (v4
    /// `encodeTurnStartEvent`). Emitted by `processMessage` (chainDepth 0, the
    /// first-turn analog) and by the chain driver before each chained turn.
    TurnStart {
        #[serde(rename = "turnStart")]
        turn_start: TrueBool,
        #[serde(flatten)]
        payload: TurnStartPayload,
    },
    /// `{turnComplete: true, participantId, messageId, chainDepth}` (v4
    /// `encodeTurnCompleteEvent`). Emitted by the chain driver after a chained turn.
    TurnComplete {
        #[serde(rename = "turnComplete")]
        turn_complete: TrueBool,
        #[serde(flatten)]
        payload: TurnCompletePayload,
    },
    /// `{chainComplete: true, reason, nextSpeakerId, chainDepth}` (v4
    /// `encodeChainCompleteEvent`). Emitted by the chain driver when the chain ends.
    ChainComplete {
        #[serde(rename = "chainComplete")]
        chain_complete: TrueBool,
        #[serde(flatten)]
        payload: ChainCompletePayload,
    },
    /// `{toolsDetected, toolNames, toolArguments}` — the tool-detection frame v4
    /// `processToolCalls` enqueues before dispatching the batch (a bare object, not
    /// a single-key wrapper). `tool_arguments` carries each call's raw arguments
    /// object (order-preserving `Value`).
    ToolsDetected {
        #[serde(rename = "toolsDetected")]
        tools_detected: usize,
        #[serde(rename = "toolNames")]
        tool_names: Vec<String>,
        #[serde(rename = "toolArguments")]
        tool_arguments: Vec<serde_json::Value>,
    },
    /// `{toolResult:{index, name, success, result, error?}}` — the per-tool result
    /// frame v4 `processToolCalls` enqueues after each dispatch. `result` is the
    /// raw tool result (`unknown`); `error` is present ONLY on failure (v4 carries
    /// the human-readable error text there).
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: ToolResultPayload,
    },
    /// `{pendingExternalTurn:true, messageId, participantId, characterName}` — the
    /// Courier parked-placeholder frame (v4 `encodePendingExternalTurnEvent`). Emitted
    /// by `dispatch_courier_transport` right before its `done` frame.
    PendingExternalTurn {
        #[serde(rename = "pendingExternalTurn")]
        pending_external_turn: TrueBool,
        #[serde(flatten)]
        payload: PendingExternalTurnPayload,
    },
    /// `{hostAnnouncement: <message>}` — a Host announcement persisted mid-turn
    /// (v4 `encodeHostAnnouncementEvent`; currently the "nothing to add"
    /// turn-pass note), so the Salon can surface the Host bubble immediately.
    /// Carries the full posted `MessageEvent` object (a raw `Value` — the
    /// message shape is owned by the Host writer, which produces it).
    HostAnnouncement {
        #[serde(rename = "hostAnnouncement")]
        host_announcement: serde_json::Value,
    },
    /// `{pascalResult: <message>}` — a Pascal `run_custom` outcome persisted
    /// mid-turn (v4 `encodePascalResultEvent`), so the Salon can splice the
    /// croupier's bubble in the instant it lands rather than waiting for the
    /// post-turn refetch. Carries the full posted `MessageEvent` object (a raw
    /// `Value` — the shape is owned by the Pascal writer).
    PascalResult {
        #[serde(rename = "pascalResult")]
        pascal_result: serde_json::Value,
    },
    /// `{error, errorType, details}` — v4 `encodeErrorEvent(error, errorType,
    /// details)` emitted MID-STREAM by an orchestrator that continues afterwards
    /// (the help-chat loop's per-participant `processing_error`, P4.9I2A). The
    /// TOP-LEVEL `fatal_error` frame stays the transport-shell
    /// `EventPayload::ChatError`, emitted by the host on `Err` — this variant is
    /// for the failures a loop reports and survives. Same bytes as v4's frame.
    Error {
        error: String,
        #[serde(rename = "errorType")]
        error_type: String,
        details: String,
    },
    /// `{done:true, …}` — the payload spreads flat next to `done: true`. Boxed:
    /// the full finalizer payload is by far the largest variant
    /// (clippy::large_enum_variant), and every event is heap-bound for the
    /// channel anyway.
    Done {
        /// Always `true` — present so the serialized object carries the `done`
        /// key (`#[serde(flatten)]` on the payload puts the rest beside it).
        done: DoneBool,
        #[serde(flatten)]
        payload: Box<DonePayload>,
    },
}

/// A unit type that always serializes to the JSON literal `true` — the `turnStart`
/// / `turnComplete` / `chainComplete` discriminator key (v4's `{ <key>: true, … }`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrueBool;

impl Serialize for TrueBool {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}

/// The `turnStart` frame payload (v4 `encodeTurnStartEvent`'s `data`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartPayload {
    pub participant_id: String,
    pub character_name: String,
    pub chain_depth: i64,
}

/// The `turnComplete` frame payload (v4 `encodeTurnCompleteEvent`'s `data`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompletePayload {
    pub participant_id: String,
    /// v4 passes `chainResult.messageId || ''` — the empty string on a null id.
    pub message_id: String,
    pub chain_depth: i64,
    /// "Nothing to add" turn-skipping: whether the chained turn was passed. The
    /// Salon chain driver ALWAYS passes `chainResult.skipped === true` (b90cd1f5),
    /// so the key is present on every Salon chained turn's frame — `Some`. The
    /// help-chat loop (P4.9I2A) emits v4's `{participantId, messageId,
    /// chainDepth}` with NO `skipped` key — `None` omits it (v4's `data` spread
    /// carries only the keys the caller passed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
}

/// The `chainComplete` frame payload (v4 `encodeChainCompleteEvent`'s `data`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainCompletePayload {
    /// The v4 chain-stop reason string (`user_turn` / `paused` / `max_depth` /
    /// `max_time` / `error` / `no_next_speaker` / `cycle_complete`).
    pub reason: String,
    pub next_speaker_id: Option<String>,
    pub chain_depth: i64,
}

/// The `pendingExternalTurn` frame payload (v4 `encodePendingExternalTurnEvent`'s
/// `data`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingExternalTurnPayload {
    pub message_id: String,
    pub participant_id: String,
    pub character_name: String,
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
            payload: Box::new(payload),
        }
    }

    /// A Courier parked-placeholder event (v4 `encodePendingExternalTurnEvent`).
    pub fn pending_external_turn(payload: PendingExternalTurnPayload) -> Self {
        ChatEvent::PendingExternalTurn {
            pending_external_turn: TrueBool,
            payload,
        }
    }

    /// A Carina reference-answer event carrying the full posted message object.
    pub fn carina_answer(message: serde_json::Value) -> Self {
        ChatEvent::CarinaAnswer {
            carina_answer: message,
        }
    }

    /// A Pascal `run_custom` outcome event carrying the full posted message
    /// object (v4 `encodePascalResultEvent`).
    pub fn pascal_result(message: serde_json::Value) -> Self {
        ChatEvent::PascalResult {
            pascal_result: message,
        }
    }

    /// `{hostAnnouncement: <message>}` (v4 `encodeHostAnnouncementEvent`).
    pub fn host_announcement(message: serde_json::Value) -> Self {
        ChatEvent::HostAnnouncement {
            host_announcement: message,
        }
    }

    /// An answer-confirmation result event.
    pub fn confirmation_result(confirmation_result: ConfirmationResultPayload) -> Self {
        ChatEvent::ConfirmationResult {
            confirmation_result,
        }
    }

    /// v4 `encodeErrorEvent(error, errorType, details)` as a mid-stream frame.
    pub fn error(
        error: impl Into<String>,
        error_type: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        ChatEvent::Error {
            error: error.into(),
            error_type: error_type.into(),
            details: details.into(),
        }
    }

    /// A `turnStart` event.
    pub fn turn_start(payload: TurnStartPayload) -> Self {
        ChatEvent::TurnStart {
            turn_start: TrueBool,
            payload,
        }
    }

    /// A `turnComplete` event.
    pub fn turn_complete(payload: TurnCompletePayload) -> Self {
        ChatEvent::TurnComplete {
            turn_complete: TrueBool,
            payload,
        }
    }

    /// A `chainComplete` event.
    pub fn chain_complete(payload: ChainCompletePayload) -> Self {
        ChatEvent::ChainComplete {
            chain_complete: TrueBool,
            payload,
        }
    }

    /// The tool-detection frame (`processToolCalls`, before the dispatch loop).
    pub fn tools_detected(tool_names: Vec<String>, tool_arguments: Vec<serde_json::Value>) -> Self {
        ChatEvent::ToolsDetected {
            tools_detected: tool_names.len(),
            tool_names,
            tool_arguments,
        }
    }

    /// A per-tool result frame (`processToolCalls`, after each dispatch).
    pub fn tool_result(tool_result: ToolResultPayload) -> Self {
        ChatEvent::ToolResult { tool_result }
    }
}

/// The `toolResult` frame payload (v4 `processToolCalls`'s `toolResultPayload`):
/// `{ index, name, success, result, ...(!success ? { error } : {}) }`. `result` is
/// the raw tool result (`unknown` → [`serde_json::Value`]); `error` carries the
/// human-readable failure text and is present only on failure.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolResultPayload {
    pub index: usize,
    pub name: String,
    pub success: bool,
    pub result: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The `confirmationResult` payload (v4 `encodeConfirmationResultEvent`'s
/// `result`): `{ messageId, confirmed, revised, notes, ...(revised ? {content} :
/// {}) }`. `confirmed` is `null` on the could-not-verify / user-driven paths;
/// `content` is present only when the re-affirmation rewrote the reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationResultPayload {
    pub message_id: String,
    /// `null` = could not verify / user-driven (unverifiable).
    pub confirmed: Option<bool>,
    pub revised: bool,
    /// `null` when no discrepancies were recorded.
    pub notes: Option<String>,
    /// The replacement reply text — present ONLY when `revised` (v4 spreads the
    /// key conditionally).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
            ..Default::default()
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
    fn finalizer_done_carries_the_full_payload_in_v4_field_order() {
        // The finalizer callsite: participantId, real usage, turn, provider,
        // modelName, isSilentMessage (present), reasoningContent (null),
        // reasoningSegments (a block). Field order matches v4's finalizer literal.
        let ev = ChatEvent::done(DonePayload {
            message_id: Some("m1".into()),
            participant_id: Some("p1".into()),
            usage: Some(DoneUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(20),
                total_tokens: Some(30),
            }),
            cache_usage: None,
            attachment_results: None,
            tools_executed: false,
            skipped: None,
            skipped_participant_id: None,
            turn: Some(DoneTurn {
                next_speaker_id: None,
                reason: "user_turn".into(),
                cycle_complete: true,
                is_users_turn: true,
            }),
            provider: Some("ANTHROPIC".into()),
            model_name: Some("claude".into()),
            pending_external_turn: None,
            is_silent_message: Some(true),
            empty_response: None,
            empty_response_reason: None,
            reasoning_content: Omittable::Null,
            reasoning_segments: Omittable::Value(vec![DoneReasoningSegment {
                anchor_offset: 5,
                content: "thinking".into(),
                seq: 0,
            }]),
        });
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({
                "done": true,
                "messageId": "m1",
                "participantId": "p1",
                "usage": { "promptTokens": 10, "completionTokens": 20, "totalTokens": 30 },
                "cacheUsage": null,
                "attachmentResults": null,
                "toolsExecuted": false,
                "turn": { "nextSpeakerId": null, "reason": "user_turn", "cycleComplete": true, "isUsersTurn": true },
                "provider": "ANTHROPIC",
                "modelName": "claude",
                "isSilentMessage": true,
                "reasoningContent": null,
                "reasoningSegments": [ { "anchorOffset": 5, "content": "thinking", "seq": 0 } ]
            })
        );
    }

    #[test]
    fn skip_done_frame_matches_v4_handle_turn_skip_key_order() {
        // The "nothing to add" skip-path done frame (v4 `handleTurnSkip`'s
        // `encodeDoneEvent` call): `skipped`/`skippedParticipantId` sit between
        // `toolsExecuted` and `provider`, `messageId` is an explicit null, and
        // none of the finalizer-only keys appear. Asserted on the serialized
        // STRING (the SSE wire is byte-level).
        let ev = ChatEvent::done(DonePayload {
            message_id: None,
            participant_id: Some("p9".into()),
            usage: None,
            cache_usage: None,
            attachment_results: Some(serde_json::Value::Null),
            tools_executed: false,
            skipped: Some(true),
            skipped_participant_id: Some("p9".into()),
            provider: Some("ANTHROPIC".into()),
            model_name: Some("claude".into()),
            ..Default::default()
        });
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            "{\"done\":true,\"messageId\":null,\"participantId\":\"p9\",\"usage\":null,\"cacheUsage\":null,\"attachmentResults\":null,\"toolsExecuted\":false,\"skipped\":true,\"skippedParticipantId\":\"p9\",\"provider\":\"ANTHROPIC\",\"modelName\":\"claude\"}"
        );
    }

    #[test]
    fn carina_answer_is_a_single_key_frame() {
        let msg = json!({ "type": "message", "id": "carina-1", "role": "ASSISTANT" });
        let ev = ChatEvent::carina_answer(msg.clone());
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({ "carinaAnswer": msg })
        );
    }

    #[test]
    fn pascal_result_is_a_single_key_frame() {
        // v4 `encodePascalResultEvent` → `{pascalResult: <message>}`.
        let msg = json!({ "type": "message", "id": "pascal-1", "systemSender": "pascal" });
        let ev = ChatEvent::pascal_result(msg.clone());
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({ "pascalResult": msg })
        );
    }

    #[test]
    fn turn_frames_serialize_as_v4_single_key_frames() {
        assert_eq!(
            serde_json::to_value(ChatEvent::turn_start(TurnStartPayload {
                participant_id: "p1".into(),
                character_name: "Friday".into(),
                chain_depth: 0,
            }))
            .unwrap(),
            json!({ "turnStart": true, "participantId": "p1", "characterName": "Friday", "chainDepth": 0 })
        );
        assert_eq!(
            serde_json::to_value(ChatEvent::turn_complete(TurnCompletePayload {
                participant_id: "p1".into(),
                message_id: "m1".into(),
                chain_depth: 1,
                skipped: Some(false),
            }))
            .unwrap(),
            json!({ "turnComplete": true, "participantId": "p1", "messageId": "m1", "chainDepth": 1, "skipped": false })
        );
        assert_eq!(
            serde_json::to_value(ChatEvent::chain_complete(ChainCompletePayload {
                reason: "cycle_complete".into(),
                next_speaker_id: None,
                chain_depth: 2,
            }))
            .unwrap(),
            json!({ "chainComplete": true, "reason": "cycle_complete", "nextSpeakerId": null, "chainDepth": 2 })
        );
    }

    #[test]
    fn empty_response_done_carries_the_reason() {
        let ev = ChatEvent::done(DonePayload {
            message_id: None,
            participant_id: Some("p1".into()),
            usage: None,
            cache_usage: None,
            attachment_results: None,
            tools_executed: false,
            empty_response: Some(true),
            empty_response_reason: Some("empty".into()),
            provider: Some("ANTHROPIC".into()),
            model_name: Some("claude".into()),
            ..Default::default()
        });
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({
                "done": true,
                "messageId": null,
                "participantId": "p1",
                "usage": null,
                "cacheUsage": null,
                "attachmentResults": null,
                "toolsExecuted": false,
                "emptyResponse": true,
                "emptyResponseReason": "empty",
                "provider": "ANTHROPIC",
                "modelName": "claude"
            })
        );
    }

    #[test]
    fn tool_frames_serialize_as_v4_bare_and_single_key_frames() {
        // Detection frame: a bare object, not a single-key wrapper.
        let ev = ChatEvent::tools_detected(
            vec!["search".into(), "roll".into()],
            vec![json!({ "query": "cats" }), json!({ "sides": 6 })],
        );
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({
                "toolsDetected": 2,
                "toolNames": ["search", "roll"],
                "toolArguments": [{ "query": "cats" }, { "sides": 6 }]
            })
        );
        // Success result frame: no `error` key.
        let ok = ChatEvent::tool_result(ToolResultPayload {
            index: 0,
            name: "roll".into(),
            success: true,
            result: json!([1, 2, 3]),
            error: None,
        });
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            json!({ "toolResult": { "index": 0, "name": "roll", "success": true, "result": [1, 2, 3] } })
        );
        // Failure result frame: `error` present, `result` often null.
        let err = ChatEvent::tool_result(ToolResultPayload {
            index: 1,
            name: "search".into(),
            success: false,
            result: json!(null),
            error: Some("Error: boom".into()),
        });
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            json!({ "toolResult": { "index": 1, "name": "search", "success": false, "result": null, "error": "Error: boom" } })
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
