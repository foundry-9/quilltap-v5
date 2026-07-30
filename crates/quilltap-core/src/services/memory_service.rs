//! The **memory-service cascade-delete family** — v4 `lib/memory/memory-service.ts`
//! `deleteMemoryWithVector` / `deleteMemoriesBySourceMessageWithVectors` /
//! `deleteMemoriesBySourceMessagesWithVectors` / `deleteMemoriesByChatIdWithVectors`.
//!
//! These are the vector-store-aware wrappers around the deletion chokepoint
//! ([`MemoriesRepository::delete_with_unlink`] / [`delete_many_with_unlink`]):
//! every path that deletes memories in bulk (a single UI delete, a source-message
//! cascade, a swipe-group cascade, a chat wipe) goes through one of these so the
//! rows are unlinked from neighbours' `relatedMemoryIds` **and** their entries are
//! removed from the per-character vector stores (with the store metadata bumped
//! only when something was actually removed — a store untouched by the sweep keeps
//! its `updatedAt`).
//!
//! No model call anywhere — this family is pure DB effect, so it is verified by a
//! plain tier-2 differential (`memory_cascade_tier2_equivalence`), not tier-3.
//!
//! ## Faithful v4 shapes
//!
//! * `delete_memory_with_vector` confirms ownership first (the chokepoint is
//!   characterId-agnostic), deletes through the chokepoint, **then** removes the
//!   vector — and the vector cleanup is non-fatal (v4 wraps it in try/catch and
//!   still returns `true`).
//! * The three cascades read the doomed set, group it by character in
//!   first-appearance order (v4's `Map` insertion order), remove each character's
//!   vectors (counting only ids the store actually held — `hasVector` first) with a
//!   per-character non-fatal guard, and only then run the chokepoint batch delete.
//! * The swipe-group variant gathers every memory across the whole group up front
//!   so the chokepoint's neighbour scan sweeps the `relatedMemoryIds` column once.
//!
//! [`MemoriesRepository::delete_with_unlink`]: crate::db::memories::MemoriesRepository::delete_with_unlink
//! [`delete_many_with_unlink`]: crate::db::memories::MemoriesRepository::delete_many_with_unlink
//!
//! ## `searchMemoriesSemantic` (Phase-3 Unit-3 wave 3)
//!
//! This module also carries [`search_memories_semantic`] — v4's semantic memory
//! recall (embed the query, search the character's [`CharacterVectorStore`], hydrate
//! the matches, blend cosine with the decaying effective weight (the ranking
//! blend — see `compute_ranking_blend`), sort,
//! and slice). It is the read half the first-message-context builder composes.
//! Since P4.d13 (episodic round 2) the FULL v4 surface is ported: the
//! `recall_context` targeting-tag re-ranking (the multiplier loop with
//! `recallAdjustment` records), one-hop related-memory expansion (item 5,
//! `RELATED_EXPANSION` caps), the `occurred_within` two-stage event-time window
//! (hard filter → soft ×1.3 fallback boost), `entity_anchors` (the
//! `searchByContent` union path — anchors union, only the tool-path literal
//! phrase boosts), and the retrospective `extra_probes` multi-probe union
//! (cap 2 extras, per-memory max cosine). Everything else was already
//! faithful: the one-retry-free embed, the dimension-mismatch → text fallback
//! (WITHOUT the access-time bump — v4 returns `searchMemoriesText` directly
//! there), the min-score / min-importance / source / aboutCharacterId filters,
//! and the `bumpAccessTimes` side-effect (final slice only).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};
use crate::embedding_vector::cosine_similarity;
use crate::literal_boost::{apply_literal_boost, contains_literal_phrase, get_literal_phrase};
use crate::memory_weighting::{
    calculate_effective_weight, compute_ranking_blend, default_min_cosine_for_provider,
    MemoryInputs, DEFAULT_WEIGHTING_CONFIG,
};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::recall_tags::{
    combine_recall_multipliers, ContextTag, MemoryTagView, RecallContext, ScopePolicy, TemporalTag,
    TimeWindow, RELATED_EXPANSION_MAX_PER_HIT, RELATED_EXPANSION_MAX_TOTAL,
};

/// Result of a source-message / swipe-group cascade (v4's
/// `{ deleted, vectorsRemoved }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CascadeDeleteResult {
    pub deleted: i64,
    pub vectors_removed: i64,
}

/// Result of a chat wipe (v4's `{ deleted, vectorsRemoved, characterCount }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatCascadeDeleteResult {
    pub deleted: i64,
    pub vectors_removed: i64,
    pub character_count: usize,
}

/// Delete a memory and remove its vector (v4 `deleteMemoryWithVector`). Returns
/// `false` — writing nothing — when the memory does not exist or belongs to a
/// different character (the ownership check precedes the characterId-agnostic
/// chokepoint). The vector cleanup after a successful delete is non-fatal.
pub async fn delete_memory_with_vector(
    db: &Db,
    character_id: &str,
    memory_id: &str,
) -> Result<bool, DbError> {
    let id = memory_id.to_string();
    let existing = db.read_main(move |conn| memories_read::find_by_id(conn, &id))?;
    let owned = existing
        .as_ref()
        .and_then(|m| m.get("characterId"))
        .and_then(Value::as_str)
        == Some(character_id);
    if !owned {
        return Ok(false);
    }

    let id = memory_id.to_string();
    let deleted = db
        .write(move |writers| writers.main().memories().delete_with_unlink(&id))
        .await?;
    if !deleted {
        return Ok(false);
    }

    // Remove from the vector store — non-fatal (v4 logs a warn and still returns
    // true, so a store failure must not turn a completed delete into an error).
    let char_id = character_id.to_string();
    let id = memory_id.to_string();
    let _ = db
        .write(move |writers| {
            let main = writers.main();
            let mut store = CharacterVectorStore::load(main.connection(), &char_id)?;
            store.remove_vector(&id);
            store.flush(&main.vector_indices())?;
            Ok(())
        })
        .await;

    Ok(true)
}

/// Delete all memories for a source message with vector-store cleanup (v4
/// `deleteMemoriesBySourceMessageWithVectors`). Handles the multi-character case —
/// one message may have produced memories for several characters.
pub async fn delete_memories_by_source_message_with_vectors(
    db: &Db,
    source_message_id: &str,
) -> Result<CascadeDeleteResult, DbError> {
    let smid = source_message_id.to_string();
    let memories =
        db.read_main(move |conn| memories_read::find_by_source_message_id(conn, &smid))?;
    if memories.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }
    cascade_delete(db, &memories).await
}

/// Delete all memories for a whole swipe group with vector cleanup (v4
/// `deleteMemoriesBySourceMessagesWithVectors`). Gathers every memory across the
/// group up front so the chokepoint scan sweeps `relatedMemoryIds` once.
pub async fn delete_memories_by_source_messages_with_vectors(
    db: &Db,
    source_message_ids: &[String],
) -> Result<CascadeDeleteResult, DbError> {
    if source_message_ids.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }

    let mut all_memories: Vec<Value> = Vec::new();
    for smid in source_message_ids {
        let smid = smid.clone();
        let slice =
            db.read_main(move |conn| memories_read::find_by_source_message_id(conn, &smid))?;
        all_memories.extend(slice);
    }
    if all_memories.is_empty() {
        return Ok(CascadeDeleteResult::default());
    }
    cascade_delete(db, &all_memories).await
}

