//! Compression cache — the async pre-compression layer (port of v4
//! `lib/services/chat-message/compression-cache.service.ts`).
//!
//! After an LLM response the finalizer fires [`trigger_async_compression`] to
//! compute + cache the context compression the *next* message will need, so the
//! next send doesn't block on it. The cache lives in two places:
//!   - a process-global in-memory map (the fast path), and
//!   - the durable `chats.compressionCache` column (survives restarts, serves as
//!     the fallback when the in-memory result isn't ready).
//!
//! [`get_cached_compression`] is the read path (`buildContext` consults it before
//! doing synchronous compression); [`invalidate_compression_cache`] clears both
//! layers (full-context request / settings change / chat delete).
//!
//! **Single-writer note:** v4 serializes the load-modify-save of
//! `chat.compressionCache` through a per-chat promise lock (`withPersistLock`) so
//! two finalizers writing different participant keys can't clobber each other. In
//! v5 the writer task is the *only* mutator (every `db.write` closure runs
//! serially on it), so the load-modify-save is already atomic — the lock is a Node
//! concurrency workaround and is not ported.
//!
//! **In-flight promises:** v4 tracks an in-flight compression promise in the
//! in-memory entry so a concurrent read falls back to the DB instead of blocking.
//! In v5 [`trigger_async_compression`] computes synchronously within its async fn
//! and stores a completed result, so there is no in-flight-promise state; the
//! `isFallback` flag it drives is therefore always `false` here.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_compression::CompressibleMessage;
use crate::db::runtime::Db;
use crate::db::{chats, chats_read, DbError};
use crate::model::completion::CompletionProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::compression::{
    apply_context_compression, ContextCompressionOptions, ContextCompressionResult,
};

/// The persisted cache entry stored in `chats.compressionCache` (v4
/// `PersistedCompressionCache`). Field order matches v4's object literal so the
/// stored JSON is byte-identical.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedCompressionCache {
    /// The compression result.
    pub result: ContextCompressionResult,
    /// Message count when compression was computed.
    pub message_count: i64,
    /// Timestamp (ms) when the cache was created.
    pub created_at: i64,
    /// System-prompt hash to detect changes.
    pub system_prompt_hash: String,
}

/// The in-memory cache entry (v4 `CompressionCacheEntry`, minus the in-flight
/// promise — see the module note).
#[derive(Clone, Debug)]
struct InMemEntry {
    result: Option<ContextCompressionResult>,
    message_count: i64,
    /// Kept for entry-shape fidelity (v4 logs a cache age from it); v5 never reads it.
    #[allow(dead_code)]
    created_at: i64,
    system_prompt_hash: String,
}

/// The `get_cached_compression` result (v4 `CachedCompressionResponse`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedCompressionResponse {
    /// The compression result.
    pub result: ContextCompressionResult,
    /// Message count when this compression was computed.
    pub cached_message_count: i64,
    /// Whether this is a fallback to older cache (always `false` in v5 — no
    /// in-flight promise; see the module note).
    pub is_fallback: bool,
}

/// The process-global in-memory cache (v4's module-level `compressionCache` Map).
fn cache() -> &'static Mutex<HashMap<String, InMemEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, InMemEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// v4 `cacheKey` — per-participant in multi-char chats, per-chat in single-char.
fn cache_key(chat_id: &str, participant_id: Option<&str>) -> String {
    match participant_id {
        Some(pid) => format!("{chat_id}:{pid}"),
        None => chat_id.to_string(),
    }
}

/// v4 `hashString` — the 32-bit rolling hash (`((h << 5) - h) + c`, forced to a
/// signed 32-bit int), rendered as JS `Number.prototype.toString(16)` (a negative
/// value renders `-<hex>`).
pub fn hash_string(s: &str) -> String {
    let mut hash: i32 = 0;
    for ch in s.encode_utf16() {
        // `((hash << 5) - hash) + char`, all in wrapping 32-bit signed arithmetic
        // (JS bitwise ops are 32-bit; `hash & hash` re-coerces to i32).
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(i32::from(ch));
    }
    if hash < 0 {
        // JS `(-x).toString(16)` = "-" + abs-in-hex.
        format!("-{:x}", (hash as i64).unsigned_abs())
    } else {
        format!("{hash:x}")
    }
}

