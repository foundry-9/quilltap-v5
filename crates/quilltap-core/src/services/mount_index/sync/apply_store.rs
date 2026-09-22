//! The store half of the applier.
//!
//! Port of v4 `lib/mount-index/sync/apply-store.ts` (`23da0b322`).
//!
//! Every write here goes through an existing chokepoint —
//! `link_document_content` / `link_blob_content`, `ensure_folder_path`,
//! `delete_database_folder`, `delete_with_gc`, `update_description` — and the
//! sync issues no SQL of its own. That is what buys it the store's own
//! post-write behaviour for free: folder rows, hard-link fan-out, the re-chunk
//! pass, and the embedding scheduler all run exactly as they do for any other
//! write, because they *are* the same write.
//!
//! One deliberate departure from a Scriptorium upload:
//!
//!   - **No opinion about metadata.** A byte write passes no `description` /
//!     `extracted_text`, which since bug 155 means "keep what is there" rather
//!     than "blank it".
//!
//! ⚠ **The module's OTHER claim in v4 — "No transcoding: a `.png` pushed from
//! disk stays a `.png` with the same sha" — is STALE PROSE at this pin, and the
//! measurement is recorded here rather than inherited.** `186eb09cb` moved image
//! normalization INSIDE `linkBlobContent` with `normalizeImages` defaulting
//! TRUE, and `apply-store.ts` does not opt out (the only `false` in v4's tree is
//! `import-document-stores.ts:326`). So at `f45a517a9` a synced image IS
//! normalized to WebP, and this port matches by passing `normalize_images: true`
//! rather than by believing the comment. v4's own engine test
//! (`engine.integration.test.ts`, "stores a .png as a .png, byte for byte — no
//! WebP transcode") still passes there for a different reason than it claims:
//! its `PNG` fixture is `Buffer.from('\x89PNG\r\n\x1a\n and then some bytes that
//! are not really a PNG')`, which `sharp` cannot decode, so `transcodeToWebP`
//! hands the original bytes straight back and the normalizer returns its input
//! unchanged. (§R.4(b) of the P4.D210 order predicted exactly this and asked
//! which of the two explanations held; it is this one.)
//!
//! Every content write is compare-and-swap against the sha the planner saw, so a
//! change that landed mid-run is reported rather than overwritten.

use std::fmt;

use rusqlite::Connection;

use super::types::{SyncAction, SyncActionKind};
use crate::db::database_store::delete_database_folder;
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{
    sha256_of_string, DocMountFileLinksRepository, LinkBlobInput, LinkDocumentInput,
};
use crate::db::DbError;
use crate::documents::mime_for_extension;
use crate::jsstr::utf16_len;
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::mount_index::path_utils::detect_native_text;
use crate::services::mount_index::reindex_file::reindex_after_database_write;
use crate::services::mount_index::sync::types::sha256_hex;

/// v4 `StoreRaceError` — a store-side write whose target moved under the
/// planner's feet.
#[derive(Debug, Clone)]
pub struct StoreRaceError {
    pub relative_path: String,
    pub expected: Option<String>,
    pub found: Option<String>,
}

impl fmt::Display for StoreRaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} changed in the store while the sync was running (expected sha {}…, found {}…)",
            self.relative_path,
            slice_12(self.expected.as_deref()),
            slice_12(self.found.as_deref()),
        )
    }
}

/// v4 `expected?.slice(0, 12) ?? 'none'`.
fn slice_12(value: Option<&str>) -> String {
    match value {
        None => "none".to_string(),
        Some(s) => s.chars().take(12).collect(),
    }
}

/// Anything the store applier can fail with. The engine renders it as the
/// action's `error` string, which is v4's `err.message`.
#[derive(Debug)]
pub enum StoreApplyError {
    Race(StoreRaceError),
    Db(DbError),
    /// v4's bare `new Error(...)` arms.
    Message(String),
}

impl fmt::Display for StoreApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Race(e) => e.fmt(f),
            Self::Db(e) => write!(f, "{e}"),
            Self::Message(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for StoreApplyError {}

impl From<DbError> for StoreApplyError {
    fn from(e: DbError) -> Self {
        Self::Db(e)
    }
}

/// v4 `assertUnchanged` — confirm the store still holds what the planner saw.
///
/// `expected` being `None` means the planner saw nothing at this path, so
/// anything there now is a race too.
fn assert_unchanged(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    expected: Option<&str>,
) -> Result<(), StoreApplyError> {
    let links = DocMountFileLinksRepository::new(conn);
    let current = links.find_by_mount_point_and_path(mount_point_id, relative_path)?;
    let found = current.map(|l| l.sha256);
    if found.as_deref() != expected {
        return Err(StoreApplyError::Race(StoreRaceError {
            relative_path: relative_path.to_string(),
            expected: expected.map(str::to_string),
            found,
        }));
    }
    Ok(())
}

/// The two clocks an action can carry, as the store writers take them.
#[derive(Debug, Clone, Default)]
pub struct StoreTimes<'a> {
    pub last_modified: Option<&'a str>,
    pub created_at: Option<&'a str>,
}

