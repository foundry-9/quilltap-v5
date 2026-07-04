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
//! (its `id`/`createdAt` preserved, `updatedAt` bumped).
//!
//! ## The mount-point+path facade (W4.1d batch 3b)
//!
//! The blob doc-edit tool handlers (`doc_write_blob` / `doc_read_blob` /
//! `doc_list_blobs` / `doc_delete_blob`) hold a `(mountPointId, relativePath)`
//! tuple, so v4's legacy facade methods are ported: [`DocMountBlobsRepository::create`]
//! (delegates to `link_blob_content` — the content/link split, the binary analogue
//! of `linkDocumentContent` — then reads the joined view back),
//! [`DocMountBlobsRepository::find_by_mount_point_and_path`],
//! [`DocMountBlobsRepository::list_by_mount_point`], [`DocMountBlobsRepository::read_data`],
//! and [`DocMountBlobsRepository::delete_by_mount_point_and_path`] (link delete +
//! file GC; blob bytes cascade off the file row).
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

use super::doc_mount_file_links::{DocMountFileLinksRepository, LinkBlobInput};
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

/// Input to [`DocMountBlobsRepository::create`] (v4 `CreateBlobInput`) — the
/// legacy "give me a blob row at this (mount, path)" call shape on top of the
/// content/link split (delegates to `link_blob_content`).
pub struct CreateBlobInput {
    pub mount_point_id: String,
    pub relative_path: String,
    pub original_file_name: String,
    pub original_mime_type: String,
    pub stored_mime_type: String,
    /// Advisory — recomputed from `data` by `link_blob_content`.
    pub sha256: String,
    pub data: Vec<u8>,
    /// `None` → `''`.
    pub description: Option<String>,
    /// `None` → `path.posix.basename(relativePath)`.
    pub file_name: Option<String>,
    /// `None` → `'blob'`.
    pub file_type: Option<String>,
}

/// A blob's metadata joined with its link location (v4 `DocMountBlobWithLink`,
/// restricted to the fields the blob tool handlers read). `link_id`/etc come from
/// `doc_mount_file_links`, the fingerprint/size/mime from `doc_mount_blobs`.
#[derive(Clone, Debug)]
pub struct BlobWithLink {
    pub id: String,
    pub file_id: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub stored_mime_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub link_id: String,
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub folder_id: Option<String>,
    pub original_file_name: String,
    pub original_mime_type: String,
    pub description: String,
    pub extracted_text: Option<String>,
    pub extraction_status: Option<String>,
    pub last_modified: String,
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

    /// v4 `listByMountPoint(mountPointId).length` — the number of blob-backed
    /// files linked to a mount point (join `doc_mount_file_links` → blobs by
    /// `fileId`). Used by the `project_info` tool's store summary (it only needs
    /// the count). Read-only.
    pub fn count_by_mount_point(&self, mount_point_id: &str) -> Result<i64, DbError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM doc_mount_file_links l \
               JOIN doc_mount_blobs b ON b.fileId = l.fileId \
              WHERE l.mountPointId = ?1",
            params![mount_point_id],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    /// Plain delete by id (v4 `delete`). Returns `false` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_blobs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `create` (`doc-mount-blobs.repository.ts:289`): the legacy one-call
    /// facade for callers holding a `(mountPointId, relativePath)` tuple. Delegates
    /// to `link_blob_content` (dedup-by-sha + upsert the link row), then reads the
    /// joined view back. Reuses the SAME connection (v4 late-binds the link repo).
    pub fn create(&self, input: &CreateBlobInput) -> Result<BlobWithLink, DbError> {
        let file_name = input
            .file_name
            .clone()
            .unwrap_or_else(|| posix_basename(&input.relative_path).to_string());
        let file_type = input
            .file_type
            .clone()
            .unwrap_or_else(|| "blob".to_string());

        let result =
            DocMountFileLinksRepository::new(self.conn).link_blob_content(&LinkBlobInput {
                mount_point_id: input.mount_point_id.clone(),
                relative_path: input.relative_path.clone(),
                file_name,
                file_type: Some(file_type),
                original_file_name: input.original_file_name.clone(),
                original_mime_type: input.original_mime_type.clone(),
                stored_mime_type: input.stored_mime_type.clone(),
                sha256: input.sha256.clone(),
                data: input.data.clone(),
                description: input.description.clone(),
                conversion_status: None,
                extracted_text: None,
                extracted_text_sha256: None,
                extraction_status: None,
            })?;

        self.find_by_mount_point_and_path(&input.mount_point_id, &input.relative_path)?
            .ok_or_else(|| {
                DbError::Key(format!(
                    "Blob row not visible after upsert: {}/{} (link {})",
                    input.mount_point_id, input.relative_path, result.link_id
                ))
            })
    }

