//! The memories **write / mutation** surface of v4's `MemoriesRepository`
//! (`lib/database/repositories/memories.repository.ts` + the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`). The read
//! path (every `findBy*` / `count*`) lives in [`super::memories_read`].
//!
//! `memories` is a plain `AbstractBaseRepository<Memory>` in the **main** db with
//! no base-method override, so `create` / `update` / `delete` are the standard
//! internals; on top of them the repo adds character-scoped and bulk mutators
//! (`updateForCharacter`, `updateAccessTime{,Bulk}`, the tier moves, the
//! reinforcement counter, the link / footnote / member deltas, and
//! `bulkDeleteUnsafe` — P4.D288: the retire path's only deleter; v4's bare
//! `delete` is REMOVED, R-F).
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

use super::DbError;
use crate::chunk::{chunk_array, SQLITE_VARIABLE_CHUNK_SIZE};
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
    /// P4.D288 (v4 `memory.types.ts:145`, bug 225): when `superseded_by_id`
    /// was set — cold retention ages by it (`None` → NULL).
    pub superseded_at: Option<String>,
    /// Digest rows only: the member ids folded into it (compact JSON text).
    pub consolidated_from: Vec<String>,
    /// Last time the consolidator considered the row (`None` → NULL).
    pub consolidated_at: Option<String>,
}

/// P4.D265 (v4 `70f9b495c`): what
/// [`MemoriesRepository::increment_reinforcement`] wrote — v4's `{
/// reinforcementCount, reinforcedImportance, lastReinforcedAt }` (P4.D288:
/// `lastReinforcedAt` joined with bug 226's forward-only rule).
#[derive(Debug, Clone, PartialEq)]
pub struct Reinforcement {
    pub reinforcement_count: f64,
    pub reinforced_importance: f64,
    pub last_reinforced_at: String,
}

/// P4.D288 (C1-7) — what [`MemoriesRepository::append_reinforcement_footnotes`]
/// wrote: v4's `{ content, appended }` (the content as it now stands, and the
/// details actually appended, possibly none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendedFootnotes {
    pub content: String,
    pub appended: Vec<String>,
}

/// P4.D288 (C1-7) — v4's `op: 'add' | 'remove'` on the link-delta pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkOp {
    Add,
    Remove,
}

impl LinkOp {
    fn as_str(self) -> &'static str {
        match self {
            LinkOp::Add => "add",
            LinkOp::Remove => "remove",
        }
    }
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
/// Since P4.168 (H-2) every production `create` is gated too —
/// [`MemoriesRepository::create`] validates the row it writes through the same
/// `zod_memory_issues`; this pre-parse stays the importer's / restorer's own
/// (it also fills `MemorySchema`'s DEFAULTS, which the repository check does
/// not).
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
        superseded_at: str_of("supersededAt"),
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
    /// P4.D288 (bug 225): the nullable `supersededAt` (`Some(None)` → NULL).
    pub superseded_at: Option<Option<String>>,
    pub consolidated_from: Option<Vec<String>>,
    pub consolidated_at: Option<Option<String>>,
    pub updated_at: Option<String>,
}

// === P4.D288 (C1-3) ===
impl MemUpdate {
    /// True when the patch names any field — v4's `alongside && Object.keys(
    /// alongside).length > 0` (`memory-index.ts`'s `withFields` count).
    pub fn names_any_field(&self) -> bool {
        self.content.is_some()
            || self.summary.is_some()
            || self.keywords.is_some()
            || self.tags.is_some()
            || self.related_memory_ids.is_some()
            || self.importance.is_some()
            || self.source.is_some()
            || self.reinforcement_count.is_some()
            || self.reinforced_importance.is_some()
            || self.about_character_id.is_some()
            || self.chat_id.is_some()
            || self.project_id.is_some()
            || self.witnessed_context.is_some()
            || self.occurred_at.is_some()
            || self.narrative_time.is_some()
            || self.entities.is_some()
            || self.kind.is_some()
            || self.source_message_id.is_some()
            || self.last_accessed_at.is_some()
            || self.last_reinforced_at.is_some()
            || self.embedding.is_some()
            || self.tier.is_some()
            || self.superseded_by_id.is_some()
            || self.superseded_at.is_some()
            || self.consolidated_from.is_some()
            || self.consolidated_at.is_some()
            || self.updated_at.is_some()
    }
}
// === end P4.D288 ===

