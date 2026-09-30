//! The `CONVERSATION_RENDER` job handler (P4.6BM) — a port of v4's
//! `lib/background-jobs/handlers/conversation-render.ts`
//! (`handleConversationRender`).
//!
//! Deterministically renders a chat (no LLM) through the shared
//! [`super::scriptorium_render::render_chat_conversation`], upserts one
//! `conversation_chunks` row per interchange, and re-enqueues
//! `EMBEDDING_GENERATE` for the chunks that still lack an embedding. **The
//! Markdown itself is not persisted** (v4 `f7f3d7bf0` dropped
//! `chats.renderedMarkdown`): it is re-rendered on demand wherever it is read,
//! and only the chunks are stored. The job no longer writes the chat row at
//! all (v4's test asserts `chats.update` is never called).
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
//!     `return`s), as is a chat with zero events (v4 `f7f3d7bf0` logs that at
//!     DEBUG; it used to return silently).
//!   - **The upsert preserves existing embeddings.** Only content /
//!     participantNames / messageIds are rewritten, so a re-render of an already
//!     embedded chunk keeps its vector and is NOT re-enqueued (unless
//!     `fullReembed`).
//!   - **The whole embedding-enqueue block is caught.** An enqueue failure warns
//!     and the job still completes — the render is the valuable half.
//!   - **The embedding profile is the `isDefault` one ONLY** (v4 `d553f72a`) — no
//!     first-row fallback: with none marked, nothing is enqueued and the chunks
//!     wait for the startup reconcile.
//!   - **The payload is a bare cast, not Zod.** Both fields decode leniently,
//!     exactly as [`super::embedding_generate_job::EmbeddingGeneratePayload`]
//!     does.

use serde_json::{json, Value};
use uuid::Uuid;

use super::queue_service::enqueue_embedding_generate;
use super::scriptorium_render::render_chat_conversation;
use crate::clock::now_iso;
use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};

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
/// `job_id` is the row's id (v4 logs `jobId` on every line). `now_iso` is the
/// injected wall clock: it feeds the render header's `Current time:` line and
/// the chunk upserts' timestamps (v4 reads `new Date()` for both). Production
/// passes [`crate::clock::now_iso`]; the differential pins it, which is what
/// makes the chunk rows compare byte-exact rather than needing normalization.
///
/// This is the production entry: the chunk text's timestamps are v4's
/// zone-less `toLocale*` renders, so they resolve the HOST's zone (P4.119 —
/// dogfood #121: a UTC render of a chunk v4 wrote in the host zone changed its
/// text, and the v4-faithful upsert then nulled its embedding). Tests and
/// differentials call [`handle_conversation_render_in_zone`] explicitly.
pub async fn handle_conversation_render(
    db: &Db,
    job_id: &str,
    user_id: &str,
    payload: &ConversationRenderPayload,
    now_iso: &str,
) -> Result<(), String> {
    handle_conversation_render_in_zone(
        db,
        job_id,
        user_id,
        payload,
        now_iso,
        &crate::host_zone::system_display_zone(),
    )
    .await
}

/// [`handle_conversation_render`] with the render's display zone passed in.
pub async fn handle_conversation_render_in_zone(
    db: &Db,
    job_id: &str,
    user_id: &str,
    payload: &ConversationRenderPayload,
    now_iso: &str,
    zone: &jiff::tz::TimeZone,
) -> Result<(), String> {
    handle_inner(db, job_id, user_id, payload, now_iso, zone)
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
    job_id: &str,
    user_id: &str,
    payload: &ConversationRenderPayload,
    now_iso: &str,
    zone: &jiff::tz::TimeZone,
) -> Result<(), RenderError> {
    let started = std::time::Instant::now();

    // 1. Load the chat (v4 :24-31) — missing is a WARN and a completed job.
    // v4's `repos.chats.findById` is the fallback `_findById`, so a FAILED read
    // logs the repository's ERROR and takes this same not-found arm (the job
    // completes; it does not fail and retry) — the `97b25fc53` unification
    // review.
    let chat_id = payload.chat_id.clone();
    let chat = db.read_main(move |conn| Ok(chats_read::find_by_id_or_none(conn, &chat_id)))?;
    let Some(chat) = chat else {
        tracing::warn!(
            target: "quilltap::jobs",
            jobId = %job_id,
            chatId = %payload.chat_id,
            "[ConversationRender] Chat not found, skipping",
        );
        return Ok(());
    };

    // 2. Render from the stored messages (v4 :37-44). The Markdown itself is
    //    not kept — only the interchange chunks are stored.
    let result = db.read_main(|conn| render_chat_conversation(conn, &chat, now_iso, zone))?;
    let Some(result) = result else {
        tracing::debug!(
            target: "quilltap::jobs",
            jobId = %job_id,
            chatId = %payload.chat_id,
            "[ConversationRender] Chat has no events, nothing to render",
        );
        return Ok(());
    };

    // 3. Upsert one chunk per interchange (v4 :46-56). v4 reads `new Date()`
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

    // 4. Re-enqueue embeddings (v4 :58-98). The WHOLE block is caught: an
    //    enqueue failure warns and the render job still completes.
    if !result.interchanges.is_empty() {
        if let Err(e) = enqueue_embeddings(db, user_id, payload, &result.interchanges).await {
            tracing::warn!(
                target: "quilltap::jobs",
                jobId = %job_id,
                chatId = %payload.chat_id,
                error = %e,
                "[ConversationRender] Failed to enqueue embedding, continuing",
            );
        }
    }

    tracing::info!(
        target: "quilltap::jobs",
        jobId = %job_id,
        chatId = %payload.chat_id,
        interchangeCount = result.interchanges.len(),
        // JS `string.length` — UTF-16 code units.
        markdownLength = result.markdown.encode_utf16().count(),
        durationMs = started.elapsed().as_millis() as u64,
        "[ConversationRender] Conversation rendered successfully",
    );

    Ok(())
}

