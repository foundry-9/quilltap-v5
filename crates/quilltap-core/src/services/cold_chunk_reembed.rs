//! Cold-tier conversation-chunk re-embedding (re-index on demand) — v4
//! `lib/scriptorium/cold-chunk-reembed.ts`.
//!
//! The stale-chat maintenance sweep cold-tiers quiet chats by NULLing their
//! `conversation_chunks.embedding` BLOBs (content kept — see
//! [`super::collapse_stale_chat_caches`]). While cold, a chat stays fully
//! readable and keyword-searchable, but semantic retrieval won't surface it.
//! This module restores warmth transparently: when a cold chat is opened, it
//! detects chunks with content but no embedding and enqueues per-chunk
//! `EMBEDDING_GENERATE` jobs through the exact pipeline the normal chunk indexer
//! uses (same default profile, same per-entity dedup in
//! [`super::queue_service::enqueue_embedding_generate`]).
//!
//! Fire-and-forget from the chat GET path — a failure here must never break
//! loading the conversation. An in-memory per-chat debounce keeps repeated opens
//! from re-scanning the chunk table on every request; the queue's per-entity
//! dedup is the hard guarantee against stacked jobs.
//!
//! Parent-process only (enqueueing writes job rows). The debounce clock is
//! injected (`now_ms`) so the differential can pin it (the
//! `[[chain-depth-frozen-clock-artifact]]` family).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::json;

use crate::db::conversation_chunks::ConversationChunksRepository;
use crate::db::embedding_profiles;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::queue_service::enqueue_embedding_generate;

/// v4 `DEBOUNCE_MS` — don't re-scan a chat's chunks more than once per window
/// per process.
pub const DEBOUNCE_MS: i64 = 5 * 60 * 1000;

/// chatId → epoch ms of the last scan kicked off for it (v4's module-level
/// `lastScanAt` Map).
static LAST_SCAN_AT: LazyLock<Mutex<HashMap<String, i64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Test hook: forget all debounce state (v4 `_resetForTesting`).
pub fn reset_debounce_for_testing() {
    LAST_SCAN_AT.lock().unwrap().clear();
}

/// If the chat has conversation chunks with content but no embedding (the
/// cold-tiered state), enqueue a re-embed job for each through the standard
/// chunk-embedding pipeline (v4 `maybeEnqueueColdChunkReembed`). Safe to call on
/// every chat open: debounced in-process, deduped per entity in the queue, and a
/// warm chat exits after one cheap COUNT query.
///
/// Returns the number of chunks newly enqueued (0 when debounced, warm,
/// chunkless, or no embedding profile is configured). `now_ms` is the injected
/// debounce clock.
pub async fn maybe_enqueue_cold_chunk_reembed(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    now_ms: i64,
) -> Result<usize, DbError> {
    // Debounce: one scan per DEBOUNCE_MS window per chat per process.
    {
        let mut last = LAST_SCAN_AT.lock().unwrap();
        if let Some(&prev) = last.get(chat_id) {
            if now_ms - prev < DEBOUNCE_MS {
                return Ok(0);
            }
        }
        last.insert(chat_id.to_string(), now_ms);
    }

    // One COUNT pair decides cold vs warm without loading chunk bodies.
    let cid = chat_id.to_string();
    let (total, embedded) =
        db.read_main(move |c| ConversationChunksRepository::new(c).count_stats_by_chat_id(&cid))?;
    if total == 0 || embedded >= total {
        return Ok(0); // chunkless or fully warm
    }

    // Pick the DEFAULT embedding profile — and only the default.
    let Some(profile_id) = db.read_main(embedding_profiles::pick_reembed_profile_id)? else {
        // Cold chat detected but no DEFAULT embedding profile configured — skip
        // (v4 `d553f72a`: no arbitrary-profile fallback).
        return Ok(0);
    };

    // Enqueue one EMBEDDING_GENERATE per cold chunk (deduped per entity in the
    // queue). v4 counts `result.isNew`.
    let cid = chat_id.to_string();
    let cold_ids = db.read_main(move |c| {
        ConversationChunksRepository::new(c).find_cold_chunk_ids_by_chat_id(&cid)
    })?;
    let mut enqueued = 0usize;
    for entity_id in cold_ids {
        let payload = json!({
            "entityType": "CONVERSATION_CHUNK",
            "entityId": entity_id,
            "chatId": chat_id,
            "profileId": profile_id,
        });
        let (_, is_new) = enqueue_embedding_generate(db, user_id, payload).await?;
        if is_new {
            enqueued += 1;
        }
    }
    Ok(enqueued)
}
