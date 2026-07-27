//! The unembedded-conversation-chunk boot repair (P4.6BL, tier-1 item 6) —
//! a **deliberate v5-only repair** for the wound dogfood finding #35 measured:
//! v5 ran for days with both EMBEDDING_GENERATE enqueue families live and NO
//! handler registered, so every enqueued job died to DEAD and every
//! conversation chunk written (or cold-tiered) in that window is unembedded —
//! invisible to semantic search — with no automatic path back.
//!
//! ## Why this is not a port of v4's startup reconcile
//!
//! v4's boot-time self-heal is `lib/startup/reconcile-conversation-rendering.ts`:
//! it scans for half-finished chats (arm A — real messages but no rendered
//! Markdown; arm B — recoverable un-embedded interchange chunks) and enqueues
//! **CONVERSATION_RENDER**, whose handler re-chunks and then re-enqueues
//! EMBEDDING_GENERATE per still-unembedded chunk. v5 has NOT ported the
//! CONVERSATION_RENDER handler (it stays on the runner's loud
//! recognized-but-unavailable fallback), so a verbatim port of the reconcile
//! would mint a fresh batch of DEAD render jobs on every boot — re-creating
//! the exact wound this lane heals, one job type over. Instead this repair
//! takes arm (B) alone and enqueues **EMBEDDING_GENERATE directly** for every
//! recoverable chunk, using v4's own recoverability filter (non-empty and
//! within `EMBEDDING_MAX_CHARS`, v4's SQL `LENGTH(content) BETWEEN 1 AND ?` —
//! oversized/empty chunks are deterministically unembeddable and counting
//! them would re-scan them on every boot for nothing). Arm (A) stays with the
//! deferred CONVERSATION_RENDER port, and this module retires in favor of the
//! reconcile when that handler lands.
//!
//! Like v4's reconcile, this runs on every startup and is a no-op on a healthy
//! instance (one indexed scan finding nothing). The per-entity dedup mirrors
//! v4's `enqueueEmbeddingGenerate`: an in-flight (PENDING/PROCESSING) job for
//! the chunk is reused, and DEAD rows deliberately do NOT block re-enqueue.
//! Skips quietly when no default embedding profile (or no user) exists — the
//! same guard every scan-and-enqueue surface has.

use rusqlite::{params, Connection};
use serde_json::json;
use uuid::Uuid;

use super::embedding_generate_job::EMBEDDING_MAX_CHARS;
use super::mount_index::embedding_scheduler::{default_profile_id, first_user_id};
use crate::clock::now_iso;
use crate::db::background_jobs::{BackgroundJobsRepository, BjCreate, CreateOptions};
use crate::db::DbError;

/// What one repair pass did (mirrors the shape of v4's reconcile result).
#[derive(Debug, Default, PartialEq)]
pub struct BacklogRepairResult {
    /// Recoverable un-embedded chunks found.
    pub scanned: usize,
    /// New EMBEDDING_GENERATE jobs enqueued.
    pub enqueued: usize,
    /// Chunks that already had an in-flight job (deduped).
    pub reused: usize,
    /// True when the pass skipped entirely (no default embedding profile or no
    /// user row — nothing can be enqueued).
    pub skipped: bool,
}

