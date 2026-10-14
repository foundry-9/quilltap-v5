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
//!
//! ## The memory tiers (P4.D265, v4 `d58548051` / `3f7320138`)
//!
//! Four columns joined the row (`tier`, `supersededById`, `consolidatedFrom`,
//! `consolidatedAt`), and v4 has THREE hot-tier semantics — each read keeps its
//! own (§R.4(b)): [`HOT_TIER_SQL`] (`COALESCE(tier, 'hot') = 'hot'`, NULL is
//! hot) in `find_most_important`, `find_by_character_about_characters`,
//! `find_hot_digests` and the two counts; an EXACT `tier = 'hot'` (NULL
//! excluded) in `find_recent_by_importance_tier` and the paginated `tier`;
//! `tier !== 'cold'` in services. The raw-SQL reads validate their rows the way
//! v4's `hydrateRawRows` does ([`hydrate_raw_rows`] — the Q2 measurement is
//! recorded there). `find_by_character_id` stays UNFILTERED (backup / export /
//! restore carry cold rows).
//!
//! **NO-PORT (R-E):** v4's `findSupersededBy(digestId)`
//! (`memories.repository.ts:934-941`) has no caller anywhere in v4 at the pin
//! (re-grepped over `lib/ app/ packages/`, 2026-10-09).

use rusqlite::types::ToSql;
use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::js_number_to_json;
use super::DbError;
use crate::chunk::{chunk_array, SQLITE_VARIABLE_CHUNK_SIZE};
use crate::embedding_blob::blob_to_float32;

/// All `memories` columns in schema order (indices 0..=29 in [`marshal_row`];
/// P4.D288 — v4 `c2795733f`'s `supersededAt` at 25, between `supersededById`
/// and `consolidatedFrom`, its generateDDL / Zod-shape position).
/// The four episodic columns (v4 8bf3cb5f) sit between `witnessedContext` and
/// `sourceMessageId`, and the four tier columns (v4 `d58548051`, P4.D265)
/// between `reinforcedImportance` and `createdAt` — their generateDDL/Zod-shape
/// positions — so the marshaled key order matches v4's parsed Memory object.
/// Pinned against `fresh_schema.json`'s `memories` statement by the
/// [`alignment_census`] tests.
const COLS: &str = "id, characterId, aboutCharacterId, chatId, projectId, content, summary, \
     keywords, tags, importance, embedding, source, witnessedContext, \
     occurredAt, narrativeTime, entities, kind, sourceMessageId, \
     lastAccessedAt, reinforcementCount, lastReinforcedAt, relatedMemoryIds, reinforcedImportance, \
     tier, supersededById, supersededAt, consolidatedFrom, consolidatedAt, \
     createdAt, updatedAt";

/// v4's `HOT_TIER_SQL` (`memories.repository.ts:33`, `d58548051`): a NULL
/// tier reads HOT. One of v4's THREE hot-tier semantics (§R.4(b)) — the raw
/// reads and the counts use this; `find_recent_by_importance_tier` and the
/// paginated `?tier=` use an exact `tier = 'hot'` (NULL excluded); services
/// use `tier !== 'cold'`.
pub const HOT_TIER_SQL: &str = "COALESCE(tier, 'hot') = 'hot'";

/// Insert a required (always-present) string column.
fn put_req_str(m: &mut Map<String, Value>, key: &str, v: String) {
    m.insert(key.to_string(), Value::String(v));
}

/// Insert a nullable-optional string column. In the normal `findByFilter` path
/// (`keep_nulls = false`) a NULL is **omitted** (v4's `undefined` dropped by
/// `JSON.stringify`). In the raw-SQL path (`keep_nulls = true`) the rawQuery
/// row carries an explicit `null` which `MemorySchema.safeParse` keeps for a
/// `.nullable()` field, so we emit `null`.
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

/// A JSON-array column on the RAW path (v4 `hydrateRawRows`, `:393-418`): a
/// string cell is `JSON.parse`d — WHATEVER it parses to, the schema then
/// judges it — and a parse failure becomes `[]`; a SQL NULL stays `null`
/// (which the array schema then refuses, `.default([])` replacing `undefined`
/// only).
fn raw_json_cell(v: Option<String>) -> Value {
    match v {
        Some(s) => serde_json::from_str::<Value>(&s).unwrap_or_else(|_| Value::Array(Vec::new())),
        None => Value::Null,
    }
}

/// A present, non-empty embedding BLOB → the `Float32Array` JSON-object shape
/// `{"0":v0,"1":v1,…}` that `JSON.stringify(Float32Array)` produces in v4.
/// It STAYS that shape after v4 bug 181 (`039f7017c`): v4 fixed the full
/// backup in `backup-service.ts` alone, not `findByCharacterId`, so every
/// other consumer of this read (the memories wire, `.qtap` export, the
/// character archive, cascade delete, rename, the reindex job) still mirrors
/// v4's object; the backup re-encodes it (`backup::collect`'s
/// `encode_memory_embedding`).
fn embedding_to_value(blob: &[u8]) -> Value {
    let v = blob_to_float32(blob);
    let mut m = Map::new();
    for (i, f) in v.iter().enumerate() {
        m.insert(i.to_string(), js_number_to_json(*f as f64));
    }
    Value::Object(m)
}

