//! Port of v4's in-memory `CharacterVectorStore` (`lib/embedding/vector-
//! store.ts`) at the `83334b9a5` pin — P4.D288 (C1-3), memory refactor Phase B.
//!
//! "It is a **cache**, never a copy: `memories.embedding` and `memories.tier`
//! are the only stored truth (the `vector_entries` mirror was retired in Phase
//! B of the memory refactor), a store loads them with one query, and nothing
//! here writes the database." [`CharacterVectorStore::load`] reads the
//! character's rows ([`super::memories_read::find_index_rows_by_character_id`])
//! and is INFALLIBLE: a failed read leaves the store empty — search then finds
//! nothing and recall falls back to text search — rather than failing a turn.
//! Every write of a vector or a tier goes through `services::memory_index`.
//!
//! **No process cache (L-2).** v5 never had v4's `VectorStoreManager`; every
//! caller loads, so every search sees v4's LOADED-cache state.
//!
//! ## Entries of any length (a corpus part-way through a re-embed)
//!
//! v4 keeps a per-length entry count (`lengthCounts`, a `Map` in insertion
//! order — a length whose count falls to zero is DELETED, so a later one is
//! appended) and [`dimensions`](CharacterVectorStore::dimensions) answers the
//! DOMINANT length (the first length, in that order, with the strictly greatest
//! count). Search scores only the entries whose length equals the query's; a
//! query no entry matches WARNs once and answers `[]`.
//!
//! ## Search fidelity
//!
//! Both v4 paths are ported: the linear path (stable descending sort, then the
//! top `limit` — V8's sort is stable, so ties keep insertion order) for stores
//! under 1000 entries or when `limit * 4 >= size`, and the bounded min-heap
//! (`searchHeap`) otherwise — the heap's tie order differs from the linear
//! path's, so the two are not interchangeable at scale.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::embedding_vector::cosine_similarity;

/// v4 `MemoryTier` (`'hot' | 'cold'`) as the index carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Hot,
    Cold,
}

impl Tier {
    /// v4's literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Hot => "hot",
            Tier::Cold => "cold",
        }
    }

    /// v4's row read: `cold` iff the cell is exactly `'cold'`.
    pub fn of_cell(cell: Option<&str>) -> Self {
        if cell == Some("cold") {
            Tier::Cold
        } else {
            Tier::Hot
        }
    }
}

/// One entry (v4 `VectorEntry`): the id (the memory id), the vector, the tier.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub tier: Tier,
}

/// A search hit (v4 `VectorSearchResult` — the metadata the callers read is
/// the tier): the entry id, its cosine score, its tier.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f64,
    pub tier: Tier,
}

/// v4's linear/heap switch: a store under this size always scans linearly.
const HEAP_THRESHOLD: usize = 1000;

/// A per-character in-memory vector index (v4 `CharacterVectorStore`).
#[derive(Debug, Clone)]
pub struct CharacterVectorStore {
    character_id: String,
    /// Entries in insertion order (v4's `Map` iteration order).
    entries: Vec<VectorEntry>,
    /// `id → index into entries`.
    index: HashMap<String, usize>,
    /// v4 `lengthCounts` — `(length, count)` in the `Map`'s insertion order.
    length_counts: Vec<(usize, usize)>,
}

impl CharacterVectorStore {
    /// An empty store for `character_id`.
    pub fn empty(character_id: &str) -> Self {
        Self {
            character_id: character_id.to_string(),
            entries: Vec::new(),
            index: HashMap::new(),
            length_counts: Vec::new(),
        }
    }