/// Delete every memory tied to a chat (across all characters) and remove their
/// vector entries (v4 `deleteMemoriesByChatIdWithVectors`) — the chat-wipe path.
pub async fn delete_memories_by_chat_id_with_vectors(
    db: &Db,
    chat_id: &str,
) -> Result<ChatCascadeDeleteResult, DbError> {
    let cid = chat_id.to_string();
    let memories = db.read_main(move |conn| memories_read::find_by_chat_id(conn, &cid))?;
    if memories.is_empty() {
        return Ok(ChatCascadeDeleteResult::default());
    }

    let character_count = group_by_character(&memories).len();
    let CascadeDeleteResult {
        deleted,
        vectors_removed,
    } = cascade_delete(db, &memories).await?;
    Ok(ChatCascadeDeleteResult {
        deleted,
        vectors_removed,
        character_count,
    })
}

/// The shared cascade body: group by character, remove each group's vectors, then
/// batch-delete every row through the chokepoint (v4's shared middle section).
async fn cascade_delete(db: &Db, memories: &[Value]) -> Result<CascadeDeleteResult, DbError> {
    let groups = group_by_character(memories);
    let vectors_removed = remove_vectors_grouped(db, groups).await;

    let all_ids: Vec<String> = memories.iter().filter_map(id_of).collect();
    let deleted = db
        .write(move |writers| writers.main().memories().delete_many_with_unlink(&all_ids))
        .await?;

    Ok(CascadeDeleteResult {
        deleted,
        vectors_removed,
    })
}

/// Group memory ids by `characterId` in first-appearance order (v4 builds a `Map`,
/// whose iteration follows insertion).
fn group_by_character(memories: &[Value]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for m in memories {
        let Some(char_id) = m.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        let Some(id) = id_of(m) else { continue };
        match groups.iter_mut().find(|(c, _)| c == char_id) {
            Some((_, ids)) => ids.push(id),
            None => groups.push((char_id.to_string(), vec![id])),
        }
    }
    groups
}

/// Remove each character group's vectors, counting only ids the store actually
/// held (v4's `hasVector` check before `removeVector`). Each character's cleanup
/// is non-fatal (v4's per-character try/catch: a failed store must not abort the
/// cascade — the chokepoint delete still runs).
async fn remove_vectors_grouped(db: &Db, groups: Vec<(String, Vec<String>)>) -> i64 {
    let mut total = 0i64;
    for (character_id, memory_ids) in groups {
        let removed = db
            .write(move |writers| {
                let main = writers.main();
                let mut store = CharacterVectorStore::load(main.connection(), &character_id)?;
                let mut n = 0i64;
                for id in &memory_ids {
                    if store.has_vector(id) && store.remove_vector(id) {
                        n += 1;
                    }
                }
                store.flush(&main.vector_indices())?;
                Ok(n)
            })
            .await;
        total += removed.unwrap_or(0);
    }
    total
}

fn id_of(memory: &Value) -> Option<String> {
    memory.get("id").and_then(Value::as_str).map(str::to_string)
}

// ---------------------------------------------------------------------------
// searchMemoriesSemantic (v4 lib/memory/memory-service.ts)
// ---------------------------------------------------------------------------

/// Result of a semantic memory search (v4 `SemanticSearchResult`, restricted to the
/// fields the consumers read — `recallAdjustment` is part of the deferred
/// recallContext path).
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    /// The matching memory (net JSON, the shape [`memories_read::find_by_ids`] /
    /// [`memories_read::search_by_content`] return).
    pub memory: Value,
    /// Similarity score (0–1) — cosine (post literal-boost) for the embedding path,
    /// the text-match score for the fallback path.
    pub score: f64,
    /// Whether the embedding path produced this result.
    pub used_embedding: bool,
    /// Floored effective weight (diagnostics, NOT ranking).
    pub effective_weight: f64,
    /// No-floor ranking weight (base importance × time decay) — the retrieval blend
    /// input.
    pub raw_weight: f64,
    /// Recall-context adjustment record — present only when a `recall_context`
    /// was supplied (v4 `recallAdjustment`). Lets the injector/replay show *why*
    /// a memory ranked where it did.
    pub recall_adjustment: Option<RecallAdjustment>,
}

/// v4 `SemanticSearchResult.recallAdjustment`.
#[derive(Debug, Clone, PartialEq)]
pub struct RecallAdjustment {
    /// Combined, clamped multiplier applied to the blended score.
    pub multiplier: f64,
    /// Short labels for the adjustments that fired (e.g. `narrow✓`, `past↓`).
    pub fired: Vec<String>,
    /// Blended ranking score (see `compute_ranking_blend`) before the multiplier.
    pub blended_before: f64,
    /// Blended score after the multiplier (the value actually sorted on).
    pub blended_after: f64,
}

/// The owned form of [`crate::recall_tags::RecallContext`] carried on
/// [`SemanticSearchOptions`] (v4 `options.recallContext`). The per-row borrow
/// view is built inside the multiplier loop; `occurredWithin` is NOT here — v4
/// folds `options.occurredWithin` into the effective context only on the
/// window-starved soft-fallback arm (see `search_memories_semantic`).
#[derive(Debug, Clone, Default)]
pub struct RecallContextInput {
    pub current_project_id: Option<String>,
    pub scope_policy: ScopePolicy,
    pub present_about_character_ids: Vec<String>,
    pub turn_context: Option<ContextTag>,
    /// Carried for debug parity with v4 (the retrospective flag, not this
    /// guess, flips the temporal multipliers).
    pub turn_temporal: Option<TemporalTag>,
    pub turn_retrospective: bool,
    /// When true, one-hop related-memory expansion runs after the top hits are
    /// ranked (item 5, `RELATED_EXPANSION` caps).
    pub expand_related: bool,
    pub recently_whispered_ids: Option<std::collections::HashSet<String>>,
    /// The current chat's id — the fresh-event boost's echo guard (v4
    /// `recallContext.currentChatId`, set at all three build sites).
    pub current_chat_id: Option<String>,
    /// The fresh-event boost's reference clock in epoch ms (v4
    /// `recallContext.nowMs`). ⚠ NOT the same value as
    /// [`SemanticSearchOptions::now_ms`] in every caller: recall-replay passes
    /// the REPLAYED TURN's clock here and wall-clock now there.
    pub now_ms: Option<f64>,
}

