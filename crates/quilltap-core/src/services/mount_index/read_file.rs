//! Port of v4 `lib/mount-index/read-file.ts` — the canonical mount-point read
//! helpers backing the per-file REST item route's GET.
//!
//! Resolves a `(mountPointId, relativePath)` to either a UTF-8 text envelope or
//! a base64 payload, across all storage shapes: filesystem mounts (bytes on
//! disk via `std::fs`, matching v4's `fs.readFile`), database documents
//! (`doc_mount_documents`), and database blobs (`doc_mount_blobs`).

use sha2::{Digest, Sha256};

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{sha256_of_string, DocMountFileLinksRepository};
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::tools::doc_edit::shared::is_text_file;

use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::file_ops::resolve_fs_absolute;
use super::path_utils::{detect_native_text, mime_for_extension, path_extname};

/// The service-layer error union: a typed `FileOpError` (mapped to HTTP status
/// via `file_op_status`) or an underlying DB error.
#[derive(Debug)]
pub enum MountFileError {
    FileOp(FileOpError),
    /// A typed `DatabaseStoreError` (the folder ops delegate to
    /// `database_store`; its codes ride `fileOpStatus` too — P4.6y).
    Store(crate::db::database_store::DatabaseStoreError),
    Db(DbError),
    /// An untyped failure (e.g. a non-ENOENT fs error) — v4 rethrows the raw
    /// error, which `fileOpStatus` maps to a 500 via its default arm.
    Other(String),
}

impl From<FileOpError> for MountFileError {
    fn from(e: FileOpError) -> Self {
        MountFileError::FileOp(e)
    }
}
impl From<DbError> for MountFileError {
    fn from(e: DbError) -> Self {
        MountFileError::Db(e)
    }
}
impl std::fmt::Display for MountFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountFileError::FileOp(e) => write!(f, "{e}"),
            MountFileError::Store(e) => write!(f, "{e}"),
            MountFileError::Db(e) => write!(f, "{e}"),
            MountFileError::Other(m) => write!(f, "{m}"),
        }
    }
}

fn sha256_of_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// v4 `FileEncoding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEncoding {
    Utf8,
    Base64,
}

impl FileEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            FileEncoding::Utf8 => "utf-8",
            FileEncoding::Base64 => "base64",
        }
    }
}

/// v4 `ReadMountFileOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadMountFileOptions {
    pub encoding: Option<FileEncoding>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// v4 `MountFileBytes`.
pub struct MountFileBytes {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub file_type: String,
}

/// v4 `ReadMountFileResult` (serialized camelCase in the response envelope).
#[derive(Debug, Clone, PartialEq)]
pub struct ReadMountFileResult {
    pub mount_point_id: String,
    pub relative_path: String,
    pub encoding: FileEncoding,
    pub content: String,
    pub mtime: i64,
    pub sha256: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub file_type: String,
    pub total_lines: Option<i64>,
    pub truncated: Option<bool>,
}

impl ReadMountFileResult {
    /// The JSON envelope v4's route returns (`readMountFile` result serialized).
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("mountPointId".into(), self.mount_point_id.clone().into());
        obj.insert("relativePath".into(), self.relative_path.clone().into());
        obj.insert("encoding".into(), self.encoding.as_str().into());
        obj.insert("content".into(), self.content.clone().into());
        obj.insert("mtime".into(), self.mtime.into());
        obj.insert("sha256".into(), self.sha256.clone().into());
        obj.insert("sizeBytes".into(), self.size_bytes.into());
        obj.insert("mimeType".into(), self.mime_type.clone().into());
        obj.insert("fileType".into(), self.file_type.clone().into());
        if let Some(t) = self.total_lines {
            obj.insert("totalLines".into(), t.into());
        }
        if let Some(t) = self.truncated {
            obj.insert("truncated".into(), t.into());
        }
        serde_json::Value::Object(obj)
    }
}

/// v4 `fileTypeForPath`.
fn file_type_for_path(relative_path: &str) -> String {
    let ext = path_extname(relative_path).to_lowercase();
    if ext == ".pdf" {
        return "pdf".to_string();
    }
    if ext == ".docx" {
        return "docx".to_string();
    }
    detect_native_text(relative_path)
        .map(|t| t.as_str().to_string())
        .unwrap_or_else(|| "blob".to_string())
}

