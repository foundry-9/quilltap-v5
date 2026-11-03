//! v4 `GET /api/v1/chats/[id]?action=absent-mentions` (`app/api/v1/chats/[id]/
//! actions/absent-mentions.ts`, `9a88f862f`) — for each workspace character
//! named in the chat but not in it, the message that first names them, the name
//! as written there, and their card.
//!
//! Drives the Salon's name chips: the first mention gets a chip, and hovering it
//! shows the card — the same particulars the model gets in its referent ledger
//! ([`crate::services::absent_mentions`]).

use serde_json::{json, Value};

use super::types::{db_error_response, ErrorKind, Response};
use crate::db::runtime::Db;
use crate::services::absent_mentions::{find_absent_mentions, presence_of};
use crate::services::off_scene::OffSceneCharacterCard;

/// v4 `handleGetAbsentMentions(chatId, {user, repos})`. Answers the BARE
/// `{mentions: […]}` (`successResponse` is `NextResponse.json`, never nested
/// under `data`); a missing chat is v4's `notFound('Chat')`.
///
/// The chat is read through the fallback reader v4's `findById` is (a failed
/// read logs `Error finding entity by ID` and answers the 404). The characters
/// are the overlaid workspace read; on a DEGRADED mount index that read
/// refuses, and the refusal answers through [`db_error_response`] (P4.166's
/// entity-class mapping — v4's middleware 500).
pub async fn chat_absent_mentions(db: &Db, chat_id: &str, user_id: &str) -> Response {
    let Some(chat) = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(|c| crate::db::chats_read::find_by_id(c, chat_id))
    }) else {
        return Response::error(ErrorKind::NotFound, "Chat not found");
    };

    let messages = match db.read_main(|c| crate::db::chats_messages_read::get_messages(c, chat_id))
    {
        Ok(m) => m,
        Err(e) => return db_error_response(e),
    };
    let candidates: Vec<OffSceneCharacterCard> = match db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::characters_read::find_by_user_id(main, mount, user_id)
        })
    }) {
        Ok(rows) => rows
            .iter()
            .filter_map(OffSceneCharacterCard::from_value)
            .collect(),
        Err(e) => return db_error_response(e),
    };
    let active_typing = chat
        .get("activeTypingParticipantId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let persona = match crate::services::user_identity_resolver::resolve_user_identity(
        db,
        user_id,
        &chat,
        active_typing.as_deref(),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => return db_error_response(e),
    };

    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let presence = presence_of(
        &participants,
        chat.get("chatType").and_then(Value::as_str),
        Some(persona.name.as_str()),
    );
    let mentions = find_absent_mentions(&messages, &candidates, &presence, Some(&persona.name));

    tracing::debug!(
        chatId = %chat_id,
        mentions = mentions.len(),
        context = "absent-mentions",
        "[Chats v1] Read absent-character mentions"
    );
    Response::ChatAbsentMentions(json!({ "mentions": mentions }))
}
