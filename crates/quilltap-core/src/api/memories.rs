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

use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{
    characters_read, chat_settings, chats_read, instance_settings, memories_read, tags, DbError,
};
use crate::memory_scoring::{filter_importance, TOOL_MIN_IMPORTANCE};
use crate::model::embedding::EmbeddingProvider;
// P4.56: the shared "render a JSON number the JS way" walk (an integer-valued
// float collapses, exactly as `JSON.stringify` writes it).
use crate::admission::AdmissionAction;
use crate::model::tool_wire::normalize_js_numbers;
use crate::services::memory_retire::{
    announce_retirement, retire_chat_memories, retire_memories, RetireOptions, RetireReason,
};
use crate::services::queue_service::{enqueue_memory_extraction_batch, MemoryExtractionBatchEntry};
use crate::services::recall::search::{
    search_memories, RecallSearchOptions, SearchMemoriesOptions,
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
///
/// P4.D269 (v4 `d58548051`): `tier` is honoured only when exactly `hot` or
/// `cold` — anything else (`warm`, `HOT`, `''`) lists both tiers, never a
/// refusal (`route.ts:302-303`). The two paths keep v4's DIFFERENT NULL-tier
/// rules: the paginated path hands `tier` to the repository, whose
/// `findByFilter` renders an exact `"tier" = ?`, so a NULL-tier row is listed
/// under NEITHER tier; the legacy path filters the hydrated rows with
/// `(m.tier ?? 'hot') === tier`, and `findByFilter`'s hydration has already
/// defaulted a NULL tier to `'hot'`, so the row lists as hot there. `source`
/// gains `CONSOLIDATED` (the digests).
#[allow(clippy::too_many_arguments)]
pub fn memory_list(
    db: &Db,
    character_id: &str,
    search: Option<&str>,
    min_importance: Option<f64>,
    source: Option<&str>,
    tier: Option<&str>,
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
    // v4 only honors `source` when it is exactly AUTO|MANUAL|CONSOLIDATED.
    let source = source.filter(|s| *s == "AUTO" || *s == "MANUAL" || *s == "CONSOLIDATED");
    // v4: `tierParam === 'hot' || tierParam === 'cold' ? tierParam : undefined`.
    let tier = tier.filter(|t| *t == "hot" || *t == "cold");
    let search_owned = search.map(str::to_string);
    let cid = character_id.to_string();

    let result: Result<Value, DbError> = db.read_main(move |main| {
        let tm = tag_map(main)?;
        if let Some(limit_raw) = limit {
            // Paginated path — v4 `Math.max(1, Math.min(200, parseInt(limitParam,
            // 10) || 50))` (`route.ts:323-331`): a ZERO limit is falsy and pages
            // 50 (P4.168, B1-3 — it used to clamp to 1); a negative one clamps
            // to 1. (A non-numeric `limit` never reaches the dispatch edge —
            // the wire type is an integer.)
            let limit = if limit_raw == 0 {
                50
            } else {
                limit_raw.clamp(1, 200)
            };
            let offset = offset.unwrap_or(0).max(0);
            let opts = memories_read::PaginateOptions {
                limit,
                offset,
                sort_by: &sort_by,
                sort_order: &sort_order,
                search: search_owned.as_deref(),
                source,
                min_importance,
                // The exact `"tier" = ?` (a NULL-tier row matches neither).
                tier,
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
            // v4 `filterImportance(m) >= minImportance` (`route.ts:374`) —
            // `reinforcedImportance ?? importance`. The sort's `importance` key
            // below stays the RAW importance (`:384-386`).
            if let Some(min) = min_importance {
                memories.retain(|m| {
                    filter_importance(
                        m.get("importance").and_then(Value::as_f64).unwrap_or(0.0),
                        m.get("reinforcedImportance").and_then(Value::as_f64),
                    ) >= min
                });
            }
            if let Some(src) = source {
                memories.retain(|m| m.get("source").and_then(Value::as_str) == Some(src));
            }
            // v4 `memories.filter(m => (m.tier ?? 'hot') === tier)` — after the
            // source filter, before the sort.
            if let Some(t) = tier {
                memories.retain(|m| m.get("tier").and_then(Value::as_str).unwrap_or("hot") == t);
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

/// v4 item GET (`[id]/route.ts:34-74`): ownership via character, `tagDetails`,
/// `{memory}`.
///
/// "Viewing a memory is not an access (bug 219, refactor decision 7):
/// `lastAccessedAt` records that a model was shown the row, and it feeds
/// protection's recent-access bonus and housekeeping's inactivity clock. An
/// operator's or script's inspection must not move either." Delivery is the
/// ONE writer of `lastAccessedAt` (R-AG). A failed read is v4's catch: ERROR
/// `[Memories API v1] Error fetching memory` + 500 `Failed to fetch memory`.
pub async fn memory_get(db: &Db, memory_id: &str) -> Response {
    let mid = memory_id.to_string();
    let loaded: Result<Option<(Value, String)>, DbError> = db.read_main(move |main| {
        let Some(memory) = crate::db::fallback::find_by_id_or_none("memories", &mid, || {
            memories_read::find_by_id_validated(main, &mid)
        }) else {
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
        Err(e) => {
            tracing::error!(
                error = %crate::db::fallback::error_text(&e),
                "[Memories API v1] Error fetching memory"
            );
            return Response::error(ErrorKind::Internal, "Failed to fetch memory");
        }
    };
    tracing::debug!(
        memoryId = memory_id,
        characterId = %character_id,
        "[Memories API v1] Fetched memory (access time not stamped)"
    );
    Response::Memory(json!({ "memory": memory }))
}

// ===========================================================================
// Character memory counts (v4 GET ?action=character-memory-counts)
// ===========================================================================

/// v4 `handleCharacterMemoryCounts` (`route.ts:1022-1052`): `{success,
/// characters: [{id, name, memoryCount, hotTotal, demotable, protected,
/// cold}]}`.
///
/// "The housekeeping cap measures demotable rows only (hot AUTO; bug 210), so
/// the card compares `demotable` to the cap. `memoryCount` stays the total
/// across both tiers for older readers." Sorted by the cap-measured count
/// descending, then `memoryCount` descending ("so characters nearest their cap
/// surface first"); v4's `Array.sort` is stable, so a full tie keeps the
/// `findByUserId` order.
pub fn memory_character_counts(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let result: Result<Vec<Value>, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let chars = characters_read::find_by_user_id(main, mount, &uid)?;
            // (demotable, memoryCount, row)
            let mut out: Vec<(i64, i64, Value)> = Vec::with_capacity(chars.len());
            for c in chars {
                let id = c
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let name = c.get("name").cloned().unwrap_or(Value::Null);
                let b = memories_read::count_cap_breakdown(main, &id)?;
                let memory_count = b.hot_total + b.cold;
                out.push((
                    b.demotable,
                    memory_count,
                    json!({
                        "id": id,
                        "name": name,
                        "memoryCount": memory_count,
                        "hotTotal": b.hot_total,
                        "demotable": b.demotable,
                        "protected": b.protected,
                        "cold": b.cold,
                    }),
                ));
            }
            // `b.demotable - a.demotable || b.memoryCount - a.memoryCount`
            // (`slice::sort_by` is stable, as v4's is).
            out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
            Ok(out.into_iter().map(|(_, _, v)| v).collect())
        })
    });
    match result {
        Ok(characters) => {
            tracing::debug!(
                userId = user_id,
                characters = characters.len(),
                "[Memories API] Character memory counts computed"
            );
            Response::Memory(json!({ "success": true, "characters": characters }))
        }
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Create (v4 POST /api/v1/memories, no action)
// ===========================================================================

/// v4 `createMemorySchema`'s OUTPUT (`route.ts:94-116`): `content` / `summary`
/// TRIMMED (a Zod transform — the stored value is the trimmed one, bug 236),
/// the prefaults applied, an absent nullable key and an explicit `null` both
/// `None` (v4 reads every one through `?? null`).
struct ManualCreate {
    character_id: String,
    content: String,
    summary: String,
    keywords: Vec<String>,
    tags: Vec<String>,
    importance: f64,
    about_character_id: Option<String>,
    chat_id: Option<String>,
    project_id: Option<String>,
    source_message_id: Option<String>,
    kind: Option<String>,
    occurred_at: Option<String>,
    entities: Option<Vec<String>>,
}

/// v4's `MemoryKindEnum` (`lib/schemas/memory.types.ts`), in schema order.
const MEMORY_KINDS: [&str; 2] = ["semantic", "episodic"];

/// `z.string().trim().min(1, msg)` at `k` — JavaScript's trim (L-7:
/// [`crate::jsstr::js_trim`], never `str::trim`, which differs on U+FEFF and
/// U+0085), then the length in UTF-16 units.
fn trimmed_min1(
    obj: &Map<String, Value>,
    k: &str,
    msg: &str,
    issues: &mut Vec<ZodIssue>,
) -> Option<String> {
    match obj.get(k) {
        Some(Value::String(s)) => {
            let t = crate::jsstr::js_trim(s);
            if t.is_empty() {
                issues.push(ZodIssue::too_small_string_message(
                    json!(1),
                    vec![zod_key(k)],
                    msg,
                ));
            }
            Some(t.to_string())
        }
        other => {
            issues.push(ZodIssue::invalid_type("string", vec![zod_key(k)], other));
            None
        }
    }
}

/// `z.uuid().nullable().optional()` at `k`: absent / `null` → `None`.
fn nullable_uuid(obj: &Map<String, Value>, k: &str, issues: &mut Vec<ZodIssue>) -> Option<String> {
    match obj.get(k) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if zod_uuid_ok(s) => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push(ZodIssue::invalid_uuid(vec![zod_key(k)]));
            None
        }
        Some(other) => {
            issues.push(ZodIssue::invalid_type(
                "string",
                vec![zod_key(k)],
                Some(other),
            ));
            None
        }
    }
}

/// `z.array(item)` at `k` (absent → `None`): each element through `item`.
fn array_of(
    obj: &Map<String, Value>,
    k: &str,
    issues: &mut Vec<ZodIssue>,
    mut item: impl FnMut(&Value, Vec<Value>, &mut Vec<ZodIssue>) -> Option<String>,
) -> Option<Vec<String>> {
    match obj.get(k) {
        None => None,
        Some(Value::Array(a)) => Some(
            a.iter()
                .enumerate()
                .filter_map(|(i, v)| item(v, vec![zod_key(k), json!(i)], issues))
                .collect(),
        ),
        Some(other) => {
            issues.push(ZodIssue::invalid_type(
                "array",
                vec![zod_key(k)],
                Some(other),
            ));
            None
        }
    }
}

/// v4 `createMemorySchema.safeParse(body)` — Zod-faithful over the dispatch
/// bag: the issues in schema order (the `parse_search_bag` shape; L-1 keeps the
/// house envelope, so the switch to v4's `details` is one line), unknown keys
/// — `source` and `skipGate` among them (bug 218) — stripped.
fn parse_create_bag(bag: &Value) -> Result<ManualCreate, Vec<ZodIssue>> {
    let Some(obj) = bag.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(bag))]);
    };
    let mut issues = Vec::new();
    let with_message = |mut issue: ZodIssue, text: &str| {
        if let ZodIssue::InvalidType { message, .. } | ZodIssue::InvalidFormat { message, .. } =
            &mut issue
        {
            *message = text.to_string();
        }
        issue
    };
    let character_id = match obj.get("characterId") {
        Some(Value::String(s)) if zod_uuid_ok(s) => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push(with_message(
                ZodIssue::invalid_uuid(vec![zod_key("characterId")]),
                "Character ID is required",
            ));
            None
        }
        other => {
            issues.push(with_message(
                ZodIssue::invalid_type("string", vec![zod_key("characterId")], other),
                "Character ID is required",
            ));
            None
        }
    };
    let content = trimmed_min1(obj, "content", "Memory content is required", &mut issues);
    let summary = trimmed_min1(obj, "summary", "Memory summary is required", &mut issues);
    let keywords = array_of(obj, "keywords", &mut issues, |v, path, issues| match v {
        Value::String(s) => Some(s.clone()),
        other => {
            issues.push(ZodIssue::invalid_type("string", path, Some(other)));
            None
        }
    });
    let tags = array_of(obj, "tags", &mut issues, |v, path, issues| match v {
        Value::String(s) if zod_uuid_ok(s) => Some(s.clone()),
        Value::String(_) => {
            issues.push(ZodIssue::invalid_uuid(path));
            None
        }
        other => {
            issues.push(ZodIssue::invalid_type("string", path, Some(other)));
            None
        }
    });
    let importance = if obj.contains_key("importance") {
        zod_bounded_number(obj, "importance", 0.0, 1.0, &mut issues)
    } else {
        Some(0.5)
    };
    let about_character_id = nullable_uuid(obj, "aboutCharacterId", &mut issues);
    let chat_id = nullable_uuid(obj, "chatId", &mut issues);
    let project_id = nullable_uuid(obj, "projectId", &mut issues);
    let source_message_id = nullable_uuid(obj, "sourceMessageId", &mut issues);
    let kind = match obj.get("kind") {
        None => None,
        Some(Value::String(s)) if MEMORY_KINDS.contains(&s.as_str()) => Some(s.clone()),
        Some(_) => {
            issues.push(ZodIssue::invalid_value(
                &MEMORY_KINDS,
                vec![zod_key("kind")],
            ));
            None
        }
    };
    let occurred_at = match obj.get("occurredAt") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if super::zod_issues::zod_iso_datetime_offset_ok(s) => {
            Some(s.clone())
        }
        Some(Value::String(_)) => {
            issues.push(super::zod_issues::invalid_datetime_offset(vec![zod_key(
                "occurredAt",
            )]));
            None
        }
        Some(other) => {
            issues.push(ZodIssue::invalid_type(
                "string",
                vec![zod_key("occurredAt")],
                Some(other),
            ));
            None
        }
    };
    // `z.array(z.string().min(1)).max(MEMORY_ENTITY_CAP)`. Zod 4.6.7's
    // `capSize` (P4.D311, measured at the `25b8fab6a` pin): an array PAST the
    // cap answers ONLY its own `too_big` and no element is parsed (4.6.5
    // reported the element issues first, then the `too_big`).
    let entities = match obj.get("entities") {
        Some(Value::Array(a)) if a.len() > crate::memory_candidate::MEMORY_ENTITY_CAP => {
            issues.push(ZodIssue::too_big_array(
                json!(crate::memory_candidate::MEMORY_ENTITY_CAP),
                vec![zod_key("entities")],
            ));
            None
        }
        _ => array_of(obj, "entities", &mut issues, |v, path, issues| match v {
            Value::String(s) => {
                if s.is_empty() {
                    issues.push(ZodIssue::too_small_string(json!(1), path));
                }
                Some(s.clone())
            }
            other => {
                issues.push(ZodIssue::invalid_type("string", path, Some(other)));
                None
            }
        }),
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(ManualCreate {
        character_id: character_id.unwrap_or_default(),
        content: content.unwrap_or_default(),
        summary: summary.unwrap_or_default(),
        keywords: keywords.unwrap_or_default(),
        tags: tags.unwrap_or_default(),
        importance: importance.unwrap_or(0.5),
        about_character_id,
        chat_id,
        project_id,
        source_message_id,
        kind,
        occurred_at,
        entities,
    })
}

