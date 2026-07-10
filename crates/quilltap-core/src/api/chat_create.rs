//! The chat-creation driver seam (P4.4u2b) — the dyn-compatible boundary
//! between the engine's `ChatCreate` dispatch arm and the host's production
//! chat-creation spine composition.
//!
//! Like [`ChatSendDriver`](super::chat_send::ChatSendDriver), the spine
//! ([`handle_create`](crate::services::chat_create::handle_create)) needs the
//! whole provider bundle — three model-boundary generics plus the cheap-LLM
//! executor, the lifecycle seam, and the api-key resolver — which only the
//! composing host can assemble. So the engine holds an
//! `Arc<dyn ChatCreateDriver>` the host's [`EngineAssembler`] returns per
//! assembly: it drops on `Lock`, and a read-only embedder (no driver) answers
//! "chat create not assembled".
//!
//! [`EngineAssembler`]: super::engine::EngineAssembler

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::types::{ChatCreateResultDto, CoreError};

/// The raw [`Request::ChatCreate`](super::types::Request::ChatCreate) fields,
/// carried through as a JSON object so the driver can deserialize them into a
/// [`ChatCreateRequest`](crate::services::chat_create::ChatCreateRequest)
/// (camelCase). Keeping the wire shape a `Value` here means new create fields
/// grow the request struct WITHOUT touching this seam.
#[derive(Debug, Clone)]
pub struct ChatCreateDriverRequest {
    /// A JSON object of the create fields (the dispatch payload minus `type`).
    pub raw: Value,
}

/// The boxed future a [`ChatCreateDriver`] returns (dyn-compatibility — RPITIT
/// is not dyn-compatible, and the engine stores the driver type-erased).
pub type ChatCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChatCreateResultDto, CoreError>> + Send + 'a>>;

/// One full chat-creation flow: dispatch → the spine → the seed whispers +
/// (optional) greeting → the 201 body. Creation-progress frames ride the
/// engine's `Event` broadcast (scope-tagged by `progressId`); the dispatch reply
/// is the typed [`ChatCreateResultDto`].
pub trait ChatCreateDriver: Send + Sync {
    fn create(&self, req: ChatCreateDriverRequest) -> ChatCreateFuture<'_>;
}

#[cfg(test)]
mod tests {
    use super::super::types::{ChatCreateResultDto, Request, Response};
    use serde_json::json;

    /// The dispatch wire shape: `{ "type": "chatCreate", ...createFields }`
    /// deserializes into `Request::ChatCreate` with the create fields captured in
    /// the flattened map (minus `type`), and a driver can rebuild the raw object.
    #[test]
    fn chat_create_request_flattens() {
        let wire = json!({
            "type": "chatCreate",
            "title": "New Chat",
            "participants": [{ "type": "CHARACTER", "characterId": "c1" }],
            "chatType": "salon",
        });
        let req: Request = serde_json::from_value(wire).unwrap();
        let Request::ChatCreate { request } = req else {
            panic!("expected ChatCreate");
        };
        assert_eq!(
            request.get("title").and_then(|v| v.as_str()),
            Some("New Chat")
        );
        assert!(request.get("participants").is_some());
        assert_eq!(
            request.get("chatType").and_then(|v| v.as_str()),
            Some("salon")
        );
        // `type` is consumed by the tag — never in the flattened map.
        assert!(request.get("type").is_none());
    }

    /// The `ChatCreate` response serializes to v4's 201 body, `{ "chat": {...} }`.
    #[test]
    fn chat_create_response_shape() {
        let dto = ChatCreateResultDto {
            chat: json!({ "id": "chat-1", "participants": [] }),
        };
        let out = serde_json::to_value(Response::ChatCreate(dto)).unwrap();
        assert_eq!(out["type"], json!("chatCreate"));
        assert_eq!(out["data"]["chat"]["id"], json!("chat-1"));
    }
}
