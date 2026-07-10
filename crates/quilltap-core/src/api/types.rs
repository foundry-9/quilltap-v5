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
use crate::services::creation_progress::CreationProgressFrame;

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
    /// Unlock a passphrase-protected vault (v4 `?action=unlock`).
    Unlock { passphrase: String },
    /// First-run setup (v4 `?action=setup`): mint a pepper, write
    /// `quilltap.dbkey`, provision a fresh encrypted instance (schema + baseline
    /// seed), and assemble. Only valid from `needs-setup`. Returns the pepper
    /// ONCE (shown for the user to save).
    Setup { passphrase: String },
    /// Store an env-provided pepper in a `.dbkey` file (v4 `?action=store`): the
    /// `needs-vault-storage` → `resolved` transition. Only valid from
    /// `needs-vault-storage`.
    #[serde(rename_all = "camelCase")]
    StorePepper { passphrase: String },
    /// Change the passphrase that wraps the pepper (v4 `?action=change-passphrase`):
    /// re-wrap only, no DB re-encryption. Only valid when unlocked (`resolved`).
    /// Either passphrase may be empty (the no-passphrase sentinel).
    #[serde(rename_all = "camelCase")]
    ChangePassphrase {
        old_passphrase: String,
        new_passphrase: String,
    },
    /// Lock the application (v4 `?action=lock` / auto-lock). Tears the engine
    /// down; the vault returns to `needs-passphrase`.
    Lock,
    /// List registered instances (the v4 launcher's instance registry).
    ListInstances,
    /// List the single user's chats, enriched (v4 `GET /api/v1/chats` →
    /// `handleList` + `enrichChatsForList`). The optional params mirror v4's
    /// query string.
    #[serde(rename_all = "camelCase")]
    ListChats {
        /// v4 `?excludeTagIds=a,b` — drop chats carrying any of these tags.
        #[serde(default)]
        exclude_tag_ids: Vec<String>,
        /// v4 `?limit=N` — cap the post-filter list (applied only when `> 0`).
        #[serde(default)]
        limit: Option<i64>,
        /// v4 `?includeAutonomous=true` — include autonomous rooms regardless of
        /// their `runVisibility`.
        #[serde(default)]
        include_autonomous: bool,
    },
    /// The single-chat GET (v4 `GET /api/v1/chats/{id}` → `handleGet` default
    /// branch): the fully-enriched chat + all messages (minus `renderedHtml`).
    #[serde(rename_all = "camelCase")]
    ChatGet { chat_id: String },
    /// The chat-settings read (v4 `GET /api/v1/settings/chat`). Now
    /// default-injects the seed row when none exists (P4.6d).
    ChatSettings,
    /// The chat-settings PUT (v4 `PUT /api/v1/settings/chat`): the partial field
    /// bag folds into the `updateForUser` upsert. Returns the updated row.
    ChatSettingsUpdate { settings: serde_json::Value },
    // --- Connection profiles (v4 connection-profiles/route.ts + [id]/) ---
    /// v4 `GET /api/v1/connection-profiles` — the enriched list.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileList {
        /// v4 `?imageCapable=true` — filter to image-generation-capable providers.
        #[serde(default)]
        image_capable: bool,
    },
    /// v4 `POST /api/v1/connection-profiles` (create) — the field bag.
    ConnectionProfileCreate { profile: serde_json::Value },
    /// v4 `PUT /api/v1/connection-profiles/[id]` — the field bag.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileUpdate {
        profile_id: String,
        profile: serde_json::Value,
    },
    /// v4 `DELETE /api/v1/connection-profiles/[id]`.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileDelete { profile_id: String },
    /// v4 `?action=reorder` — the contract's ordered-id list.
    #[serde(rename_all = "camelCase")]
    ConnectionProfileReorder { ordered_ids: Vec<String> },
    /// v4 `?action=reset-sort`.
    ConnectionProfileResetSort,
    /// v4 `?action=test-connection` — the `{provider, apiKeyId?, baseUrl?}` bag.
    ConnectionProfileTest { profile: serde_json::Value },
    /// v4 `?action=test-message` — the `{provider, apiKeyId?, baseUrl?, modelName,
    /// parameters?}` bag.
    ConnectionProfileTestMessage { profile: serde_json::Value },
    // --- API keys (v4 api-keys/route.ts + [id]/) ---
    /// v4 `GET /api/v1/api-keys` — the masked list.
    ApiKeyList,
    /// v4 `POST /api/v1/api-keys` (create).
    #[serde(rename_all = "camelCase")]
    ApiKeyCreate {
        label: String,
        provider: String,
        api_key: String,
    },
    /// v4 `PUT /api/v1/api-keys/[id]`.
    #[serde(rename_all = "camelCase")]
    ApiKeyUpdate {
        api_key_id: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        is_active: Option<bool>,
        #[serde(default)]
        api_key: Option<String>,
    },
    /// v4 `DELETE /api/v1/api-keys/[id]`.
    #[serde(rename_all = "camelCase")]
    ApiKeyDelete { api_key_id: String },
    /// v4 `POST /api/v1/api-keys/[id]?action=test`.
    #[serde(rename_all = "camelCase")]
    ApiKeyTest {
        api_key_id: String,
        #[serde(default)]
        base_url: Option<String>,
    },
    // --- Providers + models (v4 providers/route.ts + models/route.ts) ---
    /// v4 `GET /api/v1/providers`.
    ProviderList,
    /// v4 `GET /api/v1/models` (+ `?provider=`) — the cached read.
    #[serde(rename_all = "camelCase")]
    ModelList {
        #[serde(default)]
        provider: Option<String>,
    },
    /// v4 `POST /api/v1/models` — the live fetch + cache.
    #[serde(rename_all = "camelCase")]
    ModelFetch {
        provider: String,
        #[serde(default)]
        api_key_id: Option<String>,
        #[serde(default)]
        base_url: Option<String>,
    },
    /// A turn action (v4 `POST /api/v1/chats/{id}/actions?action=turn` →
    /// `handleTurnAction`): nudge/queue/dequeue/query/skipUserTurn.
    #[serde(rename_all = "camelCase")]
    ChatTurnAction {
        chat_id: String,
        action: String,
        #[serde(default)]
        participant_id: Option<String>,
    },
    /// Edit a message's content (v4 `PUT /api/v1/messages/{id}`).
    #[serde(rename_all = "camelCase")]
    MessageEdit { message_id: String, content: String },
    /// Delete a message / swipe group (v4 `DELETE /api/v1/messages/{id}`) — the
    /// memory-cascade confirmation protocol.
    #[serde(rename_all = "camelCase")]
    MessageDelete {
        message_id: String,
        /// v4 `?memoryAction=` — DELETE_MEMORIES / KEEP_MEMORIES /
        /// REGENERATE_MEMORIES / ASK_EVERY_TIME.
        #[serde(default)]
        memory_action: Option<String>,
        /// v4 `?skipConfirmation=true`.
        #[serde(default)]
        skip_confirmation: bool,
    },
    /// Swipe a message (v4 `POST /api/v1/messages/{id}?action=swipe`): switch
    /// (`swipeIndex` present) or generate (absent — needs the model driver).
    #[serde(rename_all = "camelCase")]
    MessageSwipe {
        message_id: String,
        #[serde(default)]
        swipe_index: Option<i64>,
    },
    /// The general chat edit (v4 `PUT /api/v1/chats/{id}` → `processChatUpdates`):
    /// the Salon pause/resume + title path. `chat` is the partial field bag
    /// (`updateChatSchema`).
    #[serde(rename_all = "camelCase")]
    ChatUpdate {
        chat_id: String,
        chat: serde_json::Value,
    },
    /// Start impersonating a participant (v4 `POST …?action=impersonate`).
    #[serde(rename_all = "camelCase")]
    ChatImpersonate {
        chat_id: String,
        participant_id: String,
    },
    /// Stop impersonating (v4 `POST …?action=stop-impersonate`); the optional new
    /// connection profile flips the participant back to `controlledBy:'llm'`.
    #[serde(rename_all = "camelCase")]
    ChatStopImpersonate {
        chat_id: String,
        participant_id: String,
        #[serde(default)]
        new_connection_profile_id: Option<String>,
    },
    /// Set the active typing/speaking participant (v4 `POST …?action=set-active-speaker`).
    #[serde(rename_all = "camelCase")]
    ChatSetActiveSpeaker {
        chat_id: String,
        participant_id: String,
    },
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
        /// v4 continue-mode `nudge` — the human explicitly summoned this character
        /// (withholds the "nothing to add" skip option for this turn).
        #[serde(default)]
        nudge: Option<bool>,
        /// v4 `pendingToolResults` — user-initiated tool results pre-inserted as
        /// TOOL messages before the user message.
        #[serde(default)]
        pending_tool_results: Vec<PendingToolResult>,
    },
    /// Create a chat and run the full seed sequence (v4 `POST /api/v1/chats` →
    /// `handleCreate`). The dispatch payload's create fields (everything but
    /// `type`) are flattened into `request` and handed to the driver, which
    /// deserializes them into a
    /// [`ChatCreateRequest`](crate::services::chat_create::ChatCreateRequest).
    /// Creation-progress frames ride the [`Event`] channel (scope-tagged by
    /// `progressId`); the dispatch reply is the created chat.
    ChatCreate {
        #[serde(flatten)]
        request: serde_json::Map<String, serde_json::Value>,
    },
}

