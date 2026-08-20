//! The Memories-server dispatch handlers (P4.6s) — the Commonplace Book route-logic
//! backfill the Memory SPA vertical consumes, composed over the already-ported
//! memory engine (gate / weighting / ranking / recall / housekeeping) + the
//! `memories` / `memories_read` / `tags` / `instance_settings` repos.
//!
//! Each handler is a differential port of a v4 memories route handler (the
//! oracles: `memories/route.ts`, `memories/[id]/route.ts`,
//! `chats/[id]/actions/memories.ts`) and returns a [`Response`] directly (the
//! engine arm is a one-line delegate). Reads are MAIN-only (memories, their
//! vector store, tags, chats, and characters' slim columns all live in the main
//! db); the embedding arms (create / search) take an injected
//! [`EmbeddingProvider`](crate::model::embedding::EmbeddingProvider) so the
//! differential drives them LIVE with the fixture's builtin TF-IDF profile.
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! harness can drive with the fixture's own user id on both sides; the engine
//! passes `SINGLE_USER_ID`. v4's character/chat ownership collapses to
//! NotFound-on-absent for the single-user v5. A present character with a valid
//! vault is present under both the raw and the overlaid finder, so the ownership
//! gate uses the MAIN-only `find_by_id_raw` (id is an overlay-invariant slim
//! column); the only divergence — a character whose vault is unavailable, which
//! v4's overlaid `findById` would drop to NotFound — is out of the fixture.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::db::memories::MemUpdate;
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chat_settings, chats_read, instance_settings, memories_read, tags, DbError,
};
use crate::model::embedding::EmbeddingProvider;
use crate::services::housekeeping::{
    get_housekeeping_preview, run_housekeeping, HousekeepingOptions, HousekeepingResult,
};
use crate::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use crate::services::memory_service::{
    delete_memories_by_chat_id_with_vectors, search_memories_semantic, SemanticSearchOptions,
};
use crate::services::queue_service::{
    enqueue_memory_extraction_batch, enqueue_memory_housekeeping, MemoryExtractionBatchEntry,
};

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
// Used by the write / housekeeping / config families grown into this module in
// the later P4.6s commits.
#[allow(dead_code)]
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// The loud "recognized but not yet available" refusal for memories-family
/// variants whose handler is a documented deferral (tier 3) or awaits its host
/// seam.
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' memories action is recognized but not yet available."),
    )
}

/// Does a character exist (the ownership gate; MAIN-only slim-column read)?
fn character_exists(db: &Db, character_id: &str) -> Result<bool, DbError> {
    let cid = character_id.to_string();
    db.read_main(move |main| Ok(characters_read::find_by_id_raw(main, &cid)?.is_some()))
}

/// Build the `tagId → full-tag` map (v4 `repos.tags.findAll()` → `tagDetails`
/// carries the FULL tag object, not the trimmed detail shape).
fn tag_map(main: &rusqlite::Connection) -> Result<Map<String, Value>, DbError> {
    let all = tags::find_all(main)?;
    let mut m = Map::new();
    for t in all {
        if let Some(id) = t.get("id").and_then(Value::as_str) {
            m.insert(id.to_string(), t.clone());
        }
    }
    Ok(m)
}

/// Attach `tagDetails` to one memory (`memory.tags.map(id => tagMap.get(id)).filter(Boolean)`).
fn with_tag_details(mut memory: Value, tag_map: &Map<String, Value>) -> Value {
    let details: Vec<Value> = memory
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|t| t.as_str())
                .filter_map(|id| tag_map.get(id).cloned())
                .collect()
        })
        .unwrap_or_default();
    if let Some(obj) = memory.as_object_mut() {
        obj.insert("tagDetails".into(), Value::Array(details));
    }
    memory
}

/// v4 `parseFloat`/`parseInt` numeric-string coercion for a query param.
#[allow(dead_code)]
fn parse_f64(s: Option<&str>) -> Option<f64> {
    s.and_then(|v| v.trim().parse::<f64>().ok())
}

// ===========================================================================
// List (v4 GET /api/v1/memories?characterId=)
// ===========================================================================

/// v4 `listMemoriesByCharacter`. Two code paths: `limit` present → the DB
/// paginated query (`{memories, count, totalCount}`); `limit` absent → the legacy
/// unpaginated in-memory filter/sort returning ALL (`totalCount === count`).
/// Every memory enriched with `tagDetails`.
#[allow(clippy::too_many_arguments)]
pub fn memory_list(
    db: &Db,
    character_id: &str,
    search: Option<&str>,
    min_importance: Option<f64>,
    source: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Response {
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }

    let sort_by = sort_by.unwrap_or("createdAt").to_string();
    let sort_order = sort_order.unwrap_or("desc").to_string();
    // v4 only honors `source` when it is exactly AUTO|MANUAL.
    let source = source.filter(|s| *s == "AUTO" || *s == "MANUAL");
    let search_owned = search.map(str::to_string);
    let cid = character_id.to_string();

    let result: Result<Value, DbError> = db.read_main(move |main| {
        let tm = tag_map(main)?;
        if let Some(limit_raw) = limit {
            // Paginated path — clamp exactly as v4 (`Math.max(1, Math.min(200, limit||50))`).
            let limit = limit_raw.clamp(1, 200);
            let offset = offset.unwrap_or(0).max(0);
            let opts = memories_read::PaginateOptions {
                limit,
                offset,
                sort_by: &sort_by,
                sort_order: &sort_order,
                search: search_owned.as_deref(),
                source,
                min_importance,
            };
            let (page, total) = memories_read::find_by_character_id_paginated(main, &cid, &opts)?;
            let memories: Vec<Value> = page.into_iter().map(|m| with_tag_details(m, &tm)).collect();
            Ok(json!({
                "memories": memories,
                "count": memories.len(),
                "totalCount": total,
            }))
        } else {
            // Legacy path — in-memory filter + sort over the full set.
            let mut memories = memories_read::find_by_character_id(main, &cid)?;
            if let Some(q) = &search_owned {
                let ql = q.to_lowercase();
                memories.retain(|m| memory_matches_search(m, &ql));
            }
            if let Some(min) = min_importance {
                memories
                    .retain(|m| m.get("importance").and_then(Value::as_f64).unwrap_or(0.0) >= min);
            }
            if let Some(src) = source {
                memories.retain(|m| m.get("source").and_then(Value::as_str) == Some(src));
            }
            sort_memories(&mut memories, &sort_by, &sort_order);
            let memories: Vec<Value> = memories
                .into_iter()
                .map(|m| with_tag_details(m, &tm))
                .collect();
            let count = memories.len();
            Ok(json!({
                "memories": memories,
                "count": count,
                "totalCount": count,
            }))
        }
    });
    match result {
        Ok(body) => Response::Memory(body),
        Err(e) => internal(e),
    }
}