/// A v4 context value as the file layer renders it: a string bare, anything
/// else as its JSON text (`null`, `true`, `5`).
fn field_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The project a manual memory belongs to (v4 `route.ts:532-541`): an explicit
/// `projectId` must exist — `Err` carries the refusal (404 `Project not found`,
/// or the house 503 for a project whose document store is unavailable).
fn find_project(db: &Db, project_id: &str) -> Result<Option<String>, Response> {
    use crate::db::document_store_overlay::OverlayError;
    let read = db.read_main(|main| {
        Ok(db.read_mount_index(|mount| {
            Ok(crate::db::projects::ProjectsRepository::new(main, mount).find_by_id(project_id))
        }))
    });
    let found = match read {
        Ok(Ok(Ok(found))) => found,
        Ok(Ok(Err(unavailable @ OverlayError::Unavailable { .. }))) => {
            return Err(super::types::db_error_response(unavailable.into_db()))
        }
        // v4's `_findById` is a fallback read: a failed query logs and reads
        // as absent.
        Ok(Ok(Err(OverlayError::Db(e)))) | Ok(Err(e)) | Err(e) => {
            crate::db::fallback::find_by_id_or_none("projects", project_id, || Err(e))
        }
    };
    Ok(found.and_then(|p| p.get("id").and_then(Value::as_str).map(str::to_string)))
}