    /// v4 `load()` (`vector-store.ts:116-148`): the character's vectors and
    /// tiers from `memories` (one query), then DEBUG `Loaded vector index from
    /// memory rows {context: CharacterVectorStore.load, characterId, entries,
    /// cold, dimensions}`. The row read is a FALLBACK (its own ERROR, `[]`);
    /// inside the strict scope it rethrows, and v4's catch logs ERROR `Error
    /// loading vector index from memory rows {context, characterId, error}` and
    /// leaves the store EMPTY. Never fails.
    pub fn load(conn: &Connection, character_id: &str) -> Self {
        let mut store = Self::empty(character_id);
        match super::memories_read::find_index_rows_by_character_id(conn, character_id) {
            Ok(rows) => {
                let mut cold = 0usize;
                let n = rows.len();
                for row in rows {
                    if row.tier == Tier::Cold {
                        cold += 1;
                    }
                    store.put(VectorEntry {
                        id: row.id,
                        embedding: row.embedding,
                        tier: row.tier,
                    });
                }
                tracing::debug!(
                    context = "CharacterVectorStore.load",
                    characterId = %character_id,
                    entries = n,
                    cold = cold,
                    dimensions = %store.dimensions().map_or("null".to_string(), |d| d.to_string()),
                    "Loaded vector index from memory rows"
                );
            }
            Err(error) => {
                tracing::error!(
                    context = "CharacterVectorStore.load",
                    characterId = %character_id,
                    error = %super::fallback::error_text(&error),
                    "Error loading vector index from memory rows"
                );
                store.clear();
            }
        }
        store
    }

    /// v4 `put(entry)`: replace in place (a `Map.set` on a present key keeps its
    /// position) or append; the length counts follow.
    fn put(&mut self, entry: VectorEntry) {
        let len = entry.embedding.len();
        match self.index.get(&entry.id).copied() {
            Some(i) => {
                let previous = self.entries[i].embedding.len();
                self.uncount(previous);
                self.entries[i] = entry;
            }
            None => {
                self.index.insert(entry.id.clone(), self.entries.len());
                self.entries.push(entry);
            }
        }
        match self.length_counts.iter_mut().find(|(l, _)| *l == len) {
            Some((_, n)) => *n += 1,
            None => self.length_counts.push((len, 1)),
        }
    }

    /// v4 `uncount(length)`: one fewer; a length at zero is DELETED from the map.
    fn uncount(&mut self, length: usize) {
        if let Some(at) = self.length_counts.iter().position(|(l, _)| *l == length) {
            if self.length_counts[at].1 > 1 {
                self.length_counts[at].1 -= 1;
            } else {
                self.length_counts.remove(at);
            }
        }
    }

    /// v4 `addVector(id, embedding, metadata)` — in memory only.
    pub fn add_vector(&mut self, id: &str, embedding: Vec<f32>, tier: Tier) {
        self.put(VectorEntry {
            id: id.to_string(),
            embedding,
            tier,
        });
    }

    /// v4 `removeVector(id)` — in memory only; `false` when absent.
    pub fn remove_vector(&mut self, id: &str) -> bool {
        let Some(i) = self.index.remove(id) else {
            return false;
        };
        let entry = self.entries.remove(i);
        for at in self.index.values_mut() {
            if *at > i {
                *at -= 1;
            }
        }
        self.uncount(entry.embedding.len());
        true
    }

    /// v4 `hasVector(id)`.
    pub fn has_vector(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// v4 `tierOf(id)` — `None` when the entry is not in the store.
    pub fn tier_of(&self, id: &str) -> Option<Tier> {
        self.index.get(id).map(|&i| self.entries[i].tier)
    }

    /// v4 `isHotVector` for one id: `true` unless the entry is cold; an ABSENT
    /// id answers `true` ("absent reads as hot").
    pub fn is_hot(&self, id: &str) -> bool {
        self.tier_of(id) != Some(Tier::Cold)
    }

    /// v4 `size`.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// v4 `getDimensions()` — the most common vector length (the first, in
    /// the counts' insertion order, with the strictly greatest count), or
    /// `None` when the store is empty.
    pub fn dimensions(&self) -> Option<usize> {
        let mut best = None;
        let mut best_count = 0usize;
        for &(length, count) in &self.length_counts {
            if count > best_count {
                best = Some(length);
                best_count = count;
            }
        }
        best
    }

    /// v4 `countWithDimensions(dimensions)`.
    pub fn count_with_dimensions(&self, dimensions: usize) -> usize {
        self.length_counts
            .iter()
            .find(|(l, _)| *l == dimensions)
            .map_or(0, |(_, n)| *n)
    }

    /// v4 `getAllEntries()` — every entry, in insertion order.
    pub fn entries(&self) -> &[VectorEntry] {
        &self.entries
    }

    /// Every entry as `(id, embedding)`, in insertion order (the read-only
    /// walks: consolidation clustering, the optimizer, housekeeping's lookup).
    pub fn all_entries(&self) -> impl Iterator<Item = (&str, &[f32])> {
        self.entries
            .iter()
            .map(|e| (e.id.as_str(), e.embedding.as_slice()))
    }

    /// v4 `clear()`.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
        self.length_counts.clear();
    }

