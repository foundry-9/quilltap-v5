//! The memories **read** path — slim-row marshaling + every `findBy*` / `count*`
//! query of v4's `MemoriesRepository`
//! (`lib/database/repositories/memories.repository.ts`). The write/mutation
//! surface lives in [`super::memories`].
//!
//! `memories` is a plain `AbstractBaseRepository<Memory>` in the **main** db with
//! **no overrides** except `getCollection` (which registers the `embedding` BLOB
//! column). There is no vault overlay, so — unlike `characters` — every read is a
//! single-connection SELECT + marshal; no second db, no overlay merge.
//!
//! ## Read marshaling (v4 `hydrateRow` + `MemorySchema` parse)
//!
//! v4 reads a row, `hydrateRow` parses the JSON-array columns and (for `is*`
//! columns, of which memory has none) coerces booleans, then Zod validates.
//! The net JSON shape (what `JSON.stringify` emits):
//!   - **nullable-optional** columns absent (`undefined`, dropped by stringify)
//!     when the cell is SQL NULL: `aboutCharacterId`, `chatId`, `projectId`,
//!     `embedding`, `witnessedContext`, `sourceMessageId`, `lastAccessedAt`,
//!     `lastReinforcedAt`;
//!   - the three JSON-array columns parsed to arrays: `keywords`, `tags`,
//!     `relatedMemoryIds` (always written `[]` on create, never NULL);
//!   - the three numeric columns rendered the JS way via
//!     [`super::js_number_to_json`] (an integer-valued REAL like `1.0` →
//!     `1`): `importance`, `reinforcementCount`, `reinforcedImportance`;
//!   - `embedding`, when present, is a `Float32Array` → `JSON.stringify` emits an
//!     **object** with stringified integer indices (`{"0":0.5,"1":-0.25,…}`),
//!     reproduced here from the little-endian Float32 BLOB.
//!
//! Field defaults (`keywords`/`tags`/`relatedMemoryIds` → `[]`, `importance`/
//! `reinforcedImportance` → `0.5`, `reinforcementCount` → `1`, `source` →
//! `MANUAL`) are all `.default()` columns the create path always materializes, so
//! they sit non-NULL in the row and need no read-side synthesis (unlike the
//! `characters` managed columns).
//!
//! ## The `$regex` → SQL `LIKE` seam
//!
//! v4's text searches build a `RegExp` from `escapeRegex(query)` and pass it as a
//! `{ $regex }` filter; the query translator lowers it to `LIKE` by mangling the
//! **regex source** (NOT the original query): `source.replace(/\.\*/g,'%')
//! .replace(/\./g,'_')`, wrapped `%…%`. [`like_pattern`] reproduces that exact
//! transformation byte-for-byte, so the LIKE pattern handed to SQLite is
//! identical on both sides (and SQLite, the same engine, matches identically).
//! ASCII LIKE is case-insensitive by default, matching the regex `'i'` flag for
//! the ASCII corpus.

use rusqlite::types::ToSql;
use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::js_number_to_json;
use super::DbError;
use crate::embedding_blob::blob_to_float32;

/// All `memories` columns in schema order (indices 0..=20 in [`marshal_row`]).
const COLS: &str = "id, characterId, aboutCharacterId, chatId, projectId, content, summary, \
     keywords, tags, importance, embedding, source, witnessedContext, sourceMessageId, \
     lastAccessedAt, reinforcementCount, lastReinforcedAt, relatedMemoryIds, reinforcedImportance, \
     createdAt, updatedAt";

/// Insert a required (always-present) string column.
fn put_req_str(m: &mut Map<String, Value>, key: &str, v: String) {
    m.insert(key.to_string(), Value::String(v));
}

