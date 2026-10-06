//! The conversation render/embed reconciliation (P4.6BM) — a port of v4's
//! `lib/startup/reconcile-conversation-rendering.ts`, wired at boot
//! (v4 `instrumentation.ts` "PHASE 3.6").
//!
//! Re-enqueues a `CONVERSATION_RENDER` job for every chat the Scriptorium
//! pipeline left half-finished — carrying v4's own arms and its *why*:
//!
//!   (A) a chat with real USER/ASSISTANT messages but NO `conversation_chunks`
//!       rows at all — never chunked: the per-turn render trigger never fired or
//!       the render job died (e.g. an interrupted shutdown). (v4 `f7f3d7bf0`
//!       re-keyed this arm from `renderedMarkdown IS NULL` when it dropped the
//!       column: a chat with chunks but no stored Markdown no longer matches,
//!       and a chat with a stored column but no chunks now does.) Or
//!   (B) a chat whose interchange chunks were never embedded — the embedding
//!       provider was down when the turn fired, or the render job died before
//!       enqueuing the embeds, leaving chunks with a NULL `embedding` and no
//!       recovery path; or
//!   (C) a chat with an un-embedded chunk OVER the per-chunk budget yet inside
//!       the transport cap (v4 Bug 17) — a chunk that predates interchange
//!       sub-chunking. Re-rendering (not re-embedding) splits it into in-context
//!       chunks that embed.
//!
//! Re-running `CONVERSATION_RENDER` heals all three: the handler upserts the
//! interchange chunks (preserving embeddings already present, NULLing a chunk
//! whose content changed) and re-enqueues `EMBEDDING_GENERATE` for every chunk
//! still lacking one.
//!
//! Why every startup rather than a one-time backfill: the gap recurs. New chats
//! slip through whenever the embedder is unavailable mid-conversation, and a hard
//! shutdown can drop an in-flight render. It is a no-op on a healthy instance —
//! one indexed scan that returns nothing.
//!
//! **Empty chunks are EXCLUDED from the "needs work" test, and oversized chunks
//! are excluded from arm (B).** A chunk larger than [`EMBEDDING_MAX_CHARS`] (or
//! empty) is deterministically unembeddable — the embedder marks it failed
//! without retry — so counting it in arm (B) would keep its chat perpetually
//! "incomplete" and re-render it for nothing. Arm (C) reclaims the sub-chunkable
//! middle band (over the budget, under the cap): re-rendering there is progress,
//! not a doomed re-embed.
//!
//! **Stale chats are healed too** (v4 `f7f3d7bf0`, "keep conversation
//! embeddings warm"). v4 `a0243abd` had excluded them, because the stale-chat
//! cache collapse cold-tiered quiet chats into exactly the state this scan
//! reads as damage; that collapse no longer touches embeddings, so the gate,
//! its `skippedStale` counter and its WARN are gone. ⚠ **This is the FIRST-BOOT
//! re-embed:** on an instance the old sweep cold-tiered, arm (B) now selects
//! every previously cold-tiered chat and enqueues a one-time render → one
//! `EMBEDDING_GENERATE` per un-embedded, non-FAILED, in-cap chunk (a real,
//! measurable provider cost — the P4.D235 lane record carries the recipe).
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

use super::conversation_markdown::CHUNK_CHAR_BUDGET;
use super::embedding_generate_job::EMBEDDING_MAX_CHARS;
use crate::db::DbError;

