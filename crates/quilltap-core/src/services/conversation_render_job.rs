//! The `CONVERSATION_RENDER` job handler (P4.6BM) — a port of v4's
//! `lib/background-jobs/handlers/conversation-render.ts`
//! (`handleConversationRender`).
//!
//! Deterministically renders a chat to Markdown (no LLM), stores it on
//! `chats.renderedMarkdown`, upserts one `conversation_chunks` row per
//! interchange, and re-enqueues `EMBEDDING_GENERATE` for the chunks that still
//! lack an embedding. The rendering itself is
//! [`super::conversation_markdown::render_conversation_markdown`] (tier-1
//! exact); this module is the DB choreography around it.
//!
//! ## Before this handler existed, its jobs died
//!
//! v5 has been minting `CONVERSATION_RENDER` jobs since P4.9E3B wired the manual
//! `?action=render-conversation` button (`services::chat_admin` →
//! [`enqueue_conversation_render`]), with no handler registered — so every press
//! produced a job that retried three times and went DEAD. That is the same shape
//! as dogfood finding #35, one job type over.
//!
//! ## v4 details reproduced deliberately
//!
//!   - **A missing chat is a completed job, not a failure** (v4 warns and
//!     `return`s), as is a chat with zero events.
//!   - **`renderedMarkdown` is written WITHOUT touching `updatedAt`.** v4's
//!     `chats.update` preserves `updatedAt` unless the caller names it, and this
//!     caller deliberately does not: a background render must not reorder every
//!     recents list.
//!   - **The upsert preserves existing embeddings.** Only content /
//!     participantNames / messageIds are rewritten, so a re-render of an already
//!     embedded chunk keeps its vector and is NOT re-enqueued (unless
//!     `fullReembed`).
//!   - **The whole embedding-enqueue block is caught.** An enqueue failure warns
//!     and the job still completes — the render is the valuable half.
//!   - **The default profile is `isDefault` OR, failing that, the FIRST row** of
//!     `embeddingProfiles.findAll()` — insertion order, not a sort. No profile at
//!     all → nothing is enqueued and the job still completes.
//!   - **The payload is a bare cast, not Zod.** Both fields decode leniently,
//!     exactly as [`super::embedding_generate_job::EmbeddingGeneratePayload`]
//!     does.

use serde_json::{json, Value};
use uuid::Uuid;

use super::conversation_markdown::{
    render_conversation_markdown, ConversationMetadata, RenderEvent,
};
use super::queue_service::enqueue_embedding_generate;
use crate::clock::now_iso;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, DbError};

/// The decoded `CONVERSATION_RENDER` payload (v4 `ConversationRenderPayload`).
/// v4 performs a bare `as` cast — no validation — so every field decodes
/// leniently here.
#[derive(Debug, Clone)]
pub struct ConversationRenderPayload {
    pub chat_id: String,
    /// `true` → re-enqueue an embedding for EVERY interchange chunk, not just
    /// the ones still missing one. Absent renders as JS `undefined` (falsy).
    pub full_reembed: bool,
}