impl<'a> StoreTimes<'a> {
    pub fn from_action(action: &'a SyncAction) -> Self {
        Self {
            last_modified: action.last_modified.as_deref(),
            // v4 `times.createdAt ?? undefined` — a JS `null` becomes absent.
            created_at: action.created_at.as_ref().and_then(|v| v.as_deref()),
        }
    }
}

/// v4 `writeStoreFile` — write bytes into the store verbatim, text or binary,
/// and re-chunk. Returns the sha the store now holds at that path.
pub fn write_store_file(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    bytes: &[u8],
    times: &StoreTimes<'_>,
    transcoder: Option<&dyn WebpTranscoder>,
) -> Result<String, StoreApplyError> {
    let links = match transcoder {
        Some(codec) => DocMountFileLinksRepository::with_blob_codec(conn, codec),
        None => DocMountFileLinksRepository::new(conn),
    };
    let file_name = posix_basename(relative_path).to_string();
    // v4 resolves `folderId` here (`ensureFolderPath` on the POSIX dirname) and
    // hands it to the writer. v5's writers derive it themselves from
    // `relative_path` through `ensure_link_folder_id`, which is the same folder
    // chain over the same rows, so the pre-call would only duplicate it — and
    // the inputs carry no `folder_id` field to pass it through.

    if let Some(native) = detect_native_text(relative_path) {
        // v4 `bytes.toString('utf-8')` — lossy.
        let content = String::from_utf8_lossy(bytes).into_owned();
        let content_sha256 = sha256_of_string(&content);
        links.link_document_content(&LinkDocumentInput {
            mount_point_id: mount_point_id.to_string(),
            relative_path: relative_path.to_string(),
            file_name,
            file_type: native.as_str().to_string(),
            content: content.clone(),
            content_sha256: content_sha256.clone(),
            // JS `content.length` — UTF-16 code units.
            plain_text_length: utf16_len(&content) as i64,
            file_size_bytes: content.len() as i64,
            allow_embed: None,
            allow_character_read: None,
            allow_character_write: None,
            last_modified: times.last_modified.map(str::to_string),
            created_at: times.created_at.map(str::to_string),
        })?;
        reindex_after_database_write(conn, mount_point_id, relative_path);
        // v4 `emitDocumentWritten` has no v5 analogue: it schedules the debounced
        // embedding pass, which the in-process runner reaches through the
        // re-index call above.
        return Ok(content_sha256);
    }

    let ext = node_extname(relative_path).to_lowercase();
    let file_type = match ext.as_str() {
        ".pdf" => "pdf",
        ".docx" => "docx",
        _ => "blob",
    };
    let mime = mime_for_extension(relative_path).to_string();

    // No description / extracted_text / extraction_status: the sync has no
    // opinion about the caption it is writing bytes underneath, and since bug
    // 155 an omitted field on the update branch keeps what the store already
    // holds.
    links.link_blob_content(&LinkBlobInput {
        mount_point_id: mount_point_id.to_string(),
        relative_path: relative_path.to_string(),
        file_name: file_name.clone(),
        file_type: Some(file_type.to_string()),
        original_file_name: Some(file_name),
        original_mime_type: Some(mime.clone()),
        stored_mime_type: mime,
        // Advisory; the writer recomputes and is authoritative.
        sha256: sha256_hex(bytes),
        data: bytes.to_vec(),
        // v4 passes no `normalizeImages`, so the writer's `true` default holds
        // (see the module header — the "no transcoding" comment is stale prose).
        normalize_images: true,
        description: None,
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
        last_modified: times.last_modified.map(str::to_string),
        created_at: times.created_at.map(str::to_string),
    })?;

    // pdf/docx carry extractable text, so they chunk like a document does.
    if file_type != "blob" {
        reindex_after_database_write(conn, mount_point_id, relative_path);
    }

    let written = links.find_by_mount_point_and_path(mount_point_id, relative_path)?;
    Ok(written.map(|l| l.sha256).unwrap_or_default())
}

