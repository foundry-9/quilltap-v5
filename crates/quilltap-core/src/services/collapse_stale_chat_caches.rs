//! Stale-chat cache collapse + conversation-chunk cold-tiering (v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`).
//!
//! When a chat has gone quiet (no *played* message for the configured retention
//! window — see [`resolve_stale_chat_days`]), the regenerable and discardable
//! working data it accumulated is dead weight:
//!
//!  - `chats.compressionCache`       — pre-compression cache; regenerable.
//!  - `chats.renderedMarkdown`       — Scriptorium render; rebuilt on demand.
//!  - `chat_messages.rawResponse`    — byte-exact provider payload; only read by
//!    generation-time services, never by the historical read/render path.
//!  - `chat_messages.reasoningContent` / `reasoningSegments` — model thinking
//!    traces; DISPLAY ONLY, never re-fed to models.
//!  - `chat_messages.renderedHtml`   — pre-rendered HTML; re-rendered from
//!    `content` live on the chat GET path.
//!  - `chat_messages.debugMemoryLogs` — memory-gate debug telemetry.
//!
//! It also cold-tiers the chat's `conversation_chunks` embeddings (NULLs the
//! BLOB, keeps `content` for keyword search); the Salon chat-load path re-embeds
//! on demand ([`super::cold_chunk_reembed`], once ported).
//!
//! NEVER touched: `content`, `opaqueContent`, `thoughtSignature`, `attachments`,
//! `contextSummary`, `chats.state`, memories, `summaryAnchor`.
//!
//! Gated on CHAT staleness via the same shared [`super::maintenance::is_stale`]
//! the asset collapse uses, so the sweeps can never disagree on "stale". NULLing
//! frees pages inside the file; actual file shrink happens at the periodic manual
//! `npx quilltap db optimize` (VACUUM — an unported CLI surface).
//!
//! ## The `dropInMemoryCompressionCache` seam (v4-only)
//!
//! v4 pairs the `chats.compressionCache` clear with
//! `dropInMemoryCompressionCache(chatId)` so the in-process compression-cache
//! `Map` can't serve a stale entry it believes is still persisted. v5 has no such
//! in-memory `Map` (a Node-service workaround, not shared state we hold), so
//! there is nothing to drop — the raw-SQL clear IS the whole effect. The DB
//! result the differential checks is unaffected.
//!
//! ## Single-writer shape
//!
//! v4 runs this on the parent (the sole DB writer) via `rawQuery`. Here the three
//! guarded clears for one chat run in a single [`Db::write`] transaction on the
//! main connection; reads (the chat list, the staleness gate) go through the read
//! pool. `now_ms` is injected (the differential pins it).

use rusqlite::params;
use serde_json::Value;

use crate::clock::{iso_from_unix_ms, iso_to_ms};
use crate::db::conversation_chunks::ConversationChunksRepository;
use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};

use super::maintenance::is_stale;
use super::queue_service::{resolve_stale_chat_days, retention_cutoff_iso};

/// The summary of one cache-collapse pass (v4 `StaleChatCacheCollapseSummary`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaleChatCacheCollapseSummary {
    /// Total chats examined.
    pub chats_scanned: usize,
    /// Chats found stale (eligible for collapse).
    pub stale_chats: usize,
    /// Stale chats where at least one column/row was actually cleared.
    pub chats_collapsed: usize,
    /// `chats` rows whose `compressionCache`/`renderedMarkdown` were cleared.
    pub chat_rows_cleared: usize,
    /// `chat_messages` rows that had at least one discardable column cleared.
    pub message_rows_cleared: usize,
    /// `conversation_chunks` rows whose embedding was cold-tiered to NULL.
    pub chunk_embeddings_cleared: usize,
}

/// Collapse one stale chat's regenerable caches (v4 `collapseOneChat`). Idempotent:
/// every UPDATE is guarded by `IS NOT NULL`, so a second pass rewrites nothing.
/// Returns `(chat_rows, message_rows, chunk_embeddings)`.
async fn collapse_one_chat(
    db: &Db,
    chat_id: &str,
    now_iso: &str,
) -> Result<(usize, usize, usize), DbError> {
    let cid = chat_id.to_string();
    let stamp = now_iso.to_string();
    db.write(move |writers| {
        let conn = writers.main().connection();

        // 1. `chats` columns. Raw SQL (NOT repos.chats.update) so the chat's
        //    updatedAt is NOT bumped — a maintenance pass must never make a stale
        //    chat look freshly touched. (v4 also drops the in-memory compression
        //    cache here; v5 has no such Map — see the module doc.)
        let chat_rows = conn.execute(
            "UPDATE chats SET compressionCache = NULL, renderedMarkdown = NULL \
               WHERE id = ?1 \
                 AND (compressionCache IS NOT NULL OR renderedMarkdown IS NOT NULL)",
            params![cid],
        )?;

        // 2. `chat_messages` discardable columns, one guarded UPDATE per chat.
        let message_rows = conn.execute(
            "UPDATE chat_messages \
                SET rawResponse = NULL, reasoningContent = NULL, reasoningSegments = NULL, \
                    renderedHtml = NULL, debugMemoryLogs = NULL \
              WHERE chatId = ?1 \
                AND (rawResponse IS NOT NULL OR reasoningContent IS NOT NULL \
                  OR reasoningSegments IS NOT NULL OR renderedHtml IS NOT NULL \
                  OR debugMemoryLogs IS NOT NULL)",
            params![cid],
        )?;

        // 3. Cold-tier the chat's conversation-chunk embeddings (keep content).
        let chunk_embeddings =
            ConversationChunksRepository::new(conn).clear_embeddings_for_chat(&cid, &stamp)?;

        Ok((chat_rows, message_rows, chunk_embeddings))
    })
    .await
}

/// Collapse every stale chat's regenerable caches and cold-tier its chunk
/// embeddings (v4 `collapseStaleChatCaches`). Each chat is processed
/// independently so one failure cannot abort the rest. A failure to LIST the
/// chats (or evaluate staleness) propagates — the caller records `'caches'` in
/// its failures list, exactly as v4's scheduler try/catch does.
pub async fn collapse_stale_chat_caches(
    db: &Db,
    now_ms: i64,
) -> Result<StaleChatCacheCollapseSummary, DbError> {
    let cutoff = retention_cutoff_iso(resolve_stale_chat_days(db), now_ms);
    let cutoff_ms = iso_to_ms(&cutoff).unwrap_or(now_ms);
    let now_iso = iso_from_unix_ms(now_ms);

    let all_chats = db.read_main(chats_read::find_all)?;
    let mut summary = StaleChatCacheCollapseSummary {
        chats_scanned: all_chats.len(),
        ..Default::default()
    };

    for chat in &all_chats {
        if !is_stale(db, chat, cutoff_ms)? {
            continue;
        }
        summary.stale_chats += 1;
        let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
        // v4 wraps each chat's collapse in try/catch — a failure is swallowed
        // (warn) and the rest continue.
        match collapse_one_chat(db, chat_id, &now_iso).await {
            Ok((chat_rows, message_rows, chunk_embeddings)) => {
                if chat_rows > 0 || message_rows > 0 || chunk_embeddings > 0 {
                    summary.chats_collapsed += 1;
                    summary.chat_rows_cleared += chat_rows;
                    summary.message_rows_cleared += message_rows;
                    summary.chunk_embeddings_cleared += chunk_embeddings;
                }
            }
            Err(_) => { /* v4 warns + continues */ }
        }
    }

    Ok(summary)
}
