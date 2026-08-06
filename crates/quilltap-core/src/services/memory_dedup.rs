//! Memory deduplication (v4 `lib/tools/memory-dedup.ts`) — the synchronous,
//! inline, **no-LLM / no-jobs** dedup the Settings → Memory "Memory
//! Deduplication" card drives (preview = dry run, run = apply).
//!
//! For each of the user's characters:
//!   1. load every memory, split has-embedding vs no-embedding;
//!   2. group the embedded memories BY EMBEDDING LENGTH (never compare
//!      mismatched dims — the JS `Map` insertion order is reproduced so the
//!      grouping walk is identical);
//!   3. within each dim group, pairwise `cosineSimilarity` (plain dot product —
//!      [`cosine_similarity`]) and union at `>= threshold` via a Union-Find with
//!      path compression + union-by-rank;
//!   4. pick the survivor from each multi-member cluster by [`score_memory`]
//!      (importance ×100 + length ×0.1 + specificity), STABLE sort (V8-stable —
//!      ties keep member order);
//!   5. merge each discard's NOVEL details ([`extract_novel_details`] — the
//!      heuristic proper-noun/date/number extractor, deduped across discards by
//!      lowercased key) into the survivor as `[+] ` footnotes;
//!   6. when not a dry run: update each survivor's content, bulk-delete the
//!      discards through the [`delete_many_with_unlink`] chokepoint (so
//!      neighbours' `relatedMemoryIds` are scrubbed), and clean the character's
//!      vector store (best-effort, non-fatal — v4 logs a warn and keeps going).
//!
//! `deduplicate_all_memories` iterates every character, pushing a ZERO-FILLED
//! entry on a per-character failure so the character still appears (v4's
//! try/catch). The result JSON mirrors v4's `DedupResult` name-for-name and key-
//! order-for-key-order; `processedAt` (and each survivor's minted `updatedAt`)
//! are the placeholdered fields in the differential.
//!
//! [`delete_many_with_unlink`]: crate::db::memories::MemoriesRepository::delete_many_with_unlink

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::db::memories::MemUpdate;
use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{characters_read, js_number_to_json, DbError};
use crate::embedding_blob::blob_to_float32;
use crate::embedding_vector::cosine_similarity;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::memory_gate::extract_novel_details;

// ===========================================================================
// Scoring (v4 `scoreMemory`, :113-133)
// ===========================================================================

/// `/\b[A-Z][a-z]{2,}/g` — proper nouns / capitalised words (ASCII `\b`).
static CAPS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?-u:\b)[A-Z][a-z]{2,}").unwrap());
/// `/\b\d+\b/g` — numbers (ASCII `\b`, JS `\d` = `[0-9]`).
static NUMS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?-u:\b)[0-9]+(?-u:\b)").unwrap());
/// `/[A-Za-z]+\.[A-Za-z]+|[a-z]+_[a-z]+|\b(?:API|SQL|JSON|HTTP|AWS|Azure)\b/gi` —
/// technical / code-like tokens. The `i` flag makes `[a-z]` match `[A-Za-z]` and
/// the acronym alternates case-insensitive, exactly as in JS.
static TECH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[A-Za-z]+\.[A-Za-z]+|[a-z]+_[a-z]+|(?-u:\b)(?:API|SQL|JSON|HTTP|AWS|Azure)(?-u:\b)",
    )
    .unwrap()
});

/// Score a memory for survivor selection. Higher = better candidate to keep.
/// `importance` uses v4's `memory.importance || 0.5` fall-back (`0` → `0.5`);
/// `length` is the JS `String.length` (UTF-16 code units).
fn score_memory(content: &str, importance: f64) -> f64 {
    let importance = if importance != 0.0 { importance } else { 0.5 };
    let length = utf16_len(content) as f64;

    let caps = CAPS_RE.find_iter(content).count().min(10) as f64;
    let nums = NUMS_RE.find_iter(content).count().min(5) as f64;
    let tech = TECH_RE.find_iter(content).count().min(5) as f64;
    let specificity = caps * 2.0 + nums * 3.0 + tech * 4.0;

    (importance * 100.0) + (length * 0.1) + specificity
}

// ===========================================================================
// Union-Find (v4, :70-101)
// ===========================================================================

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path compression
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, x: usize, y: usize) {
        let mut rx = self.find(x);
        let mut ry = self.find(y);
        if rx == ry {
            return;
        }
        if self.rank[rx] < self.rank[ry] {
            std::mem::swap(&mut rx, &mut ry);
        }
        self.parent[ry] = rx;
        if self.rank[rx] == self.rank[ry] {
            self.rank[rx] += 1;
        }
    }
}

