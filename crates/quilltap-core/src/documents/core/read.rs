//! Reading a document in a store, and asking whether one is there — v4
//! `lib/documents/core/read.ts`.
//!
//! [`read_document`] is the one read with the one blob fallback (#134): a path
//! in a database store with no document may still hold an uploaded blob —
//! every upload is one, `.md` included — and it is read through
//! [`database_store::read_database_blob_for_document`] (a native-text upload's
//! own bytes, otherwise its converted text).

use rusqlite::Connection;

use crate::db::database_store::{self, BlobSource};
use crate::doc_edit::path_resolver::{stat_resolved_file, ResolvedPath};
use crate::documents::DocError;
use crate::tools::doc_edit::shared::read_file_with_mtime;

/// v4's `throw new Error('Database-backed ResolvedPath is missing mountPointId')`.
pub const MISSING_MOUNT_POINT_ID: &str = "Database-backed ResolvedPath is missing mountPointId";

/// Where [`read_document`]'s text came from (v4 `DocumentRead.source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSource {
    /// The store's document (or the file on disk).
    Document,
    /// An uploaded native-text blob's own bytes.
    BlobBytes,
    /// A converted blob's extracted text, which is not the file's own bytes.
    BlobText,
}

impl DocumentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentSource::Document => "document",
            DocumentSource::BlobBytes => "blob-bytes",
            DocumentSource::BlobText => "blob-text",
        }
    }
}

/// What [`read_document`] found (v4 `DocumentRead`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRead {
    pub content: String,
    pub mtime_ms: i64,
    pub size: i64,
    pub source: DocumentSource,
    /// The blob's stored MIME type, for `blob-text` (v4 spreads it only when
    /// truthy).
    pub stored_mime_type: Option<String>,
}

/// The ONE `ENOENT` test (v4 `error.code === 'ENOENT'`): v5's read dispatch
/// carries the code as the message's `ENOENT:` prefix — the database store's
/// re-shaped NOT_FOUND (`ENOENT: database document not found at <rel>`) and
/// the disk read's Node-shaped `ENOENT: no such file or directory, …` alike.
pub fn is_enoent(message: &str) -> bool {
    message.starts_with("ENOENT")
}

impl DocError {
    /// Whether this is a read's absence (v4's `ENOENT`-coded error).
    pub fn is_enoent(&self) -> bool {
        matches!(self, DocError::Missing(m) if is_enoent(m))
    }
}

/// v4 `readDocument` — the document (or disk file) at the path, else — in a
/// database store — the uploaded blob there. Absence is
/// [`DocError::Missing`] carrying the ORIGINAL `ENOENT: …` message (v4
/// rethrows the original error when no blob is there); every other failure is
/// [`DocError::Store`].
pub fn read_document(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<DocumentRead, DocError> {
    let error = match read_file_with_mtime(mount, resolved) {
        Ok(doc) => {
            return Ok(DocumentRead {
                content: doc.content,
                mtime_ms: doc.mtime_ms,
                size: doc.size,
                source: DocumentSource::Document,
                stored_mime_type: None,
            })
        }
        Err(e) => e,
    };
    if !is_enoent(&error) {
        return Err(DocError::Store(error));
    }
    let mp = match (
        resolved.mount_type.as_deref(),
        resolved.mount_point_id.as_deref(),
    ) {
        (Some("database"), Some(mp)) if !mp.is_empty() => mp,
        _ => return Err(DocError::Missing(error)),
    };
    let Some(blob) =
        database_store::read_database_blob_for_document(mount, mp, &resolved.relative_path)
            .map_err(|e| DocError::Store(crate::tools::doc_edit::shared::store_error_message(e)))?
    else {
        return Err(DocError::Missing(error));
    };
    let source = match blob.source {
        BlobSource::BlobBytes => DocumentSource::BlobBytes,
        BlobSource::BlobText => DocumentSource::BlobText,
    };
    tracing::debug!(
        target: "quilltap::documents",
        mountPointId = %mp,
        relativePath = %resolved.relative_path,
        source = source.as_str(),
        size = blob.size,
        "readDocument: no document at path; reading the uploaded blob"
    );
    Ok(DocumentRead {
        content: blob.content,
        mtime_ms: blob.mtime_ms,
        size: blob.size,
        source,
        stored_mime_type: blob.stored_mime_type.filter(|m| !m.is_empty()),
    })
}

/// What sits at a path (v4 `statDocument`'s `'file' | 'folder'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    File,
    Folder,
}

/// v4 `statDocument` — a `File` (a document in a database store, any regular
/// file on disk), a `Folder` (a database folder row, or anything on disk that
/// is not a regular file), or nothing (`None`). **An uploaded blob is not a
/// document here**: it can be read and deleted, but not moved. Errors other
/// than absence (permissions, I/O) bubble (R-4).
pub fn stat_document(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<Option<DocKind>, DocError> {
    if resolved.mount_type.as_deref() == Some("database") {
        let mp = match resolved.mount_point_id.as_deref() {
            Some(mp) if !mp.is_empty() => mp,
            _ => return Err(DocError::Store(MISSING_MOUNT_POINT_ID.to_string())),
        };
        if database_store::database_document_exists(mount, mp, &resolved.relative_path)? {
            return Ok(Some(DocKind::File));
        }
        if database_store::database_folder_exists(mount, mp, &resolved.relative_path)? {
            return Ok(Some(DocKind::Folder));
        }
        return Ok(None);
    }
    match stat_resolved_file(resolved) {
        Ok(None) => Ok(None),
        Ok(Some(s)) => Ok(Some(if s.is_file {
            DocKind::File
        } else {
            DocKind::Folder
        })),
        Err(e) => Err(DocError::Store(super::node_fs_error_message(
            &e,
            "stat",
            &resolved.absolute_path,
            None,
        ))),
    }
}

/// v4 `documentExists` — whether a path is taken. A database store answers
/// for its documents; on disk, anything at the path counts (a directory too),
/// so a caller about to create a file never writes over one.
pub fn document_exists(mount: &Connection, resolved: &ResolvedPath) -> Result<bool, DocError> {
    let kind = stat_document(mount, resolved)?;
    if resolved.mount_type.as_deref() == Some("database") {
        return Ok(kind == Some(DocKind::File));
    }
    Ok(kind.is_some())
}
