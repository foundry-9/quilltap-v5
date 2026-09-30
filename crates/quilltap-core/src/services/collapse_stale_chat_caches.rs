//! Stale-chat cache collapse (v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`).
//!
//! When a chat has gone quiet (no *played* message for the configured retention
//! window — see [`resolve_stale_chat_days`]), the regenerable and discardable
//! working data it accumulated is dead weight:
//!
//!  - `chats.compressionCache`       — pre-compression cache; regenerable.
//!  - `chats.compiledIdentityStacks` — precompiled per-participant identity
//!    stacks; a read-through cache, so clearing it costs one recompile.
//!  - `chat_messages.rawResponse`    — byte-exact provider payload; only read by
//!    generation-time services, never by the historical read/render path.
//!  - `chat_messages.reasoningContent` / `reasoningSegments` — model thinking
//!    traces; DISPLAY ONLY, never re-fed to models.
//!  - `chat_messages.renderedHtml`   — pre-rendered HTML; re-rendered from
//!    `content` live on the chat GET path.
//!  - `chat_messages.debugMemoryLogs` — memory-gate debug telemetry.
//!
//! **`conversation_chunks` embeddings are deliberately NEVER touched here**
//! (v4 `f7f3d7bf0`, "keep conversation embeddings warm"). They were once
//! cold-tiered (NULLed) alongside these caches; v4 measured the whole embedding
//! corpus at ~16 MB on a real instance while cold-tiering silently killed
//! semantic retrieval over most of the user's history. A stale chat's chunks
//! stay warm and searchable for as long as they exist. (`chats.renderedMarkdown`
//! also left this list: the same commit dropped the column — transcripts are
//! rendered on demand, `super::scriptorium_render`.)
//!
//! NEVER touched: `content`, `opaqueContent`, `thoughtSignature`, `attachments`,
//! `contextSummary`, `chats.state`, memories, `summaryAnchor`, and (as above)
//! `conversation_chunks.embedding`.
//!
//! `chats.compiledIdentityStacks` IS collapsed, alongside `compressionCache` —
//! v4 bug 160 (`186eb09cb`). v5 had reproduced the same omission: the column
//! was absent from both the SET and the guard disjunct, so a chat whose ONLY
//! discardable column was a compiled stack was swept and left it behind.
//! Nulling is safe and costs one recompile: the stack is written by
//! `services/system_prompt_compiler.rs:269` and read back under STRICT version
//! equality, so an absent stack is simply recompiled.
//!
//! Gated on CHAT staleness via the same shared [`super::maintenance::is_stale`]
//! the asset collapse uses, so the sweeps can never disagree on "stale". NULLing
//! frees pages inside the file; actual file shrink happens at the periodic manual
//! `npx quilltap db optimize` (VACUUM — an unported CLI surface).
//!
//! ## v4's three log lines
//!
//! v5 had none of them (pre-existing; restored with `f7f3d7bf0`'s shapes): the
//! per-chat INFO `Collapsed stale chat caches {chatId, chatRows,
//! messageRows}` gated on either count being non-zero, the per-chat WARN
//! `Failed to collapse stale chat caches — continuing {chatId, error}`, and the
//! pass's INFO `Stale-chat cache collapse complete` with the summary spread
//! (camelCase field names, as v4's `{...summary}` logs them — P4.124). v4
//! logs the per-chat INFO inside the collapse; here it is logged on the calling
//! thread once the chat's write transaction returns (the writer thread runs the
//! UPDATEs), which changes no field and no order.
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
//! v4 runs this on the parent (the sole DB writer) via `rawQuery`. Here the two
//! guarded clears for one chat run in a single [`Db::write`] transaction on the
//! main connection; reads (the chat list, the staleness gate) go through the read
//! pool. `now_ms` is injected (the differential pins it).

use rusqlite::params;
use serde_json::Value;

use crate::clock::iso_to_ms;
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
    /// `chats` rows whose regenerable cache columns were cleared.
    pub chat_rows_cleared: usize,
    /// `chat_messages` rows that had at least one discardable column cleared.
    pub message_rows_cleared: usize,
}

