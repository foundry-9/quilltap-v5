//! The vector-indices repository — a Phase-2 main-DB repo port. Ports v4's
//! `lib/database/repositories/vector-indices.repository.ts` (schema
//! `lib/schemas/vector-indices.types.ts`: `VectorIndexMetaSchema` +
//! `VectorEntryRowSchema`).
//!
//! ## A standalone two-table repository
//!
//! Unlike every Phase-2 repo so far, `VectorIndicesRepository` does **NOT** extend
//! `AbstractBaseRepository`. It is hand-written and manages **two** tables in the
//! MAIN db directly with bespoke methods:
//!
//!   - `vector_indices` — per-character metadata (id, characterId, version,
//!     dimensions, timestamps). `id == characterId` (the metadata PK is the
//!     character id; there is one index row per character).
//!   - `vector_entries` — per-embedding rows (id, characterId, embedding BLOB,
//!     createdAt). The embedding is stored as a compact Float32 little-endian BLOB
//!     (~4× smaller than JSON text), exactly like `conversation_chunks` /
//!     `help_docs`.
//!
//! In v4 the collections come from `getCollection('vector_indices')` /
//! `getCollection('vector_entries')` after `ensureCollection(...)` materializes the
//! generated DDL and `registerBlobColumns('vector_entries', ['embedding'])` marks
//! the BLOB column. Here, mirroring the rest of the port, the repo borrows the
//! [`super::Writer`]'s single RW [`Connection`] and issues SQL directly; the
//! tables are materialized by the tier-2 fixture (v4's real `ensureCollection`),
//! so the on-disk shape is identical to production.
//!
//! ## Column affinities (settled against v4's schema-translator)
//!
//! `vector_indices` (schema field order):
//!   - `id` TEXT PK, `characterId` TEXT — UUID strings.
//!   - `version` — `z.number()` (NO bounds) → **REAL** affinity. v4's
//!     `mapToSQLiteType` only emits INTEGER when both `min` AND `max` are integer;
//!     a bare `z.number()` is REAL. Bound `f64` (`1.0` on create); an
//!     integer-valued REAL renders back as `1` in the dump via
//!     [`super::js_number_to_json`].
//!   - `dimensions` — `z.number()` (NO bounds) → **REAL** affinity, same as
//!     `version`. Bound `f64`.
//!   - `createdAt` / `updatedAt` TEXT — ISO timestamps.
//!
//! `vector_entries` (schema field order):
//!   - `id` TEXT PK, `characterId` TEXT — UUID strings.
//!   - `embedding` — a `z.union([...])` of mixed option types → v4's
//!     `getBaseType` yields `unknown` → DDL TEXT affinity, but the registered BLOB
//!     column makes the marshaling store a real **BLOB value** (SQLite affinity is
//!     dynamic: a TEXT-declared column holds the BLOB unchanged). `None` / empty →
//!     SQL NULL; non-empty → little-endian Float32 bytes via
//!     [`crate::embedding_blob::float32_to_blob`] (mirrors v4's `documentToRow`
//!     blob path: empty array or null → NULL, never a zero-length blob). The dump
//!     hexes the BLOB on both sides for a bit-exact compare.
//!   - `createdAt` TEXT — ISO timestamp.
//!
//! ## Minting / determinism
//!
//! `saveMeta` upserts by `characterId`: on create it inserts `id = characterId`,
//! `version = 1`, `dimensions`, `createdAt = updatedAt = now`; on update it sets
//! `dimensions` + `updatedAt = now`. `addEntry` / `addEntries` mint `createdAt =
//! now` (a single shared `now` across an `addEntries` batch). Entry `id`s are
//! supplied by the caller (NOT minted here — v4's `addEntry`/`addEntries` take the
//! id), but `vector_entries.id` is still nondeterministic from the harness's point
//! of view because the corpus mints them; so the tier-2 differential uses the
//! **minted-values remap form** (placeholder timestamps; remap entry ids to
//! first-seen tokens in natural-key order). `vector_indices.id == characterId` is
//! pinned (it is the input character id, not generated).

use rusqlite::{params, Connection};

use super::DbError;
use crate::embedding_blob::float32_to_blob;

