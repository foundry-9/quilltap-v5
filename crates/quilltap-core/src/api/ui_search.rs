//! The `uiSearch` dispatch surface (P4.9P) — the global SearchBar/SearchDialog's
//! one endpoint. A differential port of v4's `app/api/v1/ui/search/route.ts`
//! (GET only, authenticated), composed over the existing repo reads:
//! `characters_read::find_by_user_id` (overlaid), `chats_read::find_by_user_id`,
//! `ChatSearchRepository::search_messages_global`,
//! `memories_read::search_by_content`, and `tags::find_all` (filtered to the
//! user in-handler — v4's `repos.tags.findByUserId` is the base-repo
//! `findByFilter({userId})`, same rows, same rowid order).
//!
//! ## The v4 quirks this surface carries (all pinned by `ui_search_equivalence`)
//!
//! 1. **`characterNames` is broken-but-exact.** v4's `getCharacterNamesForChat`
//!    matches `characters.find((c) => c.id === participant.id)` — the
//!    PARTICIPANT row id, not `participant.characterId`. Participant ids are
//!    minted UUIDs distinct from character ids, so the list is empty for every
//!    normally-created chat. The differential seeds one participant whose `id`
//!    EQUALS its characterId to pin the exact wrong comparison in both
//!    directions.
//! 2. **A garbage `limit` empties the page.** `parseInt('abc')` is NaN;
//!    `Math.min(NaN, 50)` is NaN; `slice(offset, offset + NaN)` has end
//!    `ToIntegerOrInfinity(NaN) = 0`, so the page is empty while `hasMore`
//!    stays `offset + 0 < totalCount`. A garbage `offset` NaN-poisons the
//!    `hasMore` compare instead (`NaN < totalCount` is false).
//! 3. **JS falsy fallbacks.** `importance || 5` turns an importance of 0 into
//!    5; `title || null` turns an empty-string title into null;
//!    `chat.title || 'Untitled Chat'` renames an empty title.
//! 4. **Snippet indices come from the LOWERCASED strings** but slice the
//!    ORIGINAL content — faithful to v4, including the (non-BMP/locale) index
//!    skew that implies.
//!
//! Sort: stable by `matchPriority` asc, then `new Date(updatedAt)` desc (an
//! unparseable date is NaN → the comparator's arithmetic is NaN → V8 treats it
//! as equal; `js_date_parse_ms` returning `None` maps to `Ordering::Equal`).
//! Ties keep insertion order: characters → chats → messages → memories → tags.
//! `countsByType` is computed AFTER the sort and BEFORE pagination, so its key
//! order is first-encounter order in the sorted results (v4 object-literal
//! insertion semantics; `serde_json`'s `preserve_order` keeps it).

use serde_json::{json, Map, Value};

use crate::clock::now_iso;
use crate::content_disposition::encode_uri_component;
use crate::db::chats_search::ChatSearchRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, memories_read, tags, DbError};
use crate::episodic::js_date_parse_ms;
use crate::jsstr::{js_index_of, js_trim, utf16_len, utf16_truncate};

use super::llm_logs::{js_min, js_parse_int_10};
use super::types::{ErrorKind, Response};

/// v4 `VALID_TYPES` — exactly these five, this order (the default fan-out order
/// is NOT this array's order; the search sections run characters → chats →
/// messages → memories → tags regardless).
const VALID_TYPES: [&str; 5] = ["chats", "characters", "tags", "memories", "messages"];

/// The route's query params as the transport hands them over — all RAW strings,
/// because v4 parses them inside the handler and the NaN quirks must survive.
#[derive(Debug, Clone, Default)]
pub struct UiSearchParams<'a> {
    pub q: Option<&'a str>,
    pub types: Option<&'a str>,
    pub limit: Option<&'a str>,
    pub offset: Option<&'a str>,
}

fn bad_request(msg: &str) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// JS `Math.max(a, b)` — NaN-propagating (Rust's `f64::max` returns the non-NaN
/// operand, which would quietly repair v4's garbage-offset quirk).
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a > b {
        a
    } else {
        b
    }
}

