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
#[derive(Debug, Clone)]
struct VectorEntry {
    id: String,
    embedding: Vec<f32>,
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
    pub fn load(conn: &Connection, character_id: &str) -> Result<Self, DbError> {
        let repo = VectorIndicesRepository::new(conn);
        let meta = repo.find_meta_by_character_id(character_id)?;
        let rows = repo.find_entries_by_character_id(character_id)?;

        let mut entries = Vec::with_capacity(rows.len());
        let mut index = HashMap::with_capacity(rows.len());
        for row in rows.iter() {
            index.insert(row.id.clone(), entries.len());
            entries.push(VectorEntry {
                id: row.id.clone(),
                embedding: row.embedding.clone(),
            });
        }

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

    /// Whether an entry with this id is loaded (v4 `hasVector`).
    pub fn has_vector(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Cosine-similarity search returning the top `limit` matches, descending by
    /// score (v4 `search` → `searchLinear`). Empty store → `[]`; a query whose
    /// length disagrees with the store's known `dimensions` → `[]` (v4 logs a warn
    /// and returns empty so the caller falls back to text search).
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<VectorSearchResult> {
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

    /// Add a vector (v4 `addVector`). Validates the dimension against the store's
    /// known width (setting it on the first add into an empty store), then records
    /// the entry as added (cancelling any pending removal, as v4 does).
    pub fn add_vector(&mut self, id: &str, embedding: Vec<f32>) -> Result<(), DbError> {
        if let Some(dims) = self.dimensions {
            if embedding.len() != dims {
                return Err(DbError::Key(format!(
                    "Vector dimension mismatch: expected {dims}, got {}",
                    embedding.len()
                )));
            }
        } else {
            self.dimensions = Some(embedding.len());
        }

        match self.index.get(id).copied() {
            Some(i) => self.entries[i].embedding = embedding,
            None => {
                self.index.insert(id.to_string(), self.entries.len());
                self.entries.push(VectorEntry {
                    id: id.to_string(),
                    embedding,
                });
            }
        }
        push_unique(&mut self.added, id);
        self.removed.retain(|r| r != id);
        Ok(())
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
            return Err(DbError::Key(format!(
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
            });
        }
        s
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
}
