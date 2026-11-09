//! Renaming or moving a document within its store, and the rename-target math
//! — v4 `lib/documents/core/move.ts`. (The `chat_documents` sweeps that keep
//! open documents pointing at the new path live beside them —
//! [`super::super::sweep`].)

use rusqlite::Connection;

use crate::db::database_store::{self, DbStoreErrorCode, StoreError};
use crate::doc_edit::path_resolver::{move_resolved_file, ResolvedPath};
use crate::documents::{node_basename, node_dirname, node_extname, DocError};
use crate::inventory::lint_scope::TitleLintScope;

use super::read::stat_document;
use super::write::{after_document_write, Refresh};

/// v4 `RenameDocumentResult` — `{ok: true}` or one of three refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameOutcome {
    Moved,
    SourceMissing,
    SourceNotAFile,
    DestinationTaken,
}

/// v4 `isCaseOnlyRename` — two paths that differ only in letter case: a legal
/// rename that a case-insensitive existence probe would report as a conflict
/// with itself. (JS `toLowerCase` on both — `str::to_lowercase` is
/// byte-identical, Phase 1.)
fn is_case_only_rename(from: &str, to: &str) -> bool {
    from != to && from.to_lowercase() == to.to_lowercase()
}

/// v4 `renameDocument` — move a document from one resolved path to another in
/// the same store. A database store moves the link row
/// (`moveDatabaseDocument`); a filesystem path is renamed on disk and then
/// refreshed ([`after_document_write`] on `to`). Never overwrites.
/// `DatabaseStoreError` UNSUPPORTED (a target extension the database store
/// cannot hold) bubbles as [`DocError::Unsupported`]. **No refresh on the
/// database branch** (v4 `move.ts:69-71` calls `afterDocumentWrite` only on
/// disk) — `moveDatabaseDocument` re-chunks the moved link itself.
pub fn rename_document(
    main: &Connection,
    mount: &Connection,
    from: &ResolvedPath,
    to: &ResolvedPath,
    refresh: Refresh<'_>,
) -> Result<RenameOutcome, DocError> {
    match stat_document(mount, from)? {
        None => return Ok(RenameOutcome::SourceMissing),
        Some(super::DocKind::Folder) => return Ok(RenameOutcome::SourceNotAFile),
        Some(super::DocKind::File) => {}
    }

    if let (Some("database"), Some(mp)) =
        (from.mount_type.as_deref(), from.mount_point_id.as_deref())
    {
        if !mp.is_empty() {
            if !is_case_only_rename(&from.relative_path, &to.relative_path) {
                // v4 `statDocument({...to, mountPointId: from.mountPointId})` —
                // ANY hit, a folder too, is taken.
                let mut probe = to.clone();
                probe.mount_point_id = Some(mp.to_string());
                if stat_document(mount, &probe)?.is_some() {
                    return Ok(RenameOutcome::DestinationTaken);
                }
            }
            match database_store::move_database_document(
                mount,
                mp,
                &from.relative_path,
                &to.relative_path,
                TitleLintScope::Unlinted { main },
            ) {
                Ok(()) => {}
                Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::Conflict => {
                    return Ok(RenameOutcome::DestinationTaken)
                }
                Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::Unsupported => {
                    return Err(DocError::Unsupported(s.message))
                }
                Err(e) => {
                    return Err(DocError::Store(
                        crate::tools::doc_edit::shared::store_error_message(e),
                    ))
                }
            }
            tracing::debug!(
                target: "quilltap::documents",
                mountPointId = %mp,
                from = %from.relative_path,
                to = %to.relative_path,
                "renameDocument: moved database document"
            );
            return Ok(RenameOutcome::Moved);
        }
    }

    if !is_case_only_rename(&from.absolute_path, &to.absolute_path)
        && stat_document(mount, to)?.is_some()
    {
        return Ok(RenameOutcome::DestinationTaken);
    }
    move_resolved_file(from, to).map_err(|e| {
        DocError::Store(super::node_fs_error_message(
            &e,
            "rename",
            &from.absolute_path,
            Some(&to.absolute_path),
        ))
    })?;
    tracing::debug!(
        target: "quilltap::documents",
        from = %from.relative_path,
        to = %to.relative_path,
        "renameDocument: moved file"
    );
    after_document_write(refresh, to);
    Ok(RenameOutcome::Moved)
}

/// The rename-target computation result (v4 `RenameTarget`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Ok {
        new_file_path: String,
        new_display_title: String,
    },
    Err {
        reason: String,
    },
}

/// v4 `computeRenameTarget` — turn a user-typed title into the rename target
/// path: the input is the new basename (directory preserved); a missing
/// extension inherits the old one ("backstory" → "backstory.md"). Path
/// separators / `.` / `..` are rejected — a rename within the directory, not a
/// move. **Pure.**
pub fn compute_rename_target(current_file_path: &str, new_title: &str) -> RenameTarget {
    let raw = crate::jsstr::js_trim(new_title);
    if raw.is_empty() {
        return RenameTarget::Err {
            reason: "Name cannot be empty".to_string(),
        };
    }
    if raw.contains('/') || raw.contains('\\') {
        return RenameTarget::Err {
            reason: "Name cannot contain path separators".to_string(),
        };
    }
    if raw == "." || raw == ".." || raw.split(['\\', '/']).any(|s| s == "..") {
        return RenameTarget::Err {
            reason: "Invalid name".to_string(),
        };
    }

    let old_ext = node_extname(current_file_path);
    let old_dir = node_dirname(current_file_path);
    let new_basename = if node_extname(raw).is_empty() {
        format!("{raw}{old_ext}")
    } else {
        raw.to_string()
    };
    let new_file_path = if old_dir == "." || old_dir.is_empty() {
        new_basename
    } else {
        format!("{}/{}", old_dir.replace('\\', "/"), new_basename)
    };
    let new_display_title = node_basename(&new_file_path).to_string();
    RenameTarget::Ok {
        new_file_path,
        new_display_title,
    }
}