/// v4's legacy in-memory search filter: lowercase `includes` on content, summary,
/// or any keyword.
fn memory_matches_search(m: &Value, ql: &str) -> bool {
    let hit = |v: Option<&str>| v.map(|s| s.to_lowercase().contains(ql)).unwrap_or(false);
    if hit(m.get("content").and_then(Value::as_str))
        || hit(m.get("summary").and_then(Value::as_str))
    {
        return true;
    }
    m.get("keywords")
        .and_then(Value::as_array)
        .map(|ks| {
            ks.iter()
                .filter_map(Value::as_str)
                .any(|k| k.to_lowercase().contains(ql))
        })
        .unwrap_or(false)
}

/// v4's legacy sort comparator: importance on RAW importance; createdAt/updatedAt
/// on `Date.getTime()` (ISO strings sort lexically identically); `desc` negates.
fn sort_memories(memories: &mut [Value], sort_by: &str, sort_order: &str) {
    let desc = sort_order != "asc";
    memories.sort_by(|a, b| {
        let ord = match sort_by {
            "importance" => {
                let ia = a.get("importance").and_then(Value::as_f64).unwrap_or(0.0);
                let ib = b.get("importance").and_then(Value::as_f64).unwrap_or(0.0);
                ia.partial_cmp(&ib).unwrap_or(std::cmp::Ordering::Equal)
            }
            "updatedAt" => cmp_field(a, b, "updatedAt"),
            _ => cmp_field(a, b, "createdAt"),
        };
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn cmp_field(a: &Value, b: &Value, field: &str) -> std::cmp::Ordering {
    let sa = a.get(field).and_then(Value::as_str).unwrap_or("");
    let sb = b.get(field).and_then(Value::as_str).unwrap_or("");
    sa.cmp(sb)
}

// ===========================================================================
// Count by chat (v4 GET /api/v1/memories?chatId=)
// ===========================================================================

/// v4 `countMemoriesByChat`. `{chatId, memoryCount}` (no tagDetails).
pub fn memory_count_by_chat(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let result: Result<Option<i64>, DbError> = db.read_main(move |main| {
        if chats_read::find_by_id(main, &cid)?.is_none() {
            return Ok(None);
        }
        Ok(Some(memories_read::count_by_chat_id(main, &cid)?))
    });
    match result {
        Ok(Some(count)) => Response::Memory(json!({ "chatId": chat_id, "memoryCount": count })),
        Ok(None) => not_found("Chat"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Get by message (v4 GET /api/v1/memories?messageId=)
// ===========================================================================

/// v4 `getMemoriesByMessage`. Walks the user's chats to find the message, expands
/// the swipe group via `swipeGroupId`, and returns the TRIMMED shape
/// `{memoryCount, isSwipeGroup, swipeCount, memories: [{id, summary, characterId,
/// importance}]}`.
pub fn memory_by_message(db: &Db, user_id: &str, message_id: &str) -> Response {
    let uid = user_id.to_string();
    let mid = message_id.to_string();
    let result: Result<Option<Value>, DbError> = db.read_main(move |main| {
        let chats = chats_read::find_by_user_id(main, &uid)?;
        // Find the message + its full message list.
        let mut found: Option<(Value, Vec<Value>)> = None;
        for chat in &chats {
            let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or("");
            let messages = crate::db::chats_messages_read::get_messages(main, chat_id)?;
            if let Some(msg) = messages.iter().find(|m| {
                m.get("type").and_then(Value::as_str) == Some("message")
                    && m.get("id").and_then(Value::as_str) == Some(&mid)
            }) {
                found = Some((msg.clone(), messages.clone()));
                break;
            }
        }
        let Some((message, all_messages)) = found else {
            return Ok(None);
        };

        // Swipe-group expansion.
        let swipe_group_id = message.get("swipeGroupId").and_then(Value::as_str);
        let message_ids: Vec<String> = if let Some(sg) = swipe_group_id {
            all_messages
                .iter()
                .filter(|m| {
                    m.get("type").and_then(Value::as_str) == Some("message")
                        && m.get("swipeGroupId").and_then(Value::as_str) == Some(sg)
                })
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        } else {
            vec![mid.clone()]
        };

        let memory_count = memories_read::count_by_source_message_ids(main, &message_ids)?;
        let mut memories: Vec<Value> = Vec::new();
        if memory_count > 0 {
            for id in &message_ids {
                for m in memories_read::find_by_source_message_id(main, id)? {
                    memories.push(json!({
                        "id": m.get("id"),
                        "summary": m.get("summary"),
                        "characterId": m.get("characterId"),
                        "importance": m.get("importance"),
                    }));
                }
            }
        }

        Ok(Some(json!({
            "memoryCount": memory_count,
            "isSwipeGroup": swipe_group_id.is_some(),
            "swipeCount": message_ids.len(),
            "memories": memories,
        })))
    });
    match result {
        Ok(Some(body)) => Response::Memory(body),
        Ok(None) => not_found("Message"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Get one (v4 GET /api/v1/memories/[id])
// ===========================================================================

/// v4 item GET: ownership via character, `tagDetails`, a fire-and-forget access-time
/// bump, `{memory}`. The bump is applied synchronously here (the v5 single writer
/// has no fire-and-forget lane at the read boundary; the differential dumps the
/// row's `lastAccessedAt` structurally where it matters).
pub async fn memory_get(db: &Db, memory_id: &str) -> Response {
    let mid = memory_id.to_string();
    let loaded: Result<Option<(Value, String)>, DbError> = db.read_main(move |main| {
        let Some(memory) = memories_read::find_by_id(main, &mid)? else {
            return Ok(None);
        };
        let character_id = memory
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if characters_read::find_by_id_raw(main, &character_id)?.is_none() {
            return Ok(None); // 404 rather than 403 — do not leak existence.
        }
        let tm = tag_map(main)?;
        Ok(Some((with_tag_details(memory, &tm), character_id)))
    });
    let (memory, character_id) = match loaded {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Memory"),
        Err(e) => return internal(e),
    };
    // Access-time bump (v4 fire-and-forget; failures are swallowed there too).
    let mid = memory_id.to_string();
    let cid = character_id.clone();
    let _ = db
        .write(move |w| {
            let _ = w.main().memories().update_access_time(&cid, &mid);
            Ok::<(), DbError>(())
        })
        .await;
    Response::Memory(json!({ "memory": memory }))
}

// ===========================================================================
// Character memory counts (v4 GET ?action=character-memory-counts)
// ===========================================================================

/// v4 `handleCharacterMemoryCounts`: `{success, characters: [{id, name,
/// memoryCount}]}`, sorted by memoryCount DESCENDING.
pub fn memory_character_counts(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let result: Result<Vec<Value>, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let chars = characters_read::find_by_user_id(main, mount, &uid)?;
            let mut out: Vec<(String, Value)> = Vec::with_capacity(chars.len());
            for c in chars {
                let id = c
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let name = c.get("name").cloned().unwrap_or(Value::Null);
                let count = memories_read::count_by_character_id(main, &id)?;
                out.push((
                    id.clone(),
                    json!({ "id": id, "name": name, "memoryCount": count }),
                ));
            }
            // Sort by memoryCount desc; v4's Array.sort is stable, so ties keep
            // the findByUserId order.
            out.sort_by(|a, b| {
                let ca = a.1.get("memoryCount").and_then(Value::as_i64).unwrap_or(0);
                let cb = b.1.get("memoryCount").and_then(Value::as_i64).unwrap_or(0);
                cb.cmp(&ca)
            });
            Ok(out.into_iter().map(|(_, v)| v).collect())
        })
    });
    match result {
        Ok(characters) => Response::Memory(json!({ "success": true, "characters": characters })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Create (v4 POST /api/v1/memories, no action)
// ===========================================================================

/// v4 `createMemorySchema` — the create body (Zod prefaults inlined as serde
/// defaults). Nullable-optional uuids collapse null/absent → `None`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBag {
    character_id: String,
    content: String,
    summary: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_importance")]
    importance: f64,
    #[serde(default)]
    about_character_id: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default = "default_source")]
    source: String,
    #[serde(default)]
    source_message_id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    skip_gate: Option<bool>,
}
fn default_importance() -> f64 {
    0.5
}
fn default_source() -> String {
    "MANUAL".to_string()
}

/// v4 `handleCreateMemory` → `createMemoryWithEmbedding` (the gate). 201 `{memory}`.
/// SKIP_EMBEDDING_FAILED → serverError (no row). `skipGate` is accepted but has
/// NO direct-create path (v5's gate port always runs the gate — a documented
/// P4.6s divergence; distinctive content INSERTs either way).
pub async fn memory_create<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    user_id: &str,
    bag: Value,
) -> Response {
    let bag: CreateBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid memory body: {e}")),
    };
    match character_exists(db, &bag.character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let data = CreateMemoryOptions {
        character_id: bag.character_id,
        content: bag.content,
        summary: bag.summary,
        keywords: bag.keywords,
        tags: bag.tags,
        importance: Some(bag.importance),
        about_character_id: bag.about_character_id,
        chat_id: bag.chat_id,
        project_id: None,
        source: Some(bag.source),
        source_message_id: bag.source_message_id,
        source_message_timestamp: None,
        witnessed_context: None,
        // Episodic spine: v4's route createMemorySchema does NOT accept the
        // episodic fields (8bf3cb5f), so a route create lands with the
        // defaults — occurredAt/narrativeTime NULL, entities [], kind
        // 'semantic'. Extraction-path stamping is the processor's job.
        occurred_at: None,
        narrative_time: None,
        entities: Vec::new(),
        kind: None,
    };
    let opts = MemoryServiceOptions {
        user_id: user_id.to_string(),
        embedding_profile_id: None,
    };
    let outcome = match create_memory_with_gate(db, provider, &data, &opts).await {
        Ok(o) => o,
        Err(e) => return internal(e),
    };
    // SKIP_EMBEDDING_FAILED → no row written → serverError (v4 parity).
    if matches!(outcome.action, GateAction::SkipEmbeddingFailed) {
        return Response::error(
            ErrorKind::Internal,
            "Failed to generate embedding for memory — no row was created. Check the configured embedding profile.",
        );
    }
    let Some(memory_id) = outcome.memory_id else {
        return internal("memory gate returned no memory id");
    };
    // v4 echoes the gate outcome's memory (the created/reinforced row).
    let reread: Result<Option<Value>, DbError> =
        db.read_main(move |main| memories_read::find_by_id(main, &memory_id));
    match reread {
        Ok(Some(memory)) => Response::Memory(json!({ "memory": memory })),
        Ok(None) => internal("created memory not found on re-read"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Update (v4 PUT /api/v1/memories/[id])
// ===========================================================================

/// v4 `updateMemorySchema` — the item PUT body (all optional). `aboutCharacterId`
/// / `chatId` are nullable (present-null → SQL NULL).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBag {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    importance: Option<f64>,
    // `Option<Option<_>>`: absent → None (untouched); present (incl. null) → Some.
    #[serde(default, deserialize_with = "double_option")]
    about_character_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    chat_id: Option<Option<String>>,
    #[serde(default)]
    related_memory_ids: Option<Vec<String>>,
}

/// Distinguish `absent` from an explicit `null` for a nullable field.
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(de)?))
}

/// v4 item PUT: ownership via character, `updateForCharacter`, `{memory}`. Does
/// NOT re-embed (only v4's fire-and-forget `scheduleRefit` — a host-timing seam).
pub async fn memory_update(db: &Db, memory_id: &str, bag: Value) -> Response {
    let bag: UpdateBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid memory update body: {e}")),
    };
    // Ownership: find existing + its character.
    let mid = memory_id.to_string();
    let character_id: Result<Option<String>, DbError> = db.read_main(move |main| {
        let Some(existing) = memories_read::find_by_id(main, &mid)? else {
            return Ok(None);
        };
        let cid = existing
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if characters_read::find_by_id_raw(main, &cid)?.is_none() {
            return Ok(None);
        }
        Ok(Some(cid))
    });
    let character_id = match character_id {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Memory"),
        Err(e) => return internal(e),
    };

    let patch = MemUpdate {
        content: bag.content,
        summary: bag.summary,
        keywords: bag.keywords,
        tags: bag.tags,
        related_memory_ids: bag.related_memory_ids,
        importance: bag.importance,
        about_character_id: bag.about_character_id,
        chat_id: bag.chat_id,
        ..Default::default()
    };
    let mid = memory_id.to_string();
    let cid = character_id.clone();
    let updated = db
        .write(move |w| w.main().memories().update_for_character(&cid, &mid, &patch))
        .await;
    match updated {
        Ok(true) => {}
        Ok(false) => return not_found("Memory"),
        Err(e) => return internal(e),
    }
    let mid = memory_id.to_string();
    let reread: Result<Option<Value>, DbError> =
        db.read_main(move |main| memories_read::find_by_id(main, &mid));
    match reread {
        Ok(Some(memory)) => Response::Memory(json!({ "memory": memory })),
        Ok(None) => not_found("Memory"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Delete (v4 DELETE /api/v1/memories/[id])
// ===========================================================================

/// v4 item DELETE: ownership via character, `deleteMemoryWithUnlink` (scrubs
/// neighbours' `relatedMemoryIds`), `{success: true}`. v4's fire-and-forget
/// `handleEntityDeletion` / `scheduleRefit` are host-timing seams (not ported).
pub async fn memory_delete(db: &Db, memory_id: &str) -> Response {
    let mid = memory_id.to_string();
    let exists: Result<bool, DbError> = db.read_main(move |main| {
        let Some(existing) = memories_read::find_by_id(main, &mid)? else {
            return Ok(false);
        };
        let cid = existing
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or("");
        Ok(characters_read::find_by_id_raw(main, cid)?.is_some())
    });
    match exists {
        Ok(true) => {}
        Ok(false) => return not_found("Memory"),
        Err(e) => return internal(e),
    }
    let mid = memory_id.to_string();
    let deleted = db
        .write(move |w| w.main().memories().delete_with_unlink(&mid))
        .await;
    match deleted {
        Ok(_) => Response::Memory(json!({ "success": true })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Delete by chat (v4 DELETE /api/v1/memories?chatId=)
// ===========================================================================

/// v4 `handleDeleteByChatId`: chat ownership, `deleteMemoriesByChatIdWithVectors`,
/// `{success, chatId, deletedCount}`.
pub async fn memory_delete_by_chat(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let owned: Result<bool, DbError> =
        db.read_main(move |main| Ok(chats_read::find_by_id(main, &cid)?.is_some()));
    match owned {
        Ok(true) => {}
        Ok(false) => return not_found("Chat"),
        Err(e) => return internal(e),
    }
    match delete_memories_by_chat_id_with_vectors(db, chat_id).await {
        Ok(res) => Response::Memory(
            json!({ "success": true, "chatId": chat_id, "deletedCount": res.deleted }),
        ),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Search (v4 POST /api/v1/memories?action=search)
// ===========================================================================

/// v4 `searchMemorySchema` — the search body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchBag {
    character_id: String,
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    min_importance: Option<f64>,
    #[serde(default)]
    min_score: Option<f64>,
    #[serde(default)]
    source: Option<String>,
}
fn default_search_limit() -> usize {
    20
}

/// v4 `handleSearch`: `searchMemoriesSemantic` → each result carries the memory +
/// `score`, `usedEmbedding`, `tagDetails`; top-level `{count, query, usedEmbedding}`.
/// `now_ms` is threaded so the recency-weighted ranking is pinnable in the
/// differential (v4's route uses `Date.now()` — the oracle mocks it).
pub async fn memory_search<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    user_id: &str,
    now_ms: f64,
    bag: Value,
) -> Response {
    let bag: SearchBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid search body: {e}")),
    };
    match character_exists(db, &bag.character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let options = SemanticSearchOptions {
        user_id: user_id.to_string(),
        embedding_profile_id: None,
        limit: Some(bag.limit),
        min_score: bag.min_score,
        min_importance: bag.min_importance,
        source: bag.source,
        about_character_id: None,
        apply_literal_phrase_boost: false,
        now_ms,
        ..Default::default()
    };
    let results =
        match search_memories_semantic(db, provider, &bag.character_id, &bag.query, &options, None)
            .await
        {
            Ok(r) => r,
            Err(e) => return internal(e),
        };
    let used_embedding = results.first().map(|r| r.used_embedding).unwrap_or(false);
    let query = bag.query.clone();
    let cid = bag.character_id.clone();
    let body: Result<Value, DbError> = db.read_main(move |main| {
        let tm = tag_map(main)?;
        let memories: Vec<Value> = results
            .into_iter()
            .map(|r| {
                let mut m = with_tag_details(r.memory, &tm);
                if let Some(obj) = m.as_object_mut() {
                    obj.insert("score".into(), json!(r.score));
                    obj.insert("usedEmbedding".into(), json!(r.used_embedding));
                }
                m
            })
            .collect();
        let _ = cid;
        Ok(json!({
            "memories": memories.clone(),
            "count": memories.len(),
            "query": query,
            "usedEmbedding": used_embedding,
        }))
    });
    match body {
        Ok(b) => Response::Memory(b),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Housekeeping
// ===========================================================================

/// One `HousekeepingResult` detail → v4's `{memoryId, action, reason, summary?}`
/// (summary omitted when null).
fn hk_details(r: &HousekeepingResult) -> Vec<Value> {
    r.details
        .iter()
        .map(|d| {
            let mut o = serde_json::Map::new();
            o.insert("memoryId".into(), json!(d.memory_id));
            o.insert("action".into(), json!(d.action));
            o.insert("reason".into(), json!(d.reason));
            if let Some(s) = &d.summary {
                o.insert("summary".into(), json!(s));
            }
            Value::Object(o)
        })
        .collect()
}

/// v4 GET `?action=housekeep` — `handleHousekeepPreview`. Query params
/// `maxMemories`/`maxAgeMonths`/`minImportance`/`mergeSimilar` (manual range
/// validation, NOT Zod). Envelope `{success, preview: {wouldDelete, wouldMerge,
/// wouldKeep, totalBefore, totalAfter, details}}`.
pub async fn memory_housekeep_preview(
    db: &Db,
    character_id: &str,
    max_memories: Option<i64>,
    max_age_months: Option<i64>,
    min_importance: Option<f64>,
    merge_similar: Option<bool>,
) -> Response {
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let mut opts = HousekeepingOptions::default();
    // v4's manual per-param range checks → badRequest.
    if let Some(n) = max_memories {
        if !(1..=100000).contains(&n) {
            return bad_request("maxMemories must be an integer between 1 and 100000");
        }
        opts.max_memories = Some(n as usize);
    }
    if let Some(n) = max_age_months {
        if !(1..=1200).contains(&n) {
            return bad_request("maxAgeMonths must be an integer between 1 and 1200");
        }
        opts.max_age_months = Some(n as f64);
    }
    if let Some(n) = min_importance {
        if !(0.0..=1.0).contains(&n) {
            return bad_request("minImportance must be a number between 0 and 1");
        }
        opts.min_importance = Some(n);
    }
    if let Some(b) = merge_similar {
        opts.merge_similar = Some(b);
    }
    match get_housekeeping_preview(db, character_id, &opts).await {
        Ok(r) => Response::Memory(json!({
            "success": true,
            "preview": {
                "wouldDelete": r.deleted,
                "wouldMerge": r.merged,
                "wouldKeep": r.kept,
                "totalBefore": r.total_before,
                "totalAfter": r.total_after,
                "details": hk_details(&r),
            },
        })),
        Err(e) => internal(e),
    }
}

/// v4 `housekeepingOptionsSchema` (POST `?action=housekeep`).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct HousekeepBag {
    character_id: String,
    #[serde(default)]
    max_memories: Option<usize>,
    #[serde(default)]
    max_age_months: Option<f64>,
    #[serde(default)]
    max_inactive_months: Option<f64>,
    #[serde(default)]
    min_importance: Option<f64>,
    #[serde(default)]
    merge_similar: Option<bool>,
    #[serde(default)]
    merge_threshold: Option<f64>,
    #[serde(default)]
    dry_run: Option<bool>,
}

/// v4 POST `?action=housekeep` — `handleHousekeep`. dryRun → preview else run.
/// `{success, dryRun, result: {…, details?}}` — `details` ONLY on dryRun.
pub async fn memory_housekeep(db: &Db, bag: Value) -> Response {
    let bag: HousekeepBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid housekeeping options: {e}")),
    };
    match character_exists(db, &bag.character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let dry_run = bag.dry_run.unwrap_or(false);
    let opts = HousekeepingOptions {
        max_memories: bag.max_memories,
        max_age_months: bag.max_age_months,
        max_inactive_months: bag.max_inactive_months,
        min_importance: bag.min_importance,
        merge_similar: bag.merge_similar,
        merge_threshold: bag.merge_threshold,
        dry_run: bag.dry_run,
    };
    let result = if dry_run {
        get_housekeeping_preview(db, &bag.character_id, &opts).await
    } else {
        run_housekeeping(db, &bag.character_id, &opts).await
    };
    match result {
        Ok(r) => {
            let mut result_obj = serde_json::Map::new();
            result_obj.insert("deleted".into(), json!(r.deleted));
            result_obj.insert("merged".into(), json!(r.merged));
            result_obj.insert("kept".into(), json!(r.kept));
            result_obj.insert("totalBefore".into(), json!(r.total_before));
            result_obj.insert("totalAfter".into(), json!(r.total_after));
            result_obj.insert("deletedIds".into(), json!(r.deleted_ids));
            result_obj.insert("mergedIds".into(), json!(r.merged_ids));
            // v4: `details: options.dryRun ? result.details : undefined` — undefined
            // is dropped from the JSON, so the key is present only on dryRun.
            if dry_run {
                result_obj.insert("details".into(), json!(hk_details(&r)));
            }
            Response::Memory(json!({
                "success": true,
                "dryRun": dry_run,
                "result": Value::Object(result_obj),
            }))
        }
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=housekeep-sweep` — enqueue `MEMORY_HOUSEKEEPING` across every
/// character. `{success, jobId}`.
pub async fn memory_housekeep_sweep(db: &Db, user_id: &str) -> Response {
    match enqueue_memory_housekeeping(db, user_id, json!({ "reason": "manual" })).await {
        Ok(job_id) => Response::Memory(json!({ "success": true, "jobId": job_id })),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to enqueue housekeeping sweep"),
    }
}

// ===========================================================================
// Config: auto-housekeeping (per-user, chat_settings)
// ===========================================================================

/// v4's `autoHousekeepingSettings` default injection.
fn housekeeping_config_default() -> Value {
    json!({
        "enabled": false,
        "perCharacterCap": 2000,
        "perCharacterCapOverrides": {},
        "autoMergeSimilarThreshold": 0.90,
        "mergeSimilar": false,
    })
}

/// v4 GET `?action=housekeeping-config` — `{success, settings}` (default-injected).
pub fn memory_housekeeping_config_get(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let settings: Result<Value, DbError> = db.read_main(move |main| {
        Ok(
            chat_settings::find_auto_housekeeping_settings_by_user_id(main, &uid)?
                .unwrap_or_else(housekeeping_config_default),
        )
    });
    match settings {
        Ok(s) => Response::Memory(json!({ "success": true, "settings": s })),
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=housekeeping-config` — merge-patch each field `?? current`,
/// store via `updateForUser`, `{success, settings: merged}`.
pub async fn memory_housekeeping_config_set(db: &Db, user_id: &str, bag: Value) -> Response {
    if !bag.is_object() {
        return bad_request("Invalid housekeeping config body");
    }
    let uid = user_id.to_string();
    let current: Result<Value, DbError> = db.read_main(move |main| {
        Ok(
            chat_settings::find_auto_housekeeping_settings_by_user_id(main, &uid)?
                .unwrap_or_else(housekeeping_config_default),
        )
    });
    let current = match current {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    let pick = |key: &str| {
        bag.get(key)
            .cloned()
            .unwrap_or_else(|| current[key].clone())
    };
    let merged = json!({
        "enabled": pick("enabled"),
        "perCharacterCap": pick("perCharacterCap"),
        "perCharacterCapOverrides": pick("perCharacterCapOverrides"),
        "autoMergeSimilarThreshold": pick("autoMergeSimilarThreshold"),
        "mergeSimilar": pick("mergeSimilar"),
    });
    let uid = user_id.to_string();
    let merged_str = merged.to_string();
    let now = crate::clock::now_iso();
    let wrote = db
        .write(move |w| {
            chat_settings::update_for_user(
                w.main().connection(),
                &uid,
                &[(
                    "autoHousekeepingSettings",
                    chat_settings::SettingsColVal::Text(merged_str),
                )],
                &now,
            )
        })
        .await;
    match wrote {
        Ok(_) => Response::Memory(json!({ "success": true, "settings": merged })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Config: recall / extraction-limits / extraction-concurrency (instance-wide)
// ===========================================================================

/// v4 GET `?action=recall-config` — `{success, settings}`, where `settings` is
/// the whole parsed `MemoryRecallSettings` bag in schema-declaration order.
pub fn memory_recall_config_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_memory_recall_settings) {
        Ok(settings) => Response::Memory(json!({
            "success": true,
            "settings": settings.to_json(),
        })),
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=recall-config` — merge-patch, store, `{success, settings}`.
/// Each field is independently patchable (`parsed.data.X ?? currentSettings.X`);
/// an absent field keeps what is stored.
pub async fn memory_recall_config_set(db: &Db, bag: Value) -> Response {
    if !bag.is_object() {
        return bad_request("Invalid recall config body");
    }
    let current = match db.read_main(instance_settings::get_memory_recall_settings) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    let merged = instance_settings::MemoryRecallSettings {
        scope_policy: bag
            .get("scopePolicy")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(current.scope_policy),
        expand_related: bag
            .get("expandRelated")
            .and_then(Value::as_bool)
            .unwrap_or(current.expand_related),
        per_turn_conversation_summaries: bag
            .get("perTurnConversationSummaries")
            .and_then(Value::as_bool)
            .unwrap_or(current.per_turn_conversation_summaries),
    };
    let to_write = merged.clone();
    let wrote = db
        .write(move |w| {
            instance_settings::set_memory_recall_settings(w.main().connection(), &to_write)
        })
        .await;
    match wrote {
        Ok(_) => Response::Memory(json!({
            "success": true,
            "settings": merged.to_json(),
        })),
        Err(e) => internal(e),
    }
}

/// v4 GET `?action=extraction-limits-config` — `{success, settings}`.
pub fn memory_extraction_limits_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_memory_extraction_limits) {
        Ok(s) => Response::Memory(json!({ "success": true, "settings": s })),
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=extraction-limits-config` — merge-patch, store, `{success, settings}`.
pub async fn memory_extraction_limits_set(db: &Db, bag: Value) -> Response {
    if !bag.is_object() {
        return bad_request("Invalid extraction limits body");
    }
    let current = match db.read_main(instance_settings::get_memory_extraction_limits) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    let pick = |key: &str| {
        bag.get(key)
            .cloned()
            .unwrap_or_else(|| current[key].clone())
    };
    let merged = json!({
        "enabled": pick("enabled"),
        "maxPerHour": pick("maxPerHour"),
        "softStartFraction": pick("softStartFraction"),
        "softFloor": pick("softFloor"),
    });
    let m2 = merged.clone();
    let wrote = db
        .write(move |w| instance_settings::set_memory_extraction_limits(w.main().connection(), &m2))
        .await;
    match wrote {
        Ok(_) => Response::Memory(json!({ "success": true, "settings": merged })),
        Err(e) => internal(e),
    }
}

/// v4 GET `?action=extraction-concurrency` — `{success, concurrency}`.
pub fn memory_extraction_concurrency_get(db: &Db) -> Response {
    match db.read_main(instance_settings::get_memory_extraction_concurrency) {
        Ok(c) => Response::Memory(json!({ "success": true, "concurrency": c })),
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=extraction-concurrency` — `{concurrency}` (int 1–32). Stores the
/// setting AND pushes a runtime override into the processor (the runtime half is a
/// host seam — injected, no-op default). `{success, concurrency}`.
pub async fn memory_extraction_concurrency_set(db: &Db, bag: Value) -> Response {
    let Some(concurrency) = bag.get("concurrency").and_then(Value::as_i64) else {
        return bad_request("concurrency must be an integer between 1 and 32");
    };
    if !(1..=32).contains(&concurrency) {
        return bad_request("concurrency must be an integer between 1 and 32");
    }
    let wrote = db
        .write(move |w| {
            instance_settings::set_memory_extraction_concurrency(w.main().connection(), concurrency)
        })
        .await;
    // The runtime processor-override push is a host seam (no-op here).
    match wrote {
        Ok(_) => Response::Memory(json!({ "success": true, "concurrency": concurrency })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Regenerate + backfill status (background_jobs reads / job enqueues)
// ===========================================================================

/// Load the user's in-flight (PENDING + PROCESSING) background jobs.
fn in_flight_jobs(
    db: &Db,
    user_id: &str,
) -> Result<Vec<crate::db::background_jobs::BackgroundJob>, DbError> {
    let uid = user_id.to_string();
    db.read_main(move |conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    })
}

/// v4 GET `?action=backfill-embeddings` — `handleBackfillProgress`: `{success,
/// progress: {remaining, inFlight}}`. `inFlight` counts PENDING/PROCESSING
/// `EMBEDDING_GENERATE` jobs whose `payload.entityType === 'MEMORY'`.
pub fn memory_backfill_progress(db: &Db, user_id: &str) -> Response {
    let remaining = match db.read_main(|conn| memories_read::count_without_embedding(conn, None)) {
        Ok(n) => n,
        Err(e) => return internal(e),
    };
    let jobs = match in_flight_jobs(db, user_id) {
        Ok(j) => j,
        Err(e) => return internal(e),
    };
    let in_flight = jobs
        .iter()
        .filter(|j| {
            j.job_type == "EMBEDDING_GENERATE"
                && serde_json::from_str::<Value>(&j.payload)
                    .ok()
                    .and_then(|p| {
                        p.get("entityType")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    == Some("MEMORY".to_string())
        })
        .count();
    Response::Memory(json!({
        "success": true,
        "progress": { "remaining": remaining, "inFlight": in_flight },
    }))
}

/// v4 GET `?action=regenerate-all` — `handleRegenerateAllStatus`: `{success,
/// inFlightFanOut, inFlightWipes, inFlightExtractions, inFlight}`.
pub fn memory_regenerate_all_status(db: &Db, user_id: &str) -> Response {
    let jobs = match in_flight_jobs(db, user_id) {
        Ok(j) => j,
        Err(e) => return internal(e),
    };
    let count = |t: &str| jobs.iter().filter(|j| j.job_type == t).count();
    let fan_out = count("MEMORY_REGENERATE_ALL");
    let wipes = count("MEMORY_REGENERATE_CHAT");
    let extractions = count("MEMORY_EXTRACTION");
    Response::Memory(json!({
        "success": true,
        "inFlightFanOut": fan_out,
        "inFlightWipes": wipes,
        "inFlightExtractions": extractions,
        "inFlight": fan_out + wipes + extractions,
    }))
}

/// v4 POST `?action=regenerate-all` — `handleRegenerateAll`: wipe in-flight
/// regenerate/extraction jobs, resolve the standard + dangerous-compatible cheap
/// profiles, enqueue ONE fan-out (deduped on userId). `{success, jobId, isNew,
/// cleared, message}`.
pub async fn memory_regenerate_all(db: &Db, user_id: &str) -> Response {
    // 1. Wipe the previous sweep's in-flight jobs.
    let cleared = db
        .write(|w| {
            w.main().background_jobs().delete_by_types_and_statuses(
                &[
                    "MEMORY_REGENERATE_ALL".into(),
                    "MEMORY_REGENERATE_CHAT".into(),
                    "MEMORY_EXTRACTION".into(),
                    "INTER_CHARACTER_MEMORY".into(),
                ],
                &["PENDING".into(), "PROCESSING".into()],
            )
        })
        .await;
    let cleared = match cleared {
        Ok(n) => n as i64,
        Err(e) => return internal(e),
    };

    // 2. Resolve the cheap profiles up front (v4's self-contained payload).
    let uid = user_id.to_string();
    let resolved: Result<Option<(String, String)>, DbError> = db.read_main(move |conn| {
        let settings = chat_settings::find_by_user_id(conn, &uid)?;
        let profiles = crate::db::connection_profiles::find_by_user_id(conn, &uid)?;
        let cheap_default_id = settings
            .as_ref()
            .and_then(|s| s.get("cheapLLMSettings"))
            .and_then(|c| c.get("defaultCheapProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let find_id = |id: &str| {
            profiles
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(id))
        };
        let standard = cheap_default_id
            .as_deref()
            .and_then(find_id)
            .or_else(|| profiles.first())
            .and_then(|p| p.get("id").and_then(Value::as_str).map(str::to_string));
        let Some(standard) = standard else {
            return Ok(None);
        };
        let uncensored_id = settings
            .as_ref()
            .and_then(|s| s.get("dangerousContentSettings"))
            .and_then(|d| d.get("uncensoredTextProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_cheap = |p: &Value| p.get("isCheap").and_then(Value::as_bool) == Some(true);
        let is_dangerous =
            |p: &Value| p.get("isDangerousCompatible").and_then(Value::as_bool) == Some(true);
        let uncensored_cheap = uncensored_id
            .as_deref()
            .and_then(find_id)
            .filter(|p| is_cheap(p))
            .and_then(|p| p.get("id").and_then(Value::as_str).map(str::to_string));
        let any_dangerous_cheap = profiles
            .iter()
            .find(|p| is_cheap(p) && is_dangerous(p))
            .and_then(|p| p.get("id").and_then(Value::as_str).map(str::to_string));
        let dangerous = uncensored_cheap
            .or(any_dangerous_cheap)
            .unwrap_or_else(|| standard.clone());
        Ok(Some((standard, dangerous)))
    });
    let (standard_id, dangerous_id) = match resolved {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            return bad_request(
                "No connection profiles found. Add at least one provider connection before regenerating memories.",
            )
        }
        Err(e) => return internal(e),
    };

    // 3. Enqueue the fan-out (deduped on userId).
    let payload = json!({ "standardProfileId": standard_id, "dangerousProfileId": dangerous_id });
    let (job_id, is_new) =
        match crate::services::queue_service::enqueue_memory_regenerate_all(db, user_id, payload)
            .await
        {
            Ok(r) => r,
            Err(e) => return internal(e),
        };

    let cleared_suffix = if cleared > 0 {
        format!(
            " Cleared {cleared} in-flight job{} from the previous sweep.",
            if cleared == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    let message = if is_new {
        format!("Regeneration scheduled — building the chat list in the background. The Mem badge will start ticking shortly.{cleared_suffix}")
    } else {
        format!("A regeneration sweep is already in progress. The existing run will continue.{cleared_suffix}")
    };
    Response::Memory(json!({
        "success": true,
        "jobId": job_id,
        "isNew": is_new,
        "cleared": cleared,
        "message": message,
    }))
}

// ===========================================================================
// Embedding status + backfill start (tier 2)
// ===========================================================================

/// v4 GET `?action=embeddings&characterId=` — `handleEmbeddingStatus`: `{total,
/// withEmbeddings, withoutEmbeddings, percentComplete, embeddingProfileConfigured,
/// embeddingProfileName}`. (Embedding presence = the marshaled memory carries a
/// non-empty `embedding` key.)
pub fn memory_embedding_status(db: &Db, user_id: &str, character_id: &str) -> Response {
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let cid = character_id.to_string();
    let uid = user_id.to_string();
    let result: Result<Value, DbError> = db.read_main(move |main| {
        let memories = memories_read::find_by_character_id(main, &cid)?;
        let total = memories.len();
        let with = memories
            .iter()
            .filter(|m| m.get("embedding").map(|e| !e.is_null()).unwrap_or(false))
            .count();
        let without = total - with;
        let percent = if total > 0 {
            ((with as f64 / total as f64) * 100.0).round() as i64
        } else {
            100
        };
        let default = crate::db::embedding_profiles::find_default_id_name(main, &uid)?;
        Ok(json!({
            "total": total,
            "withEmbeddings": with,
            "withoutEmbeddings": without,
            "percentComplete": percent,
            "embeddingProfileConfigured": default.is_some(),
            "embeddingProfileName": default.map(|(_, n)| Value::String(n)).unwrap_or(Value::Null),
        }))
    });
    match result {
        Ok(body) => Response::Memory(body),
        Err(e) => internal(e),
    }
}

/// v4 `backfillStartSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackfillBag {
    #[serde(default)]
    character_id: Option<String>,
    #[serde(default = "default_batch_size")]
    batch_size: i64,
}
fn default_batch_size() -> i64 {
    500
}

/// v4 POST `?action=backfill-embeddings` — `handleBackfillStart`: batch-enqueue
/// `EMBEDDING_GENERATE` for memories missing an embedding. `{success, enqueued,
/// remaining, message}`.
pub async fn memory_backfill_start(db: &Db, user_id: &str, bag: Value) -> Response {
    let bag: BackfillBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid backfill body: {e}")),
    };
    if let Some(cid) = &bag.character_id {
        match character_exists(db, cid) {
            Ok(true) => {}
            Ok(false) => return not_found("Character"),
            Err(e) => return internal(e),
        }
    }
    let char_filter = bag.character_id.clone();
    let batch = bag.batch_size;
    let missing: Result<Vec<Value>, DbError> = db.read_main(move |conn| {
        memories_read::find_ids_without_embedding(conn, char_filter.as_deref(), Some(batch))
    });
    let missing = match missing {
        Ok(m) => m,
        Err(e) => return internal(e),
    };
    if missing.is_empty() {
        return Response::Memory(json!({
            "success": true,
            "enqueued": 0,
            "remaining": 0,
            "message": "No memories missing embeddings — nothing to backfill.",
        }));
    }
    // Resolve the user's default embedding profile.
    let uid = user_id.to_string();
    let default = db.read_main(move |conn| crate::db::embedding_profiles::find_default(conn, &uid));
    let profile = match default {
        Ok(Some(p)) => p,
        Ok(None) => {
            return bad_request(
                "No default embedding profile is configured. Set one in the Commonplace Book tab before running the backfill.",
            )
        }
        Err(e) => return internal(e),
    };
    // Enqueue one EMBEDDING_GENERATE per missing memory (v4 warns + continues on
    // an individual enqueue failure).
    let mut enqueued = 0i64;
    for m in &missing {
        let (Some(id), Some(char_id)) = (
            m.get("id").and_then(Value::as_str),
            m.get("characterId").and_then(Value::as_str),
        ) else {
            continue;
        };
        let payload = json!({
            "entityType": "MEMORY",
            "entityId": id,
            "characterId": char_id,
            "profileId": profile.id,
        });
        if crate::services::queue_service::enqueue_embedding_generate(db, user_id, payload)
            .await
            .is_ok()
        {
            enqueued += 1;
        }
    }
    let char_filter = bag.character_id.clone();
    let remaining = db
        .read_main(move |conn| memories_read::count_without_embedding(conn, char_filter.as_deref()))
        .unwrap_or(0);
    let message = if remaining > 0 {
        format!(
            "Enqueued {enqueued} embedding jobs. {remaining} memories still missing embeddings — run again to continue."
        )
    } else {
        format!("Enqueued {enqueued} embedding jobs. All memories are now accounted for.")
    };
    Response::Memory(json!({
        "success": true,
        "enqueued": enqueued,
        "remaining": remaining,
        "message": message,
    }))
}

// ============================================================================
// P4.6BL tier 2 — the two un-refused repair arms.
// ============================================================================

/// v4 POST `?action=embeddings` → `handleGenerateEmbeddings`
/// (`app/api/v1/memories/route.ts:963`): Zod parse (uuid + batchSize 1–50,
/// prefault 10), character ownership, default-profile gate, the all-embedded
/// early return, else the synchronous `generateMissingEmbeddings` sweep.
/// The Zod `details` array is the standing repo-wide deferral — the 400 body
/// carries the middleware's `Validation error` message alone.
pub async fn memory_generate_embeddings<P: crate::model::embedding::EmbeddingProvider>(
    db: &Db,
    provider: &P,
    user_id: &str,
    character_id: &str,
    batch_size: Option<i64>,
) -> Response {
    if !super::chat_outfits::is_zod_uuid(character_id) {
        return bad_request("Validation error");
    }
    // `z.number().min(1).max(50).prefault(10)`.
    let batch = batch_size.unwrap_or(10);
    if !(1..=50).contains(&batch) {
        return bad_request("Validation error");
    }
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    // v4: `repos.embeddingProfiles.findDefault(user.id)` — absent → 400.
    let uid = user_id.to_string();
    match db.read_main(move |conn| crate::db::embedding_profiles::find_default(conn, &uid)) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return bad_request(
                "No embedding profile configured. Please set up an embedding profile in settings.",
            )
        }
        Err(e) => return internal(e),
    }
    // The stats read (v4 re-reads inside the service too — both reads kept).
    let cid = character_id.to_string();
    let memories = match db.read_main(move |conn| memories_read::find_by_character_id(conn, &cid)) {
        Ok(m) => m,
        Err(e) => return internal(e),
    };
    let missing = memories
        .iter()
        .filter(|m| {
            // v4 `!m.embedding || m.embedding.length === 0`.
            match m.get("embedding") {
                Some(Value::Object(o)) => o.is_empty(),
                _ => true,
            }
        })
        .count();
    if missing == 0 {
        return Response::Memory(json!({
            "message": "All memories already have embeddings",
            "processed": 0,
            "failed": 0,
            "skipped": 0,
            "total": memories.len(),
        }));
    }
    let result = match crate::services::memory_service::generate_missing_embeddings(
        db,
        provider,
        user_id,
        character_id,
        None,
        batch as usize,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    Response::Memory(json!({
        "message": "Embedding generation complete",
        "processed": result.processed,
        "failed": result.failed,
        "skipped": result.skipped,
        "total": memories.len(),
        "remaining": missing - result.processed - result.failed,
    }))
}

/// v4 PUT `?action=embeddings` → `handleRebuildIndex`
/// (`app/api/v1/memories/route.ts:1047`): Zod parse (uuid + `confirm` literal
/// true), character ownership, then the synchronous `rebuildVectorIndex`.
pub async fn memory_rebuild_index(db: &Db, character_id: &str, confirm: bool) -> Response {
    // `z.literal(true)` + `z.uuid` — either failure is the middleware 400.
    if !confirm || !super::chat_outfits::is_zod_uuid(character_id) {
        return bad_request("Validation error");
    }
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    let result = match crate::services::memory_service::rebuild_vector_index(db, character_id).await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    Response::Memory(json!({
        "message": "Vector index rebuilt successfully",
        "indexed": result.indexed,
        "failed": result.failed,
    }))
}

// === P4.6BM: the per-chat `?action=queue-memories` action ===

/// v4 `handleQueueMemories` (`app/api/v1/chats/[id]/actions/memories.ts:85-166`)
/// — queue one per-turn `MEMORY_EXTRACTION` job for every turn in a chat.
///
/// The two branches are v4's, and they are NOT symmetric:
///
///   - **autonomous** chats have no USER role in history, so one job is queued
///     per non-system, non-silent ASSISTANT message that HAS a `participantId`,
///     with that message as the `extractionAnchorMessageId` (the anchor keeps
///     each job's dedupe key distinct, and each job's transcript walks from the
///     start of history through it).
///   - **every other** chat type queues one job per non-system USER message,
///     which OPENS the turn the handler then walks forward from.
///
/// Error arms (both 400, both byte-matched): no cheap LLM configured, and an
/// empty batch with a branch-specific message.
///
/// The legacy `characterId` / `characterName` / `messagePairs` request fields are
/// still accepted by v4's Zod schema and ignored by the handler (per-turn jobs
/// cover the whole turn), so v5's `ChatQueueMemories { chat_id }` variant is the
/// whole live surface.
pub async fn chat_queue_memories(db: &Db, user_id: &str, chat_id: &str) -> Response {
    // The chat itself (v4's route wrapper resolves it before dispatching).
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    // v4 `resolveCheapLLMProfileId` — the narrow two-branch resolver, shared
    // with any future caller (`cheap_llm::resolve_cheap_llm_profile_id`).
    let uid = user_id.to_string();
    let resolved: Result<Option<String>, DbError> = db.read_main(move |conn| {
        let settings = crate::db::chat_settings::find_by_user_id(conn, &uid)?;
        let mut lookup = |id: &str| -> Option<String> {
            crate::db::connection_profiles::find_by_id(conn, id)
                .ok()
                .flatten()
                .and_then(|p| p.get("userId").and_then(Value::as_str).map(str::to_string))
        };
        Ok(crate::cheap_llm::resolve_cheap_llm_profile_id(
            settings.as_ref(),
            &uid,
            &mut lookup,
        ))
    });
    let connection_profile_id = match resolved {
        Ok(Some(id)) => id,
        Ok(None) => {
            return bad_request(
                "No valid cheap LLM configured. Please set a cheap LLM profile in settings.",
            )
        }
        Err(e) => return internal(e),
    };

    let cid = chat_id.to_string();
    let messages =
        match db.read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &cid)) {
            Ok(m) => m,
            Err(e) => return internal(e),
        };
    let is_autonomous = chat.get("chatType").and_then(Value::as_str) == Some("autonomous");

    let str_field = |m: &Value, k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    let truthy = |m: &Value, k: &str| match m.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    };

    let mut entries: Vec<MemoryExtractionBatchEntry> = Vec::new();
    for m in &messages {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if is_autonomous {
            if m.get("role").and_then(Value::as_str) != Some("ASSISTANT")
                || truthy(m, "systemSender")
                || truthy(m, "isSilentMessage")
            {
                continue;
            }
            // v4 `if (!event.participantId) continue` — absent OR empty.
            let Some(id) = str_field(m, "id") else {
                continue;
            };
            if str_field(m, "participantId").is_none_or(|p| p.is_empty()) {
                continue;
            }
            entries.push(MemoryExtractionBatchEntry {
                turn_opener_message_id: None,
                extraction_anchor_message_id: Some(id),
            });
        } else {
            if m.get("role").and_then(Value::as_str) != Some("USER") || truthy(m, "systemSender") {
                continue;
            }
            let Some(id) = str_field(m, "id") else {
                continue;
            };
            entries.push(MemoryExtractionBatchEntry {
                turn_opener_message_id: Some(id),
                extraction_anchor_message_id: None,
            });
        }
    }

    if entries.is_empty() {
        return bad_request(if is_autonomous {
            "No assistant messages found in this chat — nothing to extract memories from."
        } else {
            "No user messages found in this chat — nothing to extract memories from."
        });
    }

    let job_ids = match enqueue_memory_extraction_batch(
        db,
        user_id,
        chat_id,
        &connection_profile_id,
        &entries,
        // v4 passes `{ priority: 0 }` explicitly.
        0.0,
    )
    .await
    {
        Ok(ids) => ids,
        Err(e) => return internal(e),
    };

    Response::Memory(json!({
        "success": true,
        "jobCount": job_ids.len(),
        "chatId": chat_id,
        "turnCount": entries.len(),
    }))
}

// === end P4.6BM ===
