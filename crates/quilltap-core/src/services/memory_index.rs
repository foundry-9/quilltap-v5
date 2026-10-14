//! `memory_index` — the ONE writer of `memories.embedding` / `memories.tier`
//! and the one reader of the per-character vector index — P4.D288 (C1-3),
//! porting v4 `lib/memory/index-store/memory-index.ts` (`af48f6cc2`, memory
//! refactor Phase B) and the queue half of `index-store/backfill.ts` at the
//! `83334b9a5` pin.
//!
//! "`memories.embedding` and `memories.tier` are the only stored copy of a
//! memory's vector and tier; the per-character in-process store is a cache of
//! them, loaded with one query and searched by brute-force cosine. This module
//! is the only writer of either column and the only importer of that cache."
//!
//! **L-2 — no process cache.** v5 never had v4's `VectorStoreManager`: every
//! search loads the character's rows ([`CharacterVectorStore::load`]) — v4's
//! LOADED-cache state. The counts v4 derives from its cache report the LOADED
//! answer here: [`upsert`]'s `cached` is the rows the write found (each now
//! carries the vector just written), [`index_inserted`]'s is every entry,
//! [`set_tier`] re-stamps the rows carrying a stored embedding (its `Re-stamped
//! vector tiers` DEBUG counts those whose tier moves), and [`remove`]'s
//! `removed` is the rows with a stored embedding among those retired — read by
//! [`indexed_among`] BEFORE the delete, in the same closure. The UNLOADED arm
//! (v4 `0`) is `MEMORY_INDEX_ALWAYS_LOADED` in `memory_index_tier2`; v4's
//! `withLoadedStore` WARN (`Memory index cache update failed; dropped the cache
//! so the next search reloads it`) is unreachable — there is no cache to fail.
//! v4's `reload` / `reloadAll` drop a cache v5 does not keep; they have no port.

use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::memories::{MemUpdate, MemoriesRepository, TierExtra};
use crate::db::runtime::Db;
use crate::db::vector_store::{CharacterVectorStore, Tier, VectorEntry, VectorSearchResult};
use crate::db::DbError;

/// v4's child-logger binding (`logger.child({ module: 'memory:index-store' })`).
const MODULE: &str = "memory:index-store";

/// v4 `MemoryIndexEntry` — one vector a creator just inserted WITH its row.
#[derive(Debug, Clone)]
pub struct MemoryIndexEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    /// v4 `tier` (defaults to hot in the cache).
    pub tier: Option<Tier>,
}

/// v4 `MemoryIndexUpsert` — a vector to write, `alongside` the fields that
/// must land in the SAME row update ("so new text and its vector are never on
/// disk apart"). `alongside` never names `embedding` or `tier`.
#[derive(Default)]
pub struct MemoryIndexUpsert {
    pub id: String,
    pub embedding: Vec<f32>,
    /// The tier the caller states (v4 uses it for the cache only — the row's
    /// tier is not written by an upsert).
    pub tier: Option<Tier>,
    pub alongside: MemUpdate,
}

/// v4 `MemoryIndexSearchOptions` (`memory-index.ts:61-66`).
#[derive(Debug, Clone, Copy)]
pub struct IndexSearchOptions<'a> {
    /// "Recall paths read the hot tier only; admission and the search tool
    /// read both."
    pub hot_only: bool,
    /// Ids left out of the scan (they never take a top-K slot).
    pub exclude: Option<&'a HashSet<String>>,
    pub limit: usize,
}

/// v4 `MemoryIndexShape` — what the index holds, measured against a query's
/// length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryIndexShape {
    pub size: usize,
    /// The most common vector length, or `None` when the index is empty.
    pub dominant_dimensions: Option<usize>,
    /// Entries whose length equals the query's — the ones a search can score.
    pub matching: usize,
}

/// v4 `memoryIndex.search(characterId, vector, opts)` (`memory-index.ts:
/// 135-147`): load the character's rows and search; the filter (when
/// `hot_only` or a non-empty `exclude`) keeps an entry that is not excluded
/// AND, when `hot_only`, hot.
pub fn search(
    conn: &Connection,
    character_id: &str,
    vector: &[f32],
    opts: IndexSearchOptions<'_>,
) -> Vec<VectorSearchResult> {
    let store = CharacterVectorStore::load(conn, character_id);
    let exclude = opts.exclude.filter(|e| !e.is_empty());
    if opts.hot_only || exclude.is_some() {
        store.search_filtered(vector, opts.limit, |id, cold| {
            exclude.is_none_or(|e| !e.contains(id)) && (!opts.hot_only || !cold)
        })
    } else {
        store.search(vector, opts.limit)
    }
}

/// v4 `memoryIndex.shape(characterId, queryDimensions)`.
pub fn shape(conn: &Connection, character_id: &str, query_dimensions: usize) -> MemoryIndexShape {
    let store = CharacterVectorStore::load(conn, character_id);
    MemoryIndexShape {
        size: store.size(),
        dominant_dimensions: store.dimensions(),
        matching: store.count_with_dimensions(query_dimensions),
    }
}

