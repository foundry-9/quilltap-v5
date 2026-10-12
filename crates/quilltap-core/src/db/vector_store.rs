//! Port of v4's in-memory `CharacterVectorStore`
//! (`lib/embedding/vector-store.ts`) — the per-character vector index the memory
//! gate searches. It is an **in-memory** structure loaded from the two
//! `vector_indices` / `vector_entries` tables, searched by cosine similarity, and
//! flushed back incrementally.
//!
//! ## Split from persistence (why load + flush are separate)
//!
//! v4's store loads, searches, and saves through the same async object because it
//! holds a repo handle. Under the Phase-3 writer-task runtime the two halves live
//! on different connections: **`load` runs on a pooled read-only connection** (the
//! gate reads the store off the read pool, never contending with the writer),
//! while **`flush` runs on the writer thread** (the only mutator). So `load` takes
//! a `&Connection` and returns an owned snapshot; `flush` takes a borrowed
//! [`VectorIndicesRepository`] (constructed over the writer's RW connection) and
//! replays v4's `save()` exactly. The in-memory model in between (dirty tracking,
//! `search`, `add_vector` / `update_vector`) is a faithful port.
//!
//! ## Search fidelity
//!
//! `search` reproduces v4's **linear path** (`searchLinear`): score every entry by
//! cosine similarity, then `sort((a,b) => b.score - a.score).slice(0, limit)`. The
//! memory-gate corpus is always well under v4's 1000-entry heap threshold, so only
//! the linear path is ported (the heap path is a pure performance optimization that
//! returns the identical top-K). Rust's `sort_by` is **stable**, matching V8's
//! stable `Array.prototype.sort`, so ties preserve insertion (= DB row) order — and
//! insertion order is the `find_entries_by_character_id` row order, identical on
//! both differential sides (same engine, same file). The dimension guard
//! (`dimensions is Some && query.len() != dims → []`) and the per-entry length
//! skip are reproduced.

use std::collections::HashMap;

use rusqlite::Connection;

use super::vector_indices::{VectorEntryInput, VectorIndicesRepository};
use super::DbError;
use crate::embedding_vector::cosine_similarity;

/// One in-memory vector entry (id + its unit-length embedding). v4's `VectorEntry`
/// also carries metadata + `createdAt`, but the gate's search only needs the id and
/// the vector; the persisted `createdAt` is minted by the repo on flush.
///
/// P4.D265 (v4 `d58548051`): `cold` is v4's `metadata.tier === 'cold'` — NOT
/// persisted with the index; stamped from the memories table at
/// [`CharacterVectorStore::load`] and kept in step by
/// [`CharacterVectorStore::set_tier`]. A freshly added vector is hot (v4's
/// callers pass metadata without a tier, and an absent tier reads hot).
#[derive(Debug, Clone)]
struct VectorEntry {
    id: String,
    embedding: Vec<f32>,
    cold: bool,
}

/// v4's `'hot' | 'cold'` tier argument to [`CharacterVectorStore::set_tier`].
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
}

/// A single search hit (v4's `VectorSearchResult` minus the metadata the gate does
/// not consume): the matched entry id and its cosine score.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f64,
}

/// A per-character in-memory vector store (v4 `CharacterVectorStore`). Load it with
/// [`CharacterVectorStore::load`], query with [`search`](Self::search), mutate with
/// [`add_vector`](Self::add_vector) / [`update_vector`](Self::update_vector), and
/// persist the accumulated changes with [`flush`](Self::flush).
pub struct CharacterVectorStore {
    character_id: String,
    /// Entries in load (DB row) order — the order `search` iterates, so a stable
    /// sort ties break the same way v4's `Map` iteration does.
    entries: Vec<VectorEntry>,
    /// `id → index into entries`, for O(1) `has_vector` / `update_vector`.
    index: HashMap<String, usize>,
    dimensions: Option<usize>,
    // Granular dirty tracking for incremental persistence (v4's three Sets), kept
    // as insertion-ordered de-duplicated lists.
    added: Vec<String>,
    removed: Vec<String>,
    updated: Vec<String>,
}