/// Options for [`search_memories_semantic`] (v4 `MemoryServiceOptions & {...}`),
/// restricted to the fields the consumers pass plus the injected clock.
#[derive(Debug, Clone, Default)]
pub struct SemanticSearchOptions {
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
    /// v4 default 20.
    pub limit: Option<usize>,
    /// Explicit cosine floor; `None` → the provider default (`??` semantics: an
    /// explicit `Some(0.0)` disables the floor).
    pub min_score: Option<f64>,
    pub min_importance: Option<f64>,
    /// `'AUTO' | 'MANUAL'` filter.
    pub source: Option<String>,
    /// Restrict to memories held *about* this other character.
    pub about_character_id: Option<String>,
    /// Union verbatim query hits into the candidate pool + boost their cosine.
    pub apply_literal_phrase_boost: bool,
    /// Per-turn recall context (v4 `options.recallContext`). When supplied, the
    /// targeting-tag multipliers are applied to the blended score *after* the
    /// ranking blend is computed. Absent → ranking is byte-identical to the
    /// historical behavior.
    pub recall_context: Option<RecallContextInput>,
    /// Event-time window (episodic recall). Two-stage on the injector path (a
    /// `recall_context` is present): candidates are filtered to the window
    /// first; if fewer than `limit` survive, fall back to the unfiltered pool
    /// with window hits taking the bounded ×`OCCURRED_WITHIN_WINDOW` boost in
    /// the multiplier loop — never fewer results than an unwindowed search.
    /// Without a `recall_context` (tool path), this is a plain hard filter.
    pub occurred_within: Option<TimeWindow>,
    /// Entity strings unioned into the candidate pool via the literal
    /// `searchByContent` path, so a verbatim place name cannot be sliced off by
    /// the cosine floor. Injector-path companion to `apply_literal_phrase_boost`
    /// (which stays tool-only — anchors union, they do NOT boost).
    pub entity_anchors: Vec<String>,
    /// Additional embedding probes (retrospective turns only): each is
    /// embedded, its vector-store pool unioned with the main query's, and each
    /// memory keeps its max cosine across probes. Capped to 2 extras.
    pub extra_probes: Vec<String>,
    /// The `Date.now()` seam used by `calculateEffectiveWeight` (epoch millis). The
    /// blend's time-decay reads it; injected so the differential is deterministic.
    pub now_ms: f64,
}

/// How the vector path failed (decides the text-fallback bump semantics).
enum SemanticFail {
    /// v4's dimension-mismatch early return: `return searchMemoriesText(...)`
    /// — WITHOUT an access-time bump.
    DimMismatch,
    /// Any thrown error (embed failure, DB error) → the catch-block fallback,
    /// which DOES bump.
    Other,
}

/// v4 `searchMemoriesSemantic`. Tries the vector path first (embed → search →
/// [multi-probe union] → [anchor-phrase union] → hydrate → blend →
/// [recall-context multiplier loop + one-hop expansion] → sort → slice),
/// falling back to text search on any failure or a dimension mismatch.
pub async fn search_memories_semantic<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    query: &str,
    options: &SemanticSearchOptions,
) -> Result<Vec<SemanticSearchResult>, DbError> {
    let limit = options.limit.unwrap_or(20);
    let explicit_min_score = options.min_score;

    // Try semantic search first. Any error inside this block → text fallback (v4
    // wraps the whole body in try/catch).
    let semantic = try_semantic(
        db,
        provider,
        character_id,
        query,
        options,
        limit,
        explicit_min_score,
    )
    .await;

    match semantic {
        Ok(Some(mut results)) => {
            // v4: `finalResults = results.slice(0, limit)` then bump ONLY the
            // final slice's ids.
            results.truncate(limit);
            let ids: Vec<String> = results.iter().filter_map(|r| id_of(&r.memory)).collect();
            bump_access_times(db, character_id, &ids).await;
            Ok(results)
        }
        // v4's dimension-mismatch arm returns the text results DIRECTLY —
        // no access-time bump on this path.
        Err(SemanticFail::DimMismatch) => {
            search_memories_text(db, character_id, query, options).await
        }
        // `Ok(None)` = the vector pool was empty (v4 falls through past the
        // `if (augmentedVectorResults.length > 0)` block to the text fallback);
        // `Err(Other)` = the catch block. Both bump.
        Ok(None) | Err(SemanticFail::Other) => {
            let text = search_memories_text(db, character_id, query, options).await?;
            let ids: Vec<String> = text.iter().filter_map(|r| id_of(&r.memory)).collect();
            bump_access_times(db, character_id, &ids).await;
            Ok(text)
        }
    }
}

