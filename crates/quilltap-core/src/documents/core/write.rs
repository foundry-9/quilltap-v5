//! Writing a document, creating a blank one, and the one refresh every write
//! into a store is followed by — v4 `lib/documents/core/write.ts`.

use rusqlite::Connection;

use crate::doc_edit::path_resolver::ResolvedPath;
use crate::documents::{DocError, MountRefreshScheduler};
use crate::tools::doc_edit::shared::write_file_with_mtime_check;

use super::read::document_exists;

/// How [`after_document_write`]'s chain runs — the caller's cadence (round
/// ruling R-L1). v4 always fires and forgets; v5 has two callers with two
/// shapes and ONE chain
/// ([`crate::services::mount_index::refresh::refresh_after_document_write`]):
#[derive(Clone, Copy)]
pub enum Refresh<'a> {
    /// Document Mode: the existing [`MountRefreshScheduler`] seam (the
    /// production impl queues the chain as its own writer job — v4's
    /// fire-and-forget with the writer channel as the scheduler). `None` keeps
    /// the loud unwired WARN.
    Scheduled(Option<&'a dyn MountRefreshScheduler>),
    /// The `doc_*` tools: the chain runs synchronously inside the tool's own
    /// write closure — the tool context carries no scheduler.
    /// `TOOL_REFRESH_IN_CLOSURE` (P4.D380 item 24): same end state, same
    /// lines, a failure never fails the tool (v4's catch-warn); only the
    /// tool's latency differs.
    Now {
        main: &'a Connection,
        mount: &'a Connection,
    },
}

/// v4 `afterDocumentWrite` — re-index, embed and refresh store stats after a
/// write lands in a store; a failure is logged, never returned.
///
/// "The gate is `mountPointId`, not `scope`: a `scope: 'project'` write into a
/// project's official store needs the same chunk/embed pass as a
/// `document_store` write, or the file is invisible to search until the next
/// periodic scan (#133). Legacy filesystem-only project paths carry no
/// `mountPointId` and are left alone. Hard-linked siblings share the bytes and
/// get their own chunk sets rebuilt." (v4's comment; round ruling R-2.)
pub fn after_document_write(refresh: Refresh<'_>, resolved: &ResolvedPath) {
    let mount_point_id = match resolved.mount_point_id.as_deref() {
        Some(mp) if !mp.is_empty() => mp,
        _ => return,
    };
    tracing::debug!(
        target: "quilltap::documents",
        mountPointId = %mount_point_id,
        relativePath = %resolved.relative_path,
        "afterDocumentWrite: scheduling store refresh"
    );
    match refresh {
        Refresh::Now { main, mount } => {
            let extractor = crate::services::mount_index::converters::default_text_extractor();
            crate::services::mount_index::refresh::refresh_after_document_write_logged(
                main,
                mount,
                mount_point_id,
                &resolved.relative_path,
                &resolved.absolute_path,
                extractor.as_ref(),
            );
        }
        Refresh::Scheduled(Some(scheduler)) => {
            scheduler.schedule_refresh(mount_point_id, Some(&resolved.relative_path));
        }
        Refresh::Scheduled(None) => {
            // The seam is unwired (a bare engine with no host). Loudly note the
            // skip — never a silent no-op.
            tracing::warn!(
                target: "quilltap::documents",
                file_path = %resolved.relative_path,
                mount_point_id = %mount_point_id,
                "MountRefreshScheduler unwired — skipping document-store refresh",
            );
        }
    }
}

/// v4's `isMtimeConflict` — the mtime precondition failure as minted by the
/// store writers, recognised by message (two of v4's three throw plain
/// Errors).
fn is_mtime_conflict(message: &str) -> bool {
    message.contains("mtime mismatch") || message.contains("modified by another process")
}

/// v4 `writeDocument` — write a document's content, with the mtime
/// precondition when `expected_mtime` is given, then [`after_document_write`].
/// A stale mtime surfaces as [`DocError::MtimeMismatch`] carrying the
/// writer's message (v4's `DocumentConflictError`). Creates parent folders
/// (disk) or folder rows (database) as needed. Answers the new mtime.
pub fn write_document(
    main: &Connection,
    mount: &Connection,
    resolved: &ResolvedPath,
    content: &str,
    expected_mtime: Option<i64>,
    refresh: Refresh<'_>,
) -> Result<i64, DocError> {
    let mtime = match write_file_with_mtime_check(main, mount, resolved, content, expected_mtime) {
        Ok(m) => m,
        Err(message) if is_mtime_conflict(&message) => {
            match resolved.mount_point_id.as_deref() {
                Some(mp) => tracing::debug!(
                    target: "quilltap::documents",
                    relativePath = %resolved.relative_path,
                    mountPointId = %mp,
                    error = %message,
                    "writeDocument: mtime conflict"
                ),
                None => tracing::debug!(
                    target: "quilltap::documents",
                    relativePath = %resolved.relative_path,
                    error = %message,
                    "writeDocument: mtime conflict"
                ),
            }
            return Err(DocError::MtimeMismatch(message));
        }
        Err(message) => return Err(DocError::Store(message)),
    };
    // v4 `content.length` — UTF-16 code units.
    let bytes = content.encode_utf16().count() as u64;
    match resolved.mount_point_id.as_deref() {
        Some(mp) => tracing::debug!(
            target: "quilltap::documents",
            relativePath = %resolved.relative_path,
            mountPointId = %mp,
            bytes = bytes,
            "writeDocument: wrote document"
        ),
        None => tracing::debug!(
            target: "quilltap::documents",
            relativePath = %resolved.relative_path,
            bytes = bytes,
            "writeDocument: wrote document"
        ),
    }
    after_document_write(refresh, resolved);
    Ok(mtime)
}

/// What [`ensure_document`] did (v4 `{created: true, mtime} | {created: false}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    /// A blank document was created; its mtime.
    Created(i64),
    /// Something was already there and was left untouched.
    Existing,
}

/// v4 `ensureDocument` — make sure a document exists at the path, creating it
/// blank when nothing is there. Creates through [`write_document`] — never
/// raw `fs` — so a database store gets a document row and every store its
/// refresh (#137).
pub fn ensure_document(
    main: &Connection,
    mount: &Connection,
    resolved: &ResolvedPath,
    refresh: Refresh<'_>,
) -> Result<Ensured, DocError> {
    if document_exists(mount, resolved)? {
        return Ok(Ensured::Existing);
    }
    let mtime = write_document(main, mount, resolved, "", None, refresh)?;
    match resolved.mount_point_id.as_deref() {
        Some(mp) => tracing::debug!(
            target: "quilltap::documents",
            relativePath = %resolved.relative_path,
            mountPointId = %mp,
            "ensureDocument: created a blank document"
        ),
        None => tracing::debug!(
            target: "quilltap::documents",
            relativePath = %resolved.relative_path,
            "ensureDocument: created a blank document"
        ),
    }
    Ok(Ensured::Created(mtime))
}
