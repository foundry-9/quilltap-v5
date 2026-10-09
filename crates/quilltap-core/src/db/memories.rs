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
use crate::chunk::{chunk_array, SQLITE_VARIABLE_CHUNK_SIZE};
use crate::clock::now_iso;
use crate::embedding_blob::float32_to_blob;
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;

/// v4's `SINGLE_DELETE_NEIGHBOUR_WARN` (`lib/memory/memory-gate.ts:474`) — the
/// warn-log threshold for single-memory deletions touching an unusually large
/// neighbour set. Either indicates a hub node (interesting) or a bug. Compared
/// with `>=`, and the warn is IN ADDITION to the debug, never instead of it.
const SINGLE_DELETE_NEIGHBOUR_WARN: usize = 20;

/// v4's `BATCH_DELETE_NEIGHBOUR_WARN` (`lib/memory/memory-gate.ts:481`) — the
/// batch twin. Batch cascades from large characters or chats are expected; the
/// warn fires when neighbour count outstrips even those. Compared with `>=`
/// against `neighboursTouched` (the rows actually rewritten), NOT the candidate
/// count the one-pass scan returned.
const BATCH_DELETE_NEIGHBOUR_WARN: usize = 200;

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
    /// Episodic spine (v4 8bf3cb5f): ISO wall-clock EVENT time (not the write
    /// clock — that's `createdAt`).
    pub occurred_at: Option<String>,
    /// Free-text in-story time for fictional-timeline chats.
    pub narrative_time: Option<String>,
    /// Proper nouns of the episode (places, people, named things). Distinct
    /// from `keywords`. Default `[]`.
    pub entities: Vec<String>,
    /// `'semantic'` | `'episodic'` (v4 default `'semantic'`).
    pub kind: String,
    pub source_message_id: Option<String>,
    pub last_accessed_at: Option<String>,
    pub reinforcement_count: f64,
    pub last_reinforced_at: Option<String>,
    pub related_memory_ids: Vec<String>,
    pub reinforced_importance: f64,
    /// P4.D265 (v4 `d58548051`): the storage tier — `"hot"` | `"cold"`. `tier`
    /// is `"hot"` and `consolidated_from` `[]` unless the caller says
    /// otherwise — v4's `create` prefill (`tier ?? 'hot'`, `consolidatedFrom ??
    /// []`, `memories.repository.ts:490-500`), which every caller states here.
    pub tier: String,
    /// The digest that replaced this row when it went cold (`None` → NULL).
    pub superseded_by_id: Option<String>,
    /// Digest rows only: the member ids folded into it (compact JSON text).
    pub consolidated_from: Vec<String>,
    /// Last time the consolidator considered the row (`None` → NULL).
    pub consolidated_at: Option<String>,
}

/// P4.D265 (v4 `70f9b495c`): what
/// [`MemoriesRepository::increment_reinforcement`] wrote — v4's `{
/// reinforcementCount, reinforcedImportance }`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reinforcement {
    pub reinforcement_count: f64,
    pub reinforced_importance: f64,
}

/// P4.D265 (v4 `d58548051`): the `extra` that rides along on
/// [`MemoriesRepository::update_tier_bulk`]'s write — v4's
/// `Partial<Pick<Memory, 'supersededById' | 'consolidatedAt'>>`. `None` leaves
/// the column untouched (key absent); `Some(None)` writes NULL (the gate's
/// promote: `{supersededById: null}`); `Some(Some(v))` writes `v`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TierExtra {
    pub superseded_by_id: Option<Option<String>>,
    pub consolidated_at: Option<Option<String>>,
}

/// P4.D265 (v4 `3f7320138` F8): the `countSource` field on both
/// `deleteMemoriesWithUnlinkBatch` lines. v4 renders `'resolved-ids'` only
/// where `bulkDelete` returned a non-number — inside the forked job child (a
/// buffered write) — and `'repository'` everywhere in the parent. v5 has no
/// child, so the label is a property of the v4 call PATH a v5 site mirrors
/// (R-C): `lib/memory/housekeeping.ts:397` run by `MEMORY_HOUSEKEEPING` and
/// `deleteMemoriesByChatIdWithVectors` run by `MEMORY_REGENERATE_CHAT` are
/// the child paths; every other caller is parent-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountSource {
    Repository,
    ResolvedIds,
}

impl CountSource {
    /// v4's literal.
    pub fn as_str(self) -> &'static str {
        match self {
            CountSource::Repository => "repository",
            CountSource::ResolvedIds => "resolved-ids",
        }
    }
}