impl ConversationRenderPayload {
    pub fn from_json(payload: &Value) -> Self {
        Self {
            chat_id: payload
                .get("chatId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            // v4 `payload.fullReembed || !chunk.embedding` — a JS truthiness
            // test, so a non-`true` value (absent, null, false) is falsy.
            full_reembed: payload
                .get("fullReembed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// Handle a `CONVERSATION_RENDER` job (v4 `handleConversationRender`).
///
/// `Ok(())` completes the job — including the missing-chat and no-messages arms
/// (v4 `return`s there). `Err(message)` fails it, so the runner retries with the
/// ported backoff to `maxAttempts` → DEAD.
/// `now_iso` is the injected wall clock: it feeds the render header's
/// `Current time:` line and the chunk upserts' timestamps (v4 reads
/// `new Date()` for both). Production passes [`crate::clock::now_iso`]; the
/// differential pins it, which is what makes `chats.renderedMarkdown` compare
/// byte-exact rather than needing normalization.
pub async fn handle_conversation_render(
    db: &Db,
    user_id: &str,
    payload: &ConversationRenderPayload,
    now_iso: &str,
) -> Result<(), String> {
    handle_inner(db, user_id, payload, now_iso)
        .await
        .map_err(|e| e.into())
}

/// A [`DbError`] rendered for the job's `lastError` (v4's
/// `error instanceof Error ? error.message : String(error)`).
struct RenderError(String);

impl From<DbError> for RenderError {
    fn from(e: DbError) -> Self {
        RenderError(format!("{e}"))
    }
}

impl From<RenderError> for String {
    fn from(e: RenderError) -> String {
        e.0
    }
}

async fn handle_inner(
    db: &Db,
    user_id: &str,
    payload: &ConversationRenderPayload,
    now_iso: &str,
) -> Result<(), RenderError> {
    // 1. Load the chat (v4 :24-31) — missing is a WARN and a completed job.
    let chat_id = payload.chat_id.clone();
    let chat = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id))?;
    let Some(chat) = chat else {
        tracing::warn!(
            target: "quilltap::jobs",
            chat_id = %payload.chat_id,
            "[ConversationRender] Chat not found, skipping",
        );
        return Ok(());
    };

    // 2. participantId -> display name (v4 :34-46). A participant with a
    //    character resolves to that character's name; a USER-controlled
    //    participant with no resolvable character falls back to "User". Note the
    //    ORDER of v4's two ifs: a user-controlled participant WITH a resolvable
    //    character keeps the character's name.
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut character_names: Vec<(String, String)> = Vec::new();
    for participant in &participants {
        let Some(pid) = participant.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(character_id) = participant.get("characterId").and_then(Value::as_str) {
            let cid = character_id.to_string();
            let character = db.read_main(|main| {
                db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
            })?;
            if let Some(name) = character
                .as_ref()
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
            {
                character_names.push((pid.to_string(), name.to_string()));
            }
        }
        if participant.get("controlledBy").and_then(Value::as_str) == Some("user")
            && !character_names.iter().any(|(k, _)| k == pid)
        {
            character_names.push((pid.to_string(), "User".to_string()));
        }
    }

    // 3. All events (v4 :49-53) — an empty chat renders nothing at all.
    let chat_id = payload.chat_id.clone();
    let events = db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id))?;
    if events.is_empty() {
        return Ok(());
    }
    let messages: Vec<RenderEvent> = events.iter().map(event_from_json).collect();

    // 4. Render (v4 :56-61).
    let metadata = ConversationMetadata {
        conversation_id: payload.chat_id.clone(),
        title: chat
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_at: chat
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        last_updated_at: chat
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    let result =
        render_conversation_markdown(&messages, &character_names, Some(&metadata), now_iso);

    // 5. Persist the markdown WITHOUT bumping updatedAt (v4 :63-66).
    let (cid, markdown) = (payload.chat_id.clone(), result.markdown.clone());
    db.write(move |ws| {
        ws.main().chats().update(
            &cid,
            &crate::db::chats::ChatUpdate {
                rendered_markdown: Some(markdown),
                updated_at: None,
                ..Default::default()
            },
        )
    })
    .await?;

    // 6. Upsert one chunk per interchange (v4 :69-78). v4 reads `new Date()`
    //    once before the loop; every upsert in a run therefore shares one
    //    timestamp — though `_update`/`_create` mint their own anyway, which is
    //    what actually lands. One `now` here matches both.
    for interchange in &result.interchanges {
        let input = crate::db::conversation_chunks::CcUpsert {
            chat_id: payload.chat_id.clone(),
            interchange_index: interchange.index as f64,
            content: interchange.content.clone(),
            participant_names: interchange.participant_names.clone(),
            message_ids: interchange.message_ids.clone(),
            // v4's render input carries no `embedding` key — the update arm
            // preserves the stored vector, or NULLs it when the content at this
            // index changed (Bug 17 sub-chunking), so the re-enqueue re-embeds.
            embedding: None,
        };
        let new_id = Uuid::new_v4().to_string();
        let now_for_write = now_iso.to_string();
        db.write(move |ws| {
            ws.main()
                .conversation_chunks()
                .upsert(&input, &new_id, &now_for_write)
        })
        .await?;
    }

    // 7. Re-enqueue embeddings (v4 :82-117). The WHOLE block is caught: an
    //    enqueue failure warns and the render job still completes.
    if !result.interchanges.is_empty() {
        if let Err(e) = enqueue_embeddings(db, user_id, payload, &result.interchanges).await {
            tracing::warn!(
                target: "quilltap::jobs",
                chat_id = %payload.chat_id,
                error = %e,
                "[ConversationRender] Failed to enqueue embedding, continuing",
            );
        }
    }

    Ok(())
}

/// v4's step-7 body (`:83-106`), lifted so its `try`/`catch` is one call site.
async fn enqueue_embeddings(
    db: &Db,
    user_id: &str,
    payload: &ConversationRenderPayload,
    interchanges: &[super::conversation_markdown::InterchangeInfo],
) -> Result<(), DbError> {
    // v4: `embeddingProfiles.findAll()`, then `find(p => p.isDefault) || [0]` —
    // insertion order, no sort. No profile at all → enqueue nothing.
    let profiles = db.read_main(crate::db::embedding_profiles::find_all_full_json)?;
    let default_profile = profiles
        .iter()
        .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true))
        .or_else(|| profiles.first());
    let Some(profile_id) = default_profile
        .and_then(|p| p.get("id"))
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let profile_id = profile_id.to_string();