/// v4's step-4 body (`:83-106`), lifted so its `try`/`catch` is one call site.
async fn enqueue_embeddings(
    db: &Db,
    user_id: &str,
    payload: &ConversationRenderPayload,
    interchanges: &[super::conversation_markdown::InterchangeInfo],
) -> Result<(), DbError> {
    // Default profile ONLY (v4 `d553f72a`) — every vector in the instance must
    // come from the same profile. With none marked, chunks wait for the startup
    // reconcile rather than embedding under an arbitrary one.
    let profiles = db.read_main(crate::db::embedding_profiles::find_all_full_json)?;
    let default_profile = profiles
        .iter()
        .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true));
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
            chatId = %chat_id,
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
            match handle_conversation_render(db, &job.id, &job.user_id, &payload, &now).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

/// P4.D235 (v4 `f7f3d7bf0`): the handler's three lines — the NEW no-events
/// DEBUG, the restored success INFO (v4 had it before this commit; v5 never
/// emitted it), and the not-found WARN now carrying `jobId` — each with its
/// silence leg, over the shared scriptorium-tool fixture.
#[cfg(test)]
mod log_tests {
    use super::*;
    use crate::test_support::global_capture::capture;
    use crate::tools::annotations::log_tests::{db, CHAT, NOW, OTHER_CHAT, USER};

    const JOB: &str = "00000000-0000-4000-8000-0000000000f1";