/// Insert a nullable-optional string column. In the normal `findByFilter` path
/// (`keep_nulls = false`) a NULL is **omitted** (v4's `undefined` dropped by
/// `JSON.stringify`). In the raw-SQL `findByCharacterAboutCharacters` path
/// (`keep_nulls = true`) the rawQuery row carries an explicit `null` which
/// `MemorySchema.safeParse` keeps for a `.nullable()` field, so we emit `null`.
fn put_nullable_str(m: &mut Map<String, Value>, key: &str, v: Option<String>, keep_nulls: bool) {
    match v {
        Some(s) => {
            m.insert(key.to_string(), Value::String(s));
        }
        None => {
            if keep_nulls {
                m.insert(key.to_string(), Value::Null);
            }
        }
    }
}

/// Parse a JSON-array text cell to a `Value` array (fallback `[]` on NULL / parse
/// failure, mirroring `fromJsonSafe` then the Zod `.default([])`).
fn json_array_or_empty(v: Option<String>) -> Value {
    match v {
        Some(s) => serde_json::from_str::<Value>(&s)
            .ok()
            .filter(Value::is_array)
            .unwrap_or_else(|| Value::Array(Vec::new())),
        None => Value::Array(Vec::new()),
    }
}

/// A present, non-empty embedding BLOB → the `Float32Array` JSON-object shape
/// `{"0":v0,"1":v1,…}` that `JSON.stringify(Float32Array)` produces in v4.
fn embedding_to_value(blob: &[u8]) -> Value {
    let v = blob_to_float32(blob);
    let mut m = Map::new();
    for (i, f) in v.iter().enumerate() {
        m.insert(i.to_string(), js_number_to_json(*f as f64));
    }
    Value::Object(m)
}

/// Marshal one `memories` row (column order = [`COLS`]) to the net Memory JSON.
/// `keep_nulls` distinguishes the normal omit-NULL path from the raw-SQL
/// keep-NULL path (see [`put_nullable_str`]).
fn marshal_row(row: &Row, keep_nulls: bool) -> Result<Value, rusqlite::Error> {
    let mut m = Map::new();
    put_req_str(&mut m, "id", row.get::<_, String>(0)?);
    put_req_str(&mut m, "characterId", row.get::<_, String>(1)?);
    put_nullable_str(
        &mut m,
        "aboutCharacterId",
        row.get::<_, Option<String>>(2)?,
        keep_nulls,
    );
    put_nullable_str(
        &mut m,
        "chatId",
        row.get::<_, Option<String>>(3)?,
        keep_nulls,
    );
    put_nullable_str(
        &mut m,
        "projectId",
        row.get::<_, Option<String>>(4)?,
        keep_nulls,
    );
    put_req_str(&mut m, "content", row.get::<_, String>(5)?);
    put_req_str(&mut m, "summary", row.get::<_, String>(6)?);
    m.insert(
        "keywords".to_string(),
        json_array_or_empty(row.get::<_, Option<String>>(7)?),
    );
    m.insert(
        "tags".to_string(),
        json_array_or_empty(row.get::<_, Option<String>>(8)?),
    );
    m.insert(
        "importance".to_string(),
        js_number_to_json(row.get::<_, f64>(9)?),
    );
    match row.get::<_, Option<Vec<u8>>>(10)? {
        Some(blob) if !blob.is_empty() => {
            m.insert("embedding".to_string(), embedding_to_value(&blob));
        }
        _ => {
            if keep_nulls {
                m.insert("embedding".to_string(), Value::Null);
            }
        }
    }
    put_req_str(&mut m, "source", row.get::<_, String>(11)?);
    put_nullable_str(
        &mut m,
        "witnessedContext",
        row.get::<_, Option<String>>(12)?,
        keep_nulls,
    );
    put_nullable_str(
        &mut m,
        "sourceMessageId",
        row.get::<_, Option<String>>(13)?,
        keep_nulls,
    );
    put_nullable_str(
        &mut m,
        "lastAccessedAt",
        row.get::<_, Option<String>>(14)?,
        keep_nulls,
    );
    m.insert(
        "reinforcementCount".to_string(),
        js_number_to_json(row.get::<_, f64>(15)?),
    );
    put_nullable_str(
        &mut m,
        "lastReinforcedAt",
        row.get::<_, Option<String>>(16)?,
        keep_nulls,
    );
    m.insert(
        "relatedMemoryIds".to_string(),
        json_array_or_empty(row.get::<_, Option<String>>(17)?),
    );
    m.insert(
        "reinforcedImportance".to_string(),
        js_number_to_json(row.get::<_, f64>(18)?),
    );
    put_req_str(&mut m, "createdAt", row.get::<_, String>(19)?);
    put_req_str(&mut m, "updatedAt", row.get::<_, String>(20)?);
    Ok(Value::Object(m))
}