impl CharacterVectorStore {
    /// Load a character's index from the DB (v4 `load`). Reads the metadata row and
    /// every entry; `dimensions` comes from the metadata when present, else from the
    /// first entry's length (else `None` for an empty store).
    ///
    /// P4.D265 (v4 `d58548051`): AFTER the entries, the character's cold memory
    /// ids ([`super::memories_read::find_cold_ids_by_character_id`] — v4's
    /// `loadColdIds`) stamp each entry's tier, then DEBUG `Loaded vector index
    /// {context: CharacterVectorStore.load, characterId, entries, cold}` (`cold`
    /// is the cold-id SET's size, as v4's `coldIds.size` is — a cold memory with
    /// no vector still counts). The cold-id read is a fallback (`[]` + its own
    /// ERROR), so v4's WARN `Could not read cold memory ids; treating every
    /// vector as hot {context, characterId, error}` is reachable ONLY inside the
    /// strict scope, where the read rethrows — then every vector reads hot.
    pub fn load(conn: &Connection, character_id: &str) -> Result<Self, DbError> {
        let repo = VectorIndicesRepository::new(conn);
        let meta = repo.find_meta_by_character_id(character_id)?;
        let rows = repo.find_entries_by_character_id(character_id)?;
        let cold_ids: std::collections::HashSet<String> =
            match super::memories_read::find_cold_ids_by_character_id(conn, character_id) {
                Ok(ids) => ids.into_iter().collect(),
                Err(error) => {
                    tracing::warn!(
                        context = "CharacterVectorStore.load",
                        characterId = %character_id,
                        error = %super::fallback::error_text(&error),
                        "Could not read cold memory ids; treating every vector as hot"
                    );
                    std::collections::HashSet::new()
                }
            };

        let mut entries = Vec::with_capacity(rows.len());
        let mut index = HashMap::with_capacity(rows.len());
        for row in rows.iter() {
            index.insert(row.id.clone(), entries.len());
            entries.push(VectorEntry {
                id: row.id.clone(),
                embedding: row.embedding.clone(),
                cold: cold_ids.contains(&row.id),
            });
        }
        tracing::debug!(
            context = "CharacterVectorStore.load",
            characterId = %character_id,
            entries = rows.len(),
            cold = cold_ids.len(),
            "Loaded vector index"
        );

        // v4: meta ? meta.dimensions : (entries.length > 0 ? entries[0].len : null)
        let dimensions = match &meta {
            Some(m) => Some(m.dimensions as usize),
            None => entries.first().map(|e| e.embedding.len()),
        };

        Ok(Self {
            character_id: character_id.to_string(),
            entries,
            index,
            dimensions,
            added: Vec::new(),
            removed: Vec::new(),
            updated: Vec::new(),
        })
    }

    /// Number of entries currently in the store (v4 `size`).
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// v4 `getDimensions()` — the store's vector width: the metadata row's
    /// `dimensions` when there is one, else the first loaded entry's length, else
    /// `None` for an empty store with no meta (all decided in
    /// [`load`](Self::load)).
    ///
    /// The housekeeping merge pass reads it to skip entries left over from a
    /// previous embedding profile (P4.d27 / v4 `7391404e`): a vector of another
    /// width cannot be compared against this index and would warn on every
    /// `search` call.
    pub fn dimensions(&self) -> Option<usize> {
        self.dimensions
    }