/// JS `s.substring(start, end)` over UTF-16 code units (both bounds clamped to
/// `[0, len]`; swapped when start > end — `String.prototype.substring`
/// semantics, which v4's `createSnippet` relies on only in the clamped range).
fn utf16_substring(s: &str, start: usize, end: usize) -> String {
    let (lo, hi) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let units: Vec<u16> = s.encode_utf16().skip(lo).take(hi - lo).collect();
    String::from_utf16_lossy(&units)
}

/// v4 `getMatchPriority` — 0 exact, 1 contains, 2 otherwise.
fn get_match_priority(value: &str, query: &str) -> u8 {
    let lower_value = value.to_lowercase();
    let lower_query = query.to_lowercase();
    if lower_value == lower_query {
        return 0;
    }
    if lower_value.contains(&lower_query) {
        return 1;
    }
    2
}

/// v4 `createSnippet` — center a window around the first (lowercased) match.
/// The match index is found in the LOWERCASED strings but slices the ORIGINAL
/// content, exactly as v4 does.
fn create_snippet(content: &str, query: &str, max_length: usize) -> String {
    if content.is_empty() {
        return String::new();
    }
    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();
    let content_len = utf16_len(content);

    let Some(match_index) = js_index_of(&lower_content, &lower_query, 0) else {
        let mut s = utf16_truncate(content, max_length);
        if content_len > max_length {
            s.push_str("...");
        }
        return s;
    };

    let start = match_index.saturating_sub(30);
    let end = (match_index + utf16_len(query) + 70).min(content_len);
    let mut snippet = utf16_substring(content, start, end);
    if start > 0 {
        snippet = format!("...{snippet}");
    }
    if end < content_len {
        snippet.push_str("...");
    }
    snippet
}

/// JS `Array.prototype.slice(start, end)` with `ToIntegerOrInfinity` bounds
/// (NaN → 0 — the garbage-limit arm rides on this).
fn js_slice<T>(v: Vec<T>, start: f64, end: f64) -> Vec<T> {
    let len = v.len() as f64;
    let norm = |x: f64| -> f64 {
        let xi = if x.is_nan() { 0.0 } else { x.trunc() };
        if xi < 0.0 {
            (len + xi).max(0.0)
        } else {
            xi.min(len)
        }
    };
    let k = norm(start) as usize;
    let fin = norm(end) as usize;
    v.into_iter().skip(k).take(fin.saturating_sub(k)).collect()
}

/// `field || fallback` for a string-typed JSON field: absent, null, and the
/// empty string are all JS-falsy.
fn str_or<'v>(row: &'v Value, key: &str, fallback: &'v str) -> &'v str {
    match row.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => fallback,
    }
}

/// `row.createdAt || new Date().toISOString()` — the fixture rows always carry
/// timestamps, so the live-clock arm never reaches the differential.
fn ts_or_now(row: &Value, key: &str) -> String {
    match row.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => now_iso(),
    }
}

/// One pre-sort result: the emitted body plus the two sort keys v4 reads back
/// off the object (`matchPriority`, `updatedAt`).
struct SearchHit {
    kind: &'static str,
    priority: u8,
    updated_at: String,
    body: Value,
}

