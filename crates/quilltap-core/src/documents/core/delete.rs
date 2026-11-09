//! Deleting a document from its store — v4 `lib/documents/core/delete.ts`.

use rusqlite::Connection;

use crate::db::database_store;
use crate::doc_edit::path_resolver::{delete_resolved_file, ResolvedPath};
use crate::documents::DocError;
use crate::inventory::lint_scope::TitleLintScope;

use super::read::{stat_document, DocKind};

/// The outcome of a delete (v4 `'deleted' | 'not-found' | 'not-a-file'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    NotFound,
    NotAFile,
}

/// v4 `deleteDocument` — delete the document at a path. A database store
/// deletes the link (and garbage-collects what it pointed at — an uploaded
/// blob included); a filesystem path is unlinked. `NotAFile` means a folder
/// sits there. **No refresh on delete** (v4 calls none).
pub fn delete_document(
    main: &Connection,
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<DeleteOutcome, DocError> {
    if let (Some("database"), Some(mp)) = (
        resolved.mount_type.as_deref(),
        resolved.mount_point_id.as_deref(),
    ) {
        if !mp.is_empty() {
            let deleted = database_store::delete_database_document(
                mount,
                mp,
                &resolved.relative_path,
                TitleLintScope::Unlinted { main },
            )
            .map_err(|e| DocError::Store(crate::tools::doc_edit::shared::store_error_message(e)))?;
            if deleted {
                tracing::debug!(
                    target: "quilltap::documents",
                    mountPointId = %mp,
                    relativePath = %resolved.relative_path,
                    "deleteDocument: deleted database document"
                );
                return Ok(DeleteOutcome::Deleted);
            }
            return Ok(
                if stat_document(mount, resolved)? == Some(DocKind::Folder) {
                    DeleteOutcome::NotAFile
                } else {
                    DeleteOutcome::NotFound
                },
            );
        }
    }

    match stat_document(mount, resolved)? {
        None => return Ok(DeleteOutcome::NotFound),
        Some(DocKind::Folder) => return Ok(DeleteOutcome::NotAFile),
        Some(DocKind::File) => {}
    }
    delete_resolved_file(resolved).map_err(|e| {
        DocError::Store(super::node_fs_error_message(
            &e,
            "unlink",
            &resolved.absolute_path,
            None,
        ))
    })?;
    tracing::debug!(
        target: "quilltap::documents",
        relativePath = %resolved.relative_path,
        "deleteDocument: deleted file"
    );
    Ok(DeleteOutcome::Deleted)
}
