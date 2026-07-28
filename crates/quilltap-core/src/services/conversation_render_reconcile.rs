//! The conversation render/embed reconciliation (P4.6BM) — a port of v4's
//! `lib/startup/reconcile-conversation-rendering.ts`, wired at boot
//! (v4 `instrumentation.ts` "PHASE 3.6").
//!
//! Re-enqueues a `CONVERSATION_RENDER` job for every chat the Scriptorium
//! pipeline left half-finished — carrying v4's own two arms and its *why*:
//!
//!   (A) a chat with real USER/ASSISTANT messages but no rendered Markdown — the
//!       per-turn render trigger never fired or the render job died (e.g. an
//!       interrupted shutdown), so `renderedMarkdown` is still NULL; or
//!   (B) a chat whose interchange chunks were never embedded — the embedding
//!       provider was down when the turn fired, or the render job died before
//!       enqueuing the embeds, leaving chunks with a NULL `embedding` and no
//!       recovery path.
//!
//! Re-running `CONVERSATION_RENDER` heals both: the handler upserts the
//! interchange chunks (preserving embeddings already present) and re-enqueues
//! `EMBEDDING_GENERATE` for every chunk still lacking one.
//!
//! Why every startup rather than a one-time backfill: the gap recurs. New chats
//! slip through whenever the embedder is unavailable mid-conversation, and a hard
//! shutdown can drop an in-flight render. It is a no-op on a healthy instance —
//! one indexed scan that returns nothing.
//!
//! **Oversized / empty chunks are EXCLUDED from the "needs work" test.** A chunk
//! larger than [`EMBEDDING_MAX_CHARS`] (or empty) is deterministically
//! unembeddable — the embedder marks it failed without retry — so counting it
//! would keep its chat perpetually "incomplete" and re-render it on every boot
//! for nothing.
//!
//! **STALE chats are EXCLUDED too** (v4 `a0243abd`), via the same shared
//! [`is_stale_conn`] gate the maintenance sweeps use. The stale-chat cache
//! collapse deliberately cold-tiers quiet chats into exactly the state this scan
//! reads as damage — NULL `renderedMarkdown`, NULL chunk embeddings — and
//! expects the Salon reopen path (`cold_chunk_reembed`) to re-embed on demand.
//! Before this exclusion the two subsystems fought: every boot re-rendered and
//! re-embedded the entire cold tier (thousands of paid embedding calls), and the
//! next sweep cleared it all again. A stale chat is healed when it is actually
//! reopened or played, not at startup.
//!
//! ## This REPLACES the P4.6BL boot repair
//!
//! `services::embedding_backlog_repair` was a sanctioned v5-only stand-in: with
//! no `CONVERSATION_RENDER` handler, a verbatim port of this reconcile would have
//! minted a fresh batch of DEAD render jobs on every boot, so that module took
//! arm (B) alone and enqueued `EMBEDDING_GENERATE` directly. Its own doc named
//! the exit: "this module retires in favor of the reconcile when that handler
//! lands". It has, so it did.
//!
//! **The coverage argument, since retiring it drops v5-only behavior.** Both use
//! the identical recoverable-chunk predicate, and the render handler re-enqueues
//! an embed for every chunk it upserts that still lacks one — so every chunk the
//! repair would have reached is reached, one hop later, plus arm (A) which the
//! repair never healed. The single case the repair covered and this does not is
//! an ORPHAN chunk whose `chats` row is gone: v4's scan selects FROM `chats`, so
//! it can never see one. That is v4's behavior, the rows are unreachable from
//! every read path anyway, and deliberately matching it is the point.
//!
//! Skips quietly (never fails the boot) when a table is missing — a v4 instance
//! that never chunked legitimately lacks them, since v4 creates collections
//! lazily (the P4.9G3 lesson). v4's own guard is a try/catch around the scan.

use rusqlite::{params, Connection};
use serde_json::json;

use super::embedding_generate_job::EMBEDDING_MAX_CHARS;
use super::maintenance::is_stale_conn;
use super::queue_service::{resolve_stale_chat_days_conn, retention_cutoff_iso};
use crate::clock::iso_to_ms;
use crate::db::DbError;

