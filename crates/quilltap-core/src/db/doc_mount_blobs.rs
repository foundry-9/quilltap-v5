//! The doc-mount-blobs repository — the document store's **binary** byte-store
//! (build step 8 of the document-store overlay slice). Ports v4's
//! `DocMountBlobsRepository` (`lib/database/repositories/doc-mount-blobs.repository.ts`).
//!
//! The text byte-store is `doc_mount_documents` (ported); this is its binary
//! sibling — avatars, PDFs/DOCX bytes, any non-text content, stored in a `data`
//! BLOB column keyed UNIQUE by `fileId` (one blob per content-identity file row).
//! Unlike the Zod-schema repos, v4 hand-writes this repo (and its table DDL): the
//! `data BLOB NOT NULL` column is deliberately ABSENT from
//! `DocMountBlobMetadataSchema` (metadata reads never hydrate the bytes), so the
//! table is materialized from the repo's own DDL, not `generateDDL`.
//!
//! ## The byte-landing mutation
//!
//! [`DocMountBlobsRepository::upsert_by_file_id`] is the binary analogue of
//! `doc_mount_documents`' upsert: insert-or-replace by `fileId`, **recomputing
//! `sha256` from the actual bytes** (the caller's `sha256` is advisory — the
//! store owns its hashes, keeping the invariant `sha256 == sha256(data)`), with
//! `sizeBytes = data.len()`. An existing `fileId` row is overwritten in place
//! (its `id`/`createdAt` preserved, `updatedAt` bumped). `linkBlobContent` (the
//! `(mountPointId, relativePath)` content/link split, the binary analogue of
//! `linkDocumentContent`) is a separate, larger unit and is out of scope here —
//! this ports the blob byte-store itself.
//!
//! ## Foreign key
//!
//! The table declares `FOREIGN KEY (fileId) REFERENCES doc_mount_files(id)`, and
//! the writable open enables `foreign_keys = ON`, so a blob's `fileId` must
//! reference a real `doc_mount_files` row — the tier-2 fixture seeds the parents.
//!
//! Determinism: `upsert_by_file_id` mints `id` + timestamps, so the tier-2
//! differential uses the minted-values remap form (remap `id`, placeholder
//! timestamps); `fileId` is the pinned seeded parent id. The `data` BLOB is
//! dumped as lowercase hex (bit-exact, mirrors `help_docs` / `doc_mount_chunks`).

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::DbError;

/// The DDL v4's `DocMountBlobsRepository.db()` lazily executes on first access —
/// reproduced verbatim so a fixture (and the Rust port) materialize a table whose
/// shape (incl. the `data BLOB` column the Zod metadata schema omits, the FK, and
/// the UNIQUE `fileId` index) is identical to production. `IF NOT EXISTS`, so it
/// is idempotent against a fixture that already created it.
pub const CREATE_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS \"doc_mount_blobs\" ( \
  \"id\" TEXT PRIMARY KEY, \
  \"fileId\" TEXT NOT NULL, \
  \"sha256\" TEXT NOT NULL, \
  \"sizeBytes\" INTEGER NOT NULL, \
  \"storedMimeType\" TEXT NOT NULL, \
  \"data\" BLOB NOT NULL, \
  \"createdAt\" TEXT NOT NULL, \
  \"updatedAt\" TEXT NOT NULL, \
  FOREIGN KEY (\"fileId\") REFERENCES \"doc_mount_files\" (\"id\") ON DELETE CASCADE \
)";

/// The input to [`DocMountBlobsRepository::upsert_by_file_id`] (v4 `UpsertBlobInput`).
pub struct UpsertBlobInput {
    pub file_id: String,
    /// Advisory only — the upsert recomputes `sha256` from `data`.
    pub sha256: String,
    pub stored_mime_type: String,
    pub data: Vec<u8>,
}

/// The blob row's metadata (v4 `DocMountBlobMetadata` — the `data` BLOB is
/// deliberately excluded; metadata reads never hydrate the bytes).
pub struct BlobMetadata {
    pub id: String,
    pub file_id: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub stored_mime_type: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountBlobsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountBlobsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert-or-replace the blob row for `fileId` (v4 `upsertByFileId`). The
    /// stored `sha256` is **recomputed from the bytes** (the caller's `sha256` is
    /// advisory); `sizeBytes = data.len()`. An existing `fileId` row is overwritten
    /// in place (id/createdAt preserved, updatedAt bumped); otherwise a fresh row
    /// is minted. Returns the resulting metadata.
    pub fn upsert_by_file_id(&self, input: &UpsertBlobInput) -> Result<BlobMetadata, DbError> {
        let now = crate::clock::now_iso();
        let size_bytes = input.data.len() as i64;
        let computed = hex::encode(Sha256::digest(&input.data));

        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM doc_mount_blobs WHERE fileId = ?1",
                params![input.file_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let id = if let Some(id) = existing {
            self.conn.execute(
                "UPDATE doc_mount_blobs SET \
                   sha256 = ?1, sizeBytes = ?2, storedMimeType = ?3, data = ?4, updatedAt = ?5 \
                 WHERE id = ?6",
                params![
                    computed,
                    size_bytes,
                    input.stored_mime_type,
                    input.data,
                    now,
                    id
                ],
            )?;
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            self.conn.execute(
                "INSERT INTO doc_mount_blobs \
                   (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    input.file_id,
                    computed,
                    size_bytes,
                    input.stored_mime_type,
                    input.data,
                    now,
                    now
                ],
            )?;
            id
        };

        self.find_by_id(&id)?
            .ok_or_else(|| DbError::Key(format!("blob disappeared after upsert: {id}")))
    }

    /// Metadata by blob id (v4 `findById`) — never hydrates the `data` bytes.
    pub fn find_by_id(&self, id: &str) -> Result<Option<BlobMetadata>, DbError> {
        self.metadata_query("WHERE id = ?1", id)
    }

    /// Metadata by `fileId` (v4 `findByFileId`).
    pub fn find_by_file_id(&self, file_id: &str) -> Result<Option<BlobMetadata>, DbError> {
        self.metadata_query("WHERE fileId = ?1", file_id)
    }

    /// Raw bytes for a blob row, by `fileId` (v4 `readDataByFileId`).
    pub fn read_data_by_file_id(&self, file_id: &str) -> Result<Option<Vec<u8>>, DbError> {
        self.conn
            .query_row(
                "SELECT data FROM doc_mount_blobs WHERE fileId = ?1",
                params![file_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// Plain delete by id (v4 `delete`). Returns `false` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_blobs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    fn metadata_query(
        &self,
        where_clause: &str,
        key: &str,
    ) -> Result<Option<BlobMetadata>, DbError> {
        self.conn
            .query_row(
                &format!(
                    "SELECT id, fileId, sha256, sizeBytes, storedMimeType, createdAt, updatedAt \
                     FROM doc_mount_blobs {where_clause}"
                ),
                params![key],
                |row| {
                    Ok(BlobMetadata {
                        id: row.get(0)?,
                        file_id: row.get(1)?,
                        sha256: row.get(2)?,
                        size_bytes: row.get(3)?,
                        stored_mime_type: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }
}
