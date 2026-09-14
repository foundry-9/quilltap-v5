//! **`project_chat_transcript`** — v4 `lib/chat/transcript-projection.ts`
//! (`5029075bb`, +324, a pure EXTRACTION out of `handlers/get.ts`, −271).
//!
//! The one place a stored chat transcript is turned into the rows the Salon
//! renders: attachments resolved, and the off-scene character cards an
//! announcement or a Carina answer needs in order to draw an avatar for
//! someone who is not a participant.
//!
//! It has two readers and must have exactly one implementation:
//!
//!   - the chat GET ([`super::salon::chat_get`]), which embeds the whole
//!     transcript in the chat object the Salon loads at mount;
//!   - the conditional re-read ([`super::chat_transcript::chat_transcript`],
//!     `GET /api/v1/messages?chatId=…&action=transcript`) that a realtime
//!     `chats` hint drives.
//!
//! A second serialization of a message is the drift this module exists to
//! prevent — the same reasoning that keeps message bodies off the realtime
//! channel. Change the shape here and both readers change together.
//!
//! **The extraction's neutrality was MEASURED, not assumed** (§R.6(5)): the
//! `salon_reads` oracle regenerated at `f4ad2c8d1` and at `31436bae4` differs
//! by exactly five ADDED `transcriptVersion` keys, with the key ORDER
//! identical everywhere else — so v4 moved this code without changing a byte
//! of what it produces, and v5's move is held to the same standard (the family
//! is green against the BASELINE oracle after the move and before the key).
//!
//! v5 keeps `project_message` and `resolve_message_attachments` in
//! [`super::salon`] and calls them from here, exactly as v4's module imports
//! `getFilePath` / `getCharacterDetail` / `renderMarkdownToHtml` rather than
//! inlining them. `renderedHtml` stays OMITTED on both readers — the locked
//! markdown-render divergence, unchanged by this move.

use serde_json::{json, Value};

use super::salon::{project_message, resolve_message_attachments, s};
use crate::db::DbError;
use crate::services::carina_query::BRAHMA_CARINA_ANSWERER_ID;
use crate::services::chat_enrichment;

/// What both readers get back: the projected message rows and the off-scene
/// author cards. v4 returns `{ messages, offSceneCharacters }` from the same
/// function.
pub struct TranscriptProjection {
    pub messages: Vec<Value>,
    pub off_scene_characters: Vec<Value>,
}

/// v4 `projectChatTranscript(chatId, chatMetadata, repos, userId)`.
///
/// `participants` is v4's `chatMetadata.participants` — passed in rather than
/// re-read so the move stays literal (the chat GET already has it enriched
/// alongside, and both callers derive it from the same chat row).
pub fn project_chat_transcript(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    participants: &[Value],
    chat_id: &str,
) -> Result<TranscriptProjection, DbError> {
    use crate::db::chats_messages_read;

    // All messages, projected (minus renderedHtml). Attachments are resolved from
    // linked `files` (+ image sha256/linkSummary); the mount-file (Scriptorium)
    // `event.attachments` probe is a tracked deferral.
    let events = chats_messages_read::get_messages(main, chat_id)?;
    let mut messages: Vec<Value> = Vec::new();
    for e in &events {
        if e.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let attachments = match e.get("id").and_then(Value::as_str) {
            Some(mid) => resolve_message_attachments(main, mount, mid)?,
            None => Value::Array(Vec::new()),
        };
        messages.push(project_message(e, attachments));
    }

    // Off-scene characters (customAnnouncer.characterId + carinaMeta.answererId
    // non-participants), resolved via getCharacterDetail (4-field subset).
    let participant_char_ids: std::collections::HashSet<String> = participants
        .iter()
        .filter_map(|p| s(p, "characterId"))
        .collect();
    let mut off_scene_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push_off =
        |id: &str, seen: &mut std::collections::HashSet<String>, out: &mut Vec<String>| {
            if !participant_char_ids.contains(id) && seen.insert(id.to_string()) {
                out.push(id.to_string());
            }
        };
    for m in &messages {
        if let Some(id) = m
            .get("customAnnouncer")
            .and_then(|c| c.get("characterId"))
            .and_then(Value::as_str)
        {
            push_off(id, &mut seen, &mut off_scene_ids);
        }
        if let Some(id) = m
            .get("carinaMeta")
            .and_then(|c| c.get("answererId"))
            .and_then(Value::as_str)
        {
            if id != BRAHMA_CARINA_ANSWERER_ID {
                push_off(id, &mut seen, &mut off_scene_ids);
            }
        }
    }
    let mut off_scene_characters: Vec<Value> = Vec::new();
    for cid in &off_scene_ids {
        if let Some(detail) =
            chat_enrichment::get_character_detail(main, mount, cid, Some(chat_id))?
        {
            off_scene_characters.push(json!({
                "id": detail.id,
                "name": detail.name,
                "title": detail.title,
                "avatarUrl": detail.avatar_url,
            }));
        }
    }
    Ok(TranscriptProjection {
        messages,
        off_scene_characters,
    })
}