/// v4's `_create` validation of a memory (`base.repository.ts:350-368` over
/// `MemorySchema`, `lib/schemas/memory.types.ts:54-108`) — the ONE parse the
/// `.qtap` import and the backup restore run BEFORE the write (P4.161 R-A,
/// dogfood #152). `item` is the data the caller hands `memories.create`
/// (unknown keys are stripped, never refused); the entity validated is
/// `{...item, id, createdAt, updatedAt}` with `id` the id the create will
/// write (`claimed_id` — `None` stands for a freshly minted one, which always
/// passes) and fresh stamps. `Err` carries the `ZodError.message` bytes
/// ([`zod_error_message`](crate::api::zod_issues::zod_error_message)) — the
/// `error` of v4's `Data validation failed` line and the tail of the
/// importer's / restorer's warning. `Ok` is the parsed entity as a
/// [`MemCreate`] with `MemorySchema`'s DEFAULTS applied to absent keys
/// (`importance` 0.5, `source` `MANUAL`, `kind` `semantic`,
/// `reinforcementCount` 1, `reinforcedImportance` 0.5, the four arrays `[]`
/// — R-C: the restore used to fill 5.0 / `AUTO` / 0 / 0).
///
/// Production `create` callers (memory extraction, the manual create) are NOT
/// gated by this yet — P4.161 Tier 3 item 13, by name.
pub fn parse_create_memory(item: &Value, claimed_id: Option<&str>) -> Result<MemCreate, String> {
    use crate::api::zod_issues::{zod_error_message, zod_memory_issues};
    let mut entity = item.as_object().cloned().unwrap_or_default();
    // P4.D265 (v4 `d58548051`): the repository's create prefill runs BEFORE
    // `MemorySchema` — `{...data, tier: data.tier ?? 'hot', consolidatedFrom:
    // data.consolidatedFrom ?? []}` — so an absent OR `null` tier /
    // consolidatedFrom becomes `'hot'` / `[]` (the raw schema refuses `null`).
    for (k, default) in [
        ("tier", Value::String("hot".into())),
        ("consolidatedFrom", Value::Array(Vec::new())),
    ] {
        if matches!(entity.get(k), None | Some(Value::Null)) {
            entity.insert(k.into(), default);
        }
    }
    let now = now_iso();
    entity.insert(
        "id".into(),
        Value::String(
            claimed_id
                .unwrap_or("00000000-0000-0000-0000-000000000000")
                .into(),
        ),
    );
    entity.insert("createdAt".into(), Value::String(now.clone()));
    entity.insert("updatedAt".into(), Value::String(now));
    let issues = zod_memory_issues(&entity);
    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    // Every key below has passed its schema check, so each read is total.
    let str_of = |k: &str| entity.get(k).and_then(Value::as_str).map(str::to_string);
    let arr = |k: &str| -> Vec<String> {
        entity
            .get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let num = |k: &str, default: f64| entity.get(k).and_then(Value::as_f64).unwrap_or(default);
    // The union's array option (a number[]); a string option is JSON-parsed as
    // v4's transform does. An empty vector is SQL NULL (`create_row`).
    let embedding = match entity.get("embedding") {
        Some(Value::Array(a)) => Some(
            a.iter()
                .filter_map(Value::as_f64)
                .map(|x| x as f32)
                .collect(),
        ),
        Some(Value::String(s)) => serde_json::from_str::<Vec<f64>>(s)
            .ok()
            .map(|a| a.into_iter().map(|x| x as f32).collect()),
        _ => None,
    };
    Ok(MemCreate {
        character_id: str_of("characterId").unwrap_or_default(),
        about_character_id: str_of("aboutCharacterId"),
        chat_id: str_of("chatId"),
        project_id: str_of("projectId"),
        content: str_of("content").unwrap_or_default(),
        summary: str_of("summary").unwrap_or_default(),
        keywords: arr("keywords"),
        tags: arr("tags"),
        importance: num("importance", 0.5),
        embedding,
        source: str_of("source").unwrap_or_else(|| "MANUAL".into()),
        witnessed_context: str_of("witnessedContext"),
        occurred_at: str_of("occurredAt"),
        narrative_time: str_of("narrativeTime"),
        entities: arr("entities"),
        kind: str_of("kind").unwrap_or_else(|| "semantic".into()),
        source_message_id: str_of("sourceMessageId"),
        last_accessed_at: str_of("lastAccessedAt"),
        reinforcement_count: num("reinforcementCount", 1.0),
        last_reinforced_at: str_of("lastReinforcedAt"),
        related_memory_ids: arr("relatedMemoryIds"),
        reinforced_importance: num("reinforcedImportance", 0.5),
        tier: str_of("tier").unwrap_or_else(|| "hot".into()),
        superseded_by_id: str_of("supersededById"),
        consolidated_from: arr("consolidatedFrom"),
        consolidated_at: str_of("consolidatedAt"),
    })
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
    /// Episodic spine (v4 8bf3cb5f) — nullable event-time / in-story-time.
    pub occurred_at: Option<Option<String>>,
    pub narrative_time: Option<Option<String>>,
    /// Episodic entities (JSON string[] column, like `keywords`).
    pub entities: Option<Vec<String>>,
    /// `'semantic'` | `'episodic'`.
    pub kind: Option<String>,
    pub source_message_id: Option<Option<String>>,
    pub last_accessed_at: Option<Option<String>>,
    pub last_reinforced_at: Option<Option<String>>,
    /// `None` → leave the BLOB untouched; `Some(Some(v))` → Float32 LE bytes
    /// (empty → NULL); `Some(None)` → SQL NULL.
    pub embedding: Option<Option<Vec<f32>>>,
    /// P4.D265 (v4 `d58548051`): the tier columns — `tier` `'hot'` | `'cold'`,
    /// the nullable `supersededById` / `consolidatedAt` (`Some(None)` → NULL),
    /// `consolidatedFrom` a JSON string[] column like `relatedMemoryIds`.
    pub tier: Option<String>,
    pub superseded_by_id: Option<Option<String>>,
    pub consolidated_from: Option<Vec<String>>,
    pub consolidated_at: Option<Option<String>>,
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
    ///
    /// P4.149: a failure logs v4's two RETHROW lines — the base `_create`'s
    /// `Error creating entity` (through `db::fallback`) then the repository's
    /// own `Error creating memory {collection, characterId}`
    /// (`memories.repository.ts:418-430`) — and propagates.
    pub fn create(&self, data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.create_row(data, opts).inspect_err(|e| {
            super::fallback::log_create_failure("memories", e);
            super::fallback::log_memory_create_failure(&data.character_id, e);
        })
    }

    fn create_row(&self, data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        let keywords_json = json_array(&data.keywords)?;
        let tags_json = json_array(&data.tags)?;
        let related_json = json_array(&data.related_memory_ids)?;
        let entities_json = json_array(&data.entities)?;
        let consolidated_from_json = json_array(&data.consolidated_from)?;

        self.conn.execute(
            "INSERT INTO memories \
               (id, characterId, aboutCharacterId, chatId, projectId, content, summary, \
                keywords, tags, importance, embedding, source, witnessedContext, \
                occurredAt, narrativeTime, entities, kind, \
                sourceMessageId, lastAccessedAt, reinforcementCount, lastReinforcedAt, \
                relatedMemoryIds, reinforcedImportance, \
                tier, supersededById, consolidatedFrom, consolidatedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
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
                data.occurred_at,
                data.narrative_time,
                entities_json,
                data.kind,
                data.source_message_id,
                data.last_accessed_at,
                data.reinforcement_count,
                data.last_reinforced_at,
                related_json,
                data.reinforced_importance,
                data.tier,
                data.superseded_by_id,
                consolidated_from_json,
                data.consolidated_at,
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
    ///
    /// P4.149: v4's `_update` reads the row first through the FALLBACK
    /// `findById` — a failed read logs `Error finding entity by ID` and answers
    /// `null` → `Ok(false)` (its `Entity not found for update` WARN is not
    /// ported — Ruling R-A); a failed WRITE logs the base `Error updating entity`
    /// then the repository's `Error updating memory {collection, memoryId}`
    /// (`:437-447`) and propagates.
    pub fn update(&self, id: &str, patch: &MemUpdate) -> Result<bool, DbError> {
        let exists = super::fallback::find_by_id_or_none("memories", id, || {
            self.row_exists(id).map(|found| found.then_some(()))
        });
        if exists.is_none() {
            return Ok(false);
        }
        self.update_row(id, patch).inspect_err(|e| {
            super::fallback::log_update_failure("memories", id, e);
            super::fallback::log_memory_update_failure(id, e);
        })
    }

    fn update_row(&self, id: &str, patch: &MemUpdate) -> Result<bool, DbError> {
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
        if let Some(v) = &patch.occurred_at {
            set_col!("occurredAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.narrative_time {
            set_col!("narrativeTime", Box::new(v.clone()));
        }
        if let Some(v) = &patch.entities {
            set_col!("entities", Box::new(json_array(v)?));
        }
        if let Some(v) = &patch.kind {
            set_col!("kind", Box::new(v.clone()));
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
        if let Some(v) = &patch.tier {
            set_col!("tier", Box::new(v.clone()));
        }
        if let Some(v) = &patch.superseded_by_id {
            set_col!("supersededById", Box::new(v.clone()));
        }
        if let Some(v) = &patch.consolidated_from {
            set_col!("consolidatedFrom", Box::new(json_array(v)?));
        }
        if let Some(v) = &patch.consolidated_at {
            set_col!("consolidatedAt", Box::new(v.clone()));
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
    ///
    /// P4.149: a failure logs the base `Error deleting entity` then the
    /// repository's `Error deleting memory {collection, memoryId}`
    /// (`memories.repository.ts:486-497`) and propagates.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        self.conn
            .execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map(|n| n > 0)
            .map_err(|e| {
                let e = DbError::from(e);
                super::fallback::log_delete_failure("memories", id, &e);
                super::fallback::log_memory_delete_failure(id, &e);
                e
            })
    }

    /// `updateForCharacter` — update only if the memory exists AND belongs to
    /// `character_id` (else a no-op returning `Ok(false)`).
    ///
    /// P4.149 (P4.144 item 11): v4's ownership read is the FALLBACK `findById`
    /// (`memories.repository.ts:454-478`), so a failed read logs `Error finding
    /// entity by ID {collection: memories, id}`, answers `null` and takes the
    /// not-found arm — WARN `Memory not found for update {memoryId, characterId}`
    /// and `Ok(false)`: the caller CONTINUES (v5's `?` had stopped the fold's
    /// character). An owner mismatch WARNs `Memory does not belong to character
    /// {characterId, memoryId}`. A failed update adds the outer RETHROW line
    /// `Error updating memory for character {collection, characterId,
    /// memoryId}` beneath [`Self::update`]'s two and propagates.
    pub fn update_for_character(
        &self,
        character_id: &str,
        memory_id: &str,
        patch: &MemUpdate,
    ) -> Result<bool, DbError> {
        match super::fallback::find_by_id_or_none("memories", memory_id, || {
            self.character_id_of(memory_id)
        }) {
            None => {
                tracing::warn!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    "Memory not found for update"
                );
                return Ok(false);
            }
            Some(owner) if owner != character_id => {
                tracing::warn!(
                    target: "quilltap::db",
                    characterId = %character_id,
                    memoryId = %memory_id,
                    "Memory does not belong to character"
                );
                return Ok(false);
            }
            Some(_) => {}
        }
        self.update(memory_id, patch).inspect_err(|e| {
            super::fallback::log_memory_update_for_character_failure(character_id, memory_id, e);
        })
    }

    /// `deleteForCharacter` — delete only if owned by `character_id`.
    ///
    /// P4.149: v4's shape as [`Self::update_for_character`]
    /// (`memories.repository.ts:503-522`) — `Memory not found for deletion
    /// {memoryId, characterId}` / `Memory does not belong to character`, and the
    /// outer `Error deleting memory for character` on a failed delete.
    pub fn delete_for_character(
        &self,
        character_id: &str,
        memory_id: &str,
    ) -> Result<bool, DbError> {
        match super::fallback::find_by_id_or_none("memories", memory_id, || {
            self.character_id_of(memory_id)
        }) {
            None => {
                tracing::warn!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    "Memory not found for deletion"
                );
                return Ok(false);
            }
            Some(owner) if owner != character_id => {
                tracing::warn!(
                    target: "quilltap::db",
                    characterId = %character_id,
                    memoryId = %memory_id,
                    "Memory does not belong to character"
                );
                return Ok(false);
            }
            Some(_) => {}
        }
        self.delete(memory_id).inspect_err(|e| {
            super::fallback::log_memory_delete_for_character_failure(character_id, memory_id, e);
        })
    }

    /// `bulkDelete` — `deleteMany({ characterId, id: { $in } })`; empty → 0.
    ///
    /// **Chunked** (v4 `805ef12bf`): v4's query translator expands `$in` to one
    /// bind variable per id and this one binds them directly, so a full-wipe
    /// cascade can exceed `SQLITE_MAX_VARIABLE_NUMBER` (999 on older builds,
    /// 32766 on current ones) and the whole statement fails with "too many SQL
    /// variables" instead of deleting. The chunks are summed, exactly as v4
    /// accumulates `deletedCount`.
    pub fn bulk_delete(&self, character_id: &str, memory_ids: &[String]) -> Result<i64, DbError> {
        if memory_ids.is_empty() {
            return Ok(0);
        }
        let mut deleted = 0i64;
        for chunk in chunk_array(memory_ids, SQLITE_VARIABLE_CHUNK_SIZE) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql =
                format!("DELETE FROM memories WHERE characterId = ? AND id IN ({placeholders})");
            let mut p: Vec<&dyn ToSql> = Vec::with_capacity(1 + chunk.len());
            p.push(&character_id);
            for id in chunk {
                p.push(id);
            }
            deleted += self.conn.execute(&sql, p.as_slice())? as i64;
        }
        Ok(deleted)
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

    /// P4.D265 (v4 `70f9b495c`, bug 182) — v4 `incrementReinforcement`
    /// (`memories.repository.ts:571-606`): count one more reinforcement of
    /// `memory_id` (scoped to `character_id`) as a read-then-write inside ONE
    /// transaction, so two concurrent reinforcements both count. `updatedAt`
    /// is minted BEFORE the transaction (v4 reads `getCurrentTimestamp()`
    /// first); `lastReinforcedAt` is `at`.
    ///
    /// Inside a SAVEPOINT (v4's `db.transaction`, which nests when the job
    /// applier's `BEGIN IMMEDIATE` is open): `SELECT "importance",
    /// "reinforcementCount"` for the id AND character; none → `Ok(None)` + WARN
    /// `Memory not found for reinforcement {memoryId, characterId}`; else
    /// `count = (reinforcementCount ?? 1) + 1`, `importance =
    /// calculate_reinforced_importance(importance ?? 0.5, count)`, one UPDATE
    /// keyed by `id` alone, and DEBUG `Memory reinforcement counted {memoryId,
    /// characterId, reinforcementCount, reinforcedImportance}`. A failure is
    /// v4's 3-argument RETHROW: ERROR `Error reinforcing memory` (the
    /// `db::fallback` home), then the `Err` propagates. NO-PORT (§R.4(o)):
    /// the `getRawDatabase()` null throw and the job child's buffered
    /// `increment*` arm — v5 has no child.
    pub fn increment_reinforcement(
        &self,
        character_id: &str,
        memory_id: &str,
        at: &str,
    ) -> Result<Option<Reinforcement>, DbError> {
        let now = now_iso();
        let counted = self
            .reinforce_in_savepoint(character_id, memory_id, at, &now)
            .inspect_err(|e| {
                super::fallback::log_memory_reinforce_failure(character_id, memory_id, e);
            })?;
        match counted {
            None => {
                tracing::warn!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    "Memory not found for reinforcement"
                );
            }
            Some(r) => {
                tracing::debug!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    reinforcementCount = r.reinforcement_count,
                    reinforcedImportance = r.reinforced_importance,
                    "Memory reinforcement counted"
                );
            }
        }
        Ok(counted)
    }

    /// The transaction body of [`Self::increment_reinforcement`].
    fn reinforce_in_savepoint(
        &self,
        character_id: &str,
        memory_id: &str,
        at: &str,
        now: &str,
    ) -> Result<Option<Reinforcement>, DbError> {
        self.conn
            .execute_batch("SAVEPOINT \"memory_reinforcement\"")?;
        let work = || -> Result<Option<Reinforcement>, DbError> {
            let row: Option<(Option<f64>, Option<f64>)> = self
                .conn
                .query_row(
                    "SELECT \"importance\", \"reinforcementCount\" FROM \"memories\" \
                     WHERE \"id\" = ? AND \"characterId\" = ?",
                    params![memory_id, character_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            let Some((importance, count)) = row else {
                return Ok(None);
            };
            let reinforcement_count = count.unwrap_or(1.0) + 1.0;
            let reinforced_importance = crate::memory_gate::calculate_reinforced_importance(
                importance.unwrap_or(0.5),
                reinforcement_count,
            );
            self.conn.execute(
                "UPDATE \"memories\" SET \"reinforcementCount\" = ?, \"reinforcedImportance\" = ?, \
                 \"lastReinforcedAt\" = ?, \"updatedAt\" = ? WHERE \"id\" = ?",
                params![reinforcement_count, reinforced_importance, at, now, memory_id],
            )?;
            Ok(Some(Reinforcement {
                reinforcement_count,
                reinforced_importance,
            }))
        };
        match work() {
            Ok(v) => {
                self.conn
                    .execute_batch("RELEASE \"memory_reinforcement\"")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch(
                    "ROLLBACK TO \"memory_reinforcement\"; RELEASE \"memory_reinforcement\"",
                );
                Err(e)
            }
        }
    }

    /// P4.D265 (v4 `d58548051`) — v4 `updateTierBulk`
    /// (`memories.repository.ts:874-905`): move `ids` of `character_id` to
    /// `tier`, `extra` riding on the same write. Empty → `Ok(0)` with no line.
    /// Per `chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE)` one `UPDATE memories
    /// SET tier = ?[, supersededById = ?][, consolidatedAt = ?], updatedAt = ?
    /// WHERE characterId = ? AND id IN (…)` (ONE `updatedAt` for the call — v4's
    /// base `updateMany` re-stamps it per chunk, a sub-millisecond difference
    /// the differentials normalize); a stray id of another character is never
    /// touched. DEBUG `Moved memories between tiers {characterId, tier,
    /// requested, changed, supersededById}` — `supersededById` PRESENT only for
    /// `Some(Some(_))` (v4's `extra.supersededById ?? undefined` omits it for
    /// both a demotion and a promotion's `null`). A failure logs TWO ERRORs
    /// (the base `updateMany`'s `Error updating entities`, then `Error moving
    /// memories between tiers {characterId, tier, count}`) and answers
    /// `Ok(0)` — strict-aware (`db::fallback::tier_move_or_zero`). Callers
    /// keep the vector store in step (`CharacterVectorStore::set_tier`).
    pub fn update_tier_bulk(
        &self,
        character_id: &str,
        ids: &[String],
        tier: &str,
        extra: TierExtra,
    ) -> Result<i64, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        super::fallback::tier_move_or_zero(character_id, tier, ids.len(), || {
            let now = now_iso();
            let mut changed = 0i64;
            for chunk in chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE) {
                let mut sets = vec!["tier = ?"];
                let mut p: Vec<&dyn ToSql> = vec![&tier];
                if let Some(v) = &extra.superseded_by_id {
                    sets.push("supersededById = ?");
                    p.push(v);
                }
                if let Some(v) = &extra.consolidated_at {
                    sets.push("consolidatedAt = ?");
                    p.push(v);
                }
                sets.push("updatedAt = ?");
                p.push(&now);
                p.push(&character_id);
                for id in chunk {
                    p.push(id);
                }
                let sql = format!(
                    "UPDATE memories SET {} WHERE characterId = ? AND id IN ({})",
                    sets.join(", "),
                    vec!["?"; chunk.len()].join(", ")
                );
                changed += self.conn.execute(&sql, p.as_slice())? as i64;
            }
            // Inside the operation, after the loop — so a failed write (the
            // fallback's `Ok(0)`) logs no DEBUG, as v4's does not.
            if let Some(Some(digest)) = &extra.superseded_by_id {
                tracing::debug!(
                    target: "quilltap::db",
                    characterId = %character_id,
                    tier = %tier,
                    requested = ids.len(),
                    changed = changed,
                    supersededById = %digest,
                    "Moved memories between tiers"
                );
            } else {
                tracing::debug!(
                    target: "quilltap::db",
                    characterId = %character_id,
                    tier = %tier,
                    requested = ids.len(),
                    changed = changed,
                    "Moved memories between tiers"
                );
            }
            Ok(changed)
        })
    }

    /// P4.D265 (v4 `d58548051`) — v4 `markConsidered`
    /// (`memories.repository.ts:911-929`): stamp `consolidatedAt = when` on
    /// `ids` of `character_id`, chunked. v4's base `updateMany` ALSO stamps
    /// `updatedAt = now` — which feeds `find_expired_cold_ids`' `updatedAt <`
    /// window. No debug line; empty → `Ok(0)`. A failure logs the two ERRORs
    /// (`Error updating entities`, then `Error marking memories considered
    /// {characterId, count}`) and answers `Ok(0)` (strict-aware).
    pub fn mark_considered(
        &self,
        character_id: &str,
        ids: &[String],
        when: &str,
    ) -> Result<i64, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        super::fallback::memories_considered_or_zero(character_id, ids.len(), || {
            let now = now_iso();
            let mut changed = 0i64;
            for chunk in chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE) {
                let mut p: Vec<&dyn ToSql> = vec![&when, &now, &character_id];
                for id in chunk {
                    p.push(id);
                }
                let sql = format!(
                    "UPDATE memories SET consolidatedAt = ?, updatedAt = ? \
                     WHERE characterId = ? AND id IN ({})",
                    vec!["?"; chunk.len()].join(", ")
                );
                changed += self.conn.execute(&sql, p.as_slice())? as i64;
            }
            Ok(changed)
        })
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
        // v4 takes its clock BEFORE the `findById` precondition, but returns from
        // that precondition without logging — so the early exit narrates nothing
        // on either side.
        let started_at = crate::clock::now_unix_ms();
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
        let neighbour_count = neighbours.len();
        let mut characters_affected: std::collections::HashSet<String> =
            std::collections::HashSet::new();
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
            characters_affected.insert(character_id);
        }
        let deleted = self.delete(memory_id)?;

        // v4 `memory-gate.ts:538-548`. Log-only (the finding-#103/#110/#116
        // class): the row set and the returned bool are identical whether or not
        // these fire, so their only proof is the capturing layer in this module's
        // tests. `durationMs` is a REAL wall-clock read — v4's
        // `Date.now() - startedAt` — never the pinned harness clock.
        let duration_ms = crate::clock::now_unix_ms() - started_at;
        if neighbour_count >= SINGLE_DELETE_NEIGHBOUR_WARN {
            tracing::warn!(
                memoryId = memory_id,
                neighbourCount = neighbour_count,
                charactersAffected = characters_affected.len(),
                durationMs = duration_ms,
                "[MemoryGate] deleteMemoryWithUnlink touched an unusually large neighbour set"
            );
        }
        tracing::debug!(
            memoryId = memory_id,
            neighbourCount = neighbour_count,
            charactersAffected = characters_affected.len(),
            durationMs = duration_ms,
            "[MemoryGate] deleteMemoryWithUnlink complete"
        );

        // Bug 128 (`4a9be9878`). Collection-wide: this takes a memory id, not a
        // chat id, so there is no narrower hint to give. A no-op from the job
        // child by design — the parent chokepoints announce a child's deletions.
        //
        // The five-line logging gap `4a9be9878` widened (the two `complete`
        // debugs, the two neighbour-set WARNs, and `handleDeleteByChatId`'s
        // `[Memories API] Deleted every memory for a chat`) is CLOSED — P4.88.
        // The four gate lines are emitted above and in the batch twin; the route
        // line lives in `api/memories.rs`.
        //
        // v4 publishes from the GATE (`lib/memory/memory-gate.ts`), whose v5
        // twin is this repo method, so every one of the eight callers
        // (`api/memories.rs`, `memory_service` ×2, `character_archive::service`,
        // `cascade_delete`, `memory_dedup`, `delete_all`, `housekeeping`) is
        // covered by construction — including the by-chat delete route, which
        // deliberately publishes NOTHING of its own.
        if deleted {
            publish_realtime(RealtimeTopic::Memories, None);
        }
        Ok(deleted)
    }

    /// v4's `deleteMemoriesWithUnlinkBatch` — the cascade chokepoint. Scans every
    /// row with a non-empty links array ONCE, scrubs every doomed id from each
    /// neighbour's `relatedMemoryIds` in one update per neighbour, then deletes the
    /// doomed set grouped by character (the repo's `bulkDelete` is
    /// characterId-scoped). Returns the number of rows actually deleted. Empty
    /// input → 0.
    ///
    /// P4.D265 (F8): `count_source` is v4's `countSource` label on both lines
    /// ([`CountSource`] — the v4 call path this site mirrors, R-C). The count
    /// itself is unchanged: v4's child counts the resolved ids, which equal
    /// the rows the commit deletes.
    pub fn delete_many_with_unlink(
        &self,
        memory_ids: &[String],
        count_source: CountSource,
    ) -> Result<i64, DbError> {
        // v4's empty short circuit PRECEDES its `const startedAt = Date.now()`,
        // so an empty batch narrates nothing at all.
        if memory_ids.is_empty() {
            return Ok(0);
        }
        let started_at = crate::clock::now_unix_ms();
        let doomed: std::collections::HashSet<&str> =
            memory_ids.iter().map(String::as_str).collect();

        // One-pass scan of every row with a non-empty links array (v4's stable
        // query shape, filtered in-JS regardless of batch size).
        let candidates = self.scan_neighbours(
            "SELECT id, characterId, relatedMemoryIds FROM memories \
             WHERE relatedMemoryIds IS NOT NULL AND relatedMemoryIds != '[]'",
            params![],
        )?;
        let mut characters_affected: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut neighbours_touched = 0usize;
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
            characters_affected.insert(character_id);
            neighbours_touched += 1;
        }

        // Resolve id → characterId for the doomed set, group, `bulkDelete` per
        // character (final DB state is independent of group iteration order).
        //
        // **Chunked** (v4 `805ef12bf`): each id is one bind variable, and a
        // full-wipe cascade can pass more ids than `SQLITE_MAX_VARIABLE_NUMBER`
        // allows in a single statement. Every chunk accumulates into the SAME
        // map, so the grouping is unchanged.
        let mut by_character: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for chunk in chunk_array(memory_ids, SQLITE_VARIABLE_CHUNK_SIZE) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!("SELECT id, characterId FROM memories WHERE id IN ({placeholders})");
            let mut stmt = self.conn.prepare(&sql)?;
            let p: Vec<&dyn ToSql> = chunk.iter().map(|s| s as &dyn ToSql).collect();
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

        // v4 `memory-gate.ts:623-633` — the batch twin of the single-id pair.
        // `requested` counts the ids handed in (ghosts included); `deleted` the
        // rows that existed. The WARN compares `neighboursTouched`, not the
        // candidate count the one-pass scan returned.
        let duration_ms = crate::clock::now_unix_ms() - started_at;
        if neighbours_touched >= BATCH_DELETE_NEIGHBOUR_WARN {
            tracing::warn!(
                requested = memory_ids.len(),
                deleted = deleted,
                countSource = count_source.as_str(),
                neighboursTouched = neighbours_touched,
                charactersAffected = characters_affected.len(),
                durationMs = duration_ms,
                "[MemoryGate] deleteMemoriesWithUnlinkBatch touched an unusually large neighbour set"
            );
        }
        tracing::debug!(
            requested = memory_ids.len(),
            deleted = deleted,
            countSource = count_source.as_str(),
            neighboursTouched = neighbours_touched,
            charactersAffected = characters_affected.len(),
            durationMs = duration_ms,
            "[MemoryGate] deleteMemoriesWithUnlinkBatch complete"
        );

        // Bug 128: collection-wide for the same reason as the single-id path,
        // and doubly so — a batch can span several chats.
        if deleted > 0 {
            publish_realtime(RealtimeTopic::Memories, None);
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
    serde_json::to_string(items)
        .map_err(|e| DbError::Internal(format!("json array serialize: {e}")))
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

    /// P4.161 (R-C): `parse_create_memory` applies `MemorySchema`'s DEFAULTS
    /// to absent keys — importance 0.5, `MANUAL`, `semantic`, 1, 0.5 — where
    /// the restore used to fill 5.0 / `AUTO` / 0 / 0. The defaults are the
    /// `repository_zod_messages` oracle's parsed rows; this pins them unit-side.
    #[test]
    fn parse_create_memory_applies_the_schema_defaults() {
        let item = serde_json::json!({
            "characterId": "c1610000-0000-4000-8000-0000000000b1",
            "content": "c",
            "summary": "s",
        });
        let m = parse_create_memory(&item, None).expect("a minimal memory parses");
        assert_eq!(
            (m.importance, m.source.as_str(), m.kind.as_str()),
            (0.5, "MANUAL", "semantic")
        );
        assert_eq!((m.reinforcement_count, m.reinforced_importance), (1.0, 0.5));
        assert!(m.keywords.is_empty() && m.tags.is_empty() && m.entities.is_empty());
        assert!(m.related_memory_ids.is_empty() && m.embedding.is_none());
    }

    /// P4.161 (dogfood #152): the walk's C6 shape is REFUSED with both issues,
    /// in schema order, as v4's `ZodError.message` — and a claimed id that is
    /// not a uuid is refused too (the restore preserves ids).
    #[test]
    fn parse_create_memory_refuses_the_c6_shape_and_a_bad_claimed_id() {
        let item = serde_json::json!({
            "characterId": "c1610000-0000-4000-8000-0000000000b1",
            "content": "c",
            "summary": "s",
            "importance": 5,
            "kind": "bogus-kind",
        });
        let err = parse_create_memory(&item, None).err().expect("refused");
        let issues: Value = serde_json::from_str(&err).unwrap();
        let paths: Vec<&Value> = issues
            .as_array()
            .unwrap()
            .iter()
            .map(|i| &i["path"])
            .collect();
        assert_eq!(
            paths,
            [
                &serde_json::json!(["importance"]),
                &serde_json::json!(["kind"])
            ]
        );
        let ok = serde_json::json!({
            "characterId": "c1610000-0000-4000-8000-0000000000b1",
            "content": "c",
            "summary": "s",
        });
        let err = parse_create_memory(&ok, Some("not-a-uuid"))
            .err()
            .expect("refused");
        assert!(
            err.contains("\"Invalid UUID\"") && err.contains("\"id\""),
            "{err}"
        );
    }
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// Throwaway pepper for a fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const DDL: &str = "CREATE TABLE memories (
        id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
        projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
        importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
        sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
        lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
        tier TEXT DEFAULT 'hot', supersededById TEXT, consolidatedFrom TEXT DEFAULT '[]',
        consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);";

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
                    occurred_at: None,
                    narrative_time: None,
                    entities: Vec::new(),
                    kind: "semantic".to_string(),
                    source_message_id: None,
                    last_accessed_at: None,
                    reinforcement_count: 1.0,
                    last_reinforced_at: None,
                    related_memory_ids: related.iter().map(|s| s.to_string()).collect(),
                    reinforced_importance: 0.5,
                    tier: "hot".to_string(),
                    superseded_by_id: None,
                    consolidated_from: Vec::new(),
                    consolidated_at: None,
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
            .delete_many_with_unlink(
                &["mA4".to_string(), "mB2".to_string()],
                CountSource::Repository,
            )
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
        assert_eq!(
            repo.delete_many_with_unlink(&[], CountSource::Repository)
                .unwrap(),
            0
        );
        assert_eq!(related_of(&w, "mA1"), Some(vec!["mA2".to_string()]));
    }

    // ── P4.D126 unit 2: the SQLite bind-variable ceiling (v4 `805ef12bf`) ────
    //
    // RED-FIRST, measured 2026-08-26 before the chunking landed: both sites
    // answered
    //   Sqlite(SqliteFailure(Error { code: Unknown, extended_code: 1 },
    //                        Some("too many SQL variables")))
    // on a 40,000-id batch. SQLite3MC is the same engine v4 links, so the
    // ceiling (32,766 here) and v4's 900-id budget are the same numbers.

    /// A batch far past `SQLITE_MAX_VARIABLE_NUMBER` must still delete the rows
    /// it names — the ids that exist — and count them.
    #[test]
    fn bulk_delete_chunks_past_the_variable_limit() {
        let (_dir, w) = seed(&[
            ("mA1", "charA", &[]),
            ("mA2", "charA", &[]),
            ("mB1", "charB", &[]),
        ]);
        let repo = w.memories();
        // 40,000 > 32,766: one statement per id list is impossible.
        let mut ids: Vec<String> = (0..40_000).map(|i| format!("ghost-{i}")).collect();
        // Spread the REAL ids across three different chunks (0, 900, 39_999) so a
        // port that only ever ran the first chunk would miss two of them.
        ids[0] = "mA1".to_string();
        ids[SQLITE_VARIABLE_CHUNK_SIZE] = "mA2".to_string();
        ids[39_999] = "mB1".to_string();

        let deleted = repo
            .bulk_delete("charA", &ids)
            .expect("a chunked bulk_delete must not hit the variable limit");
        assert_eq!(deleted, 2, "only charA's two rows are in scope");
        assert_eq!(related_of(&w, "mA1"), None);
        assert_eq!(related_of(&w, "mA2"), None);
        assert!(
            related_of(&w, "mB1").is_some(),
            "the characterId scope still holds across chunks"
        );
    }

    /// The chokepoint's doomed-id → character resolve, same ceiling. Every chunk
    /// accumulates into the SAME map, so the by-character grouping — and the
    /// neighbour scrub that runs before it — are unchanged.
    #[test]
    fn delete_many_with_unlink_chunks_past_the_variable_limit() {
        let (_dir, w) = seed(&[
            ("mA1", "charA", &[]),
            ("mA2", "charA", &[]),
            ("mB1", "charB", &[]),
            ("mB2", "charB", &["mA1", "mB1"]),
        ]);
        let repo = w.memories();
        let mut ids: Vec<String> = (0..40_000).map(|i| format!("ghost-{i}")).collect();
        ids[0] = "mA1".to_string();
        ids[SQLITE_VARIABLE_CHUNK_SIZE] = "mA2".to_string();
        ids[39_999] = "mB1".to_string();

        let deleted = repo
            .delete_many_with_unlink(&ids, CountSource::Repository)
            .expect("a chunked resolve must not hit the variable limit");
        assert_eq!(deleted, 3, "all three real rows, across two characters");
        assert_eq!(related_of(&w, "mA1"), None);
        assert_eq!(related_of(&w, "mA2"), None);
        assert_eq!(related_of(&w, "mB1"), None);
        assert_eq!(
            related_of(&w, "mB2"),
            Some(vec![]),
            "the surviving neighbour is still scrubbed of BOTH doomed ids"
        );
    }

    /// v4's own pin at both sites is a **2,000-id batch** — under the current
    /// ceiling, over the 900 budget, so it proves the chunking runs rather than
    /// merely that nothing threw. Rows sit either side of the first boundary.
    #[test]
    fn a_two_thousand_id_batch_crosses_the_chunk_boundary() {
        let (_dir, w) = seed(&[("mA1", "charA", &[]), ("mA2", "charA", &[])]);
        let repo = w.memories();
        let mut ids: Vec<String> = (0..2_000).map(|i| format!("ghost-{i}")).collect();
        // 899 is the last id of chunk 1; 900 is the first of chunk 2.
        ids[SQLITE_VARIABLE_CHUNK_SIZE - 1] = "mA1".to_string();
        ids[SQLITE_VARIABLE_CHUNK_SIZE] = "mA2".to_string();
        assert_eq!(repo.bulk_delete("charA", &ids).unwrap(), 2);
        assert_eq!(related_of(&w, "mA1"), None);
        assert_eq!(related_of(&w, "mA2"), None);
    }

    // -----------------------------------------------------------------------
    // The deletion chokepoint's four log lines (P4.88 — v4
    // `lib/memory/memory-gate.ts:545-548` and `:630-633`).
    //
    // Log-only: every `memories` row and every returned count is identical
    // whether or not these fire, so the ONLY proof is a capturing layer over the
    // real repository method (the finding-#103/#110/#116 class). Each test
    // asserts the line's whole field set AND the discriminating values;
    // `durationMs` is a real wall-clock read, so it is asserted present and
    // non-negative rather than pinned.
    // -----------------------------------------------------------------------

    /// The `durationMs=<n>` a captured line carries, parsed.
    fn duration_ms_of(line: &str) -> i64 {
        let tail = line
            .split(" durationMs=")
            .nth(1)
            .unwrap_or_else(|| panic!("no durationMs field in {line:?}"));
        tail.split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("durationMs is not an integer in {line:?}"))
    }

    /// Every captured line whose message contains `needle`.
    fn lines_with<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(needle)).collect()
    }

    /// P4.93 — the five capture tests below go through THIS, not
    /// `test_support::captured` directly.
    ///
    /// `captured` installs a THREAD-scoped subscriber, and `tracing` caches each
    /// callsite's `Interest` **globally** on first use. The two sibling tests
    /// above (`delete_with_unlink_scrubs_neighbours_and_deletes`,
    /// `delete_with_unlink_missing_is_noop`) reach `delete_with_unlink` with NO
    /// subscriber armed on their thread; if one of them touches the
    /// `deleteMemoryWithUnlink complete` callsite first, the interest cache can
    /// retire it for the whole binary and the capture below sees nothing. That
    /// is exactly what `delete_with_unlink_logs_v4s_complete_debug` did once in
    /// a full workspace run ("exactly one complete line: []") while passing
    /// alone and twice in its own binary.
    ///
    /// The fix is to keep a dispatcher permanently registered, so every rebuild
    /// of the interest cache (one happens on each `set_default`) ANDs in a
    /// subscriber that says `always`. **Which** dispatcher is not a free choice,
    /// though — and that is the part worth reading twice.
    ///
    /// `quilltap-host`'s `lock.rs:1026-1040` fixes this exact symptom by arming
    /// an EMPTY `tracing_subscriber::registry()`, and its comment is careful to
    /// say why it may: "No global default exists anywhere else in
    /// `quilltap-host`." That is FALSE here. `quilltap-core` already has one —
    /// `test_support::global_capture` installs `GlobalCaptureLayer` for
    /// `job_runner`'s smoke test — and `set_global_default` succeeds exactly
    /// once per process. Arming an empty registry from this module therefore
    /// WINS THE RACE whenever a `memories` test runs first, `global_capture`'s
    /// own installer silently fails its `let _ =`, and
    /// `failed_job_emits_a_tracing_event` captures nothing. It was measured, not
    /// reasoned about: that test went red on this lane's first full workspace
    /// gate, and this paragraph is what the red bought.
    ///
    /// So the arming comes from `global_capture` itself, through its public
    /// entry point, over an empty future — one process, one global, and the one
    /// that both consumers need. The CAPTURE stays thread-scoped, because
    /// `global_capture`'s own reader is `async` and renders the MESSAGE only
    /// while every assertion here reads `key=value` fields (`memoryId`,
    /// `neighbourCount`, `charactersAffected`, `durationMs`). The module doc
    /// names `global_capture` as the sanctioned answer to the `Interest` race;
    /// this is that answer split into the half each test needs.
    fn captured(f: impl FnOnce()) -> Vec<String> {
        arm_global_callsites();
        crate::test_support::captured(f)
    }

    /// Install `global_capture`'s process-global subscriber once per binary, by
    /// running it over an empty future. Idempotent, and a no-op for anything
    /// else: the layer reads only a thread that has ARMED its buffer, so an
    /// un-armed thread's events are dropped rather than colouring whichever test
    /// happens to be capturing.
    fn arm_global_callsites() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a current-thread runtime to arm the global capture layer")
                .block_on(crate::test_support::global_capture::capture_events(async {}));
        });
    }

    #[test]
    fn delete_with_unlink_logs_v4s_complete_debug() {
        let (_dir, w) = seed(&[
            ("mA1", "charA", &["mA2"]),
            ("mA2", "charA", &["mA1"]),
            ("mB1", "charB", &["mA1"]),
        ]);
        let lines = captured(|| {
            w.memories().delete_with_unlink("mA1").unwrap();
        });
        let hits = lines_with(&lines, "[MemoryGate] deleteMemoryWithUnlink complete");
        assert_eq!(hits.len(), 1, "exactly one complete line: {lines:?}");
        let line = hits[0];
        assert!(line.starts_with("DEBUG "), "v4 logs at debug: {line:?}");
        assert!(line.contains(" memoryId=mA1"), "{line:?}");
        // Two neighbours name it (mA2 same character, mB1 another); both are
        // rewritten, so two characters are affected.
        assert!(line.contains(" neighbourCount=2"), "{line:?}");
        assert!(line.contains(" charactersAffected=2"), "{line:?}");
        assert!(duration_ms_of(line) >= 0, "{line:?}");
        assert!(
            lines_with(&lines, "unusually large neighbour set").is_empty(),
            "two neighbours is under the 20 threshold: {lines:?}"
        );
    }

    /// v4's `SINGLE_DELETE_NEIGHBOUR_WARN = 20`, compared with `>=`. Both sides
    /// of the bound, so a flipped comparison or an off-by-one reddens.
    #[test]
    fn delete_with_unlink_warns_at_v4s_single_threshold() {
        let warn = "[MemoryGate] deleteMemoryWithUnlink touched an unusually large neighbour set";
        for (neighbours, expect_warn) in [(19usize, false), (20usize, true)] {
            let mut rows: Vec<(String, String, Vec<String>)> =
                vec![("target".to_string(), "charA".to_string(), Vec::new())];
            for i in 0..neighbours {
                rows.push((format!("n{i}"), "charA".to_string(), vec!["target".into()]));
            }
            let borrowed: Vec<(&str, &str, Vec<&str>)> = rows
                .iter()
                .map(|(a, b, c)| {
                    (
                        a.as_str(),
                        b.as_str(),
                        c.iter().map(String::as_str).collect(),
                    )
                })
                .collect();
            let seeds: Vec<(&str, &str, &[&str])> = borrowed
                .iter()
                .map(|(a, b, c)| (*a, *b, c.as_slice()))
                .collect();
            let (_dir, w) = seed(&seeds);
            let lines = captured(|| {
                w.memories().delete_with_unlink("target").unwrap();
            });
            let hits = lines_with(&lines, warn);
            assert_eq!(
                hits.len(),
                usize::from(expect_warn),
                "{neighbours} neighbours, expect_warn={expect_warn}: {lines:?}"
            );
            if expect_warn {
                let line = hits[0];
                assert!(line.starts_with("WARN "), "v4 logs at warn: {line:?}");
                assert!(line.contains(" memoryId=target"), "{line:?}");
                assert!(line.contains(" neighbourCount=20"), "{line:?}");
                assert!(line.contains(" charactersAffected=1"), "{line:?}");
                assert!(duration_ms_of(line) >= 0, "{line:?}");
            }
            // The debug fires either way — v4 warns IN ADDITION, never instead.
            assert_eq!(
                lines_with(&lines, "deleteMemoryWithUnlink complete").len(),
                1,
                "{lines:?}"
            );
        }
    }

    #[test]
    fn delete_many_with_unlink_logs_v4s_complete_debug() {
        let (_dir, w) = seed(&[
            ("mA1", "charA", &[]),
            ("mA2", "charA", &["mA1"]),
            ("mB1", "charB", &["mA1"]),
            ("mB2", "charB", &[]),
        ]);
        let ids = vec!["mA1".to_string(), "mB2".to_string(), "ghost".to_string()];
        let lines = captured(|| {
            w.memories()
                .delete_many_with_unlink(&ids, CountSource::Repository)
                .unwrap();
        });
        let hits = lines_with(
            &lines,
            "[MemoryGate] deleteMemoriesWithUnlinkBatch complete",
        );
        assert_eq!(hits.len(), 1, "exactly one complete line: {lines:?}");
        let line = hits[0];
        assert!(line.starts_with("DEBUG "), "v4 logs at debug: {line:?}");
        // `requested` counts the ids handed in (the ghost included); `deleted`
        // counts the rows that existed.
        assert!(line.contains(" requested=3"), "{line:?}");
        assert!(line.contains(" deleted=2"), "{line:?}");
        // P4.D265 (F8): `countSource` between `deleted` and `neighboursTouched`
        // (v4's ONE `logFields` key order, `memory-gate.ts:900-928`).
        assert!(
            line.contains(" deleted=2 countSource=repository neighboursTouched=2"),
            "{line:?}"
        );
        assert!(line.contains(" charactersAffected=2"), "{line:?}");
        assert!(duration_ms_of(line) >= 0, "{line:?}");
        assert!(
            lines_with(&lines, "unusually large neighbour set").is_empty(),
            "two neighbours is under the 200 threshold: {lines:?}"
        );
    }

    /// P4.D265 (F8, R-C): the job-child path's `resolved-ids` label — v4's
    /// child arm is unreachable through the oracle, so the rendering is pinned
    /// here; the count is unchanged.
    #[test]
    fn delete_many_with_unlink_renders_the_resolved_ids_label() {
        let (_dir, w) = seed(&[("mA1", "charA", &[]), ("mA2", "charA", &[])]);
        let ids = vec!["mA1".to_string(), "ghost".to_string()];
        let (n, lines) = crate::test_support::captured_with(|| {
            w.memories()
                .delete_many_with_unlink(&ids, CountSource::ResolvedIds)
                .unwrap()
        });
        assert_eq!(n, 1);
        let hits = lines_with(&lines, "deleteMemoriesWithUnlinkBatch complete");
        assert!(
            hits[0].contains(" deleted=1 countSource=resolved-ids neighboursTouched=0"),
            "{hits:?}"
        );
        assert_eq!(CountSource::Repository.as_str(), "repository");
    }

    /// P4.D265 (v4 `70f9b495c`): the increment's hit / miss / wrong-character
    /// / NULL-count arms and their lines.
    #[test]
    fn increment_reinforcement_counts_inside_one_savepoint() {
        let (_dir, w) = seed(&[("m1", "charA", &[]), ("m2", "charA", &[])]);
        let repo = w.memories();
        let (r, lines) = crate::test_support::captured_with(|| {
            repo.increment_reinforcement("charA", "m1", "2026-05-03T00:00:00.000Z")
                .unwrap()
        });
        let r = r.expect("hit");
        assert_eq!(r.reinforcement_count, 2.0);
        assert_eq!(
            r.reinforced_importance,
            crate::memory_gate::calculate_reinforced_importance(0.5, 2.0)
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(
            "DEBUG quilltap::db Memory reinforcement counted memoryId=m1 characterId=charA reinforcementCount=2 "
        ));
        let (miss, lines) = crate::test_support::captured_with(|| {
            repo.increment_reinforcement("charB", "m1", "t").unwrap()
        });
        assert!(miss.is_none());
        assert_eq!(
            lines,
            vec!["WARN quilltap::db Memory not found for reinforcement memoryId=m1 characterId=charB"]
        );
        w.connection()
            .execute(
                "UPDATE memories SET reinforcementCount = NULL WHERE id = 'm2'",
                [],
            )
            .unwrap();
        let r = repo
            .increment_reinforcement("charA", "m2", "t")
            .unwrap()
            .unwrap();
        assert_eq!(r.reinforcement_count, 2.0, "NULL count reads 1 (v4 `?? 1`)");
        let (stamp,): (String,) = w
            .connection()
            .query_row(
                "SELECT lastReinforcedAt FROM memories WHERE id = 'm2'",
                [],
                |row| Ok((row.get(0)?,)),
            )
            .unwrap();
        assert_eq!(stamp, "t");
    }

    /// P4.D265 (v4 `d58548051`): the tier move — extras, the foreign-id scope,
    /// `supersededById` on the DEBUG only for a real id, the chunked path.
    #[test]
    fn update_tier_bulk_moves_scoped_rows_with_their_extras() {
        let (_dir, w) = seed(&[
            ("m1", "charA", &[]),
            ("m2", "charA", &[]),
            ("o1", "charB", &[]),
        ]);
        let repo = w.memories();
        let ids = vec!["m1".to_string(), "o1".to_string()];
        let (n, lines) = crate::test_support::captured_with(|| {
            repo.update_tier_bulk(
                "charA",
                &ids,
                "cold",
                TierExtra {
                    superseded_by_id: Some(Some("d1".into())),
                    consolidated_at: Some(Some("t5".into())),
                },
            )
            .unwrap()
        });
        assert_eq!(n, 1, "the foreign row is untouched");
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::db Moved memories between tiers characterId=charA tier=cold requested=2 changed=1 supersededById=d1"]
        );
        let row = |id: &str| -> (String, Option<String>, Option<String>) {
            w.connection()
                .query_row(
                    "SELECT tier, supersededById, consolidatedAt FROM memories WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
        };
        assert_eq!(
            row("m1"),
            ("cold".into(), Some("d1".into()), Some("t5".into()))
        );
        assert_eq!(row("o1"), ("hot".into(), None, None));
        let (_, lines) = crate::test_support::captured_with(|| {
            repo.update_tier_bulk(
                "charA",
                &["m1".to_string()],
                "hot",
                TierExtra {
                    superseded_by_id: Some(None),
                    consolidated_at: None,
                },
            )
            .unwrap()
        });
        assert!(!lines[0].contains("supersededById"), "{lines:?}");
        assert_eq!(row("m1"), ("hot".into(), None, Some("t5".into())));
        // > SQLITE_VARIABLE_CHUNK_SIZE ids, the real one in the second chunk.
        let mut many: Vec<String> = (0..SQLITE_VARIABLE_CHUNK_SIZE)
            .map(|i| format!("ghost-{i}"))
            .collect();
        many.push("m2".into());
        assert_eq!(
            repo.update_tier_bulk("charA", &many, "cold", TierExtra::default())
                .unwrap(),
            1
        );
        let (zero, lines) = crate::test_support::captured_with(|| {
            repo.update_tier_bulk("charA", &[], "cold", TierExtra::default())
                .unwrap()
        });
        assert_eq!((zero, lines.len()), (0, 0), "empty → 0, no line");
        assert_eq!(
            repo.mark_considered("charA", &["m1".into(), "o1".into()], "t6")
                .unwrap(),
            1
        );
    }

    /// The empty-input short circuit returns before v4 ever reads its clock, so
    /// NOTHING is logged — v4's `if (memoryIds.length === 0) return 0` precedes
    /// `const startedAt = Date.now()`.
    #[test]
    fn delete_many_with_unlink_is_silent_on_an_empty_batch() {
        let (_dir, w) = seed(&[("mA1", "charA", &[])]);
        let lines = captured(|| {
            assert_eq!(
                w.memories()
                    .delete_many_with_unlink(&[], CountSource::Repository)
                    .unwrap(),
                0
            );
        });
        assert!(
            lines_with(&lines, "[MemoryGate]").is_empty(),
            "the empty short circuit precedes v4's clock read: {lines:?}"
        );
    }

    /// v4's `BATCH_DELETE_NEIGHBOUR_WARN = 200`, compared with `>=` against
    /// `neighboursTouched` (NOT the candidate count). Both sides of the bound.
    #[test]
    fn delete_many_with_unlink_warns_at_v4s_batch_threshold() {
        let warn =
            "[MemoryGate] deleteMemoriesWithUnlinkBatch touched an unusually large neighbour set";
        for (neighbours, expect_warn) in [(199usize, false), (200usize, true)] {
            let mut rows: Vec<(String, String, Vec<String>)> =
                vec![("target".to_string(), "charA".to_string(), Vec::new())];
            for i in 0..neighbours {
                rows.push((format!("n{i}"), "charA".to_string(), vec!["target".into()]));
            }
            let borrowed: Vec<(&str, &str, Vec<&str>)> = rows
                .iter()
                .map(|(a, b, c)| {
                    (
                        a.as_str(),
                        b.as_str(),
                        c.iter().map(String::as_str).collect(),
                    )
                })
                .collect();
            let seeds: Vec<(&str, &str, &[&str])> = borrowed
                .iter()
                .map(|(a, b, c)| (*a, *b, c.as_slice()))
                .collect();
            let (_dir, w) = seed(&seeds);
            let ids = vec!["target".to_string()];
            let lines = captured(|| {
                w.memories()
                    .delete_many_with_unlink(&ids, CountSource::Repository)
                    .unwrap();
            });
            let hits = lines_with(&lines, warn);
            assert_eq!(
                hits.len(),
                usize::from(expect_warn),
                "{neighbours} neighbours, expect_warn={expect_warn}: {lines:?}"
            );
            if expect_warn {
                let line = hits[0];
                assert!(line.starts_with("WARN "), "v4 logs at warn: {line:?}");
                assert!(line.contains(" requested=1"), "{line:?}");
                assert!(line.contains(" deleted=1"), "{line:?}");
                assert!(
                    line.contains(" deleted=1 countSource=repository neighboursTouched=200"),
                    "{line:?}"
                );
                assert!(line.contains(" charactersAffected=1"), "{line:?}");
            }
            assert_eq!(
                lines_with(&lines, "deleteMemoriesWithUnlinkBatch complete").len(),
                1,
                "{lines:?}"
            );
        }
    }

    /// P4.149 (P4.144 item 11): a failed OWNER read is v4's fallback `findById`
    /// — `Error finding entity by ID` then `Memory not found for update`, and
    /// `Ok(false)`: the caller CONTINUES (v5's `?` had stopped it). A missing row
    /// WARNs alone; a foreign owner WARNs `Memory does not belong to character`;
    /// the owner's own update is silent.
    #[test]
    fn update_for_character_continues_past_a_failed_owner_read_with_v4s_lines() {
        let (_d, w) = seed(&[("m-1", "c-1", &[]), ("m-2", "c-2", &[])]);
        let repo = w.memories();
        let patch = MemUpdate {
            related_memory_ids: Some(vec!["x".to_string()]),
            ..Default::default()
        };
        // The owner read fails on a BLOB `characterId` (v4's `validate` refuses it
        // the same way; v5 reads the cell as text and fails).
        w.connection()
            .execute(
                "UPDATE memories SET characterId = x'00000000' WHERE id = 'm-2'",
                [],
            )
            .unwrap();
        let mut got = Vec::new();
        let lines = captured(|| {
            got.push(repo.update_for_character("c-1", "m-1", &patch).unwrap());
            got.push(repo.update_for_character("c-1", "m-2", &patch).unwrap());
            got.push(repo.update_for_character("c-1", "m-gone", &patch).unwrap());
            got.push(repo.delete_for_character("c-9", "m-1").unwrap());
            got.push(repo.delete_for_character("c-1", "m-gone").unwrap());
        });
        assert_eq!(got, [true, false, false, false, false]);
        let lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR") || l.starts_with("WARN"))
            .collect();
        assert_eq!(
            lines,
            [
                "ERROR quilltap::db Error finding entity by ID collection=memories id=m-2 error=Invalid column type Blob at index: 0, name: characterId",
                "WARN quilltap::db Memory not found for update memoryId=m-2 characterId=c-1",
                "WARN quilltap::db Memory not found for update memoryId=m-gone characterId=c-1",
                "WARN quilltap::db Memory does not belong to character characterId=c-9 memoryId=m-1",
                "WARN quilltap::db Memory not found for deletion memoryId=m-gone characterId=c-1",
            ]
        );
    }

    /// P4.149: a failed WRITE under `update_for_character` / `delete_for_character`
    /// logs v4's three rethrow lines in order and propagates.
    #[test]
    fn a_failed_write_logs_the_three_rethrow_lines_and_propagates() {
        let (_d, w) = seed(&[("m-1", "c-1", &[])]);
        let repo = w.memories();
        w.connection()
            .execute_batch(
                "CREATE TRIGGER no_update BEFORE UPDATE ON memories BEGIN SELECT RAISE(ABORT, 'planted'); END;\
                 CREATE TRIGGER no_delete BEFORE DELETE ON memories BEGIN SELECT RAISE(ABORT, 'planted'); END;",
            )
            .unwrap();
        let patch = MemUpdate {
            summary: Some("s".to_string()),
            ..Default::default()
        };
        let mut errs = Vec::new();
        let lines = captured(|| {
            errs.push(repo.update_for_character("c-1", "m-1", &patch).is_err());
            errs.push(repo.delete_for_character("c-1", "m-1").is_err());
        });
        assert_eq!(errs, [true, true]);
        let lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR") || l.starts_with("WARN"))
            .collect();
        assert_eq!(
            lines,
            [
                "ERROR quilltap::db Error updating entity collection=memories id=m-1 error=planted",
                "ERROR quilltap::db Error updating memory collection=memories memoryId=m-1 error=planted",
                "ERROR quilltap::db Error updating memory for character collection=memories characterId=c-1 memoryId=m-1 error=planted",
                "ERROR quilltap::db Error deleting entity collection=memories id=m-1 error=planted",
                "ERROR quilltap::db Error deleting memory collection=memories memoryId=m-1 error=planted",
                "ERROR quilltap::db Error deleting memory for character collection=memories characterId=c-1 memoryId=m-1 error=planted",
            ]
        );
    }
}
