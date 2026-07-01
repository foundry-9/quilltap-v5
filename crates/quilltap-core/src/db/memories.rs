//! The memories **write / mutation** surface of v4's `MemoriesRepository`
//! (`lib/database/repositories/memories.repository.ts` + the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`). The read
//! path (every `findBy*` / `count*`) lives in [`super::memories_read`].
//!
//! `memories` is a plain `AbstractBaseRepository<Memory>` in the **main** db with
//! no base-method override, so `create` / `update` / `delete` are the standard
//! internals; on top of them the repo adds character-scoped and bulk mutators
//! (`updateForCharacter`, `bulkDelete`, `updateAccessTime{,Bulk}`,
//! `deleteByChatId`, `deleteBySourceMessageId{,s}`, `replaceInMemories`).
//!
//! ## Marshaling (verified against v4's `mapToSQLiteType` + `documentToRow`)
//!
//!   - `importance` / `reinforcedImportance` are `z.number().min(0).max(1)` — min
//!     AND max are integers (`0`, `1`), so `mapToSQLiteType` gives the column
//!     **INTEGER** affinity; `reinforcementCount` is `z.number().int().min(1)`
//!     (min only) → **REAL**. All three are bound as `f64`: a fractional value
//!     (`0.5`) lands as REAL on both column kinds, an integer value (`1.0`) is
//!     collapsed by NUMERIC affinity to INTEGER `1` identically to v4's
//!     better-sqlite3 binding, and the canonical dump's `js_number_to_json`
//!     renders an integer-valued REAL back to a JSON integer — so the stored cell
//!     and the dump match byte-for-byte.
//!   - `embedding` is the BLOB column (little-endian Float32 via
//!     [`crate::embedding_blob::float32_to_blob`]); `None` / empty → SQL NULL
//!     (never a zero-length blob), exactly like `conversation_chunks` /
//!     `help_docs`. A **text-only** patch (`MemUpdate::embedding == None`) never
//!     names the column, so the stored vector is left intact (the
//!     `conversation_chunks` / `help_docs` rule). The memory gate does write the
//!     BLOB through `update` (v4's `updateForCharacter(id, { embedding })`), so
//!     `MemUpdate::embedding` is a `Some`-gated setter: only a `Some(_)` patch
//!     names `embedding`, matching v4's partial `$set`.
//!   - `keywords` / `tags` / `relatedMemoryIds` are JSON-array columns (compact
//!     JSON text); the eight nullable-optional columns bind SQL NULL when absent.
//!
//! Determinism: `create` honors pinned id + timestamps (`CreateOptions`); the
//! mutators mint `updatedAt` (and `lastAccessedAt`) via [`crate::clock::now_iso`]
//! unless overridden — the harness placeholders those two columns on both dumps.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde_json::Value;

use super::memories_read;
use super::DbError;
use crate::clock::now_iso;
use crate::embedding_blob::float32_to_blob;

