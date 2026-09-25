//! The help-doc CHUNKS repository — port of v4
//! `lib/database/repositories/help-doc-chunks.repository.ts` (v4 `24633026`,
//! "section-level help embeddings").
//!
//! One row per *section* of a help document. A whole-document vector for a long,
//! topically broad page (`help/chat-settings.md` covers a dozen unrelated
//! subsystems) is a smear that matches any specific question only weakly; a
//! per-section vector is what lets "how do I describe an image for a model that
//! can't see?" land on the paragraph that answers it.
//!
//! ## The two DDL shapes (deliberate, and neither is a mistake)
//!
//! v4 creates this table two different ways, and v5 follows both:
//!
//!   - **A fresh instance** gets the `generateDDL` shape — `chunkIndex REAL`,
//!     `embedding TEXT`, no UNIQUE, no FOREIGN KEY, plus
//!     `idx_help_doc_chunks_createdAt`. That is what the D23 re-dump at
//!     `24633026` captured into `provisioning/fresh_schema.json` (it mirrors
//!     `help_docs` and `conversation_chunks` exactly — SQLite affinity is
//!     lenient, so an integer index and a BLOB both round-trip through those
//!     declared types unchanged).
//!   - **An existing instance** gets the MIGRATION shape (v4
//!     `migrations/scripts/create-help-doc-chunks-table.ts`) — `chunkIndex
//!     INTEGER`, `embedding BLOB`, `UNIQUE(docId, chunkIndex)`, `FOREIGN KEY
//!     (docId) REFERENCES help_docs(id) ON DELETE CASCADE`, plus
//!     `idx_help_doc_chunks_docId`. v5 has no migration runner, so that DDL is
//!     re-homed as a boot ensure ([`super::help_doc_chunks_repair`]), the
//!     P4.D41/P4.D63/P4.D73 precedent.
//!
//! ⚠ **Nothing here may depend on the FK cascade**, because half the instances
//! in the field will not have it (and a read-only open never enables
//! `foreign_keys` at all). v4 ships [`HelpDocChunksRepository::delete_by_doc_id`]
//! belt-and-braces at every prune site and [`delete_orphaned`] as the sweep —
//! this port carries both, and relies on neither alone. That is the P4.D41
//! cascade lesson, applied before it can bite.
//!
//! ## What did NOT port (recorded, not deferred)
//!
//! v4's commit also touches `lib/database/manager.ts`
//! (`registerBlobColumns('help_doc_chunks', ['embedding'])`),
//! `repositories/index.ts`, and `child/child-repositories-proxy.ts`. All three
//! are **Node-writer/document-mapper workarounds with no v5 analog** — the same
//! finding `help_docs.rs`'s header records at length: v5 has no runtime blob
//! registry to forget to populate (every write calls
//! [`crate::embedding_blob::float32_to_blob`] at the binding site), no repository
//! container to register into (repos are constructed over a borrowed
//! connection), and no forked child writer (the single-writer runtime is a
//! type/ownership rule). **Do not add a registration mechanism in order to have
//! something to register.**
//!
//! v4's `prettify.ts` migration label ("Slipping bookmarks between the chapters
//! of the help library") likewise has no v5 analog — v5 surfaces no migration
//! labels anywhere. NO-PORT, the `231be14c` precedent.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::DbError;
use crate::embedding_blob::{blob_to_float32, float32_to_blob};

/// One chunk row as the readers see it (v4 `HelpDocChunk`, embedding hydrated).
/// A NULL / empty stored embedding decodes to an empty vector — every consumer
/// treats that as "not embedded yet", exactly as v4's `embedding.length > 0`
/// guards do.
#[derive(Debug, Clone)]
pub struct HelpDocChunkRow {
    pub id: String,
    pub doc_id: String,
    /// `chunkIndex` — a JS number, carried as `f64`.
    ///
    /// ⚠ It MUST be `f64` and not `i64`. On a fresh-provisioned instance the
    /// column is `generateDDL`'s `REAL`, and SQLite's REAL affinity *forces an
    /// integer into floating-point representation on write* — so the stored
    /// cell is a Real and an `i64` read fails outright with
    /// `InvalidColumnType`. (The boot-ensure shape declares `INTEGER`, where an
    /// `f64` read converts cleanly, so `f64` is the one type that serves both.)
    /// The same convention covers `conversation_chunks.interchangeIndex`.
    /// Caught by `help_doc_ensure_equivalence` on its first run.
    pub chunk_index: f64,
    pub heading: Option<String>,
    pub content: String,
    pub embedding: Vec<f32>,
}

