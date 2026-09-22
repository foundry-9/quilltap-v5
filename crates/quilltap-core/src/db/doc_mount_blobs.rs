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
    /// v4 passes these straight through to the link row INCLUDING their `null`
    /// (see [`crate::db::doc_mount_file_links::LinkBlobInput::original_file_name`]).
    pub original_file_name: Option<String>,
    pub original_mime_type: Option<String>,
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
    /// Forwarded to `link_blob_content`. `true` normalizes images to WebP
    /// before storing; `false` is for byte-fidelity restores ONLY — the
    /// `.qtap` import and archive rehydrate (v4 `186eb09cb`).
    pub normalize_images: bool,
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
    pub original_file_name: Option<String>,
    pub original_mime_type: Option<String>,
    pub description: String,
    pub extracted_text: Option<String>,
    pub extraction_status: Option<String>,
    pub last_modified: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountBlobsRepository<'c> {
    conn: &'c Connection,
    /// The WebP encoder [`Self::create`] hands to the links repository it
    /// writes through (P4.104). v4's `create` late-binds
    /// `docMountFileLinks.linkBlobContent`, whose normalization imports `sharp`
    /// at module scope; v5 injects the encoder, so the facade must carry it or
    /// its writes store the original bytes where v4 stores WebP. `None` (the
    /// [`Self::new`] default) is for reads and deletes — and for the ONE write
    /// that passes `normalize_images: false` (the `.qtap` import), where the
    /// encoder is never consulted.
    blob_codec: Option<&'c dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
}