/// v4 `isTextLike`: whether a file should default to a UTF-8 text read.
fn is_text_like(relative_path: &str, file_type: &str) -> bool {
    match file_type {
        "markdown" | "txt" | "json" | "jsonl" => true,
        "pdf" | "docx" | "blob" => false,
        _ => is_text_file(relative_path),
    }
}

fn load_mount(db: &Db, mount_point_id: &str) -> Result<MountServiceInfo, MountFileError> {
    let id = mount_point_id.to_string();
    let mp = db.read_mount_index(move |conn| {
        DocMountPointsRepository::new(conn).find_service_info_by_id(&id)
    })?;
    mp.ok_or_else(|| {
        MountFileError::FileOp(FileOpError::new(
            format!("Mount point not found: {mount_point_id}"),
            FileOpErrorCode::MountNotFound,
        ))
    })
}

/// v4 `readMountFileBytes`: raw bytes + metadata for a mount file, regardless of
/// storage shape. `rel` must already be normalised (the callers normalise).
///
/// Thin wrapper over [`read_mount_file_bytes_conn`]: it takes the mount-index
/// connection ONCE for the whole read rather than per query, so the link row and
/// the content it points at are read against the same connection.
pub fn read_mount_file_bytes(
    db: &Db,
    mp: &MountServiceInfo,
    rel: &str,
) -> Result<MountFileBytes, MountFileError> {
    match db.read_mount_index(|conn| Ok(read_mount_file_bytes_conn(conn, mp, rel))) {
        Ok(inner) => inner,
        Err(e) => Err(MountFileError::Db(e)),
    }
}