/// One slice ready to be persisted (v4 `HelpDocChunkDraft` — the argument shape
/// of `replaceForDoc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpDocChunkDraft {
    pub chunk_index: i64,
    pub heading: Option<String>,
    pub content: String,
}

/// Pinned id + timestamps (v4's `CreateOptions`), for the differential's
/// deterministic arms.
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`] on the
/// write paths, by a read connection on the search path).
pub struct HelpDocChunksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> HelpDocChunksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `_create` — insert one chunk with the given pinned id + timestamps.
    /// The embedding serializes exactly as `help_docs` does: `None` or an empty
    /// vector binds SQL NULL (v4's empty→NULL rule in `documentToRow`), a
    /// non-empty vector the quantized blob.
    pub fn create(
        &self,
        doc_id: &str,
        draft: &HelpDocChunkDraft,
        embedding: Option<&[f32]>,
        opts: &CreateOptions,
    ) -> Result<(), DbError> {
        let blob: Option<Vec<u8>> = match embedding {
            Some(v) if !v.is_empty() => Some(float32_to_blob(v)),
            _ => None,
        };
        self.conn.execute(
            "INSERT INTO help_doc_chunks \
               (id, docId, chunkIndex, heading, content, embedding, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                opts.id,
                doc_id,
                draft.chunk_index,
                draft.heading,
                draft.content,
                blob,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// v4 `update` over `_update` — patch the mutable text fields of one chunk,
    /// stamping `updatedAt`. Returns `Ok(false)` when no row matched (v4's
    /// "not found → null"). `id`/`docId`/`createdAt` are preserved, and the
    /// `embedding` column is deliberately NOT named — the dedicated
    /// [`Self::update_embedding`] owns it, the same split `help_docs` uses.
    pub fn update(
        &self,
        id: &str,
        heading: Option<&str>,
        content: &str,
        now_iso: &str,
    ) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "UPDATE help_doc_chunks SET heading = ?1, content = ?2, updatedAt = ?3 WHERE id = ?4",
            params![heading, content, now_iso, id],
        )?;
        Ok(affected > 0)
    }

    /// v4 `delete` over `_delete` — `Ok(false)` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM help_doc_chunks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `findByDocId` — every chunk belonging to one help document, in
    /// document order.
    ///
    /// v4 filters in JS then `.sort((a,b) => a.chunkIndex - b.chunkIndex)`;
    /// `ORDER BY chunkIndex` is exactly that, and does not hydrate every blob to
    /// sort them. (JS's numeric sort is total over the integers stored here, so
    /// there is no comparator subtlety to reproduce.)
    pub fn find_by_doc_id(&self, doc_id: &str) -> Result<Vec<HelpDocChunkRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, docId, chunkIndex, heading, content, embedding \
             FROM help_doc_chunks WHERE docId = ?1 ORDER BY chunkIndex",
        )?;
        let rows = stmt.query_map(params![doc_id], row_to_chunk)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// v4 `deleteByDocId` — delete every chunk belonging to one help document,
    /// returning the number removed.
    ///
    /// Called when a doc's content changes (the old slices are meaningless) and
    /// when a doc is pruned, **so the rows never outlive their parent even where
    /// foreign-key cascade is not enforced** — v4's own why-comment, and the
    /// reason this port cannot lean on the FK (see the module header).
    pub fn delete_by_doc_id(&self, doc_id: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM help_doc_chunks WHERE docId = ?1",
            params![doc_id],
        )?;
        Ok(affected)
    }

    /// v4 `replaceForDoc` — replace a document's chunks wholesale, leaving every
    /// embedding NULL for the embedding job to fill. Returns the number written.
    ///
    /// Delete-then-insert rather than a diff (v4's why, carried forward): chunk
    /// boundaries move when the prose above them changes, so matching old rows
    /// to new ones by index would preserve embeddings that no longer describe
    /// their content.
    ///
    /// Ids and timestamps are MINTED per row, exactly as v4's `create` does
    /// (`generateId()` + `getCurrentTimestamp()` per call, so two chunks written
    /// in the same pass can carry different `createdAt` values in principle).
    pub fn replace_for_doc(
        &self,
        doc_id: &str,
        chunks: &[HelpDocChunkDraft],
    ) -> Result<usize, DbError> {
        self.delete_by_doc_id(doc_id)?;

        let mut created = 0usize;
        for chunk in chunks {
            let now = crate::clock::now_iso();
            self.create(
                doc_id,
                chunk,
                None,
                &CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            created += 1;
        }
        Ok(created)
    }

    /// v4 `updateEmbedding` — store the embedding for one chunk (+ the minted
    /// `updatedAt` v4's `_update` stamps). Empty → SQL NULL, as everywhere else.
    ///
    /// v4's no-fallback `safeQuery` logs and RETHROWS (`safe-query.ts:52-58`),
    /// so a DB error here throws into `embedHelpDocChunks`' per-chunk catch and
    /// the chunk is not counted; a no-row-matched update merely returns null
    /// and the caller counts on. This returns the row-matched bool and lets the
    /// job's arm map exactly those two shapes (Err → warn+skip, Ok(_) → count).
    pub fn update_embedding(
        &self,
        id: &str,
        embedding: &[f32],
        now_iso: &str,
    ) -> Result<bool, DbError> {
        let blob: Option<Vec<u8>> = if embedding.is_empty() {
            None
        } else {
            Some(float32_to_blob(embedding))
        };
        let affected = self.conn.execute(
            "UPDATE help_doc_chunks SET embedding = ?1, updatedAt = ?2 WHERE id = ?3",
            params![blob, now_iso, id],
        )?;
        Ok(affected > 0)
    }

    /// v4 `findAllWithEmbeddings` — every chunk that carries a usable embedding,
    /// the corpus help search scores against.
    ///
    /// v4 reads every row and filters `embedding != null && length > 0` in JS.
    /// `WHERE embedding IS NOT NULL` is the same set on both edges (v4 never
    /// stores a zero-length blob — empty binds NULL), and the decoded-length
    /// re-check below covers a legacy row whose stored text decodes to nothing.
    /// Row order is v4's `_findAll` order (rowid / insertion).
    pub fn find_all_with_embeddings(&self) -> Result<Vec<HelpDocChunkRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, docId, chunkIndex, heading, content, embedding \
             FROM help_doc_chunks WHERE embedding IS NOT NULL",
        )?;
        let rows = stmt.query_map([], row_to_chunk)?;
        let mut out = Vec::new();
        for r in rows {
            let row = r?;
            if row.embedding.is_empty() {
                continue;
            }
            out.push(row);
        }
        Ok(out)
    }

    /// v4 `clearAllEmbeddings` — NULL every chunk embedding, leaving the chunk
    /// text in place, and stamp `updatedAt`; returns the modified count.
    ///
    /// Used by a full reindex so the re-embedding pass writes into a known-empty
    /// column. Unconditional, exactly as `help_docs::clear_all_embeddings` is:
    /// v4's `updateMany({}, …)` touches EVERY row, so an `embedding IS NOT NULL`
    /// guard would both under-count and skip the `updatedAt` bump.
    pub fn clear_all_embeddings(&self, now_iso: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "UPDATE help_doc_chunks SET embedding = NULL, updatedAt = ?1",
            params![now_iso],
        )?;
        Ok(affected)
    }

    /// v4 `deleteOrphaned` — remove chunks whose owning document no longer
    /// exists, returning the number removed.
    ///
    /// v4's why, carried forward: foreign-key cascade covers this on SQLite when
    /// `PRAGMA foreign_keys` is on, but the sync path prunes docs through the
    /// repository layer and this gives the sweep something to call regardless.
    /// In v5 the argument is stronger still — a fresh-provisioned instance's
    /// table carries no FK at all (see the module header).
    ///
    /// v4 reads every chunk and deletes one at a time; the set semantics are
    /// identical to the anti-join below, which does not hydrate every blob to
    /// throw it away.
    pub fn delete_orphaned(&self, live_doc_ids: &[String]) -> Result<usize, DbError> {
        // An empty live set means every chunk is orphaned — v4's `Set` lookup
        // says the same, so no special case is needed beyond building the list.
        let mut removed = 0usize;
        let mut stmt = self.conn.prepare("SELECT id, docId FROM help_doc_chunks")?;
        let doomed: Vec<String> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(_, doc_id)| !live_doc_ids.iter().any(|live| live == doc_id))
            .map(|(id, _)| id)
            .collect();
        for id in doomed {
            if self.delete(&id)? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// v4 `count()` (the base-repo method). Its production caller — the P4.D77
    /// upgrade backfill's `count() > 0` gate — retired with v4 `492771aff`
    /// (the startup reconcile reads [`Self::count_by_doc`] instead); kept for
    /// the repository's own tests.
    pub fn count(&self) -> Result<i64, DbError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM help_doc_chunks", [], |r| r.get(0))?;
        Ok(n)
    }

    /// v4 `countByDoc` (`help-doc-chunks.repository.ts:80-101`, new at
    /// `492771aff`) — section and embedded-section counts for every help
    /// document that has any sections, in one GROUP BY. v4's *why*, carried
    /// forward: it reads no vectors — the embedding column is only tested for
    /// NULL — so the startup reconcile can find incomplete docs without
    /// decoding ~700 BLOBs. Docs with no sections are ABSENT from the map.
    ///
    /// ⚠ A zero-length BLOB counts as EMBEDDED here (`IS NOT NULL`), while the
    /// HELP_DOC job treats the same cell as no vector (`length > 0`). Both are
    /// v4's; reproduced, not reconciled.
    ///
    /// **Never answers `Err`:** v4 runs it through a FALLBACK `safeQuery`, so a
    /// failing read logs one ERROR `Error counting help doc chunks by doc`
    /// `{collection, error}` (v4 passes `{}`; the base class enriches every
    /// context with `collection`) and answers an EMPTY map — which
    /// the reconcile then reads as "every doc is section-less" and re-slices.
    pub fn count_by_doc(&self) -> HashMap<String, SectionCounts> {
        match self.count_by_doc_strict() {
            Ok(map) => map,
            Err(err) => {
                tracing::error!(
                    target: "quilltap::db",
                    collection = "help_doc_chunks",
                    error = %err,
                    "Error counting help doc chunks by doc",
                );
                HashMap::new()
            }
        }
    }

    /// [`Self::count_by_doc`]'s query without v4's fallback.
    fn count_by_doc_strict(&self) -> Result<HashMap<String, SectionCounts>, DbError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT "docId" AS docId,
                  COUNT(*) AS total,
                  SUM(CASE WHEN "embedding" IS NOT NULL THEN 1 ELSE 0 END) AS embedded
           FROM "help_doc_chunks"
           GROUP BY "docId""#,
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                // v4's `Number(r.embedded ?? 0)`.
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        let mut out = HashMap::new();
        for r in rows {
            let (doc_id, total, embedded) = r?;
            out.insert(doc_id, SectionCounts { total, embedded });
        }
        Ok(out)
    }
}

/// One entry of [`HelpDocChunksRepository::count_by_doc`] — v4's `{ total,
/// embedded }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionCounts {
    pub total: i64,
    pub embedded: i64,
}

