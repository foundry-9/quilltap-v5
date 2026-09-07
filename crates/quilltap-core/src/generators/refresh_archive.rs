//! v4 `POST /api/v1/characters/[id]?action=refresh-archive`
//! (`app/api/v1/characters/[id]/handlers/post.ts:336-370`, `p4.9k`, P4.9K1) —
//! re-render and re-embed every conversation the character appears in.
//!
//! The arm is the whole service: every chat the character participates in gets
//! a `CONVERSATION_RENDER` job with `fullReembed: true` through the SAME
//! enqueue the Rename/Replace execute leg uses; a chat that already has a
//! pending render job is reused, not duplicated, and counts as queued (v4's
//! `enqueueConversationRender` returns `{isNew: false}` rather than throwing).
//! v4's per-chat `catch {}` swallows a failed enqueue in silence — reproduced:
//! that chat is simply not counted.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::services::queue_service::enqueue_conversation_render;

/// The response body: `{ queued: 0 }` for a character in no chat at all,
/// else `{ queued, total }` — two DIFFERENT key sets, as v4 answers them.
pub async fn refresh_archive(db: &Db, user_id: &str, character_id: &str) -> Result<Value, DbError> {
    let cid = character_id.to_string();
    let chats = db.read_main(move |conn| chats_read::find_by_character_id(conn, &cid))?;

    if chats.is_empty() {
        return Ok(json!({ "queued": 0 }));
    }

    let mut queued = 0i64;
    for chat in &chats {
        let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
            continue;
        };
        // v4: `try { await enqueueConversationRender(...); queued++ } catch {}`
        // — "Skip chats that already have a pending render job" is the comment,
        // but the dedupe answers `isNew: false` without throwing, so a pending
        // chat DOES count. Only a genuine enqueue failure is skipped.
        if enqueue_conversation_render(db, user_id, chat_id, Some(true))
            .await
            .is_ok()
        {
            queued += 1;
        }
    }

    tracing::info!(
        character_id = %character_id,
        user_id = %user_id,
        total_chats = chats.len(),
        queued,
        "[Characters v1] Conversation archive refresh queued"
    );

    Ok(json!({ "queued": queued, "total": chats.len() }))
}