/// [`read_mount_file_bytes`] against an already-held mount-index connection.
///
/// The `_conn` twin exists because callers that already own a `&Connection` —
/// Pascal's definition loader runs inside one read (P4.D30) — cannot re-enter
/// the pool. Same dispatch, same errors; only the plumbing differs.
pub fn read_mount_file_bytes_conn(
    conn: &rusqlite::Connection,
    mp: &MountServiceInfo,
    rel: &str,
) -> Result<MountFileBytes, MountFileError> {
    if mp.is_filesystem_mount() {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, rel)?;
        match std::fs::read(&abs) {
            Ok(bytes) => {
                let sha = sha256_of_bytes(&bytes);
                let size = bytes.len() as i64;
                Ok(MountFileBytes {
                    bytes,
                    mime_type: mime_for_extension(rel).to_string(),
                    sha256: sha,
                    size_bytes: size,
                    file_type: file_type_for_path(rel),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(MountFileError::FileOp(FileOpError::new(
                    format!("File not found: {rel}"),
                    FileOpErrorCode::SourceNotFound,
                )))
            }
            Err(e) => Err(MountFileError::Other(e.to_string())),
        }
    } else {
        // Database mount: the link row tells us documents (text) vs blobs.
        let link =
            DocMountFileLinksRepository::new(conn).find_by_mount_point_and_path(&mp.id, rel)?;
        let link = link.ok_or_else(|| {
            MountFileError::FileOp(FileOpError::new(
                format!("File not found: {rel}"),
                FileOpErrorCode::SourceNotFound,
            ))
        })?;

        if matches!(
            link.file_type.as_str(),
            "markdown" | "txt" | "json" | "jsonl"
        ) {
            let content =
                DocMountDocumentsRepository::new(conn).find_by_mount_point_and_path(&mp.id, rel)?;
            let content = content.ok_or_else(|| {
                MountFileError::FileOp(FileOpError::new(
                    format!("Document content missing: {rel}"),
                    FileOpErrorCode::SourceNotFound,
                ))
            })?;
            let bytes = content.as_bytes().to_vec();
            let size = bytes.len() as i64;
            Ok(MountFileBytes {
                bytes,
                mime_type: mime_for_extension(rel).to_string(),
                sha256: sha256_of_string(&content),
                size_bytes: size,
                file_type: link.file_type,
            })
        } else {
            let bytes = DocMountBlobsRepository::new(conn).read_data_by_file_id(&link.file_id)?;
            let bytes = bytes.ok_or_else(|| {
                MountFileError::FileOp(FileOpError::new(
                    format!("Blob content missing: {rel}"),
                    FileOpErrorCode::SourceNotFound,
                ))
            })?;
            let meta =
                DocMountBlobsRepository::new(conn).find_by_mount_point_and_path(&mp.id, rel)?;
            let (mime, sha, size) = match meta {
                Some(m) => (m.stored_mime_type, m.sha256, m.size_bytes),
                None => (
                    mime_for_extension(rel).to_string(),
                    sha256_of_bytes(&bytes),
                    bytes.len() as i64,
                ),
            };
            Ok(MountFileBytes {
                bytes,
                mime_type: mime,
                sha256: sha,
                size_bytes: size,
                file_type: link.file_type,
            })
        }
    }
}

/// v4 `readMountFile`: read a mount file into a JSON-friendly envelope. `db`
/// closures resolve the mount + link; the fs read is `std::fs`.
pub fn read_mount_file(
    db: &Db,
    mount_point_id: &str,
    relative_path: &str,
    options: ReadMountFileOptions,
) -> Result<ReadMountFileResult, MountFileError> {
    let mp = load_mount(db, mount_point_id)?;
    let rel = super::path_utils::normalise_relative_path(relative_path)?;

    let raw = read_mount_file_bytes(db, &mp, &rel)?;

    // mtime: filesystem stat, or the link's lastModified for database mounts.
    let mut mtime: i64 = crate::clock::now_unix_ms();
    if mp.is_filesystem_mount() {
        if let Ok(abs) = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &rel) {
            if let Ok(meta) = std::fs::metadata(&abs) {
                if let Ok(modified) = meta.modified() {
                    if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                        mtime = dur.as_millis() as i64;
                    }
                }
            }
        }
    } else {
        let id = mp.id.clone();
        let relc = rel.clone();
        let link = db.read_mount_index(move |conn| {
            DocMountFileLinksRepository::new(conn).find_by_mount_point_and_path(&id, &relc)
        })?;
        if let Some(l) = link {
            if !l.last_modified.is_empty() {
                if let Some(ms) = crate::clock::iso_to_ms(&l.last_modified) {
                    mtime = ms;
                }
            }
        }
    }

    let wants_text = options.encoding == Some(FileEncoding::Utf8)
        || (options.encoding.is_none() && is_text_like(&rel, &raw.file_type));

    if !wants_text {
        return Ok(ReadMountFileResult {
            mount_point_id: mp.id.clone(),
            relative_path: rel,
            encoding: FileEncoding::Base64,
            content: base64_encode(&raw.bytes),
            mtime,
            sha256: raw.sha256,
            size_bytes: raw.size_bytes,
            mime_type: raw.mime_type,
            file_type: raw.file_type,
            total_lines: None,
            truncated: None,
        });
    }

    // UTF-8 read: v4 `raw.bytes.toString('utf-8')` (lossy for non-UTF-8 bytes).
    let full_text = String::from_utf8_lossy(&raw.bytes).into_owned();
    let all_lines: Vec<&str> = full_text.split('\n').collect();
    let total_lines = all_lines.len() as i64;

    let mut content = full_text.clone();
    let mut truncated = false;
    if options.offset.is_some() || options.limit.is_some() {
        // v4: start = offset ?? 0; end = limit !== undefined ? start + limit :
        // totalLines. `truncated` compares the UNCLAMPED end; the slice clamps.
        let start_i = options.offset.unwrap_or(0);
        let end_i = match options.limit {
            Some(l) => start_i + l,
            None => total_lines,
        };
        let s = start_i.clamp(0, total_lines) as usize;
        let e = end_i.clamp(0, total_lines) as usize;
        let (s, e) = (s.min(all_lines.len()), e.min(all_lines.len()));
        content = if e > s {
            all_lines[s..e].join("\n")
        } else {
            String::new()
        };
        truncated = end_i < total_lines;
    }

    Ok(ReadMountFileResult {
        mount_point_id: mp.id.clone(),
        relative_path: rel,
        encoding: FileEncoding::Utf8,
        content,
        mtime,
        sha256: raw.sha256,
        size_bytes: raw.size_bytes,
        mime_type: raw.mime_type,
        file_type: raw.file_type,
        total_lines: Some(total_lines),
        truncated: Some(truncated),
    })
}

/// Base64 (standard, padded) — matching Node `Buffer.toString('base64')`.
fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