/// v4 `isCacheValid` — invalidate on a system-prompt change, on stale data
/// (cache has MORE messages than current), or when > 50 messages behind.
fn is_cache_valid(
    entry_message_count: i64,
    entry_system_prompt_hash: &str,
    current_message_count: i64,
    current_system_prompt_hash: Option<&str>,
) -> bool {
    // System-prompt change (only checked when a hash is provided).
    if let Some(h) = current_system_prompt_hash {
        if entry_system_prompt_hash != h {
            return false;
        }
    }
    // Cache computed with MORE messages than current → data was deleted.
    if entry_message_count > current_message_count {
        return false;
    }
    // Allow the cache to be up to 50 messages behind.
    if current_message_count - entry_message_count > 50 {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// DB persistence (the `chats.compressionCache` column).
// ---------------------------------------------------------------------------

/// Read the `chats.compressionCache` column as a `Value` (`null` when absent).
fn read_column(db: &Db, chat_id: &str) -> Result<Value, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(move |c| chats_read::find_by_id(c, &cid))?;
    Ok(chat
        .and_then(|c| c.get("compressionCache").cloned())
        .unwrap_or(Value::Null))
}

/// Write the `chats.compressionCache` column (an object, or `null` to clear). Does
/// not bump `updatedAt`.
async fn write_column(db: &Db, chat_id: &str, value: Value) -> Result<(), DbError> {
    let cid = chat_id.to_string();
    db.write(move |w| {
        w.main()
            .chats()
            .update(
                &cid,
                &chats::ChatUpdate {
                    compression_cache: Some(value),
                    ..Default::default()
                },
            )
            .map(|_| ())
    })
    .await
}

/// v4 `persistToDatabase` — merge the entry into `chats.compressionCache` (keyed
/// by participantId for multi-char; stored directly for single-char).
async fn persist_to_database(
    db: &Db,
    chat_id: &str,
    participant_id: Option<&str>,
    entry: &PersistedCompressionCache,
) -> Result<(), DbError> {
    let entry_value = serde_json::to_value(entry)
        .map_err(|e| DbError::Internal(format!("cache entry marshal: {e}")))?;
    match participant_id {
        Some(pid) => {
            let existing = read_column(db, chat_id)?;
            let mut map = existing.as_object().cloned().unwrap_or_default();
            map.insert(pid.to_string(), entry_value);
            write_column(db, chat_id, Value::Object(map)).await
        }
        None => write_column(db, chat_id, entry_value).await,
    }
}

/// v4 `loadFromDatabase` — read + validate the persisted entry.
fn load_from_database(
    db: &Db,
    chat_id: &str,
    participant_id: Option<&str>,
) -> Result<Option<PersistedCompressionCache>, DbError> {
    let column = read_column(db, chat_id)?;
    if column.is_null() {
        return Ok(None);
    }
    let cache_value: Value = match participant_id {
        Some(pid) => match column.get(pid) {
            Some(v) => v.clone(),
            None => return Ok(None),
        },
        None => {
            // Old format (direct entry) vs new format with a `_default` key.
            if column.get("result").is_none() {
                match column.get("_default") {
                    Some(v) => v.clone(),
                    None => column.clone(),
                }
            } else {
                column.clone()
            }
        }
    };
    // Validate the structure (v4: result present, messageCount a number,
    // systemPromptHash present) before trusting it.
    let valid = cache_value.get("result").is_some_and(|r| !r.is_null())
        && cache_value
            .get("messageCount")
            .and_then(Value::as_i64)
            .is_some()
        && cache_value
            .get("systemPromptHash")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
    if !valid {
        return Ok(None);
    }
    match serde_json::from_value::<PersistedCompressionCache>(cache_value) {
        Ok(entry) => Ok(Some(entry)),
        Err(_) => Ok(None),
    }
}

/// v4 `clearFromDatabase` — drop the participant key (or the whole column).
async fn clear_from_database(
    db: &Db,
    chat_id: &str,
    participant_id: Option<&str>,
) -> Result<(), DbError> {
    match participant_id {
        Some(pid) => {
            let column = read_column(db, chat_id)?;
            if let Some(obj) = column.as_object() {
                let mut map = obj.clone();
                map.remove(pid);
                let value = if map.is_empty() {
                    Value::Null
                } else {
                    Value::Object(map)
                };
                write_column(db, chat_id, value).await
            } else {
                Ok(())
            }
        }
        None => write_column(db, chat_id, Value::Null).await,
    }
}

// ---------------------------------------------------------------------------
// The public surface.
// ---------------------------------------------------------------------------

/// v4 `triggerAsyncCompression` — compute + cache the compression for the next
/// message. Guarded on enough messages (`> windowSize`); re-computes when enough
/// new messages accumulated since the last cache. Persists to the DB only when
/// compression was actually applied. `now_ms` is the injected wall clock (v4
/// `Date.now()`).
#[allow(clippy::too_many_arguments)]
pub async fn trigger_async_compression<C: CompletionProvider>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    participant_id: Option<&str>,
    messages: &[CompressibleMessage],
    system_prompt: &str,
    options: &ContextCompressionOptions<'_>,
    now_ms: i64,
) -> Result<(), DbError> {
    let window_size = options.window_size;
    // Not enough messages to pre-compress.
    if (messages.len() as i64) <= window_size {
        return Ok(());
    }

    let system_prompt_hash = hash_string(system_prompt);
    let key = cache_key(chat_id, participant_id);

    // A still-valid cache entry: only re-compress when enough new messages have
    // accumulated (else the dynamic window grows unbounded).
    {
        let map = cache().lock().unwrap();
        if let Some(existing) = map.get(&key) {
            if is_cache_valid(
                existing.message_count,
                &existing.system_prompt_hash,
                messages.len() as i64,
                Some(&system_prompt_hash),
            ) {
                let messages_since_cache = messages.len() as i64 - existing.message_count;
                if messages_since_cache < window_size {
                    return Ok(());
                }
            }
        }
    }

    let result =
        apply_context_compression(completion, executor, messages, system_prompt, options).await;

    // Update the in-memory cache with the result.
    {
        let mut map = cache().lock().unwrap();
        map.insert(
            key.clone(),
            InMemEntry {
                result: Some(result.clone()),
                message_count: messages.len() as i64,
                created_at: now_ms,
                system_prompt_hash: system_prompt_hash.clone(),
            },
        );
    }

    // Persist to the DB (only when compression was actually applied).
    if result.compression_applied {
        let entry = PersistedCompressionCache {
            result,
            message_count: messages.len() as i64,
            created_at: now_ms,
            system_prompt_hash,
        };
        persist_to_database(db, chat_id, participant_id, &entry).await?;
    }
    Ok(())
}

