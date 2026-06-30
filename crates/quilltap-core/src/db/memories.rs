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
//!     `help_docs`. Following them, **`update` never touches the BLOB**: the
//!     partial `UPDATE SET` never names `embedding`, so a text-only patch leaves
//!     the stored vector intact (matching v4's whole-row `$set` re-persisting the
//!     unchanged embedding).
//!   - `keywords` / `tags` / `relatedMemoryIds` are JSON-array columns (compact
//!     JSON text); the eight nullable-optional columns bind SQL NULL when absent.
//!
//! Determinism: `create` honors pinned id + timestamps (`CreateOptions`); the
//! mutators mint `updatedAt` (and `lastAccessedAt`) via [`crate::clock::now_iso`]
//! unless overridden — the harness placeholders those two columns on both dumps.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

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
/// NULL while `None` leaves the column untouched. `embedding` is deliberately
/// absent — the BLOB is never written through `update` (see module docs).
/// `updated_at` overrides the minted timestamp when `Some` (v4's
/// `'updatedAt' in data` branch); otherwise it is minted.
#[derive(Default)]
pub struct MemUpdate {
    pub content: Option<String>,
    pub summary: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
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