/// The vector path. `Ok(Some(sorted))` on a non-empty pool, `Ok(None)` when the pool
/// is empty (→ text fallback), `Err` on embed failure / dimension mismatch (→ text
/// fallback, with [`SemanticFail`] deciding the bump semantics).
#[allow(clippy::too_many_arguments)]
async fn try_semantic<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    query: &str,
    options: &SemanticSearchOptions,
    limit: usize,
    explicit_min_score: Option<f64>,
) -> Result<Option<Vec<SemanticSearchResult>>, SemanticFail> {
    let embed = provider
        .generate_embedding_for_user(
            query,
            &options.user_id,
            options.embedding_profile_id.as_deref(),
            EmbeddingPriority::Interactive,
        )
        .await
        .map_err(|_| SemanticFail::Other)?;

    let min_score = explicit_min_score
        .unwrap_or_else(|| default_min_cosine_for_provider(Some(&embed.provider)));

    // Load the store off the read pool; searches run in-memory on the loaded
    // store (v4 loads the store once and searches it per probe).
    let query_embedding = embed.embedding.clone();
    let store = {
        let cid = character_id.to_string();
        db.read_main(move |conn| CharacterVectorStore::load(conn, &cid))
            .map_err(|_| SemanticFail::Other)?
    };
    // The store's known dimension: the length of any stored entry (all entries
    // share it; v4 resolves it from the index metadata / first entry the same
    // way). `None` on an empty store → no mismatch guard.
    let stored_dimensions = store.all_entries().next().map(|(_, v)| v.len());

    // Dimension mismatch → text fallback (v4 returns searchMemoriesText
    // immediately, WITHOUT the access-time bump).
    if let Some(dims) = stored_dimensions {
        if query_embedding.len() != dims {
            return Err(SemanticFail::DimMismatch);
        }
    }

    // v4 searches limit*3 candidates.
    let mut vector_results = store.search(&query_embedding, limit.saturating_mul(3));

    // Multi-probe union (retrospective turns): embed each extra probe, union
    // its top-K pool with the main query's, keep each memory's max cosine.
    // Bounded cost (≤ 2 extra embeddings), gated to the turns that need it.
    let extra_probes: Vec<String> = options
        .extra_probes
        .iter()
        .map(|p| crate::jsstr::js_trim(p).to_string())
        .filter(|p| !p.is_empty())
        .take(2)
        .collect();
    if !extra_probes.is_empty() {
        // JS `Map` semantics: insertion order preserved; a higher-scoring
        // re-set keeps the original position.
        let mut order: Vec<String> = Vec::new();
        let mut by_id: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for vr in &vector_results {
            if !by_id.contains_key(&vr.id) {
                order.push(vr.id.clone());
            }
            by_id.insert(vr.id.clone(), vr.score);
        }
        for probe in &extra_probes {
            // A failed probe embedding is skipped (v4's per-probe try/catch).
            let Ok(probe_result) = provider
                .generate_embedding_for_user(
                    probe,
                    &options.user_id,
                    options.embedding_profile_id.as_deref(),
                    EmbeddingPriority::Interactive,
                )
                .await
            else {
                continue;
            };
            if probe_result.embedding.len() != query_embedding.len() {
                continue;
            }
            for vr in store.search(&probe_result.embedding, limit.saturating_mul(3)) {
                match by_id.get(&vr.id) {
                    Some(existing) if vr.score <= *existing => {}
                    Some(_) => {
                        by_id.insert(vr.id.clone(), vr.score);
                    }
                    None => {
                        order.push(vr.id.clone());
                        by_id.insert(vr.id.clone(), vr.score);
                    }
                }
            }
        }
        vector_results = order
            .into_iter()
            .map(|id| {
                let score = by_id[&id];
                crate::db::vector_store::VectorSearchResult { id, score }
            })
            .collect();
    }

    // Hybrid step: union literal text hits into the candidate pool. Two
    // sources of literal phrases: the whole query (tool path,
    // `apply_literal_phrase_boost`) and the turn's entity anchors (injector
    // path) — a verbatim place name must not be sliced off by the cosine
    // floor. Anchors only UNION; the cosine boost below stays gated on the
    // tool-path literal phrase.
    let literal_phrase = if options.apply_literal_phrase_boost {
        get_literal_phrase(Some(query))
    } else {
        None
    };
    let mut anchor_phrases: Vec<String> = Vec::new();
    if literal_phrase.is_some() {
        // v4 `query.trim()` — JS trim (Unicode whitespace class).
        anchor_phrases.push(crate::jsstr::js_trim(query).to_string());
    }
    for entity in options.entity_anchors.iter().take(3) {
        let trimmed = crate::jsstr::js_trim(entity);
        if !trimmed.is_empty() && crate::jsstr::utf16_len(trimmed) >= 2 {
            anchor_phrases.push(trimmed.to_string());
        }
    }

    let mut literal_hit_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // (id, score) candidate pool; the vector store's search order is preserved.
    let mut augmented: Vec<(String, f64)> = vector_results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect();

    if !anchor_phrases.is_empty() {
        // Direct hits across all anchor phrases, first-seen order (v4
        // `directSeen`), with EVERY hit recorded in `literal_hit_ids`.
        let mut direct_hit_memories: Vec<Value> = Vec::new();
        let mut direct_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for phrase in &anchor_phrases {
            let cid = character_id.to_string();
            let ph = phrase.clone();
            let hits = db
                .read_main(move |conn| memories_read::search_by_content(conn, &cid, &ph))
                .map_err(|_| SemanticFail::Other)?;
            for m in hits {
                let Some(id) = id_of(&m) else { continue };
                literal_hit_ids.insert(id.clone());
                if direct_seen.insert(id) {
                    direct_hit_memories.push(m);
                }
            }
        }
        let in_pool: std::collections::HashSet<String> =
            vector_results.iter().map(|r| r.id.clone()).collect();
        for m in &direct_hit_memories {
            let Some(id) = id_of(m) else { continue };
            if in_pool.contains(&id) {
                continue;
            }
            if let Some(emb) = memory_embedding(m) {
                if emb.len() == query_embedding.len() {
                    if let Ok(score) = cosine_similarity(&query_embedding, &emb) {
                        augmented.push((id, score));
                    }
                }
            }
        }
    }

    if augmented.is_empty() {
        return Ok(None);
    }

    // Hydrate the matched ids (only the top-K).
    let matched_ids: Vec<String> = augmented.iter().map(|(id, _)| id.clone()).collect();
    let memories = db
        .read_main(move |conn| memories_read::find_by_ids(conn, &matched_ids))
        .map_err(|_| SemanticFail::Other)?;
    let mut memory_map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for m in memories {
        if let Some(id) = id_of(&m) {
            memory_map.insert(id, m);
        }
    }

    let mut results: Vec<SemanticSearchResult> = Vec::new();
    for (id, raw_score) in &augmented {
        let Some(memory) = memory_map.get(id) else {
            continue;
        };
        let literal_hit = match &literal_phrase {
            Some(phrase) => {
                literal_hit_ids.contains(id)
                    || contains_literal_phrase(
                        memory.get("content").and_then(Value::as_str),
                        phrase,
                    )
                    || contains_literal_phrase(
                        memory.get("summary").and_then(Value::as_str),
                        phrase,
                    )
            }
            None => false,
        };
        // v4: literalHit ? applyLiteralBoost(vr.score) : vr.score  (default 0.5).
        let cosine_score = if literal_hit {
            apply_literal_boost(*raw_score, 0.5)
        } else {
            *raw_score
        };
        let ew = calculate_effective_weight(
            &memory_inputs(memory),
            &DEFAULT_WEIGHTING_CONFIG,
            options.now_ms,
        );
        results.push(SemanticSearchResult {
            memory: memory.clone(),
            score: cosine_score,
            used_embedding: true,
            effective_weight: ew.effective_weight,
            raw_weight: ew.raw_weight,
            recall_adjustment: None,
        });
    }

    // Filter: score >= minScore, then the optional filters.
    results.retain(|r| r.score >= min_score);
    if let Some(min_imp) = options.min_importance {
        results.retain(|r| {
            r.memory
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                >= min_imp
        });
    }
    if let Some(src) = &options.source {
        results.retain(|r| r.memory.get("source").and_then(Value::as_str) == Some(src.as_str()));
    }
    if let Some(about) = &options.about_character_id {
        results.retain(|r| {
            r.memory.get("aboutCharacterId").and_then(Value::as_str) == Some(about.as_str())
        });
    }

    // Event-time window (episodic recall) — two-stage on the injector path:
    // filter to the window first; if fewer than `limit` survive, fall back
    // to the unfiltered pool and let window hits take the bounded
    // ×occurredWithinWindow boost inside the one multiplier loop instead.
    // Never fewer results than an unwindowed search. Tool path (no
    // recall_context): a plain hard filter — the caller asked for a window.
    let mut soft_window: Option<&TimeWindow> = None;
    if let Some(win) = &options.occurred_within {
        if let (Some(from), Some(to)) = (
            crate::episodic::event_time_ms(Some(&win.from)),
            crate::episodic::event_time_ms(Some(&win.to)),
        ) {
            if from <= to {
                let in_window = |r: &SemanticSearchResult| -> bool {
                    let occ = r.memory.get("occurredAt").and_then(Value::as_str);
                    let iso = occ.or_else(|| r.memory.get("createdAt").and_then(Value::as_str));
                    match crate::episodic::event_time_ms(iso) {
                        Some(t) => t >= from && t <= to,
                        None => false,
                    }
                };
                let window_hits: Vec<SemanticSearchResult> =
                    results.iter().filter(|r| in_window(r)).cloned().collect();
                // Tool path (no recall context): a plain hard filter. Injector
                // path: hard filter only while it can still fill the head;
                // starved → keep the full pool and let the multiplier loop
                // apply the soft ×OCCURRED_WITHIN_WINDOW boost to window hits
                // (v4 folds occurredWithin into the effective context).
                if options.recall_context.is_none() || window_hits.len() >= limit {
                    results = window_hits;
                } else {
                    soft_window = Some(win);
                }
            }
        }
    }

    // Blended ranking key (see compute_ranking_blend). The blend itself is
    // never modified — when a recall context is supplied, the targeting-tag
    // adjustments are bounded, clamped multipliers applied to this blended
    // score *after* it is computed, so semantic relevance and recency keep
    // their relative footing and each adjustment is auditable in isolation.
    // No recall context → the exact historical sort, byte-for-byte.
    if let Some(rc) = &options.recall_context {
        let mut adjusted: Vec<SemanticSearchResult> = Vec::new();
        for mut r in results {
            let blended_before = compute_ranking_blend(r.score, r.raw_weight);
            let adj = apply_recall_multipliers(&r.memory, rc, soft_window);
            let Some(adj) = adj else {
                continue; // excluded (cross-project narrow + exclude policy)
            };
            let blended_after = blended_before * adj.0;
            r.recall_adjustment = Some(RecallAdjustment {
                multiplier: adj.0,
                fired: adj.1,
                blended_before,
                blended_after,
            });
            adjusted.push(r);
        }
        adjusted.sort_by(|a, b| {
            let sa = a
                .recall_adjustment
                .as_ref()
                .map(|x| x.blended_after)
                .unwrap_or(0.0);
            let sb = b
                .recall_adjustment
                .as_ref()
                .map(|x| x.blended_after)
                .unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Item 5 — one-hop related-memory expansion (only when the caller
        // opts in via recall_context.expand_related).
        let final_results = if rc.expand_related {
            expand_related_memories(
                db,
                adjusted,
                limit,
                &query_embedding,
                rc,
                soft_window,
                options,
                character_id,
            )?
        } else {
            adjusted
        };
        return Ok(Some(final_results));
    }

    // Sort by the blended ranking key (see compute_ranking_blend). Stable —
    // preserves the pool order among ties (JS Array.sort is stable).
    results.sort_by(|a, b| {
        let sa = compute_ranking_blend(a.score, a.raw_weight);
        let sb = compute_ranking_blend(b.score, b.raw_weight);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Some(results))
}

/// Run [`combine_recall_multipliers`] for one memory row (net JSON) against the
/// owned recall context (+ the soft window when the two-stage filter starved).
/// `None` = excluded. Returns `(multiplier, fired)` otherwise.
fn apply_recall_multipliers(
    memory: &Value,
    rc: &RecallContextInput,
    soft_window: Option<&TimeWindow>,
) -> Option<(f64, Vec<String>)> {
    let keywords: Vec<String> = memory
        .get("keywords")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let view = MemoryTagView {
        id: memory.get("id").and_then(Value::as_str),
        project_id: memory.get("projectId").and_then(Value::as_str),
        keywords: &keywords,
        about_character_id: memory.get("aboutCharacterId").and_then(Value::as_str),
        occurred_at: memory.get("occurredAt").and_then(Value::as_str),
        created_at: memory.get("createdAt").and_then(Value::as_str),
        // The memory's own chat (net JSON carries `chatId`) — the echo guard.
        chat_id: memory.get("chatId").and_then(Value::as_str),
    };
    let ctx = RecallContext {
        current_project_id: rc.current_project_id.as_deref(),
        scope_policy: rc.scope_policy,
        present_about_character_ids: &rc.present_about_character_ids,
        turn_context: rc.turn_context,
        turn_temporal: rc.turn_temporal,
        turn_retrospective: rc.turn_retrospective,
        occurred_within: soft_window,
        expand_related: rc.expand_related,
        recently_whispered_ids: rc.recently_whispered_ids.as_ref(),
        current_chat_id: rc.current_chat_id.as_deref(),
        now_ms: rc.now_ms,
    };
    let adj = combine_recall_multipliers(&view, &ctx);
    if adj.exclude {
        return None;
    }
    Some((
        adj.multiplier,
        adj.fired.iter().map(|s| s.to_string()).collect(),
    ))
}

/// Item 5 — one-hop related-memory expansion (v4 `expandRelatedMemories`).
///
/// Given the already-ranked candidate pool (`ranked`, sorted by
/// post-adjustment blended score), pull the strongly-linked neighbors of the
/// top hits in as extra candidates, score them against the same query
/// embedding, run them through the same blend + recall multipliers, union
/// them with the pool, and re-rank. Bounded on every axis
/// (`RELATED_EXPANSION_*`); neighbors already in the pool are skipped;
/// neighbors without a dimension-matching embedding are skipped; the same
/// `min_importance`/`source` filters apply. `min_score` is intentionally NOT
/// re-applied — a low-cosine neighbor relevant purely by association is the
/// whole point of expansion.
#[allow(clippy::too_many_arguments)]
fn expand_related_memories(
    db: &Db,
    ranked: Vec<SemanticSearchResult>,
    limit: usize,
    query_embedding: &[f32],
    rc: &RecallContextInput,
    soft_window: Option<&TimeWindow>,
    options: &SemanticSearchOptions,
    character_id: &str,
) -> Result<Vec<SemanticSearchResult>, SemanticFail> {
    let in_pool: std::collections::HashSet<String> =
        ranked.iter().filter_map(|r| id_of(&r.memory)).collect();

    // Collect capped neighbor ids from the top hits only.
    let mut neighbor_ids: Vec<String> = Vec::new();
    let mut neighbor_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    'seeds: for seed in ranked.iter().take(limit) {
        let mut pulled_from_seed = 0usize;
        let related = seed
            .memory
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for neighbor_id in related {
            if neighbor_ids.len() >= RELATED_EXPANSION_MAX_TOTAL {
                break 'seeds;
            }
            if pulled_from_seed >= RELATED_EXPANSION_MAX_PER_HIT {
                break;
            }
            if in_pool.contains(&neighbor_id) || neighbor_set.contains(&neighbor_id) {
                continue;
            }
            neighbor_set.insert(neighbor_id.clone());
            neighbor_ids.push(neighbor_id);
            pulled_from_seed += 1;
        }
    }

    if neighbor_ids.is_empty() {
        return Ok(ranked);
    }

    let neighbors = {
        let ids = neighbor_ids.clone();
        db.read_main(move |conn| memories_read::find_by_ids(conn, &ids))
            .map_err(|_| SemanticFail::Other)?
    };
    let mut survivors: Vec<SemanticSearchResult> = Vec::new();
    for memory in neighbors {
        // Character-scope + filter guards mirror the main pool.
        if memory.get("characterId").and_then(Value::as_str) != Some(character_id) {
            continue;
        }
        if let Some(min_imp) = options.min_importance {
            if memory
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                < min_imp
            {
                continue;
            }
        }
        if let Some(src) = &options.source {
            if memory.get("source").and_then(Value::as_str) != Some(src.as_str()) {
                continue;
            }
        }
        let Some(emb) = memory_embedding(&memory) else {
            continue;
        };
        if emb.len() != query_embedding.len() {
            continue;
        }
        let Ok(cosine_score) = cosine_similarity(query_embedding, &emb) else {
            continue;
        };
        let ew = calculate_effective_weight(
            &memory_inputs(&memory),
            &DEFAULT_WEIGHTING_CONFIG,
            options.now_ms,
        );
        let blended_before = compute_ranking_blend(cosine_score, ew.raw_weight);
        let Some((multiplier, mut fired)) = apply_recall_multipliers(&memory, rc, soft_window)
        else {
            continue;
        };
        let blended_after = blended_before * multiplier;
        fired.push("related↗".to_string());
        survivors.push(SemanticSearchResult {
            memory,
            score: cosine_score,
            used_embedding: true,
            effective_weight: ew.effective_weight,
            raw_weight: ew.raw_weight,
            recall_adjustment: Some(RecallAdjustment {
                multiplier,
                fired,
                blended_before,
                blended_after,
            }),
        });
    }

    if survivors.is_empty() {
        return Ok(ranked);
    }

    let mut union: Vec<SemanticSearchResult> = ranked;
    union.extend(survivors);
    union.sort_by(|a, b| {
        let sa = a
            .recall_adjustment
            .as_ref()
            .map(|x| x.blended_after)
            .unwrap_or(0.0);
        let sb = b
            .recall_adjustment
            .as_ref()
            .map(|x| x.blended_after)
            .unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(union)
}

/// v4 `searchMemoriesText` (the fallback when embeddings are unavailable): full-phrase
/// `searchByContent`, broadened to per-significant-word searches when short, then a
/// weighted text score + the decaying-weight blend sort.
async fn search_memories_text(
    db: &Db,
    character_id: &str,
    query: &str,
    options: &SemanticSearchOptions,
) -> Result<Vec<SemanticSearchResult>, DbError> {
    let limit = options.limit.unwrap_or(20);

    // Full-phrase search first.
    let mut memories = {
        let cid = character_id.to_string();
        let q = query.to_string();
        db.read_main(move |conn| memories_read::search_by_content(conn, &cid, &q))?
    };

    // Broaden to per-word search when short (stop-words filtered).
    let query_words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.chars().count() > 2 && !STOP_WORDS.contains(w))
        .map(str::to_string)
        .collect();
    if memories.len() < limit && !query_words.is_empty() {
        let mut existing: std::collections::HashSet<String> =
            memories.iter().filter_map(id_of).collect();
        for word in &query_words {
            let cid = character_id.to_string();
            let w = word.clone();
            let word_results =
                db.read_main(move |conn| memories_read::search_by_content(conn, &cid, &w))?;
            for mem in word_results {
                if let Some(id) = id_of(&mem) {
                    if existing.insert(id) {
                        memories.push(mem);
                    }
                }
            }
        }
    }

    // Filters.
    if let Some(min_imp) = options.min_importance {
        memories.retain(|m| m.get("importance").and_then(Value::as_f64).unwrap_or(0.0) >= min_imp);
    }
    if let Some(src) = &options.source {
        memories.retain(|m| m.get("source").and_then(Value::as_str) == Some(src.as_str()));
    }
    if let Some(about) = &options.about_character_id {
        memories
            .retain(|m| m.get("aboutCharacterId").and_then(Value::as_str) == Some(about.as_str()));
    }

    // Score.
    let query_lower = query.to_lowercase();
    let mut results: Vec<SemanticSearchResult> = memories
        .into_iter()
        .map(|memory| {
            let mut score = 0.0_f64;
            let content_lower = memory
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let summary_lower = memory
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();

            if summary_lower.contains(&query_lower) {
                score += 0.4;
            }
            if content_lower.contains(&query_lower) {
                score += 0.3;
            }
            if !query_words.is_empty() {
                let cwm = query_words
                    .iter()
                    .filter(|w| content_lower.contains(w.as_str()))
                    .count();
                let swm = query_words
                    .iter()
                    .filter(|w| summary_lower.contains(w.as_str()))
                    .count();
                score += 0.2 * (cwm as f64 / query_words.len() as f64);
                score += 0.1 * (swm as f64 / query_words.len() as f64);
            }
            // Keyword matches.
            let keywords: Vec<String> = memory
                .get("keywords")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let matching = keywords
                .iter()
                .filter(|kw| {
                    let kwl = kw.to_lowercase();
                    query_words.iter().any(|qw| kwl.contains(qw.as_str()))
                })
                .count();
            score += 0.1 * (matching as f64 / (keywords.len().max(1) as f64));

            let ew = calculate_effective_weight(
                &memory_inputs(&memory),
                &DEFAULT_WEIGHTING_CONFIG,
                options.now_ms,
            );
            SemanticSearchResult {
                memory,
                score: score.min(1.0),
                used_embedding: false,
                effective_weight: ew.effective_weight,
                raw_weight: ew.raw_weight,
                recall_adjustment: None,
            }
        })
        .collect();

    // Drop zero-score results, blend-sort.
    results.retain(|r| r.score > 0.0);
    results.sort_by(|a, b| {
        let sa = compute_ranking_blend(a.score, a.raw_weight);
        let sb = compute_ranking_blend(b.score, b.raw_weight);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
}

/// The stop-word set v4 uses in `searchMemoriesText`.
static STOP_WORDS: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        [
            "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with",
            "by", "is", "was", "are", "were", "be", "been", "being", "have", "has", "had", "do",
            "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
            "that", "this", "these", "those", "it", "its", "my", "your", "his", "her", "our",
            "their", "what", "which", "who", "whom", "how", "when", "where", "why", "not", "no",
            "nor", "if", "then", "than", "so", "as", "about", "from", "into", "up", "out", "off",
            "over", "under", "again", "before", "after", "between", "through",
        ]
        .into_iter()
        .collect()
    });

/// Build [`MemoryInputs`] from a memory `Value` (the ISO→ms + graph-degree mapping,
/// same as the housekeeping service's `parse_mem`).
fn memory_inputs(v: &Value) -> MemoryInputs {
    let num = |k: &str| v.get(k).and_then(Value::as_f64);
    let ms = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .and_then(crate::clock::iso_to_ms)
            .map(|m| m as f64)
    };
    MemoryInputs {
        importance: num("importance").unwrap_or(0.0),
        reinforced_importance: num("reinforcedImportance"),
        created_at_ms: ms("createdAt").unwrap_or(f64::NAN),
        last_reinforced_at_ms: ms("lastReinforcedAt"),
        last_accessed_at_ms: ms("lastAccessedAt"),
        reinforcement_count: num("reinforcementCount").map(|c| c as u64),
        graph_degree: v
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0),
        // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
        kind_episodic: v.get("kind").and_then(Value::as_str) == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(v.get("occurredAt").and_then(Value::as_str)),
    }
}

/// Decode the `{"0":v0,…}` Float32Array-shaped embedding a memory `Value` carries
/// into a `Vec<f32>` (v4's `memory.embedding`). Ascending integer-key order.
fn memory_embedding(v: &Value) -> Option<Vec<f32>> {
    let obj = v.get("embedding").and_then(Value::as_object)?;
    if obj.is_empty() {
        return None;
    }
    let mut pairs: Vec<(usize, f32)> = obj
        .iter()
        .filter_map(|(k, val)| Some((k.parse::<usize>().ok()?, val.as_f64()? as f32)))
        .collect();
    pairs.sort_by_key(|(i, _)| *i);
    Some(pairs.into_iter().map(|(_, f)| f).collect())
}

/// Fire-and-forget `updateAccessTimeBulk` (v4 `bumpAccessTimes`). Non-fatal: a
/// failure is swallowed. Awaited here (v4 `void`s the promise) — same committed
/// effect once settled.
async fn bump_access_times(db: &Db, character_id: &str, memory_ids: &[String]) {
    if memory_ids.is_empty() {
        return;
    }
    let cid = character_id.to_string();
    let ids = memory_ids.to_vec();
    let _ = db
        .write(move |writers| {
            writers
                .main()
                .memories()
                .update_access_time_bulk(&cid, &ids)
        })
        .await;
}

// ============================================================================
// P4.6BL tier 2 — the two repair services behind the memories route arms
// (v4 `generateMissingEmbeddings` / `rebuildVectorIndex`,
// lib/memory/memory-service.ts:1325 / :1392).
// ============================================================================

/// v4 `generateMissingEmbeddings`'s `{ processed, failed, skipped }`.
#[derive(Debug, Default, PartialEq)]
pub struct GenerateMissingEmbeddingsResult {
    pub processed: usize,
    pub failed: usize,
    /// Always 0 in v4 (the counter exists but nothing increments it) —
    /// carried for response-shape fidelity.
    pub skipped: usize,
}

/// v4 `rebuildVectorIndex`'s `{ indexed, failed }`.
#[derive(Debug, Default, PartialEq)]
pub struct RebuildVectorIndexResult {
    pub indexed: usize,
    pub failed: usize,
}

/// The anchors view a hydrated memory row satisfies in v4 (the `memory` object
/// IS the `EpisodicAnchorView` there — `buildMemoryEmbeddingText(summary,
/// content, memory)`).
fn anchor_view_of(memory: &Value) -> crate::episodic::EpisodicAnchorView {
    let entities = match memory.get("entities") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(raw)) => serde_json::from_str::<Vec<String>>(raw).unwrap_or_default(),
        _ => Vec::new(),
    };
    crate::episodic::EpisodicAnchorView {
        occurred_at: memory
            .get("occurredAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        narrative_time: memory
            .get("narrativeTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        entities,
    }
}

/// Decode the `Float32Array`-object shape `memories_read` emits for a stored
/// embedding (`{"0": x, "1": y, …}`) back to a vector. `None` for anything
/// else (null / absent / malformed).
fn embedding_vec_of(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding")?.as_object()?;
    let mut out = vec![0f32; obj.len()];
    for (k, v) in obj {
        let i: usize = k.parse().ok()?;
        if i >= out.len() {
            return None;
        }
        out[i] = v.as_f64()? as f32;
    }
    Some(out)
}

/// True when the row's stored embedding is absent or empty (v4's
/// `!m.embedding || m.embedding.length === 0` filter).
fn embedding_missing(memory: &Value) -> bool {
    match embedding_vec_of(memory) {
        Some(v) => v.is_empty(),
        None => true,
    }
}

/// v4 `generateMissingEmbeddings(characterId, { userId, batchSize })` — embed
/// every memory that lacks an embedding, updating the row AND the in-memory
/// vector store (flushed every `batch_size` successes and once at the end).
/// A per-memory failure (embed OR write) is counted and the sweep continues
/// (v4's catch → `failed++`). Uses the anchor-aware
/// [`crate::episodic::build_memory_embedding_text`] — unlike the
/// EMBEDDING_GENERATE handler's plain concat, and exactly like v4.
pub async fn generate_missing_embeddings<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    user_id: &str,
    character_id: &str,
    embedding_profile_id: Option<&str>,
    batch_size: usize,
) -> Result<GenerateMissingEmbeddingsResult, DbError> {
    let cid = character_id.to_string();
    let memories = db.read_main(move |c| memories_read::find_by_character_id(c, &cid))?;
    let missing: Vec<&Value> = memories.iter().filter(|m| embedding_missing(m)).collect();

    let cid = character_id.to_string();
    let mut store = db.read_main(move |c| CharacterVectorStore::load(c, &cid))?;

    let mut result = GenerateMissingEmbeddingsResult::default();
    for memory in missing {
        let id = memory
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let summary = memory
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let content = memory
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let anchors = anchor_view_of(memory);
        let text = crate::episodic::build_memory_embedding_text(summary, content, Some(&anchors));

        // v4's try block: embed → updateForCharacter → store.addVector. Any
        // failure counts and the loop continues.
        let step: Result<(), String> = async {
            let embedded = provider
                .generate_embedding_for_user(
                    &text,
                    user_id,
                    embedding_profile_id,
                    EmbeddingPriority::Background,
                )
                .await
                .map_err(|e| e.message)?;
            let (cid, mid, vec) = (
                character_id.to_string(),
                id.clone(),
                embedded.embedding.clone(),
            );
            db.write(move |ws| {
                let patch = crate::db::memories::MemUpdate {
                    embedding: Some(Some(vec.clone())),
                    ..Default::default()
                };
                ws.main()
                    .memories()
                    .update_for_character(&cid, &mid, &patch)
            })
            .await
            .map_err(|e| format!("{e}"))?;
            store
                .add_vector(&id, embedded.embedding)
                .map_err(|e| format!("{e}"))?;
            Ok(())
        }
        .await;

        match step {
            Ok(()) => {
                result.processed += 1;
                // v4: save periodically every `batchSize` successes.
                if batch_size > 0 && result.processed % batch_size == 0 {
                    store = flush_store(db, store).await?;
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::memories",
                    memory_id = %id,
                    character_id,
                    error = %e,
                    "[Memory] Failed to generate embedding for memory",
                );
                result.failed += 1;
            }
        }
    }

    // v4's final save.
    flush_store(db, store).await?;
    Ok(result)
}

/// v4 `rebuildVectorIndex(characterId, { userId })` — delete the character's
/// vector index outright, then re-add every memory that HAS an embedding and
/// persist. A per-memory add failure counts and the loop continues.
pub async fn rebuild_vector_index(
    db: &Db,
    character_id: &str,
) -> Result<RebuildVectorIndexResult, DbError> {
    // v4 `manager.deleteStore(characterId)` → `repo.deleteByCharacterId`.
    let cid = character_id.to_string();
    db.write(move |ws| {
        crate::db::vector_indices::VectorIndicesRepository::new(ws.main().connection())
            .delete_by_character_id(&cid)
            .map(|_| ())
    })
    .await?;

    // v4 `manager.getStore` — a fresh (now empty) store.
    let cid = character_id.to_string();
    let mut store = db.read_main(move |c| CharacterVectorStore::load(c, &cid))?;

    let cid = character_id.to_string();
    let memories = db.read_main(move |c| memories_read::find_by_character_id(c, &cid))?;

    let mut result = RebuildVectorIndexResult::default();
    for memory in &memories {
        let Some(vec) = embedding_vec_of(memory).filter(|v| !v.is_empty()) else {
            continue;
        };
        let id = memory
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match store.add_vector(&id, vec) {
            Ok(()) => result.indexed += 1,
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::memories",
                    memory_id = %id,
                    character_id,
                    error = %e,
                    "[Memory] Failed to index memory",
                );
                result.failed += 1;
            }
        }
    }

    flush_store(db, store).await?;
    Ok(result)
}

