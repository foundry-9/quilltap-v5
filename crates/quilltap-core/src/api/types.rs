//! The pure contract types of the Core API boundary (Phase-4 D8): the
//! `Request`/`Response` enums, the scope-tagged `Event` envelope, the DTOs,
//! and the error/readiness vocabulary. No IO here — every transport (axum,
//! CLI, Tauri, uniffi later) marshals these and nothing else.
//!
//! Growth rule (D7): variants are **action-centric** and added when a consumer
//! (an SPA vertical, a CLI subcommand, a test) needs them — never enumerated
//! speculatively. v4's ~124 routes / ~162 action verbs are the *checklist*
//! for eventual coverage, not the wire shape.
//!
//! Wire tagging: `Request` is internally tagged (`{"type": "unlock",
//! "passphrase": "…"}` — the natural dispatch-JSON shape); `Response` is
//! adjacently tagged (`{"type": "chats", "data": […]}`) so list payloads can
//! carry a tag. The HTTP envelope semantics (v4 `lib/api/responses.ts`,
//! the 503/423 readiness statuses) are the transport's marshalling concern
//! (P4.2), not this layer's.

use serde::{Deserialize, Serialize};

use crate::services::chat_events::ChatEvent;

// ============================================================================
// Readiness (v4 DbKeyState — lib/startup/dbkey.ts)
// ============================================================================

/// v4's `DbKeyState`, verbatim strings on the wire. `Resolved` and
/// `NeedsVaultStorage` are both **operational** (the pepper is in hand; the
/// latter just recommends storing it in a `.dbkey` file) — v4
/// `startupState.isPepperResolved` treats them identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PepperState {
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "needs-setup")]
    NeedsSetup,
    #[serde(rename = "needs-passphrase")]
    NeedsPassphrase,
    #[serde(rename = "needs-vault-storage")]
    NeedsVaultStorage,
}

impl PepperState {
    /// v4 `isPepperResolved`: whether the engine can run (pepper available).
    pub fn is_operational(self) -> bool {
        matches!(self, PepperState::Resolved | PepperState::NeedsVaultStorage)
    }
}

// ============================================================================
// Request / Response
// ============================================================================

/// One variant per user-meaningful operation (api-boundary.md Part 1). The
/// always-available family (health, unlock, instances) works while the vault
/// is locked; everything else is readiness-gated in dispatch (D2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    /// Liveness + readiness (v4 `GET /health` + `startup-status` essentials).
    Health,
    /// The unlock-family state read (v4 `GET /api/v1/system/unlock`).
    UnlockState,
    /// Unlock a passphrase-protected vault (v4 `?action=unlock`). The
    /// `setup`/`store`/`change-passphrase` actions land with the P4.4
    /// unlock-service backfill (setup also needs schema creation).
    Unlock { passphrase: String },
    /// Lock the application (v4 `?action=lock` / auto-lock). Tears the engine
    /// down; the vault returns to `needs-passphrase`.
    Lock,
    /// List registered instances (the v4 launcher's instance registry).
    ListInstances,
    /// List the single user's chats (v4 `GET /api/v1/chats`), summarized.
    ListChats,
    /// Send a chat message and run the full turn (v4
    /// `POST /api/v1/chats/{id}/messages` → `handleSendMessage`). The stream
    /// frames ride the [`Event`] channel (chat-scoped); the dispatch reply is
    /// the typed result of the initial turn. Fields project v4's
    /// `SendMessageOptions`.
    #[serde(rename_all = "camelCase")]
    ChatSend {
        chat_id: String,
        #[serde(default)]
        content: String,
        #[serde(default)]
        continue_mode: bool,
        #[serde(default)]
        responding_participant_id: Option<String>,
        #[serde(default)]
        target_participant_ids: Option<Vec<String>>,
        #[serde(default)]
        speaking_as_participant_id: Option<String>,
        #[serde(default)]
        file_ids: Vec<String>,
    },
}

/// Typed DTO per variant (the uniffi payoff). `Error` carries the one
/// cross-cutting error envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Response {
    Health(HealthDto),
    UnlockState(UnlockStateDto),
    Instances(InstancesDto),
    Chats(Vec<ChatSummaryDto>),
    ChatSend(ChatSendResultDto),
    Error(CoreError),
}

impl Response {
    /// Shorthand for an error response.
    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Response {
        Response::Error(CoreError {
            kind,
            message: message.into(),
            pepper_state: None,
        })
    }