/// v4's `SELECT_INCOMPLETE_CHATS`, verbatim. The first `?` binds
/// [`EMBEDDING_MAX_CHARS`]; the second binds the current default embedding
/// profile's id (or a sentinel matching nothing when no profile exists).
/// `updatedAt` rides along for the staleness gate's no-played-messages fallback.
///
/// The length guard alone is not enough: a chunk can sit under the
/// 131,072-char transport cap yet exceed the embedding model's token context
/// (e.g. >8,192 tokens ≈ ~31k chars for text-embedding-3-large). Those fail
/// deterministically — `is_permanent_embedding_error` marks them FAILED without
/// retry — so any chunk with a FAILED `embedding_status` for the profile the
/// re-embed would actually use is excluded, or its chat would re-render and
/// re-fail on every boot (v4 `a5d6cee5`).
const SELECT_INCOMPLETE_CHATS: &str = "\
  SELECT c.\"id\" AS chatId, c.\"userId\" AS userId, c.\"updatedAt\" AS updatedAt
  FROM \"chats\" c
  WHERE (
    -- (A) Real messages but never rendered to Markdown.
    c.\"renderedMarkdown\" IS NULL
    AND EXISTS (
      SELECT 1 FROM \"chat_messages\" m
      WHERE m.\"chatId\" = c.\"id\"
        AND m.\"type\" = 'message'
        AND m.\"role\" IN ('USER', 'ASSISTANT')
    )
  ) OR EXISTS (
    -- (B) At least one recoverable un-embedded interchange chunk
    --     (non-empty, within the embedder's size cap, and not already
    --     permanently FAILED for the current default profile).
    SELECT 1 FROM \"conversation_chunks\" cc
    WHERE cc.\"chatId\" = c.\"id\"
      AND cc.\"embedding\" IS NULL
      AND LENGTH(cc.\"content\") BETWEEN 1 AND ?1
      AND NOT EXISTS (
        SELECT 1 FROM \"embedding_status\" es
        WHERE es.\"entityType\" = 'CONVERSATION_CHUNK'
          AND es.\"entityId\" = cc.\"id\"
          AND es.\"profileId\" = ?2
          AND es.\"status\" = 'FAILED'
      )
  )
";

/// What one reconciliation pass did (v4 `ConversationRenderReconcileResult`).
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub struct ReconcileResult {
    /// Distinct chats found to be incomplete.
    pub incomplete_chats: usize,
    /// New render jobs enqueued.
    pub enqueued: usize,
    /// Chats that already had a render job pending (deduped).
    pub reused: usize,
    /// Chats whose enqueue failed (logged, sweep continues).
    pub failed: usize,
    /// Incomplete-looking chats skipped because they are stale (cold-tiered).
    pub skipped_stale: usize,
}

/// One incomplete-chat row.
struct IncompleteChat {
    chat_id: String,
    user_id: String,
    /// v4's `updatedAt: string | null` — the staleness gate's fallback when the
    /// chat has no played messages.
    updated_at: Option<String>,
}

/// Scan for half-rendered / un-embedded conversations and re-enqueue a render for
/// each (v4 `reconcileConversationRendering`). Safe on every startup; idempotent
/// and a no-op when every conversation is already rendered and embedded.
///
/// Runs on the writer's main connection, like v5's other boot repairs. v4 yields
/// to the event loop between enqueues (`setImmediate`) so a large backlog cannot
/// hog startup; that has no analog here — the pass runs on its own thread.
///
/// The enqueue's dedupe (an in-flight `CONVERSATION_RENDER` for the same chat is
/// reused) is delegated entirely to the enqueue helper, exactly as in v4.
pub fn reconcile_conversation_rendering(main: &Connection, now_ms: i64) -> ReconcileResult {
    let mut result = ReconcileResult::default();

    // Resolve the profile a re-embed would actually use — the same selection the
    // render handler makes (default, else first). FAILED statuses are only
    // meaningful per profile: a chunk that failed under one profile may embed
    // fine under another, so the exclusion is scoped to this id. No profile →
    // bind a sentinel that matches no rows (the exclusion becomes a no-op).
    let default_profile_id = match crate::db::embedding_profiles::pick_reembed_profile_id(main) {
        Ok(id) => id.unwrap_or_default(),
        Err(e) => {
            tracing::warn!(
                target: "quilltap::boot",
                error = %e,
                "Failed to resolve default embedding profile; FAILED-status exclusion disabled",
            );
            String::new()
        }
    };

    // v4 wraps the scan in a try/catch and returns zeros on failure. v5's extra
    // reason to reach that arm is a lazily-created table that does not exist yet.
    let rows = match scan(main, &default_profile_id) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::boot",
                error = %e,
                "Failed to scan for incomplete conversations; skipping reconciliation",
            );
            return result;
        }
    };

    result.incomplete_chats = rows.len();
    if rows.is_empty() {
        return result;
    }

    // Staleness gate — the same shared predicate and window as the maintenance
    // sweeps, so a chat the cache collapse cold-tiered is never "healed" here.
    let cutoff_ms = iso_to_ms(&retention_cutoff_iso(
        resolve_stale_chat_days_conn(main),
        now_ms,
    ))
    .unwrap_or(now_ms);

    for row in rows {
        // v4 narrows `isStale` to `Pick<ChatMetadata, 'id' | 'updatedAt'>`, so
        // the raw scan row is passed straight through — `updatedAt ?? ''`.
        let chat = json!({
            "id": row.chat_id,
            "updatedAt": row.updated_at.clone().unwrap_or_default(),
        });
        match is_stale_conn(main, &chat, cutoff_ms) {
            Ok(true) => {
                result.skipped_stale += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                // Unknown staleness → skip, don't heal. Healing a chat that is
                // actually cold-tiered re-enters the boot-time mass re-embed
                // loop, while a skipped chat still has a recovery path: the
                // Salon open path re-embeds any visited chat regardless of
                // staleness.
                result.skipped_stale += 1;
                tracing::warn!(
                    target: "quilltap::boot",
                    chat_id = %row.chat_id,
                    error = %e,
                    "Staleness check failed during reconciliation; skipping chat",
                );
                continue;
            }
        }
        match crate::services::queue_service::enqueue_conversation_render_blocking(
            main,
            &row.user_id,
            &row.chat_id,
            None,
        ) {
            Ok((_, true)) => result.enqueued += 1,
            Ok((_, false)) => result.reused += 1,
            Err(e) => {
                result.failed += 1;
                tracing::warn!(
                    target: "quilltap::boot",
                    chat_id = %row.chat_id,
                    error = %e,
                    "Failed to enqueue conversation render during reconciliation",
                );
            }
        }
    }

    result
}