/// v4 `memoryIndex.entries(characterId)` — every entry, for read-only walks.
pub fn entries(conn: &Connection, character_id: &str) -> Vec<VectorEntry> {
    CharacterVectorStore::load(conn, character_id)
        .entries()
        .to_vec()
}

/// v4 `memoryIndex.upsert(characterId, entries)` (`memory-index.ts:158-193`):
/// each entry's `alongside` fields + its embedding in ONE
/// `updateForCharacter`, in order; a row the write did not find (gone, or
/// another character's) answers `false` and logs DEBUG `Upsert found no row;
/// vector not cached {module, characterId, memoryId}`; then DEBUG `Upserted
/// memory vectors {module, characterId, count, withFields, cached}` (`cached`
/// the rows found — L-2's loaded answer). Empty → `[]`, no line. On the
/// writer's connection.
pub fn upsert(
    conn: &Connection,
    character_id: &str,
    entries: Vec<MemoryIndexUpsert>,
) -> Result<Vec<bool>, DbError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let repo = MemoriesRepository::new(conn);
    let count = entries.len();
    let with_fields = entries
        .iter()
        .filter(|e| e.alongside.names_any_field())
        .count();
    let mut found = Vec::with_capacity(count);
    let mut ids = Vec::with_capacity(count);
    for entry in entries {
        let mut patch = entry.alongside;
        patch.embedding = Some(Some(entry.embedding));
        found.push(repo.update_for_character(character_id, &entry.id, &patch)?);
        ids.push(entry.id);
    }
    let mut cached = 0usize;
    for (id, row) in ids.iter().zip(&found) {
        if *row {
            cached += 1;
        } else {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                memoryId = %id,
                "Upsert found no row; vector not cached"
            );
        }
    }
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        count = count,
        withFields = with_fields,
        cached = cached,
        "Upserted memory vectors"
    );
    Ok(found)
}

/// v4 `memoryIndex.indexInserted(characterId, entries)` (`:195-205`): the rows
/// were inserted WITH their embeddings by their creator, so there is nothing to
/// write; DEBUG `Indexed newly inserted memories {module, characterId, count,
/// cached}` (`cached` = every entry — L-2). Empty → no line.
pub fn index_inserted(character_id: &str, entries: &[MemoryIndexEntry]) {
    if entries.is_empty() {
        return;
    }
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        count = entries.len(),
        cached = entries.len(),
        "Indexed newly inserted memories"
    );
}

/// The rows of `ids` (of `character_id`) the LOADED index would hold — a
/// stored embedding that decodes non-empty — with their tiers; silent (no read
/// line: v4's cache is already loaded).
fn cached_rows(
    conn: &Connection,
    character_id: &str,
    ids: &[String],
) -> Result<Vec<(String, Tier)>, DbError> {
    let mut out = Vec::new();
    for chunk in crate::chunk::chunk_array(ids, crate::chunk::SQLITE_VARIABLE_CHUNK_SIZE) {
        let sql = format!(
            "SELECT id, embedding, tier FROM memories WHERE characterId = ? AND embedding IS NOT NULL AND id IN ({})",
            vec!["?"; chunk.len()].join(", ")
        );
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&character_id];
        for id in chunk {
            params.push(id);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params.as_slice(), |r| {
                let decodes = crate::db::memories_read::decode_stored_embedding(r.get_ref(1)?)
                    .is_some_and(|e| !e.is_empty());
                let tier = match r.get_ref(2)? {
                    rusqlite::types::ValueRef::Text(t) if t == b"cold" => Tier::Cold,
                    _ => Tier::Hot,
                };
                Ok((r.get::<_, String>(0)?, decodes, tier))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        out.extend(
            rows.into_iter()
                .filter(|(_, decodes, _)| *decodes)
                .map(|(id, _, tier)| (id, tier)),
        );
    }
    Ok(out)
}

/// v4 `memoryIndex.setTier(characterId, ids, tier, extra)` (`:207-226`): the
/// rows through `updateTierBulk` (its own DEBUG), the loaded index re-stamped
/// (v4's `Re-stamped vector tiers` DEBUG — `changed` counts the cached rows
/// whose tier moves, read BEFORE the write), then DEBUG `Moved memories
/// between tiers through the index {module, characterId, tier, requested,
/// changed}` (`changed` the rows the write changed). Empty → `0`, no line. On
/// the writer's connection.
pub fn set_tier(
    conn: &Connection,
    character_id: &str,
    ids: &[String],
    tier: Tier,
    extra: TierExtra,
) -> Result<i64, DbError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let restamped = cached_rows(conn, character_id, ids)?
        .into_iter()
        .filter(|(_, t)| *t != tier)
        .count();
    let changed =
        MemoriesRepository::new(conn).update_tier_bulk(character_id, ids, tier.as_str(), extra)?;
    tracing::debug!(
        context = "CharacterVectorStore.setTier",
        characterId = %character_id,
        tier = tier.as_str(),
        requested = ids.len(),
        changed = restamped,
        "Re-stamped vector tiers"
    );
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        tier = tier.as_str(),
        requested = ids.len(),
        changed = changed,
        "Moved memories between tiers through the index"
    );
    Ok(changed)
}

