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

use super::embedding_generate_job::EMBEDDING_MAX_CHARS;
use crate::db::DbError;

/// v4's `SELECT_INCOMPLETE_CHATS`, verbatim. The `?1` binds
/// [`EMBEDDING_MAX_CHARS`].
const SELECT_INCOMPLETE_CHATS: &str = "\
  SELECT c.\"id\" AS chatId, c.\"userId\" AS userId
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
    --     (non-empty and within the embedder's size cap).
    SELECT 1 FROM \"conversation_chunks\" cc
    WHERE cc.\"chatId\" = c.\"id\"
      AND cc.\"embedding\" IS NULL
      AND LENGTH(cc.\"content\") BETWEEN 1 AND ?1
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
}

/// One incomplete-chat row.
struct IncompleteChat {
    chat_id: String,
    user_id: String,
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
pub fn reconcile_conversation_rendering(main: &Connection) -> ReconcileResult {
    let mut result = ReconcileResult::default();

    // v4 wraps the scan in a try/catch and returns zeros on failure. v5's extra
    // reason to reach that arm is a lazily-created table that does not exist yet.
    let rows = match scan(main) {
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

    for row in rows {
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

fn scan(main: &Connection) -> Result<Vec<IncompleteChat>, DbError> {
    let mut stmt = main.prepare(SELECT_INCOMPLETE_CHATS)?;
    let rows = stmt.query_map(params![EMBEDDING_MAX_CHARS as i64], |r| {
        Ok(IncompleteChat {
            chat_id: r.get(0)?,
            user_id: r.get(1)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema subset the scan reads. Deliberately hand-rolled rather than
    /// provisioned: the point is to exercise the SQL, and a missing table is one
    /// of the cases under test.
    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chats (id TEXT PRIMARY KEY, userId TEXT, renderedMarkdown TEXT);\
             CREATE TABLE chat_messages (id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT);\
             CREATE TABLE conversation_chunks (\
                id TEXT PRIMARY KEY, chatId TEXT, content TEXT, embedding BLOB);\
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

    fn chat(conn: &Connection, id: &str, rendered: Option<&str>) {
        conn.execute(
            "INSERT INTO chats (id, userId, renderedMarkdown) VALUES (?1, 'u1', ?2)",
            params![id, rendered],
        )
        .unwrap();
    }

    fn message(conn: &Connection, id: &str, chat_id: &str, type_: &str, role: &str) {
        conn.execute(
            "INSERT INTO chat_messages (id, chatId, type, role) VALUES (?1, ?2, ?3, ?4)",
            params![id, chat_id, type_, role],
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

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!(r.incomplete_chats, 2);
        assert_eq!(r.enqueued, 2);
        assert_eq!((r.reused, r.failed), (0, 0));
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

        let first = reconcile_conversation_rendering(&conn);
        assert_eq!((first.enqueued, first.reused), (1, 0));
        let second = reconcile_conversation_rendering(&conn);
        assert_eq!((second.enqueued, second.reused), (0, 1));
        assert_eq!(enqueued_chat_ids(&conn).len(), 1);
    }

    /// A missing table (a lazily-created v4 instance that never chunked) warns
    /// and returns zeros instead of failing the boot.
    #[test]
    fn missing_tables_return_zeros() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(
            reconcile_conversation_rendering(&conn),
            ReconcileResult::default()
        );
    }
}