/// v4 `handleCreateMemory` (`route.ts:506-638`) — the manual create through
/// admission. The order is load-bearing (R-I): Zod → the `source` / `skipGate`
/// DEBUG → character 404 → project 404 → chat 404 (the project derived from
/// the chat when none was named) → the candidate → `admit_candidate` → the
/// reason arms. "Every manual write goes through the gate (decision 13):
/// source forced to MANUAL, provenance stamped 'manual'. A hand-written memory
/// carries no targeting tags (it reads as the defaults) and its event time is
/// given, not phrased."
///
/// `provider` is the OPTIONAL embedding seam (L-2): `None` drives admission
/// with [`NoEmbeddingProvider`](crate::model::embedding::NoEmbeddingProvider)
/// — v4's no-profile state — so an unassembled engine answers v4's order and
/// its 502 through admission's own `embedding-failed` arm. `now_ms` is the
/// candidate's `anchorTimeMs` (v4 `Date.now()`); `host_zone_fallback` the
/// memory calendar's host zone.
///
/// Success is `{memory, action}` — the 201-vs-200 split rides `action`
/// (`insert` / `insert-linked` → v4's 201; the dispatch edge has no status).
/// The 502 is `Internal` + `details.status: 502` (R-H, L-5).
pub async fn memory_create<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    now_ms: f64,
    host_zone_fallback: &str,
    bag: Value,
) -> Response {
    let data = match parse_create_bag(&bag) {
        Ok(d) => d,
        Err(_issues) => return bad_request("Validation error"),
    };
    // v4 `route.ts:517-523`: the client's `source` / `skipGate` are stripped by
    // Zod; their presence is narrated. An absent value is omitted (§R.5) —
    // `source: null` is present.
    if let Some(obj) = bag.as_object() {
        let source = obj.get("source");
        let skip_gate = obj.get("skipGate");
        if skip_gate.is_some() || source.is_some_and(|s| s != "MANUAL") {
            let cid = data.character_id.as_str();
            const MSG: &str =
                "[Memories API] Ignoring client-supplied source/skipGate on manual create";
            match (source.map(field_text), skip_gate.map(field_text)) {
                (Some(s), Some(k)) => {
                    tracing::debug!(characterId = cid, source = %s, skipGate = %k, "{MSG}")
                }
                (Some(s), None) => tracing::debug!(characterId = cid, source = %s, "{MSG}"),
                (None, Some(k)) => tracing::debug!(characterId = cid, skipGate = %k, "{MSG}"),
                (None, None) => {}
            }
        }
    }
    match character_exists(db, &data.character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    // "Resolve the project: an explicit one must exist; otherwise the source
    // chat's project (if any) is the memory's, as it is for extracted rows."
    let mut project_id: Option<String> = None;
    if let Some(pid) = &data.project_id {
        match find_project(db, pid) {
            Ok(Some(id)) => project_id = Some(id),
            Ok(None) => return not_found("Project"),
            Err(refusal) => return refusal,
        }
    }
    if let Some(chat_id) = &data.chat_id {
        let cid = chat_id.clone();
        let chat = match db.read_main(move |main| chats_read::find_by_id(main, &cid)) {
            Ok(c) => c,
            Err(e) => return internal(e),
        };
        let Some(chat) = chat else {
            return not_found("Chat");
        };
        if project_id.is_none() {
            project_id = chat
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }

    // The candidate, in v4's literal order (`route.ts:555-574`).
    let candidate_json = json!({
        "content": data.content,
        "summary": data.summary,
        "keywords": data.keywords,
        "targeting": null,
        "importance": data.importance,
        "kind": data.kind.as_deref().unwrap_or("semantic"),
        "when": null,
        "entities": data.entities.clone().unwrap_or_default(),
        "tags": data.tags,
        "subject": {
            "holderId": data.character_id,
            "aboutId": data.about_character_id,
        },
        "provenance": {
            "source": "MANUAL",
            "witnessedContext": "manual",
            "chatId": data.chat_id,
            "projectId": project_id,
            "sourceMessageId": data.source_message_id,
            "anchorTimeMs": now_ms,
            "pass": "manual",
        },
    });
    let candidate = match crate::memory_candidate::parse_memory_candidate(&candidate_json) {
        Ok(c) => c,
        Err(issues) => {
            // "The request schema should already have refused anything the
            // candidate refuses; a gap between the two is still the client's
            // 400, never a 500." `issues` is the COUNT.
            tracing::debug!(
                characterId = %data.character_id,
                issues = issues.len(),
                "[Memories API] Manual memory refused by the candidate schema"
            );
            return bad_request("Validation error");
        }
    };
    // "A hand-written memory never folds into an extracted row (it is inserted
    // and linked instead); folded into a MANUAL row or a digest, it raises that
    // row's importance to the one asked for."
    let resolved = crate::memory_candidate::ResolvedCandidateTime {
        occurred_at: data.occurred_at.clone(),
        narrative_time: None,
        source_message_timestamp: None,
    };
    let opts = || crate::services::admission::AdmitOptions {
        user_id,
        dry_run: false,
        backfill: false,
        dry_run_book: None,
        key_scope: None,
        host_zone_fallback,
    };
    let outcome = match provider {
        Some(p) => {
            crate::services::admission::admit_candidate(db, p, &candidate, &resolved, opts()).await
        }
        None => {
            crate::services::admission::admit_candidate(
                db,
                &crate::model::embedding::NoEmbeddingProvider,
                &candidate,
                &resolved,
                opts(),
            )
            .await
        }
    };
    let outcome = match outcome {
        Ok(o) => o,
        // P4.168 (H-2): the repository's `MemorySchema` refusal (an offset
        // `occurredAt`) — v4's ZodError reaches `handleRouteError`, which
        // answers 400 `{"error": "Validation error", "details": issues}`
        // (`context.ts:166-167`, `responses.ts:108-118`). AFTER the embedding
        // call: an offset with a failing embedding answers the 502 above.
        Err(DbError::RowSchemaRefusal { issues, .. }) => {
            let mut resp = bad_request("Validation error");
            if let Response::Error(e) = &mut resp {
                e.details = Some(Box::new(issues));
            }
            return resp;
        }
        Err(e) => return internal(e),
    };

    let Some(memory) = outcome.memory else {
        // "Nothing was written. Say why: only an embedding failure is the
        // provider's fault (bug 236)."
        let reason = match &outcome.action {
            AdmissionAction::Reject { reason } => reason.as_str(),
            _ => "unknown",
        };
        tracing::debug!(
            characterId = %data.character_id,
            reason = reason,
            "[Memories API] Manual memory not admitted"
        );
        return match reason {
            "empty" => bad_request("Memory content and summary must not be blank."),
            // "A memory without an embedding would be invisible to every future
            // admission, so none was kept. The edit route answers the same way."
            "embedding-failed" => bad_gateway(
                "The memory was not saved because its text could not be embedded. Check the configured embedding profile.",
            ),
            _ => Response::error(ErrorKind::Internal, "The memory was not saved."),
        };
    };

    let inserted = crate::admission::admission_inserted(&outcome.action);
    let action = outcome.action.kind().as_str();
    let memory_id = memory
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let project_text = project_id
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| "null".to_string());
    tracing::debug!(
        characterId = %data.character_id,
        memoryId = %memory_id,
        action = action,
        inserted = inserted,
        projectId = %project_text,
        "[Memories API] Manual memory create resolved"
    );

    // v4: "Schedule vocabulary refit for BUILTIN profiles (debounced). This
    // runs in the background and doesn't block the response" —
    // `scheduleRefit(user.id)` (P4.D292's R-Q debounce: armed here, fired by
    // the host's driver), WARN `[Memories API] Failed to schedule refit`
    // `{error}` on a rejection (a profile read failure). Landed at the
    // `83334b9a5` unification (§S.5 (2)).
    if let Err(e) = crate::services::embedding_job_scheduler::schedule_refit(
        db,
        user_id,
        None,
        false,
        now_ms as i64,
    )
    .await
    {
        tracing::warn!(
            error = %crate::db::fallback::error_text(&e),
            "[Memories API] Failed to schedule refit"
        );
    }

    // "201 only when a row was created; a reinforcement or near-duplicate
    // absorption returns 200 with the existing row, and `action` says which."
    Response::Memory(json!({ "memory": memory, "action": action }))
}

/// §R.7 `MEMORY_BAD_GATEWAY_AS_INTERNAL` (R-H, L-5): v4's 502 carried as
/// `Internal` with v4's message and `details: { "status": 502 }` — built by
/// MUTATING a constructed error, never a `CoreError` literal (§R.10(a)).
fn bad_gateway(message: impl Into<String>) -> Response {
    let mut r = Response::error(ErrorKind::Internal, message);
    if let Response::Error(e) = &mut r {
        e.details = Some(Box::new(json!({ "status": 502 })));
    }
    r
}

// ===========================================================================
// Update (v4 PUT /api/v1/memories/[id])
// ===========================================================================

/// v4 `updateMemorySchema.parse(body)`'s OUTPUT (`[id]/route.ts:19-29`) as
/// admission's [`MemoryEditPatch`](crate::services::admission::MemoryEditPatch):
/// every key optional, `content` / `summary` TRIMMED (bug 236 — a blank edit is
/// a 400), `aboutCharacterId` / `chatId` nullable (present `null` → `Some(None)`).
/// Issues in schema order (the house 400 per L-1).
fn parse_update_bag(
    bag: &Value,
) -> Result<crate::services::admission::MemoryEditPatch, Vec<ZodIssue>> {
    let Some(obj) = bag.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(bag))]);
    };
    let mut issues = Vec::new();
    let trimmed_opt = |k: &str, issues: &mut Vec<ZodIssue>| -> Option<String> {
        obj.get(k)?;
        // `z.string().trim().min(1).optional()` — Zod's default `.min(1)` text.
        match obj.get(k) {
            Some(Value::String(s)) => {
                let t = crate::jsstr::js_trim(s);
                if t.is_empty() {
                    issues.push(ZodIssue::too_small_string(json!(1), vec![zod_key(k)]));
                }
                Some(t.to_string())
            }
            other => {
                issues.push(ZodIssue::invalid_type("string", vec![zod_key(k)], other));
                None
            }
        }
    };
    let content = trimmed_opt("content", &mut issues);
    let summary = trimmed_opt("summary", &mut issues);
    let keywords = array_of(obj, "keywords", &mut issues, |v, path, issues| match v {
        Value::String(s) => Some(s.clone()),
        other => {
            issues.push(ZodIssue::invalid_type("string", path, Some(other)));
            None
        }
    });
    let uuid_item = |v: &Value, path: Vec<Value>, issues: &mut Vec<ZodIssue>| match v {
        Value::String(s) if zod_uuid_ok(s) => Some(s.clone()),
        Value::String(_) => {
            issues.push(ZodIssue::invalid_uuid(path));
            None
        }
        other => {
            issues.push(ZodIssue::invalid_type("string", path, Some(other)));
            None
        }
    };
    let tags = array_of(obj, "tags", &mut issues, uuid_item);
    let importance = zod_bounded_number(obj, "importance", 0.0, 1.0, &mut issues);
    // Present `null` → `Some(None)`; absent → `None` (untouched).
    let nullable = |k: &str, issues: &mut Vec<ZodIssue>| -> Option<Option<String>> {
        match obj.get(k) {
            None => None,
            Some(Value::Null) => Some(None),
            Some(_) => nullable_uuid(obj, k, issues).map(Some),
        }
    };
    let about_character_id = nullable("aboutCharacterId", &mut issues);
    let chat_id = nullable("chatId", &mut issues);
    let related_memory_ids = array_of(obj, "relatedMemoryIds", &mut issues, uuid_item);
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(crate::services::admission::MemoryEditPatch {
        content,
        summary,
        keywords,
        tags,
        importance,
        about_character_id,
        chat_id,
        related_memory_ids,
    })
}