/// v4 `readStoreBytes` — read a store file's bytes back out: text from
/// documents, binaries from blobs.
pub fn read_store_bytes(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<Option<Vec<u8>>, DbError> {
    let links = DocMountFileLinksRepository::new(conn);
    let Some(link) = links.find_by_mount_point_and_path(mount_point_id, relative_path)? else {
        return Ok(None);
    };

    let documents = DocMountDocumentsRepository::new(conn);
    if let Some(content) = documents.find_content_by_file_id(&link.file_id)? {
        return Ok(Some(content.into_bytes()));
    }

    DocMountBlobsRepository::new(conn).read_data_by_file_id(&link.file_id)
}

/// v4 `applyStoreAction` — apply one store-side action.
pub fn apply_store_action(
    conn: &Connection,
    mount_point_id: &str,
    action: &SyncAction,
    bytes: Option<&[u8]>,
    transcoder: Option<&dyn WebpTranscoder>,
) -> Result<(), StoreApplyError> {
    let links = DocMountFileLinksRepository::new(conn);

    match action.kind {
        SyncActionKind::Mkdir => {
            links.ensure_folder_path(mount_point_id, &action.relative_path)?;
            Ok(())
        }

        SyncActionKind::Create | SyncActionKind::Modify => {
            let Some(bytes) = bytes else {
                return Err(StoreApplyError::Message(format!(
                    "No bytes supplied for {} {}",
                    action.kind.as_str(),
                    action.relative_path
                )));
            };
            assert_unchanged(
                conn,
                mount_point_id,
                &action.relative_path,
                action.expected_store_sha256.as_deref(),
            )?;
            write_store_file(
                conn,
                mount_point_id,
                &action.relative_path,
                bytes,
                &StoreTimes::from_action(action),
                transcoder,
            )?;
            Ok(())
        }

        SyncActionKind::Touch => {
            let Some(link_id) = action.link_id.as_deref() else {
                return Err(StoreApplyError::Message(format!(
                    "No link to touch at {}",
                    action.relative_path
                )));
            };
            let times = StoreTimes::from_action(action);
            links.set_link_timestamps(link_id, times.last_modified, times.created_at)?;
            Ok(())
        }

        SyncActionKind::Describe => {
            // A caption for a file this same run has just adopted has no link id
            // yet — the planner could not know one — so resolve by path when it
            // is absent.
            let link = match action.link_id.as_deref() {
                Some(link_id) => links
                    .find_by_id_with_content(link_id)?
                    .map(|l| (l.id, l.file_id)),
                None => links
                    .find_by_mount_point_and_path(mount_point_id, &action.relative_path)?
                    .map(|l| (l.id, l.file_id)),
            };
            let Some((link_id, file_id)) = link else {
                return Err(StoreApplyError::Message(format!(
                    "No link to describe at {}",
                    action.relative_path
                )));
            };
            let blobs = DocMountBlobsRepository::new(conn);
            let Some(blob) = blobs.find_by_file_id(&file_id)? else {
                return Err(StoreApplyError::Message(format!(
                    "No blob row behind {}; cannot set its description",
                    action.relative_path
                )));
            };
            // The three-arg form: the two-arg one picks an arbitrary link off
            // the file row, which for a hard-linked image is the wrong
            // location's caption (v4 bug 157).
            blobs.update_description(
                &blob.id,
                action.description.as_deref().unwrap_or(""),
                &link_id,
            )?;
            Ok(())
        }

        SyncActionKind::Delete => {
            let Some(link_id) = action.link_id.as_deref() else {
                return Err(StoreApplyError::Message(format!(
                    "No link to delete at {}",
                    action.relative_path
                )));
            };
            links.delete_with_gc(link_id)?;
            Ok(())
        }

        SyncActionKind::Rmdir => {
            // Refuses a non-empty folder; the planner has already ordered this
            // after every file deletion beneath it.
            delete_database_folder(conn, mount_point_id, &action.relative_path)
                .map_err(|e| StoreApplyError::Message(e.to_string()))?;
            Ok(())
        }

        other => {
            tracing::debug!(
                target: "quilltap::mount_index",
                kind = %other.as_str(),
                "[Sync] Store applier ignoring action",
            );
            Ok(())
        }
    }
}

// ============================================================================
// Node path helpers
// ============================================================================

/// `path.posix.basename(p)`.
fn posix_basename(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

/// `path.extname(p)` — the last dot in the basename, empty when it leads.
fn node_extname(p: &str) -> String {
    let base = posix_basename(p);
    match base.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => base[i..].to_string(),
    }
}