/// v4 `GET /api/v1/ui/search` (`route.ts:71-311`) — the whole handler.
pub fn ui_search(db: &Db, user_id: &str, p: &UiSearchParams<'_>) -> Response {
    // `limitParam ? Math.min(parseInt(limitParam, 10), 50) : 20` — an empty
    // string param is JS-falsy and takes the default.
    let limit = match p.limit.filter(|s| !s.is_empty()) {
        Some(s) => js_min(js_parse_int_10(s), 50.0),
        None => 20.0,
    };
    // `offsetParam ? Math.max(parseInt(offsetParam, 10), 0) : 0`.
    let offset = match p.offset.filter(|s| !s.is_empty()) {
        Some(s) => js_max(js_parse_int_10(s), 0.0),
        None => 0.0,
    };

    // `typesParam.split(',').filter(VALID_TYPES.includes)`; only a NON-EMPTY
    // parse overrides the all-types default.
    let requested: Vec<&str> = match p.types.filter(|s| !s.is_empty()) {
        Some(ts) => {
            let parsed: Vec<&str> = ts.split(',').filter(|t| VALID_TYPES.contains(t)).collect();
            if parsed.is_empty() {
                VALID_TYPES.to_vec()
            } else {
                parsed
            }
        }
        None => VALID_TYPES.to_vec(),
    };

    // `searchParams.get('q')?.trim()` then the two validation arms.
    let query = p.q.map(js_trim);
    let query = match query {
        Some(q) if !q.is_empty() => q,
        _ => return bad_request("Search query required"),
    };
    if utf16_len(query) < 2 {
        return bad_request("Search query must be at least 2 characters");
    }

    match search_body(db, user_id, query, &requested, limit, offset) {
        Ok(v) => Response::UiSearch(v),
        // v4's catch-all: log and answer the one opaque serverError.
        Err(e) => {
            tracing::error!(
                target: "quilltap::ui_search",
                error = %e,
                "Error during search",
            );
            Response::error(ErrorKind::Internal, "Search failed")
        }
    }
}