/// Marshal one `memories` row (column order = [`COLS`]) to the net Memory JSON.
///
/// `keep_nulls = false` is the `findByFilter` path: v4's `hydrateRow` maps NULL
/// → `undefined`, so a nullable-optional key is OMITTED and a `.default()` key
/// takes its default (`tier` → `"hot"`, `consolidatedFrom` → `[]`, `kind` →
/// `"semantic"`). `keep_nulls = true` is the RAW path (`rawQuery` keeps SQL
/// NULLs as `null`): every NULL cell is an explicit `null` — including the
/// required and `.default()` ones — and the JSON-array cells are parsed as
/// [`raw_json_cell`] does, so [`hydrate_raw_rows`] can judge the row exactly
/// as `MemorySchema.safeParse` does.
fn marshal_row(row: &Row, keep_nulls: bool) -> Result<Value, rusqlite::Error> {
    let mut m = Map::new();
    // A required / `.default()` string column: the raw path keeps a NULL as
    // `null` (the schema judges it); the filter path reads it as before.
    let req_str =
        |m: &mut Map<String, Value>, key: &str, idx: usize| -> Result<(), rusqlite::Error> {
            if keep_nulls {
                let v: Option<String> = row.get(idx)?;
                m.insert(key.to_string(), v.map(Value::String).unwrap_or(Value::Null));
            } else {
                put_req_str(m, key, row.get::<_, String>(idx)?);
            }
            Ok(())
        };
    let num = |m: &mut Map<String, Value>, key: &str, idx: usize| -> Result<(), rusqlite::Error> {
        if keep_nulls {
            let v: Option<f64> = row.get(idx)?;
            m.insert(
                key.to_string(),
                v.map(js_number_to_json).unwrap_or(Value::Null),
            );
        } else {
            m.insert(key.to_string(), js_number_to_json(row.get::<_, f64>(idx)?));
        }
        Ok(())
    };
    let arr = |m: &mut Map<String, Value>, key: &str, idx: usize| -> Result<(), rusqlite::Error> {
        let v: Option<String> = row.get(idx)?;
        m.insert(
            key.to_string(),
            if keep_nulls {
                raw_json_cell(v)
            } else {
                json_array_or_empty(v)
            },
        );
        Ok(())
    };
    // A `.default(<v>)` string column: NULL reads `<v>` on the filter path and
    // stays `null` (refused) on the raw path.
    let defaulted_str = |m: &mut Map<String, Value>,
                         key: &str,
                         idx: usize,
                         default: &str|
     -> Result<(), rusqlite::Error> {
        let v: Option<String> = row.get(idx)?;
        m.insert(
            key.to_string(),
            match (v, keep_nulls) {
                (Some(s), _) => Value::String(s),
                (None, true) => Value::Null,
                (None, false) => Value::String(default.to_string()),
            },
        );
        Ok(())
    };
    // The BLOB: present and non-empty → the Float32Array object; else omitted
    // (filter path) / `null` (raw path).
    let embedding =
        |m: &mut Map<String, Value>, key: &str, idx: usize| -> Result<(), rusqlite::Error> {
            match row.get::<_, Option<Vec<u8>>>(idx)? {
                Some(blob) if !blob.is_empty() => {
                    m.insert(key.to_string(), embedding_to_value(&blob));
                }
                _ => {
                    if keep_nulls {
                        m.insert(key.to_string(), Value::Null);
                    }
                }
            }
            Ok(())
        };
    req_str(&mut m, "id", 0)?;
    req_str(&mut m, "characterId", 1)?;
    put_nullable_str(&mut m, "aboutCharacterId", row.get(2)?, keep_nulls);
    put_nullable_str(&mut m, "chatId", row.get(3)?, keep_nulls);
    put_nullable_str(&mut m, "projectId", row.get(4)?, keep_nulls);
    req_str(&mut m, "content", 5)?;
    req_str(&mut m, "summary", 6)?;
    arr(&mut m, "keywords", 7)?;
    arr(&mut m, "tags", 8)?;
    num(&mut m, "importance", 9)?;
    embedding(&mut m, "embedding", 10)?;
    req_str(&mut m, "source", 11)?;
    put_nullable_str(&mut m, "witnessedContext", row.get(12)?, keep_nulls);
    // Episodic spine (v4 8bf3cb5f): occurredAt/narrativeTime nullable-optional;
    // entities a JSON string[] (default []); kind required-with-default —
    // the column carries DEFAULT 'semantic' so NULL never occurs on a
    // v4-migrated or v5-written row, but read defensively on the filter path
    // (Zod's `.default('semantic')` supplies it for an `undefined`); the raw
    // path keeps the `null` the schema refuses.
    put_nullable_str(&mut m, "occurredAt", row.get(13)?, keep_nulls);
    put_nullable_str(&mut m, "narrativeTime", row.get(14)?, keep_nulls);
    arr(&mut m, "entities", 15)?;
    defaulted_str(&mut m, "kind", 16, "semantic")?;
    put_nullable_str(&mut m, "sourceMessageId", row.get(17)?, keep_nulls);
    put_nullable_str(&mut m, "lastAccessedAt", row.get(18)?, keep_nulls);
    num(&mut m, "reinforcementCount", 19)?;
    put_nullable_str(&mut m, "lastReinforcedAt", row.get(20)?, keep_nulls);
    arr(&mut m, "relatedMemoryIds", 21)?;
    num(&mut m, "reinforcedImportance", 22)?;
    // P4.D265 (v4 `d58548051`): the tier columns. `tier` is
    // `MemoryTierEnum.default('hot')` — NULL reads `"hot"` on the filter path
    // and stays `null` (refused) on the raw path; `consolidatedFrom` is a
    // `.default([])` JSON array like `relatedMemoryIds`; the other two are
    // nullable-optional.
    defaulted_str(&mut m, "tier", 23, "hot")?;
    put_nullable_str(&mut m, "supersededById", row.get(24)?, keep_nulls);
    // P4.D288 (v4 `c2795733f`): when a superseded row was superseded —
    // `TimestampSchema.nullable().optional()`.
    put_nullable_str(&mut m, "supersededAt", row.get(25)?, keep_nulls);
    arr(&mut m, "consolidatedFrom", 26)?;
    put_nullable_str(&mut m, "consolidatedAt", row.get(27)?, keep_nulls);
    req_str(&mut m, "createdAt", 28)?;
    req_str(&mut m, "updatedAt", 29)?;
    Ok(Value::Object(m))
}