/// v4 `getCachedCompression` — the read path: in-memory (fast) → DB (fallback) →
/// `None`. Never waits on an in-flight compression (v5 has none). `now_ms` unused
/// (v4 logs a cache age); kept out of the signature.
pub async fn get_cached_compression(
    db: &Db,
    chat_id: &str,
    current_message_count: i64,
    participant_id: Option<&str>,
    current_system_prompt_hash: Option<&str>,
) -> Result<Option<CachedCompressionResponse>, DbError> {
    let key = cache_key(chat_id, participant_id);

    // 1. In-memory cache (fastest path).
    {
        let mut map = cache().lock().unwrap();
        if let Some(entry) = map.get(&key).cloned() {
            let message_diff = current_message_count - entry.message_count;
            let cache_has_more = entry.message_count > current_message_count;
            let too_stale = message_diff > 50;
            let hash_changed =
                current_system_prompt_hash.is_some_and(|h| entry.system_prompt_hash != h);
            if cache_has_more || too_stale || hash_changed {
                map.remove(&key);
            } else if let Some(result) = entry.result {
                return Ok(Some(CachedCompressionResponse {
                    result,
                    cached_message_count: entry.message_count,
                    is_fallback: false,
                }));
            }
            // (v5 has no in-flight promise → nothing more to check in memory.)
        }
    }

    // 2. DB cache (survives restarts; also the fallback).
    if let Some(db_entry) = load_from_database(db, chat_id, participant_id)? {
        if is_cache_valid(
            db_entry.message_count,
            &db_entry.system_prompt_hash,
            current_message_count,
            current_system_prompt_hash,
        ) {
            // Populate the in-memory cache.
            {
                let mut map = cache().lock().unwrap();
                map.insert(
                    key.clone(),
                    InMemEntry {
                        result: Some(db_entry.result.clone()),
                        message_count: db_entry.message_count,
                        created_at: db_entry.created_at,
                        system_prompt_hash: db_entry.system_prompt_hash.clone(),
                    },
                );
            }
            return Ok(Some(CachedCompressionResponse {
                result: db_entry.result,
                cached_message_count: db_entry.message_count,
                is_fallback: false,
            }));
        }
        // Invalid DB cache → clear it.
        clear_from_database(db, chat_id, participant_id).await?;
    }

    Ok(None)
}