/// Typed DTO per variant (the uniffi payoff). `Error` carries the one
/// cross-cutting error envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Response {
    Health(HealthDto),
    UnlockState(UnlockStateDto),
    /// v4 setup response — the minted pepper (shown once) + the save-it message.
    Setup(SetupResultDto),
    /// The empty-body success ack (v4 `successResponse({})`) — `storePepper` /
    /// `changePassphrase`.
    Ack(AckDto),
    Instances(InstancesDto),
    /// v4 `handleList`'s `cleanEnrichedChats` output — the enriched chat list.
    Chats(Vec<crate::services::chat_enrichment::EnrichedChatSummary>),
    ChatSend(ChatSendResultDto),
    /// The v4 `POST /api/v1/chats` 201 body (`{ chat: {...} }`).
    ChatCreate(ChatCreateResultDto),
    /// The single-chat GET / chat PUT body (`{ chat: {...} }`).
    Chat(ChatWrapDto),
    /// v4 `GET /api/v1/settings/chat` body (the raw settings object).
    ChatSettings(serde_json::Value),
    /// v4 `handleTurnAction` body.
    TurnAction(serde_json::Value),
    /// v4 message edit / swipe body (`{ message: {...} }`).
    Message(serde_json::Value),
    /// v4 message-delete body (the confirmation body OR `{ success, memoriesDeleted }`).
    MessageDelete(serde_json::Value),
    /// v4 impersonation-verb body (`{ success, ... }`).
    ChatImpersonation(serde_json::Value),
    // --- Settings surface (P4.6d) ---
    /// v4 connection-profiles list (`{profiles, count}`).
    ConnectionProfiles(serde_json::Value),
    /// v4 connection-profile create/update/get (`{profile}`).
    ConnectionProfile(serde_json::Value),
    /// v4 connection test-connection / test-message body.
    ConnectionTest(serde_json::Value),
    /// v4 api-keys list (`{apiKeys, count}`).
    ApiKeys(serde_json::Value),
    /// v4 api-key create/update/get (`{apiKey}`).
    ApiKey(serde_json::Value),
    /// v4 api-key test body (`{valid, error?}`).
    ApiKeyTest(serde_json::Value),
    /// v4 providers listing (`{providers, count}`).
    Providers(serde_json::Value),
    /// v4 models read/fetch body.
    Models(serde_json::Value),
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