/// v4 `hydrateRawRows(rows, context)` (`memories.repository.ts:393-418`, R-D —
/// measured at the pin, keystone survey Q2): the three RAW-SQL reads
/// (`find_most_important` `most-important fetch`, `find_hot_digests` `hot
/// digest fetch`, `find_by_character_about_characters` `partition fetch`)
/// validate each row through `MemorySchema.safeParse` over its RAW cells — SQL
/// NULLs as `null` ([`marshal_row`]'s keep-NULL path) — and DROP a failing row
/// with WARN `` `Memory failed validation in ${context}` `` `{memoryId, error}`
/// (`error` = the `ZodError.message` bytes). Zod 4's `.default()` replaces
/// `undefined` only, so a `tier IS NULL` / `consolidatedFrom IS NULL` row (and
/// any NULL `kind` / `source` / numeric cell) passes the SQL hot predicate and
/// is then dropped here, where the `findByFilter` reads keep it with the
/// default. MEASURED at `01a83539d`: a tier-NULL row and a
/// consolidatedFrom-NULL row are dropped by all three raw reads (`invalid_value`
/// enum / `invalid_type` array); `find_recent_by_importance_tier`'s exact
/// `tier = 'hot'` EXCLUDES the tier-NULL row and KEEPS the
/// consolidatedFrom-NULL one with `[]`; the paginated list (no `tier`) and
/// `find_by_character_id` KEEP both, read as `'hot'` / `[]`.
///
/// The `embedding` BLOB is v4's `z.instanceof(Buffer)` union arm — always
/// valid — so it is left out of the validated bag. A passing row is emitted as
/// marshaled (explicit `null`s kept for the nullable keys — the parsed shape).
fn hydrate_raw_rows(rows: Vec<Value>, context: &str) -> Vec<Value> {
    use crate::api::zod_issues::{zod_error_message, zod_memory_issues};
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(obj) = row.as_object() else { continue };
        let mut bag = obj.clone();
        bag.remove("embedding");
        let issues = zod_memory_issues(&bag);
        if issues.is_empty() {
            out.push(row);
        } else {
            let memory_id = obj.get("id").and_then(Value::as_str).unwrap_or_default();
            tracing::warn!(
                target: "quilltap::db",
                memoryId = %memory_id,
                error = %zod_error_message(&issues),
                "Memory failed validation in {}",
                context
            );
        }
    }
    out
}