    /// v4 `findByMountPointAndPath` (`doc-mount-blobs.repository.ts:393`): the blob
    /// joined with its link at `(mountPointId, relativePath)`, **case-insensitive**
    /// on the path (`LOWER(l.relativePath) = LOWER(?)`), or `None`.
    pub fn find_by_mount_point_and_path(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<Option<BlobWithLink>, DbError> {
        self.conn
            .query_row(
                &Self::join_query(
                    "WHERE l.mountPointId = ?1 AND LOWER(l.relativePath) = LOWER(?2)",
                ),
                params![mount_point_id, relative_path],
                Self::map_blob_with_link,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `listByMountPoint` (`doc-mount-blobs.repository.ts:428`): every blob
    /// visible at a mount, joined through links, ordered by `relativePath ASC`. An
    /// optional `folder` filter limits to blobs under that folder prefix (v4
    /// appends a trailing `/` and matches `LIKE 'prefix/%'`).
    pub fn list_by_mount_point(
        &self,
        mount_point_id: &str,
        folder: Option<&str>,
    ) -> Result<Vec<BlobWithLink>, DbError> {
        match folder {
            Some(f) => {
                let prefix = if f.ends_with('/') {
                    f.to_string()
                } else {
                    format!("{f}/")
                };
                let sql = Self::join_query(
                    "WHERE l.mountPointId = ?1 AND l.relativePath LIKE ?2 \
                     ORDER BY l.relativePath ASC",
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(
                        params![mount_point_id, format!("{prefix}%")],
                        Self::map_blob_with_link,
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            None => {
                let sql = Self::join_query("WHERE l.mountPointId = ?1 ORDER BY l.relativePath ASC");
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![mount_point_id], Self::map_blob_with_link)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }

    /// v4 `readData` (`doc-mount-blobs.repository.ts:189`): the raw bytes for a
    /// blob row by its blob `id`, or `None`.
    pub fn read_data(&self, id: &str) -> Result<Option<Vec<u8>>, DbError> {
        self.conn
            .query_row(
                "SELECT data FROM doc_mount_blobs WHERE id = ?1",
                params![id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `deleteByMountPointAndPath` (`doc-mount-blobs.repository.ts:594`): delete
    /// the link at `(mountPointId, relativePath)`, GC'ing the file row when it was
    /// the last link (the blob bytes cascade off `doc_mount_files` deletion; the
    /// writable open enforces `foreign_keys = ON`). NOTE v4 matches the path
    /// **case-sensitively** here (`relativePath = ?`, unlike the read join) —
    /// reproduced verbatim. Returns `false` when no link matched.
    pub fn delete_by_mount_point_and_path(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<bool, DbError> {
        let link: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, fileId FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND relativePath = ?2",
                params![mount_point_id, relative_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((link_id, file_id)) = link else {
            return Ok(false);
        };

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM doc_mount_file_links WHERE id = ?1",
            params![link_id],
        )?;
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM doc_mount_file_links WHERE fileId = ?1",
            params![file_id],
            |row| row.get(0),
        )?;
        if remaining == 0 {
            tx.execute(
                "DELETE FROM doc_mount_files WHERE id = ?1",
                params![file_id],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// The `l JOIN b` SELECT used by the joined-view finders (v4's shared column
    /// list), with the caller's WHERE/ORDER appended.
    fn join_query(tail: &str) -> String {
        format!(
            "SELECT \
               b.id, b.fileId, b.sha256, b.sizeBytes, b.storedMimeType, \
               b.createdAt, b.updatedAt, \
               l.id AS linkId, l.mountPointId, l.relativePath, l.fileName, \
               l.folderId, l.originalFileName, l.originalMimeType, \
               l.description, l.extractedText, l.extractionStatus, l.lastModified \
             FROM doc_mount_file_links l \
             JOIN doc_mount_blobs b ON b.fileId = l.fileId \
             {tail}"
        )
    }

    fn map_blob_with_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<BlobWithLink> {
        Ok(BlobWithLink {
            id: row.get(0)?,
            file_id: row.get(1)?,
            sha256: row.get(2)?,
            // `sizeBytes` is INTEGER on the blobs table (metadata schema), so a
            // straight i64 read; tolerate a Real form defensively.
            size_bytes: match row.get_ref(3)? {
                rusqlite::types::ValueRef::Integer(i) => i,
                rusqlite::types::ValueRef::Real(f) => f as i64,
                _ => 0,
            },
            stored_mime_type: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            link_id: row.get(7)?,
            mount_point_id: row.get(8)?,
            relative_path: row.get(9)?,
            file_name: row.get(10)?,
            folder_id: row.get(11)?,
            original_file_name: row.get(12)?,
            original_mime_type: row.get(13)?,
            description: row.get(14)?,
            extracted_text: row.get(15)?,
            extraction_status: row.get(16)?,
            last_modified: row.get(17)?,
        })
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

/// Node `path.posix.basename` for a POSIX path — everything after the last `/`
/// (used to default a create's `fileName`).
fn posix_basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}