    /// Whether an entry with this id is loaded (v4 `hasVector`).
    pub fn has_vector(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Every loaded entry as `(id, embedding)`, in load (DB row) order — v4
    /// `getAllEntries()` (a `Map` iterates in insertion order). Housekeeping's
    /// merge pass uses this to look up each memory's stored embedding.
    pub fn all_entries(&self) -> impl Iterator<Item = (&str, &[f32])> {
        self.entries
            .iter()
            .map(|e| (e.id.as_str(), e.embedding.as_slice()))
    }

    /// Cosine-similarity search returning the top `limit` matches, descending by
    /// score (v4 `search` → `searchLinear`). Empty store → `[]`; a query whose
    /// length disagrees with the store's known `dimensions` → `[]` (v4 logs a warn
    /// and returns empty so the caller falls back to text search). BOTH tiers —
    /// v4's `search` with no filter (the gate, the `search` tool, the Memories
    /// API search).
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<VectorSearchResult> {
        self.search_filtered(query, limit, |_, _| true)
    }

    /// P4.D265 (v4 `d58548051`) — v4 `search(query, limit, filter)`: the filter
    /// gets `(id, is_cold)` and runs FIRST for each entry — before the per-entry
    /// length skip and the scoring (`searchLinear` `:376-392`) — so a filtered
    /// entry never reaches the top-K and `limit` passing results still come back.
    /// v4's hot-only recall paths pass `isHotVector` → `|_, cold| !cold`.
    pub fn search_filtered(
        &self,
        query: &[f32],
        limit: usize,
        filter: impl Fn(&str, bool) -> bool,
    ) -> Vec<VectorSearchResult> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        if let Some(dims) = self.dimensions {
            if query.len() != dims {
                return Vec::new();
            }
        }

        let mut results: Vec<VectorSearchResult> = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            if !filter(&entry.id, entry.cold) {
                continue;
            }
            if entry.embedding.len() != query.len() {
                continue;
            }
            // Lengths are equal here, so cosine_similarity never returns the
            // mismatch error; a defensive skip keeps parity with v4's `continue`.
            if let Ok(score) = cosine_similarity(query, &entry.embedding) {
                results.push(VectorSearchResult {
                    id: entry.id.clone(),
                    score,
                });
            }
        }

        // Stable descending sort (matches V8's stable sort + `b.score - a.score`),
        // then take the top-K.
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// P4.D265 — v4 `isHotVector` for one loaded id: `true` unless the entry is
    /// stamped cold. An ABSENT id answers `true` (v4: "absent reads as hot").
    pub fn is_hot(&self, id: &str) -> bool {
        self.index.get(id).is_none_or(|&i| !self.entries[i].cold)
    }

