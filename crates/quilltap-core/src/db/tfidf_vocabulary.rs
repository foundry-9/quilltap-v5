//! The tfidf-vocabulary repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`,
//! `conversation_annotations`, `provider_models`, `help_docs`,
//! `roleplay_templates`, `image_profiles`, and `connection_profiles`. Ports v4's
//! `lib/database/repositories/tfidf-vocabulary.repository.ts`.
//!
//! Scope: `create`, `update`, and `delete`. The custom helpers —
//! `findByProfileId`/`findByUserId`/`upsertByProfileId`/`deleteByProfileId` — are
//! out of scope. `tfidf_vocabulary` stores the fitted TF-IDF vocabulary, IDF
//! weights, and statistics for each BUILTIN embedding profile.
//!
//! ## ⚠️ This repo OVERRIDES the base `create`/`update` — `updatedAt` is minted,
//! not pinnable
//!
//! Unlike `folders`/`tags`/`image_profiles` (which delegate to the base repo's
//! `_create`/`_update`, both of which HONOR an injected `updatedAt`), v4's
//! `TfidfVocabularyRepository` **overrides** `create` and `update` with bespoke
//! bodies that set `updatedAt: this.getCurrentTimestamp()` UNCONDITIONALLY:
//!
//!   - `create`: `id = options?.id || generateId()`, `createdAt =
//!     options?.createdAt || now`, but `updatedAt = now` — `options.updatedAt` is
//!     **ignored**.
//!   - `update`: strips `id`/`createdAt` off the patch, then `$set: { ...patch,
//!     updatedAt: now }` — any `updatedAt` in the patch is **overwritten** with
//!     `now`.
//!
//! So this port mints `updatedAt` itself via [`crate::clock::now_iso`] (v4's
//! `new Date().toISOString()`), exactly as v4 does. `id` + `createdAt` are still
//! pinnable (create honors both), so the tier-2 differential pins those and
//! **placeholder-normalizes only `updatedAt`** — the minted-timestamp form first
//! established by `folders_remap`, but here applied to a single column with ids +
//! createdAt left exact. (Pinning `updatedAt` is simply not a behavior this repo
//! offers; reproducing the override faithfully is the point.)
//!
//! ## Marshaling surface this repo banks
//!
//!   - **plain-string columns that hold JSON text**: `vocabulary` (`z.string()`,
//!     the `[[term, index], ...]` payload) and `idf` (`z.string()`, the `number[]`
//!     payload) are ordinary TEXT columns. v4 stores the value the caller already
//!     serialized — it does **not** re-stringify a `z.string()`. They bind a Rust
//!     `String` **as-is**; running them through `serde_json::to_string` would
//!     double-encode and diverge from v4 byte-for-byte.
//!   - **two REAL number columns** (`avgDocLength`, `vocabularySize`).
//!     `avgDocLength` is a bare `z.number()` (no min/max → REAL); `vocabularySize`
//!     is `z.number().int().positive()` — a min only, no max, so still REAL by
//!     v4's `mapToSQLiteType` (INTEGER affinity requires BOTH an integer min AND
//!     max). Both bind `f64`. An integer-valued REAL renders back as a JSON
//!     integer in the canonical dump via [`super::js_number_to_json`]; a
//!     fractional `avgDocLength` renders as a float — matching v4's
//!     better-sqlite3 → `JSON.stringify` path.
//!   - a **boolean column** (`includeBigrams`, `z.boolean().default(true)`) →
//!     INTEGER 0/1 (`i64::from(bool)`). The corpus sets it explicitly on every
//!     create to avoid relying on the Zod default.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock;

/// Fields for creating a TF-IDF vocabulary record (the `Omit<TfidfVocabulary,
/// 'id'|timestamps>` shape). `vocabulary`/`idf` are PLAIN strings (JSON text the
/// caller already serialized — bound as-is, never re-stringified);
/// `avg_doc_length`/`vocabulary_size` are the REAL number columns;
/// `include_bigrams` lands as INTEGER 0/1. `fitted_at` is a timestamp TEXT column
/// (an ordinary field — not minted, the caller supplies it).
pub struct TvCreate {
    pub profile_id: String,
    pub user_id: String,
    /// Plain JSON-text payload (`[["term",0], ...]`), stored as-is — NOT
    /// re-serialized.
    pub vocabulary: String,
    /// Plain JSON-text payload (`[0.5,1.2]`), stored as-is — NOT re-serialized.
    pub idf: String,
    /// Bare `z.number()` → REAL. Integer-valued collapses in the dump; a
    /// fractional value dumps as a float.
    pub avg_doc_length: f64,
    /// `z.number().int().positive()` (min only) → REAL. Integer-valued.
    pub vocabulary_size: f64,
    pub include_bigrams: bool,
    pub fitted_at: String,
}