/// The loaded index's share of `ids` — rows with a stored embedding that
/// decodes — read BEFORE a retire deletes them, in the same closure, for
/// [`remove`]'s `removed` (L-2).
pub fn indexed_among(
    conn: &Connection,
    character_id: &str,
    ids: &[String],
) -> Result<usize, DbError> {
    Ok(cached_rows(conn, character_id, ids)?.len())
}

/// v4 `memoryIndex.remove(characterId, ids)` (`:228-238`) — the eviction after
/// a retire: DEBUG `Evicted retired memories from the index {module,
/// characterId, requested, removed}` (`removed` from [`indexed_among`], taken
/// before the delete). `requested == 0` → `0`, no line.
pub fn remove(character_id: &str, requested: usize, removed: usize) -> usize {
    if requested == 0 {
        return 0;
    }
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        requested = requested,
        removed = removed,
        "Evicted retired memories from the index"
    );
    removed
}

/// v4 `EnqueueMissingEmbeddingsResult` (`index-store/backfill.ts:103-105`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueMissingEmbeddingsResult {
    Ok { enqueued: usize, remaining: i64 },
    NoProfile { remaining: i64 },
}

impl EnqueueMissingEmbeddingsResult {
    /// v4's object, key order `status, enqueued, remaining`.
    pub fn to_json(self) -> Value {
        match self {
            Self::Ok {
                enqueued,
                remaining,
            } => {
                json!({ "status": "ok", "enqueued": enqueued, "remaining": remaining })
            }
            Self::NoProfile { remaining } => {
                json!({ "status": "no-profile", "remaining": remaining })
            }
        }
    }
}

/// v4 `enqueueMissingEmbeddings({userId, characterId, limit})`
/// (`index-store/backfill.ts:115-150`): up to `limit` memories with no
/// embedding (one character, or all — `createdAt` DESC); none → `Ok {0, 0}`;
/// no default embedding profile for the user → `NoProfile {remaining}`; else
/// ONE batch enqueue (`queue_service::enqueue_embedding_generate_many` —
/// payload `{entityType: 'MEMORY', entityId, characterId, profileId}`), then
/// DEBUG `[Memory backfill] Embedding jobs enqueued {characterId ('(all)'
/// when absent), enqueued, remaining}` — its bytes P4.D292's pin (§R.5).
/// `remaining` counts every row still without an embedding, the ones just
/// queued included until their jobs run.
pub async fn enqueue_missing_embeddings(
    db: &Db,
    user_id: &str,
    character_id: Option<&str>,
    limit: i64,
) -> Result<EnqueueMissingEmbeddingsResult, DbError> {
    let cid = character_id.map(str::to_string);
    let uid = user_id.to_string();
    let (missing, profile) = db.read_main(move |conn| {
        let missing = crate::db::memories_read::find_ids_without_embedding(
            conn,
            cid.as_deref(),
            Some(limit),
        )?;
        let profile = if missing.is_empty() {
            None
        } else {
            crate::db::embedding_profiles::find_default(conn, &uid)?
        };
        Ok((missing, profile))
    })?;
    if missing.is_empty() {
        return Ok(EnqueueMissingEmbeddingsResult::Ok {
            enqueued: 0,
            remaining: 0,
        });
    }
    let remaining_of = |db: &Db| -> Result<i64, DbError> {
        let cid = character_id.map(str::to_string);
        db.read_main(move |conn| {
            crate::db::memories_read::count_without_embedding(conn, cid.as_deref())
        })
    };
    let Some(profile) = profile else {
        return Ok(EnqueueMissingEmbeddingsResult::NoProfile {
            remaining: remaining_of(db)?,
        });
    };
    let payloads: Vec<Value> = missing
        .iter()
        .map(|m| {
            json!({
                "entityType": "MEMORY",
                "entityId": m.get("id").cloned().unwrap_or(Value::Null),
                "characterId": m.get("characterId").cloned().unwrap_or(Value::Null),
                "profileId": profile.id,
            })
        })
        .collect();
    let job_ids =
        crate::services::queue_service::enqueue_embedding_generate_many(db, user_id, &payloads)
            .await?;
    let remaining = remaining_of(db)?;
    tracing::debug!(
        characterId = %character_id.unwrap_or("(all)"),
        enqueued = job_ids.len(),
        remaining = remaining,
        "[Memory backfill] Embedding jobs enqueued"
    );
    Ok(EnqueueMissingEmbeddingsResult::Ok {
        enqueued: job_ids.len(),
        remaining,
    })
}
