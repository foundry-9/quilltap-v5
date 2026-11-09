//! The chat-send driver seam (P4.2) — the dyn-compatible boundary between the
//! engine's `ChatSend` dispatch arm and the host's production spine
//! composition.
//!
//! The spine (`process_message` + `execute_turn_chain`) needs the whole
//! provider bundle — thirteen model/seam generics — which only the composing
//! host can construct (the `JobHandler` boxed-future precedent, and the same
//! reason the enclave turn handler is a step-runner closure). So the engine
//! holds an `Arc<dyn ChatSendDriver>` the host's [`EngineAssembler`] returns
//! per assembly: the driver drops on `Lock` with the rest of the assembly, and
//! a read-only embedder (no driver) answers "chat dispatch not assembled".
//!
//! [`EngineAssembler`]: super::engine::EngineAssembler

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::types::{ChatSendResultDto, CoreError, PendingToolResult};

/// The projected `SendMessageOptions` a `ChatSend` dispatch carries (the
/// [`Request::ChatSend`](super::types::Request::ChatSend) fields, untangled
/// from the wire enum).
#[derive(Debug, Clone, Default)]
pub struct ChatSendRequest {
    pub chat_id: String,
    pub content: String,
    pub continue_mode: bool,
    pub responding_participant_id: Option<String>,
    pub target_participant_ids: Option<Vec<String>>,
    pub speaking_as_participant_id: Option<String>,
    pub file_ids: Vec<String>,
    /// v4 continue-mode `nudge` (withholds the "nothing to add" skip option).
    pub nudge: Option<bool>,
    /// v4 `pendingToolResults` — pre-inserted as TOOL messages by the spine.
    pub pending_tool_results: Vec<PendingToolResult>,
    /// P4.D372: v4's `parsed.isEstablishedFact || undefined` — `Some(true)` or
    /// `None` (never forwarded on a continue send).
    pub is_established_fact: Option<bool>,
}

// === P4.D372 ===
/// v4 `sendMessageSchema`'s `superRefine` (`orchestrator.service.ts:132-158`,
/// `db6190b44`), in its order: when the line is an established fact, (1) a
/// whisper → `An established fact is public; it cannot be whispered`, (2)
/// blank content → `An established fact needs words`; then (3) blank content
/// with no files and no tool results → `Message must have content, attached
/// files, or tool results`. Every `.trim()` is JS's ([`crate::jsstr::js_trim`]
/// — U+FEFF and the Unicode space separators trim; Rust's `str::trim` keeps
/// U+FEFF).
///
/// Zod collects EVERY issue and v4's route renders them all
/// (`{error: 'Validation error', details: […]}`); v5's `ChatSend` arm answers
/// the FIRST issue's message as its 400 body — the standing P4.6a shape,
/// extended in v4's precedence. NAMED divergence `SEND_REFINE_ISSUE_LIST`,
/// pinned both ways by `send_message_schema_equivalence` against v4's REAL
/// schema; it retires if a later order renders Zod issue bodies at the
/// dispatch edge. A continue send never reaches it (v4 parses
/// `continueMessageSchema` instead).
pub fn chat_send_refine(
    content: &str,
    target_participant_ids: Option<&[String]>,
    file_ids: &[String],
    pending_tool_results: &[PendingToolResult],
    is_established_fact: Option<bool>,
) -> Option<&'static str> {
    let blank = crate::jsstr::js_trim(content).is_empty();
    if is_established_fact == Some(true) {
        if target_participant_ids.is_some_and(|t| !t.is_empty()) {
            return Some("An established fact is public; it cannot be whispered");
        }
        if blank {
            return Some("An established fact needs words");
        }
    }
    if blank && file_ids.is_empty() && pending_tool_results.is_empty() {
        return Some("Message must have content, attached files, or tool results");
    }
    None
}
// === /P4.D372 ===

/// The boxed future a [`ChatSendDriver`] returns (dyn-compatibility — RPITIT
/// is not dyn-compatible, and the engine stores the driver type-erased).
pub type ChatSendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChatSendResultDto, CoreError>> + Send + 'a>>;

/// One full send turn: dispatch → the spine → frames on the engine's `Event`
/// broadcast → the typed result. Implementations must emit v4's transport-shell
/// error frame themselves on failure (the engine only maps the returned error
/// into the `Response` envelope).
pub trait ChatSendDriver: Send + Sync {
    fn send(&self, req: ChatSendRequest) -> ChatSendFuture<'_>;
}

/// The inputs a swipe-generate (v4 `handleGenerateSwipe`) hands its driver — the
/// route handler has already loaded the chat + resolved ownership + passed the
/// role/`systemSender` guards, so the driver only composes the regeneration.
#[derive(Debug, Clone, Default)]
pub struct SwipeGenerateRequest {
    pub user_id: String,
    /// The owning chat (slim-row read + overlaid `Value`).
    pub chat: Value,
    /// The ASSISTANT `chat_messages` event being regenerated.
    pub target_message: Value,
    /// Every `type:'message'` event in the chat (v4's `allMessages` filter).
    pub all_messages: Vec<Value>,
    /// The user-controlled participant the human is "Speaking As" (v4
    /// `chat.activeTypingParticipantId ?? null`).
    pub active_user_participant_id: Option<String>,
    // P4.D207 OUT-OF-MANDATE — no lane in the `f45a517a9` round owns
    // `api/chat_send.rs`, and the streamed swipe cannot reach its driver
    // without this field: v4's `onProgress` is an argument to the service, and
    // the composing host is the only thing that can build the provider bundle
    // the service runs on. Additive, `Default`-inert, and confined to this
    // hunk. Recorded for the unifier in the P4.D207 lane record.
    /// v4 `onProgress?` — the live narration the SSE leg hands the service
    /// (`route.ts:321-337`). Inert unless the caller asked for `stream: true`,
    /// which is v4's absent callback: *with no callback the generation is
    /// identical, just silent.*
    pub progress: crate::services::regenerate_swipe::SwipeProgressEmitter,
    /// P4.D228 (v4 `profileOverride`, `ce2f1dabf` #77): "Try uncensored"
    /// regenerates on the Concierge's uncensored understudy instead of the
    /// responder's own profile — its WHOLE stored connection-profile row, as
    /// the route resolved it before any frame. The HOST resolves what core
    /// cannot: the model context limit for THIS profile's model and, since
    /// P4.133, the row's own API key (`SwipeProfileOverride.api_key`). The row
    /// rides rather than an id, so the host never re-reads a profile the gate
    /// already chose. `None` is a plain re-roll.
    pub profile_override: Option<Value>,
    /// P4.D228 (v4 `routeTrail`): the trail to persist on the new swipe —
    /// `None` / empty for a plain re-roll (no key written).
    pub route_trail: Option<Vec<Value>>,
}

/// The boxed future a [`SwipeGenerateDriver`] returns (the new swipe `chat_messages`
/// event `Value`, v4's `{ message: newSwipe }` 201 body).
pub type SwipeGenerateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, CoreError>> + Send + 'a>>;

/// The model-boundary seam for swipe generation (v4 `regenerateMessageAsSwipe`,
/// the sibling of `process_message`). Like [`ChatSendDriver`], only the composing
/// host can construct the provider bundle, so the engine holds this type-erased
/// and the host's assembler returns it per assembly.
pub trait SwipeGenerateDriver: Send + Sync {
    fn generate_swipe(&self, req: SwipeGenerateRequest) -> SwipeGenerateFuture<'_>;
}