/// Create fields — the post-Zod `Omit<Memory,'id'|'createdAt'|'updatedAt'>` shape
/// (defaults already materialized by the caller, mirroring v4's `_create` which
/// validates the merged entity). Nullable-optional fields are `Option`.
pub struct MemCreate {
    pub character_id: String,
    pub about_character_id: Option<String>,
    pub chat_id: Option<String>,
    pub project_id: Option<String>,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub importance: f64,
    /// `None` / empty → SQL NULL; non-empty → little-endian Float32 bytes.
    pub embedding: Option<Vec<f32>>,
    pub source: String,
    pub witnessed_context: Option<String>,
    pub source_message_id: Option<String>,
    pub last_accessed_at: Option<String>,
    pub reinforcement_count: f64,
    pub last_reinforced_at: Option<String>,
    pub related_memory_ids: Vec<String>,
    pub reinforced_importance: f64,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An update patch (v4 `update(id, Partial<Memory>)`). Each `Some` sets that
/// column; nullable columns use `Option<Option<_>>` so `Some(None)` writes SQL
/// NULL while `None` leaves the column untouched. `updated_at` overrides the
/// minted timestamp when `Some` (v4's `'updatedAt' in data` branch); otherwise
/// it is minted.
///
/// `embedding` is the one BLOB-writing setter: `None` leaves the stored vector
/// untouched (the common text-only patch — the `conversation_chunks` / `help_docs`
/// rule the Phase-2 corpus proved), while `Some(inner)` names the column, exactly
/// mirroring v4's `updateForCharacter(id, { embedding })` — `Some(Some(vec))`
/// writes the Float32 LE bytes (empty → NULL), `Some(None)` writes SQL NULL. The
/// memory gate's `createMemoryDirectWithEmbedding` and its reinforce re-embed both
/// take this path.
#[derive(Default)]
pub struct MemUpdate {
    pub content: Option<String>,
    pub summary: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub related_memory_ids: Option<Vec<String>>,
    pub importance: Option<f64>,
    pub source: Option<String>,
    pub reinforcement_count: Option<f64>,
    pub reinforced_importance: Option<f64>,
    pub about_character_id: Option<Option<String>>,
    pub chat_id: Option<Option<String>>,
    pub project_id: Option<Option<String>>,
    pub witnessed_context: Option<Option<String>>,
    pub source_message_id: Option<Option<String>>,
    pub last_accessed_at: Option<Option<String>>,
    pub last_reinforced_at: Option<Option<String>>,
    /// `None` → leave the BLOB untouched; `Some(Some(v))` → Float32 LE bytes
    /// (empty → NULL); `Some(None)` → SQL NULL.
    pub embedding: Option<Option<Vec<f32>>>,
    pub updated_at: Option<String>,
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct MemoriesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> MemoriesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `create` — insert with pinned id + timestamps. Embedding → Float32 LE BLOB
    /// (`None`/empty → NULL); JSON-array columns → compact JSON text; the three
    /// numeric columns bind `f64`.
    pub fn create(&self, data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        let keywords_json = json_array(&data.keywords)?;
        let tags_json = json_array(&data.tags)?;
        let related_json = json_array(&data.related_memory_ids)?;

        self.conn.execute(
            "INSERT INTO memories \
               (id, characterId, aboutCharacterId, chatId, projectId, content, summary, \
                keywords, tags, importance, embedding, source, witnessedContext, \
                sourceMessageId, lastAccessedAt, reinforcementCount, lastReinforcedAt, \
                relatedMemoryIds, reinforcedImportance, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18, ?19, ?20, ?21)",
            params![
                opts.id,
                data.character_id,
                data.about_character_id,
                data.chat_id,
                data.project_id,
                data.content,
                data.summary,
                keywords_json,
                tags_json,
                data.importance,
                embedding_blob,
                data.source,
                data.witnessed_context,
                data.source_message_id,
                data.last_accessed_at,
                data.reinforcement_count,
                data.last_reinforced_at,
                related_json,
                data.reinforced_importance,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// `update` — apply a patch to memory `id`. Returns `Ok(false)` when no row
    /// matched (v4's `_update` "not found → null"). id / createdAt / the
    /// `embedding` BLOB are never touched; each `Some` field sets its column;
    /// `updatedAt` is always set (override or minted).
    pub fn update(&self, id: &str, patch: &MemUpdate) -> Result<bool, DbError> {
        if !self.row_exists(id)? {
            return Ok(false);
        }
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        macro_rules! set_col {
            ($col:literal, $boxed:expr) => {{
                assignments.push(format!("{} = ?{}", $col, values.len() + 1));
                values.push($boxed);
            }};
        }

        if let Some(v) = &patch.content {
            set_col!("content", Box::new(v.clone()));
        }
        if let Some(v) = &patch.summary {
            set_col!("summary", Box::new(v.clone()));
        }
        if let Some(v) = &patch.keywords {
            set_col!("keywords", Box::new(json_array(v)?));
        }
        if let Some(v) = &patch.tags {
            set_col!("tags", Box::new(json_array(v)?));
        }
        if let Some(v) = &patch.related_memory_ids {
            set_col!("relatedMemoryIds", Box::new(json_array(v)?));
        }
        if let Some(v) = patch.importance {
            set_col!("importance", Box::new(v));
        }
        if let Some(v) = &patch.source {
            set_col!("source", Box::new(v.clone()));
        }
        if let Some(v) = patch.reinforcement_count {
            set_col!("reinforcementCount", Box::new(v));
        }
        if let Some(v) = patch.reinforced_importance {
            set_col!("reinforcedImportance", Box::new(v));
        }
        if let Some(v) = &patch.about_character_id {
            set_col!("aboutCharacterId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.chat_id {
            set_col!("chatId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.project_id {
            set_col!("projectId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.witnessed_context {
            set_col!("witnessedContext", Box::new(v.clone()));
        }
        if let Some(v) = &patch.source_message_id {
            set_col!("sourceMessageId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.last_accessed_at {
            set_col!("lastAccessedAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.last_reinforced_at {
            set_col!("lastReinforcedAt", Box::new(v.clone()));
        }
        if let Some(emb) = &patch.embedding {
            let blob: Option<Vec<u8>> = match emb {
                Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
                _ => None,
            };
            set_col!("embedding", Box::new(blob));
        }
        let updated_at = patch.updated_at.clone().unwrap_or_else(now_iso);
        set_col!("updatedAt", Box::new(updated_at));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));
        let sql = format!(
            "UPDATE memories SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );
        let refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let n = self.conn.execute(&sql, refs.as_slice())?;
        Ok(n > 0)
    }

    /// `delete` — returns `Ok(false)` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let n = self
            .conn
            .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    /// `updateForCharacter` — update only if the memory exists AND belongs to
    /// `character_id` (else a no-op returning `Ok(false)`).
    pub fn update_for_character(
        &self,
        character_id: &str,
        memory_id: &str,
        patch: &MemUpdate,
    ) -> Result<bool, DbError> {
        if self.character_id_of(memory_id)?.as_deref() != Some(character_id) {
            return Ok(false);
        }
        self.update(memory_id, patch)
    }

    /// `deleteForCharacter` — delete only if owned by `character_id`.
    pub fn delete_for_character(
        &self,
        character_id: &str,
        memory_id: &str,
    ) -> Result<bool, DbError> {
        if self.character_id_of(memory_id)?.as_deref() != Some(character_id) {
            return Ok(false);
        }
        self.delete(memory_id)
    }

    /// `bulkDelete` — `deleteMany({ characterId, id: { $in } })`; empty → 0.
    pub fn bulk_delete(&self, character_id: &str, memory_ids: &[String]) -> Result<i64, DbError> {
        if memory_ids.is_empty() {
            return Ok(0);
        }
        let placeholders = (0..memory_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("DELETE FROM memories WHERE characterId = ? AND id IN ({placeholders})");
        let mut p: Vec<&dyn ToSql> = Vec::with_capacity(1 + memory_ids.len());
        p.push(&character_id);
        for id in memory_ids {
            p.push(id);
        }
        let n = self.conn.execute(&sql, p.as_slice())?;
        Ok(n as i64)
    }

    /// `updateAccessTime` — set `lastAccessedAt = now` if owned. (v4's `update`
    /// also bumps `updatedAt`.)
    pub fn update_access_time(&self, character_id: &str, memory_id: &str) -> Result<bool, DbError> {
        if self.character_id_of(memory_id)?.as_deref() != Some(character_id) {
            return Ok(false);
        }
        let patch = MemUpdate {
            last_accessed_at: Some(Some(now_iso())),
            ..Default::default()
        };
        self.update(memory_id, &patch)
    }

    /// `updateAccessTimeBulk` — `updateMany({ characterId, id: { $in } },
    /// { lastAccessedAt: now })`; `updateMany` also sets `updatedAt = now`.
    /// Returns rows affected; empty → 0.
    pub fn update_access_time_bulk(
        &self,
        character_id: &str,
        memory_ids: &[String],
    ) -> Result<i64, DbError> {
        if memory_ids.is_empty() {
            return Ok(0);
        }
        let now = now_iso();
        let placeholders = (0..memory_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE memories SET lastAccessedAt = ?, updatedAt = ? \
             WHERE characterId = ? AND id IN ({placeholders})"
        );
        let mut p: Vec<&dyn ToSql> = Vec::with_capacity(3 + memory_ids.len());
        p.push(&now);
        p.push(&now);
        p.push(&character_id);
        for id in memory_ids {
            p.push(id);
        }
        let n = self.conn.execute(&sql, p.as_slice())?;
        Ok(n as i64)
    }

    /// `deleteByChatId` — `deleteMany({ chatId })`.
    pub fn delete_by_chat_id(&self, chat_id: &str) -> Result<i64, DbError> {
        let n = self
            .conn
            .execute("DELETE FROM memories WHERE chatId = ?1", params![chat_id])?;
        Ok(n as i64)
    }

    /// `deleteBySourceMessageId` — `deleteMany({ sourceMessageId })`.
    pub fn delete_by_source_message_id(&self, source_message_id: &str) -> Result<i64, DbError> {
        let n = self.conn.execute(
            "DELETE FROM memories WHERE sourceMessageId = ?1",
            params![source_message_id],
        )?;
        Ok(n as i64)
    }

    /// `deleteBySourceMessageIds` — `deleteMany({ sourceMessageId: { $in } })`;
    /// empty → 0.
    pub fn delete_by_source_message_ids(&self, ids: &[String]) -> Result<i64, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!("DELETE FROM memories WHERE sourceMessageId IN ({placeholders})");
        let p: Vec<&dyn ToSql> = ids.iter().map(|s| s as &dyn ToSql).collect();
        let n = self.conn.execute(&sql, p.as_slice())?;
        Ok(n as i64)
    }

    /// `replaceInMemories` — for each id, literal substring replace across
    /// content / summary / keywords (`split(search).join(replace)` = replace
    /// ALL); only `update`s a memory if something changed. Returns the ids of
    /// updated memories (in input order), for embedding regeneration.
    pub fn replace_in_memories(
        &self,
        memory_ids: &[String],
        search_text: &str,
        replace_text: &str,
    ) -> Result<Vec<String>, DbError> {
        let mut updated: Vec<String> = Vec::new();
        for id in memory_ids {
            let memory = match memories_read::find_by_id(self.conn, id)? {
                Some(m) => m,
                None => continue,
            };
            let content = memory.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let summary = memory.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let keywords: Vec<String> = memory
                .get("keywords")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let mut changed = false;
            let new_content = if content.contains(search_text) {
                changed = true;
                content.replace(search_text, replace_text)
            } else {
                content.to_string()
            };
            let new_summary = if summary.contains(search_text) {
                changed = true;
                summary.replace(search_text, replace_text)
            } else {
                summary.to_string()
            };
            let new_keywords: Vec<String> = keywords
                .iter()
                .map(|k| {
                    if k.contains(search_text) {
                        changed = true;
                        k.replace(search_text, replace_text)
                    } else {
                        k.clone()
                    }
                })
                .collect();

            if changed {
                let patch = MemUpdate {
                    content: Some(new_content),
                    summary: Some(new_summary),
                    keywords: Some(new_keywords),
                    ..Default::default()
                };
                if self.update(id, &patch)? {
                    updated.push(id.clone());
                }
            }
        }
        Ok(updated)
    }

    /// The `characterId` of memory `id`, or `None` if no such row (v4's
    /// ownership precondition via `findById`).
    fn character_id_of(&self, id: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT characterId FROM memories WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(DbError::from)
    }

    // -----------------------------------------------------------------------
    // The deletion chokepoint (v4 `lib/memory/memory-gate.ts`).
    //
    // v4 places `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch` in
    // memory-gate.ts (parallel to `createMemoryWithGate` on the write side), but
    // they are pure `memories`-table operations — a neighbour-unlink scan wrapped
    // around the repo's own `updateForCharacter` / `delete` / `bulkDelete` — so
    // they live on the repository here. Every cascade path (housekeeping retention
    // sweeps, chat-wipe, swipe-group cleanup, single-memory delete) funnels
    // through one of these two so a deleted id never lingers in another memory's
    // `relatedMemoryIds`.
    // -----------------------------------------------------------------------

    /// v4's `deleteMemoryWithUnlink` — the single-memory chokepoint. Delete one
    /// memory and scrub the deleted id from every neighbour's `relatedMemoryIds`.
    /// Idempotent: if the row is already gone, returns `Ok(false)` without touching
    /// neighbours.
    ///
    /// The `LIKE '%"<id>"%'` pre-filter narrows the scan; the quoted id inside the
    /// pattern prevents partial-UUID collisions across the JSON column (v4's
    /// `likeParam`). Each neighbour that actually lists the id is rewritten through
    /// `updateForCharacter` (character-scoped, bumps `updatedAt`); then the target
    /// row is deleted.
    pub fn delete_with_unlink(&self, memory_id: &str) -> Result<bool, DbError> {
        // v4: `findById` → if missing, false without touching neighbours.
        if !self.row_exists(memory_id)? {
            return Ok(false);
        }
        let like_param = format!("%\"{memory_id}\"%");
        let neighbours = self.scan_neighbours(
            "SELECT id, characterId, relatedMemoryIds FROM memories \
             WHERE relatedMemoryIds LIKE ?1 AND id != ?2",
            params![like_param, memory_id],
        )?;
        for (id, character_id, related_raw) in neighbours {
            let current = parse_related_ids(related_raw.as_deref());
            if !current.iter().any(|x| x == memory_id) {
                continue;
            }
            let filtered: Vec<String> = current.into_iter().filter(|x| x != memory_id).collect();
            let patch = MemUpdate {
                related_memory_ids: Some(filtered),
                ..Default::default()
            };
            self.update_for_character(&character_id, &id, &patch)?;
        }
        self.delete(memory_id)
    }

    /// v4's `deleteMemoriesWithUnlinkBatch` — the cascade chokepoint. Scans every
    /// row with a non-empty links array ONCE, scrubs every doomed id from each
    /// neighbour's `relatedMemoryIds` in one update per neighbour, then deletes the
    /// doomed set grouped by character (the repo's `bulkDelete` is
    /// characterId-scoped). Returns the number of rows actually deleted. Empty
    /// input → 0.
    pub fn delete_many_with_unlink(&self, memory_ids: &[String]) -> Result<i64, DbError> {
        if memory_ids.is_empty() {
            return Ok(0);
        }
        let doomed: std::collections::HashSet<&str> =
            memory_ids.iter().map(String::as_str).collect();

        // One-pass scan of every row with a non-empty links array (v4's stable
        // query shape, filtered in-JS regardless of batch size).
        let candidates = self.scan_neighbours(
            "SELECT id, characterId, relatedMemoryIds FROM memories \
             WHERE relatedMemoryIds IS NOT NULL AND relatedMemoryIds != '[]'",
            params![],
        )?;
        for (id, character_id, related_raw) in candidates {
            if doomed.contains(id.as_str()) {
                continue;
            }
            let current = parse_related_ids(related_raw.as_deref());
            if current.is_empty() {
                continue;
            }
            let filtered: Vec<String> = current
                .iter()
                .filter(|x| !doomed.contains(x.as_str()))
                .cloned()
                .collect();
            if filtered.len() == current.len() {
                continue;
            }
            let patch = MemUpdate {
                related_memory_ids: Some(filtered),
                ..Default::default()
            };
            self.update_for_character(&character_id, &id, &patch)?;
        }

        // Resolve id → characterId for the doomed set, group, `bulkDelete` per
        // character (final DB state is independent of group iteration order).
        let placeholders = (0..memory_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT id, characterId FROM memories WHERE id IN ({placeholders})");
        let mut by_character: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        {
            let mut stmt = self.conn.prepare(&sql)?;
            let p: Vec<&dyn ToSql> = memory_ids.iter().map(|s| s as &dyn ToSql).collect();
            let rows = stmt.query_map(p.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, character_id) = row?;
                by_character.entry(character_id).or_default().push(id);
            }
        }

        let mut deleted = 0i64;
        for (character_id, ids) in by_character {
            deleted += self.bulk_delete(&character_id, &ids)?;
        }
        Ok(deleted)
    }

    /// Run a `SELECT id, characterId, relatedMemoryIds` scan, collecting the raw
    /// `(id, characterId, relatedMemoryIds-text)` triples the unlink passes read.
    fn scan_neighbours(
        &self,
        sql: &str,
        p: &[&dyn ToSql],
    ) -> Result<Vec<(String, String, Option<String>)>, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// True iff a row with this id exists (v4's `_update` `findById` precondition).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM memories WHERE id = ?1", params![id], |r| {
                r.get::<_, i64>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}

/// Serialize a string list to compact JSON array text (`["a","b"]`, `[]`).
fn json_array(items: &[String]) -> Result<String, DbError> {
    serde_json::to_string(items).map_err(|e| DbError::Key(format!("json array serialize: {e}")))
}

/// v4's `parseRelatedIds` (`memory-gate.ts`) — parse a `relatedMemoryIds` cell to
/// its string elements. A null/empty cell, a non-array, or a parse failure → `[]`,
/// and non-string array elements are dropped (v4's `filter(x => typeof x ===
/// 'string')`).
fn parse_related_ids(raw: Option<&str>) -> Vec<String> {
    let raw = match raw {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => items
            .into_iter()
            .filter_map(|x| match x {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// Throwaway pepper for a fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const DDL: &str = "CREATE TABLE memories (
        id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
        projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
        importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
        sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
        lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
        createdAt TEXT, updatedAt TEXT);";

    /// Build a fresh encrypted DB, create the memories table, and seed the given
    /// `(id, characterId, relatedMemoryIds)` rows (all pinned timestamps).
    fn seed(rows: &[(&str, &str, &[&str])]) -> (TempDir, Writer) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        let w = Writer::open_writable(&path, PEPPER).unwrap();
        w.connection().execute_batch(DDL).unwrap();
        let repo = w.memories();
        for (id, character_id, related) in rows {
            repo.create(
                &MemCreate {
                    character_id: (*character_id).to_string(),
                    about_character_id: None,
                    chat_id: None,
                    project_id: None,
                    content: format!("content {id}"),
                    summary: format!("summary {id}"),
                    keywords: vec![],
                    tags: vec![],
                    importance: 0.5,
                    embedding: None,
                    source: "AUTO".to_string(),
                    witnessed_context: None,
                    source_message_id: None,
                    last_accessed_at: None,
                    reinforcement_count: 1.0,
                    last_reinforced_at: None,
                    related_memory_ids: related.iter().map(|s| s.to_string()).collect(),
                    reinforced_importance: 0.5,
                },
                &CreateOptions {
                    id: (*id).to_string(),
                    created_at: "2020-01-01T00:00:00.000Z".to_string(),
                    updated_at: "2020-01-01T00:00:00.000Z".to_string(),
                },
            )
            .unwrap();
        }
        (dir, w)
    }

    /// The `relatedMemoryIds` of `id`, or `None` if the row is gone.
    fn related_of(w: &Writer, id: &str) -> Option<Vec<String>> {
        let raw: Option<Option<String>> = w
            .connection()
            .query_row(
                "SELECT relatedMemoryIds FROM memories WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<String>>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .unwrap();
        raw.map(|cell| parse_related_ids(cell.as_deref()))
    }

    #[test]
    fn delete_with_unlink_scrubs_neighbours_and_deletes() {
        // mA1 is referenced by mA2 (same char) and mB1 (other char); mA3 never
        // references it.
        let (_dir, w) = seed(&[
            ("mA1", "charA", &["mA2", "mB1"]),
            ("mA2", "charA", &["mA1", "mA3"]),
            ("mA3", "charA", &["mA2"]),
            ("mB1", "charB", &["mA1"]),
        ]);
        let repo = w.memories();
        assert!(repo.delete_with_unlink("mA1").unwrap());
        assert_eq!(related_of(&w, "mA1"), None, "target deleted");
        assert_eq!(
            related_of(&w, "mA2"),
            Some(vec!["mA3".to_string()]),
            "scrubbed"
        );
        assert_eq!(
            related_of(&w, "mB1"),
            Some(vec![]),
            "cross-character neighbour scrubbed"
        );
        assert_eq!(
            related_of(&w, "mA3"),
            Some(vec!["mA2".to_string()]),
            "non-referencing row untouched"
        );
    }

    #[test]
    fn delete_with_unlink_missing_is_noop() {
        let (_dir, w) = seed(&[("mA2", "charA", &["mA1"])]);
        let repo = w.memories();
        assert!(!repo.delete_with_unlink("nope").unwrap());
        // Neighbour with a stale ref is left exactly as-is (no scan on a miss).
        assert_eq!(related_of(&w, "mA2"), Some(vec!["mA1".to_string()]));
    }

    #[test]
    fn delete_many_with_unlink_batch_scrubs_and_deletes_across_chars() {
        let (_dir, w) = seed(&[
            ("mA4", "charA", &["mA5"]),
            ("mA5", "charA", &["mA4", "mB2", "mA6"]),
            ("mA6", "charA", &[]),
            ("mB2", "charB", &["mA4"]),
            ("mB3", "charB", &["mB2"]),
        ]);
        let repo = w.memories();
        let deleted = repo
            .delete_many_with_unlink(&["mA4".to_string(), "mB2".to_string()])
            .unwrap();
        assert_eq!(deleted, 2, "two rows deleted across two characters");
        assert_eq!(related_of(&w, "mA4"), None);
        assert_eq!(related_of(&w, "mB2"), None);
        assert_eq!(
            related_of(&w, "mA5"),
            Some(vec!["mA6".to_string()]),
            "both doomed ids scrubbed in one update, survivor kept"
        );
        assert_eq!(related_of(&w, "mB3"), Some(vec![]));
        assert_eq!(
            related_of(&w, "mA6"),
            Some(vec![]),
            "empty-links row untouched"
        );
    }

    #[test]
    fn delete_many_with_unlink_empty_is_zero() {
        let (_dir, w) = seed(&[("mA1", "charA", &["mA2"])]);
        let repo = w.memories();
        assert_eq!(repo.delete_many_with_unlink(&[]).unwrap(), 0);
        assert_eq!(related_of(&w, "mA1"), Some(vec!["mA2".to_string()]));
    }
}