/// Enqueue an `EMBEDDING_GENERATE` job for every recoverable un-embedded
/// conversation chunk (v4 reconcile arm B, re-targeted at the embed job —
/// see the module doc). Runs on the writer connection at boot; idempotent.
pub fn repair_unembedded_conversation_chunks(
    main: &Connection,
) -> Result<BacklogRepairResult, DbError> {
    let mut result = BacklogRepairResult::default();

    // v4 creates its collections lazily, so an instance that never chunked (or
    // never configured embedding) legitimately lacks these tables — a boot
    // repair must skip, never fail the boot (the P4.9G3 missing-table lesson).
    for table in [
        "conversation_chunks",
        "background_jobs",
        "embedding_profiles",
        "users",
    ] {
        if !table_exists(main, table)? {
            result.skipped = true;
            return Ok(result);
        }
    }

    let Some(profile_id) = default_profile_id(main)? else {
        result.skipped = true;
        return Ok(result);
    };
    let Some(user_id) = first_user_id(main)? else {
        result.skipped = true;
        return Ok(result);
    };

    // v4's recoverable-chunk filter, verbatim SQL semantics: non-empty and
    // within the embedder's cap (`LENGTH` on TEXT is SQLite's character count,
    // exactly what v4 binds `EMBEDDING_MAX_CHARS` against). A whitespace-only
    // chunk passes the filter — as in v4 — and the handler's empty-input guard
    // then marks it failed once, without retry.
    let mut stmt = main.prepare(
        "SELECT id, chatId FROM conversation_chunks \
         WHERE embedding IS NULL AND LENGTH(content) BETWEEN 1 AND ?1 \
         ORDER BY rowid ASC",
    )?;
    let chunks: Vec<(String, String)> = stmt
        .query_map(params![EMBEDDING_MAX_CHARS as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<_, _>>()?;
    result.scanned = chunks.len();

    let repo = BackgroundJobsRepository::new(main);
    for (chunk_id, chat_id) in chunks {
        // The per-entity dedup (v4 `enqueueEmbeddingGenerate`): PENDING /
        // PROCESSING only — DEAD rows never block the heal.
        let in_flight = repo.find_pending_for_entity(&chunk_id)?;
        if in_flight.iter().any(|j| j.job_type == "EMBEDDING_GENERATE") {
            result.reused += 1;
            continue;
        }
        let now = now_iso();
        let create = BjCreate {
            user_id: user_id.clone(),
            job_type: "EMBEDDING_GENERATE".to_string(),
            status: Some("PENDING".to_string()),
            payload: json!({
                "entityType": "CONVERSATION_CHUNK",
                "entityId": chunk_id,
                "chatId": chat_id,
                "profileId": profile_id,
            }),
            // v4's EMBEDDING_ENTITY_PRIORITIES: CONVERSATION_CHUNK is a
            // real-time kind (10); maxAttempts 3 as everywhere in the family.
            priority: 10.0,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        };
        let opts = CreateOptions {
            id: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        repo.create(&create, &opts)?;
        result.enqueued += 1;
    }

    Ok(result)
}

/// True iff `table` exists in the main schema.
fn table_exists(conn: &Connection, table: &str) -> Result<bool, DbError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE conversation_chunks (\
                id TEXT PRIMARY KEY, chatId TEXT, interchangeIndex REAL, content TEXT, \
                participantNames TEXT, messageIds TEXT, embedding BLOB, \
                createdAt TEXT, updatedAt TEXT);\
             CREATE TABLE background_jobs (\
                id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, \
                status TEXT NOT NULL, payload TEXT NOT NULL, priority REAL NOT NULL, \
                attempts REAL NOT NULL, maxAttempts REAL NOT NULL, lastError TEXT, \
                scheduledAt TEXT NOT NULL, startedAt TEXT, completedAt TEXT, \
                createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
             CREATE TABLE embedding_profiles (\
                id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
                isDefault INTEGER, createdAt TEXT, updatedAt TEXT);\
             CREATE TABLE users (id TEXT PRIMARY KEY, createdAt TEXT);",
        )
        .unwrap();
        conn
    }

    fn seed_profile_and_user(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO embedding_profiles (id, userId, name, provider, isDefault, createdAt, updatedAt) \
               VALUES ('p1', 'u1', 'Default', 'BUILTIN', 1, '2026-01-01', '2026-01-01');\
             INSERT INTO users (id, createdAt) VALUES ('u1', '2026-01-01');",
        )
        .unwrap();
    }

    fn seed_chunk(conn: &Connection, id: &str, content: &str, embedded: bool) {
        let embedding: Option<Vec<u8>> = embedded.then(|| vec![0u8; 8]);
        conn.execute(
            "INSERT INTO conversation_chunks \
               (id, chatId, interchangeIndex, content, participantNames, messageIds, \
                embedding, createdAt, updatedAt) \
             VALUES (?1, 'chat1', 0, ?2, '[]', '[]', ?3, '2026-01-01', '2026-01-01')",
            params![id, content, embedding],
        )
        .unwrap();
    }

    fn job_entity_ids(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT json_extract(payload, '$.entityId') FROM background_jobs ORDER BY rowid",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// The filter: unembedded non-empty within-cap chunks enqueue; embedded,
    /// truly empty, and oversize chunks do not. Whitespace-only DOES enqueue
    /// (v4's `LENGTH >= 1` — the handler's guard fails it once, no retry).
    #[test]
    fn enqueues_only_recoverable_chunks() {
        let conn = test_conn();
        seed_profile_and_user(&conn);
        seed_chunk(&conn, "c-plain", "hello world", false);
        seed_chunk(&conn, "c-embedded", "already embedded", true);
        seed_chunk(&conn, "c-empty", "", false);
        seed_chunk(&conn, "c-ws", "   ", false);
        let oversize = "x".repeat(EMBEDDING_MAX_CHARS + 1);
        seed_chunk(&conn, "c-oversize", &oversize, false);

        let result = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert_eq!(result.scanned, 2);
        assert_eq!(result.enqueued, 2);
        assert_eq!(result.reused, 0);
        assert!(!result.skipped);
        assert_eq!(job_entity_ids(&conn), vec!["c-plain", "c-ws"]);

        // Payload shape + priority.
        let (priority, payload): (f64, String) = conn
            .query_row(
                "SELECT priority, payload FROM background_jobs LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(priority, 10.0);
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(p["entityType"], "CONVERSATION_CHUNK");
        assert_eq!(p["chatId"], "chat1");
        assert_eq!(p["profileId"], "p1");
    }

    /// Idempotence + dedup: a second pass reuses the in-flight jobs; a DEAD row
    /// deliberately does NOT block re-enqueue (the wound's own rows must not
    /// pin the wound open).
    #[test]
    fn dedups_in_flight_but_not_dead() {
        let conn = test_conn();
        seed_profile_and_user(&conn);
        seed_chunk(&conn, "c1", "one", false);

        let first = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert_eq!((first.enqueued, first.reused), (1, 0));

        let second = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert_eq!((second.enqueued, second.reused), (0, 1));

        // Kill the job (the finding-#35 state) — the next pass re-enqueues.
        conn.execute("UPDATE background_jobs SET status = 'DEAD'", [])
            .unwrap();
        let third = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert_eq!((third.enqueued, third.reused), (1, 0));
    }

    /// Missing tables (a lazily-created v4 instance that never chunked) → the
    /// pass skips instead of failing the boot.
    #[test]
    fn skips_when_tables_missing() {
        let conn = Connection::open_in_memory().unwrap();
        let result = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert!(result.skipped);
    }

    /// No default profile (or no user) → the whole pass skips without writing.
    #[test]
    fn skips_without_profile_or_user() {
        let conn = test_conn();
        seed_chunk(&conn, "c1", "one", false);
        let result = repair_unembedded_conversation_chunks(&conn).unwrap();
        assert!(result.skipped);
        assert_eq!(job_entity_ids(&conn), Vec::<String>::new());
    }
}