    for interchange in interchanges {
        let (cid, index) = (payload.chat_id.clone(), interchange.index as f64);
        let chunk = db.read_main(move |conn| {
            crate::db::conversation_chunks::ConversationChunksRepository::new(conn)
                .find_by_interchange_index(&cid, index)
        })?;
        let Some(chunk) = chunk else { continue };
        if payload.full_reembed || !chunk.has_embedding {
            enqueue_embedding_generate(
                db,
                user_id,
                json!({
                    "entityType": "CONVERSATION_CHUNK",
                    "entityId": chunk.id,
                    "chatId": payload.chat_id,
                    "profileId": profile_id,
                }),
            )
            .await?;
        }
    }
    Ok(())
}

/// v4 `triggerConversationRender`
/// (`lib/services/chat-message/memory-trigger.service.ts:239`) — the per-turn
/// enqueue, fired from the send path once a turn (and any turn chain) has
/// produced content.
///
/// The payload is `{ chatId }` alone — NO `fullReembed` key — so a per-turn
/// render only embeds the chunks that still lack an embedding; the manual
/// re-render button is the one that passes `fullReembed: true`.
///
/// **Every failure is swallowed** (v4 catches and logs, and its caller catches
/// again): the turn's content is already persisted and streamed, and a queue
/// hiccup must not surface as a failed send. That is why this returns `()`.
pub async fn trigger_conversation_render(db: &Db, user_id: &str, chat_id: &str) {
    if let Err(e) =
        crate::services::queue_service::enqueue_conversation_render(db, user_id, chat_id, None)
            .await
    {
        tracing::warn!(
            target: "quilltap::chat",
            chat_id = %chat_id,
            error = %e,
            "Failed to trigger conversation render",
        );
    }
}

/// A `CONVERSATION_RENDER` [`crate::services::job_runner::JobHandler`]. Like
/// `EMBEDDING_REFIT` it needs no model/wire seam — only the DB — so the host
/// registers it in the seam-free set rather than through the spine.
pub struct ConversationRenderHandler {
    /// `None` reads the real clock at handle time; `Some(s)` pins it (the
    /// differential's runner path).
    pub now_iso: Option<String>,
}

impl crate::services::job_runner::JobHandler for ConversationRenderHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = ConversationRenderPayload::from_json(&payload_json);
            let now = self.now_iso.clone().unwrap_or_else(now_iso);
            // v4 passes `job.userId` — the row's own value.
            match handle_conversation_render(db, &job.user_id, &payload, &now).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

/// Marshal one `getMessages` row into the renderer's input shape. The renderer
/// reads only these six fields; everything else on the event is irrelevant to it.
fn event_from_json(v: &Value) -> RenderEvent {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    RenderEvent {
        id: s("id").unwrap_or_default(),
        type_: s("type"),
        role: s("role"),
        content: s("content"),
        participant_id: s("participantId"),
        created_at: s("createdAt").unwrap_or_default(),
    }
}