/// v4's `SELECT_INCOMPLETE_CHATS`, verbatim. `?1` binds [`EMBEDDING_MAX_CHARS`];
/// `?2` binds the current default embedding profile's id (or a sentinel matching
/// nothing when no profile exists); `?3` binds [`CHUNK_CHAR_BUDGET`] (arm C's
/// lower bound) and `?4` binds [`EMBEDDING_MAX_CHARS`] again (arm C's upper
/// bound). `updatedAt` still rides in the SELECT, as in v4 — vestigial since
/// `f7f3d7bf0` deleted the staleness gate that read it; kept so the statement
/// stays v4's byte for byte, and simply not read.
///
/// The length guard alone is not enough: a chunk can sit under the
/// 131,072-char transport cap yet exceed the embedding model's token context
/// (e.g. >8,192 tokens ≈ ~31k chars for text-embedding-3-large). Those fail
/// deterministically — `is_permanent_embedding_error` marks them FAILED without
/// retry — so any chunk with a FAILED `embedding_status` for the profile the
/// re-embed would actually use is excluded from arm (B), or its chat would
/// re-render and re-fail on every boot (v4 `a5d6cee5`).
///
/// Arm (C) is the one exception that DOES pull those oversize chunks back in —
/// but only now that the renderer sub-chunks them (Bug 17). A chunk over the
/// per-chunk budget yet inside the transport cap predates interchange
/// sub-chunking; re-rendering (not re-embedding) splits it into in-context chunks
/// that embed, so FAILED status is deliberately NOT excluded here — those are
/// exactly the chunks arm (B) skips. Self-limiting: a re-rendered chat has no
/// over-budget chunk left, so it stops matching. Like the others, it is gated by
/// the stale-chat check in the loop below.
const SELECT_INCOMPLETE_CHATS: &str = "\
  SELECT c.\"id\" AS chatId, c.\"userId\" AS userId, c.\"updatedAt\" AS updatedAt
  FROM \"chats\" c
  WHERE (
    -- (A) Real messages but no conversation_chunks rows at all — never chunked.
    NOT EXISTS (
      SELECT 1 FROM \"conversation_chunks\" cc0 WHERE cc0.\"chatId\" = c.\"id\"
    )
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
      -- qt_text() decodes the compressed-text column so LENGTH counts
      -- CHARACTERS of the rendered chunk, not bytes of its brotli blob
      -- (v4 `reconcile-conversation-rendering.ts:108-112`). Unwrapped, this
      -- compared a compressed byte count against a character budget — which
      -- was SILENT-WRONG, never an error: `LENGTH(<blob>)` counts compressed
      -- bytes, measured in `db/text_compression.rs`.
      AND LENGTH(qt_text(cc.\"content\")) BETWEEN 1 AND ?1
      AND NOT EXISTS (
        SELECT 1 FROM \"embedding_status\" es
        WHERE es.\"entityType\" = 'CONVERSATION_CHUNK'
          AND es.\"entityId\" = cc.\"id\"
          AND es.\"profileId\" = ?2
          AND es.\"status\" = 'FAILED'
      )
  ) OR EXISTS (
    -- (C) A sub-chunkable oversize chunk (Bug 17): un-embedded, over the
    --     per-chunk budget but still within the transport cap. Re-rendering now
    --     splits it into in-context chunks. FAILED status is deliberately NOT
    --     excluded — these are exactly the chunks arm (B) skips, and a re-render
    --     (not a re-embed) is what heals them. Self-limiting once split.
    SELECT 1 FROM \"conversation_chunks\" cc2
    WHERE cc2.\"chatId\" = c.\"id\"
      AND cc2.\"embedding\" IS NULL
      AND LENGTH(qt_text(cc2.\"content\")) > ?3
      AND LENGTH(qt_text(cc2.\"content\")) <= ?4
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
            // v4 logs `err.message` — SQLite's bare sentence, never `DbError`'s
            // `sqlite error: ` Display (dogfood #147, the #137/#140 class).
            tracing::warn!(
                target: "quilltap::boot",
                error = %crate::db::fallback::error_text(&e),
                "Failed to scan for incomplete conversations; skipping reconciliation",
            );
            return result;
        }
    };

    result.incomplete_chats = rows.len();
    if rows.is_empty() {
        return result;
    }
    tracing::info!(
        target: "quilltap::boot",
        count = rows.len(),
        "Conversation render reconciliation: found incomplete conversations",
    );

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

fn scan(main: &Connection, default_profile_id: &str) -> Result<Vec<IncompleteChat>, DbError> {
    let mut stmt = main.prepare(SELECT_INCOMPLETE_CHATS)?;
    let rows = stmt.query_map(
        // v4 binds (EMBEDDING_MAX_CHARS, defaultProfileId, CHUNK_CHAR_BUDGET,
        // EMBEDDING_MAX_CHARS) — arm B's cap + FAILED scope, then arm C's window.
        params![
            EMBEDDING_MAX_CHARS as i64,
            default_profile_id,
            CHUNK_CHAR_BUDGET as i64,
            EMBEDDING_MAX_CHARS as i64
        ],
        |r| {
            Ok(IncompleteChat {
                chat_id: r.get(0)?,
                user_id: r.get(1)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recent stamp and a months-old one — "stale" by the 30-day window the
    /// deleted gate used; since v4 `f7f3d7bf0` the scan treats them alike.
    const FRESH_ISO: &str = "2026-06-07T00:00:00.000Z";
    const OLD_ISO: &str = "2026-01-01T00:00:00.000Z";

    /// The schema subset the scan reads. Deliberately
    /// hand-rolled rather than provisioned: the point is to exercise the SQL,
    /// and a missing table is one of the cases under test.
    ///
    /// (`chat_messages.customAnnouncer` was load-bearing while the staleness
    /// gate lived here; v4 `f7f3d7bf0` deleted the gate, and the column stays
    /// only as a faithful slice of the table.)
    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // The scan's three `LENGTH(qt_text(cc."content"))` need the UDF, just
        // as every real connection does (P4.D203). A raw test open registers
        // nothing, so without this the six tests below fail with "no such
        // function: qt_text" — which is the loud failure the codec's doc calls
        // for, arriving here as six reds the moment the wrap landed.
        crate::db::text_compression::register_qt_text(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE chats (\
                id TEXT PRIMARY KEY, userId TEXT, renderedMarkdown TEXT, updatedAt TEXT);\
             CREATE TABLE chat_messages (\
                id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, \
                systemSender TEXT, customAnnouncer TEXT, createdAt TEXT);\
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
        let (result, lines) =
            crate::test_support::captured_with(|| reconcile_conversation_rendering(&conn));
        assert_eq!(result, ReconcileResult::default());
        // v4's `err.message`: the bare SQLite sentence (dogfood #147).
        let warn = lines
            .iter()
            .find(|l| l.contains("Failed to scan for incomplete conversations"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(warn.contains("error=no such table: "), "{warn}");
        assert!(!warn.contains("sqlite error"), "{warn}");
    }

    /// v4 `f7f3d7bf0` INVERTED v4 `a0243abd` ("enqueues a render for a stale
    /// chat too"): the cache collapse no longer cold-tiers, so a stale chat is
    /// healed like any other — the first-boot re-embed of the old cold tier.
    #[test]
    fn stale_chats_are_healed_too() {
        let conn = test_conn();
        chat_at(&conn, "quiet", None, OLD_ISO);
        message(&conn, "m-old", "quiet", "message", "USER");
        conn.execute(
            "UPDATE chat_messages SET createdAt = ?1 WHERE id = 'm-old'",
            params![OLD_ISO],
        )
        .unwrap();
        chat(&conn, "fresh", None);
        message(&conn, "m-new", "fresh", "message", "USER");

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!(
            (r.incomplete_chats, r.enqueued, r.reused, r.failed),
            (2, 2, 0, 0)
        );
        let mut ids = enqueued_chat_ids(&conn);
        ids.sort();
        assert_eq!(ids, vec!["fresh", "quiet"]);
    }

    /// v4 `f7f3d7bf0` re-keyed arm (A) on chunk ABSENCE ("keys arm (A) on
    /// messages with no conversation_chunks rows at all"): a chat with real
    /// messages and fully embedded chunks but NO stored Markdown is healthy now
    /// (it matched arm A before), and a chat with a stored transcript but no
    /// chunks at all needs work (it matched nothing before). The test table
    /// keeps the column — a migrated instance — to prove nothing reads it.
    #[test]
    fn arm_a_keys_on_chunk_absence_not_the_column() {
        let conn = test_conn();
        chat(&conn, "chunked-no-column", None);
        message(&conn, "m1", "chunked-no-column", "message", "USER");
        chunk(&conn, "c1", "chunked-no-column", "embedded", true);
        chat(&conn, "column-no-chunks", Some("# a stored transcript"));
        message(&conn, "m2", "column-no-chunks", "message", "ASSISTANT");

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!((r.incomplete_chats, r.enqueued), (1, 1));
        assert_eq!(enqueued_chat_ids(&conn), vec!["column-no-chunks"]);
    }

    /// v4's pre-loop INFO (restored — v5 had never emitted it) fires only when
    /// the scan found work, with v4's single `count` field.
    #[test]
    fn the_found_line_fires_only_when_there_is_work() {
        crate::test_support::global_capture::install();
        let conn = test_conn();
        let (_, quiet) = crate::test_support::global_capture::capture(|| {
            reconcile_conversation_rendering(&conn)
        });
        assert!(
            !quiet
                .iter()
                .any(|l| l.contains("found incomplete conversations")),
            "{quiet:?}"
        );
        chat(&conn, "arm-a", None);
        message(&conn, "m1", "arm-a", "message", "USER");
        let (_, lines) = crate::test_support::global_capture::capture(|| {
            reconcile_conversation_rendering(&conn)
        });
        let found: Vec<&String> = lines
            .iter()
            .filter(|l| {
                l.contains("Conversation render reconciliation: found incomplete conversations")
            })
            .collect();
        assert_eq!(found.len(), 1, "{lines:?}");
        assert!(
            found[0].starts_with("INFO quilltap::boot") && found[0].contains("count=1"),
            "{}",
            found[0]
        );
        assert!(
            !lines.iter().any(|l| l.contains("Staleness check failed")),
            "the stale WARN is gone: {lines:?}"
        );
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

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!(r.incomplete_chats, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["failed-under-other"]);
    }

    /// v4 Bug 17 arm (C): a chunk OVER the per-chunk budget but inside the
    /// transport cap is reclaimed even when it is FAILED for the default profile
    /// — exactly the chunk arm (B) skips. Re-rendering sub-chunks it. A chunk
    /// beyond the cap stays excluded.
    #[test]
    fn arm_c_reclaims_failed_oversize_chunks() {
        let conn = test_conn();
        profile(&conn, "p-default", true);

        // Rendered (arm A off) + a window chunk FAILED for the default profile
        // (arm B off) → only arm (C) can select it.
        chat(&conn, "sub-chunkable", Some("rendered"));
        chunk(
            &conn,
            "c-window",
            "sub-chunkable",
            &"x".repeat(CHUNK_CHAR_BUDGET + 1),
            false,
        );
        failed_status(&conn, "c-window", "p-default");

        // Beyond the transport cap → arm (C)'s `<= EMBEDDING_MAX_CHARS` excludes it.
        chat(&conn, "beyond-cap", Some("rendered"));
        chunk(
            &conn,
            "c-huge",
            "beyond-cap",
            &"x".repeat(EMBEDDING_MAX_CHARS + 1),
            false,
        );
        failed_status(&conn, "c-huge", "p-default");

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!(r.incomplete_chats, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["sub-chunkable"]);
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

        let r = reconcile_conversation_rendering(&conn);
        assert_eq!(r.incomplete_chats, 1);
        assert_eq!(enqueued_chat_ids(&conn), vec!["failed-but-no-profile"]);
    }
}