fn scan(main: &Connection, default_profile_id: &str) -> Result<Vec<IncompleteChat>, DbError> {
    let mut stmt = main.prepare(SELECT_INCOMPLETE_CHATS)?;
    let rows = stmt.query_map(
        params![EMBEDDING_MAX_CHARS as i64, default_profile_id],
        |r| {
            Ok(IncompleteChat {
                chat_id: r.get(0)?,
                user_id: r.get(1)?,
                updated_at: r.get(2)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `now` far past every seeded timestamp, so a chat's staleness is decided
    /// purely by the stamps the case gives it.
    const NOW_MS: i64 = 1_780_000_000_000; // 2026-06-08T…Z
    /// Inside the default 30-day window relative to [`NOW_MS`].
    const FRESH_ISO: &str = "2026-06-07T00:00:00.000Z";
    /// Well outside it.
    const OLD_ISO: &str = "2026-01-01T00:00:00.000Z";

    /// The schema subset the scan + the staleness gate read. Deliberately
    /// hand-rolled rather than provisioned: the point is to exercise the SQL,
    /// and a missing table is one of the cases under test.
    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chats (\
                id TEXT PRIMARY KEY, userId TEXT, renderedMarkdown TEXT, updatedAt TEXT);\
             CREATE TABLE chat_messages (\
                id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, \
                systemSender TEXT, createdAt TEXT);\
             CREATE TABLE conversation_chunks (\
                id TEXT PRIMARY KEY, chatId TEXT, content TEXT, embedding BLOB);\
             CREATE TABLE embedding_status (\
                id TEXT PRIMARY KEY, entityType TEXT, entityId TEXT, profileId TEXT, \
                status TEXT);\
             CREATE TABLE embedding_profiles (id TEXT PRIMARY KEY, isDefault INTEGER);\
             CREATE TABLE background_jobs (\
                id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, \
                status TEXT NOT NULL, payload TEXT NOT NULL, priority REAL NOT NULL, \
                attempts REAL NOT NULL, maxAttempts REAL NOT NULL, lastError TEXT, \
                scheduledAt TEXT NOT NULL, startedAt TEXT, completedAt TEXT, \
                createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    /// A chat that is NOT stale (its `updatedAt` sits inside the window and it
    /// has no played messages).
    fn chat(conn: &Connection, id: &str, rendered: Option<&str>) {
        chat_at(conn, id, rendered, FRESH_ISO);
    }

    fn chat_at(conn: &Connection, id: &str, rendered: Option<&str>, updated_at: &str) {
        conn.execute(
            "INSERT INTO chats (id, userId, renderedMarkdown, updatedAt) \
             VALUES (?1, 'u1', ?2, ?3)",
            params![id, rendered, updated_at],
        )
        .unwrap();
    }

    fn message(conn: &Connection, id: &str, chat_id: &str, type_: &str, role: &str) {
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, systemSender, createdAt) \
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![id, chat_id, type_, role, FRESH_ISO],
        )
        .unwrap();
    }

    fn chunk(conn: &Connection, id: &str, chat_id: &str, content: &str, embedded: bool) {
        let blob: Option<Vec<u8>> = embedded.then(|| vec![0u8; 8]);
        conn.execute(
            "INSERT INTO conversation_chunks (id, chatId, content, embedding) \
             VALUES (?1, ?2, ?3, ?4)",
            params![id, chat_id, content, blob],
        )
        .unwrap();
    }

    fn profile(conn: &Connection, id: &str, is_default: bool) {
        conn.execute(
            "INSERT INTO embedding_profiles (id, isDefault) VALUES (?1, ?2)",
            params![id, is_default as i64],
        )
        .unwrap();
    }

    fn failed_status(conn: &Connection, chunk_id: &str, profile_id: &str) {
        conn.execute(
            "INSERT INTO embedding_status (id, entityType, entityId, profileId, status) \
             VALUES (?1, 'CONVERSATION_CHUNK', ?2, ?3, 'FAILED')",
            params![format!("es-{chunk_id}-{profile_id}"), chunk_id, profile_id],
        )
        .unwrap();
    }

    fn enqueued_chat_ids(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT json_extract(payload, '$.chatId') FROM background_jobs \
                 WHERE type = 'CONVERSATION_RENDER' ORDER BY rowid",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Both arms fire; the three exclusions (no real message, system-only,
    /// oversize chunk) do not.
    #[test]
    fn selects_both_arms_and_excludes_the_unrecoverable() {
        let conn = test_conn();
        chat(&conn, "arm-a", None);
        message(&conn, "m1", "arm-a", "message", "USER");
        chat(&conn, "arm-b", Some("rendered"));
        chunk(&conn, "c1", "arm-b", "recoverable", false);
        chat(&conn, "no-messages", None);
        chat(&conn, "system-only", None);
        message(&conn, "m2", "system-only", "system", "ASSISTANT");
        chat(&conn, "oversize", Some("rendered"));
        chunk(
            &conn,
            "c2",
            "oversize",
            &"x".repeat(EMBEDDING_MAX_CHARS + 1),
            false,
        );
        chat(&conn, "healthy", Some("rendered"));
        chunk(&conn, "c3", "healthy", "done", true);

        let r = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!(r.incomplete_chats, 2);
        assert_eq!(r.enqueued, 2);
        assert_eq!((r.reused, r.failed, r.skipped_stale), (0, 0, 0));
        let mut ids = enqueued_chat_ids(&conn);
        ids.sort();
        assert_eq!(ids, vec!["arm-a", "arm-b"]);
    }

    /// The second pass reuses the jobs the first enqueued (the dedupe lives in
    /// the enqueue helper, as in v4).
    #[test]
    fn second_pass_reuses() {
        let conn = test_conn();
        chat(&conn, "arm-a", None);
        message(&conn, "m1", "arm-a", "message", "ASSISTANT");

        let first = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!((first.enqueued, first.reused), (1, 0));
        let second = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!((second.enqueued, second.reused), (0, 1));
        assert_eq!(enqueued_chat_ids(&conn).len(), 1);
    }

    /// A missing table (a lazily-created v4 instance that never chunked) warns
    /// and returns zeros instead of failing the boot.
    #[test]
    fn missing_tables_return_zeros() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(
            reconcile_conversation_rendering(&conn, NOW_MS),
            ReconcileResult::default()
        );
    }

    /// v4 `a0243abd`: a chat the cache collapse cold-tiered looks exactly like
    /// arm (A) damage, and must be SKIPPED rather than healed. The staleness is
    /// decided by the last PLAYED message, so a chat whose only activity is old
    /// is stale even though the scan still selects it.
    #[test]
    fn stale_chats_are_skipped_not_healed() {
        let conn = test_conn();
        chat_at(&conn, "cold-tiered", None, OLD_ISO);
        message(&conn, "m-old", "cold-tiered", "message", "USER");
        conn.execute(
            "UPDATE chat_messages SET createdAt = ?1 WHERE id = 'm-old'",
            params![OLD_ISO],
        )
        .unwrap();
        chat(&conn, "fresh", None);
        message(&conn, "m-new", "fresh", "message", "USER");

        let r = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!(r.incomplete_chats, 2, "both are still SELECTed");
        assert_eq!(r.skipped_stale, 1);
        assert_eq!(r.enqueued, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["fresh"]);
    }

    /// v4 `a5d6cee5`: a chunk already FAILED for the profile a re-embed would
    /// use is excluded from arm (B) — otherwise its chat re-renders and re-fails
    /// on every boot. The exclusion is per-profile: a FAILED row under a
    /// DIFFERENT profile must not mask it.
    #[test]
    fn failed_chunks_are_excluded_for_the_default_profile_only() {
        let conn = test_conn();
        // `isDefault` is NOT the first row, so "default, else first" is visible.
        profile(&conn, "p-other", false);
        profile(&conn, "p-default", true);

        chat(&conn, "failed-under-default", Some("rendered"));
        chunk(
            &conn,
            "c-fail",
            "failed-under-default",
            "unembeddable",
            false,
        );
        failed_status(&conn, "c-fail", "p-default");

        chat(&conn, "failed-under-other", Some("rendered"));
        chunk(&conn, "c-other", "failed-under-other", "recoverable", false);
        failed_status(&conn, "c-other", "p-other");

        let r = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!(r.incomplete_chats, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["failed-under-other"]);
    }

    /// No embedding profile at all → the sentinel `''` binds, matching no
    /// `embedding_status` row, so the exclusion is a no-op and the FAILED chunk
    /// is scanned as before. (Same shape as v4's "profile resolution threw"
    /// arm, which also keeps `''`.)
    #[test]
    fn no_profile_binds_a_sentinel_that_excludes_nothing() {
        let conn = test_conn();
        chat(&conn, "failed-but-no-profile", Some("rendered"));
        chunk(
            &conn,
            "c-fail",
            "failed-but-no-profile",
            "unembeddable",
            false,
        );
        failed_status(&conn, "c-fail", "p-default");

        let r = reconcile_conversation_rendering(&conn, NOW_MS);
        assert_eq!(r.incomplete_chats, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["failed-but-no-profile"]);
    }
}