/// P4.168 (H-2): the entity v4's `_create` validates —
/// `{...data, id, createdAt, updatedAt}` (`base.repository.ts:350-366`) over
/// what the memories repository's `create` prefilled (`tier ?? 'hot'`,
/// `consolidatedFrom ?? []`, every caller's `MemCreate` states both) — checked
/// against `MemorySchema` through the ONE port,
/// [`zod_memory_issues`](crate::api::zod_issues::zod_memory_issues). A `None`
/// nullable is left ABSENT (v4's `undefined` / `null` both pass
/// `.nullable().optional()`); the embedding is left out (every arm of v4's
/// union accepts a vector — a `Float32Array` is always valid). `Err` is
/// [`DbError::RowSchemaRefusal`] carrying the issues v4's 400 `details` names.
fn validate_create_row(data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
    use crate::api::zod_issues::{zod_issue_details, zod_memory_issues};
    use serde_json::Map;
    let mut row = Map::new();
    let mut put = |k: &str, v: Value| {
        row.insert(k.to_string(), v);
    };
    let opt = |v: &Option<String>| v.clone().map(Value::String);
    put("id", Value::String(opts.id.clone()));
    put("characterId", Value::String(data.character_id.clone()));
    for (k, v) in [
        ("aboutCharacterId", &data.about_character_id),
        ("chatId", &data.chat_id),
        ("projectId", &data.project_id),
    ] {
        if let Some(v) = opt(v) {
            put(k, v);
        }
    }
    put("content", Value::String(data.content.clone()));
    put("summary", Value::String(data.summary.clone()));
    put("keywords", serde_json::json!(data.keywords));
    put("tags", serde_json::json!(data.tags));
    put("importance", super::js_number_to_json(data.importance));
    put("source", Value::String(data.source.clone()));
    for (k, v) in [
        ("witnessedContext", &data.witnessed_context),
        ("occurredAt", &data.occurred_at),
        ("narrativeTime", &data.narrative_time),
    ] {
        if let Some(v) = opt(v) {
            put(k, v);
        }
    }
    put("entities", serde_json::json!(data.entities));
    put("kind", Value::String(data.kind.clone()));
    for (k, v) in [
        ("sourceMessageId", &data.source_message_id),
        ("lastAccessedAt", &data.last_accessed_at),
    ] {
        if let Some(v) = opt(v) {
            put(k, v);
        }
    }
    put(
        "reinforcementCount",
        super::js_number_to_json(data.reinforcement_count),
    );
    if let Some(v) = opt(&data.last_reinforced_at) {
        put("lastReinforcedAt", v);
    }
    put(
        "relatedMemoryIds",
        serde_json::json!(data.related_memory_ids),
    );
    put(
        "reinforcedImportance",
        super::js_number_to_json(data.reinforced_importance),
    );
    put("tier", Value::String(data.tier.clone()));
    for (k, v) in [
        ("supersededById", &data.superseded_by_id),
        ("supersededAt", &data.superseded_at),
    ] {
        if let Some(v) = opt(v) {
            put(k, v);
        }
    }
    put(
        "consolidatedFrom",
        serde_json::json!(data.consolidated_from),
    );
    if let Some(v) = opt(&data.consolidated_at) {
        put("consolidatedAt", v);
    }
    put("createdAt", Value::String(opts.created_at.clone()));
    put("updatedAt", Value::String(opts.updated_at.clone()));
    let issues = zod_memory_issues(&row);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(DbError::RowSchemaRefusal {
            collection: "memories",
            issues: zod_issue_details(&issues),
        })
    }
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
    ///
    /// P4.168 (the human's ruling H-2, 2026-10-27 — "refuse as v4 does"): the
    /// row is validated against `MemorySchema` FIRST, as v4's `_create` runs
    /// `this.validate({...data, id, createdAt, updatedAt})` before the insert
    /// (`base.repository.ts:350-379`). A refused row (an offset `occurredAt` —
    /// the route's `z.iso.datetime({ offset: true })` admits it, the row's
    /// `Z`-only `TimestampSchema` does not) logs v4's THREE ERRORs in order —
    /// `Data validation failed` / `Error creating entity` / `Error creating
    /// memory`, each carrying the `ZodError.message` — writes nothing, and
    /// answers [`DbError::RowSchemaRefusal`] (contract C1; the route's 400
    /// `Validation error`). ONE check for every caller: the manual create and
    /// every admission insert (`services/admission/apply.rs`), the
    /// consolidation digest (`services/consolidation.rs`), and — a second,
    /// redundant check after their own pre-validation — the `.qtap` import and
    /// the backup restore (P4.161).
    pub fn create(&self, data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
        if let Err(refusal) = validate_create_row(data, opts) {
            let zod = refusal.row_schema_refusal_message().unwrap_or_default();
            // `error_text` renders the variant's Display (`Validation error`);
            // v4's three lines carry the `ZodError.message` — the P4.161
            // `log_refused_create` pair, then the repository's own wrap over
            // the same text.
            super::fallback::log_refused_create("memories", &zod);
            super::fallback::log_memory_create_failure(&data.character_id, &DbError::Internal(zod));
            return Err(refusal);
        }
        self.create_row(data, opts).inspect_err(|e| {
            super::fallback::log_create_failure("memories", e);
            super::fallback::log_memory_create_failure(&data.character_id, e);
        })
    }

    /// TEST PLANT ONLY (P4.168): write `data` with NO `MemorySchema` check —
    /// the in-code twin of a raw-SQL plant, for the unit seeds whose legacy
    /// fixture ids (`m-1`, `char-a`) the repository `create` now refuses as
    /// v4's does. A row v4's own instance can hold (pre-validation legacy
    /// data) is still a row the readers must handle. Never compiled into a
    /// production build.
    #[cfg(any(test, feature = "test-support"))]
    pub fn plant_unvalidated(&self, data: &MemCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.create_row(data, opts)
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
                tier, supersededById, supersededAt, consolidatedFrom, consolidatedAt, \
                createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)",
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
                data.superseded_at,
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
        if let Some(v) = &patch.superseded_at {
            set_col!("supersededAt", Box::new(v.clone()));
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

    /// P4.D265 (v4 `70f9b495c`, bug 182), widened by P4.D288 (v4 `c2795733f`,
    /// bug 226) — v4 `incrementReinforcement(characterId, memoryId, at, by)`
    /// (`memories.repository.ts:622-662`): count `by` more observations of
    /// `memory_id` (scoped to `character_id`) as a read-then-write inside ONE
    /// transaction, so two concurrent reinforcements both count.
    ///
    /// `by` must be a positive integer: anything else is REFUSED before the
    /// `safeQuery` — `Memory reinforcement: delta must be a positive integer
    /// (got {by})`, no line, nothing read (v4 throws ahead of the try).
    /// `updatedAt` is minted BEFORE the transaction (v4 reads
    /// `getCurrentTimestamp()` first). Inside a SAVEPOINT (v4's
    /// `db.transaction`, which nests when the job applier's `BEGIN IMMEDIATE`
    /// is open): `SELECT "importance", "reinforcementCount",
    /// "lastReinforcedAt"` for the id AND character; none → `Ok(None)` + WARN
    /// `Memory not found for reinforcement {memoryId, characterId}`; else
    /// `count = (reinforcementCount ?? 1) + by`, `importance =
    /// calculate_reinforced_importance(importance ?? 0.5, count)` (the formula
    /// moves to `memory_scoring` with unit 12), and `lastReinforcedAt` moves
    /// FORWARD only — the stored value when it is truthy and sorts after `at`
    /// (a JS string `>`, compared in UTF-16 code units), else `at` — so a merge
    /// carrying a loser's older timestamp never rewinds a reinforcement
    /// committed since. One UPDATE keyed by `id` alone, then DEBUG `Memory
    /// reinforcement counted {memoryId, characterId, by, reinforcementCount,
    /// reinforcedImportance, lastReinforcedAt}`. A failure is v4's 3-argument
    /// RETHROW: ERROR `Error reinforcing memory` (the `db::fallback` home), then
    /// the `Err` propagates. NO-PORT (§R.4(o)): the `getRawDatabase()` null
    /// throw and the job child's buffered `increment*` arm — v5 has no child.
    pub fn increment_reinforcement(
        &self,
        character_id: &str,
        memory_id: &str,
        at: &str,
        by: i64,
    ) -> Result<Option<Reinforcement>, DbError> {
        if by < 1 {
            return Err(DbError::Internal(format!(
                "Memory reinforcement: delta must be a positive integer (got {by})"
            )));
        }
        let now = now_iso();
        let counted = self
            .reinforce_in_savepoint(character_id, memory_id, at, by, &now)
            .inspect_err(|e| {
                super::fallback::log_memory_reinforce_failure(character_id, memory_id, e);
            })?;
        match &counted {
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
                    by = by,
                    reinforcementCount = r.reinforcement_count,
                    reinforcedImportance = r.reinforced_importance,
                    lastReinforcedAt = %r.last_reinforced_at,
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
        by: i64,
        now: &str,
    ) -> Result<Option<Reinforcement>, DbError> {
        super::schema_probe::in_savepoint(self.conn, "memory_reinforcement", |_| {
            let row: Option<(Option<f64>, Option<f64>, Option<String>)> = self
                .conn
                .query_row(
                    "SELECT \"importance\", \"reinforcementCount\", \"lastReinforcedAt\" \
                     FROM \"memories\" WHERE \"id\" = ? AND \"characterId\" = ?",
                    params![memory_id, character_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            let Some((importance, count, stored_at)) = row else {
                return Ok(None);
            };
            let reinforcement_count = count.unwrap_or(1.0) + by as f64;
            let reinforced_importance = crate::memory_scoring::reinforced_importance(
                importance.unwrap_or(0.5),
                reinforcement_count,
            );
            let last_reinforced_at = match stored_at {
                Some(s) if !s.is_empty() && s.encode_utf16().gt(at.encode_utf16()) => s,
                _ => at.to_string(),
            };
            self.conn.execute(
                "UPDATE \"memories\" SET \"reinforcementCount\" = ?, \"reinforcedImportance\" = ?, \
                 \"lastReinforcedAt\" = ?, \"updatedAt\" = ? WHERE \"id\" = ?",
                params![
                    reinforcement_count,
                    reinforced_importance,
                    last_reinforced_at,
                    now,
                    memory_id
                ],
            )?;
            Ok(Some(Reinforcement {
                reinforcement_count,
                reinforced_importance,
                last_reinforced_at,
            }))
        })
    }

    /// P4.D265 (v4 `d58548051`) — v4 `updateTierBulk`
    /// (`memories.repository.ts:874-905`): move `ids` of `character_id` to
    /// `tier`, `extra` riding on the same write. Empty → `Ok(0)` with no line.
    /// Per `chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE)` one `UPDATE memories
    /// SET tier = ?[, supersededById = ?][, supersededAt = ?][, consolidatedAt = ?],
    /// updatedAt = ?
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
            // P4.D288 (v4 `:1158-1165`, bug 225): `supersededAt` moves with
            // `supersededById` and only with it — stamped `now` when a digest
            // supersedes the rows (v4's truthiness: an empty id clears), NULL
            // when the link is cut, untouched when the move says nothing.
            let superseded_at: Option<Option<&str>> = extra
                .superseded_by_id
                .as_ref()
                .map(|v| v.as_deref().filter(|d| !d.is_empty()).map(|_| now.as_str()));
            let mut changed = 0i64;
            for chunk in chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE) {
                let mut sets = vec!["tier = ?"];
                let mut p: Vec<&dyn ToSql> = vec![&tier];
                if let Some(v) = &extra.superseded_by_id {
                    sets.push("supersededById = ?");
                    p.push(v);
                }
                if let Some(v) = &superseded_at {
                    sets.push("supersededAt = ?");
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

    // === P4.D288 (C1-7): the retire path's writer verbs (v4 `af48f6cc2` +
    // `c2795733f`, `memories.repository.ts` at the `83334b9a5` pin) ===
    //
    // v4's bare `delete` REFUSES at the pin (`:868-873` — an ERROR `Refused a
    // bare memory delete; memories are deleted through retireMemories`, then a
    // throw); v5 REMOVES it (R-F), so the refusal has no caller to reach and
    // its line is recorded unreachable. The old deleters (`deleteForCharacter`,
    // `bulkDelete`, `deleteByChatId`, `deleteBySourceMessageId`, the gate's two
    // `…WithUnlink` chokepoints) are gone with v4's: every row deletion is
    // `services::memory_retire::retire_memories`, which alone calls
    // [`Self::bulk_delete_unsafe`].

    /// v4 `addRelatedMemoryIds` (`:676-682`, bug 231): add `ids` to one row's
    /// `relatedMemoryIds` against the row as it stands at write time — a delta,
    /// not a whole-array write. Ids already present, and the row's own id, are
    /// skipped. `Ok(None)` when the row is missing (or another character's).
    pub fn add_related_memory_ids(
        &self,
        character_id: &str,
        memory_id: &str,
        ids: &[String],
    ) -> Result<Option<Vec<String>>, DbError> {
        self.update_related_memory_ids(character_id, memory_id, LinkOp::Add, ids)
    }

    /// v4 `removeRelatedMemoryIds` (`:688-694`): the removal twin of
    /// [`Self::add_related_memory_ids`].
    pub fn remove_related_memory_ids(
        &self,
        character_id: &str,
        memory_id: &str,
        ids: &[String],
    ) -> Result<Option<Vec<String>>, DbError> {
        self.update_related_memory_ids(character_id, memory_id, LinkOp::Remove, ids)
    }

    /// v4 `updateRelatedMemoryIdsAtomically` (`:696-760`): the delta (non-empty
    /// strings, deduped in first-seen order) and `updatedAt` taken BEFORE the
    /// transaction; inside ONE savepoint the row's `relatedMemoryIds` read
    /// leniently, the delta applied, and an UNCHANGED list written NOT at all.
    /// A missing row → WARN `Memory not found for link update {memoryId,
    /// characterId, op}` and `Ok(None)`; else DEBUG `Memory links updated
    /// {memoryId, characterId, op, requested, changed, size}`. A failure is
    /// v4's RETHROW: ERROR `Error updating memory links {collection,
    /// characterId, memoryId, op, error}`, then the `Err`.
    fn update_related_memory_ids(
        &self,
        character_id: &str,
        memory_id: &str,
        op: LinkOp,
        ids: &[String],
    ) -> Result<Option<Vec<String>>, DbError> {
        let mut delta: Vec<&str> = Vec::new();
        for id in ids {
            if !id.is_empty() && !delta.contains(&id.as_str()) {
                delta.push(id);
            }
        }
        let now = now_iso();
        let written = super::schema_probe::in_savepoint(self.conn, "memory_link_update", |_| {
            let row: Option<Option<String>> = self
                .conn
                .query_row(
                    "SELECT \"relatedMemoryIds\" FROM \"memories\" WHERE \"id\" = ? AND \"characterId\" = ?",
                    params![memory_id, character_id],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            let Some(raw) = row else {
                return Ok(None);
            };
            let current = parse_related_ids(raw.as_deref());
            let next: Vec<String> = match op {
                LinkOp::Add => {
                    let mut next = current.clone();
                    for id in &delta {
                        if *id == memory_id || next.iter().any(|x| x == id) {
                            continue;
                        }
                        next.push((*id).to_string());
                    }
                    next
                }
                LinkOp::Remove => current
                    .iter()
                    .filter(|x| !delta.contains(&x.as_str()))
                    .cloned()
                    .collect(),
            };
            if next == current {
                return Ok(Some((current, false)));
            }
            self.conn.execute(
                "UPDATE \"memories\" SET \"relatedMemoryIds\" = ?, \"updatedAt\" = ? WHERE \"id\" = ?",
                params![json_array(&next)?, now, memory_id],
            )?;
            Ok(Some((next, true)))
        })
        .inspect_err(|e| {
            super::fallback::log_memory_link_update_failure(
                character_id,
                memory_id,
                op.as_str(),
                e,
            );
        })?;
        match written {
            None => {
                tracing::warn!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    op = op.as_str(),
                    "Memory not found for link update"
                );
                Ok(None)
            }
            Some((list, changed)) => {
                tracing::debug!(
                    target: "quilltap::db",
                    memoryId = %memory_id,
                    characterId = %character_id,
                    op = op.as_str(),
                    requested = delta.len(),
                    changed = changed,
                    size = list.len(),
                    "Memory links updated"
                );
                Ok(Some(list))
            }
        }
    }

    /// v4 `removeRelatedMemoryIdsEverywhere` (`:767-813`, bug 234): remove
    /// `ids` from the `relatedMemoryIds` of EVERY row that names any of them,
    /// whichever character holds it — one SQL removal delta per chunk of
    /// `⌊(SQLITE_VARIABLE_CHUNK_SIZE − 1) / 2⌋` (each id binds twice), applied
    /// against the rows as they stand. Empty (after dedupe) → `0`, no line.
    /// DEBUG `Scrubbed deleted ids from related links {ids, changed}`. A
    /// failure → ERROR `Error scrubbing related memory links {collection,
    /// count, error}` and `Ok(0)` (strict-aware; `count` the ids as passed).
    pub fn remove_related_memory_ids_everywhere(&self, ids: &[String]) -> Result<i64, DbError> {
        super::fallback::related_scrub_or_zero(ids.len(), || {
            let mut removing: Vec<&String> = Vec::new();
            for id in ids {
                if !removing.contains(&id) {
                    removing.push(id);
                }
            }
            if removing.is_empty() {
                return Ok(0);
            }
            let now = now_iso();
            let chunk_size = ((SQLITE_VARIABLE_CHUNK_SIZE - 1) / 2).max(1);
            let mut changed = 0i64;
            for chunk in removing.chunks(chunk_size) {
                let placeholders = vec!["?"; chunk.len()].join(",");
                let sql = format!(
                    "UPDATE memories \
                        SET relatedMemoryIds = ( \
                              SELECT COALESCE(json_group_array(value), '[]') \
                                FROM json_each(memories.relatedMemoryIds) \
                               WHERE value NOT IN ({placeholders}) \
                            ), \
                            updatedAt = ? \
                      WHERE relatedMemoryIds IS NOT NULL \
                        AND relatedMemoryIds <> '[]' \
                        AND json_valid(relatedMemoryIds) \
                        AND EXISTS ( \
                              SELECT 1 FROM json_each(memories.relatedMemoryIds) \
                               WHERE value IN ({placeholders}) \
                            )"
                );
                let mut p: Vec<&dyn ToSql> = Vec::with_capacity(chunk.len() * 2 + 1);
                for id in chunk {
                    p.push(*id);
                }
                p.push(&now);
                for id in chunk {
                    p.push(*id);
                }
                changed += self.conn.execute(&sql, p.as_slice())? as i64;
            }
            tracing::debug!(
                target: "quilltap::db",
                ids = removing.len(),
                changed = changed,
                "Scrubbed deleted ids from related links"
            );
            Ok(changed)
        })
    }

    /// v4 `appendReinforcementFootnotes` (`:820-861`, bug 231): append novel
    /// details as `[+]` footnotes against the row as it stands at write time —
    /// the cap and the "already there" check read the live content inside ONE
    /// savepoint ([`crate::reinforcement_footnotes::append_capped_footnotes`]
    /// at [`MAX_REINFORCEMENT_FOOTNOTES`]); nothing appended → no write. A
    /// missing row → WARN `Memory not found for reinforcement footnotes
    /// {memoryId, characterId}` and `Ok(None)`; else DEBUG `Reinforcement
    /// footnotes appended {memoryId, characterId, offered, appended}`. A
    /// failure is v4's RETHROW: ERROR `Error appending reinforcement footnotes
    /// {collection, characterId, memoryId, error}`, then the `Err`.
    ///
    /// [`MAX_REINFORCEMENT_FOOTNOTES`]: crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES
    pub fn append_reinforcement_footnotes(
        &self,
        character_id: &str,
        memory_id: &str,
        details: &[String],
    ) -> Result<Option<AppendedFootnotes>, DbError> {
        let now = now_iso();
        let result = super::schema_probe::in_savepoint(self.conn, "memory_footnotes", |_| {
            let row: Option<Option<String>> = self
                .conn
                .query_row(
                    "SELECT \"content\" FROM \"memories\" WHERE \"id\" = ? AND \"characterId\" = ?",
                    params![memory_id, character_id],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            let Some(content) = row else {
                return Ok(None);
            };
            let (content, appended) = crate::reinforcement_footnotes::append_capped_footnotes(
                content.as_deref().unwrap_or(""),
                details,
                crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES,
            );
            if !appended.is_empty() {
                self.conn.execute(
                    "UPDATE \"memories\" SET \"content\" = ?, \"updatedAt\" = ? WHERE \"id\" = ?",
                    params![content, now, memory_id],
                )?;
            }
            Ok(Some(AppendedFootnotes { content, appended }))
        })
        .inspect_err(|e| {
            super::fallback::log_memory_footnotes_failure(character_id, memory_id, e);
        })?;
        match &result {
            None => tracing::warn!(
                target: "quilltap::db",
                memoryId = %memory_id,
                characterId = %character_id,
                "Memory not found for reinforcement footnotes"
            ),
            Some(r) => tracing::debug!(
                target: "quilltap::db",
                memoryId = %memory_id,
                characterId = %character_id,
                offered = details.len(),
                appended = r.appended.len(),
                "Reinforcement footnotes appended"
            ),
        }
        Ok(result)
    }

    /// v4 `bulkDeleteUnsafe` (`:882-905`): delete a character's rows outright
    /// — nothing else. Named *Unsafe* because it scrubs no links and touches no
    /// index: `services::memory_retire::retire_memories` is its only caller
    /// (`memory_retire_sole_deleter_guard` greps for any other). Empty → `0`
    /// with no line. Chunked at [`SQLITE_VARIABLE_CHUNK_SIZE`] (v4
    /// `805ef12bf`: one bind per id, and a full-wipe cascade can pass the
    /// variable ceiling); each chunk is v4's base `deleteMany` — INFO `Entities
    /// deleted {collection, count}` per chunk, a zero count included. A failure
    /// logs the base RETHROW `Error deleting entities`, then v4's `Error bulk
    /// deleting memories for character {collection, characterId, count,
    /// error}`, and propagates.
    pub fn bulk_delete_unsafe(
        &self,
        character_id: &str,
        memory_ids: &[String],
    ) -> Result<i64, DbError> {
        let run = || -> Result<i64, DbError> {
            if memory_ids.is_empty() {
                return Ok(0);
            }
            let mut deleted = 0i64;
            for chunk in chunk_array(memory_ids, SQLITE_VARIABLE_CHUNK_SIZE) {
                let placeholders = vec!["?"; chunk.len()].join(", ");
                let sql = format!(
                    "DELETE FROM memories WHERE characterId = ? AND id IN ({placeholders})"
                );
                let mut p: Vec<&dyn ToSql> = Vec::with_capacity(1 + chunk.len());
                p.push(&character_id);
                for id in chunk {
                    p.push(id);
                }
                let n = self.conn.execute(&sql, p.as_slice()).map_err(|e| {
                    let e = DbError::from(e);
                    super::fallback::log_delete_many_failure("memories", &e);
                    e
                })?;
                super::fallback::log_entities_deleted("memories", n);
                deleted += n as i64;
            }
            Ok(deleted)
        };
        run().inspect_err(|e| {
            super::fallback::log_memory_bulk_delete_failure(character_id, memory_ids.len(), e);
        })
    }

    /// v4 `removeConsolidatedMembers` (`:1387-1425`, review of bug 225):
    /// remove `member_ids` from one digest's `consolidatedFrom` in SQL against
    /// the row as it stands — a delta. Empty (after dedupe) → `0`, no line.
    /// Chunked at `SQLITE_VARIABLE_CHUNK_SIZE − 3`; `changed` is the MAX over
    /// chunks (v4's `Math.max`, one row). DEBUG `Removed members from digest
    /// {characterId, digestId, members, changed}`. A failure → ERROR `Error
    /// removing members from digest {collection, characterId, digestId, count,
    /// error}` and `Ok(0)` (strict-aware).
    pub fn remove_consolidated_members(
        &self,
        character_id: &str,
        digest_id: &str,
        member_ids: &[String],
    ) -> Result<i64, DbError> {
        super::fallback::digest_members_removed_or_zero(
            character_id,
            digest_id,
            member_ids.len(),
            || {
                let mut members: Vec<&String> = Vec::new();
                for id in member_ids {
                    if !members.contains(&id) {
                        members.push(id);
                    }
                }
                if members.is_empty() {
                    return Ok(0);
                }
                let now = now_iso();
                let mut changed = 0i64;
                for chunk in members.chunks(SQLITE_VARIABLE_CHUNK_SIZE - 3) {
                    let sql = format!(
                        "UPDATE memories \
                            SET consolidatedFrom = ( \
                                  SELECT COALESCE(json_group_array(value), '[]') \
                                    FROM json_each(memories.consolidatedFrom) \
                                   WHERE value NOT IN ({}) \
                                ), \
                                updatedAt = ? \
                          WHERE id = ? AND characterId = ? \
                            AND json_valid(consolidatedFrom)",
                        vec!["?"; chunk.len()].join(",")
                    );
                    let mut p: Vec<&dyn ToSql> = Vec::with_capacity(chunk.len() + 3);
                    for id in chunk {
                        p.push(*id);
                    }
                    p.push(&now);
                    p.push(&digest_id);
                    p.push(&character_id);
                    changed = changed.max(self.conn.execute(&sql, p.as_slice())? as i64);
                }
                tracing::debug!(
                    target: "quilltap::db",
                    characterId = %character_id,
                    digestId = %digest_id,
                    members = members.len(),
                    changed = changed,
                    "Removed members from digest"
                );
                Ok(changed)
            },
        )
    }
    // === end P4.D288 ===

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
        tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]',
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
            repo.plant_unvalidated(
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
                    superseded_at: None,
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
            .bulk_delete_unsafe("charA", &ids)
            .expect("a chunked bulk_delete_unsafe must not hit the variable limit");
        assert_eq!(deleted, 2, "only charA's two rows are in scope");
        assert_eq!(related_of(&w, "mA1"), None);
        assert_eq!(related_of(&w, "mA2"), None);
        assert!(
            related_of(&w, "mB1").is_some(),
            "the characterId scope still holds across chunks"
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
        assert_eq!(repo.bulk_delete_unsafe("charA", &ids).unwrap(), 2);
        assert_eq!(related_of(&w, "mA1"), None);
        assert_eq!(related_of(&w, "mA2"), None);
    }

    // -----------------------------------------------------------------------
    // The capture rig the line-pinning tests below share (the deletion
    // chokepoint's four line tests RETIRED with v4's gate deleters — P4.D288,
    // `af48f6cc2`; the retire path's lines are `memory_retire`'s).
    // -----------------------------------------------------------------------

    /// P4.93 — the capture tests below go through THIS, not
    /// `test_support::captured` directly.
    ///
    /// `captured` installs a THREAD-scoped subscriber, and `tracing` caches each
    /// callsite's `Interest` **globally** on first use. A sibling test that
    /// reaches a logging repository method with NO subscriber armed on its
    /// thread can retire that callsite for the whole binary, and the capture
    /// sees nothing. That is exactly what a (since-retired) chokepoint line
    /// test did once in a full workspace run ("exactly one complete line: []") while passing
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

    /// P4.D265 (v4 `70f9b495c`) + P4.D288 (bug 226): the increment's hit /
    /// miss / wrong-character / NULL-count arms and their lines, `by`, the
    /// forward-only `lastReinforcedAt`, and the refusal BEFORE anything runs.
    #[test]
    fn increment_reinforcement_counts_inside_one_savepoint() {
        let (_dir, w) = seed(&[("m1", "charA", &[]), ("m2", "charA", &[])]);
        let repo = w.memories();
        let (r, lines) = crate::test_support::captured_with(|| {
            repo.increment_reinforcement("charA", "m1", "2026-05-03T00:00:00.000Z", 1)
                .unwrap()
        });
        let r = r.expect("hit");
        assert_eq!(r.reinforcement_count, 2.0);
        assert_eq!(
            r.reinforced_importance,
            crate::memory_scoring::reinforced_importance(0.5, 2.0)
        );
        assert_eq!(r.last_reinforced_at, "2026-05-03T00:00:00.000Z");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(
            "DEBUG quilltap::db Memory reinforcement counted memoryId=m1 characterId=charA by=1 reinforcementCount=2 "
        ));
        assert!(
            lines[0].ends_with(" lastReinforcedAt=2026-05-03T00:00:00.000Z"),
            "{lines:?}"
        );
        // `by` counts several observations; an OLDER `at` never rewinds.
        let r = repo
            .increment_reinforcement("charA", "m1", "2026-01-01T00:00:00.000Z", 3)
            .unwrap()
            .unwrap();
        assert_eq!(r.reinforcement_count, 5.0);
        assert_eq!(r.last_reinforced_at, "2026-05-03T00:00:00.000Z");
        let (refused, lines) = crate::test_support::captured_with(|| {
            repo.increment_reinforcement("charA", "m1", "t", 0)
                .err()
                .map(|e| e.to_string())
        });
        assert!(refused
            .unwrap()
            .contains("Memory reinforcement: delta must be a positive integer (got 0)"),);
        assert!(
            lines.is_empty(),
            "the refusal precedes the safeQuery: {lines:?}"
        );
        let (miss, lines) = crate::test_support::captured_with(|| {
            repo.increment_reinforcement("charB", "m1", "t", 1).unwrap()
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
            .increment_reinforcement("charA", "m2", "t", 1)
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

    /// P4.D288 (C1-7): the link deltas compose (add skips self + present, an
    /// unchanged list writes nothing), the everywhere-scrub removes across
    /// holders, the footnotes cap, and the digest member delta.
    #[test]
    fn the_delta_verbs_apply_against_the_row_as_it_stands() {
        let (_dir, w) = seed(&[
            ("m1", "charA", &["m2"]),
            ("m2", "charA", &[]),
            ("m3", "charB", &["m2", "m9"]),
        ]);
        let repo = w.memories();
        let ids = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                repo.add_related_memory_ids("charA", "m1", &ids(&["m1", "m2", "m3", "m3", ""]))
                    .unwrap(),
                repo.remove_related_memory_ids("charA", "m1", &ids(&["m9"]))
                    .unwrap(),
                repo.add_related_memory_ids("charB", "m1", &ids(&["m3"]))
                    .unwrap(),
            )
        });
        assert_eq!(got.0, Some(ids(&["m2", "m3"])));
        assert_eq!(got.1, Some(ids(&["m2", "m3"])));
        assert_eq!(got.2, None);
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::db Memory links updated memoryId=m1 characterId=charA op=add requested=3 changed=true size=2",
                "DEBUG quilltap::db Memory links updated memoryId=m1 characterId=charA op=remove requested=1 changed=false size=2",
                "WARN quilltap::db Memory not found for link update memoryId=m1 characterId=charB op=add",
            ]
        );
        let (n, lines) = crate::test_support::captured_with(|| {
            repo.remove_related_memory_ids_everywhere(&ids(&["m2", "m2"]))
                .unwrap()
        });
        assert_eq!(n, 2);
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::db Scrubbed deleted ids from related links ids=1 changed=2"]
        );
        assert_eq!(related_of(&w, "m1"), Some(ids(&["m3"])));
        assert_eq!(related_of(&w, "m3"), Some(ids(&["m9"])));
        assert_eq!(repo.remove_related_memory_ids_everywhere(&[]).unwrap(), 0);

        let details: Vec<String> = (0..10).map(|i| format!("d{i}")).collect();
        let first = repo
            .append_reinforcement_footnotes("charA", "m2", &details)
            .unwrap()
            .unwrap();
        assert_eq!(
            first.appended.len(),
            crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES
        );
        let second = repo
            .append_reinforcement_footnotes("charA", "m2", &details)
            .unwrap()
            .unwrap();
        assert!(second.appended.is_empty());
        assert_eq!(second.content, first.content);
        assert!(repo
            .append_reinforcement_footnotes("charB", "m2", &details)
            .unwrap()
            .is_none());

        w.connection()
            .execute(
                "UPDATE memories SET consolidatedFrom = '[\"a\",\"b\",\"c\"]' WHERE id = 'm1'",
                [],
            )
            .unwrap();
        assert_eq!(
            repo.remove_consolidated_members("charA", "m1", &ids(&["b", "x", "b"]))
                .unwrap(),
            1
        );
        let (cf,): (String,) = w
            .connection()
            .query_row(
                "SELECT consolidatedFrom FROM memories WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();
        assert_eq!(cf, "[\"a\",\"c\"]");
        assert_eq!(
            repo.remove_consolidated_members("charB", "m1", &ids(&["a"]))
                .unwrap(),
            0
        );
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
            got.push(repo.update_for_character("c-9", "m-1", &patch).unwrap());
        });
        assert_eq!(got, [true, false, false, false]);
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
            ]
        );
    }

    /// P4.149: a failed WRITE under `update_for_character` logs v4's three
    /// rethrow lines in order and propagates (P4.D288: `delete_for_character`
    /// RETIRED with v4's, R-F); a failed `bulk_delete_unsafe` logs the base
    /// `deleteMany` line, then v4's own.
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
            errs.push(
                repo.bulk_delete_unsafe("c-1", &["m-1".to_string()])
                    .is_err(),
            );
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
                "ERROR quilltap::db Error deleting entities collection=memories error=planted",
                "ERROR quilltap::db Error bulk deleting memories for character collection=memories characterId=c-1 count=1 error=planted",
            ]
        );
    }
    /// P4.168 (H-2): the repository `create` refuses a row `MemorySchema`
    /// refuses — an offset `occurredAt` — with v4's three ERRORs in order
    /// (each the `ZodError.message`), [`DbError::RowSchemaRefusal`] carrying
    /// the issues, and NO row; the same row with a `Z` time is written.
    #[test]
    fn create_refuses_a_row_memory_schema_refuses_with_v4s_three_lines() {
        let (_d, w) = seed(&[]);
        let repo = w.memories();
        let id = "c1680000-0000-4000-8000-000000000001";
        let row = |occurred_at: &str| MemCreate {
            character_id: "c1680000-0000-4000-8000-0000000000c1".to_string(),
            about_character_id: None,
            chat_id: None,
            project_id: None,
            content: "c".to_string(),
            summary: "s".to_string(),
            keywords: vec![],
            tags: vec![],
            importance: 0.5,
            embedding: None,
            source: "MANUAL".to_string(),
            witnessed_context: None,
            occurred_at: Some(occurred_at.to_string()),
            narrative_time: None,
            entities: Vec::new(),
            kind: "semantic".to_string(),
            source_message_id: None,
            last_accessed_at: None,
            reinforcement_count: 1.0,
            last_reinforced_at: None,
            related_memory_ids: vec![],
            reinforced_importance: 0.5,
            tier: "hot".to_string(),
            superseded_by_id: None,
            superseded_at: None,
            consolidated_from: Vec::new(),
            consolidated_at: None,
        };
        let opts = CreateOptions {
            id: id.to_string(),
            created_at: "2026-10-27T00:00:00.000Z".to_string(),
            updated_at: "2026-10-27T00:00:00.000Z".to_string(),
        };
        let mut result = None;
        let lines =
            captured(|| result = Some(repo.create(&row("2026-03-01T10:00:00+05:30"), &opts)));
        let err = result
            .unwrap()
            .expect_err("an offset occurredAt is refused");
        let DbError::RowSchemaRefusal { collection, issues } = &err else {
            panic!("not a row-schema refusal: {err:?}");
        };
        assert_eq!(*collection, "memories");
        assert_eq!(issues[0]["path"], serde_json::json!(["occurredAt"]));
        assert_eq!(issues[0]["code"], "invalid_format");
        assert_eq!(err.to_string(), "Validation error");
        let zod = err.row_schema_refusal_message().unwrap();
        let errors: Vec<&String> = lines.iter().filter(|l| l.starts_with("ERROR")).collect();
        assert_eq!(
            errors,
            [
                &format!("ERROR quilltap::db Data validation failed collection=memories error={zod}"),
                &format!("ERROR quilltap::db Error creating entity collection=memories error={zod}"),
                &format!(
                    "ERROR quilltap::db Error creating memory collection=memories characterId=c1680000-0000-4000-8000-0000000000c1 error={zod}"
                ),
            ]
        );
        let rows: i64 = w
            .connection()
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "a refused row is never written");
        repo.create(&row("2026-03-01T10:00:00.000Z"), &opts)
            .expect("a Z time passes");
    }
}