/// Run a SELECT that returns full memory rows ([`COLS`] order) and marshal each
/// (normal omit-NULL path).
fn query_memories(
    conn: &Connection,
    where_order: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, DbError> {
    let sql = format!("SELECT {COLS} FROM memories {where_order}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| marshal_row(r, false))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Run a `SELECT COUNT(*)` and return the count.
fn query_count(
    conn: &Connection,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> Result<i64, DbError> {
    let sql = format!("SELECT COUNT(*) FROM memories {where_clause}");
    let n: i64 = conn.query_row(&sql, params, |r| r.get(0))?;
    Ok(n)
}

// ============================================================================
// $regex → LIKE pattern (the exact v4 mangling)
// ============================================================================

/// `escapeRegex(input)` — `lib/utils/regex.ts`: prefix each
/// `[.*+?^${}()|[\]\\]` char with a backslash.
fn regex_source_escaped(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for c in query.chars() {
        if matches!(
            c,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Reproduce the query translator's `$regex` → `LIKE` conversion on the escaped
/// regex source: `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, wrapped `%…%`.
fn like_pattern(query: &str) -> String {
    let src = regex_source_escaped(query);
    let p = src.replace(".*", "%").replace('.', "_");
    format!("%{p}%")
}

/// Maximum search length before v4 bails (ReDoS guard).
const MAX_SEARCH_QUERY_LENGTH: usize = 1000;

/// UTF-16 code-unit length (v4's `string.length`), for the search-length gate.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

// ============================================================================
// Retrieval
// ============================================================================

pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Value>, DbError> {
    let mut v = query_memories(conn, "WHERE id = ?1 LIMIT 1", &[&id])?;
    Ok(v.pop())
}

/// `findByIdForCharacter` — `findOneByFilter({ id, characterId })`.
pub fn find_by_id_for_character(
    conn: &Connection,
    character_id: &str,
    memory_id: &str,
) -> Result<Option<Value>, DbError> {
    let mut v = query_memories(
        conn,
        "WHERE id = ?1 AND characterId = ?2 LIMIT 1",
        &[&memory_id, &character_id],
    )?;
    Ok(v.pop())
}

pub fn find_all(conn: &Connection) -> Result<Vec<Value>, DbError> {
    query_memories(conn, "", &[])
}

pub fn find_by_character_id(conn: &Connection, character_id: &str) -> Result<Vec<Value>, DbError> {
    query_memories(conn, "WHERE characterId = ?1", &[&character_id])
}

/// `findByCharacterIdInBatches` — the skip/limit generator, sorted by id ASC.
/// Returns the batches in order; errors if `batch_size <= 0` (v4 throws).
pub fn find_by_character_id_in_batches(
    conn: &Connection,
    character_id: &str,
    batch_size: i64,
) -> Result<Vec<Vec<Value>>, DbError> {
    if batch_size <= 0 {
        return Err(DbError::Key("batchSize must be positive".to_string()));
    }
    let mut batches = Vec::new();
    let mut skip = 0i64;
    loop {
        let batch = query_memories(
            conn,
            "WHERE characterId = ?1 ORDER BY id ASC LIMIT ?2 OFFSET ?3",
            &[&character_id, &batch_size, &skip],
        )?;
        let n = batch.len() as i64;
        if n == 0 {
            return Ok(batches);
        }
        batches.push(batch);
        if n < batch_size {
            return Ok(batches);
        }
        skip += batch_size;
    }
}

/// `findByIds` — `{ id: { $in: ids } }`; empty input → `[]`.
pub fn find_by_ids(conn: &Connection, ids: &[String]) -> Result<Vec<Value>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let params: Vec<&dyn ToSql> = ids.iter().map(|s| s as &dyn ToSql).collect();
    query_memories(conn, &format!("WHERE id IN ({placeholders})"), &params)
}

/// Options for [`find_by_character_id_paginated`] (v4's `options` bag; `sort_by`
/// / `sort_order` defaults — `createdAt` / `desc` — applied by the caller).
pub struct PaginateOptions<'a> {
    pub limit: i64,
    pub offset: i64,
    pub sort_by: &'a str,
    pub sort_order: &'a str,
    pub search: Option<&'a str>,
    pub source: Option<&'a str>,
    pub min_importance: Option<f64>,
}

/// Restrict a caller-supplied sort column to a known identifier (v4 interpolates
/// it raw; the corpus uses these). Unknown → `createdAt`.
fn safe_sort_col(sort_by: &str) -> &'static str {
    match sort_by {
        "importance" => "importance",
        "reinforcedImportance" => "reinforcedImportance",
        "reinforcementCount" => "reinforcementCount",
        "updatedAt" => "updatedAt",
        _ => "createdAt",
    }
}

/// `findByCharacterIdPaginated` — SQL-filter by `characterId` (+ optional
/// `source` / `importance >= min`), sort, fetch ALL, then in-memory `search`
/// filter (lowercase `includes` on content/summary/keywords), `totalCount` after
/// filter, `slice(offset, offset+limit)`. Returns `(page, totalCount)`.
pub fn find_by_character_id_paginated(
    conn: &Connection,
    character_id: &str,
    opts: &PaginateOptions<'_>,
) -> Result<(Vec<Value>, i64), DbError> {
    let mut where_clause = String::from("WHERE characterId = ?");
    let mut params: Vec<Box<dyn ToSql>> = vec![Box::new(character_id.to_string())];
    if let Some(src) = opts.source {
        where_clause.push_str(" AND source = ?");
        params.push(Box::new(src.to_string()));
    }
    if let Some(min) = opts.min_importance {
        where_clause.push_str(" AND importance >= ?");
        params.push(Box::new(min));
    }
    let dir = if opts.sort_order == "desc" {
        "DESC"
    } else {
        "ASC"
    };
    let where_order = format!(
        "{where_clause} ORDER BY {} {dir}",
        safe_sort_col(opts.sort_by)
    );
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let all = query_memories(conn, &where_order, &param_refs)?;

    let filtered: Vec<Value> = match opts.search {
        Some(search) if !search.is_empty() => {
            let needle = search.to_lowercase();
            all.into_iter()
                .filter(|m| memory_matches_search(m, &needle))
                .collect()
        }
        _ => all,
    };
    let total = filtered.len() as i64;
    let start = opts.offset.max(0) as usize;
    let end = start
        .saturating_add(opts.limit.max(0) as usize)
        .min(filtered.len());
    let page = if start >= filtered.len() {
        Vec::new()
    } else {
        filtered[start..end].to_vec()
    };
    Ok((page, total))
}

/// In-memory search predicate: lowercase `includes` on content, summary, or any
/// keyword (v4's `findByCharacterIdPaginated` search branch).
fn memory_matches_search(m: &Value, needle_lower: &str) -> bool {
    let field_has = |k: &str| {
        m.get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| s.to_lowercase().contains(needle_lower))
    };
    if field_has("content") || field_has("summary") {
        return true;
    }
    m.get("keywords")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .any(|k| k.to_lowercase().contains(needle_lower))
        })
}

/// `findByKeywords` — `{ characterId, keywords: { $in } }`; `keywords` is a JSON
/// column so `$in` lowers to `EXISTS(json_each … value IN (…))`.
pub fn find_by_keywords(
    conn: &Connection,
    character_id: &str,
    keywords: &[String],
) -> Result<Vec<Value>, DbError> {
    if keywords.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (0..keywords.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let where_order = format!(
        "WHERE characterId = ? AND EXISTS (SELECT 1 FROM json_each(keywords) WHERE value IN ({placeholders}))"
    );
    let mut params: Vec<&dyn ToSql> = Vec::with_capacity(1 + keywords.len());
    params.push(&character_id);
    for k in keywords {
        params.push(k);
    }
    query_memories(conn, &where_order, &params)
}

/// `searchByContent` — `{ characterId, $or:[{content:{$regex}},{summary:{$regex}}] }`.
pub fn search_by_content(
    conn: &Connection,
    character_id: &str,
    query: &str,
) -> Result<Vec<Value>, DbError> {
    if utf16_len(query) > MAX_SEARCH_QUERY_LENGTH {
        return Ok(Vec::new());
    }
    let pat = like_pattern(query);
    query_memories(
        conn,
        "WHERE characterId = ?1 AND ((content LIKE ?2) OR (summary LIKE ?2))",
        &[&character_id, &pat],
    )
}

/// `findByImportance` — `{ characterId, importance: { $gte } }`; out-of-range
/// threshold → `[]`.
pub fn find_by_importance(
    conn: &Connection,
    character_id: &str,
    min_importance: f64,
) -> Result<Vec<Value>, DbError> {
    if !(0.0..=1.0).contains(&min_importance) {
        return Ok(Vec::new());
    }
    query_memories(
        conn,
        "WHERE characterId = ?1 AND importance >= ?2",
        &[&character_id, &min_importance],
    )
}

/// `findBySource` — `{ characterId, source }`.
pub fn find_by_source(
    conn: &Connection,
    character_id: &str,
    source: &str,
) -> Result<Vec<Value>, DbError> {
    query_memories(
        conn,
        "WHERE characterId = ?1 AND source = ?2",
        &[&character_id, &source],
    )
}

/// `findRecent` — sort `createdAt` DESC, limit.
pub fn find_recent(
    conn: &Connection,
    character_id: &str,
    limit: i64,
) -> Result<Vec<Value>, DbError> {
    query_memories(
        conn,
        "WHERE characterId = ?1 ORDER BY createdAt DESC LIMIT ?2",
        &[&character_id, &limit],
    )
}

/// `findMostImportant` — sort `importance` DESC, limit.
pub fn find_most_important(
    conn: &Connection,
    character_id: &str,
    limit: i64,
) -> Result<Vec<Value>, DbError> {
    query_memories(
        conn,
        "WHERE characterId = ?1 ORDER BY importance DESC LIMIT ?2",
        &[&character_id, &limit],
    )
}

/// `findRecentByImportanceTier` — high (`≥0.7`, limit), medium (`≥0.3` then
/// in-memory `< 0.7` on `reinforcedImportance ?? importance`, take), low
/// (`<0.3`, limit). Returns `(high, medium, low)`.
#[allow(clippy::type_complexity)]
pub fn find_recent_by_importance_tier(
    conn: &Connection,
    character_id: &str,
    high_limit: i64,
    medium_limit: i64,
    low_limit: i64,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>), DbError> {
    let thresh_high = 0.7_f64;
    let thresh_med = 0.3_f64;
    let high = query_memories(
        conn,
        "WHERE characterId = ?1 AND importance >= ?2 ORDER BY createdAt DESC LIMIT ?3",
        &[&character_id, &thresh_high, &high_limit],
    )?;
    let all_above_low = query_memories(
        conn,
        "WHERE characterId = ?1 AND importance >= ?2 ORDER BY createdAt DESC",
        &[&character_id, &thresh_med],
    )?;
    let medium: Vec<Value> = all_above_low
        .into_iter()
        .filter(|m| {
            let eff = m
                .get("reinforcedImportance")
                .and_then(Value::as_f64)
                .or_else(|| m.get("importance").and_then(Value::as_f64))
                .unwrap_or(0.0);
            eff < 0.7
        })
        .take(medium_limit.max(0) as usize)
        .collect();
    let low = query_memories(
        conn,
        "WHERE characterId = ?1 AND importance < ?2 ORDER BY createdAt DESC LIMIT ?3",
        &[&character_id, &thresh_med, &low_limit],
    )?;
    Ok((high, medium, low))
}

/// `findByCharacterAboutCharacter` — sort `importance` DESC, `createdAt` DESC.
pub fn find_by_character_about_character(
    conn: &Connection,
    character_id: &str,
    about_character_id: &str,
) -> Result<Vec<Value>, DbError> {
    query_memories(
        conn,
        "WHERE characterId = ?1 AND aboutCharacterId = ?2 ORDER BY importance DESC, createdAt DESC",
        &[&character_id, &about_character_id],
    )
}

/// `findByCharacterAboutCharacters` — the window-function partition cap. Verbatim
/// v4 SQL (ROW_NUMBER over `aboutCharacterId`, ranked by importance then
/// `COALESCE(lastReinforcedAt, createdAt)`), `rn <= limitPerCharacter`.
pub fn find_by_character_about_characters(
    conn: &Connection,
    character_id: &str,
    about_character_ids: &[String],
    limit_per_character: i64,
) -> Result<Vec<Value>, DbError> {
    if about_character_ids.is_empty() || limit_per_character <= 0 {
        return Ok(Vec::new());
    }
    let placeholders = about_character_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH ranked AS (\n  SELECT {COLS},\n    ROW_NUMBER() OVER (\n      PARTITION BY aboutCharacterId\n      ORDER BY importance DESC,\n               COALESCE(lastReinforcedAt, createdAt) DESC\n    ) AS rn\n  FROM memories\n  WHERE characterId = ?\n    AND aboutCharacterId IN ({placeholders})\n)\nSELECT {COLS} FROM ranked WHERE rn <= ?"
    );
    let mut params: Vec<&dyn ToSql> = Vec::with_capacity(2 + about_character_ids.len());
    params.push(&character_id);
    for a in about_character_ids {
        params.push(a);
    }
    params.push(&limit_per_character);
    let mut stmt = conn.prepare(&sql)?;
    // Raw-SQL path: rawQuery rows carry explicit NULLs which `safeParse` keeps.
    let rows = stmt.query_map(params.as_slice(), |r| marshal_row(r, true))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn find_by_chat_id(conn: &Connection, chat_id: &str) -> Result<Vec<Value>, DbError> {
    query_memories(conn, "WHERE chatId = ?1", &[&chat_id])
}

pub fn find_by_source_message_id(
    conn: &Connection,
    source_message_id: &str,
) -> Result<Vec<Value>, DbError> {
    query_memories(conn, "WHERE sourceMessageId = ?1", &[&source_message_id])
}

pub fn find_by_about_character_id(
    conn: &Connection,
    about_character_id: &str,
) -> Result<Vec<Value>, DbError> {
    query_memories(conn, "WHERE aboutCharacterId = ?1", &[&about_character_id])
}

/// `searchByContentAboutCharacter` — `{ characterId, aboutCharacterId,
/// $or:[content,summary] }`.
pub fn search_by_content_about_character(
    conn: &Connection,
    character_id: &str,
    about_character_id: &str,
    query: &str,
) -> Result<Vec<Value>, DbError> {
    let pat = like_pattern(query);
    query_memories(
        conn,
        "WHERE characterId = ?1 AND aboutCharacterId = ?2 AND ((content LIKE ?3) OR (summary LIKE ?3))",
        &[&character_id, &about_character_id, &pat],
    )
}

/// Build the shared `$or` text-search WHERE for `findMemoriesWithText` /
/// `countMemoriesWithText`: search content/summary/keywords, optional
/// character/chat scope. Returns `(where_clause, owned_params)`.
fn text_search_where(
    character_id: Option<&str>,
    chat_id: Option<&str>,
    pat: &str,
) -> (String, Vec<String>) {
    let mut clause = String::from(
        "WHERE ((content LIKE ?) OR (summary LIKE ?) OR \
         EXISTS (SELECT 1 FROM json_each(keywords) WHERE value LIKE ?))",
    );
    let mut params: Vec<String> = vec![pat.to_string(), pat.to_string(), pat.to_string()];
    if let Some(c) = character_id {
        clause.push_str(" AND characterId = ?");
        params.push(c.to_string());
    }
    if let Some(c) = chat_id {
        clause.push_str(" AND chatId = ?");
        params.push(c.to_string());
    }
    (clause, params)
}

/// `findMemoriesWithText` — content/summary/keywords text search, optional
/// character/chat scope. Truthy (non-empty) scope only, matching v4's `if (id)`.
pub fn find_memories_with_text(
    conn: &Connection,
    character_id: Option<&str>,
    chat_id: Option<&str>,
    search_text: &str,
) -> Result<Vec<Value>, DbError> {
    if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
        return Ok(Vec::new());
    }
    let pat = like_pattern(search_text);
    let (clause, owned) =
        text_search_where(filter_truthy(character_id), filter_truthy(chat_id), &pat);
    let params: Vec<&dyn ToSql> = owned.iter().map(|s| s as &dyn ToSql).collect();
    query_memories(conn, &clause, &params)
}

/// `countMemoriesWithText` — count variant of [`find_memories_with_text`].
pub fn count_memories_with_text(
    conn: &Connection,
    character_id: Option<&str>,
    chat_id: Option<&str>,
    search_text: &str,
) -> Result<i64, DbError> {
    if utf16_len(search_text) > MAX_SEARCH_QUERY_LENGTH {
        return Ok(0);
    }
    let pat = like_pattern(search_text);
    let (clause, owned) =
        text_search_where(filter_truthy(character_id), filter_truthy(chat_id), &pat);
    let params: Vec<&dyn ToSql> = owned.iter().map(|s| s as &dyn ToSql).collect();
    query_count(conn, &clause, &params)
}

/// v4 scopes only on a truthy id (`if (characterId)` / `if (chatId)`) — an empty
/// string is falsy and skips the clause.
fn filter_truthy(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty())
}

