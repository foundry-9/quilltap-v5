//! Memory deduplication (v4 `lib/tools/memory-dedup.ts` at `01a83539d`) — the synchronous,
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
//!   5. preview each cluster's fold through the pure
//!      [`plan_memory_merge`](crate::memory_merge::plan_memory_merge) (v4
//!      `3f7320138` F9 — capped `[+]` footnotes judged against the growing
//!      content, summed counts, unioned links, earliest `occurredAt`);
//!   6. when not a dry run AND something is to be removed: FOLD each cluster
//!      into its survivor first (`apply_memory_merge` — the patch, then ONE
//!      re-embed through the optional embedding seam when the content or the
//!      anchor date changed); a failed fold KEEPS its losers; then bulk-delete
//!      the rest through the ONE retire path ([`retire_and_announce`] — so
//!      neighbours' `relatedMemoryIds` are scrubbed and the vectors go with
//!      the rows), and invalidate the character's frozen archive.
//!
//! `deduplicate_all_memories` iterates every character, pushing a ZERO-FILLED
//! entry on a per-character failure so the character still appears (v4's
//! try/catch). The result JSON mirrors v4's `DedupResult` name-for-name and key-
//! order-for-key-order; `processedAt` (and each survivor's minted `updatedAt`)
//! are the placeholdered fields in the differential.
//!
//! [`retire_and_announce`]: crate::services::memory_retire::retire_and_announce

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, js_number_to_json, memories_read, DbError};
use crate::embedding_vector::cosine_similarity;
use crate::jsstr::{utf16_len, utf16_truncate};
use crate::memory_merge::plan_memory_merge;
use crate::model::embedding::EmbeddingProvider;
use crate::services::memory_gate::apply_memory_merge;

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

/// One character's memory row: the fields dedup scores on, plus the whole row
/// (the merge plan's survivor / losers). `embedding` is `Some` only for a
/// present, non-empty vector (v4's `m.embedding && m.embedding.length > 0`).
struct MemRow {
    id: String,
    content: String,
    summary: String,
    importance: f64,
    embedding: Option<Vec<f32>>,
    row: Value,
}

/// The row JSON's `embedding` — v4's `Float32Array` object shape `{"0":v0,…}`
/// ([`memories_read`]'s marshal) — back to its f32s. f32 → f64 → f32 is exact,
/// so this is the raw BLOB decode the read used to do.
fn embedding_of(row: &Value) -> Option<Vec<f32>> {
    let obj = row.get("embedding")?.as_object()?;
    let mut v = Vec::with_capacity(obj.len());
    for i in 0..obj.len() {
        v.push(obj.get(&i.to_string())?.as_f64()? as f32);
    }
    (!v.is_empty()).then_some(v)
}

/// Load a character's memories through v4's `findByCharacterId` (=
/// `findByFilter({ characterId })`: natural rowid order, Zod-faithful drops,
/// ALL tiers — R-A: dedup can fold a hot loser into a COLD survivor and can
/// delete a CONSOLIDATED digest as a loser, faithful to v4 at `01a83539d`).
fn read_character_memories(db: &Db, character_id: &str) -> Result<Vec<MemRow>, DbError> {
    let cid = character_id.to_string();
    let rows = db.read_main(move |conn| memories_read::find_by_character_id(conn, &cid))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let s = |k: &str| {
                row.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            MemRow {
                id: s("id"),
                content: s("content"),
                summary: s("summary"),
                importance: row.get("importance").and_then(Value::as_f64).unwrap_or(0.0),
                embedding: embedding_of(&row),
                row,
            }
        })
        .collect())
}

// ===========================================================================
// Per-character dedup (v4 `deduplicateCharacterMemories`, :157)
// ===========================================================================

