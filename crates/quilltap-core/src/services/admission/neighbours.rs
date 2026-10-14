//! Gather a candidate's neighbours for the decider — v4
//! `lib/memory/admission/neighbours.ts` (`8af764514`; bug 200's orphan widen
//! from `124c8aca3`; bug 237's mixed-dimension fallback from `93c5af6ad`).
//!
//! v4's why, carried: "search the character's index (both tiers), keep live
//! rows, and replace each cold member a digest supersedes with that digest.
//!
//! A vector whose row is gone (an orphan) must not stand at rank 1: with no
//! row behind it a live second match was never considered (bug 200). The
//! search widens past orphans so they cannot crowd live neighbours out of the
//! top K. Orphans are not pruned here; `retireMemories` owns deletion.
//!
//! When some stored vectors cannot be compared with the candidate's (their
//! length differs — a vocabulary refit or a profile switch is mid-reindex, or
//! the cache is stale), the vector search is blind to those rows and every
//! candidate would insert (bug 237). Admission then drops the cache once (the
//! rows may already be right) and, if rows still differ, finds likely
//! neighbours among them by word match and scores those few by embedding
//! their stored text with the candidate's profile."
//!
//! **LR-11 — the stale-cache arm.** v5's index has no cache (C1-3: loaded per
//! call from the rows), so v4's `memoryIndex.reload` is a re-measure over the
//! rows; with nothing between the two reads the second sees what the first
//! saw, and `Admission index was stale; reloaded it from the rows` fires only
//! if a writer heals the rows between them — pinned NEGATIVE for a sequential
//! run.
//!
//! Reads only (the re-embeds call the provider; nothing is written).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::admission::{AdmissionNeighbour, ADMISSION_TOP_K};
use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::embedding_vector::cosine_similarity;
use crate::memory_embedding_text::{embedding_text_for, lexical_embedding_text_for};
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::memory_index::{self, IndexSearchOptions};
use crate::services::memory_time_zone::refresh_memory_time_zone;

use super::{now_ms, text_source_of};

/// v4's child-logger binding (`logger.child({ module:
/// 'memory:admission:neighbours' })`).
const MODULE: &str = "memory:admission:neighbours";

/// Widest search admission will make to see past orphan vectors (v4
/// `ADMISSION_MAX_SEARCH_K = ADMISSION_TOP_K * 4`).
pub const ADMISSION_MAX_SEARCH_K: usize = ADMISSION_TOP_K * 4;
/// Longest supersession chain followed to a live digest.
pub const MAX_DIGEST_HOPS: usize = 3;
/// Words of the candidate's text the unindexed fallback searches for.
const FALLBACK_MAX_WORDS: usize = 6;
/// Shortest word (in UTF-16 code units) the unindexed fallback searches for.
const FALLBACK_MIN_WORD_LENGTH: usize = 4;

/// v4 `GatheredNeighbours`.
#[derive(Clone, Debug, PartialEq)]
pub struct GatheredNeighbours {
    pub neighbours: Vec<AdmissionNeighbour>,
    pub orphan_ids: Vec<String>,
    pub searched_k: usize,
    /// Neighbours scored by re-embedding rows the index could not compare
    /// (bug 237); v4 sets the key only when it is above zero.
    pub reembedded: Option<usize>,
}

/// What the unindexed fallback needs (v4 `NeighbourProbe`): the candidate's
/// text (to find likely neighbours by word) and the user whose embedding
/// profile scores them. The memory-calendar zone the re-embeds render in
/// rides [`gather_neighbours`]'s trailing `host_zone_fallback` (LR-10).
#[derive(Clone, Copy, Debug)]
pub struct NeighbourProbe<'a> {
    pub text: &'a str,
    pub user_id: &'a str,
}

/// A scored hit (v4 `ScoredHit`).
#[derive(Clone, Debug)]
struct ScoredHit {
    id: String,
    score: f64,
}

fn id_of(memory: &Value) -> &str {
    memory.get("id").and_then(Value::as_str).unwrap_or_default()
}