/// Pinned id + createdAt (v4's `CreateOptions`). NOTE: this repo's `create`
/// IGNORES `options.updatedAt` (it always mints `updatedAt = now`), so there is
/// no `updated_at` field here — mirroring the override.
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
}

/// A TF-IDF vocabulary update patch. Mirrors v4 `update`: provided fields
/// overwrite, id and createdAt are preserved, and `updatedAt` is set to `now`
/// (minted internally — NOT a field here, because v4 overwrites any caller value).
#[derive(Default)]
pub struct TvUpdate {
    pub profile_id: Option<String>,
    pub user_id: Option<String>,
    /// Plain JSON-text payload, stored as-is when provided.
    pub vocabulary: Option<String>,
    /// Plain JSON-text payload, stored as-is when provided.
    pub idf: Option<String>,
    pub avg_doc_length: Option<f64>,
    pub vocabulary_size: Option<f64>,
    pub include_bigrams: Option<bool>,
    pub fitted_at: Option<String>,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct TfidfVocabularyRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TfidfVocabularyRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a TF-IDF vocabulary record. `id` + `createdAt` come from `opts`;
    /// `updatedAt` is MINTED here (`clock::now_iso`), exactly as v4's override
    /// (`updatedAt = now`, ignoring `options.updatedAt`). `vocabulary`/`idf` bind
    /// their plain `String` as-is; the REAL columns bind `f64`; `includeBigrams`
    /// binds `i64::from(bool)`.
    pub fn create(&self, data: &TvCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let now = clock::now_iso();
        self.conn.execute(
            "INSERT INTO tfidf_vocabularies \
               (id, profileId, userId, vocabulary, idf, avgDocLength, vocabularySize, \
                includeBigrams, fittedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                opts.id,
                data.profile_id,
                data.user_id,
                data.vocabulary,
                data.idf,
                data.avg_doc_length,
                data.vocabulary_size,
                i64::from(data.include_bigrams),
                data.fitted_at,
                opts.created_at,
                now,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the vocabulary `id`. Returns `Ok(false)` when no
    /// row matched (v4's "modifiedCount === 0 -> null"). id and createdAt are
    /// never touched. Each `Some` field sets that column; `updatedAt` is always
    /// set to a freshly minted `now` (v4 overwrites any caller-supplied value).
    pub fn update(&self, id: &str, patch: &TvUpdate) -> Result<bool, DbError> {
        // v4's bespoke update issues an `updateOne({ id }, ...)`; a missing row
        // yields `modifiedCount === 0 -> null` (a no-op).
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let now = clock::now_iso();
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(profile_id) = &patch.profile_id {
            assignments.push(format!("profileId = ?{}", values.len() + 1));
            values.push(Box::new(profile_id.clone()));
        }
        if let Some(user_id) = &patch.user_id {
            assignments.push(format!("userId = ?{}", values.len() + 1));
            values.push(Box::new(user_id.clone()));
        }
        if let Some(vocabulary) = &patch.vocabulary {
            assignments.push(format!("vocabulary = ?{}", values.len() + 1));
            values.push(Box::new(vocabulary.clone()));
        }
        if let Some(idf) = &patch.idf {
            assignments.push(format!("idf = ?{}", values.len() + 1));
            values.push(Box::new(idf.clone()));
        }
        if let Some(avg_doc_length) = patch.avg_doc_length {
            assignments.push(format!("avgDocLength = ?{}", values.len() + 1));
            values.push(Box::new(avg_doc_length));
        }
        if let Some(vocabulary_size) = patch.vocabulary_size {
            assignments.push(format!("vocabularySize = ?{}", values.len() + 1));
            values.push(Box::new(vocabulary_size));
        }
        if let Some(include_bigrams) = patch.include_bigrams {
            assignments.push(format!("includeBigrams = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(include_bigrams)));
        }
        if let Some(fitted_at) = &patch.fitted_at {
            assignments.push(format!("fittedAt = ?{}", values.len() + 1));
            values.push(Box::new(fitted_at.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(now));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE tfidf_vocabularies SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the vocabulary `id`. Returns `Ok(false)` when no row matched (v4's
    /// `deletedCount > 0 -> true` else `false`).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM tfidf_vocabularies WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — the `update` precondition (a missing
    /// target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM tfidf_vocabularies WHERE id = ?1",
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
}