/// Collapse one stale chat's regenerable caches (v4 `collapseOneChat`). Idempotent:
/// every UPDATE is guarded by `IS NOT NULL`, so a second pass rewrites nothing.
/// Returns `(chat_rows, message_rows)`.
async fn collapse_one_chat(db: &Db, chat_id: &str) -> Result<(usize, usize), DbError> {
    let cid = chat_id.to_string();
    let counts = db
        .write(move |writers| {
            let conn = writers.main().connection();

            // 1. `chats` columns. Raw SQL (NOT repos.chats.update) so the chat's
            //    updatedAt is NOT bumped — a maintenance pass must never make a stale
            //    chat look freshly touched. (v4 also drops the in-memory compression
            //    cache here; v5 has no such Map — see the module doc.)
            let chat_rows = conn.execute(
                "UPDATE chats SET compressionCache = NULL, compiledIdentityStacks = NULL \
               WHERE id = ?1 \
                 AND (compressionCache IS NOT NULL OR compiledIdentityStacks IS NOT NULL)",
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

            Ok((chat_rows, message_rows))
        })
        .await?;
    let (chat_rows, message_rows) = counts;
    if chat_rows > 0 || message_rows > 0 {
        tracing::info!(
            target: "quilltap::maintenance",
            chatId = %chat_id,
            chatRows = chat_rows,
            messageRows = message_rows,
            "Collapsed stale chat caches",
        );
    }
    Ok(counts)
}

/// Collapse every stale chat's regenerable caches (v4 `collapseStaleChatCaches`). Each chat is processed
/// independently so one failure cannot abort the rest. A failure to LIST the
/// chats (or evaluate staleness) propagates — the caller records `'caches'` in
/// its failures list, exactly as v4's scheduler try/catch does.
pub async fn collapse_stale_chat_caches(
    db: &Db,
    now_ms: i64,
) -> Result<StaleChatCacheCollapseSummary, DbError> {
    let cutoff = retention_cutoff_iso(resolve_stale_chat_days(db), now_ms);
    let cutoff_ms = iso_to_ms(&cutoff).unwrap_or(now_ms);

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
        match collapse_one_chat(db, chat_id).await {
            Ok((chat_rows, message_rows)) => {
                if chat_rows > 0 || message_rows > 0 {
                    summary.chats_collapsed += 1;
                    summary.chat_rows_cleared += chat_rows;
                    summary.message_rows_cleared += message_rows;
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::maintenance",
                    chatId = %chat_id,
                    error = %e,
                    "Failed to collapse stale chat caches — continuing",
                );
            }
        }
    }

    tracing::info!(
        target: "quilltap::maintenance",
        chatsScanned = summary.chats_scanned,
        staleChats = summary.stale_chats,
        chatsCollapsed = summary.chats_collapsed,
        chatRowsCleared = summary.chat_rows_cleared,
        messageRowsCleared = summary.message_rows_cleared,
        "Stale-chat cache collapse complete",
    );
    Ok(summary)
}

/// P4.D235: v4's three log lines (none of which v5 had emitted), each with its
/// silence leg, over the shared scriptorium-tool fixture — whose chats are all
/// stale at a `now` a year past their messages.
#[cfg(test)]
mod log_tests {
    use super::*;
    use crate::test_support::global_capture::capture;
    use crate::tools::annotations::log_tests::{db, CHAT, OTHER_CHAT};

    /// 2027-09-28 — both fixture chats are far outside the 30-day window.
    const NOW_MS: i64 = 1_822_000_000_000;

    fn sweep(db: &Db) -> (StaleChatCacheCollapseSummary, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (out, lines) = capture(|| rt.block_on(collapse_stale_chat_caches(db, NOW_MS)));
        (out.expect("the sweep answers"), lines)
    }

    fn maint<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines
            .iter()
            .filter(|l| l.contains("quilltap::maintenance") && l.contains(needle))
            .collect()
    }

    #[test]
    fn a_collapsed_chat_logs_its_counts_and_the_pass_its_summary() {
        let db = db();
        db.write_blocking(|w| {
            w.main().connection().execute(
                "UPDATE chats SET compressionCache = '{}' WHERE id = ?1",
                [CHAT],
            )?;
            Ok(())
        })
        .unwrap();
        let (summary, lines) = sweep(&db);
        assert_eq!(
            (
                summary.stale_chats,
                summary.chats_collapsed,
                summary.chat_rows_cleared
            ),
            (2, 1, 1)
        );
        let per_chat = maint(&lines, "Collapsed stale chat caches");
        assert_eq!(
            per_chat.len(),
            1,
            "only the chat with something to clear: {lines:?}"
        );
        assert!(
            per_chat[0].starts_with("INFO")
                && per_chat[0].contains(&format!("chatId={CHAT}"))
                && per_chat[0].contains("chatRows=1")
                && per_chat[0].contains("messageRows=0")
                && !per_chat[0].contains("chat_id="),
            "{}",
            per_chat[0]
        );
        assert!(!per_chat[0].contains(OTHER_CHAT));
        let done = maint(&lines, "Stale-chat cache collapse complete");
        assert_eq!(done.len(), 1, "{lines:?}");
        for f in [
            "chatsScanned=2",
            "staleChats=2",
            "chatsCollapsed=1",
            "chatRowsCleared=1",
            "messageRowsCleared=0",
        ] {
            assert!(done[0].contains(f), "{f} missing: {}", done[0]);
        }
        assert!(!done[0].contains("chats_scanned="), "{}", done[0]);
        assert!(!done[0].contains("chunk_embeddings"), "{}", done[0]);
        assert!(maint(&lines, "Failed to collapse").is_empty());

        // Idempotent: a second pass clears nothing and says so only in its
        // summary — the per-chat INFO is silent.
        let (_, again) = sweep(&db);
        assert!(
            maint(&again, "Collapsed stale chat caches").is_empty(),
            "{again:?}"
        );
        assert_eq!(maint(&again, "Stale-chat cache collapse complete").len(), 1);
    }

    #[test]
    fn a_failing_chat_warns_and_the_pass_continues() {
        let db = db();
        db.write_blocking(|w| {
            // Both chats carry a cache, so the guarded UPDATE matches a row
            // and the trigger (per affected row) refuses it.
            w.main().connection().execute_batch(
                "UPDATE chats SET compressionCache = '{}'; \
                 CREATE TRIGGER no_chat_updates BEFORE UPDATE ON chats \
                 BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
            )?;
            Ok(())
        })
        .unwrap();
        let (summary, lines) = sweep(&db);
        let warns = maint(&lines, "Failed to collapse stale chat caches — continuing");
        assert_eq!(warns.len(), 2, "one per stale chat: {lines:?}");
        assert!(warns
            .iter()
            .all(|w| w.starts_with("WARN") && w.contains("error=") && w.contains("chatId=")));
        assert_eq!(summary.chats_collapsed, 0);
        assert_eq!(maint(&lines, "Stale-chat cache collapse complete").len(), 1);
    }
}