/// Make sure the index can compare the candidate with every row (v4
/// `unscorableRowCount`). Returns how many rows still cannot be compared.
fn unscorable_row_count(
    db: &Db,
    character_id: &str,
    query_dimensions: usize,
) -> Result<usize, DbError> {
    let shape = db.read_main(|c| Ok(memory_index::shape(c, character_id, query_dimensions)))?;
    if shape.size == 0 || shape.matching == shape.size {
        return Ok(0);
    }
    // v4 `memoryIndex.reload(characterId)` then measures again; v5 keeps no
    // cache, so the reload IS the re-measure over the rows (LR-11).
    let shape = db.read_main(|c| Ok(memory_index::shape(c, character_id, query_dimensions)))?;
    let unscorable = shape.size - shape.matching;
    if unscorable > 0 {
        tracing::warn!(
            module = MODULE,
            characterId = %character_id,
            queryDimensions = query_dimensions,
            dominantDimensions = %shape
                .dominant_dimensions
                .map_or("null".to_string(), |d| d.to_string()),
            unscorable = unscorable,
            size = shape.size,
            "Admission index cannot compare some rows with the candidate; matching them by text"
        );
    } else {
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            queryDimensions = query_dimensions,
            "Admission index was stale; reloaded it from the rows"
        );
    }
    Ok(unscorable)
}

/// v4's `/[^\p{L}\p{N}]+/u` split.
fn word_split_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap())
}

/// v4 `fallbackWords(text)`: lower-cased words of at least four UTF-16 code
/// units, deduped first-seen, longest first (a stable sort, so equal lengths
/// keep their order), the first six.
pub(crate) fn fallback_words(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut words: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for w in word_split_re().split(&lower) {
        if w.encode_utf16().count() >= FALLBACK_MIN_WORD_LENGTH && seen.insert(w) {
            words.push(w.to_string());
        }
    }
    // Longer words are the more distinctive ones.
    words.sort_by_key(|w| std::cmp::Reverse(w.encode_utf16().count()));
    words.truncate(FALLBACK_MAX_WORDS);
    words
}

/// Likely neighbours among the rows the index cannot compare, best first (v4
/// `unindexedNeighbourHits`): rows sharing the most words with the candidate,
/// each scored by embedding its stored text with the candidate's profile.
/// Embedding failures cost that row alone.
async fn unindexed_neighbour_hits<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    character_id: &str,
    embedding: &[f32],
    probe: &NeighbourProbe<'_>,
    skip: &HashSet<String>,
    host_zone_fallback: &str,
) -> Result<(Vec<ScoredHit>, Vec<Value>), DbError> {
    let comparable: HashSet<String> = db
        .read_main(|c| Ok(memory_index::entries(c, character_id)))?
        .into_iter()
        .filter(|e| e.embedding.len() == embedding.len())
        .map(|e| e.id)
        .collect();
    // Insertion-ordered `Map<id, {memory, words}>`.
    let mut order: Vec<String> = Vec::new();
    let mut overlap: HashMap<String, (Value, usize)> = HashMap::new();
    for word in fallback_words(probe.text) {
        let found = db.read_main(|c| memories_read::search_by_content(c, character_id, &word))?;
        for memory in found {
            let id = id_of(&memory).to_string();
            if comparable.contains(&id) || skip.contains(&id) {
                continue;
            }
            match overlap.get_mut(&id) {
                // v4 `overlap.set(id, { memory, words: prior + 1 })` — the
                // latest row read, the key keeping its first position.
                Some(prior) => *prior = (memory, prior.1 + 1),
                None => {
                    order.push(id.clone());
                    overlap.insert(id, (memory, 1));
                }
            }
        }
    }
    let candidates = order.len();
    let mut ranked: Vec<(Value, usize)> =
        order.iter().filter_map(|id| overlap.remove(id)).collect();
    ranked.sort_by_key(|(_, words)| std::cmp::Reverse(*words));
    ranked.truncate(ADMISSION_TOP_K);
    if ranked.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let time_zone = refresh_memory_time_zone(db, host_zone_fallback, now_ms());
    let mut hits = Vec::new();
    let mut rows = Vec::new();
    for (memory, _) in ranked {
        let source = text_source_of(&memory);
        let call = EmbeddingCallOptions::background()
            .with_lexical_text(lexical_embedding_text_for(&source));
        let text = embedding_text_for(&source, &time_zone);
        match provider
            .generate_embedding_for_user(&text, probe.user_id, None, call)
            .await
        {
            Ok(result) => {
                let Ok(score) = cosine_similarity(embedding, &result.embedding) else {
                    continue;
                };
                hits.push(ScoredHit {
                    id: id_of(&memory).to_string(),
                    score,
                });
                rows.push(memory);
            }
            Err(e) => {
                tracing::debug!(
                    module = MODULE,
                    characterId = %character_id,
                    memoryId = %id_of(&memory),
                    error = %e.message,
                    "Could not re-embed an unindexed neighbour; skipping it"
                );
            }
        }
    }
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        candidates = candidates,
        scored = hits.len(),
        "Scored unindexed admission neighbours by re-embedding"
    );
    Ok((hits, rows))
}