    /// P4.D265 (v4 `d58548051`) — v4 `setTier(ids, tier)` (`:506-522`): re-stamp
    /// the tier of entries already in the store (in memory only), counting only
    /// those whose tier actually changed, then DEBUG `Re-stamped vector tiers
    /// {context: CharacterVectorStore.setTier, characterId, tier, requested,
    /// changed}` — logged even when `changed` is 0. An id not in the store is
    /// skipped (it counts in `requested` only).
    pub fn set_tier(&mut self, ids: &[String], tier: Tier) {
        let cold = tier == Tier::Cold;
        let mut changed = 0usize;
        for id in ids {
            if let Some(&i) = self.index.get(id) {
                if self.entries[i].cold != cold {
                    self.entries[i].cold = cold;
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
    }

    /// Add a vector (v4 `addVector`). Validates the dimension against the store's
    /// known width (setting it on the first add into an empty store), then records
    /// the entry as added (cancelling any pending removal, as v4 does).
    pub fn add_vector(&mut self, id: &str, embedding: Vec<f32>) -> Result<(), DbError> {
        if let Some(dims) = self.dimensions {
            if embedding.len() != dims {
                return Err(DbError::Internal(format!(
                    "Vector dimension mismatch: expected {dims}, got {}",
                    embedding.len()
                )));
            }
        } else {
            self.dimensions = Some(embedding.len());
        }

        // v4 replaces the whole entry (`entries.set(id, {…, metadata})`), and its
        // callers' metadata carries no tier — so a re-added vector reads hot.
        match self.index.get(id).copied() {
            Some(i) => {
                self.entries[i].embedding = embedding;
                self.entries[i].cold = false;
            }
            None => {
                self.index.insert(id.to_string(), self.entries.len());
                self.entries.push(VectorEntry {
                    id: id.to_string(),
                    embedding,
                    cold: false,
                });
            }
        }
        push_unique(&mut self.added, id);
        self.removed.retain(|r| r != id);
        Ok(())
    }

    /// Remove a vector from the store (v4 `removeVector`). Returns `false` when the
    /// id is not loaded. A same-flush add is simply un-added (never persisted);
    /// otherwise the id is tracked for deletion — and any pending update for it is
    /// dropped (v4's `updatedIds.delete(id)`).
    pub fn remove_vector(&mut self, id: &str) -> bool {
        let Some(i) = self.index.remove(id) else {
            return false;
        };
        self.entries.remove(i);
        // Re-point the shifted tail entries' indices.
        for (j, entry) in self.entries.iter().enumerate().skip(i) {
            self.index.insert(entry.id.clone(), j);
        }
        if self.added.iter().any(|a| a == id) {
            self.added.retain(|a| a != id);
        } else {
            push_unique(&mut self.removed, id);
        }
        self.updated.retain(|u| u != id);
        true
    }

    /// Update an existing vector's embedding (v4 `updateVector`). Returns `false`
    /// when the id is not loaded; errors on a dimension mismatch. Only tracks the id
    /// as `updated` when it was already persisted (not a same-flush add) — v4's
    /// `if (!this.addedIds.has(id)) this.updatedIds.add(id)`.
    pub fn update_vector(&mut self, id: &str, embedding: Vec<f32>) -> Result<bool, DbError> {
        let Some(&i) = self.index.get(id) else {
            return Ok(false);
        };
        if Some(embedding.len()) != self.dimensions {
            return Err(DbError::Internal(format!(
                "Vector dimension mismatch: expected {:?}, got {}",
                self.dimensions,
                embedding.len()
            )));
        }
        self.entries[i].embedding = embedding;
        if !self.added.iter().any(|a| a == id) {
            push_unique(&mut self.updated, id);
        }
        Ok(true)
    }

    /// Persist the accumulated changes (v4 `save`), returning `true` when a write
    /// was actually issued. Replays v4's save order: batch-insert added entries,
    /// batch-delete removed, per-id update changed embeddings, then `saveMeta`
    /// (bumping the metadata `updatedAt`) — but only when there were changes.
    /// Clears the dirty tracking afterward.
    pub fn flush(&mut self, repo: &VectorIndicesRepository) -> Result<bool, DbError> {
        let has_changes =
            !self.added.is_empty() || !self.removed.is_empty() || !self.updated.is_empty();
        if !has_changes && self.entries.is_empty() {
            return Ok(false);
        }

        if !self.added.is_empty() {
            let mut new_entries: Vec<VectorEntryInput> = Vec::new();
            for id in &self.added {
                if let Some(&i) = self.index.get(id) {
                    new_entries.push(VectorEntryInput {
                        id: id.clone(),
                        character_id: self.character_id.clone(),
                        embedding: Some(self.entries[i].embedding.clone()),
                    });
                }
            }
            if !new_entries.is_empty() {
                repo.add_entries(&new_entries)?;
            }
        }

        if !self.removed.is_empty() {
            repo.remove_entries(&self.removed)?;
        }

        if !self.updated.is_empty() {
            for id in &self.updated {
                if let Some(&i) = self.index.get(id) {
                    repo.update_entry_embedding(id, Some(&self.entries[i].embedding))?;
                }
            }
        }

        if has_changes {
            // v4: `saveMeta(characterId, this.dimensions || 0)`.
            repo.save_meta(&self.character_id, self.dimensions.unwrap_or(0) as f64)?;
        }

        self.added.clear();
        self.removed.clear();
        self.updated.clear();
        Ok(has_changes)
    }
}

/// Push `id` onto a dirty-tracking list only if not already present (the Sets in v4
/// dedup; the order among distinct ids follows first insertion).
fn push_unique(list: &mut Vec<String>, id: &str) {
    if !list.iter().any(|x| x == id) {
        list.push(id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(entries: &[(&str, Vec<f32>)]) -> CharacterVectorStore {
        let mut s = CharacterVectorStore {
            character_id: "char".to_string(),
            entries: Vec::new(),
            index: HashMap::new(),
            dimensions: entries.first().map(|(_, v)| v.len()),
            added: Vec::new(),
            removed: Vec::new(),
            updated: Vec::new(),
        };
        for (id, v) in entries {
            s.index.insert((*id).to_string(), s.entries.len());
            s.entries.push(VectorEntry {
                id: (*id).to_string(),
                embedding: v.clone(),
                cold: false,
            });
        }
        s
    }

    /// P4.D265: the hot-only filter runs BEFORE the top-K — a cold entry that
    /// would rank first is excluded and `limit` hot results still come back;
    /// the unfiltered `search` keeps both tiers.
    #[test]
    fn search_filtered_excludes_cold_before_top_k() {
        let mut s = store_with(&[
            ("cold-best", vec![1.0, 0.0]),
            ("hot-a", vec![0.8, 0.6]),
            ("hot-b", vec![0.6, 0.8]),
        ]);
        s.set_tier(&["cold-best".to_string()], Tier::Cold);
        let hot: Vec<String> = s
            .search_filtered(&[1.0, 0.0], 2, |_, cold| !cold)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(hot, ["hot-a", "hot-b"]);
        let both: Vec<String> = s.search(&[1.0, 0.0], 2).into_iter().map(|r| r.id).collect();
        assert_eq!(both, ["cold-best", "hot-a"]);
        assert!(!s.is_hot("cold-best"));
        assert!(s.is_hot("hot-a"));
        assert!(s.is_hot("never-loaded"), "an absent id reads hot (v4)");
    }

    /// v4 `setTier`: counts only the entries whose tier changed, ignores
    /// absent ids, and logs its DEBUG even for a no-op re-stamp.
    #[test]
    fn set_tier_counts_changes_and_always_logs() {
        let mut s = store_with(&[("a", vec![1.0]), ("b", vec![1.0])]);
        let ids = vec!["a".to_string(), "ghost".to_string()];
        let lines = crate::test_support::captured(|| s.set_tier(&ids, Tier::Cold));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].ends_with(
                "Re-stamped vector tiers context=CharacterVectorStore.setTier characterId=char \
                 tier=cold requested=2 changed=1"
            ),
            "{lines:?}"
        );
        let lines = crate::test_support::captured(|| s.set_tier(&ids, Tier::Cold));
        assert!(lines[0].ends_with("requested=2 changed=0"), "{lines:?}");
        // A re-add reads hot again (v4 replaces the entry's metadata).
        s.add_vector("a", vec![1.0]).unwrap();
        assert!(s.is_hot("a"));
    }

    #[test]
    fn empty_store_returns_no_results() {
        let s = store_with(&[]);
        assert!(s.search(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn search_ranks_by_cosine_descending() {
        // query = (1,0). Entries at descending similarity.
        let s = store_with(&[
            ("far", vec![0.0, 1.0]),  // cosine 0.0
            ("near", vec![1.0, 0.0]), // cosine 1.0
            ("mid", vec![0.8, 0.6]),  // cosine 0.8
        ]);
        let got = s.search(&[1.0, 0.0], 5);
        let ids: Vec<&str> = got.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["near", "mid", "far"]);
        assert!((got[0].score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn search_truncates_to_limit() {
        let s = store_with(&[
            ("a", vec![1.0, 0.0]),
            ("b", vec![0.9, 0.1]),
            ("c", vec![0.8, 0.2]),
        ]);
        assert_eq!(s.search(&[1.0, 0.0], 2).len(), 2);
    }

    #[test]
    fn dimension_mismatch_query_returns_empty() {
        let s = store_with(&[("a", vec![1.0, 0.0, 0.0])]);
        assert!(s.search(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn remove_vector_tracks_persisted_entry_for_deletion() {
        let mut s = store_with(&[("a", vec![1.0, 0.0]), ("b", vec![0.0, 1.0])]);
        assert!(s.remove_vector("a"));
        assert!(!s.has_vector("a"));
        assert_eq!(s.size(), 1);
        assert_eq!(s.removed, vec!["a".to_string()]);
        // The shifted tail entry stays addressable.
        assert!(s.has_vector("b"));
        assert!(s.update_vector("b", vec![0.5, 0.5]).unwrap());
    }

    #[test]
    fn remove_vector_missing_id_is_false_and_untracked() {
        let mut s = store_with(&[("a", vec![1.0, 0.0])]);
        assert!(!s.remove_vector("nope"));
        assert!(s.removed.is_empty());
    }

    #[test]
    fn remove_vector_cancels_same_flush_add() {
        let mut s = store_with(&[]);
        s.add_vector("fresh", vec![1.0, 0.0]).unwrap();
        assert!(s.remove_vector("fresh"));
        // Never persisted, so nothing to delete on flush (v4 un-adds it).
        assert!(s.added.is_empty());
        assert!(s.removed.is_empty());
    }
}