/// v4 `?action=setup` success body: the pepper is returned ONCE (the user must
/// save it — it is never displayed again).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResultDto {
    pub pepper: String,
    pub message: String,
}

/// The empty success body (v4 `successResponse({})`). Serializes to `{}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AckDto {}

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

/// The typed result of a `ChatCreate` dispatch. Serializes to v4's 201 body,
/// `{ "chat": { ...chat, "participants": [EnrichedParticipantSummary] } }`:
/// `chat` is the full hydrated chat row whose own `participants` array has been
/// REPLACED by the enriched participant summaries (the driver merges
/// [`ChatCreateResult`](crate::services::chat_create::ChatCreateResult)'s two
/// halves before constructing this). The SPA's chat-create call and the P4.5 TS
/// contract mirror both consume this shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCreateResultDto {
    /// The created chat, with `participants` = the enriched summaries.
    pub chat: serde_json::Value,
}

/// The `{ chat: {...} }` wrapper the single-chat GET + chat PUT return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatWrapDto {
    pub chat: serde_json::Value,
}

/// v4 `pendingToolResultSchema` element — a user-initiated tool result the send
/// route pre-inserts as a TOOL message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingToolResult {
    pub tool: String,
    pub success: bool,
    pub result: String,
    pub prompt: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
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
    /// v4 `unauthorized` (HTTP 401): a wrong passphrase on `changePassphrase`.
    Unauthorized,
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
    /// A chat-creation progress frame (D6, "The Green Room" —
    /// `services::creation_progress`), scope-tagged by `progress_id`. Serializes
    /// to v4's `{kind, …, ts}` frame shape; the [`Event`] envelope adds the
    /// `progressId` tag.
    CreationProgress(CreationProgressFrame),
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

    /// A creation-progress frame scope-tagged by `progress_id` (D6). The
    /// transport uses this to replay a `CreationProgressBus` backlog onto a new
    /// `/api/events` stream.
    pub fn creation_progress(
        progress_id: impl Into<String>,
        frame: CreationProgressFrame,
    ) -> Event {
        Event {
            chat_id: None,
            room_id: None,
            progress_id: Some(progress_id.into()),
            payload: EventPayload::CreationProgress(frame),
        }
    }
}