/// A `vector_indices` metadata row (v4 `VectorIndexMeta`). `version` / `dimensions`
/// are REAL-affinity numbers surfaced as `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorIndexMeta {
    pub id: String,
    pub character_id: String,
    pub version: f64,
    pub dimensions: f64,
    pub created_at: String,
    pub updated_at: String,
}

/// A new `vector_entries` row (v4's `addEntry`/`addEntries` input — id +
/// characterId + embedding; `createdAt` is minted by the repo). `embedding`
/// `None`/empty → SQL NULL, non-empty → little-endian Float32 bytes.
pub struct VectorEntryInput {
    pub id: String,
    pub character_id: String,
    /// `None` or empty → SQL NULL; non-empty → little-endian Float32 bytes.
    pub embedding: Option<Vec<f32>>,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct VectorIndicesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> VectorIndicesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    // ======================================================================
    // Meta operations (vector_indices table)
    // ======================================================================

    /// Find the metadata row for a character's vector index (v4
    /// `findMetaByCharacterId`). Returns `None` when absent.
    pub fn find_meta_by_character_id(
        &self,
        character_id: &str,
    ) -> Result<Option<VectorIndexMeta>, DbError> {
        self.conn
            .query_row(
                "SELECT id, characterId, version, dimensions, createdAt, updatedAt \
                 FROM vector_indices WHERE characterId = ?1",
                params![character_id],
                |row| {
                    Ok(VectorIndexMeta {
                        id: row.get(0)?,
                        character_id: row.get(1)?,
                        version: row.get(2)?,
                        dimensions: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// Upsert the metadata row for a character (v4 `saveMeta`). If a row already
    /// exists for `characterId`, set `dimensions` + `updatedAt = now`; otherwise
    /// insert a fresh row with `id = characterId`, `version = 1`, the given
    /// `dimensions`, and `createdAt = updatedAt = now`.
    pub fn save_meta(&self, character_id: &str, dimensions: f64) -> Result<(), DbError> {
        let now = crate::clock::now_iso();
        let existing = self.find_meta_by_character_id(character_id)?;

        if let Some(existing) = existing {
            self.conn.execute(
                "UPDATE vector_indices SET dimensions = ?1, updatedAt = ?2 WHERE id = ?3",
                params![dimensions, now, existing.id],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO vector_indices \
                   (id, characterId, version, dimensions, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![character_id, character_id, 1.0_f64, dimensions, now, now],
            )?;
        }
        Ok(())
    }

    /// Delete the metadata row(s) for a character (v4 `deleteMetaByCharacterId`).
    /// Returns `true` when at least one row was removed.
    pub fn delete_meta_by_character_id(&self, character_id: &str) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM vector_indices WHERE characterId = ?1",
            params![character_id],
        )?;
        Ok(affected > 0)
    }

    /// All character ids that have a vector index (v4 `getAllCharacterIds`).
    /// Reads every metadata row's `characterId`.
    pub fn get_all_character_ids(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT characterId FROM vector_indices")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    // ======================================================================
    // Entry operations (vector_entries table)
    // ======================================================================

    /// All entries for a character (v4 `findEntriesByCharacterId`). The embedding
    /// is returned as a `Vec<f32>` (decoded from the BLOB; SQL NULL → empty vec).
    pub fn find_entries_by_character_id(
        &self,
        character_id: &str,
    ) -> Result<Vec<VectorEntryRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, characterId, embedding, createdAt \
             FROM vector_entries WHERE characterId = ?1",
        )?;
        let rows = stmt
            .query_map(params![character_id], |row| {
                let blob: Option<Vec<u8>> = row.get(2)?;
                Ok(VectorEntryRow {
                    id: row.get(0)?,
                    character_id: row.get(1)?,
                    embedding: blob
                        .map(|b| crate::embedding_blob::blob_to_float32(&b))
                        .unwrap_or_default(),
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Add a single entry (v4 `addEntry`). Mints `createdAt = now`. `None`/empty
    /// embedding → SQL NULL, non-empty → little-endian Float32 bytes.
    pub fn add_entry(&self, entry: &VectorEntryInput) -> Result<(), DbError> {
        let now = crate::clock::now_iso();
        self.insert_entry(entry, &now)
    }

    /// Add multiple entries in a batch (v4 `addEntries`). Empty input is a no-op.
    /// A SINGLE `now` is minted and shared as `createdAt` across the whole batch
    /// (v4 computes `const now = new Date().toISOString()` once before the map).
    pub fn add_entries(&self, entries: &[VectorEntryInput]) -> Result<(), DbError> {
        if entries.is_empty() {
            return Ok(());
        }
        let now = crate::clock::now_iso();
        for entry in entries {
            self.insert_entry(entry, &now)?;
        }
        Ok(())
    }

    /// Shared INSERT for `add_entry` / `add_entries` with a caller-supplied
    /// `created_at` (so a batch shares one timestamp).
    fn insert_entry(&self, entry: &VectorEntryInput, created_at: &str) -> Result<(), DbError> {
        let embedding_blob: Option<Vec<u8>> = match &entry.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        self.conn.execute(
            "INSERT INTO vector_entries (id, characterId, embedding, createdAt) \
             VALUES (?1, ?2, ?3, ?4)",
            params![entry.id, entry.character_id, embedding_blob, created_at],
        )?;
        Ok(())
    }

    /// Update only the embedding of an entry (v4 `updateEntryEmbedding`). Returns
    /// `true` when a row matched (v4's `modifiedCount > 0`). `None`/empty → SQL
    /// NULL, non-empty → little-endian Float32 bytes. No timestamp is touched
    /// (v4's `$set` names only `embedding`).
    pub fn update_entry_embedding(
        &self,
        id: &str,
        embedding: Option<&[f32]>,
    ) -> Result<bool, DbError> {
        let embedding_blob: Option<Vec<u8>> = match embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        let affected = self.conn.execute(
            "UPDATE vector_entries SET embedding = ?1 WHERE id = ?2",
            params![embedding_blob, id],
        )?;
        Ok(affected > 0)
    }

    /// Remove a single entry by id (v4 `removeEntry`). Returns `true` when a row
    /// was removed.
    pub fn remove_entry(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM vector_entries WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Remove multiple entries by id (v4 `removeEntries`). Empty input → 0.
    /// Reproduces v4's per-id loop EXACTLY: one `deleteOne` per id, summing the
    /// per-delete counts. (A duplicate id in the list therefore counts once — the
    /// second delete of the same id matches nothing — matching v4's `deleteOne`
    /// loop, not a single `IN (...)` delete.)
    pub fn remove_entries(&self, ids: &[String]) -> Result<usize, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut removed = 0_usize;
        for id in ids {
            let affected = self
                .conn
                .execute("DELETE FROM vector_entries WHERE id = ?1", params![id])?;
            removed += affected;
        }
        Ok(removed)
    }

    /// Remove all entries for a character (v4 `removeEntriesByCharacterId`).
    /// Returns the number removed.
    pub fn remove_entries_by_character_id(&self, character_id: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM vector_entries WHERE characterId = ?1",
            params![character_id],
        )?;
        Ok(affected)
    }

    /// Whether an entry with this id exists (v4 `entryExists`).
    pub fn entry_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM vector_entries WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }

    // ======================================================================
    // Combined operations
    // ======================================================================

    /// Delete a character's vector index entirely (v4 `deleteByCharacterId`):
    /// remove all entries, then the metadata row. v4 does these as **two
    /// independent operations** (NOT a single SQL transaction — the writable open's
    /// per-op autocommit applies to each), so this mirrors that exactly. Returns
    /// `metaDeleted || entriesRemoved > 0`.
    pub fn delete_by_character_id(&self, character_id: &str) -> Result<bool, DbError> {
        let entries_removed = self.remove_entries_by_character_id(character_id)?;
        let meta_deleted = self.delete_meta_by_character_id(character_id)?;
        Ok(meta_deleted || entries_removed > 0)
    }
}

/// A hydrated `vector_entries` row (v4 `VectorEntryRow`). The BLOB is decoded to a
/// `Vec<f32>` (SQL NULL → empty vec).
#[derive(Debug, Clone, PartialEq)]
pub struct VectorEntryRow {
    pub id: String,
    pub character_id: String,
    pub embedding: Vec<f32>,
    pub created_at: String,
}