/// The accumulating totals + the per-character result JSON.
struct CharOutcome {
    json: Value,
    original_count: i64,
    removed_count: i64,
    merged_detail_count: i64,
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

const CHAR_CONTEXT: &str = "memory-dedup.deduplicateCharacterMemories";
const ALL_CONTEXT: &str = "memory-dedup.deduplicateAllMemories";

async fn deduplicate_character_memories<P: EmbeddingProvider>(
    db: &Db,
    provider: Option<&P>,
    character_id: &str,
    character_name: &str,
    threshold: f64,
    dry_run: bool,
    user_id: Option<&str>,
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
    // (survivor, losers) per multi-member cluster, in cluster order.
    let mut all_merges: Vec<(&MemRow, Vec<&MemRow>)> = Vec::new();
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

            let survivor = group[scored[0].0];
            let discards: Vec<&MemRow> = scored[1..].iter().map(|(idx, _)| group[*idx]).collect();

            // Preview the fold: which novel details the discards would add to
            // the survivor — CAPPED, judged against the growing content. (v4's
            // comment says the real plan is recomputed against the full removal
            // set; both calls pass no `excludeIds` — `memory-merge.ts:18-25`
            // describes the code.)
            let loser_rows: Vec<Value> = discards.iter().map(|d| d.row.clone()).collect();
            let preview = plan_memory_merge(&survivor.row, &loser_rows, &[]);
            let merged_detail_count = preview.merged_details.len();
            total_merged_details += merged_detail_count as i64;
            all_merges.push((survivor, discards.clone()));

            for d in &discards {
                all_remove_ids.push(d.id.clone());
            }

            let removed_summaries: Vec<Value> = discards
                .iter()
                .take(3)
                .map(|d| json!(utf16_truncate(&d.summary, 100)))
                .collect();

            all_clusters.push(json!({
                "size": member_indices.len(),
                "survivorId": survivor.id,
                "survivorSummary": utf16_truncate(&survivor.summary, 120),
                "survivorImportance": js_number_to_json(survivor.importance),
                "removedCount": discards.len(),
                "removedSummaries": removed_summaries,
                "mergedDetailCount": merged_detail_count,
            }));
        }
    }

    let mut remove_ids = all_remove_ids.clone();

    // Apply changes if not a dry run — and only when there is something to
    // remove (v4 `:292`; the baseline also ran on survivor updates alone).
    if !dry_run && !all_remove_ids.is_empty() {
        // Fold each cluster's discards into its survivor FIRST: details,
        // reinforcement, links, earliest occurredAt; re-embedded when the
        // content changed. A failed fold keeps its losers rather than deleting
        // what the survivor never absorbed.
        //
        // R-E (NO-PORT, measured): v4 passes each patched survivor's new link
        // list to the delete as `currentLinks`, because in its job child the
        // patch is still buffered when the scrub reads the DB. Dedup runs in
        // v4's PARENT and every v5 path is in-process: by the time the delete
        // runs, the survivor's list is committed through the single writer, so
        // the scrub's DB read IS that list.
        let mut kept: HashSet<String> = HashSet::new();
        for (survivor, losers) in &all_merges {
            let loser_rows: Vec<Value> = losers.iter().map(|l| l.row.clone()).collect();
            let plan = plan_memory_merge(&survivor.row, &loser_rows, &[]);
            let applied =
                match apply_memory_merge(db, provider, &survivor.row, &plan, user_id, None).await {
                    Ok(applied) => applied,
                    Err(e) => {
                        tracing::warn!(
                            context = CHAR_CONTEXT,
                            characterId = character_id,
                            survivorId = survivor.id,
                            error = %e,
                            "[MemoryDedup] Failed to fold discards into survivor"
                        );
                        None
                    }
                };
            if applied.is_none() {
                for loser in losers {
                    kept.insert(loser.id.clone());
                }
            }
        }
        if !kept.is_empty() {
            remove_ids = all_remove_ids
                .iter()
                .filter(|id| !kept.contains(*id))
                .cloned()
                .collect();
            tracing::warn!(
                context = CHAR_CONTEXT,
                characterId = character_id,
                kept = kept.len(),
                "[MemoryDedup] Some folds failed; their discards were kept"
            );
        }

        // P4.D288 (C1-7): the ONE retire path (`near-duplicate-fold`, v4
        // `fold-near-duplicates.ts:195`) — every link to the folded rows (the
        // survivors' freshly committed lists included) scrubbed, their vectors
        // with them; the announcement after the commit (R-AF).
        let deleted = crate::services::memory_retire::retire_and_announce(
            db,
            remove_ids.clone(),
            crate::services::memory_retire::RetireOptions::new(
                crate::services::memory_retire::RetireReason::NearDuplicateFold,
            ),
        )
        .await?
        .retired;
        tracing::info!(
            context = CHAR_CONTEXT,
            characterId = character_id,
            requested = remove_ids.len(),
            deleted = deleted,
            "[MemoryDedup] Bulk deleted memories"
        );

        crate::services::frozen_archive::invalidate_frozen_archive(character_id);
    }

    let removed_count = remove_ids.len() as i64;
    let final_count = original_count - removed_count;
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
// All-characters dedup (v4 `deduplicateAllMemories`, :388)
// ===========================================================================