/// What [`resolve_digest`] found for one row.
struct Resolved {
    memory: Value,
    via: Option<String>,
    dangling_digest: bool,
}

fn is_cold(memory: &Value) -> bool {
    memory.get("tier").and_then(Value::as_str) == Some("cold")
}

/// v4's truthy `memory.supersededById`.
fn superseded_by(memory: &Value) -> Option<String> {
    memory
        .get("supersededById")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Follow a cold member's `supersededById` (up to [`MAX_DIGEST_HOPS`]) to the
/// digest that replaced it (v4 `resolveDigest`). A missing digest leaves the
/// row as it is, marked `dangling_digest`, so the decider treats it as a cold
/// row nothing supersedes. `via` is ALWAYS the original member's id.
fn resolve_digest(db: &Db, memory: Value) -> Result<Resolved, DbError> {
    if !is_cold(&memory) || superseded_by(&memory).is_none() {
        return Ok(Resolved {
            memory,
            via: None,
            dangling_digest: false,
        });
    }
    let origin_id = id_of(&memory).to_string();
    let mut current = memory;
    let mut moved = false;
    for _ in 0..MAX_DIGEST_HOPS {
        let Some(next_id) = superseded_by(&current).filter(|_| is_cold(&current)) else {
            break;
        };
        let ids = vec![next_id.clone()];
        let digest = db
            .read_main(|c| memories_read::find_by_ids(c, &ids))?
            .into_iter()
            .next();
        let Some(digest) = digest else {
            tracing::debug!(
                module = MODULE,
                memoryId = %id_of(&current),
                supersededById = %next_id,
                "Superseding digest missing; the cold row stands for itself"
            );
            return Ok(Resolved {
                memory: current,
                via: moved.then_some(origin_id),
                dangling_digest: true,
            });
        };
        current = digest;
        moved = true;
    }
    Ok(Resolved {
        memory: current,
        via: moved.then_some(origin_id),
        dangling_digest: false,
    })
}

/// One live-row search of the index (both tiers); `exclude` is filtered AFTER
/// the search, as if those rows were already gone (`exhausted` is measured
/// before it).
fn search(
    db: &Db,
    character_id: &str,
    embedding: &[f32],
    exclude: Option<&HashSet<String>>,
    limit: usize,
) -> Result<(Vec<ScoredHit>, bool), DbError> {
    let found = db.read_main(|c| {
        Ok(memory_index::search(
            c,
            character_id,
            embedding,
            IndexSearchOptions {
                hot_only: false,
                exclude: None,
                limit,
            },
        ))
    })?;
    let exhausted = found.len() < limit;
    let hits = found
        .into_iter()
        .filter(|h| exclude.is_none_or(|e| !e.contains(&h.id)))
        .map(|h| ScoredHit {
            id: h.id,
            score: h.score,
        })
        .collect();
    Ok((hits, exhausted))
}

/// The candidate's neighbours, best first, each row once (several members of
/// one cluster collapse onto their digest at the best cosine) — v4
/// `gatherNeighbours(characterId, embedding, { exclude, probe })`.
///
/// The search starts at [`ADMISSION_TOP_K`] and widens past orphan vectors
/// (`k + (5 − live)`, at most [`ADMISSION_MAX_SEARCH_K`]) until five live rows
/// are found or the index is exhausted (bug 200); orphans log WARN `Vector
/// index holds entries with no memory row; ignoring them {module,
/// characterId, orphanIdsJson, searchedK}`. When some rows cannot be
/// compared and a `probe` is given, word-matched rows are re-embedded and
/// merged in (bug 237); `host_zone_fallback` is the memory-calendar fallback
/// those re-embeds render their anchor line in (LR-10). DEBUG `Gathered
/// admission neighbours {module, characterId, neighbours, redirected,
/// orphans, searchedK, reembedded}`.
pub async fn gather_neighbours<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    character_id: &str,
    embedding: &[f32],
    exclude: Option<&HashSet<String>>,
    probe: Option<&NeighbourProbe<'_>>,
    host_zone_fallback: &str,
) -> Result<GatheredNeighbours, DbError> {
    let unscorable = unscorable_row_count(db, character_id, embedding.len())?;
    let mut k = ADMISSION_TOP_K;
    let (mut hits, mut exhausted) = search(db, character_id, embedding, exclude, k)?;
    let mut rows: HashMap<String, Value>;
    loop {
        let ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
        let found = if ids.is_empty() {
            Vec::new()
        } else {
            db.read_main(|c| memories_read::find_by_ids(c, &ids))?
        };
        rows = found
            .into_iter()
            .map(|m| (id_of(&m).to_string(), m))
            .collect();
        let live = hits.iter().filter(|h| rows.contains_key(&h.id)).count();
        if live >= ADMISSION_TOP_K || exhausted || k >= ADMISSION_MAX_SEARCH_K {
            break;
        }
        k = ADMISSION_MAX_SEARCH_K.min(k + (ADMISSION_TOP_K - live));
        (hits, exhausted) = search(db, character_id, embedding, exclude, k)?;
    }

    let orphan_ids: Vec<String> = hits
        .iter()
        .filter(|h| !rows.contains_key(&h.id))
        .map(|h| h.id.clone())
        .collect();
    if !orphan_ids.is_empty() {
        tracing::warn!(
            module = MODULE,
            characterId = %character_id,
            orphanIdsJson = %serde_json::to_string(&orphan_ids).unwrap_or_default(),
            searchedK = k,
            "Vector index holds entries with no memory row; ignoring them"
        );
    }

    let mut scored: Vec<ScoredHit> = hits
        .into_iter()
        .filter(|h| rows.contains_key(&h.id))
        .collect();
    let mut reembedded = 0;
    if let (true, Some(probe)) = (unscorable > 0, probe) {
        let mut skip: HashSet<String> = scored.iter().map(|h| h.id.clone()).collect();
        if let Some(exclude) = exclude {
            skip.extend(exclude.iter().cloned());
        }
        let (extra_hits, extra_rows) = unindexed_neighbour_hits(
            db,
            provider,
            character_id,
            embedding,
            probe,
            &skip,
            host_zone_fallback,
        )
        .await?;
        for memory in extra_rows {
            rows.insert(id_of(&memory).to_string(), memory);
        }
        reembedded = extra_hits.len();
        scored.extend(extra_hits);
        js_array_sort_by(&mut scored, |a, b| b.score - a.score);
    }

    // An insertion-ordered `Map<id, AdmissionNeighbour>`: a later, better hit
    // on the same row replaces the value in place (the key keeps its slot).
    let mut by_id: Vec<AdmissionNeighbour> = Vec::new();
    for hit in scored.iter().take(ADMISSION_TOP_K) {
        let row = rows.get(&hit.id).cloned().unwrap_or(Value::Null);
        let resolved = resolve_digest(db, row)?;
        let rid = id_of(&resolved.memory).to_string();
        let neighbour = AdmissionNeighbour {
            memory: resolved.memory,
            cosine: hit.score,
            via: resolved.via,
            dangling_digest: resolved.dangling_digest,
        };
        match by_id.iter_mut().find(|n| id_of(&n.memory) == rid) {
            Some(prior) if prior.cosine >= hit.score => continue,
            Some(prior) => *prior = neighbour,
            None => by_id.push(neighbour),
        }
    }
    let mut neighbours = by_id;
    js_array_sort_by(&mut neighbours, |a, b| b.cosine - a.cosine);
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        neighbours = neighbours.len(),
        redirected = neighbours
            .iter()
            .filter(|n| n.via.as_deref().is_some_and(|v| !v.is_empty()))
            .count(),
        orphans = orphan_ids.len(),
        searchedK = k,
        reembedded = reembedded,
        "Gathered admission neighbours"
    );
    Ok(GatheredNeighbours {
        neighbours,
        orphan_ids,
        searched_k: k,
        reembedded: (reembedded > 0).then_some(reembedded),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_words_are_long_distinct_lowercase_and_longest_first() {
        assert_eq!(
            fallback_words("The Lighthouse keeper — the keeper! — met Mariana at dawn, 2026."),
            ["lighthouse", "mariana", "keeper", "dawn", "2026"]
        );
        // At most six, ties in first-seen order.
        assert_eq!(
            fallback_words("aaaa bbbb cccc dddd eeee ffff gggg"),
            ["aaaa", "bbbb", "cccc", "dddd", "eeee", "ffff"]
        );
        // Length is UTF-16 code units: one astral letter counts two.
        assert_eq!(fallback_words("𝐀𝐁 abc"), ["𝐀𝐁"]);
    }
}