    fn run(db: &Db, chat_id: &str) -> Vec<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let payload = ConversationRenderPayload {
            chat_id: chat_id.to_string(),
            full_reembed: false,
        };
        let (out, lines) = capture(|| {
            rt.block_on(handle_conversation_render_in_zone(
                db,
                JOB,
                USER,
                &payload,
                NOW,
                &jiff::tz::TimeZone::UTC,
            ))
        });
        out.expect("the job completes");
        lines
    }

    fn jobs(lines: &[String], level: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with(&format!("{level} quilltap::jobs")))
            .cloned()
            .collect()
    }

    #[test]
    fn a_render_logs_v4s_success_line() {
        let db = db();
        let lines = run(&db, CHAT);
        let info = jobs(&lines, "INFO");
        assert_eq!(info.len(), 1, "{lines:?}");
        // v4's camelCase field NAMES (P4.126 — v5 had logged snake_case).
        for f in [
            "[ConversationRender] Conversation rendered successfully",
            " jobId=00000000-0000-4000-8000-0000000000f1",
            " chatId=00000000-0000-4000-8000-0000000000c1",
            " interchangeCount=1",
            " markdownLength=",
            " durationMs=",
        ] {
            assert!(info[0].contains(f), "{f} missing: {}", info[0]);
        }
        assert!(jobs(&lines, "DEBUG").is_empty(), "{lines:?}");
        assert!(jobs(&lines, "WARN").is_empty(), "{lines:?}");
        // The chunk landed and the chat row was never written (v4's test
        // asserts `chats.update` is not called).
        let (chunks, updated): (i64, String) = db
            .read_main(|c| {
                let n = c.query_row(
                    "SELECT COUNT(*) FROM conversation_chunks WHERE chatId = ?1",
                    [CHAT],
                    |r| r.get(0),
                )?;
                let u = c.query_row("SELECT updatedAt FROM chats WHERE id = ?1", [CHAT], |r| {
                    r.get(0)
                })?;
                Ok((n, u))
            })
            .unwrap();
        assert_eq!((chunks, updated.as_str()), (1, NOW));
    }

    #[test]
    fn a_chat_with_no_events_debugs_and_logs_no_success() {
        let db = db();
        let lines = run(&db, OTHER_CHAT);
        let debug = jobs(&lines, "DEBUG");
        assert_eq!(debug.len(), 1, "{lines:?}");
        assert!(
            debug[0].contains("[ConversationRender] Chat has no events, nothing to render")
                && debug[0].contains(" jobId=00000000-0000-4000-8000-0000000000f1")
                && debug[0].contains(" chatId=00000000-0000-4000-8000-0000000000c2"),
            "{}",
            debug[0]
        );
        assert!(jobs(&lines, "INFO").is_empty(), "{lines:?}");
    }

    /// v4's `repos.chats.findById` is the fallback `_findById`: a FAILED read
    /// logs the repository's ERROR and the job takes the same not-found arm —
    /// it completes (no retry) with the WARN (the `97b25fc53` unification
    /// review).
    #[test]
    fn a_failed_chat_read_warns_not_found_and_completes() {
        let db = db();
        db.write_blocking(|w| {
            w.main().connection().execute_batch("DROP TABLE chats")?;
            Ok(())
        })
        .unwrap();
        let lines = run(&db, CHAT);
        let warn = jobs(&lines, "WARN");
        assert_eq!(warn.len(), 1, "{lines:?}");
        assert!(
            warn[0].contains("[ConversationRender] Chat not found, skipping"),
            "{}",
            warn[0]
        );
        // The repository line carries v4's `Repository` target (`quilltap::db`, the one `db::fallback` home).
        let db_err: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR ") && l.contains("Error finding entity by ID"))
            .collect();
        assert_eq!(db_err.len(), 1, "{lines:?}");
        assert!(
            db_err[0].contains("Error finding entity by ID"),
            "{}",
            db_err[0]
        );
        assert!(jobs(&lines, "INFO").is_empty() && jobs(&lines, "DEBUG").is_empty());
    }

    #[test]
    fn a_missing_chat_warns_with_its_job_id() {
        let db = db();
        let lines = run(&db, "00000000-0000-4000-8000-0000000000ff");
        let warn = jobs(&lines, "WARN");
        assert_eq!(warn.len(), 1, "{lines:?}");
        assert!(
            warn[0].contains("[ConversationRender] Chat not found, skipping")
                && warn[0].contains(" jobId=00000000-0000-4000-8000-0000000000f1")
                && warn[0].contains(" chatId=00000000-0000-4000-8000-0000000000ff"),
            "{}",
            warn[0]
        );
        assert!(jobs(&lines, "INFO").is_empty() && jobs(&lines, "DEBUG").is_empty());
    }

    /// v4 `:92-96`: an enqueue failure warns with `jobId`/`chatId`/`error`
    /// and the job still completes. Planted as a throw in v4 too: a DEFAULT
    /// profile exists (the read succeeds) and the job write fails.
    #[test]
    fn an_enqueue_failure_warns_with_v4s_field_names() {
        let db = db();
        db.write_blocking(|w| {
            w.main().connection().execute_batch(&format!(
                "INSERT INTO embedding_profiles (id, userId, name, provider, modelName, \
                 isDefault, createdAt, updatedAt) VALUES \
                 ('00000000-0000-4000-8000-0000000000e9', '{USER}', 'Default', 'OPENAI', \
                 'text-embedding-3-small', 1, '{NOW}', '{NOW}'); \
                 DROP TABLE background_jobs;"
            ))?;
            Ok(())
        })
        .unwrap();
        let lines = run(&db, CHAT);
        let warn = jobs(&lines, "WARN");
        assert_eq!(warn.len(), 1, "{lines:?}");
        for f in [
            "[ConversationRender] Failed to enqueue embedding, continuing",
            " jobId=00000000-0000-4000-8000-0000000000f1",
            " chatId=00000000-0000-4000-8000-0000000000c1",
            " error=",
        ] {
            assert!(warn[0].contains(f), "{f} missing: {}", warn[0]);
        }
        assert_eq!(jobs(&lines, "INFO").len(), 1, "{lines:?}");
    }

    /// v4 `orchestrator.service.ts:253-256`: a failed trigger warns with
    /// `chatId` + `error` and is swallowed.
    #[test]
    fn a_failed_trigger_warns_with_v4s_field_names() {
        let db = db();
        db.write_blocking(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE background_jobs")?;
            Ok(())
        })
        .unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ((), lines) = capture(|| rt.block_on(trigger_conversation_render(&db, USER, CHAT)));
        let warn: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("WARN quilltap::chat"))
            .collect();
        assert_eq!(warn.len(), 1, "{lines:?}");
        for f in [
            "Failed to trigger conversation render",
            " chatId=00000000-0000-4000-8000-0000000000c1",
            " error=",
        ] {
            assert!(warn[0].contains(f), "{f} missing: {}", warn[0]);
        }
    }
}