// ============================================================================
// Counts / existence / id-only
// ============================================================================

pub fn count_by_character_id(conn: &Connection, character_id: &str) -> Result<i64, DbError> {
    query_count(conn, "WHERE characterId = ?1", &[&character_id])
}

/// `countCreatedSince` — `{ characterId, createdAt: { $gte: since } }`.
pub fn count_created_since(
    conn: &Connection,
    character_id: &str,
    since: &str,
) -> Result<i64, DbError> {
    query_count(
        conn,
        "WHERE characterId = ?1 AND createdAt >= ?2",
        &[&character_id, &since],
    )
}

/// `countWithoutEmbedding` — `embedding IS NULL`, optional character scope.
pub fn count_without_embedding(
    conn: &Connection,
    character_id: Option<&str>,
) -> Result<i64, DbError> {
    match character_id {
        Some(c) => query_count(conn, "WHERE characterId = ?1 AND embedding IS NULL", &[&c]),
        None => query_count(conn, "WHERE embedding IS NULL", &[]),
    }
}

/// `findIdsWithoutEmbedding` — `{ id, characterId }` of rows missing an
/// embedding, sorted `createdAt` DESC, limit (default 500).
pub fn find_ids_without_embedding(
    conn: &Connection,
    character_id: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<Value>, DbError> {
    let lim = limit.unwrap_or(500);
    // Run inside each arm so the `&c` borrow never outlives the param slice.
    match character_id {
        Some(c) => id_pairs_query(
            conn,
            "WHERE characterId = ?1 AND embedding IS NULL ORDER BY createdAt DESC LIMIT ?2",
            &[&c, &lim],
        ),
        None => id_pairs_query(
            conn,
            "WHERE embedding IS NULL ORDER BY createdAt DESC LIMIT ?1",
            &[&lim],
        ),
    }
}

/// Run a `SELECT id, characterId FROM memories <where_order>` and shape each row
/// as `{ id, characterId }` (for `findIdsWithoutEmbedding`).
fn id_pairs_query(
    conn: &Connection,
    where_order: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, DbError> {
    let sql = format!("SELECT id, characterId FROM memories {where_order}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| {
        let id: String = r.get(0)?;
        let cid: String = r.get(1)?;
        let mut m = Map::new();
        m.insert("id".to_string(), Value::String(id));
        m.insert("characterId".to_string(), Value::String(cid));
        Ok(Value::Object(m))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn count_by_chat_id(conn: &Connection, chat_id: &str) -> Result<i64, DbError> {
    query_count(conn, "WHERE chatId = ?1", &[&chat_id])
}

pub fn count_by_source_message_id(
    conn: &Connection,
    source_message_id: &str,
) -> Result<i64, DbError> {
    query_count(conn, "WHERE sourceMessageId = ?1", &[&source_message_id])
}

/// `countBySourceMessageIds` — `{ sourceMessageId: { $in } }`; empty → 0.
pub fn count_by_source_message_ids(conn: &Connection, ids: &[String]) -> Result<i64, DbError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
    let params: Vec<&dyn ToSql> = ids.iter().map(|s| s as &dyn ToSql).collect();
    query_count(
        conn,
        &format!("WHERE sourceMessageId IN ({placeholders})"),
        &params,
    )
}

/// `countByChatIds` — single GROUP BY; returns `{ chatId: count }` (chats with
/// zero memories absent), as a JSON object for differential comparison.
pub fn count_by_chat_ids(conn: &Connection, chat_ids: &[String]) -> Result<Value, DbError> {
    let mut m = Map::new();
    if chat_ids.is_empty() {
        return Ok(Value::Object(m));
    }
    let placeholders = (0..chat_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let params: Vec<&dyn ToSql> = chat_ids.iter().map(|s| s as &dyn ToSql).collect();
    let sql = format!(
        "SELECT chatId, COUNT(*) AS c FROM memories WHERE chatId IN ({placeholders}) GROUP BY chatId"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    for r in rows {
        let (chat, count) = r?;
        m.insert(chat, Value::from(count));
    }
    Ok(Value::Object(m))
}

/// `findDistinctChatIds` — distinct non-null, non-empty chatIds.
pub fn find_distinct_chat_ids(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare("SELECT DISTINCT chatId FROM memories WHERE chatId IS NOT NULL")?;
    let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))?;
    let mut out = Vec::new();
    for r in rows {
        if let Some(s) = r? {
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_reproduces_v4_mangling() {
        // Plain alphanumerics: no specials → simple contains.
        assert_eq!(like_pattern("hello"), "%hello%");
        // A literal dot: escaped to "\.", the `.` then → "_", leaving "\_".
        assert_eq!(like_pattern("a.b"), "%a\\_b%");
        // A literal ".*": escaped to "\.\*"; no bare ".*" remains; the dot → "_".
        assert_eq!(like_pattern(".*"), "%\\_\\*%");
        // Percent is not a regex special, so it passes through (a LIKE wildcard).
        assert_eq!(like_pattern("50%"), "%50%%");
    }

    #[test]
    fn utf16_len_counts_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("é"), 1);
        assert_eq!(utf16_len("😀"), 2); // astral → surrogate pair
    }
}