impl<'c> DocMountBlobsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self {
            conn,
            blob_codec: None,
        }
    }

    /// [`Self::new`] with the host's WebP encoder wired, so [`Self::create`]
    /// normalizes image bytes the way v4's `linkBlobContent` does (P4.104).
    pub fn with_blob_codec(
        conn: &'c Connection,
        codec: &'c dyn crate::services::mount_index::blob_transcode::WebpTranscoder,
    ) -> Self {
        Self {
            conn,
            blob_codec: Some(codec),
        }
    }

    /// v4's lazy table-init (the repo's overridden `db()` creates
    /// `doc_mount_blobs` on first access — generateDDL can't express the
    /// `data BLOB` column, so the DDL is hand-written there and reproduced
    /// verbatim here). Cheap and idempotent; the write paths call it so a
    /// store created at runtime accepts its first blob (P4.6y).
    pub fn ensure_table(conn: &Connection) -> Result<(), DbError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS \"doc_mount_blobs\" (\
               \"id\" TEXT PRIMARY KEY,\
               \"fileId\" TEXT NOT NULL,\
               \"sha256\" TEXT NOT NULL,\
               \"sizeBytes\" INTEGER NOT NULL,\
               \"storedMimeType\" TEXT NOT NULL,\
               \"data\" BLOB NOT NULL,\
               \"createdAt\" TEXT NOT NULL,\
               \"updatedAt\" TEXT NOT NULL,\
               FOREIGN KEY (\"fileId\") REFERENCES \"doc_mount_files\" (\"id\") ON DELETE CASCADE\
             );\
             CREATE UNIQUE INDEX IF NOT EXISTS \"idx_doc_mount_blobs_fileId\" \
             ON \"doc_mount_blobs\" (\"fileId\");",
        )?;
        Ok(())
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
            .ok_or_else(|| DbError::Internal(format!("blob disappeared after upsert: {id}")))
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
        self.create_with_ids(
            input,
            &crate::db::doc_mount_file_links::CarriedRowIds::default(),
        )
    }

    /// [`Self::create`] forwarding explicit row ids for the rows the write
    /// actually CREATES — v4's `CreateBlobInput.fileId` / `.linkId` / `.blobId`
    /// (`01e481f6`), which it passes straight through to `linkBlobContent`.
    /// Only the `preserveIds` import path supplies them.
    pub fn create_with_ids(
        &self,
        input: &CreateBlobInput,
        carried: &crate::db::doc_mount_file_links::CarriedRowIds,
    ) -> Result<BlobWithLink, DbError> {
        let file_name = input
            .file_name
            .clone()
            .unwrap_or_else(|| posix_basename(&input.relative_path).to_string());
        let file_type = input
            .file_type
            .clone()
            .unwrap_or_else(|| "blob".to_string());

        let links = match self.blob_codec {
            Some(codec) => DocMountFileLinksRepository::with_blob_codec(self.conn, codec),
            None => DocMountFileLinksRepository::new(self.conn),
        };
        let result = links.link_blob_content_with_ids(
            &LinkBlobInput {
                mount_point_id: input.mount_point_id.clone(),
                relative_path: input.relative_path.clone(),
                file_name,
                file_type: Some(file_type),
                original_file_name: input.original_file_name.clone(),
                original_mime_type: input.original_mime_type.clone(),
                stored_mime_type: input.stored_mime_type.clone(),
                sha256: input.sha256.clone(),
                data: input.data.clone(),
                normalize_images: input.normalize_images,
                description: input.description.clone(),
                conversion_status: None,
                extracted_text: None,
                extracted_text_sha256: None,
                extraction_status: None,
                last_modified: None,
                created_at: None,
            },
            carried,
        )?;

        // v4 reads back by `link.mountPointId` / `link.relativePath` — the
        // location the links write LANDED at, which normalization may have
        // rewritten (`plate.png` → `plate.webp`, P4.104). The caller's path
        // names a row that no longer exists once the codec is wired.
        let (mount_point_id, relative_path): (String, String) = self.conn.query_row(
            "SELECT mountPointId, relativePath FROM doc_mount_file_links WHERE id = ?1",
            params![result.link_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        self.find_by_mount_point_and_path(&mount_point_id, &relative_path)?
            .ok_or_else(|| {
                DbError::Internal(format!(
                    "Blob row not visible after upsert: {}/{} (link {})",
                    mount_point_id, relative_path, result.link_id
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

    /// Update the description on ONE link to a blob row — v4
    /// `updateDescription(id, description, linkId)` (`0c14fd61f`, bug 157).
    ///
    /// Blobs themselves don't carry per-link metadata; the description is a
    /// property of the `(mountPoint, path)` LINK, and content-addressing means
    /// one blob row can carry several links — a character vault holds every
    /// avatar at both `photos/` and `images/history/`, byte-identical, on one
    /// file row.
    ///
    /// `link_id` is therefore **required**: there is no such thing as "the"
    /// link for a blob. It used to be optional, and the two-argument form
    /// resolved the target with `WHERE fileId = ? LIMIT 1`, which wrote the
    /// caption to an arbitrary one of the sharing locations (bug 157 — v5
    /// reproduced it). Callers that hold a path already hold the link:
    /// [`Self::find_by_mount_point_and_path`] returns `link_id` on the joined
    /// view.
    ///
    /// Returns the refreshed joined view, or `None` when the blob is missing.
    pub fn update_description(
        &self,
        id: &str,
        description: &str,
        link_id: &str,
    ) -> Result<Option<BlobWithLink>, DbError> {
        let now = crate::clock::now_iso();
        let Some(_blob) = self.find_by_id(id)? else {
            return Ok(None);
        };
        let target_link = link_id;
        self.conn.execute(
            "UPDATE doc_mount_file_links                SET description = ?1, descriptionUpdatedAt = ?2, updatedAt = ?3              WHERE id = ?4",
            params![description, now, now, target_link],
        )?;
        self.conn
            .query_row(
                &Self::join_query("WHERE l.id = ?1"),
                params![target_link],
                Self::map_blob_with_link,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// Update the extracted-text / extraction-status fields on ONE link to a
    /// blob row — v4 `updateExtractedText(id, input, linkId)` (`0c14fd61f`,
    /// bug 157). Same per-link semantics as [`Self::update_description`],
    /// `link_id` included: the extracted text of a shared blob belongs to a
    /// location, not to the bytes. All four fields are always set (`None` → SQL
    /// NULL), matching v4's unconditional UPDATE.
    #[allow(clippy::too_many_arguments)]
    pub fn update_extracted_text(
        &self,
        id: &str,
        extracted_text: Option<&str>,
        extracted_text_sha256: Option<&str>,
        extraction_status: &str,
        extraction_error: Option<&str>,
        link_id: &str,
    ) -> Result<bool, DbError> {
        let now = crate::clock::now_iso();
        let Some(_blob) = self.find_by_id(id)? else {
            return Ok(false);
        };
        let target_link = link_id;
        self.conn.execute(
            "UPDATE doc_mount_file_links SET                extractedText = ?1, extractedTextSha256 = ?2,                extractionStatus = ?3, extractionError = ?4, updatedAt = ?5              WHERE id = ?6",
            params![
                extracted_text,
                extracted_text_sha256,
                extraction_status,
                extraction_error,
                now,
                target_link
            ],
        )?;
        Ok(true)
    }

    /// The FULL v4 `DocMountBlobWithLink` row as JSON in v4's exact key order
    /// (the 21-column joined view the blobs routes return verbatim — nulls
    /// literal, REAL numbers collapsed via `js_number_to_json`). `tail` is the
    /// WHERE/ORDER clause; P4.6y additive.
    fn full_json_rows(
        &self,
        tail: &str,
        params_slice: &[&dyn rusqlite::types::ToSql],
    ) -> Result<Vec<serde_json::Value>, DbError> {
        let sql = format!(
            "SELECT                b.id, b.fileId, b.sha256, b.sizeBytes, b.storedMimeType,                b.createdAt, b.updatedAt,                l.id AS linkId, l.mountPointId, l.relativePath, l.fileName,                l.folderId, l.originalFileName, l.originalMimeType,                l.description, l.descriptionUpdatedAt,                l.extractedText, l.extractedTextSha256, l.extractionStatus, l.extractionError,                l.lastModified              FROM doc_mount_file_links l              JOIN doc_mount_blobs b ON b.fileId = l.fileId              {tail}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let rows = stmt
            .query_map(params_slice, |row| {
                let mut obj = serde_json::Map::new();
                for (i, name) in names.iter().enumerate() {
                    let v = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => serde_json::json!(n),
                        rusqlite::types::ValueRef::Real(f) => super::js_number_to_json(f),
                        rusqlite::types::ValueRef::Text(t) => {
                            serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        rusqlite::types::ValueRef::Blob(b) => serde_json::json!(b.len()),
                    };
                    obj.insert(name.clone(), v);
                }
                Ok(serde_json::Value::Object(obj))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByMountPointAndPath` as the verbatim JSON view (P4.6y).
    pub fn find_full_json_by_mount_point_and_path(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<Option<serde_json::Value>, DbError> {
        Ok(self
            .full_json_rows(
                "WHERE l.mountPointId = ?1 AND LOWER(l.relativePath) = LOWER(?2)",
                &[&mount_point_id, &relative_path],
            )?
            .into_iter()
            .next())
    }

    /// The updated joined view by LINK id (v4 `updateDescription`'s return
    /// query, `WHERE l.id = ?`) — P4.6y.
    pub fn find_full_json_by_link_id(
        &self,
        link_id: &str,
    ) -> Result<Option<serde_json::Value>, DbError> {
        Ok(self
            .full_json_rows("WHERE l.id = ?1", &[&link_id])?
            .into_iter()
            .next())
    }

    /// v4 `listByMountPoint` as the verbatim JSON view (P4.6y).
    pub fn list_full_json_by_mount_point(
        &self,
        mount_point_id: &str,
        folder: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        match folder {
            Some(f) => {
                let prefix = if f.ends_with('/') {
                    f.to_string()
                } else {
                    format!("{f}/")
                };
                let like = format!("{prefix}%");
                self.full_json_rows(
                    "WHERE l.mountPointId = ?1 AND l.relativePath LIKE ?2 ORDER BY l.relativePath ASC",
                    &[&mount_point_id, &like],
                )
            }
            None => self.full_json_rows(
                "WHERE l.mountPointId = ?1 ORDER BY l.relativePath ASC",
                &[&mount_point_id],
            ),
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

#[cfg(test)]
mod create_reads_back_the_normalized_row_tests {
    use super::*;
    use crate::services::mount_index::blob_transcode::WebpTranscoder;

    /// "Transcodes" anything to a fixed payload (D19: the ORDER of the write's
    /// steps is the point, never sharp's bytes).
    struct ScriptedCodec;
    impl WebpTranscoder for ScriptedCodec {
        fn encode_webp(&self, _bytes: &[u8], _quality: u8) -> Result<Vec<u8>, String> {
            Ok(b"SCRIPTED-WEBP-PAYLOAD".to_vec())
        }
    }

    fn scratch() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL, sha256 TEXT NOT NULL,
                fileSizeBytes REAL, fileType TEXT NOT NULL, source TEXT NOT NULL,
                createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);
             CREATE TABLE doc_mount_folders (
                id TEXT PRIMARY KEY NOT NULL, mountPointId TEXT NOT NULL, path TEXT NOT NULL,
                name TEXT NOT NULL, parentId TEXT, createdAt TEXT NOT NULL,
                updatedAt TEXT NOT NULL);
             CREATE TABLE doc_mount_file_links (
                id TEXT PRIMARY KEY NOT NULL, fileId TEXT NOT NULL, linkGroupId TEXT,
                mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL, fileName TEXT NOT NULL,
                folderId TEXT, originalFileName TEXT, originalMimeType TEXT,
                description TEXT, descriptionUpdatedAt TEXT, conversionStatus TEXT NOT NULL,
                conversionError TEXT, plainTextLength REAL, extractedText TEXT,
                extractedTextSha256 TEXT, extractionStatus TEXT NOT NULL, extractionError TEXT,
                chunkCount REAL NOT NULL DEFAULT 0, allowEmbed REAL NOT NULL DEFAULT 1,
                allowCharacterRead REAL NOT NULL DEFAULT 1,
                allowCharacterWrite REAL NOT NULL DEFAULT 1,
                lastModified TEXT NOT NULL, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn png_create() -> CreateBlobInput {
        CreateBlobInput {
            mount_point_id: "mp-1".to_string(),
            relative_path: "art/plate.png".to_string(),
            original_file_name: Some("plate.png".to_string()),
            original_mime_type: Some("image/png".to_string()),
            stored_mime_type: "image/png".to_string(),
            sha256: "0".repeat(64),
            data: b"pretend-png-bytes".to_vec(),
            description: None,
            file_name: None,
            file_type: None,
            normalize_images: true,
        }
    }

    /// v4 `create` reads back by `link.relativePath` — the path the links
    /// write RETURNS, which normalization has already rewritten. Reading back
    /// by the CALLER's path finds nothing once a `.png` lands as `.webp`, and
    /// the write that succeeded answers "Blob row not visible after upsert".
    #[test]
    fn a_normalized_create_answers_the_webp_row() {
        let conn = scratch();
        let codec = ScriptedCodec;
        let found = DocMountBlobsRepository::with_blob_codec(&conn, &codec)
            .create(&png_create())
            .expect("the normalized write must read back");
        assert_eq!(found.relative_path, "art/plate.webp");
        assert_eq!(found.file_name, "plate.webp");
        assert_eq!(found.stored_mime_type, "image/webp");
        assert_eq!(
            found.sha256,
            hex::encode(Sha256::digest(b"SCRIPTED-WEBP-PAYLOAD"))
        );
    }

    /// The pass-through's other half: the facade with no codec leaves the
    /// bytes alone, so the green above cannot come from a row that never moved.
    #[test]
    fn a_create_without_a_codec_is_byte_preserving() {
        let conn = scratch();
        let found = DocMountBlobsRepository::new(&conn)
            .create(&png_create())
            .unwrap();
        assert_eq!(found.relative_path, "art/plate.png");
        assert_eq!(found.stored_mime_type, "image/png");
    }

    /// `normalize_images: false` wins over a wired codec — the `.qtap`
    /// import's byte-fidelity path rides on it.
    #[test]
    fn the_false_flag_wins_over_a_wired_codec() {
        let conn = scratch();
        let codec = ScriptedCodec;
        let mut input = png_create();
        input.normalize_images = false;
        let found = DocMountBlobsRepository::with_blob_codec(&conn, &codec)
            .create(&input)
            .unwrap();
        assert_eq!(found.relative_path, "art/plate.png");
        assert_eq!(found.stored_mime_type, "image/png");
    }
}