    /// v4 `search(query, limit)` with no filter — BOTH tiers.
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<VectorSearchResult> {
        self.search_filtered(query, limit, |_, _| true)
    }

    /// v4 `search(query, limit, filter)` (`vector-store.ts:243-270`): an empty
    /// store → `[]`; no entry of the query's length → WARN `Query vector
    /// dimension mismatch — search embedding profile likely differs from index
    /// {context: CharacterVectorStore.search, characterId, storedDimensions,
    /// queryDimensions}` and `[]`; else the linear or the heap path. The
    /// filter gets `(id, is_cold)` and runs FIRST for each entry, before the
    /// length skip and the scoring, so a filtered entry never takes a slot.
    pub fn search_filtered(
        &self,
        query: &[f32],
        limit: usize,
        filter: impl Fn(&str, bool) -> bool,
    ) -> Vec<VectorSearchResult> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        if self.count_with_dimensions(query.len()) == 0 {
            tracing::warn!(
                context = "CharacterVectorStore.search",
                characterId = %self.character_id,
                storedDimensions = %self.dimensions().map_or("null".to_string(), |d| d.to_string()),
                queryDimensions = query.len(),
                "Query vector dimension mismatch — search embedding profile likely differs from index"
            );
            return Vec::new();
        }
        if self.entries.len() < HEAP_THRESHOLD || limit.saturating_mul(4) >= self.entries.len() {
            self.search_linear(query, limit, &filter)
        } else {
            self.search_heap(query, limit, &filter)
        }
    }

    /// The candidates a search scores, in insertion order.
    fn scored<'a>(
        &'a self,
        query: &'a [f32],
        filter: &'a dyn Fn(&str, bool) -> bool,
    ) -> impl Iterator<Item = VectorSearchResult> + 'a {
        self.entries.iter().filter_map(move |entry| {
            if !filter(&entry.id, entry.tier == Tier::Cold) {
                return None;
            }
            if entry.embedding.len() != query.len() {
                return None;
            }
            cosine_similarity(query, &entry.embedding)
                .ok()
                .map(|score| VectorSearchResult {
                    id: entry.id.clone(),
                    score,
                    tier: entry.tier,
                })
        })
    }

    /// v4 `searchLinear`: score every candidate, stable sort descending, take
    /// the top `limit`.
    fn search_linear(
        &self,
        query: &[f32],
        limit: usize,
        filter: &dyn Fn(&str, bool) -> bool,
    ) -> Vec<VectorSearchResult> {
        let mut results: Vec<VectorSearchResult> = self.scored(query, filter).collect();
        // `b.score - a.score`: stable, a NaN difference compares equal.
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// v4 `searchHeap` (`vector-store.ts:318-380`): a bounded min-heap of
    /// `limit` kept as two parallel arrays, a candidate replacing the root only
    /// when it scores strictly higher, then drained into descending order —
    /// transcribed operation for operation, so ties land where v4's do.
    fn search_heap(
        &self,
        query: &[f32],
        limit: usize,
        filter: &dyn Fn(&str, bool) -> bool,
    ) -> Vec<VectorSearchResult> {
        if limit == 0 {
            // v4: `heapSize < 0` never holds and `score > undefined` is false.
            return Vec::new();
        }
        let mut scores: Vec<f64> = Vec::with_capacity(limit);
        let mut results: Vec<VectorSearchResult> = Vec::with_capacity(limit);

        fn sift_up(scores: &mut [f64], results: &mut [VectorSearchResult], mut i: usize) {
            while i > 0 {
                let parent = (i - 1) >> 1;
                // `if (heapScores[parent] <= heapScores[i]) break` — a NaN
                // comparison is false, so it swaps.
                if scores[parent] <= scores[i] {
                    break;
                }
                scores.swap(parent, i);
                results.swap(parent, i);
                i = parent;
            }
        }
        fn sift_down(
            scores: &mut [f64],
            results: &mut [VectorSearchResult],
            size: usize,
            mut i: usize,
        ) {
            loop {
                let l = i * 2 + 1;
                let r = i * 2 + 2;
                let mut min = i;
                if l < size && scores[l] < scores[min] {
                    min = l;
                }
                if r < size && scores[r] < scores[min] {
                    min = r;
                }
                if min == i {
                    break;
                }
                scores.swap(min, i);
                results.swap(min, i);
                i = min;
            }
        }

        for candidate in self.scored(query, filter) {
            if scores.len() < limit {
                scores.push(candidate.score);
                results.push(candidate);
                let last = scores.len() - 1;
                sift_up(&mut scores, &mut results, last);
            } else if candidate.score > scores[0] {
                scores[0] = candidate.score;
                results[0] = candidate;
                let size = scores.len();
                sift_down(&mut scores, &mut results, size, 0);
            }
        }

        // Drain into descending order: `out[i] = heapResults[0]`, the last
        // moved to the root, sift down — from the back.
        let mut size = scores.len();
        let mut out: Vec<Option<VectorSearchResult>> = vec![None; size];
        for i in (0..size).rev() {
            out[i] = Some(results[0].clone());
            scores[0] = scores[size - 1];
            results[0] = results[size - 1].clone();
            size -= 1;
            if size > 0 {
                sift_down(&mut scores, &mut results, size, 0);
            }
        }
        out.into_iter().map(|r| r.expect("drained")).collect()
    }

    /// v4 `setTier(ids, tier)` (`vector-store.ts:382-398`): re-stamp the tier of
    /// entries already in the store, in memory only, counting those whose tier
    /// actually changed, then DEBUG `Re-stamped vector tiers {context:
    /// CharacterVectorStore.setTier, characterId, tier, requested, changed}` —
    /// logged even when `changed` is 0. An id not in the store counts in
    /// `requested` only.
    pub fn set_tier(&mut self, ids: &[String], tier: Tier) -> usize {
        let mut changed = 0usize;
        for id in ids {
            if let Some(&i) = self.index.get(id) {
                if self.entries[i].tier != tier {
                    self.entries[i].tier = tier;
                    changed += 1;
                }
            }
        }
        tracing::debug!(
            context = "CharacterVectorStore.setTier",
            characterId = %self.character_id,
            tier = tier.as_str(),
            requested = ids.len(),
            changed = changed,
            "Re-stamped vector tiers"
        );
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(entries: &[(&str, Vec<f32>, Tier)]) -> CharacterVectorStore {
        let mut s = CharacterVectorStore::empty("char");
        for (id, v, tier) in entries {
            s.add_vector(id, v.clone(), *tier);
        }
        s
    }

    #[test]
    fn empty_store_returns_no_results() {
        assert!(CharacterVectorStore::empty("c")
            .search(&[1.0], 5)
            .is_empty());
    }

    #[test]
    fn search_ranks_by_cosine_descending_and_carries_the_tier() {
        let s = store_with(&[
            ("a", vec![1.0, 0.0], Tier::Hot),
            ("b", vec![0.6, 0.8], Tier::Cold),
            ("c", vec![0.0, 1.0], Tier::Hot),
        ]);
        let hits = s.search(&[1.0, 0.0], 2);
        assert_eq!(
            hits.iter()
                .map(|h| (h.id.as_str(), h.tier))
                .collect::<Vec<_>>(),
            [("a", Tier::Hot), ("b", Tier::Cold)]
        );
    }

    #[test]
    fn search_filtered_excludes_cold_before_top_k() {
        let s = store_with(&[
            ("a", vec![1.0, 0.0], Tier::Cold),
            ("b", vec![0.6, 0.8], Tier::Hot),
            ("c", vec![0.0, 1.0], Tier::Hot),
        ]);
        let hits = s.search_filtered(&[1.0, 0.0], 1, |_, cold| !cold);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "b");
    }

    /// Mixed lengths: the dominant length wins (the first to reach the
    /// strictly greatest count on a tie), and search scores only matching
    /// entries — the WARN only when NONE match.
    #[test]
    fn mixed_lengths_dominant_dimensions_and_length_skip() {
        let s = store_with(&[
            ("a", vec![1.0, 0.0], Tier::Hot),
            ("b", vec![1.0, 0.0, 0.0], Tier::Hot),
            ("c", vec![0.0, 1.0], Tier::Hot),
            ("d", vec![0.0, 1.0, 0.0], Tier::Hot),
        ]);
        assert_eq!(s.dimensions(), Some(2), "a tie: the first length wins");
        assert_eq!(s.count_with_dimensions(3), 2);
        let hits = s.search(&[0.0, 0.0, 1.0], 5);
        assert_eq!(hits.len(), 2);
        let (none, lines) = crate::test_support::captured_with(|| s.search(&[1.0; 4], 5));
        assert!(none.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("Query vector dimension mismatch"),
            "{lines:?}"
        );
        assert!(
            lines[0].contains("storedDimensions=2 queryDimensions=4"),
            "{lines:?}"
        );
    }

    /// A length whose count falls to zero is DELETED from the counts, so a
    /// later length is appended after the survivors (v4's `Map`).
    #[test]
    fn a_removed_length_rejoins_at_the_end() {
        let mut s = store_with(&[
            ("a", vec![1.0, 0.0], Tier::Hot),
            ("b", vec![1.0, 0.0, 0.0], Tier::Hot),
        ]);
        assert_eq!(s.dimensions(), Some(2));
        assert!(s.remove_vector("a"));
        s.add_vector("c", vec![0.0, 1.0], Tier::Hot);
        assert_eq!(
            s.dimensions(),
            Some(3),
            "length 2 re-entered AFTER length 3"
        );
        assert!(!s.remove_vector("a"));
        assert_eq!(s.size(), 2);
    }

    #[test]
    fn replacing_an_entry_keeps_its_position() {
        let mut s = store_with(&[
            ("a", vec![1.0, 0.0], Tier::Hot),
            ("b", vec![1.0, 0.0], Tier::Hot),
        ]);
        s.add_vector("a", vec![1.0, 0.0], Tier::Cold);
        assert_eq!(s.entries()[0].id, "a");
        assert_eq!(s.tier_of("a"), Some(Tier::Cold));
        assert_eq!(s.count_with_dimensions(2), 2);
    }

    #[test]
    fn set_tier_counts_changes_and_always_logs() {
        let mut s = store_with(&[("a", vec![1.0], Tier::Hot), ("b", vec![1.0], Tier::Cold)]);
        let (changed, lines) = crate::test_support::captured_with(|| {
            s.set_tier(&["a".into(), "b".into(), "zz".into()], Tier::Cold)
        });
        assert_eq!(changed, 1);
        assert_eq!(
            lines,
            ["DEBUG quilltap_core::db::vector_store Re-stamped vector tiers context=CharacterVectorStore.setTier characterId=char tier=cold requested=3 changed=1"]
        );
    }

    /// The heap path (≥ 1000 entries, `limit * 4 < size`) answers the same
    /// top-K SET as the linear path for distinct scores.
    #[test]
    fn heap_and_linear_agree_on_distinct_scores() {
        let mut s = CharacterVectorStore::empty("c");
        for i in 0..1200usize {
            let x = (i as f32) / 1200.0;
            s.add_vector(&format!("m{i}"), vec![x, 1.0 - x], Tier::Hot);
        }
        let heap = s.search(&[1.0, 0.0], 10);
        let linear = s.search_linear(&[1.0, 0.0], 10, &|_, _| true);
        assert_eq!(heap, linear);
        assert_eq!(heap[0].id, "m1199");
    }
}