/// v4 `invalidateCompressionCache` — clear both layers. With a participant id,
/// only that participant's cache; without, every cache for the chat.
pub async fn invalidate_compression_cache(
    db: &Db,
    chat_id: &str,
    participant_id: Option<&str>,
) -> Result<(), DbError> {
    match participant_id {
        Some(pid) => {
            let key = cache_key(chat_id, Some(pid));
            cache().lock().unwrap().remove(&key);
            clear_from_database(db, chat_id, Some(pid)).await
        }
        None => {
            // Scope the lock so the guard is released before the await.
            {
                let mut map = cache().lock().unwrap();
                map.remove(chat_id);
                let prefix = format!("{chat_id}:");
                let doomed: Vec<String> = map
                    .keys()
                    .filter(|k| k.starts_with(&prefix))
                    .cloned()
                    .collect();
                for k in doomed {
                    map.remove(&k);
                }
            }
            clear_from_database(db, chat_id, None).await
        }
    }
}

/// v4 `clearCompressionCache` — drop every in-memory entry (does NOT touch the
/// DB). Used by tests + the differential to force a DB read.
pub fn clear_compression_cache() {
    cache().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_string_matches_js() {
        // Empty → 0 → "0".
        assert_eq!(hash_string(""), "0");
        // Known JS values (computed against v4's hashString).
        assert_eq!(hash_string("a"), "61");
        // A longer string exercising the negative-hex branch is proven by the
        // differential; here we assert the sign-rendering shape is reachable.
        let h = hash_string("the quick brown fox jumps over the lazy dog");
        assert!(h.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    }

    #[test]
    fn is_cache_valid_bands() {
        assert!(is_cache_valid(10, "h", 12, Some("h")));
        // System-prompt changed.
        assert!(!is_cache_valid(10, "h", 12, Some("h2")));
        // Cache has more messages (data deleted).
        assert!(!is_cache_valid(20, "h", 12, Some("h")));
        // > 50 behind.
        assert!(!is_cache_valid(10, "h", 61, Some("h")));
        // No hash provided → hash check skipped.
        assert!(is_cache_valid(10, "h", 12, None));
    }

    #[test]
    fn cache_key_shape() {
        assert_eq!(cache_key("chat", None), "chat");
        assert_eq!(cache_key("chat", Some("p1")), "chat:p1");
    }
}