/// Run `store.flush` through the writer, handing the store back (the loop keeps
/// mutating it between flushes).
async fn flush_store(
    db: &Db,
    mut store: CharacterVectorStore,
) -> Result<CharacterVectorStore, DbError> {
    db.write(move |ws| {
        let repo = crate::db::vector_indices::VectorIndicesRepository::new(ws.main().connection());
        store.flush(&repo)?;
        Ok(store)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::vector_indices::VectorEntryInput;
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const SENTINEL: &str = "2020-01-01T00:00:00.000Z";

    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_indices (
            id TEXT PRIMARY KEY, characterId TEXT, version REAL, dimensions REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_entries (
            id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB, createdAt TEXT);
    ";

    struct Seed {
        id: &'static str,
        character_id: &'static str,
        chat_id: Option<&'static str>,
        source_message_id: Option<&'static str>,
        related: &'static [&'static str],
        vector: Option<Vec<f32>>,
    }

    fn make_db(seeds: &[Seed]) -> (TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for s in seeds {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: s.character_id.to_string(),
                            about_character_id: None,
                            chat_id: s.chat_id.map(str::to_string),
                            project_id: None,
                            content: format!("content {}", s.id),
                            summary: format!("summary {}", s.id),
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
                            source_message_id: s.source_message_id.map(str::to_string),
                            last_accessed_at: None,
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: s.related.iter().map(|r| r.to_string()).collect(),
                            reinforced_importance: 0.5,
                        },
                        &CreateOptions {
                            id: s.id.to_string(),
                            created_at: SENTINEL.to_string(),
                            updated_at: SENTINEL.to_string(),
                        },
                    )
                    .unwrap();
                if let Some(vec) = &s.vector {
                    let vi = w.vector_indices();
                    vi.save_meta(s.character_id, vec.len() as f64).unwrap();
                    vi.add_entry(&VectorEntryInput {
                        id: s.id.to_string(),
                        character_id: s.character_id.to_string(),
                        embedding: Some(vec.clone()),
                    })
                    .unwrap();
                }
            }
            // Pin the seed-minted vector timestamps to the sentinel so the tests
            // can tell a flush-time bump from the seeding itself.
            w.connection()
                .execute(
                    "UPDATE vector_indices SET createdAt = ?1, updatedAt = ?1",
                    [SENTINEL],
                )
                .unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn count(db: &Db, table: &str, where_clause: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table} {where_clause}");
        db.read_main(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
            .unwrap()
    }

    /// Ownership gate: a wrong-character or missing target returns false and
    /// writes nothing; the owned path deletes the row + its vector entry.
    #[tokio::test]
    async fn delete_memory_with_vector_checks_ownership() {
        let (_dir, db) = make_db(&[Seed {
            id: "m1",
            character_id: "char-a",
            chat_id: None,
            source_message_id: None,
            related: &[],
            vector: Some(vec![1.0, 0.0]),
        }]);

        assert!(!delete_memory_with_vector(&db, "char-b", "m1")
            .await
            .unwrap());
        assert!(!delete_memory_with_vector(&db, "char-a", "nope")
            .await
            .unwrap());
        assert_eq!(count(&db, "memories", ""), 1);
        assert_eq!(count(&db, "vector_entries", ""), 1);

        assert!(delete_memory_with_vector(&db, "char-a", "m1")
            .await
            .unwrap());
        assert_eq!(count(&db, "memories", ""), 0);
        assert_eq!(count(&db, "vector_entries", ""), 0);
    }

    /// A source-message cascade spans characters: rows deleted through the
    /// chokepoint (surviving neighbour unlinked), vectors counted only where the
    /// store held them, and an untouched store's metadata keeps its sentinel.
    #[tokio::test]
    async fn source_message_cascade_spans_characters() {
        let (_dir, db) = make_db(&[
            Seed {
                id: "m1",
                character_id: "char-a",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "m2",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                // char-b's store never held m2 (no vector) — the sweep must not
                // bump char-b's metadata.
                vector: None,
            },
            Seed {
                id: "m3",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-keep"),
                related: &["m1"],
                vector: Some(vec![0.0, 1.0]),
            },
        ]);

        let r = delete_memories_by_source_message_with_vectors(&db, "msg-1")
            .await
            .unwrap();
        assert_eq!(r.deleted, 2);
        assert_eq!(r.vectors_removed, 1);

        // Survivor m3 got unlinked from the doomed m1.
        let related: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT relatedMemoryIds FROM memories WHERE id = 'm3'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(related, "[]");

        // char-a's store was swept (metadata bumped); char-b's held nothing
        // matching, so its save was a no-op and the sentinel survives.
        let meta_b: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT updatedAt FROM vector_indices WHERE characterId = 'char-b'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(meta_b, SENTINEL);
        let meta_a: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT updatedAt FROM vector_indices WHERE characterId = 'char-a'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_ne!(meta_a, SENTINEL);
    }

    /// The chat wipe reports the character count and the empty branches of all
    /// three cascades return zeroed results without writing.
    #[tokio::test]
    async fn chat_wipe_counts_characters_and_empty_branches_noop() {
        let (_dir, db) = make_db(&[
            Seed {
                id: "m1",
                character_id: "char-a",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-1"),
                related: &[],
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "m2",
                character_id: "char-b",
                chat_id: Some("chat-1"),
                source_message_id: Some("msg-2"),
                related: &[],
                vector: Some(vec![0.0, 1.0]),
            },
        ]);

        let none = delete_memories_by_source_message_with_vectors(&db, "msg-none")
            .await
            .unwrap();
        assert_eq!(none, CascadeDeleteResult::default());
        let none = delete_memories_by_source_messages_with_vectors(&db, &[])
            .await
            .unwrap();
        assert_eq!(none, CascadeDeleteResult::default());

        let r = delete_memories_by_chat_id_with_vectors(&db, "chat-1")
            .await
            .unwrap();
        assert_eq!(r.deleted, 2);
        assert_eq!(r.vectors_removed, 2);
        assert_eq!(r.character_count, 2);
        assert_eq!(count(&db, "memories", ""), 0);
        assert_eq!(count(&db, "vector_entries", ""), 0);
    }
}
