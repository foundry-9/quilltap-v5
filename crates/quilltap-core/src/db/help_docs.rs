//! The help-docs repository — a Phase-2 repo port, after `folders`, `tags`,
//! `text_replacement_rules`, `prompt_templates`, `conversation_annotations`,
//! and `provider_models`. Ports v4's
//! `lib/database/repositories/help-docs.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, `delete`, and `upsert_by_path` (v4's
//! `upsertByPath` — find-by-path then text-only update / minted create, verified
//! in the minted-values remap form). The embedding-only updates and the
//! `clearAll*` / `findAll*` reads remain out of scope.
//!
//! ## The first tier-2 BLOB column (the headline)
//!
//! `help_docs.embedding` is the first **BLOB column** to land in the tier-2
//! differential. v4 stores an embedding as a raw **Float32 byte buffer** —
//! little-endian `f32` bytes — via `documentToRow`'s blob path
//! (`embeddingToBlob`), which the help-docs repo wires up by auto-registering
//! the `embedding` blob column on first `getCollection()`. Two subtleties the
//! port reproduces exactly:
//!
//!   - **empty / null → SQL NULL.** v4 stores an empty embedding array (and a
//!     null) as NULL, never as a zero-length blob. Here `None` *or* an empty
//!     `Vec<f32>` binds SQL NULL; only a non-empty vector is serialized.
//!   - **bit-exact comparison.** The canonical dump emits BLOBs as lowercase
//!     hex on BOTH sides (Rust `cell_to_json` → `hex::encode`, the TS
//!     `canonValue` → `Buffer.toString('hex')`), so a deterministic Float32
//!     buffer compares byte-for-byte. The fixture uses only
//!     exactly-float32-representable values (`0.5`, `-0.25`, …) so the f64→f32
//!     cast is lossless and identical on both sides.
//!
//! The Float32→bytes conversion is [`crate::embedding_blob::float32_to_blob`]
//! (the same little-endian encoder the embedding layer uses).
//!
//! ## ⚠ v4's blob-column REGISTRATION bug — which this port cannot have (P4.d6)
//!
//! The "auto-registering on first `getCollection()`" described above is v4
//! machinery, and v4 `6c59b1ca` found it was **broken for this very table**:
//! `manager.ts` registers the known blob columns when it builds a backend
//! "regardless of which repository is accessed first", but `help_docs` was NOT
//! on that list. It alone relied on `HelpDocsRepository` registering lazily and
//! caching `blobColumnsRegistered` **on the instance** — and a repository
//! OUTLIVES the backend it first ran against (a reconnect, a dev-server
//! reload), so the stale flag left the fresh backend with no blob handling.
//! Both directions then broke silently: `documentToRow` skipped
//! `embeddingToBlob` and `JSON.stringify(Float32Array)` persisted an
//! index-keyed object (`{"0":…}`) as TEXT; `hydrateRow` skipped
//! `parseLegacyEmbeddingText` for that same unregistered column, so the row
//! failed Zod validation and was **dropped from `findAll()`**. A sync rewrote
//! 28 rows and all 28 vanished from `/api/v1/help-docs`. v4's fix: one line in
//! `manager.ts`, plus dropping the instance flag so registration is re-asserted
//! on every `getCollection()` (it is keyed to the BACKEND, so it must be
//! re-asserted, not remembered).
//!
//! **The insight worth keeping: the "legacy" JSON-text embeddings were never
//! legacy — an unregistered blob column was MINTING them on every write.**
//!
//! **None of this is portable, because the bug's mechanism does not exist
//! here.** `grep -rn "register_blob\|blob_columns\|BLOB_COLUMNS" crates/
//! --include='*.rs'` returns ZERO hits: the port abandoned v4's generic
//! document-mapper architecture, so there is no `documentToRow`, no
//! `hydrateRow`, no runtime registry to forget to populate, no cached instance
//! flag, and no `JSON.stringify` fallback a Float32 vector could fall into.
//! Every repository writes typed, explicit SQL and calls
//! [`crate::embedding_blob::float32_to_blob`] / `blob_to_float32` directly at
//! the binding site (here at `create`, and in `memories`,
//! `conversation_chunks`, `vector_indices`, `doc_mount_chunks`);
//! `embedding_blob` is the single source of truth. **Do not port
//! `registerBlobColumns`, and do not add a registration mechanism in order to
//! have something to fix** — that would import the bug and then patch it.
//!
//! v5 correspondingly needs no `repair-text-embeddings` port (v4's every-boot
//! repair over the TEXT shape): it cannot mint that shape.
//! [`crate::embedding_blob::parse_legacy_embedding_text`] IS ported and MUST
//! stay — it is read-side recovery for genuinely old v4 rows, covered by
//! `legacy_embedding_equivalence`. That is the correct residue.
//!
//! ## Text-only update preserves the embedding (banked behavior)
//!
//! v4's `_update` rewrites the *whole* row from the hydrated existing entity
//! merged with the patch, so an update that touches only text fields (e.g.
//! `content` + `contentHash`) re-persists the existing embedding unchanged (the
//! BLOB round-trips losslessly through hydrate→re-store). This port models the
//! patch as a partial `UPDATE SET` over only the provided text columns and
//! `updatedAt`, so it simply never names the `embedding` column — leaving the
//! stored BLOB untouched. The corpus exercises this directly: a content +
//! contentHash update on a row that HAS an embedding, asserted via the dump to
//! still show the original embedding hex.
//!
//! Determinism: the tier-2 case pins the id and timestamps, so the persisted
//! rows match v4's byte-for-byte with no normalization — the form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates` use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;
use crate::embedding_blob::{blob_to_float32, float32_to_blob};