/// Run a raw-path SELECT (keep-NULL marshaling) — the rows
/// [`hydrate_raw_rows`] judges.
fn query_raw_memories(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |r| marshal_row(r, true))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
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
        return Err(DbError::Internal("batchSize must be positive".to_string()));
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
    /// P4.D265 (v4 `d58548051`): restrict to one tier — an EXACT `tier = ?`
    /// (v4's `filter.tier = tier` through `findByFilter`), so a NULL-tier row
    /// is in neither tier's listing. `None` lists both.
    pub tier: Option<&'a str>,
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
/// `source` / `importance >= min` / exact `tier`), sort, fetch ALL, then
/// in-memory `search` filter (lowercase `includes` on content/summary/keywords),
/// `totalCount` after filter, `slice(offset, offset+limit)`. Returns `(page,
/// totalCount)`.
///
/// P4.D265 (R-I): v4's fallback contract. The body's read is a `findByFilter`
/// (a fallback itself): outside the strict scope a failed read logs `Error
/// finding entities by filter` and the page is `{memories: [], totalCount:
/// 0}`; inside it that line rethrows and the method's own `Error finding
/// paginated memories for character {characterId, limit, offset}` follows,
/// both `strictFailures=true`, and the `Err` propagates.
pub fn find_by_character_id_paginated(
    conn: &Connection,
    character_id: &str,
    opts: &PaginateOptions<'_>,
) -> Result<(Vec<Value>, i64), DbError> {
    super::fallback::paginated_memories_or_empty(character_id, opts.limit, opts.offset, || {
        let all = super::fallback::find_by_filter_strict_aware("memories", || {
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
            if let Some(tier) = opts.tier {
                where_clause.push_str(" AND tier = ?");
                params.push(Box::new(tier.to_string()));
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
            query_memories(conn, &where_order, &param_refs)
        })?;

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
    })
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

/// v4 `findMostImportant` (`memories.repository.ts:361-389`, F4 `3f7320138` +
/// the hot predicate `d58548051`): a character's HOT memories (NULL tier reads
/// hot — [`HOT_TIER_SQL`]) ranked `reinforcedImportance DESC,
/// COALESCE(lastReinforcedAt, createdAt) DESC, id ASC`, `LIMIT`. `limit <= 0`
/// → `[]` before any query (and no line). Raw SQL → the keep-NULL rows go
/// through [`hydrate_raw_rows`] (`most-important fetch`), then DEBUG `Fetched
/// most important memories {characterId, limit, returned}`. A failed read is
/// v4's fallback: ERROR `Error finding most important memories {characterId,
/// limit}` and `[]` (strict-aware, R-I).
pub fn find_most_important(
    conn: &Connection,
    character_id: &str,
    limit: i64,
) -> Result<Vec<Value>, DbError> {
    super::fallback::most_important_memories_or_empty(character_id, limit, || {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {COLS} FROM memories \
             WHERE characterId = ? AND {HOT_TIER_SQL} \
             ORDER BY reinforcedImportance DESC, COALESCE(lastReinforcedAt, createdAt) DESC, id ASC \
             LIMIT ?"
        );
        let rows = query_raw_memories(conn, &sql, &[&character_id, &limit])?;
        let memories = hydrate_raw_rows(rows, "most-important fetch");
        tracing::debug!(
            target: "quilltap::db",
            characterId = %character_id,
            limit = limit,
            returned = memories.len(),
            "Fetched most important memories"
        );
        Ok(memories)
    })
}

/// `findRecentByImportanceTier` — high (`≥0.7`, limit), medium (`≥0.3` then
/// in-memory `< 0.7` on `reinforcedImportance ?? importance`, take), low
/// (`<0.3`, limit). Returns `(high, medium, low)`.
///
/// P4.D265 (v4 `d58548051`): every leg gains `tier: 'hot'` — an EXACT
/// `tier = 'hot'` through `findByFilter` (NOT [`HOT_TIER_SQL`]), so a NULL-tier
/// row is EXCLUDED here (measured). Each leg is v4's own 4-argument fallback
/// around a `findByFilter` (itself a fallback): outside the strict scope a
/// failed leg logs `Error finding entities by filter` and answers `[]`; inside
/// it the leg's own `Error finding <high|medium|low>-importance memories
/// {characterId, limit}` follows and the `Err` propagates (R-I).
#[allow(clippy::type_complexity)]
pub fn find_recent_by_importance_tier(
    conn: &Connection,
    character_id: &str,
    high_limit: i64,
    medium_limit: i64,
    low_limit: i64,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>), DbError> {
    use super::fallback::{
        find_by_filter_strict_aware, importance_tier_leg_or_empty, ImportanceTierLeg,
    };
    let thresh_high = 0.7_f64;
    let thresh_med = 0.3_f64;
    let high =
        importance_tier_leg_or_empty(ImportanceTierLeg::High, character_id, high_limit, || {
            find_by_filter_strict_aware("memories", || {
                query_memories(
                    conn,
                    "WHERE characterId = ?1 AND tier = 'hot' AND importance >= ?2 \
                 ORDER BY createdAt DESC LIMIT ?3",
                    &[&character_id, &thresh_high, &high_limit],
                )
            })
        })?;
    let medium = importance_tier_leg_or_empty(
        ImportanceTierLeg::Medium,
        character_id,
        medium_limit,
        || {
            let all_above_low = find_by_filter_strict_aware("memories", || {
                query_memories(
                    conn,
                    "WHERE characterId = ?1 AND tier = 'hot' AND importance >= ?2 \
                     ORDER BY createdAt DESC",
                    &[&character_id, &thresh_med],
                )
            })?;
            Ok(all_above_low
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
                .collect())
        },
    )?;
    let low =
        importance_tier_leg_or_empty(ImportanceTierLeg::Low, character_id, low_limit, || {
            find_by_filter_strict_aware("memories", || {
                query_memories(
                    conn,
                    "WHERE characterId = ?1 AND tier = 'hot' AND importance < ?2 \
                 ORDER BY createdAt DESC LIMIT ?3",
                    &[&character_id, &thresh_med, &low_limit],
                )
            })
        })?;
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
///
/// P4.D265 (v4 `d58548051`): the CTE's WHERE gains [`HOT_TIER_SQL`] (the ORDER
/// BY is unchanged — F4 did not touch it); `rn` is stripped (v4 deletes it) and
/// the rows go through [`hydrate_raw_rows`] (`partition fetch`). A failed read
/// is v4's fallback: ERROR `Error finding memories about characters
/// {characterId, aboutCharacterCount, limitPerCharacter}` and `[]` (R-I).
pub fn find_by_character_about_characters(
    conn: &Connection,
    character_id: &str,
    about_character_ids: &[String],
    limit_per_character: i64,
) -> Result<Vec<Value>, DbError> {
    super::fallback::memories_about_characters_or_empty(
        character_id,
        about_character_ids.len(),
        limit_per_character,
        || {
            if about_character_ids.is_empty() || limit_per_character <= 0 {
                return Ok(Vec::new());
            }
            let placeholders = about_character_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "WITH ranked AS (\n  SELECT {COLS},\n    ROW_NUMBER() OVER (\n      PARTITION BY aboutCharacterId\n      ORDER BY importance DESC,\n               COALESCE(lastReinforcedAt, createdAt) DESC\n    ) AS rn\n  FROM memories\n  WHERE characterId = ?\n    AND aboutCharacterId IN ({placeholders})\n    AND {HOT_TIER_SQL}\n)\nSELECT {COLS} FROM ranked WHERE rn <= ?"
            );
            let mut params: Vec<&dyn ToSql> = Vec::with_capacity(2 + about_character_ids.len());
            params.push(&character_id);
            for a in about_character_ids {
                params.push(a);
            }
            params.push(&limit_per_character);
            let rows = query_raw_memories(conn, &sql, params.as_slice())?;
            Ok(hydrate_raw_rows(rows, "partition fetch"))
        },
    )
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

/// v4 `findByCharacterAndSourceMessageIds` — a character's memories whose
/// `sourceMessageId` is any of the given ids. The fold-time episode pass links
/// a consolidated episode to the per-turn fragment memories from the same
/// window. Empty id list → `[]` (v4's early return, no query).
pub fn find_by_character_and_source_message_ids(
    conn: &Connection,
    character_id: &str,
    source_message_ids: &[String],
) -> Result<Vec<Value>, DbError> {
    if source_message_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (0..source_message_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let mut params: Vec<&dyn ToSql> = vec![&character_id];
    params.extend(source_message_ids.iter().map(|s| s as &dyn ToSql));
    query_memories(
        conn,
        &format!("WHERE characterId = ?1 AND sourceMessageId IN ({placeholders})"),
        &params,
    )
}

/// v4's FALLBACK `findByCharacterAndSourceMessageIds`, as its one caller (the
/// fold-time episode pass) sees it (`memories.repository.ts:833-848`): an
/// empty id list answers `[]` before any query (v4 `:837` — no query, no
/// line); otherwise a failed read logs `Error finding entities by filter
/// {collection: 'memories', error}` and answers `[]`.
///
/// ⚠ NOT the method's own `'Error finding memories by character and source
/// message IDs'` (P4.144's order prescribed that literal; measured at
/// `f6426e196` it is UNREACHABLE). v4's outer `safeQuery` wraps
/// `this.findByFilter(…)`, which is itself a fallback `safeQuery`
/// (`base.repository.ts:283-297`, `[]`): it logs FIRST and never throws
/// outside a strict-failures scope (`safe-query.ts:64-69`; only the importer
/// enters one), so the outer catch never runs. A tsx probe through v4's real
/// repository over a renamed `memories` table logged exactly `SQLite find
/// error {table, error}` (the backend line, the recorded unported class) then
/// `Error finding entities by filter {collection: 'memories', error: 'no such
/// table: memories'}` — the `doc_mount_folders` `find_by_mount_point_id_or_empty`
/// shape. The propagating sibling stays for strict callers.
pub fn find_by_character_and_source_message_ids_or_empty(
    conn: &Connection,
    character_id: &str,
    source_message_ids: &[String],
) -> Vec<Value> {
    if source_message_ids.is_empty() {
        return Vec::new();
    }
    super::fallback::find_by_filter_or_empty("memories", || {
        find_by_character_and_source_message_ids(conn, character_id, source_message_ids)
    })
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

// ============================================================================
// Tiers and consolidation (P4.D265 — v4 `d58548051`, `memories.repository.ts:
// 758-966`). Each a 4-argument FALLBACK `safeQuery` over a raw read — v4's
// fallback value with its ERROR line, strict-aware (R-I).
// ============================================================================

/// v4 `countHotByCharacterId` (`:758-771`): `SELECT COUNT(*) AS n FROM
/// memories WHERE characterId = ? AND` [`HOT_TIER_SQL`] — the number the
/// housekeeping cap and the consolidation watermark are measured against.
pub fn count_hot_by_character_id(conn: &Connection, character_id: &str) -> Result<i64, DbError> {
    super::fallback::hot_memory_count_or_zero(character_id, || {
        query_count(
            conn,
            &format!("WHERE characterId = ?1 AND {HOT_TIER_SQL}"),
            &[&character_id],
        )
    })
}

/// v4 `countUnconsideredHot` (`:777-794`): hot, non-digest rows the
/// consolidator has never considered (`consolidatedAt IS NULL AND source !=
/// 'CONSOLIDATED'`) — the watermark trigger's count.
pub fn count_unconsidered_hot(conn: &Connection, character_id: &str) -> Result<i64, DbError> {
    super::fallback::unconsidered_hot_count_or_zero(character_id, || {
        query_count(
            conn,
            &format!(
                "WHERE characterId = ?1 AND {HOT_TIER_SQL} AND consolidatedAt IS NULL \
                 AND source != 'CONSOLIDATED'"
            ),
            &[&character_id],
        )
    })
}

/// v4 `findColdIdsByCharacterId` (`:801-814`): `SELECT id FROM memories WHERE
/// characterId = ? AND tier = 'cold'` — the vector store's tier stamp source.
pub fn find_cold_ids_by_character_id(
    conn: &Connection,
    character_id: &str,
) -> Result<Vec<String>, DbError> {
    super::fallback::cold_memory_ids_or_empty(character_id, || {
        query_ids(
            conn,
            "SELECT id FROM memories WHERE characterId = ?1 AND tier = 'cold'",
            &[&character_id],
        )
    })
}

/// The subject bucket of [`find_hot_digests`] — v4's `subject: string | 'self'
/// | 'any'`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestSubject {
    /// Digests about the holder (`aboutCharacterId = characterId`).
    SelfSubject,
    /// Every hot digest (no subject clause).
    Any,
    /// Digests about this character id.
    About(String),
}

impl DigestSubject {
    /// v4's rendering in the ERROR line's `subject` field: `self` / `any` /
    /// the id.
    pub fn as_log_str(&self) -> &str {
        match self {
            DigestSubject::SelfSubject => "self",
            DigestSubject::Any => "any",
            DigestSubject::About(id) => id,
        }
    }
}

/// v4 `findHotDigests(characterId, subject, limit)` (`:824-860`): a
/// character's HOT digests (`source = 'CONSOLIDATED'` AND [`HOT_TIER_SQL`]),
/// best first `reinforcedImportance DESC, COALESCE(lastReinforcedAt,
/// createdAt) DESC, id ASC`, `LIMIT`. `limit <= 0` → `[]` (no query). Raw rows
/// through [`hydrate_raw_rows`] (`hot digest fetch`).
pub fn find_hot_digests(
    conn: &Connection,
    character_id: &str,
    subject: DigestSubject,
    limit: i64,
) -> Result<Vec<Value>, DbError> {
    super::fallback::hot_digests_or_empty(character_id, subject.as_log_str(), limit, || {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let (subject_sql, subject_param): (&str, Option<&str>) = match &subject {
            DigestSubject::SelfSubject => (" AND aboutCharacterId = ?", Some(character_id)),
            DigestSubject::Any => ("", None),
            DigestSubject::About(id) => (" AND aboutCharacterId = ?", Some(id.as_str())),
        };
        let sql = format!(
            "SELECT {COLS} FROM memories \
             WHERE characterId = ? AND source = 'CONSOLIDATED' AND {HOT_TIER_SQL}{subject_sql} \
             ORDER BY reinforcedImportance DESC, COALESCE(lastReinforcedAt, createdAt) DESC, id ASC \
             LIMIT ?"
        );
        let mut params: Vec<&dyn ToSql> = vec![&character_id];
        if let Some(p) = &subject_param {
            params.push(p);
        }
        params.push(&limit);
        let rows = query_raw_memories(conn, &sql, &params)?;
        Ok(hydrate_raw_rows(rows, "hot digest fetch"))
    })
}

/// v4 `findExpiredColdIds(characterId, olderThan)` (`:947-965`): superseded,
/// cold, non-MANUAL rows last touched before `olderThan` (a STRING compare on
/// `updatedAt`) — the only rows `coldRetentionDays` may delete. CONSOLIDATED
/// rows are included (`source != 'MANUAL'`, §R.4(e)).
pub fn find_expired_cold_ids(
    conn: &Connection,
    character_id: &str,
    older_than: &str,
) -> Result<Vec<String>, DbError> {
    super::fallback::expired_cold_ids_or_empty(character_id, older_than, || {
        query_ids(
            conn,
            "SELECT id FROM memories WHERE characterId = ?1 AND tier = 'cold' \
             AND supersededById IS NOT NULL AND source != 'MANUAL' AND updatedAt < ?2",
            &[&character_id, &older_than],
        )
    })
}

/// Run a `SELECT id FROM memories …` and collect the ids.
fn query_ids(conn: &Connection, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
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
///
/// ⚠ v4 does **not** chunk this read (only its two memories-delete sites feed
/// `lib/utils/chunk.ts`). The chunking is the P4.65 scale-safety measure taken
/// after P4.D126 measured a 40,000-id batch failing with "too many SQL
/// variables", and it is invisible in the output: [`chunk_array`] partitions the
/// input, so **each chatId lands in exactly one chunk** and the per-chunk objects
/// have disjoint keys. It matters now because this read is about to gain its
/// first production caller — the chat-list preload, which passes every chat id on
/// the instance (861+ on the real Friday data).
pub fn count_by_chat_ids(conn: &Connection, chat_ids: &[String]) -> Result<Value, DbError> {
    let mut m = Map::new();
    if chat_ids.is_empty() {
        return Ok(Value::Object(m));
    }
    for chunk in chunk_array(chat_ids, SQLITE_VARIABLE_CHUNK_SIZE) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let params: Vec<&dyn ToSql> = chunk.iter().map(|s| s as &dyn ToSql).collect();
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
    }
    Ok(Value::Object(m))
}

/// v4 `findDistinctCharacterIds` (P4.d27 / v4 `7391404e`) — every distinct
/// `characterId` that owns at least one memory, non-null and non-empty.
///
/// The `EMBEDDING_REINDEX_ALL` fan-out reads this INSTEAD of the characters
/// repository: `characters.findAll`/`findByUserId` silently DROP a character
/// whose vault is unavailable, and a dropped character's memories would then be
/// left permanently un-re-embedded after a profile switch. Memories are the
/// ground truth for "who needs re-embedding".
///
/// **Deliberately NOT user-scoped** — the fan-out it replaced was. v4's SQL has
/// no `userId` predicate (memories carry none directly; ownership is via the
/// character), so on a multi-user instance every character with memories is
/// enumerated, whoever owns it.
///
/// Returns the ids directly rather than a `Result` because v4 wraps this in
/// `safeQuery` with `[]` as the fallback, and the difference is observable: the
/// reindex's memory phase is the one phase v4 does NOT wrap in a try/catch, so a
/// read failure here must yield an empty fan-out and a COMPLETED job — not a
/// failed one. (Its sibling [`find_distinct_chat_ids`] is `safeQuery`-wrapped in
/// v4 too but was ported as a `Result`; its callers are another lane's.)
pub fn find_distinct_character_ids(conn: &Connection) -> Vec<String> {
    let query = || -> Result<Vec<String>, DbError> {
        let mut stmt = conn
            .prepare("SELECT DISTINCT characterId FROM memories WHERE characterId IS NOT NULL")?;
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
    };
    match query() {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::db",
                error = %e,
                "Error listing distinct memory characterIds",
            );
            Vec::new()
        }
    }
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

// === P4.D288 (C1-3): the index-store reads ===

/// One row of a character's vector index (v4 `findIndexRowsByCharacterId`'s
/// `{ id, embedding, tier }`): the decoded vector and the row's tier (`cold`
/// iff the cell says `'cold'`; NULL / anything else reads hot).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRow {
    pub id: String,
    pub embedding: Vec<f32>,
    pub tier: super::vector_store::Tier,
}

/// v4 `decodeStoredEmbedding` (`memories.repository.ts:69-83`) for a raw
/// cell: a BLOB through the header-aware [`blob_to_float32`]; TEXT through
/// `JSON.parse` — an array becomes the vector (`Float32Array.from`: a
/// non-number element is `NaN`), anything else (and unparseable text) is
/// `None`; any other storage class is `None`.
pub fn decode_stored_embedding(cell: rusqlite::types::ValueRef<'_>) -> Option<Vec<f32>> {
    match cell {
        rusqlite::types::ValueRef::Blob(b) => Some(blob_to_float32(b)),
        rusqlite::types::ValueRef::Text(t) => {
            let parsed: Value = serde_json::from_str(&String::from_utf8_lossy(t)).ok()?;
            let arr = parsed.as_array()?;
            Some(
                arr.iter()
                    .map(|v| v.as_f64().map_or(f32::NAN, |f| f as f32))
                    .collect(),
            )
        }
        _ => None,
    }
}

/// v4 `findIndexRowsByCharacterId(characterId)` (`memories.repository.ts:
/// 1060-1093`): `SELECT id, embedding, tier FROM memories WHERE characterId = ?
/// AND embedding IS NOT NULL`, each embedding decoded; an undecodable or EMPTY
/// one is skipped and counted — WARN `Skipped memory embeddings that could not
/// be decoded {characterId, undecodable}` when any were — then DEBUG `Read
/// memory index rows {characterId, rows}`. FALLBACK `[]` with ERROR `Error
/// reading memory index rows {collection, characterId, error}`
/// ([`super::fallback::index_rows_or_empty`] — strict-aware).
pub fn find_index_rows_by_character_id(
    conn: &Connection,
    character_id: &str,
) -> Result<Vec<IndexRow>, DbError> {
    super::fallback::index_rows_or_empty(character_id, || {
        let mut stmt = conn.prepare(
            "SELECT id, embedding, tier FROM memories WHERE characterId = ? AND embedding IS NOT NULL",
        )?;
        let raw = stmt
            .query_map([character_id], |r| {
                let tier = match r.get_ref(2)? {
                    rusqlite::types::ValueRef::Text(t) if t == b"cold" => {
                        super::vector_store::Tier::Cold
                    }
                    _ => super::vector_store::Tier::Hot,
                };
                Ok((
                    r.get::<_, String>(0)?,
                    decode_stored_embedding(r.get_ref(1)?),
                    tier,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(raw.len());
        let mut undecodable = 0usize;
        for (id, embedding, tier) in raw {
            match embedding {
                Some(embedding) if !embedding.is_empty() => out.push(IndexRow {
                    id,
                    embedding,
                    tier,
                }),
                _ => undecodable += 1,
            }
        }
        if undecodable > 0 {
            tracing::warn!(
                target: "quilltap::db",
                characterId = %character_id,
                undecodable = undecodable,
                "Skipped memory embeddings that could not be decoded"
            );
        }
        tracing::debug!(
            target: "quilltap::db",
            characterId = %character_id,
            rows = out.len(),
            "Read memory index rows"
        );
        Ok(out)
    })
}

/// v4 `countCorpusSince(since)` (`memories.repository.ts:980-993`): `{ total,
/// createdSince }` — every memory row, and those with `createdAt >= since`
/// (0 when `since` is `None`), each through the base `count` (a FALLBACK `0`,
/// `Error counting entities`); FALLBACK `{0, 0}` with ERROR `Error counting the
/// memory corpus {collection, since, error}` (strict-aware).
pub fn count_corpus_since(conn: &Connection, since: Option<&str>) -> Result<(i64, i64), DbError> {
    super::fallback::corpus_count_or_zero(since, || {
        let total = super::fallback::count_strict_aware("memories", || query_count(conn, "", &[]))?;
        let created_since = match since {
            Some(s) => super::fallback::count_strict_aware("memories", || {
                query_count(conn, "WHERE createdAt >= ?1", &[&s])
            })?,
            None => 0,
        };
        Ok((total, created_since))
    })
}

// === end P4.D288 ===

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

    // ── `count_by_chat_ids` (P4.65: chunked) ────────────────────────────────

    /// Only the two columns this GROUP BY names.
    fn count_scratch(rows: &[(&str, &str)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE memories (id TEXT PRIMARY KEY, chatId TEXT);")
            .unwrap();
        for (id, chat_id) in rows {
            conn.execute(
                "INSERT INTO memories (id, chatId) VALUES (?1, ?2)",
                rusqlite::params![id, chat_id],
            )
            .unwrap();
        }
        conn
    }

    fn chat_ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// The counts are per chat, and a chat with no memories is **absent** from
    /// the object (the GROUP BY emits no row for it) — as is an unknown id.
    #[test]
    fn count_by_chat_ids_counts_present_chats_only() {
        let conn = count_scratch(&[
            ("m1", "chatA"),
            ("m2", "chatA"),
            ("m3", "chatB"),
            ("m4", "chatC"),
        ]);
        let out = count_by_chat_ids(&conn, &chat_ids(&["chatA", "chatB", "ghost"])).unwrap();
        assert_eq!(out["chatA"], Value::from(2));
        assert_eq!(out["chatB"], Value::from(1));
        assert!(out.get("ghost").is_none(), "a chat with no rows is absent");
        assert!(
            out.get("chatC").is_none(),
            "an unqueried chat is never counted"
        );
        assert_eq!(out.as_object().unwrap().len(), 2);
    }

    #[test]
    fn count_by_chat_ids_empty_input_is_an_empty_object() {
        let conn = count_scratch(&[("m1", "chatA")]);
        let out = count_by_chat_ids(&conn, &[]).unwrap();
        assert_eq!(out, Value::Object(Map::new()));
    }

    /// The P4.65 chunking, in the P4.D126 idiom: 40,000 ids is past the engine's
    /// 32,766 ceiling, so an un-chunked `IN (…)` fails with "too many SQL
    /// variables". The three real chats sit in three different chunk regions, so
    /// a port that only ran the first chunk would lose two of them — and because
    /// the chunks partition the input, the merged object's keys are disjoint.
    #[test]
    fn count_by_chat_ids_chunks_past_the_variable_limit() {
        let conn = count_scratch(&[
            ("m1", "chatA"),
            ("m2", "chatA"),
            ("m3", "chatB"),
            ("m4", "chatC"),
            ("m5", "chatC"),
            ("m6", "chatC"),
        ]);
        let mut batch: Vec<String> = (0..40_000).map(|i| format!("ghost-{i}")).collect();
        batch[0] = "chatA".to_string();
        batch[SQLITE_VARIABLE_CHUNK_SIZE] = "chatB".to_string();
        batch[39_999] = "chatC".to_string();

        let out = count_by_chat_ids(&conn, &batch)
            .expect("a chunked count_by_chat_ids must not hit the variable limit");
        assert_eq!(out["chatA"], Value::from(2));
        assert_eq!(out["chatB"], Value::from(1));
        assert_eq!(out["chatC"], Value::from(3));
        assert_eq!(out.as_object().unwrap().len(), 3);
    }

    // ── `find_by_character_and_source_message_ids_or_empty` (P4.144) ────────

    /// A `memories` table carrying exactly the columns `COLS` selects.
    fn fragment_scratch() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let cols: Vec<String> = COLS.split(',').map(|c| c.trim().to_string()).collect();
        conn.execute_batch(&format!("CREATE TABLE memories ({});", cols.join(", ")))
            .unwrap();
        conn
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// The capture leg over a planted failure (the table renamed away): v4's
    /// INNER `findByFilter` line, `collection` first, the BARE SQLite message
    /// (no `sqlite error:` prefix — the `97b25fc53` smalls unification's rule;
    /// v4's bytes measured through its real repository at `f6426e196`), and `[]`.
    #[test]
    fn fragment_read_failure_logs_v4s_find_by_filter_line_and_answers_empty() {
        let conn = fragment_scratch();
        conn.execute_batch("ALTER TABLE memories RENAME TO memories_x;")
            .unwrap();
        let (out, lines) = crate::test_support::captured_with(|| {
            find_by_character_and_source_message_ids_or_empty(&conn, "c1", &ids(&["m1", "m2"]))
        });
        assert!(out.is_empty());
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding entities by filter collection=memories \
                 error=no such table: memories"
                    .to_string()
            ],
        );
    }

    /// The silence leg: a healthy table logs nothing.
    #[test]
    fn fragment_read_on_a_healthy_table_is_silent() {
        let conn = fragment_scratch();
        let (out, lines) = crate::test_support::captured_with(|| {
            find_by_character_and_source_message_ids_or_empty(&conn, "c1", &ids(&["m1"]))
        });
        assert!(out.is_empty());
        assert!(lines.is_empty(), "a healthy read logs nothing: {lines:?}");
    }

    /// The empty-ids leg: v4 answers `[]` BEFORE the query (`:837`), so even a
    /// missing table logs nothing.
    #[test]
    fn fragment_read_with_no_ids_never_queries() {
        let conn = Connection::open_in_memory().unwrap();
        let (out, lines) = crate::test_support::captured_with(|| {
            find_by_character_and_source_message_ids_or_empty(&conn, "c1", &[])
        });
        assert!(out.is_empty());
        assert!(lines.is_empty(), "no ids, no query, no line: {lines:?}");
    }
}

// ============================================================================
// The index↔column alignment census (P4.D265 — the `chats_read` shape)
// ============================================================================

#[cfg(test)]
mod alignment_census {
    use super::COLS;

    /// This file's own source: [`super::marshal_row`] reads by POSITION, and
    /// every statement in it names its JSON key right before its column index
    /// (`req_str(&mut m, "id", 0)`, `put_nullable_str(&mut m, "chatId",
    /// row.get(3)?, …)`), so the census pairs each index with that key.
    const SOURCE: &str = include_str!("memories_read.rs");

    /// The D23 dump — the single source of truth for the `memories` DDL.
    const FRESH_SCHEMA: &str = include_str!("../services/provisioning/fresh_schema.json");

    fn marshal_row_body() -> String {
        let start = SOURCE
            .find("fn marshal_row(row: &Row, keep_nulls: bool)")
            .expect("marshal_row is defined in this file");
        let open = start + SOURCE[start..].find('{').expect("marshal_row has a body");
        let mut depth = 0usize;
        let mut end = None;
        for (offset, ch) in SOURCE[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        SOURCE[open..end.expect("balanced braces")]
            .lines()
            .map(|line| match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every `"<key>", <N>` / `"<key>", row.get(<N>)` pairing, in source order.
    fn key_index_pairs() -> Vec<(usize, String)> {
        let body = marshal_row_body();
        let mut pairs = Vec::new();
        let mut rest = body.as_str();
        while let Some(q) = rest.find('"') {
            let after = &rest[q + 1..];
            let Some(close) = after.find('"') else { break };
            let key = &after[..close];
            let tail = after[close + 1..].trim_start();
            if let Some(tail) = tail.strip_prefix(',') {
                let tail = tail.trim_start();
                let tail = tail.strip_prefix("row.get(").unwrap_or(tail);
                let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
                if !digits.is_empty()
                    && key.chars().all(|c| c.is_ascii_alphanumeric())
                    && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                {
                    pairs.push((digits.parse().unwrap(), key.to_string()));
                }
            }
            rest = &after[close + 1..];
        }
        pairs
    }

    fn schema_columns() -> Vec<String> {
        let schema: serde_json::Value =
            serde_json::from_str(FRESH_SCHEMA).expect("fresh_schema.json parses");
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"memories\" ("))
            .expect("the dump carries the memories DDL");
        let inner = &ddl[ddl.find('(').unwrap() + 1..ddl.rfind(')').unwrap()];
        inner
            .lines()
            .filter_map(|line| {
                let t = line.trim();
                t.strip_prefix('"')
                    .and_then(|r| r.find('"').map(|e| r[..e].to_string()))
            })
            .collect()
    }

    fn selected_columns() -> Vec<String> {
        COLS.split(',')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    }

    /// `COLS` is EXACTLY the D23 dump's `memories` column list, in its order
    /// (29 columns at `01a83539d`; 30 at `83334b9a5` — P4.D288's
    /// `supersededAt`) — the marshaled key order is v4's parsed Memory order
    /// because the two coincide.
    #[test]
    fn cols_is_the_d23_dump_memories_column_list_in_order() {
        assert_eq!(selected_columns(), schema_columns());
        assert_eq!(selected_columns().len(), 30);
    }

    /// …and every index `marshal_row` reads marshals the column at that
    /// position under its own name. Swapping two indices reddens this.
    #[test]
    fn marshal_row_reads_each_column_at_its_own_index() {
        let cols = selected_columns();
        let pairs = key_index_pairs();
        let mut indices: Vec<usize> = pairs.iter().map(|(i, _)| *i).collect();
        indices.sort_unstable();
        assert_eq!(indices, (0..cols.len()).collect::<Vec<_>>(), "{pairs:?}");
        for (index, key) in &pairs {
            assert_eq!(key, &cols[*index], "index {index}");
        }
        // Source order is schema order too (the JSON key order).
        let in_order: Vec<usize> = pairs.iter().map(|(i, _)| *i).collect();
        assert_eq!(in_order, (0..cols.len()).collect::<Vec<_>>());
    }
}