/// v4 item PUT (`[id]/route.ts:79-142`): the 404s FIRST (the row, then its
/// character — a malformed body for a missing memory is a 404, not a 400),
/// then `updateMemorySchema.parse` (the house 400), then the edit chokepoint.
///
/// "Through the edit chokepoint, not a raw row update: it re-embeds changed
/// text, recomputes reinforcedImportance, links both sides, invalidates the
/// frozen archive, publishes `memories` and refreshes a digest's Commonplace
/// mirror (bug 203)." `provider` is the OPTIONAL seam (L-2): `None` drives
/// [`NoEmbeddingProvider`](crate::model::embedding::NoEmbeddingProvider), so an
/// importance-only edit still succeeds and a text edit answers v4's 502.
pub async fn memory_update<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    memory_id: &str,
    host_zone_fallback: &str,
    now_ms: f64,
    bag: Value,
) -> Response {
    // Ownership: find existing + its character.
    let mid = memory_id.to_string();
    let character_id: Result<Option<String>, DbError> = db.read_main(move |main| {
        let Some(existing) = crate::db::fallback::find_by_id_or_none("memories", &mid, || {
            memories_read::find_by_id_validated(main, &mid)
        }) else {
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
    let patch = match parse_update_bag(&bag) {
        Ok(p) => p,
        Err(_issues) => return bad_request("Validation error"),
    };
    let fields: Vec<&'static str> = patch.keys();
    let result = match provider {
        Some(p) => {
            crate::services::admission::update_memory(
                db,
                p,
                &character_id,
                memory_id,
                patch,
                user_id,
                host_zone_fallback,
            )
            .await
        }
        None => {
            crate::services::admission::update_memory(
                db,
                &crate::model::embedding::NoEmbeddingProvider,
                &character_id,
                memory_id,
                patch,
                user_id,
                host_zone_fallback,
            )
            .await
        }
    };
    use crate::services::admission::UpdateMemoryResult;
    let memory = match result {
        Ok(UpdateMemoryResult::Ok { memory }) => memory,
        Ok(UpdateMemoryResult::EmbeddingFailed { message }) => {
            // "The provider could not embed the new text; nothing was saved."
            tracing::warn!(
                memoryId = memory_id,
                characterId = %character_id,
                error = %message,
                "[Memories API v1] Memory edit refused: embedding failed"
            );
            return bad_gateway(format!(
                "The memory was not saved because its new text could not be embedded. {message}"
            ));
        }
        Ok(UpdateMemoryResult::InvalidRelated { invalid_ids }) => {
            return bad_request(format!(
                "relatedMemoryIds must name existing memories of the same character: {}",
                invalid_ids.join(", ")
            ))
        }
        Ok(UpdateMemoryResult::NotFound) => return not_found("Memory"),
        Err(e) => return internal(e),
    };
    tracing::debug!(
        memoryId = memory_id,
        characterId = %character_id,
        fieldsJson = %serde_json::to_string(&fields).unwrap_or_default(),
        "[Memories API v1] Memory updated"
    );
    // v4: "Schedule refit for BUILTIN profiles (non-blocking)" —
    // `scheduleRefit(user.id)`, WARN `[Memories API v1] Failed to schedule
    // refit after update` `{memoryId, error}` on a rejection. P4.D292's R-Q
    // debounce; landed at the `83334b9a5` unification (§S.5 (2)).
    if let Err(e) = crate::services::embedding_job_scheduler::schedule_refit(
        db,
        user_id,
        None,
        false,
        now_ms as i64,
    )
    .await
    {
        tracing::warn!(
            memoryId = memory_id,
            error = %crate::db::fallback::error_text(&e),
            "[Memories API v1] Failed to schedule refit after update"
        );
    }
    Response::Memory(json!({ "memory": memory }))
}

// ===========================================================================
// Delete (v4 DELETE /api/v1/memories/[id])
// ===========================================================================

/// v4 item DELETE (`[id]/route.ts:147-195`): ownership via character, then
/// the ONE retire path (`item-delete`), `{success: true}`.
///
/// "The deletion chokepoint removes the row, scrubs every link that names it
/// and evicts its vector (bug 200: an orphan vector blinded the gate)." A row
/// that vanished between the read and the retire (`retired === 0`) is v4's
/// 404. The announcement runs after the commit (R-AF). A thrown error is v4's
/// catch: ERROR `[Memories API v1] Error deleting memory` + 500 `Failed to
/// delete memory`.
pub async fn memory_delete(db: &Db, user_id: &str, memory_id: &str, now_ms: f64) -> Response {
    let mid = memory_id.to_string();
    let owner: Result<Option<String>, DbError> = db.read_main(move |main| {
        let Some(existing) = crate::db::fallback::find_by_id_or_none("memories", &mid, || {
            memories_read::find_by_id_validated(main, &mid)
        }) else {
            return Ok(None);
        };
        let cid = existing
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Ok(characters_read::find_by_id_raw(main, &cid)?.map(|_| cid))
    });
    let character_id = match owner {
        Ok(Some(cid)) => cid,
        Ok(None) => return not_found("Memory"),
        Err(e) => return delete_failed(&e),
    };
    let ids = vec![memory_id.to_string()];
    let retired = db
        .write(move |w| {
            retire_memories(
                w.main().connection(),
                &ids,
                RetireOptions::new(RetireReason::ItemDelete),
            )
        })
        .await;
    let report = match retired {
        Ok(report) => report,
        Err(e) => return delete_failed(&e),
    };
    announce_retirement(db, report.announcement.as_ref()).await;
    if report.retired == 0 {
        return not_found("Memory");
    }

    // v4: "Clean up embedding status (non-blocking)" — `handleEntityDeletion(
    // 'MEMORY', memoryId)`, WARN `[Memories API v1] Failed to handle entity
    // deletion` `{memoryId, error}` on a rejection (after the repository's own
    // rethrow ERROR) — then "Schedule refit for BUILTIN profiles
    // (non-blocking)" — `scheduleRefit(user.id)`, WARN `[Memories API v1]
    // Failed to schedule refit after delete` `{memoryId, error}`. Both
    // P4.D292's (R-Q); landed at the `83334b9a5` unification (§S.5 (2)).
    if let Err(e) =
        crate::services::embedding_job_scheduler::handle_entity_deletion(db, "MEMORY", memory_id)
            .await
    {
        tracing::warn!(
            memoryId = memory_id,
            error = %crate::db::fallback::error_text(&e),
            "[Memories API v1] Failed to handle entity deletion"
        );
    }
    if let Err(e) = crate::services::embedding_job_scheduler::schedule_refit(
        db,
        user_id,
        None,
        false,
        now_ms as i64,
    )
    .await
    {
        tracing::warn!(
            memoryId = memory_id,
            error = %crate::db::fallback::error_text(&e),
            "[Memories API v1] Failed to schedule refit after delete"
        );
    }

    tracing::info!(
        memoryId = memory_id,
        characterId = %character_id,
        "[Memories API v1] Memory deleted"
    );
    Response::Memory(json!({ "success": true }))
}

/// v4 item DELETE's catch arm.
fn delete_failed(e: &DbError) -> Response {
    tracing::error!(
        error = %crate::db::fallback::error_text(e),
        "[Memories API v1] Error deleting memory"
    );
    Response::error(ErrorKind::Internal, "Failed to delete memory")
}

// ===========================================================================
// Delete by chat (v4 DELETE /api/v1/memories?chatId=)
// ===========================================================================

/// v4 `handleDeleteByChatId` (`route.ts:1240-1269`): chat ownership, then
/// `retireChatMemories` — "Manual rows, cold rows and digests with surviving
/// members are spared (bug 201)" — and `{success, chatId, deletedCount,
/// sparedCount}`.
pub async fn memory_delete_by_chat(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let owned: Result<bool, DbError> =
        db.read_main(move |main| Ok(chats_read::find_by_id(main, &cid)?.is_some()));
    match owned {
        Ok(true) => {}
        Ok(false) => return not_found("Chat"),
        Err(e) => return internal(e),
    }
    // The announcement runs after the commit (R-AF).
    let cid = chat_id.to_string();
    let wiped = db
        .write(move |w| retire_chat_memories(w.main().connection(), &cid))
        .await;
    match wiped {
        Ok(report) => {
            announce_retirement(db, report.announcement.as_ref()).await;
            // v4 `app/api/v1/memories/route.ts:1250`. No publish here: this path
            // runs through `retireMemories`, whose announcement already
            // announces `memories` collection-wide — a
            // chat-scoped subscriber takes it, since the id filter only discards
            // an event naming a DIFFERENT id. A second, chat-scoped hint would
            // cost every subscriber a duplicate refetch, and would fire on the
            // one case where the gate correctly stays silent: a chat with no
            // memories, where nothing was deleted and nothing changed.
            //
            // Log-only (P4.88, the finding-#103/#110/#116 class) — pinned by the
            // capturing layer in this module's tests. It fires on BOTH arms, the
            // empty chat included, because v4 logs after the call, not inside a
            // deleted-something branch.
            tracing::debug!(
                chatId = chat_id,
                deleted = report.retired,
                spared = report.spared,
                "[Memories API] Wiped a chat's extracted memories"
            );
            Response::Memory(json!({
                "success": true,
                "chatId": chat_id,
                "deletedCount": report.retired,
                "sparedCount": report.spared,
            }))
        }
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Search (v4 POST /api/v1/memories?action=search)
// ===========================================================================

/// The parsed search body (v4 `searchMemorySchema.parse(body)`'s output).
struct SearchBag {
    character_id: String,
    query: String,
    limit: f64,
    min_importance: Option<f64>,
    min_score: Option<f64>,
    source: Option<String>,
}

/// v4's search sources (`route.ts:96`, `d58548051` added `CONSOLIDATED`).
const SEARCH_SOURCES: [&str; 3] = ["AUTO", "MANUAL", "CONSOLIDATED"];

/// One `z.number().min(lo).max(hi)` field: the type issue, else the bounds.
fn zod_bounded_number(
    bag: &Map<String, Value>,
    field: &str,
    lo: f64,
    hi: f64,
    issues: &mut Vec<ZodIssue>,
) -> Option<f64> {
    let v = bag.get(field)?;
    let Some(n) = v.as_f64() else {
        issues.push(ZodIssue::invalid_type(
            "number",
            vec![zod_key(field)],
            Some(v),
        ));
        return None;
    };
    if n < lo {
        issues.push(ZodIssue::too_small_number(
            crate::db::js_number_to_json(lo),
            vec![zod_key(field)],
        ));
    }
    if n > hi {
        issues.push(ZodIssue::too_big_number(
            crate::db::js_number_to_json(hi),
            vec![zod_key(field)],
        ));
    }
    Some(n)
}

/// P4.D269 (R-E) — v4 `searchMemorySchema` (`route.ts:90-97`), Zod-faithful:
///
/// ```ts
/// characterId: z.uuid('Character ID is required'),          // type AND format carry the message
/// query: z.string().min(1, 'Search query is required'),
/// limit: z.number().min(1).max(100).prefault(20),
/// minImportance: z.number().min(0).max(1).optional(),
/// minScore: z.number().min(0).max(1).optional(),
/// source: z.enum(['AUTO', 'MANUAL', 'CONSOLIDATED']).optional(),  // null → invalid_value
/// ```
///
/// v4's `.parse` THROWS, and `createContextHandler` maps the `ZodError` to
/// 400 `{error: 'Validation error', details: issues}` (`context.ts:166-167`).
/// Issue bytes measured at the pin; unknown keys are stripped. The dispatch
/// bag is an untyped `Value`, so every shape reaches this gate.
fn parse_search_bag(bag: &Value) -> Result<SearchBag, Vec<ZodIssue>> {
    let Some(obj) = bag.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(bag))]);
    };
    let mut issues = Vec::new();
    let with_message = |mut issue: ZodIssue, text: &str| {
        match &mut issue {
            ZodIssue::InvalidType { message, .. } | ZodIssue::InvalidFormat { message, .. } => {
                *message = text.to_string();
            }
            _ => {}
        }
        issue
    };
    let character_id = match obj.get("characterId") {
        Some(Value::String(s)) if zod_uuid_ok(s) => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push(with_message(
                ZodIssue::invalid_uuid(vec![zod_key("characterId")]),
                "Character ID is required",
            ));
            None
        }
        other => {
            issues.push(with_message(
                ZodIssue::invalid_type("string", vec![zod_key("characterId")], other),
                "Character ID is required",
            ));
            None
        }
    };
    let query = match obj.get("query") {
        Some(Value::String(s)) => {
            if s.encode_utf16().count() < 1 {
                issues.push(ZodIssue::too_small_string_message(
                    json!(1),
                    vec![zod_key("query")],
                    "Search query is required",
                ));
            }
            Some(s.clone())
        }
        other => {
            issues.push(ZodIssue::invalid_type(
                "string",
                vec![zod_key("query")],
                other,
            ));
            None
        }
    };
    let limit = if obj.contains_key("limit") {
        zod_bounded_number(obj, "limit", 1.0, 100.0, &mut issues)
    } else {
        Some(20.0)
    };
    let min_importance = zod_bounded_number(obj, "minImportance", 0.0, 1.0, &mut issues);
    let min_score = zod_bounded_number(obj, "minScore", 0.0, 1.0, &mut issues);
    let source = match obj.get("source") {
        None => None,
        Some(Value::String(s)) if SEARCH_SOURCES.contains(&s.as_str()) => Some(s.clone()),
        Some(_) => {
            issues.push(ZodIssue::invalid_value(
                &SEARCH_SOURCES,
                vec![zod_key("source")],
            ));
            None
        }
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(SearchBag {
        character_id: character_id.unwrap_or_default(),
        query: query.unwrap_or_default(),
        limit: limit.unwrap_or(20.0),
        min_importance,
        min_score,
        source,
    })
}

/// v4 `handleSearch`: `searchMemoriesSemantic(…, { includeCold: true })` →
/// each result is `{...memory, score, usedEmbedding, tagDetails}` (that key
/// order — P4.D269 fixed v5's `tagDetails`-first rows); top-level `{memories,
/// count, query, usedEmbedding}`. `now_ms` is threaded so the
/// recency-weighted ranking is pinnable in the differential (v4's route uses
/// `Date.now()` — the oracle mocks it). The API search stamps no access times
/// (v4's "bumped inside searchMemoriesSemantic" comment at `:575` is stale at
/// the pin — F6 removed the bump; that removal is P4.D267's).
///
/// A fractional `limit` (Zod admits `2.5`) is floored here, where v4 slices
/// by the raw number — out of corpus, recorded.
pub async fn memory_search<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    user_id: &str,
    now_ms: f64,
    bag: Value,
) -> Response {
    let bag = match parse_search_bag(&bag) {
        Ok(b) => b,
        Err(issues) => return Response::validation_error(zod_issue_details(&issues)),
    };
    match character_exists(db, &bag.character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    // §S.5 (4), landed at the `83334b9a5` unification: the route reads
    // P4.D291's one-shot `searchMemories` directly (v4 `route.ts:655-672`);
    // the pre-round `search_memories_semantic` shim is gone with it.
    let options = SearchMemoriesOptions {
        search: RecallSearchOptions {
            limit: Some(bag.limit as usize),
            min_score: bag.min_score,
            // v4 `minImportance ?? TOOL_MIN_IMPORTANCE` (`route.ts:660`). Not a
            // no-op: with `0` the recall search filters on the
            // `reinforcedImportance` COLUMN, which excludes a NULL row (§R.4(f)).
            min_importance: Some(bag.min_importance.unwrap_or(TOOL_MIN_IMPORTANCE)),
            source: bag.source,
            about_character_id: None,
            literal_phrase: false,
            now_ms,
            // The Commonplace Book browses the whole archive, cold rows included.
            include_cold: true,
            ..Default::default()
        },
        user_id: user_id.to_string(),
        embedding_profile_id: None,
        ..Default::default()
    };
    let results = match search_memories(db, provider, &bag.character_id, &bag.query, &options).await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let used_embedding = results.first().map(|r| r.used_embedding).unwrap_or(false);
    let query = bag.query.clone();
    let body: Result<Value, DbError> = db.read_main(move |main| {
        let tm = tag_map(main)?;
        let memories: Vec<Value> = results
            .into_iter()
            .map(|r| {
                let mut m = r.memory;
                if let Some(obj) = m.as_object_mut() {
                    obj.insert("score".into(), crate::db::js_number_to_json(r.score));
                    obj.insert("usedEmbedding".into(), json!(r.used_embedding));
                }
                with_tag_details(m, &tm)
            })
            .collect();
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
///
/// Validation FIRST, before any read or write (v4 `recallConfigSchema.safeParse`
/// → `validationError` at 400): a present-but-invalid value refuses and stores
/// nothing — it must never be silently dropped in favour of the stored value.
/// v5 answers the message only, no `details` issues array — the recorded
/// divergence class (see `quilltap-web`'s `validation_error`).
pub async fn memory_recall_config_set(db: &Db, bag: Value) -> Response {
    let Some(obj) = bag.as_object() else {
        // Zod's root arm: a non-object body is its own invalid_type issue.
        return bad_request("Validation error");
    };
    if let Some(v) = obj.get("scopePolicy") {
        // Zod `invalid_value` — any non-member (wrong type included) fails.
        if !matches!(v.as_str(), Some("down-weight") | Some("exclude")) {
            return bad_request("Validation error");
        }
    }
    for key in ["expandRelated", "perTurnConversationSummaries"] {
        if let Some(v) = obj.get(key) {
            // Zod `invalid_type` — present must be a boolean (null included fails).
            if !v.is_boolean() {
                return bad_request("Validation error");
            }
        }
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
        Ok(_) => {
            tracing::info!(
                scopePolicy = %merged.scope_policy,
                expandRelated = merged.expand_related,
                perTurnConversationSummaries = merged.per_turn_conversation_summaries,
                "[Memories API] Recall settings updated (instance-wide)"
            );
            Response::Memory(json!({
                "success": true,
                "settings": merged.to_json(),
            }))
        }
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

/// v4 POST `?action=extraction-limits-config` (`route.ts:909-935`):
/// `extractionLimitsConfigSchema = patchSchemaOf(MemoryExtractionLimitsSchema)`
/// — "Every settings write validates through the façade's own schemas, as a
/// patch: an omitted key stays omitted, so a card that sends one field changes
/// one field (bug 244 — `.partial()` alone re-applies every default)." ONE
/// validator (C1-5's [`instance_settings::parse_memory_extraction_limits_patch`])
/// FIRST — a refusal stores nothing and answers the family's house 400
/// `Validation error` (L-1) — then the `?? current` merge, the store, the
/// INFO, `{success, settings: merged}`.
pub async fn memory_extraction_limits_set(db: &Db, bag: Value) -> Response {
    let patch = match instance_settings::parse_memory_extraction_limits_patch(&bag) {
        Ok(p) => p,
        Err(_issues) => return bad_request("Validation error"),
    };
    let current = match db.read_main(instance_settings::get_memory_extraction_limits) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    let pick = |key: &str| {
        patch
            .get(key)
            .cloned()
            .unwrap_or_else(|| current[key].clone())
    };
    // P4.56: the stored and echoed numbers render as JS renders them (an
    // integer-valued float collapses, as `JSON.stringify` writes it).
    let merged = normalize_js_numbers(json!({
        "enabled": pick("enabled"),
        "maxPerHour": pick("maxPerHour"),
        "softStartFraction": pick("softStartFraction"),
        "softFloor": pick("softFloor"),
    }));
    let m2 = merged.clone();
    let wrote = db
        .write(move |w| instance_settings::set_memory_extraction_limits(w.main().connection(), &m2))
        .await;
    match wrote {
        Ok(_) => {
            tracing::info!(
                enabled = %merged["enabled"],
                maxPerHour = %merged["maxPerHour"],
                "[Memories API] Extraction rate limits updated (instance-wide)"
            );
            Response::Memory(json!({ "success": true, "settings": merged }))
        }
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

/// v4 POST `?action=regenerate-all` — `handleRegenerateAll` (`route.ts:
/// 1271-1361`): wipe in-flight regenerate/extraction jobs (`MEMORY_REGENERATE_ALL`,
/// `MEMORY_REGENERATE_CHAT`, `MEMORY_EXTRACTION` — `ac6348c5f` dropped
/// `INTER_CHARACTER_MEMORY`, a retired type nothing enqueues), resolve the standard + dangerous-compatible cheap
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
                ],
                &["PENDING".into(), "PROCESSING".into()],
            )
        })
        .await;
    let cleared = match cleared {
        Ok(n) => n as i64,
        Err(e) => return internal(e),
    };
    if cleared > 0 {
        tracing::info!(
            userId = user_id,
            cleared = cleared,
            "[Memories API] Cleared in-flight regenerate jobs before fresh sweep"
        );
    }

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
        // v4 `3b463d6b1` (#76): `readConciergeSettings(settings).uncensoredTextProfileId
        // ?? null` — the desk's text profile moved into `conciergeSettings`.
        let uncensored_id = crate::services::dangerous_content::resolver::read_concierge_settings(
            settings.as_ref(),
        )
        .uncensored_text_profile_id;
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
    tracing::info!(
        userId = user_id,
        jobId = %job_id,
        isNew = is_new,
        standardProfileId = %standard_id,
        dangerousProfileId = %dangerous_id,
        "[Memories API] Regenerate-all fan-out enqueued"
    );

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

/// v4 `backfillStartSchema` (`route.ts:180-185`), Zod-faithful over the
/// dispatch bag: `characterId: z.uuid().optional()`, `batchSize:
/// z.number().int().min(1).max(2000).prefault(500)`. The issues come back in
/// schema order (the `parse_search_bag` shape); the handler answers the
/// memories family's house 400 `Validation error` (L-1).
fn parse_backfill_bag(bag: &Value) -> Result<(Option<String>, i64), Vec<ZodIssue>> {
    let Some(obj) = bag.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(bag))]);
    };
    let mut issues = Vec::new();
    let character_id = match obj.get("characterId") {
        None => None,
        Some(Value::String(s)) if zod_uuid_ok(s) => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push(ZodIssue::invalid_uuid(vec![zod_key("characterId")]));
            None
        }
        Some(other) => {
            issues.push(ZodIssue::invalid_type(
                "string",
                vec![zod_key("characterId")],
                Some(other),
            ));
            None
        }
    };
    let batch_size = match obj.get("batchSize") {
        None => Some(500),
        Some(v) => match v.as_f64() {
            Some(n) if n.fract() == 0.0 && n.abs() <= MAX_SAFE_INTEGER => {
                if n < 1.0 {
                    issues.push(ZodIssue::too_small_number(
                        json!(1),
                        vec![zod_key("batchSize")],
                    ));
                }
                if n > 2000.0 {
                    issues.push(ZodIssue::too_big_number(
                        json!(2000),
                        vec![zod_key("batchSize")],
                    ));
                }
                Some(n as i64)
            }
            Some(n) => {
                let as_value = crate::db::js_number_to_json(n);
                issues.push(ZodIssue::invalid_int_type(
                    vec![zod_key("batchSize")],
                    Some(&as_value),
                ));
                None
            }
            None => {
                issues.push(ZodIssue::invalid_type(
                    "number",
                    vec![zod_key("batchSize")],
                    Some(v),
                ));
                None
            }
        },
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok((character_id, batch_size.unwrap_or(500)))
}

