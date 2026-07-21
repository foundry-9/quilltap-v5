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
//! and slice). It is the read half the first-message-context builder composes. The
//! full v4 surface's two opt-in extensions — the `recallContext` targeting-tag
//! re-ranking + one-hop related-memory expansion — are a **tracked deferral**: no
//! consumer in this wave (the knowledge injector doesn't use it; first-message
//! context leaves both off), and the expansion needs `RELATED_EXPANSION` /
//! `expandRelatedMemories`, which `recall_tags` deliberately does not yet carry.
//! Everything else is faithful: the one-retry-free embed, the dimension-mismatch →
//! text fallback, the optional literal-phrase boost, the min-score / min-importance /
//! source / aboutCharacterId filters, and the `bumpAccessTimes` side-effect.

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
    /// The `Date.now()` seam used by `calculateEffectiveWeight` (epoch millis). The
    /// blend's time-decay reads it; injected so the differential is deterministic.
    pub now_ms: f64,
}

/// v4 `searchMemoriesSemantic`. Tries the vector path first (embed → search →
/// hydrate → blend → sort → slice), falling back to text search on any failure or a
/// dimension mismatch. The `recallContext` re-ranking + related-expansion path is a
/// tracked deferral (see the module doc).
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
            let ids: Vec<String> = results.iter().filter_map(|r| id_of(&r.memory)).collect();
            bump_access_times(db, character_id, &ids).await;
            results.truncate(limit);
            Ok(results)
        }
        // `Ok(None)` = the vector pool was empty (v4 falls through past the
        // `if (augmentedVectorResults.length > 0)` block to the text fallback).
        Ok(None) | Err(_) => {
            let mut text = search_memories_text(db, character_id, query, options).await?;
            let ids: Vec<String> = text.iter().filter_map(|r| id_of(&r.memory)).collect();
            bump_access_times(db, character_id, &ids).await;
            text.truncate(limit);
            Ok(text)
        }
    }
}

/// The vector path. `Ok(Some(sorted))` on a non-empty pool, `Ok(None)` when the pool
/// is empty (→ text fallback), `Err` on embed failure / dimension mismatch (→ text
/// fallback). The `Err` carries no diagnostic — the caller only branches on it.
#[allow(clippy::too_many_arguments)]
async fn try_semantic<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    query: &str,
    options: &SemanticSearchOptions,
    limit: usize,
    explicit_min_score: Option<f64>,
) -> Result<Option<Vec<SemanticSearchResult>>, ()> {
    let embed = provider
        .generate_embedding_for_user(
            query,
            &options.user_id,
            options.embedding_profile_id.as_deref(),
            EmbeddingPriority::Interactive,
        )
        .await
        .map_err(|_| ())?;

    let min_score = explicit_min_score
        .unwrap_or_else(|| default_min_cosine_for_provider(Some(&embed.provider)));

    // Load the store + dimension check off the read pool.
    let query_embedding = embed.embedding.clone();
    let (stored_dimensions, vector_results) = {
        let query_embedding = query_embedding.clone();
        let cid = character_id.to_string();
        db.read_main(move |conn| {
            let store = CharacterVectorStore::load(conn, &cid)?;
            // The store's known dimension: the length of any stored entry (all
            // entries share it; v4 resolves it from the index metadata / first
            // entry the same way). `None` on an empty store → no mismatch guard.
            let dims = store.all_entries().next().map(|(_, v)| v.len());
            // v4 searches limit*3 candidates.
            let results = store.search(&query_embedding, limit.saturating_mul(3));
            Ok((dims, results))
        })
        .map_err(|_| ())?
    };

    // Dimension mismatch → text fallback (v4 returns searchMemoriesText immediately).
    if let Some(dims) = stored_dimensions {
        if query_embedding.len() != dims {
            return Err(());
        }
    }

    // Hybrid literal-boost union (off for our consumers, but ported faithfully).
    let literal_phrase = if options.apply_literal_phrase_boost {
        get_literal_phrase(Some(query))
    } else {
        None
    };
    let mut literal_hit_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // (id, score) candidate pool; the vector store's search order is preserved.
    let mut augmented: Vec<(String, f64)> = vector_results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect();

    if let Some(phrase) = &literal_phrase {
        let q = query.trim().to_string();
        let cid = character_id.to_string();
        let direct = db
            .read_main(move |conn| memories_read::search_by_content(conn, &cid, &q))
            .map_err(|_| ())?;
        for m in &direct {
            if let Some(id) = id_of(m) {
                literal_hit_ids.insert(id);
            }
        }
        let in_pool: std::collections::HashSet<String> =
            vector_results.iter().map(|r| r.id.clone()).collect();
        for m in &direct {
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
            let _ = phrase; // phrase only gates the union; boost is applied below
        }
    }

    if augmented.is_empty() {
        return Ok(None);
    }

    // Hydrate the matched ids (only the top-K).
    let matched_ids: Vec<String> = augmented.iter().map(|(id, _)| id.clone()).collect();
    let memories = db
        .read_main(move |conn| memories_read::find_by_ids(conn, &matched_ids))
        .map_err(|_| ())?;
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

    // Sort by the blended ranking key (see compute_ranking_blend). Stable —
    // preserves the pool order among ties (JS Array.sort is stable).
    results.sort_by(|a, b| {
        let sa = compute_ranking_blend(a.score, a.raw_weight);
        let sb = compute_ranking_blend(b.score, b.raw_weight);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Some(results))
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
