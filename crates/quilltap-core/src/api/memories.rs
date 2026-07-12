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
use crate::db::{characters_read, chats_read, memories_read, tags, DbError};
use crate::model::embedding::EmbeddingProvider;
use crate::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use crate::services::memory_service::{
    delete_memories_by_chat_id_with_vectors, search_memories_semantic, SemanticSearchOptions,
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
    };
    let results =
        match search_memories_semantic(db, provider, &bag.character_id, &bag.query, &options).await
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