/// v4 POST `?action=backfill-embeddings` — `handleBackfillStart`
/// (`route.ts:1082-1133`): ONE deduped batch through
/// `enqueueMissingEmbeddings` (rows already holding an active
/// `EMBEDDING_GENERATE` job are skipped). `{success, enqueued, remaining,
/// message}`. Every row already queued (`enqueued: 0`, `remaining > 0`) takes
/// the INFO + template arm, not the zero sentence.
pub async fn memory_backfill_start(db: &Db, user_id: &str, bag: Value) -> Response {
    let (character_id, batch_size) = match parse_backfill_bag(&bag) {
        Ok(parsed) => parsed,
        Err(_issues) => return bad_request("Validation error"),
    };
    if let Some(cid) = &character_id {
        match character_exists(db, cid) {
            Ok(true) => {}
            Ok(false) => return not_found("Character"),
            Err(e) => return internal(e),
        }
    }
    let queued = match crate::services::memory_index::enqueue_missing_embeddings(
        db,
        user_id,
        character_id.as_deref(),
        batch_size,
    )
    .await
    {
        Ok(q) => q,
        Err(e) => return internal(e),
    };
    let (enqueued, remaining) = match queued {
        crate::services::memory_index::EnqueueMissingEmbeddingsResult::NoProfile { .. } => {
            return bad_request(
                "No default embedding profile is configured. Set one in the Commonplace Book tab before running the backfill.",
            )
        }
        crate::services::memory_index::EnqueueMissingEmbeddingsResult::Ok {
            enqueued,
            remaining,
        } => (enqueued, remaining),
    };
    if enqueued == 0 && remaining == 0 {
        return Response::Memory(json!({
            "success": true,
            "enqueued": 0,
            "remaining": 0,
            "message": "No memories missing embeddings — nothing to backfill.",
        }));
    }
    tracing::info!(
        userId = user_id,
        characterId = character_id.as_deref().unwrap_or("(all)"),
        enqueued = enqueued,
        remaining = remaining,
        "[Memories API] Embedding backfill batch enqueued"
    );
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
/// (`app/api/v1/memories/route.ts:1135-1181`): Zod parse (`characterId` uuid
/// only — `af48f6cc2` dropped `batchSize`, which Zod now strips), character
/// ownership, default-profile gate, the all-embedded early return, else the
/// synchronous `generateMissingEmbeddings` sweep (`...result` spreads
/// `processed, failed, skipped`). The Zod `details` array is the standing
/// repo-wide deferral — the 400 body carries the middleware's `Validation
/// error` message alone.
///
/// The sweep still runs on the base's `memory_service` backfill (its old
/// per-call batch of 10); the re-point onto P4.D292's
/// `services/memory_index_backfill.rs` is §S.5 (3).
pub async fn memory_generate_embeddings<P: crate::model::embedding::EmbeddingProvider>(
    db: &Db,
    provider: &P,
    user_id: &str,
    character_id: &str,
    host_zone_fallback: &str,
    now_ms: f64,
) -> Response {
    if !super::chat_outfits::is_zod_uuid(character_id) {
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
    // §S.5 (3), landed at the `83334b9a5` unification: the route reads
    // P4.D292's `memory_index_backfill` pair directly (v4 `route.ts:1005`
    // calls the index backfill's `generateMissingEmbeddings`); the per-call
    // `batchSize` left with v4's `af48f6cc2`.
    let result = match crate::services::memory_index_backfill::generate_missing_embeddings(
        db,
        provider,
        character_id,
        user_id,
        None,
        host_zone_fallback,
        now_ms as i64,
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
    let result = match crate::services::memory_index_backfill::rebuild_vector_index(
        db,
        character_id,
    )
    .await
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

// === P4.D296: queue-memories (owned) ===

/// v4 `resolveExtractionProfileId(chat, ctx)` (`app/api/v1/chats/[id]/actions/
/// memories.ts:36-54`, pin `83334b9a5`) — the connection profile a chat's
/// extraction runs against, the same one the live pipeline uses: an LLM
/// seat's profile, else any seat's ([`pick_extraction_connection_profile_id`]).
/// A chat with no seat profile falls back to the user's configured cheap
/// profile — `defaultCheapProfileId`, else (`strategy === 'USER_DEFINED'`)
/// `userDefinedProfileId` — each only when that profile exists AND belongs to
/// the user. `None` when neither exists.
fn resolve_extraction_profile_id(
    db: &Db,
    user_id: &str,
    chat: &Value,
) -> Result<Option<String>, DbError> {
    if let Some(seat) = pick_extraction_connection_profile_id(chat) {
        return Ok(Some(seat));
    }
    let uid = user_id.to_string();
    db.read_main(move |conn| {
        let settings = crate::db::chat_settings::find_by_user_id(conn, &uid)?;
        let cheap = settings.as_ref().and_then(|s| s.get("cheapLLMSettings"));
        let str_of = |k: &str| {
            cheap
                .and_then(|c| c.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let user_defined = (str_of("strategy").as_deref() == Some("USER_DEFINED"))
            .then(|| str_of("userDefinedProfileId"))
            .flatten();
        for id in [str_of("defaultCheapProfileId"), user_defined]
            .into_iter()
            .flatten()
        {
            if id.is_empty() {
                continue;
            }
            let profile = crate::db::connection_profiles::find_by_id(conn, &id)?;
            if profile
                .as_ref()
                .and_then(|p| p.get("userId"))
                .and_then(Value::as_str)
                == Some(uid.as_str())
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    })
}

/// v4 `handleQueueMemories` (`actions/memories.ts:65-113`) — queue one
/// `MEMORY_EXTRACTION` job for every turn of a chat, as extraction addresses
/// them ([`list_extraction_turns`]: an ordinary chat's greeting turn, then
/// one per non-staff USER message; an autonomous room's one per closing
/// ASSISTANT line, each bounded by the previous anchor — bug 245). Every job
/// is a `backfill` (the rate limit does not apply to a rebuild of history).
///
/// Error arms (both 400, byte-matched): no connection profile (WARN `[Chats
/// v1] No connection profile for memory extraction {userId, chatId}`), and an
/// empty turn list with v4's branch-specific sentence. The legacy
/// `characterId` / `characterName` / `messagePairs` request fields are still
/// accepted by v4's schema and ignored by the handler (per-turn jobs cover
/// the whole turn), so v5's `ChatQueueMemories { chat_id }` is the whole live
/// surface.
pub async fn chat_queue_memories(db: &Db, user_id: &str, chat_id: &str) -> Response {
    // The chat itself (v4's route wrapper resolves it before dispatching).
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let connection_profile_id = match resolve_extraction_profile_id(db, user_id, &chat) {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!(
                userId = %user_id,
                chatId = %chat_id,
                "[Chats v1] No connection profile for memory extraction"
            );
            return bad_request(
                "No connection profile available for memory extraction. Give a seat a profile or set a cheap LLM profile in settings.",
            );
        }
        Err(e) => return internal(e),
    };

    let messages = match load_chat_message_events(db, chat_id) {
        Ok(m) => m,
        Err(e) => return internal(e),
    };
    let chat_type = chat.get("chatType").and_then(Value::as_str);
    let is_autonomous = chat_type == Some("autonomous");
    let turns = list_extraction_turns(&messages, chat_type);
    if turns.is_empty() {
        return bad_request(if is_autonomous {
            "No assistant messages found in this chat — nothing to extract memories from."
        } else {
            "No messages found in this chat — nothing to extract memories from."
        });
    }
    tracing::info!(
        chatId = %chat_id,
        chatType = %chat_type.unwrap_or("null"),
        turnCount = turns.len(),
        "[Chats v1] Queueing per-turn memory extraction jobs"
    );
    let entries: Vec<MemoryExtractionBatchEntry> = turns
        .iter()
        .map(|t| MemoryExtractionBatchEntry {
            turn_opener_message_id: t.turn_opener_message_id.clone(),
            extraction_anchor_message_id: t.extraction_anchor_message_id.clone(),
            extract_after_message_id: t.extract_after_message_id.clone(),
        })
        .collect();
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
        "turnCount": turns.len(),
    }))
}

use crate::services::extraction::context::pick_extraction_connection_profile_id;
use crate::services::turn_transcript::{list_extraction_turns, load_chat_message_events};

// === end P4.D296 ===

// === P4.D269: anchor-gate probe (append-only) ===

use super::zod_issues::{key as zod_key, zod_issue_details, zod_uuid_ok, ZodIssue};
use crate::services::anchor_gate_probe::{run_anchor_gate_probe_in_zone, RunAnchorGateProbeInput};

/// The issues of v4's `anchorGateProbeSchema` (`route.ts:585-588` —
/// `z.object({ characterId: z.string().uuid(), limit:
/// z.number().int().min(1).max(200).optional() })`), in schema order. The
/// wire types `characterId` as a `String` and `limit` as an `f64` (P4.D269
/// R-F), so a non-string id or a non-number limit is the dispatch layer's
/// refusal; everything Zod checks past the type is reproduced here.
fn anchor_gate_probe_issues(character_id: &str, limit: Option<f64>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    if !zod_uuid_ok(character_id) {
        issues.push(ZodIssue::invalid_uuid(vec![zod_key("characterId")]));
    }
    if let Some(n) = limit {
        let path = || vec![zod_key("limit")];
        if !(n.is_finite() && n.fract() == 0.0) {
            // `z.int()` on a non-integer: the issue ABORTS the number's later
            // checks (measured at the pin — `-0.5` carries this issue alone).
            let as_value = crate::db::js_number_to_json(n);
            issues.push(ZodIssue::invalid_int_type(path(), Some(&as_value)));
        } else {
            // An integer past the safe range keeps going (non-fatal) — `1e20`
            // carries the safe-int ceiling AND `.max(200)`.
            if n > MAX_SAFE_INTEGER {
                issues.push(ZodIssue::too_big_int(path()));
            }
            if n < -MAX_SAFE_INTEGER {
                issues.push(ZodIssue::too_small_int(path()));
            }
            if n < 1.0 {
                issues.push(ZodIssue::too_small_number(json!(1), path()));
            }
            if n > 200.0 {
                issues.push(ZodIssue::too_big_number(json!(200), path()));
            }
        }
    }
    issues
}

/// JS `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// v4 `POST /api/v1/memories?action=anchor-gate-probe` — `handleAnchorGateProbe`
/// (`route.ts:596-633`): the Zod gate (400 `Validation error` + `details`),
/// the character gate (404 `Character not found`), DEBUG `[Memories API]
/// Running anchor gate probe {characterId, limit}` (`limit` omitted when
/// absent), then `runAnchorGateProbe` with NO `embeddingProfileId` → the result
/// BARE; a failure logs ERROR `[Memories API] Anchor gate probe failed
/// {characterId, error}` and answers 500 `Anchor gate probe failed`.
pub async fn memory_anchor_gate_probe<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    user_id: &str,
    character_id: &str,
    limit: Option<f64>,
    host_zone_fallback: &str,
) -> Response {
    let issues = anchor_gate_probe_issues(character_id, limit);
    if !issues.is_empty() {
        return Response::validation_error(zod_issue_details(&issues));
    }
    match character_exists(db, character_id) {
        Ok(true) => {}
        Ok(false) => return not_found("Character"),
        Err(e) => return internal(e),
    }
    match limit {
        Some(n) => tracing::debug!(
            characterId = character_id,
            limit = %crate::db::js_number_to_json(n),
            "[Memories API] Running anchor gate probe"
        ),
        None => tracing::debug!(
            characterId = character_id,
            "[Memories API] Running anchor gate probe"
        ),
    }
    let input = RunAnchorGateProbeInput {
        character_id: character_id.to_string(),
        user_id: user_id.to_string(),
        limit,
        embedding_profile_id: None,
    };
    // The memory calendar's host fallback is the host's display-zone NAME
    // (P4.D290's HANDOFF to the route, landed at the `83334b9a5` unification).
    match run_anchor_gate_probe_in_zone(db, provider, &input, host_zone_fallback).await {
        Ok(result) => match serde_json::to_value(&result) {
            Ok(body) => Response::Memory(body),
            Err(e) => anchor_gate_probe_failed(character_id, &e.to_string()),
        },
        Err(e) => anchor_gate_probe_failed(character_id, &crate::db::fallback::error_text(&e)),
    }
}

/// v4's catch arm: the ERROR line (no error-object argument) + the 500.
fn anchor_gate_probe_failed(character_id: &str, error: &str) -> Response {
    tracing::error!(
        characterId = character_id,
        error = error,
        "[Memories API] Anchor gate probe failed"
    );
    Response::error(ErrorKind::Internal, "Anchor gate probe failed")
}

// === end P4.D269 ===

#[cfg(test)]
mod tests {
    use super::*;

    /// P4.D311 — zod 4.6.7's `capSize` on `entities`. The route answers its
    /// 400 WITHOUT `details` (the P4.D298 L-1 no-details class), so
    /// `memories_routes_equivalence` cannot see the issue list; these are v4's
    /// recorded `details` bytes from that family's oracle at the `25b8fab6a`
    /// pin (`create_entities_over_cap_non_string_first_400` /
    /// `create_entities_over_cap_empty_first_400`): the array's `too_big`
    /// ALONE, no element issue first.
    #[test]
    fn entities_past_the_cap_answer_only_the_arrays_too_big() {
        const V4_DETAILS: &str = r#"[{"origin":"array","code":"too_big","maximum":12,"inclusive":true,"path":["entities"],"message":"Too big: expected array to have <=12 items"}]"#;
        for first in [json!(7), json!("")] {
            let mut entities = vec![first];
            entities.extend((0..12).map(|i| json!(format!("entity-{i}"))));
            let bag = json!({
                "characterId": "b1000000-0000-4000-8000-000000000001",
                "content": "c",
                "summary": "s",
                "entities": entities,
            });
            let issues = match parse_create_bag(&bag) {
                Err(issues) => issues,
                Ok(_) => panic!("an over-cap entities list must refuse"),
            };
            assert_eq!(
                super::super::zod_issues::zod_issue_details(&issues).to_string(),
                V4_DETAILS
            );
        }
    }
    use crate::db::runtime::DbPaths;

    /// Throwaway pepper for a fresh encrypted instance (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT_A: &str = "c0000000-0000-4000-8000-00000000000a";
    const CHAT_EMPTY: &str = "c0000000-0000-4000-8000-00000000000b";
    const CHAR_A: &str = "a0000000-0000-4000-8000-00000000000a";

    /// A fully provisioned throwaway instance (v4's real `ensureCollection` DDL
    /// through `fresh_schema.json`) carrying two chats — one with two memories
    /// across two characters, one with none.
    fn provisioned() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        {
            let w = crate::db::Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
            let c = w.connection();
            for id in [CHAT_A, CHAT_EMPTY] {
                c.execute(
                    "INSERT INTO \"chats\" (\"id\", \"userId\", \"title\", \"createdAt\", \"updatedAt\") \
                     VALUES (?1, ?2, 'T', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![id, crate::services::provisioning::SINGLE_USER_ID],
                )
                .unwrap();
            }
            for (mid, char_id) in [
                ("m-1", CHAR_A),
                ("m-2", "b0000000-0000-4000-8000-00000000000b"),
            ] {
                // AUTO, as extraction writes them: the chat wipe spares MANUAL
                // rows (bug 201, P4.D288) — the column's default.
                c.execute(
                    "INSERT INTO \"memories\" (\"id\", \"characterId\", \"chatId\", \"content\", \
                     \"summary\", \"source\", \"createdAt\", \"updatedAt\") \
                     VALUES (?1, ?2, ?3, 'c', 's', 'AUTO', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![mid, char_id, CHAT_A],
                )
                .unwrap();
            }
        }
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    /// Install a capturing layer on this thread (thread-scoped — parallel tests
    /// cannot steal it) and hand back the lines. The guard is held across the
    /// `await`, which is why this is not `test_support::captured_with`: that one
    /// takes a synchronous closure.
    fn capture_guard() -> (
        tracing::subscriber::DefaultGuard,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sub =
            tracing_subscriber::registry().with(crate::test_support::CaptureLayer(logs.clone()));
        (tracing::subscriber::set_default(sub), logs)
    }

    /// v4 `app/api/v1/memories/route.ts:1261` (bug 201's rename). Log-only: the
    /// response body and every `memories` row are identical whether or not this
    /// fires, so the only proof is the capturing layer over the real handler.
    #[tokio::test]
    async fn delete_by_chat_logs_v4s_debug_on_both_arms() {
        let (_dir, db) = provisioned();
        for (chat_id, expect_deleted) in [(CHAT_A, 2i64), (CHAT_EMPTY, 0i64)] {
            let (guard, logs) = capture_guard();
            let resp = memory_delete_by_chat(&db, chat_id).await;
            drop(guard);
            let lines = logs.lock().unwrap().clone();
            assert!(matches!(resp, Response::Memory(_)), "{resp:?}");
            let hits: Vec<&String> = lines
                .iter()
                .filter(|l| l.contains("[Memories API] Wiped a chat's extracted memories"))
                .collect();
            assert_eq!(hits.len(), 1, "one line per call: {lines:?}");
            let line = hits[0];
            assert!(line.starts_with("DEBUG "), "v4 logs at debug: {line:?}");
            assert!(line.contains(&format!(" chatId={chat_id}")), "{line:?}");
            assert!(
                line.contains(&format!(" deleted={expect_deleted}")),
                "the EMPTY chat still narrates deleted=0 — v4 logs after the \
                 call, not inside a deleted-something branch: {line:?}"
            );
            assert!(line.ends_with(" spared=0"), "{line:?}");
        }
    }

    /// The ownership gate returns BEFORE the delete, so a missing chat narrates
    /// nothing (v4's `notFound('Chat')` precedes its logger call).
    #[tokio::test]
    async fn delete_by_chat_is_silent_for_a_missing_chat() {
        let (_dir, db) = provisioned();
        let (guard, logs) = capture_guard();
        let resp = memory_delete_by_chat(&db, "c0000000-0000-4000-8000-0000000000ff").await;
        drop(guard);
        let lines = logs.lock().unwrap().clone();
        assert!(
            matches!(&resp, Response::Error(e) if e.kind == ErrorKind::NotFound),
            "{resp:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[Memories API] Wiped a chat")),
            "{lines:?}"
        );
    }

    // === P4.D269: anchor-gate probe handler pins (append-only) ===
    struct OneDim;
    impl crate::model::embedding::EmbeddingProvider for OneDim {
        async fn generate_embedding_for_user(
            &self,
            _text: &str,
            _user_id: &str,
            _profile_id: Option<&str>,
            _opts: crate::model::embedding::EmbeddingCallOptions,
        ) -> Result<crate::model::embedding::EmbeddingResult, crate::model::embedding::EmbeddingError>
        {
            Ok(crate::model::embedding::EmbeddingResult {
                embedding: vec![1.0],
                model: "m".into(),
                dimensions: 1,
                provider: "SCRIPTED".into(),
            })
        }
    }

    /// `provisioned()` plus the character the probe's gate looks up.
    fn provisioned_with_character() -> (tempfile::TempDir, Db) {
        let (dir, db) = provisioned();
        let main = dir.path().join("data/quilltap.db");
        let w = crate::db::Writer::open_writable(&main, PEPPER).unwrap();
        w.connection()
            .execute(
                "INSERT INTO \"characters\" (\"id\", \"userId\", \"name\", \"createdAt\", \"updatedAt\") \
                 VALUES (?1, ?2, 'A', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                rusqlite::params![CHAR_A, crate::services::provisioning::SINGLE_USER_ID],
            )
            .unwrap();
        (dir, db)
    }

    fn probe_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains("[Memories API]"))
            .cloned()
            .collect()
    }

    /// v4's DEBUG `{characterId, limit}` — `limit` OMITTED when absent (§R.5).
    #[tokio::test]
    async fn anchor_probe_logs_v4s_debug_with_and_without_limit() {
        let (_dir, db) = provisioned_with_character();
        for (limit, tail) in [(Some(4.0), " limit=4"), (None, "")] {
            let (guard, logs) = capture_guard();
            let resp = memory_anchor_gate_probe(&db, &OneDim, "u", CHAR_A, limit, "UTC").await;
            drop(guard);
            assert!(matches!(resp, Response::Memory(_)), "{resp:?}");
            assert_eq!(
                probe_lines(&logs.lock().unwrap()),
                [format!(
                    "DEBUG quilltap_core::api::memories [Memories API] Running anchor gate probe characterId={CHAR_A}{tail}"
                )]
            );
        }
    }

    /// The Zod gate and the character gate both answer BEFORE the debug line.
    #[tokio::test]
    async fn anchor_probe_refusals_are_silent() {
        let (_dir, db) = provisioned_with_character();
        for (id, limit, status) in [
            ("nope", None, ErrorKind::BadRequest),
            (CHAR_A, Some(1.5), ErrorKind::BadRequest),
            (
                "a0000000-0000-4000-8000-0000000000ff",
                None,
                ErrorKind::NotFound,
            ),
        ] {
            let (guard, logs) = capture_guard();
            let resp = memory_anchor_gate_probe(&db, &OneDim, "u", id, limit, "UTC").await;
            drop(guard);
            assert!(
                matches!(&resp, Response::Error(e) if e.kind == status),
                "{resp:?}"
            );
            assert!(probe_lines(&logs.lock().unwrap()).is_empty());
        }
    }

    /// The catch arm: v4's ERROR line and 500 `Anchor gate probe failed`.
    /// P4.D288 (C1-3): the index load became INFALLIBLE (v4's two fallbacks),
    /// so the store-load failure this test once planted (`DROP TABLE
    /// vector_entries`) no longer reaches the arm — v5 now answers 200 there,
    /// as v4 does (the harness's `PROBE_STORE_LOAD_FAILURE` row closes —
    /// P4.D290's to retire). The arm stays reachable through a read-pool
    /// failure; it is pinned directly.
    #[tokio::test]
    async fn anchor_probe_store_failure_logs_v4s_error_and_answers_500() {
        let (guard, logs) = capture_guard();
        let resp = anchor_gate_probe_failed(CHAR_A, "no such table: vector_entries");
        drop(guard);
        assert!(
            matches!(&resp, Response::Error(e) if e.kind == ErrorKind::Internal && e.message == "Anchor gate probe failed"),
            "{resp:?}"
        );
        let lines = probe_lines(&logs.lock().unwrap());
        assert_eq!(
            lines.last().unwrap(),
            &format!(
                "ERROR quilltap_core::api::memories [Memories API] Anchor gate probe failed characterId={CHAR_A} error=no such table: vector_entries"
            )
        );
    }

    /// The probe's Zod issues in schema order, measured at the pin.
    #[test]
    fn anchor_probe_issues_follow_zod_4() {
        let render = |id: &str, limit: Option<f64>| {
            serde_json::to_string(&zod_issue_details(&anchor_gate_probe_issues(id, limit))).unwrap()
        };
        assert_eq!(render(CHAR_A, Some(200.0)), "[]");
        // A non-integer ABORTS the bounds (`-0.5` carries one issue).
        assert_eq!(
            render(CHAR_A, Some(-0.5)),
            r#"[{"expected":"int","format":"safeint","code":"invalid_type","path":["limit"],"message":"Invalid input: expected int, received number"}]"#
        );
        // An unsafe integer keeps going.
        assert!(render(CHAR_A, Some(1e20)).contains(r#""origin":"int""#));
        assert!(render(CHAR_A, Some(1e20)).contains(r#""maximum":200"#));
        let both = render("nope", Some(0.0));
        assert!(both.find("invalid_format").unwrap() < both.find("too_small").unwrap());
    }
    // === end ===
}