// ===========================================================================
// Reads
// ===========================================================================

/// One character's memory row, slimmed to the fields dedup needs. `embedding` is
/// `Some` only for a present, non-empty vector (v4's `m.embedding &&
/// m.embedding.length > 0`).
struct MemRow {
    id: String,
    content: String,
    summary: String,
    importance: f64,
    embedding: Option<Vec<f32>>,
}

/// Load a character's memories in v4's `findByFilter({ characterId })` order
/// (natural rowid order — `WHERE characterId = ?`, no `ORDER BY`).
fn read_character_memories(db: &Db, character_id: &str) -> Result<Vec<MemRow>, DbError> {
    let cid = character_id.to_string();
    db.read_main(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content, summary, importance, embedding FROM memories WHERE characterId = ?1",
        )?;
        let rows = stmt
            .query_map([&cid], |r| {
                let blob: Option<Vec<u8>> = r.get(4)?;
                let embedding = blob.map(|b| blob_to_float32(&b)).filter(|v| !v.is_empty());
                Ok(MemRow {
                    id: r.get(0)?,
                    content: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    summary: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    importance: r.get::<_, f64>(3)?,
                    embedding,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

// ===========================================================================
// Per-character dedup (v4 `deduplicateCharacterMemories`, :152)
// ===========================================================================

/// The accumulating totals + the per-character result JSON.
struct CharOutcome {
    json: Value,
    original_count: i64,
    removed_count: i64,
    merged_detail_count: i64,
}

/// A survivor content update queued for the write pass.
struct SurvivorUpdate {
    memory_id: String,
    new_content: String,
}

/// v4's early-return / final CharacterDedupResult shape (identical key order in
/// all three branches).
#[allow(clippy::too_many_arguments)]
fn char_result_json(
    character_id: &str,
    character_name: &str,
    original_count: i64,
    with_embeddings: i64,
    without_embeddings: i64,
    clusters_found: i64,
    memories_in_clusters: i64,
    removed_count: i64,
    merged_detail_count: i64,
    final_count: i64,
    clusters: Vec<Value>,
) -> Value {
    json!({
        "characterId": character_id,
        "characterName": character_name,
        "originalCount": original_count,
        "withEmbeddings": with_embeddings,
        "withoutEmbeddings": without_embeddings,
        "clustersFound": clusters_found,
        "memoriesInClusters": memories_in_clusters,
        "removedCount": removed_count,
        "mergedDetailCount": merged_detail_count,
        "finalCount": final_count,
        "clusters": clusters,
    })
}

async fn deduplicate_character_memories(
    db: &Db,
    character_id: &str,
    character_name: &str,
    threshold: f64,
    dry_run: bool,
) -> Result<CharOutcome, DbError> {
    let memories = read_character_memories(db, character_id)?;
    let original_count = memories.len() as i64;

    let with_emb: Vec<&MemRow> = memories.iter().filter(|m| m.embedding.is_some()).collect();
    let with_embeddings = with_emb.len() as i64;
    let without_embeddings = original_count - with_embeddings;

    // v4: `originalCount < 2` and `withEmbeddings.length < 2` early returns —
    // both keep the full character in the result with zeroed cluster fields.
    if original_count < 2 || with_embeddings < 2 {
        return Ok(CharOutcome {
            json: char_result_json(
                character_id,
                character_name,
                original_count,
                with_embeddings,
                without_embeddings,
                0,
                0,
                0,
                0,
                original_count,
                Vec::new(),
            ),
            original_count,
            removed_count: 0,
            merged_detail_count: 0,
        });
    }

    // Group by embedding dimension length (JS `Map` insertion order).
    let mut dim_order: Vec<usize> = Vec::new();
    let mut dim_groups: HashMap<usize, Vec<&MemRow>> = HashMap::new();
    for m in &with_emb {
        let dim = m.embedding.as_ref().unwrap().len();
        dim_groups
            .entry(dim)
            .or_insert_with(|| {
                dim_order.push(dim);
                Vec::new()
            })
            .push(m);
    }

    let mut all_clusters: Vec<Value> = Vec::new();
    let mut survivor_updates: Vec<SurvivorUpdate> = Vec::new();
    let mut all_remove_ids: Vec<String> = Vec::new();
    let mut total_merged_details: i64 = 0;

    for dim in &dim_order {
        let group = &dim_groups[dim];
        let n = group.len();
        if n < 2 {
            continue;
        }

        // Pairwise cosine + Union-Find clustering.
        let mut uf = UnionFind::new(n);
        for i in 0..n {
            for j in (i + 1)..n {
                // Same dim group → the lengths always match, so this is `Ok`.
                if let Ok(sim) = cosine_similarity(
                    group[i].embedding.as_ref().unwrap(),
                    group[j].embedding.as_ref().unwrap(),
                ) {
                    if sim >= threshold {
                        uf.union(i, j);
                    }
                }
            }
        }

        // Collect clusters (first-seen-root insertion order = JS `Map`).
        let mut cluster_order: Vec<usize> = Vec::new();
        let mut cluster_map: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..n {
            let root = uf.find(i);
            cluster_map
                .entry(root)
                .or_insert_with(|| {
                    cluster_order.push(root);
                    Vec::new()
                })
                .push(i);
        }

        for root in &cluster_order {
            let member_indices = &cluster_map[root];
            if member_indices.len() < 2 {
                continue;
            }

            // Score each member; STABLE sort by score descending (V8-stable →
            // ties preserve member order).
            let mut scored: Vec<(usize, f64)> = member_indices
                .iter()
                .map(|&idx| {
                    (
                        idx,
                        score_memory(&group[idx].content, group[idx].importance),
                    )
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let survivor_idx = scored[0].0;
            let discard_indices: Vec<usize> = scored[1..].iter().map(|(idx, _)| *idx).collect();

            // Extract novel details from discards, deduped across the cluster by
            // lowercased+trimmed key (v4 `detail.toLowerCase().trim()`).
            let mut all_novel_details: Vec<String> = Vec::new();
            let mut seen_details: HashSet<String> = HashSet::new();
            for &d in &discard_indices {
                let novel = extract_novel_details(&group[d].content, &group[survivor_idx].content);
                for detail in novel {
                    let key = js_trim(&detail.to_lowercase()).to_string();
                    if !seen_details.contains(&key) {
                        seen_details.insert(key);
                        all_novel_details.push(detail);
                    }
                }
            }

            total_merged_details += all_novel_details.len() as i64;

            // Build the survivor's updated content with `[+] ` footnotes.
            if !all_novel_details.is_empty() {
                let footnotes = all_novel_details
                    .iter()
                    .map(|d| format!("[+] {d}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let new_content = format!("{}\n{}", group[survivor_idx].content, footnotes);
                survivor_updates.push(SurvivorUpdate {
                    memory_id: group[survivor_idx].id.clone(),
                    new_content,
                });
            }

            for &d in &discard_indices {
                all_remove_ids.push(group[d].id.clone());
            }

            let removed_summaries: Vec<Value> = discard_indices
                .iter()
                .take(3)
                .map(|&d| json!(utf16_truncate(&group[d].summary, 100)))
                .collect();

            all_clusters.push(json!({
                "size": member_indices.len(),
                "survivorId": group[survivor_idx].id,
                "survivorSummary": utf16_truncate(&group[survivor_idx].summary, 120),
                "survivorImportance": js_number_to_json(group[survivor_idx].importance),
                "removedCount": discard_indices.len(),
                "removedSummaries": removed_summaries,
                "mergedDetailCount": all_novel_details.len(),
            }));
        }
    }

    let removed_count = all_remove_ids.len() as i64;
    let final_count = original_count - removed_count;

    // Apply changes if not a dry run.
    if !dry_run && (!survivor_updates.is_empty() || !all_remove_ids.is_empty()) {
        let updates_for_write: Vec<(String, String)> = survivor_updates
            .iter()
            .map(|u| (u.memory_id.clone(), u.new_content.clone()))
            .collect();
        let removes = all_remove_ids.clone();
        let cid = character_id.to_string();
        db.write(move |writers| {
            let repo = writers.main().memories();
            for (memory_id, new_content) in &updates_for_write {
                let patch = MemUpdate {
                    content: Some(new_content.clone()),
                    ..Default::default()
                };
                repo.update_for_character(&cid, memory_id, &patch)?;
            }
            if !removes.is_empty() {
                repo.delete_many_with_unlink(&removes)?;
            }
            Ok(())
        })
        .await?;

        // Vector-store cleanup — best-effort (v4 logs a warn and keeps the
        // result). A store load failure skips it, like v4's catch.
        if !all_remove_ids.is_empty() {
            let cid = character_id.to_string();
            let removes = all_remove_ids.clone();
            let _ = db
                .write(move |writers| {
                    let main = writers.main();
                    let mut store = CharacterVectorStore::load(main.connection(), &cid)?;
                    for id in &removes {
                        store.remove_vector(id);
                    }
                    store.flush(&main.vector_indices())?;
                    Ok(())
                })
                .await;
        }
    }

    let clusters_found = all_clusters.len() as i64;
    let memories_in_clusters: i64 = all_clusters
        .iter()
        .map(|c| c["size"].as_i64().unwrap_or(0))
        .sum();

    Ok(CharOutcome {
        json: char_result_json(
            character_id,
            character_name,
            original_count,
            with_embeddings,
            without_embeddings,
            clusters_found,
            memories_in_clusters,
            removed_count,
            total_merged_details,
            final_count,
            all_clusters,
        ),
        original_count,
        removed_count,
        merged_detail_count: total_merged_details,
    })
}

// ===========================================================================
// All-characters dedup (v4 `deduplicateAllMemories`, :366)
// ===========================================================================

/// Deduplicate memories across all of a user's characters. Returns v4's
/// `DedupResult` JSON (the SPA reads a subset; the differential diffs the whole
/// shape + the post-state of `memories`). `dry_run = true` is the preview.
pub async fn deduplicate_all_memories(
    db: &Db,
    _user_id: &str,
    threshold: f64,
    dry_run: bool,
) -> Result<Value, DbError> {
    // Every character, in v4's `userRepos.characters.findAll()` order (the
    // single-user v5 has one user; the overlay read reproduces the order).
    let characters: Vec<(String, String)> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let chars = characters_read::find_all(main, mount)?;
            Ok(chars
                .iter()
                .map(|c| {
                    (
                        c.get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        c.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
                .collect())
        })
    })?;

    let mut character_results: Vec<Value> = Vec::new();
    let mut total_original: i64 = 0;
    let mut total_removed: i64 = 0;
    let mut total_merged_details: i64 = 0;

    for (id, name) in &characters {
        match deduplicate_character_memories(db, id, name, threshold, dry_run).await {
            Ok(outcome) => {
                total_original += outcome.original_count;
                total_removed += outcome.removed_count;
                total_merged_details += outcome.merged_detail_count;
                character_results.push(outcome.json);
            }
            Err(e) => {
                // v4: log + push a ZERO-FILLED entry so the character still
                // appears in the results.
                tracing::error!(
                    character_id = %id,
                    error = %e,
                    "[MemoryDedup] Failed to deduplicate character",
                );
                character_results.push(char_result_json(
                    id,
                    name,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    Vec::new(),
                ));
            }
        }
    }

    let total_final = total_original - total_removed;

    Ok(json!({
        "threshold": js_number_to_json(threshold),
        "dryRun": dry_run,
        "characters": character_results,
        "totalOriginal": total_original,
        "totalRemoved": total_removed,
        "totalMergedDetails": total_merged_details,
        "totalFinal": total_final,
        "processedAt": crate::clock::now_iso(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // v4 `scoreMemory` — the specificity heuristics, checked against hand-computed
    // JS values so the regex ports are pinned independent of the differential.
    #[test]
    fn score_counts_caps_numbers_and_tech_tokens() {
        // "Bram loves the API and SQL databases." — importance 0.9.
        //   importance 0.9*100 = 90
        //   length 37 * 0.1 = 3.7
        //   caps: "Bram" = 1 -> *2 = 2
        //   nums: 0
        //   tech: "API","SQL" = 2 -> *4 = 8
        let s = score_memory("Bram loves the API and SQL databases.", 0.9);
        assert!((s - 103.7).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn score_zero_importance_falls_back_to_half() {
        // "" -> importance 0 falls back to 0.5; length 0; no specificity.
        let s = score_memory("", 0.0);
        assert!((s - 50.0).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn score_caps_and_numbers_are_capped() {
        // 3 numbers with units? No — bare `\b\d+\b` counts each integer once.
        // "1 2 3 4 5 6" -> 6 numbers capped at 5 -> *3 = 15; length 11 -> 1.1;
        // importance 0.5 -> 50. No caps/tech.
        let s = score_memory("1 2 3 4 5 6", 0.5);
        assert!((s - (50.0 + 1.1 + 15.0)).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn union_find_clusters_transitively() {
        let mut uf = UnionFind::new(4);
        uf.union(0, 1);
        uf.union(1, 2);
        assert_eq!(uf.find(0), uf.find(2));
        assert_ne!(uf.find(0), uf.find(3));
    }
}