fn search_body(
    db: &Db,
    user_id: &str,
    query: &str,
    requested: &[&str],
    limit: f64,
    offset: f64,
) -> Result<Value, DbError> {
    let want = |t: &str| requested.contains(&t);
    let lower_query = query.to_lowercase();

    let uid = user_id.to_string();
    let q = query.to_string();
    let req: Vec<String> = requested.iter().map(|s| s.to_string()).collect();
    let (characters, chats, message_matches, memories_by_char, tag_rows) =
        db.read_main(move |main| {
            db.read_mount_index(move |mount| {
                let want = |t: &str| req.iter().any(|r| r == t);
                // Characters are fetched EITHER WAY — the else-branch comment in
                // v4: "Still need characters for chat lookups" (and the memories
                // loop iterates them).
                let characters = characters_read::find_by_user_id(main, mount, &uid)?;
                // `needChats` — chat title search or message content search.
                let chats = if want("chats") || want("messages") {
                    chats_read::find_by_user_id(main, &uid)?
                } else {
                    Vec::new()
                };
                let message_matches = if want("messages") {
                    let chat_ids: Vec<String> = chats
                        .iter()
                        .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
                        .collect();
                    ChatSearchRepository::new(main).search_messages_global(&chat_ids, &q, 100)?
                } else {
                    Vec::new()
                };
                let memories_by_char: Vec<(usize, Vec<Value>)> = if want("memories") {
                    let mut out = Vec::new();
                    for (idx, character) in characters.iter().enumerate() {
                        let cid = character.get("id").and_then(Value::as_str).unwrap_or("");
                        out.push((idx, memories_read::search_by_content(main, cid, &q)?));
                    }
                    out
                } else {
                    Vec::new()
                };
                // v4 `repos.tags.findByUserId(user.id)` — the base-repo
                // `findByFilter({userId})`: same rows + rowid order as
                // `find_all` filtered here.
                let tag_rows = if want("tags") {
                    tags::find_all(main)?
                        .into_iter()
                        .filter(|t| t.get("userId").and_then(Value::as_str) == Some(uid.as_str()))
                        .collect()
                } else {
                    Vec::new()
                };
                Ok((
                    characters,
                    chats,
                    message_matches,
                    memories_by_char,
                    tag_rows,
                ))
            })
        })?;

    let mut all_results: Vec<SearchHit> = Vec::new();
    // `typesFound` is a Set — insertion-ordered, first-add wins.
    let mut types_found: Vec<&'static str> = Vec::new();
    let found = |t: &'static str, types_found: &mut Vec<&'static str>| {
        if !types_found.contains(&t) {
            types_found.push(t);
        }
    };

    // ── Characters ──────────────────────────────────────────────────────────
    if want("characters") {
        for char_row in &characters {
            let name = char_row.get("name").and_then(Value::as_str).unwrap_or("");
            let description = char_row.get("description").and_then(Value::as_str);
            let name_match = name.to_lowercase().contains(&lower_query);
            let desc_match = description
                .map(|d| d.to_lowercase().contains(&lower_query))
                .unwrap_or(false);
            if !name_match && !desc_match {
                continue;
            }
            let matched_field = if name_match { "name" } else { "description" };
            let matched_value = if name_match {
                name
            } else {
                description.unwrap_or("")
            };
            let updated_at = ts_or_now(char_row, "updatedAt");
            // `char.title || null` — an empty-string title is falsy → null.
            let title = match char_row.get("title").and_then(Value::as_str) {
                Some(t) if !t.is_empty() => Value::String(t.to_string()),
                _ => Value::Null,
            };
            let priority = get_match_priority(matched_value, query);
            all_results.push(SearchHit {
                kind: "characters",
                priority,
                updated_at: updated_at.clone(),
                body: json!({
                    "id": char_row.get("id").cloned().unwrap_or(Value::Null),
                    "type": "characters",
                    "name": name,
                    "matchedField": matched_field,
                    "matchedValue": utf16_truncate(matched_value, 200),
                    "snippet": create_snippet(matched_value, query, 100),
                    "url": format!("/aurora/{}", char_row.get("id").and_then(Value::as_str).unwrap_or("")),
                    "matchPriority": priority,
                    "createdAt": ts_or_now(char_row, "createdAt"),
                    "updatedAt": updated_at,
                    "title": title,
                    "isFavorite": char_row.get("isFavorite").and_then(Value::as_bool).unwrap_or(false),
                }),
            });
            found("characters", &mut types_found);
        }
    }

    // The chat-id → row map + the (broken-but-exact) character-name helper.
    let character_names_for_chat = |chat: &Value| -> Vec<String> {
        let mut names = Vec::new();
        let participants = chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for participant in participants
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
        {
            // v4: `characters.find((c) => c.id === participant.id)` — the
            // PARTICIPANT row id, not characterId (quirk 1 in the header).
            let pid = participant.get("id").and_then(Value::as_str);
            if let Some(ch) = characters
                .iter()
                .find(|c| c.get("id").and_then(Value::as_str) == pid)
            {
                if let Some(n) = ch.get("name").and_then(Value::as_str) {
                    names.push(n.to_string());
                }
            }
        }
        names
    };

    // ── Chats ───────────────────────────────────────────────────────────────
    if want("chats") {
        for chat in &chats {
            // `c.title?.toLowerCase().includes(...)` — a null title never matches.
            let Some(title) = chat.get("title").and_then(Value::as_str) else {
                continue;
            };
            if !title.to_lowercase().contains(&lower_query) {
                continue;
            }
            let updated_at = ts_or_now(chat, "updatedAt");
            let priority = get_match_priority(title, query);
            all_results.push(SearchHit {
                kind: "chats",
                priority,
                updated_at: updated_at.clone(),
                body: json!({
                    "id": chat.get("id").cloned().unwrap_or(Value::Null),
                    "type": "chats",
                    "name": str_or(chat, "title", "Untitled Chat"),
                    "matchedField": "title",
                    "matchedValue": title,
                    "snippet": create_snippet(title, query, 100),
                    "url": format!("/salon/{}", chat.get("id").and_then(Value::as_str).unwrap_or("")),
                    "matchPriority": priority,
                    "createdAt": ts_or_now(chat, "createdAt"),
                    "updatedAt": updated_at,
                    "characterNames": character_names_for_chat(chat),
                    "messageCount": chat.get("messageCount").cloned().unwrap_or(json!(0)),
                }),
            });
            found("chats", &mut types_found);
        }
    }

    // ── Messages ────────────────────────────────────────────────────────────
    if want("messages") {
        for msg in &message_matches {
            let chat_id = msg.get("chatId").and_then(Value::as_str).unwrap_or("");
            let chat = chats
                .iter()
                .find(|c| c.get("id").and_then(Value::as_str) == Some(chat_id));
            let chat_title = chat
                .map(|c| str_or(c, "title", "Untitled Chat").to_string())
                .unwrap_or_else(|| "Untitled Chat".to_string());
            let character_names = chat.map(character_names_for_chat).unwrap_or_default();
            let content = msg.get("content").and_then(Value::as_str).unwrap_or("");
            let message_id = msg.get("messageId").and_then(Value::as_str).unwrap_or("");
            // BOTH timestamps come from the message's createdAt (v4 has no
            // per-message updatedAt).
            let stamp = ts_or_now(msg, "createdAt");
            let priority = get_match_priority(content, query);
            all_results.push(SearchHit {
                kind: "messages",
                priority,
                updated_at: stamp.clone(),
                body: json!({
                    "id": format!("msg-{message_id}"),
                    "type": "messages",
                    "name": chat_title,
                    "matchedField": "content",
                    "matchedValue": utf16_truncate(content, 200),
                    "snippet": create_snippet(content, query, 120),
                    "url": format!("/salon/{chat_id}?msg={message_id}"),
                    "matchPriority": priority,
                    "createdAt": stamp,
                    "updatedAt": stamp,
                    "chatId": chat_id,
                    "chatTitle": chat_title,
                    "characterNames": character_names,
                    "role": msg.get("role").cloned().unwrap_or(Value::Null),
                    "messageId": message_id,
                }),
            });
            found("messages", &mut types_found);
        }
    }

    // ── Memories ────────────────────────────────────────────────────────────
    for (char_idx, character_memories) in &memories_by_char {
        let character = &characters[*char_idx];
        let character_name = character.get("name").and_then(Value::as_str).unwrap_or("");
        let character_id = character.get("id").and_then(Value::as_str).unwrap_or("");
        for memory in character_memories {
            let summary = memory.get("summary").and_then(Value::as_str).unwrap_or("");
            let summary_len = utf16_len(summary);
            let mut name = utf16_truncate(summary, 50);
            if summary_len > 50 {
                name.push_str("...");
            }
            let updated_at = ts_or_now(memory, "updatedAt");
            let priority = get_match_priority(summary, query);
            // `memory.importance || 5` — an importance of 0 is falsy (quirk 3).
            let importance = match memory.get("importance").and_then(Value::as_f64) {
                Some(f) if f != 0.0 => crate::db::js_number_to_json(f),
                _ => json!(5),
            };
            all_results.push(SearchHit {
                kind: "memories",
                priority,
                updated_at: updated_at.clone(),
                body: json!({
                    "id": memory.get("id").cloned().unwrap_or(Value::Null),
                    "type": "memories",
                    "name": name,
                    "matchedField": "summary",
                    "matchedValue": summary,
                    "snippet": create_snippet(summary, query, 100),
                    "url": format!("/aurora/{character_id}?tab=memories"),
                    "matchPriority": priority,
                    "createdAt": ts_or_now(memory, "createdAt"),
                    "updatedAt": updated_at,
                    "characterId": memory.get("characterId").cloned().unwrap_or(Value::Null),
                    "characterName": character_name,
                    "importance": importance,
                    "source": str_or(memory, "source", "AUTO"),
                }),
            });
            found("memories", &mut types_found);
        }
    }

    // ── Tags ────────────────────────────────────────────────────────────────
    if want("tags") {
        for tag in &tag_rows {
            let name = tag.get("name").and_then(Value::as_str).unwrap_or("");
            if !name.to_lowercase().contains(&lower_query) {
                continue;
            }
            let updated_at = ts_or_now(tag, "updatedAt");
            let priority = get_match_priority(name, query);
            all_results.push(SearchHit {
                kind: "tags",
                priority,
                updated_at: updated_at.clone(),
                body: json!({
                    "id": tag.get("id").cloned().unwrap_or(Value::Null),
                    "type": "tags",
                    "name": name,
                    "matchedField": "name",
                    "matchedValue": name,
                    // v4 uses the bare name — no createSnippet call here.
                    "snippet": name,
                    "url": format!("/gallery?tag={}", encode_uri_component(name)),
                    "matchPriority": priority,
                    "createdAt": ts_or_now(tag, "createdAt"),
                    "updatedAt": updated_at,
                    // "Tag usage count not tracked in current schema" — v4's
                    // literal 0.
                    "usageCount": 0,
                    "quickHide": tag.get("quickHide").and_then(Value::as_bool).unwrap_or(false),
                }),
            });
            found("tags", &mut types_found);
        }
    }

    // Stable sort: matchPriority asc, then `new Date(updatedAt)` desc. An
    // unparseable date makes v4's comparator NaN, which V8 treats as equal.
    all_results.sort_by(|a, b| {
        a.priority.cmp(&b.priority).then_with(|| {
            match (
                js_date_parse_ms(&a.updated_at),
                js_date_parse_ms(&b.updated_at),
            ) {
                (Some(ta), Some(tb)) => tb.cmp(&ta),
                _ => std::cmp::Ordering::Equal,
            }
        })
    });

    // countsByType BEFORE pagination — key order is first-encounter order in
    // the SORTED results (v4 object insertion semantics).
    let mut counts_by_type = Map::new();
    for hit in &all_results {
        let n = counts_by_type
            .get(hit.kind)
            .and_then(Value::as_i64)
            .unwrap_or(0);
        counts_by_type.insert(hit.kind.to_string(), json!(n + 1));
    }

    let total_count = all_results.len();
    let paginated: Vec<Value> = js_slice(all_results, offset, offset + limit)
        .into_iter()
        .map(|h| h.body)
        .collect();
    // `offset + paginatedResults.length < totalCount` — NaN-poisoned by a
    // garbage offset (quirk 2).
    let has_more = offset + (paginated.len() as f64) < total_count as f64;

    Ok(json!({
        "results": paginated,
        "totalCount": total_count,
        "query": query,
        "types": types_found,
        "hasMore": has_more,
        "countsByType": Value::Object(counts_by_type),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_centers_and_ellipsizes_like_v4() {
        // Match at the head: no leading ellipsis, trailing one when cut.
        let long_tail = format!("zeppelin {}", "x".repeat(200));
        let s = create_snippet(&long_tail, "zeppelin", 100);
        assert!(s.starts_with("zeppelin"));
        assert!(s.ends_with("..."));
        // Match deep in the string: leading ellipsis, window is
        // [match-30, match+len+70).
        let deep = format!("{}zeppelin{}", "a".repeat(100), "b".repeat(100));
        let s = create_snippet(&deep, "zeppelin", 100);
        assert!(s.starts_with("..."));
        assert!(s.ends_with("..."));
        assert!(s.contains("zeppelin"));
        assert_eq!(utf16_len(&s) - 6, 30 + 8 + 70);
        // No match (createSnippet's -1 branch): plain head truncation.
        let s = create_snippet("short body", "absent", 100);
        assert_eq!(s, "short body");
    }

    #[test]
    fn js_slice_matches_array_prototype_slice() {
        let v: Vec<i32> = (0..10).collect();
        assert_eq!(js_slice(v.clone(), 0.0, 20.0), (0..10).collect::<Vec<_>>());
        assert_eq!(js_slice(v.clone(), 5.0, 8.0), vec![5, 6, 7]);
        // NaN end (the garbage-limit arm) → empty.
        assert_eq!(js_slice(v.clone(), 2.0, f64::NAN), Vec::<i32>::new());
        // NaN start (the garbage-offset arm) → 0.
        assert_eq!(js_slice(v.clone(), f64::NAN, 3.0), vec![0, 1, 2]);
        // Negative end counts from the tail.
        assert_eq!(js_slice(v, 0.0, -5.0), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn match_priority_tiers() {
        assert_eq!(get_match_priority("Airship", "airship"), 0);
        assert_eq!(get_match_priority("The Airship Log", "airship"), 1);
        assert_eq!(get_match_priority("unrelated", "airship"), 2);
    }

    #[test]
    fn js_max_propagates_nan() {
        assert_eq!(js_max(5.0, 0.0), 5.0);
        assert_eq!(js_max(-5.0, 0.0), 0.0);
        assert!(js_max(f64::NAN, 0.0).is_nan());
        // The divergence the helper exists to prevent.
        assert_eq!(f64::NAN.max(0.0), 0.0);
    }
}