/// Fields for creating a help doc (the `Omit<HelpDoc,'id'|timestamps>` shape).
/// `embedding` is the BLOB column: `None` or an empty vector → SQL NULL, a
/// non-empty vector → little-endian Float32 bytes.
pub struct HdCreate {
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub content_hash: String,
    pub embedding: Option<Vec<f32>>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A help-doc update patch. Mirrors v4 `update` over `_update` for the
/// text-only path: provided fields overwrite, id and createdAt are preserved,
/// `updatedAt` is set explicitly. It deliberately has **no embedding field** —
/// a text-only update never touches the BLOB column, matching v4's whole-row
/// rewrite that re-persists the existing embedding unchanged (see the module
/// header).
#[derive(Default)]
pub struct HdUpdate {
    pub title: Option<String>,
    pub url: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub updated_at: String,
}

/// Input to [`HelpDocsRepository::upsert_by_path`] — v4's
/// `Omit<HelpDoc,'id'|'createdAt'|'updatedAt'|'embedding'>`. There is
/// deliberately **no `embedding` field**: an upsert that hits the create branch
/// stores a NULL embedding, and one that hits the update branch patches only the
/// four text columns, leaving any existing embedding BLOB untouched.
pub struct HdUpsert {
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub content_hash: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct HelpDocsRepository<'c> {
    conn: &'c Connection,
}

/// A help doc without its embedding — the [`HelpDocsRepository::find_all`] read
/// shape, consumed by the help-search keyword fallback / listing and by the
/// disk sync's read-once path index.
///
/// v4's `findAll()` returns the whole `HelpDoc` entity; this is that row minus
/// the `embedding` BLOB and the timestamps, which no consumer of `find_all`
/// reads. `content_hash` IS carried: v4's `syncHelpDocs` indexes `findAll()` by
/// path and compares `existing.contentHash` to decide unchanged-vs-updated
/// (`help-doc-sync.ts:188`), so a projection without it could not serve the
/// sync's one read.
#[derive(Debug, Clone)]
pub struct HelpDocRow {
    pub id: String,
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub content_hash: String,
}

/// A help doc carrying its decoded embedding (v4 `HelpDocumentWithEmbedding`) —
/// the [`HelpDocsRepository::find_all_with_embeddings`] read shape, consumed by
/// the help-search semantic path. A NULL/empty stored embedding yields an empty
/// vector (the search skips those, matching v4's `length === 0` guard).
#[derive(Debug, Clone)]
pub struct HelpDocEmbeddedRow {
    pub id: String,
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

impl<'c> HelpDocsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `findAll` — every help doc (no scoping, rowid/insertion order), sans
    /// embedding. Read-only.
    pub fn find_all(&self) -> Result<Vec<HelpDocRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, path, url, content, contentHash FROM help_docs")?;
        let rows = stmt.query_map([], |r| {
            Ok(HelpDocRow {
                id: r.get(0)?,
                title: r.get(1)?,
                path: r.get(2)?,
                url: r.get(3)?,
                content: r.get(4)?,
                content_hash: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// v4 `findById` (P4.6BL), sans embedding (the handler embeds
    /// `` `${title}\n\n${content}` `` and never reads the stored vector). `None`
    /// when no row matches.
    pub fn find_by_id(&self, id: &str) -> Result<Option<HelpDocRow>, DbError> {
        self.conn
            .query_row(
                "SELECT id, title, path, url, content, contentHash \
                 FROM help_docs WHERE id = ?1",
                params![id],
                |r| {
                    Ok(HelpDocRow {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        path: r.get(2)?,
                        url: r.get(3)?,
                        content: r.get(4)?,
                        content_hash: r.get(5)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `updateEmbedding` (P4.6BL) — set just the `embedding` BLOB on a help
    /// doc (+ the minted `updatedAt`, injected as `now_iso`). This is the
    /// dedicated method the module doc reserves the embedding write for —
    /// `HdUpdate`/`HdUpsert` deliberately carry no embedding field. The vector
    /// serializes as the create path does (`empty → SQL NULL`, else Float32 LE
    /// bytes). Returns `Ok(false)` when no row matched — v4 THROWS
    /// `` `Help doc not found for embedding update: ${id}` `` there; the
    /// EMBEDDING_GENERATE handler (its only caller) formats that exact string.
    pub fn update_embedding(
        &self,
        id: &str,
        embedding: &[f32],
        now_iso: &str,
    ) -> Result<bool, DbError> {
        let embedding_blob: Option<Vec<u8>> = if embedding.is_empty() {
            None
        } else {
            Some(crate::embedding_blob::float32_to_blob(embedding))
        };
        let affected = self.conn.execute(
            "UPDATE help_docs SET embedding = ?1, updatedAt = ?2 WHERE id = ?3",
            params![embedding_blob, now_iso, id],
        )?;
        Ok(affected > 0)
    }

    /// v4 `findAllNeedingEmbedding` — every help doc with no stored embedding
    /// (rowid/insertion order), the enqueue's work list. Read-only.
    ///
    /// v4 filters the hydrated rows in JS (`_findAll().filter(doc => doc.embedding
    /// == null)`); `WHERE embedding IS NULL` is exactly equivalent and does not
    /// hydrate 28 vectors to throw them away. The equivalence holds on both edges:
    /// v4 stores an empty embedding as SQL NULL (never a zero-length BLOB), and a
    /// zero-length BLOB would hydrate to `Float32Array(0)` — which is NOT
    /// `== null`, and is likewise not `IS NULL`. A legacy TEXT embedding hydrates
    /// to a vector (not null) and is not `IS NULL` either.
    pub fn find_all_needing_embedding(&self) -> Result<Vec<HelpDocRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, path, url, content, contentHash FROM help_docs \
             WHERE embedding IS NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(HelpDocRow {
                id: r.get(0)?,
                title: r.get(1)?,
                path: r.get(2)?,
                url: r.get(3)?,
                content: r.get(4)?,
                content_hash: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// v4 `findAllWithEmbeddings` — every help doc with its decoded embedding
    /// (rowid/insertion order). A NULL embedding decodes to an empty vector.
    /// Read-only.
    pub fn find_all_with_embeddings(&self) -> Result<Vec<HelpDocEmbeddedRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, path, url, content, embedding FROM help_docs")?;
        let rows = stmt.query_map([], |r| {
            let blob: Option<Vec<u8>> = r.get(5)?;
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                blob,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, title, path, url, content, blob) = r?;
            let embedding = blob.as_deref().map(blob_to_float32).unwrap_or_default();
            out.push(HelpDocEmbeddedRow {
                id,
                title,
                path,
                url,
                content,
                embedding,
            });
        }
        Ok(out)
    }

    /// Insert a help doc with the given pinned id + timestamps. The embedding is
    /// serialized to a little-endian Float32 BLOB; `None` or an empty vector
    /// binds SQL NULL (v4's empty→NULL rule in `documentToRow`).
    pub fn create(&self, data: &HdCreate, opts: &CreateOptions) -> Result<(), DbError> {
        // empty / null embedding -> SQL NULL; non-empty -> Float32 LE bytes.
        let embedding_blob: Option<Vec<u8>> = match &data.embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };

        self.conn.execute(
            "INSERT INTO help_docs \
               (id, title, path, url, content, contentHash, embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                opts.id,
                data.title,
                data.path,
                data.url,
                data.content,
                data.content_hash,
                embedding_blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply a text-only update patch to the help doc `id`. Returns `Ok(false)`
    /// when no row matched (v4's "not found -> null"). id, createdAt, and the
    /// `embedding` BLOB are never touched.
    pub fn update(&self, id: &str, patch: &HdUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`: the row must exist or the update is a
        // no-op (-> null). Mirror that so a missing target yields Ok(false)
        // rather than relying on the UPDATE affecting zero rows (which would be
        // ambiguous when the patch is text-only).
        if !self.exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(title) = &patch.title {
            assignments.push(format!("title = ?{}", values.len() + 1));
            values.push(Box::new(title.clone()));
        }
        if let Some(url) = &patch.url {
            assignments.push(format!("url = ?{}", values.len() + 1));
            values.push(Box::new(url.clone()));
        }
        if let Some(content) = &patch.content {
            assignments.push(format!("content = ?{}", values.len() + 1));
            values.push(Box::new(content.clone()));
        }
        if let Some(content_hash) = &patch.content_hash {
            assignments.push(format!("contentHash = ?{}", values.len() + 1));
            values.push(Box::new(content_hash.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE help_docs SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Insert or update a help doc keyed by its `path` (v4's `upsertByPath`).
    ///
    /// If a row with `path` already exists, patches ONLY the four text columns
    /// (`title`, `url`, `content`, `contentHash`) plus `updatedAt` — the
    /// `embedding` BLOB is never named, so it survives untouched, matching v4's
    /// whole-row rewrite that re-persists the existing embedding. Otherwise it
    /// creates a fresh row with a minted id + timestamps and a NULL embedding
    /// (v4's `_create` over the embedding-less `data`).
    ///
    /// Mints `id` (`uuid::Uuid::new_v4`) and `now` ([`crate::clock::now_iso`])
    /// just like the create/remap path, so the resulting row carries
    /// nondeterministic id + timestamps (verified by the harness via remap +
    /// timestamp-placeholder normalization). Returns the id of the affected row.
    pub fn upsert_by_path(&self, data: &HdUpsert) -> Result<String, DbError> {
        let now = now_iso();

        if let Some(existing_id) = self.find_id_by_path(&data.path)? {
            // Existing row -> text-only update. The embedding column is NOT in
            // the patch, so the stored BLOB is left intact.
            self.update(
                &existing_id,
                &HdUpdate {
                    title: Some(data.title.clone()),
                    url: Some(data.url.clone()),
                    content: Some(data.content.clone()),
                    content_hash: Some(data.content_hash.clone()),
                    updated_at: now,
                },
            )?;
            return Ok(existing_id);
        }

        // No existing row -> create with a minted id + timestamps and (since
        // `HdUpsert` carries no embedding) a NULL embedding.
        let id = uuid::Uuid::new_v4().to_string();
        self.create(
            &HdCreate {
                title: data.title.clone(),
                path: data.path.clone(),
                url: data.url.clone(),
                content: data.content.clone(),
                content_hash: data.content_hash.clone(),
                embedding: None,
            },
            &CreateOptions {
                id: id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        Ok(id)
    }

    /// The id of the row whose `path` matches, or `None` (v4's `findByPath`
    /// non-null check; reads only the key column).
    fn find_id_by_path(&self, path: &str) -> Result<Option<String>, DbError> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM help_docs WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(id)
    }

    /// Delete the help doc `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM help_docs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // === P4.6BM ===

    /// Every help doc as `(id, embedding_dim)` — the projection
    /// `EMBEDDING_REINDEX_ALL` needs for v4's `embeddingMatchesDim`, which only
    /// ever reads `embedding.length`. A NULL or empty BLOB yields `0`, which can
    /// never equal a positive target dim — matching v4's "treat null/empty as a
    /// mismatch so they get re-embedded". Insertion order, like `find_all`.
    ///
    /// ⚠ The length is DECODED through [`crate::embedding_blob::blob_to_float32`],
    /// never derived from `LENGTH(embedding)`. Current writes are int8-quantized
    /// (`11 + dim` bytes), so byte arithmetic silently under-reports — an
    /// 8-component vector is 19 bytes, and `19 / 4` is 4.
    pub fn find_all_with_embedding_dims(&self) -> Result<Vec<(String, usize)>, DbError> {
        let mut stmt = self.conn.prepare("SELECT id, embedding FROM help_docs")?;
        let rows = stmt.query_map([], |r| {
            let blob: Option<Vec<u8>> = r.get(1)?;
            let dim = blob
                .as_deref()
                .map(|b| blob_to_float32(b).len())
                .unwrap_or(0);
            Ok((r.get::<_, String>(0)?, dim))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// v4 `clearAllEmbeddings` — NULL every help-doc `embedding` and stamp
    /// `updatedAt`, returning the modified count. `EMBEDDING_REINDEX_ALL`'s full
    /// scope calls it so the re-embedding pass writes into a known-empty column.
    ///
    /// v4's `updateMany({}, {$set: …})` touches EVERY row, so its `modifiedCount`
    /// is the whole table — an unconditional `UPDATE` here, deliberately WITHOUT
    /// an `embedding IS NOT NULL` guard (which would both under-count and skip
    /// the `updatedAt` bump on already-empty rows).
    pub fn clear_all_embeddings(&self, now_iso: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "UPDATE help_docs SET embedding = NULL, updatedAt = ?1",
            params![now_iso],
        )?;
        Ok(affected)
    }

    // === end P4.6BM ===

    /// True iff a row with `id` exists — v4's `findById` non-null check, reading
    /// nothing but the key.
    fn exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM help_docs WHERE id = ?1",
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
