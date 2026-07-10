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
}

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