    /// The readiness-gate refusal (D2): dispatch answers this for every
    /// ready-gated variant while the vault is locked; the HTTP transport maps
    /// it to 503/423 with the setup URL.
    pub fn locked(pepper_state: PepperState) -> Response {
        Response::Error(CoreError {
            kind: ErrorKind::Locked,
            message: "The database is locked. Unlock it to continue.".to_string(),
            pepper_state: Some(pepper_state),
        })
    }
}

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthDto {
    /// Always `"ok"` if dispatch answered at all.
    pub status: String,
    /// The core/app version string.
    pub version: String,
    /// Whether the engine is assembled and serving (pepper operational).
    pub ready: bool,
    pub pepper_state: PepperState,
}

/// v4 `GET /api/v1/system/unlock` body: `{ state, hasUserPassphrase,
/// autoLockMinutes }` — `autoLockMinutes` only populated when unlocked and
/// the user's auto-lock setting is enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockStateDto {
    pub state: PepperState,
    pub has_user_passphrase: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_lock_minutes: Option<f64>,
}

/// One registered instance from the launcher registry (`instances.json`).
/// Never carries the stored passphrase — only whether one is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDto {
    pub name: String,
    pub path: String,
    pub is_default: bool,
    pub has_passphrase: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancesDto {
    pub instances: Vec<InstanceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_instance: Option<String>,
}

/// A chat list row — a projection of the (differential-verified) chat read
/// shape, not new marshaling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummaryDto {
    pub id: String,
    pub title: String,
    pub chat_type: String,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The typed result of a `ChatSend` dispatch — a projection of the spine's
/// [`ProcessMessageResult`](crate::services::message_finalizer::ProcessMessageResult)
/// (the frames themselves ride the [`Event`] channel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResultDto {
    /// The assistant message id the turn minted (or targeted, in continue mode).
    pub message_id: String,
    pub has_content: bool,
    pub is_multi_character: bool,
    pub is_paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_participant_id: Option<String>,
}

// ============================================================================
// Errors
// ============================================================================

/// The cross-transport error envelope. `kind` follows v4's response-helper
/// vocabulary (`lib/api/responses.ts`); the transport maps kinds to statuses
/// (bad-request → 400, not-found → 404, locked → 423/503 per D2, internal →
/// 500).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreError {
    pub kind: ErrorKind,
    pub message: String,
    /// Populated on readiness refusals so the client can route to the right
    /// unlock/setup screen without a second round-trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pepper_state: Option<PepperState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    BadRequest,
    NotFound,
    Locked,
    Internal,
}

// ============================================================================
// Events (D3): one global stream, every event scope-tagged
// ============================================================================

/// The server-push envelope. Scope ids identify what the payload is about so
/// one global stream per client suffices (D3); the payload flattens into the
/// envelope, so a chat frame serializes as v4's SSE frame plus its scope tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_id: Option<String>,
    #[serde(flatten)]
    pub payload: EventPayload,
}

/// The event families (phase-4.md "Event families"). P4.0 defines the one
/// vocabulary that already exists — the chat stream frames
/// ([`ChatEvent`], byte-identical to v4's SSE `StreamChunkData`). Creation
/// progress (D6) and the low-vocabulary progress frames join as their
/// producers are ported.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EventPayload {
    Chat(ChatEvent),
    /// v4's transport-shell error frame (`handleStreamError` →
    /// `encodeErrorEvent`: `{error, errorType, details}`). The ported
    /// `process_message` propagates its error to the caller; the TRANSPORT owns
    /// the frame — the spine driver emits this when the turn errors, exactly
    /// where v4's stream shell does.
    ChatError(ChatErrorPayload),
}

/// v4 `encodeErrorEvent(encoder, error, errorType, details)`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatErrorPayload {
    /// v4 hardcodes `'Failed to generate response'` at the send-path shell.
    pub error: String,
    #[serde(rename = "errorType")]
    pub error_type: String,
    pub details: String,
}

impl Event {
    /// A chat-scoped stream frame.
    pub fn chat(chat_id: impl Into<String>, frame: ChatEvent) -> Event {
        Event {
            chat_id: Some(chat_id.into()),
            room_id: None,
            progress_id: None,
            payload: EventPayload::Chat(frame),
        }
    }

    /// A chat-scoped transport-shell error frame (v4 `handleStreamError`).
    pub fn chat_error(chat_id: impl Into<String>, payload: ChatErrorPayload) -> Event {
        Event {
            chat_id: Some(chat_id.into()),
            room_id: None,
            progress_id: None,
            payload: EventPayload::ChatError(payload),
        }
    }
}