/// Deduplicate memories across all of a user's characters. Returns v4's
/// `DedupResult` JSON (the SPA reads a subset; the differential diffs the whole
/// shape + the post-state of `memories`). `dry_run = true` is the preview.
///
/// `provider` is the optional memory-embedding seam the changed survivors are
/// re-embedded through (R-D — `None` makes each re-embed fail non-fatally).
pub async fn deduplicate_all_memories<P: EmbeddingProvider>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    threshold: f64,
    dry_run: bool,
) -> Result<Value, DbError> {
    tracing::info!(
        context = ALL_CONTEXT,
        userId = user_id,
        threshold = threshold,
        dryRun = dry_run,
        "[MemoryDedup] Starting deduplication for all characters"
    );

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
        match deduplicate_character_memories(
            db,
            provider,
            id,
            name,
            threshold,
            dry_run,
            Some(user_id),
        )
        .await
        {
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
                    context = ALL_CONTEXT,
                    characterId = %id,
                    characterName = %name,
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

    tracing::info!(
        context = ALL_CONTEXT,
        userId = user_id,
        characterCount = characters.len(),
        totalOriginal = total_original,
        totalRemoved = total_removed,
        totalMergedDetails = total_merged_details,
        totalFinal = total_final,
        dryRun = dry_run,
        "[MemoryDedup] Deduplication complete"
    );

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

    // -----------------------------------------------------------------------
    // P4.D266 U6 — the fold (v4 `memory-recall-housekeeping-fixes.test.ts`
    // F9 `:337-402`), the failed fold, the frozen-archive pin, and the logs.
    // -----------------------------------------------------------------------

    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::Writer;
    use crate::memory_merge::plan_memory_merge;
    use crate::model::embedding::{EmbeddingCallOptions, EmbeddingError, EmbeddingResult};
    use tempfile::{tempdir, TempDir};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const T: &str = "2026-01-01T00:00:00.000Z";
    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]', consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_indices (
            id TEXT PRIMARY KEY, characterId TEXT, version REAL, dimensions REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_entries (
            id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB, createdAt TEXT);
    ";

    /// Any text → one fixed vector, with every call recorded.
    #[derive(Clone, Default)]
    struct AnyText {
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl EmbeddingProvider for AnyText {
        fn generate_embedding_for_user(
            &self,
            text: &str,
            _user_id: &str,
            _profile_id: Option<&str>,
            _opts: EmbeddingCallOptions,
        ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send
        {
            self.calls.lock().unwrap().push(text.to_string());
            async {
                Ok(EmbeddingResult {
                    embedding: vec![0.0, 1.0],
                    model: "canned".into(),
                    dimensions: 2,
                    provider: "canned".into(),
                })
            }
        }
    }

    struct M {
        id: &'static str,
        content: &'static str,
        importance: f64,
        count: f64,
        links: Vec<&'static str>,
        occurred_at: Option<&'static str>,
    }

    fn m(id: &'static str, content: &'static str, importance: f64) -> M {
        M {
            id,
            content,
            importance,
            count: 1.0,
            links: vec![],
            occurred_at: None,
        }
    }

    /// Every memory on [1, 0] — one cluster at any threshold.
    fn make_db(character: &str, rows: &[M]) -> (TempDir, Db) {
        crate::test_support::global_capture::install();
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for r in rows {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: character.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: r.content.to_string(),
                            summary: format!("summary {}", r.id),
                            keywords: vec![],
                            tags: vec![],
                            importance: r.importance,
                            embedding: Some(vec![1.0, 0.0]),
                            source: "AUTO".into(),
                            witnessed_context: None,
                            occurred_at: r.occurred_at.map(str::to_string),
                            narrative_time: None,
                            entities: vec![],
                            kind: "semantic".into(),
                            source_message_id: None,
                            last_accessed_at: None,
                            reinforcement_count: r.count,
                            last_reinforced_at: None,
                            related_memory_ids: r.links.iter().map(|s| s.to_string()).collect(),
                            reinforced_importance: r.importance,
                            tier: "hot".into(),
                            superseded_by_id: None,
                            superseded_at: None,
                            consolidated_from: vec![],
                            consolidated_at: None,
                        },
                        &CreateOptions {
                            id: r.id.to_string(),
                            created_at: T.into(),
                            updated_at: T.into(),
                        },
                    )
                    .unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn row(db: &Db, id: &str) -> Option<Value> {
        let id = id.to_string();
        db.read_main(move |c| memories_read::find_by_id(c, &id))
            .unwrap()
    }

    const KEEP: &str = "a0000000-0000-4000-8000-000000000001";
    const LOSER: &str = "a0000000-0000-4000-8000-000000000002";
    const N1: &str = "a0000000-0000-4000-8000-000000000003";
    const N2: &str = "a0000000-0000-4000-8000-000000000004";

    /// v4 `:338-365` + `:381-392`: the fold carries the loser's novel detail
    /// (capped footnote), the summed count with a recomputed importance, the
    /// link union minus the group, and the EARLIEST occurredAt into the
    /// survivor; the content changed, so it re-embeds through EXACTLY ONE
    /// provider call; the loser is deleted.
    #[tokio::test]
    async fn the_fold_folds_details_counts_links_and_the_earliest_occurred_at() {
        const CHAR: &str = "c0000000-0000-4000-8000-0000000000d1";
        let mut keep = m(KEEP, "Friday keeps the ledger for the long voyages.", 0.6);
        keep.count = 2.0;
        keep.links = vec![N1, LOSER];
        keep.occurred_at = Some("2026-03-10T00:00:00.000Z");
        let mut loser = m(LOSER, "Friday keeps the ledger for Marguerite.", 0.3);
        loser.count = 3.0;
        loser.links = vec![KEEP, N2];
        loser.occurred_at = Some("2026-03-08T00:00:00.000Z");
        let (_d, db) = make_db(CHAR, &[keep, loser]);
        let provider = AnyText::default();
        let (outcome, lines) =
            crate::test_support::global_capture::capture_async(deduplicate_character_memories(
                &db,
                Some(&provider),
                CHAR,
                "Friday",
                0.8,
                false,
                Some("user-1"),
            ))
            .await;
        let outcome = outcome.unwrap();
        assert_eq!((outcome.removed_count, outcome.merged_detail_count), (1, 1));
        let survivor = row(&db, KEEP).unwrap();
        assert_eq!(
            survivor["content"],
            "Friday keeps the ledger for the long voyages.\n[+] Marguerite"
        );
        assert_eq!(survivor["reinforcementCount"], 5);
        assert_eq!(
            survivor["reinforcedImportance"].as_f64().unwrap(),
            crate::memory_gate::calculate_reinforced_importance(0.6, 5.0)
        );
        // `N2` survives the scrub (it is not deleted); `LOSER` is gone.
        assert_eq!(survivor["relatedMemoryIds"], json!([N1, N2]));
        assert_eq!(survivor["occurredAt"], "2026-03-08T00:00:00.000Z");
        assert!(row(&db, LOSER).is_none());
        assert_eq!(provider.calls.lock().unwrap().len(), 1, "one re-embed call");
        let folded = lines
            .iter()
            .find(|l| l.contains("[MemoryMerge] Folded losers into survivor"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(folded.starts_with("DEBUG "), "{folded:?}");
        assert!(
            folded.contains(&format!(
                " survivorId={KEEP} fieldsJson=[\"content\",\"reinforcementCount\",\
                 \"reinforcedImportance\",\"relatedMemoryIds\",\"occurredAt\"] \
                 mergedDetails=1 reembedded=true"
            )),
            "{folded:?}"
        );
        let deleted = lines
            .iter()
            .find(|l| l.contains("[MemoryDedup] Bulk deleted memories"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(deleted.starts_with("INFO "), "{deleted:?}");
        assert!(
            deleted.contains(&format!(
                " context=memory-dedup.deduplicateCharacterMemories characterId={CHAR} \
                 requested=1 deleted=1"
            )),
            "{deleted:?}"
        );
        for silent in [
            "Some folds failed",
            "Failed to fold",
            "Failed to clean up vector store",
        ] {
            assert!(
                !lines.iter().any(|l| l.contains(silent)),
                "{silent}: {lines:#?}"
            );
        }
    }

    /// v4 `:367-379` + `:394-402`: at the footnote cap the fold appends
    /// nothing, and a fold that changes only the counts does NOT re-embed.
    #[tokio::test]
    async fn at_the_cap_only_counts_move_and_nothing_re_embeds() {
        const CHAR: &str = "c0000000-0000-4000-8000-0000000000d2";
        let keep = m(
            KEEP,
            "Body of the ship log.\n[+] D0\n[+] D1\n[+] D2\n[+] D3\n[+] D4\n[+] D5\n[+] D6\n[+] D7",
            0.9,
        );
        let loser = m(LOSER, "body with Marguerite and Valparaiso.", 0.2);
        let (_d, db) = make_db(CHAR, &[keep, loser]);
        let provider = AnyText::default();
        let outcome = deduplicate_character_memories(
            &db,
            Some(&provider),
            CHAR,
            "Friday",
            0.8,
            false,
            Some("user-1"),
        )
        .await
        .unwrap();
        assert_eq!(outcome.merged_detail_count, 0);
        let survivor = row(&db, KEEP).unwrap();
        assert!(survivor["content"].as_str().unwrap().ends_with("[+] D7"));
        assert_eq!(survivor["reinforcementCount"], 2);
        assert!(
            provider.calls.lock().unwrap().is_empty(),
            "counts only → no re-embed"
        );
    }

    /// A FAILED fold keeps its losers: a planted `BEFORE UPDATE` abort on the
    /// survivor makes the patch throw → WARN `[MemoryDedup] Failed to fold
    /// discards into survivor` + WARN `Some folds failed; their discards were
    /// kept`, nothing deleted (the INFO still fires with `requested=0`), and the
    /// character's frozen archive is invalidated anyway (v4 `:360`).
    #[tokio::test]
    async fn a_failed_fold_keeps_its_losers_and_still_invalidates() {
        const CHAR: &str = "c0000000-0000-4000-8000-0000000000d3";
        let (_d, db) = make_db(
            CHAR,
            &[
                m(KEEP, "the survivor row with the longest words.", 0.9),
                m(LOSER, "the loser row.", 0.2),
            ],
        );
        db.write(|w| {
            w.main().connection().execute_batch(&format!(
                "CREATE TRIGGER planted BEFORE UPDATE ON memories WHEN OLD.id = '{KEEP}' \
                 BEGIN SELECT RAISE(ABORT, 'planted fold failure'); END"
            ))?;
            Ok(())
        })
        .await
        .unwrap();
        // R-I: seed the cache, then plant a row behind its back.
        let archive = |db: &Db| -> Vec<String> {
            crate::services::frozen_archive::get_or_compute_frozen_archive(
                db,
                CHAR,
                "chat-seed-for-test",
                7,
                None,
                &[],
                crate::clock::now_unix_ms() as f64,
                None,
            )
            .unwrap()
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
            .collect()
        };
        let before = archive(&db);
        assert_eq!(before.len(), 2);
        db.write(|w| {
            w.main().connection().execute(
                "INSERT INTO memories (id, characterId, content, summary, keywords, tags, \
                 importance, source, reinforcementCount, relatedMemoryIds, reinforcedImportance, \
                 tier, consolidatedFrom, entities, kind, createdAt, updatedAt) VALUES \
                 ('a0000000-0000-4000-8000-0000000000ff', ?1, 'p', 'p', '[]', '[]', 0.9, \
                 'MANUAL', 1, '[]', 0.9, 'hot', '[]', '[]', 'semantic', ?2, ?2)",
                rusqlite::params![CHAR, T],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let provider = AnyText::default();
        let (outcome, lines) =
            crate::test_support::global_capture::capture_async(deduplicate_character_memories(
                &db,
                Some(&provider),
                CHAR,
                "Friday",
                0.8,
                false,
                Some("user-1"),
            ))
            .await;
        let outcome = outcome.unwrap();
        assert_eq!(outcome.removed_count, 0, "the loser is kept");
        assert!(row(&db, LOSER).is_some());
        let fold = lines
            .iter()
            .find(|l| l.contains("[MemoryDedup] Failed to fold discards into survivor"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(fold.starts_with("WARN "), "{fold:?}");
        assert!(
            fold.contains(&format!(
                " context=memory-dedup.deduplicateCharacterMemories characterId={CHAR} \
                 survivorId={KEEP} error="
            )),
            "{fold:?}"
        );
        assert!(fold.contains("planted fold failure"), "{fold:?}");
        let kept = lines
            .iter()
            .find(|l| l.contains("[MemoryDedup] Some folds failed; their discards were kept"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(
            kept.contains(&format!(
                " context=memory-dedup.deduplicateCharacterMemories characterId={CHAR} kept=1"
            )),
            "{kept:?}"
        );
        let deleted = lines
            .iter()
            .find(|l| l.contains("[MemoryDedup] Bulk deleted memories"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(deleted.contains(" requested=0 deleted=0"), "{deleted:?}");
        assert_ne!(
            archive(&db),
            before,
            "the dedup apply invalidated the archive"
        );
    }

    /// `apply_memory_merge`'s arms (v4 `memory-merge.ts:139-176`): an empty
    /// patch DEBUGs `Nothing to fold`; no `userId` WARNs (unreachable through
    /// dedup, which always passes one — pinned here, recorded) and the DEBUG
    /// says `reembedded=false`; a `None` embedding seam (R-D) WARNs v4's
    /// re-embed line with v5's seam text and the DEBUG still says
    /// `reembedded=true`.
    #[tokio::test]
    async fn apply_memory_merge_logs_each_arm() {
        const CHAR: &str = "c0000000-0000-4000-8000-0000000000d4";
        let (_d, db) = make_db(
            CHAR,
            &[
                m(KEEP, "Friday keeps the ledger.", 0.9),
                m(LOSER, "Friday keeps the ledger for Marguerite.", 0.2),
            ],
        );
        let survivor = row(&db, KEEP).unwrap();
        let loser = row(&db, LOSER).unwrap();

        let empty = plan_memory_merge(&survivor, &[], &[]);
        let (r, lines) = crate::test_support::global_capture::capture_async(
            crate::services::memory_gate::apply_memory_merge::<AnyText>(
                &db,
                None,
                &survivor,
                &empty,
                Some("u"),
                None,
            ),
        )
        .await;
        assert_eq!(r.unwrap(), Some(survivor.clone()));
        let line = lines
            .iter()
            .find(|l| l.contains("[MemoryMerge] Nothing to fold into survivor"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(line.contains(&format!(" survivorId={KEEP}")), "{line:?}");

        let plan = plan_memory_merge(&survivor, std::slice::from_ref(&loser), &[]);
        let (r, lines) = crate::test_support::global_capture::capture_async(
            crate::services::memory_gate::apply_memory_merge::<AnyText>(
                &db, None, &survivor, &plan, None, None,
            ),
        )
        .await;
        assert!(r.unwrap().is_some());
        let warn = lines
            .iter()
            .find(|l| l.contains("[MemoryMerge] Survivor changed but no userId to re-embed with"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(warn.starts_with("WARN "), "{warn:?}");
        assert!(lines
            .iter()
            .any(|l| l.contains("Folded losers into survivor") && l.contains(" reembedded=false")));
        // The same (pre-fold) snapshot and plan, so the content still changes.
        let plan = plan_memory_merge(&survivor, std::slice::from_ref(&loser), &[]);
        let (r, lines) = crate::test_support::global_capture::capture_async(
            crate::services::memory_gate::apply_memory_merge::<AnyText>(
                &db,
                None,
                &survivor,
                &plan,
                Some("u"),
                None,
            ),
        )
        .await;
        assert!(r.unwrap().is_some());
        let warn = lines
            .iter()
            .find(|l| l.contains("[MemoryGate] Failed to re-embed memory"))
            .unwrap_or_else(|| panic!("{lines:#?}"));
        assert!(
            warn.contains(&format!(
                " memoryId={KEEP} error=Error: memory embedding not assembled (dedup re-embed seam)"
            )),
            "{warn:?}"
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("Folded losers into survivor") && l.contains(" reembedded=true")));
    }
}