fn row_to_chunk(r: &rusqlite::Row<'_>) -> rusqlite::Result<HelpDocChunkRow> {
    let blob: Option<Vec<u8>> = r.get(5)?;
    Ok(HelpDocChunkRow {
        id: r.get(0)?,
        doc_id: r.get(1)?,
        chunk_index: r.get(2)?,
        heading: r.get(3)?,
        content: r.get(4)?,
        embedding: blob.as_deref().map(blob_to_float32).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Materialize the MIGRATION-shaped table (the boot-ensure shape) so these
    /// unit tests exercise the constrained variant — the stricter of the two.
    fn open() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            r#"CREATE TABLE "help_docs" ("id" TEXT PRIMARY KEY NOT NULL, "title" TEXT NOT NULL);"#,
        )
        .unwrap();
        conn.execute_batch(super::super::help_doc_chunks_repair::HELP_DOC_CHUNKS_TABLE_DDL)
            .unwrap();
        conn.execute_batch(super::super::help_doc_chunks_repair::HELP_DOC_CHUNKS_INDEX_DDL)
            .unwrap();
        conn.execute(
            r#"INSERT INTO "help_docs" ("id","title") VALUES ('doc-a','A'),('doc-b','B')"#,
            [],
        )
        .unwrap();
        conn
    }

    fn draft(i: i64, heading: Option<&str>, content: &str) -> HelpDocChunkDraft {
        HelpDocChunkDraft {
            chunk_index: i,
            heading: heading.map(str::to_string),
            content: content.to_string(),
        }
    }

    #[test]
    fn replace_for_doc_discards_old_rows_and_their_embeddings() {
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);

        assert_eq!(repo.count().unwrap(), 0);
        assert_eq!(
            repo.replace_for_doc(
                "doc-a",
                &[draft(0, Some("H"), "one"), draft(1, None, "two")]
            )
            .unwrap(),
            2
        );
        assert_eq!(repo.count().unwrap(), 2);

        // Fill one embedding, then re-slice: the vector must NOT survive.
        let rows = repo.find_by_doc_id("doc-a").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chunk_index, 0.0);
        assert_eq!(rows[0].heading.as_deref(), Some("H"));
        assert!(rows[1].heading.is_none());
        repo.update_embedding(&rows[0].id, &[0.5, -0.25], "2026-01-01T00:00:00.000Z")
            .unwrap();
        assert_eq!(repo.find_all_with_embeddings().unwrap().len(), 1);

        assert_eq!(
            repo.replace_for_doc("doc-a", &[draft(0, Some("H"), "one (edited)")])
                .unwrap(),
            1
        );
        assert_eq!(repo.count().unwrap(), 1);
        assert!(repo.find_all_with_embeddings().unwrap().is_empty());
    }

    #[test]
    fn delete_by_doc_id_is_scoped_and_clear_all_is_not() {
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);
        repo.replace_for_doc("doc-a", &[draft(0, None, "a0"), draft(1, None, "a1")])
            .unwrap();
        repo.replace_for_doc("doc-b", &[draft(0, None, "b0")])
            .unwrap();
        for row in repo.find_by_doc_id("doc-a").unwrap() {
            repo.update_embedding(&row.id, &[1.0, 0.0], "2026-01-01T00:00:00.000Z")
                .unwrap();
        }

        // clear_all touches every row, embedded or not (v4's updateMany({})).
        assert_eq!(
            repo.clear_all_embeddings("2026-01-02T00:00:00.000Z")
                .unwrap(),
            3
        );
        assert!(repo.find_all_with_embeddings().unwrap().is_empty());

        assert_eq!(repo.delete_by_doc_id("doc-a").unwrap(), 2);
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(repo.find_by_doc_id("doc-b").unwrap().len(), 1);
    }

    #[test]
    fn delete_orphaned_removes_only_parentless_chunks() {
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);
        repo.replace_for_doc("doc-a", &[draft(0, None, "a0")])
            .unwrap();
        repo.replace_for_doc("doc-b", &[draft(0, None, "b0"), draft(1, None, "b1")])
            .unwrap();

        assert_eq!(
            repo.delete_orphaned(&["doc-a".to_string(), "doc-b".to_string()])
                .unwrap(),
            0
        );
        assert_eq!(repo.delete_orphaned(&["doc-a".to_string()]).unwrap(), 2);
        assert_eq!(repo.count().unwrap(), 1);
        // An empty live set orphans everything.
        assert_eq!(repo.delete_orphaned(&[]).unwrap(), 1);
        assert_eq!(repo.count().unwrap(), 0);
    }

    #[test]
    fn update_patches_text_only_and_reports_missing_rows() {
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);
        repo.replace_for_doc("doc-a", &[draft(0, Some("Old"), "before")])
            .unwrap();
        let row = repo.find_by_doc_id("doc-a").unwrap().remove(0);
        repo.update_embedding(&row.id, &[0.5, 0.5], "2026-01-01T00:00:00.000Z")
            .unwrap();

        assert!(repo
            .update(&row.id, Some("New"), "after", "2026-01-03T00:00:00.000Z")
            .unwrap());
        let after = repo.find_by_doc_id("doc-a").unwrap().remove(0);
        assert_eq!(after.heading.as_deref(), Some("New"));
        assert_eq!(after.content, "after");
        // The embedding survived a text-only update.
        assert_eq!(after.embedding.len(), 2);

        assert!(!repo
            .update("nope", None, "x", "2026-01-03T00:00:00.000Z")
            .unwrap());
        assert!(!repo.delete("nope").unwrap());
    }

    #[test]
    fn foreign_key_cascade_fires_on_the_migration_shape() {
        // The belt: on an instance that DID get the migration DDL, deleting the
        // parent takes the chunks with it. The braces (`delete_by_doc_id` at
        // every prune site) are what cover the instances that did not.
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);
        repo.replace_for_doc("doc-a", &[draft(0, None, "a0")])
            .unwrap();
        conn.execute("DELETE FROM help_docs WHERE id = 'doc-a'", [])
            .unwrap();
        assert_eq!(repo.count().unwrap(), 0);
    }

    /// P4.D222: v4's `countByDoc` is a FALLBACK `safeQuery` — a failing read
    /// logs ONE ERROR `Error counting help doc chunks by doc` `{collection,
    /// error}` (v4 passes `{}`; `AbstractBaseRepository.safeQuery` enriches it
    /// with `collection`) and answers an EMPTY map.
    #[test]
    fn count_by_doc_falls_back_to_an_empty_map_with_one_error() {
        let conn = open();
        conn.execute_batch(r#"ALTER TABLE "help_doc_chunks" RENAME TO "help_doc_chunks_gone""#)
            .expect("plant");
        let (counts, lines) = crate::test_support::captured_with(|| {
            HelpDocChunksRepository::new(&conn).count_by_doc()
        });
        assert!(counts.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert!(
            line.starts_with("ERROR quilltap::db"),
            "level/target: {line}"
        );
        assert!(
            line.contains(
                " Error counting help doc chunks by doc collection=help_doc_chunks error="
            ),
            "{line}"
        );
        assert!(line.contains("no such table: help_doc_chunks"), "{line}");
    }

    /// The silence leg, and the counts over the shapes the reconcile reads.
    #[test]
    fn count_by_doc_counts_sections_and_is_silent_when_healthy() {
        let conn = open();
        let repo = HelpDocChunksRepository::new(&conn);
        repo.replace_for_doc("doc-a", &[draft(0, None, "a0"), draft(1, None, "a1")])
            .unwrap();
        repo.replace_for_doc("doc-b", &[draft(0, None, "b0")])
            .unwrap();
        let a0 = repo.find_by_doc_id("doc-a").unwrap().remove(0);
        repo.update_embedding(&a0.id, &[1.0, 0.0], "2026-01-02T00:00:00.000Z")
            .unwrap();
        // A zero-length BLOB counts as embedded here (v4's `IS NOT NULL`).
        conn.execute(
            "UPDATE help_doc_chunks SET embedding = X'' WHERE docId = 'doc-b'",
            [],
        )
        .unwrap();
        let (counts, lines) = crate::test_support::captured_with(|| repo.count_by_doc());
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(counts.len(), 2);
        assert_eq!(
            counts["doc-a"],
            SectionCounts {
                total: 2,
                embedded: 1
            }
        );
        assert_eq!(
            counts["doc-b"],
            SectionCounts {
                total: 1,
                embedded: 1
            }
        );
    }
}
