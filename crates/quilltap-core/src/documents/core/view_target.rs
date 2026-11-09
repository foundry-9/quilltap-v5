//! Which store files Document Mode shows read-only rather than edits — v4
//! `lib/documents/core/view-target.ts` (`5efe53ea7`, contract C14).

use rusqlite::Connection;
use serde::Serialize;

use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::doc_edit::path_resolver::{stat_resolved_file, ResolvedPath};
use crate::documents::{node_basename, DocError};
use crate::services::mount_index::path_utils::detect_native_text;
use crate::tools::doc_edit::shared::is_text_file;

/// v4 `DocumentViewTarget` (`lib/folder-browser/document-actions.ts:139-147`),
/// keys in v4's order; `originalMimeType` is an explicit `null`, never omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentViewTarget {
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size_bytes: i64,
    pub original_mime_type: Option<String>,
    pub updated_at: String,
}

/// v4 `viewTargetFor` — whether a resolved document is one Document Mode
/// shows read-only rather than edits: any file in a document store that is
/// not a text file. "Its bytes are never read as text — the viewer fetches
/// them itself, and no model is handed them through Document Mode." `None`
/// for a text file, for a file outside a store (the legacy project and
/// general areas, which the viewer cannot address), and for a store file that
/// does not exist.
pub fn view_target_for(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<Option<DocumentViewTarget>, DocError> {
    let mount_point_id = match resolved.mount_point_id.as_deref() {
        Some(mp) if !mp.is_empty() => mp,
        _ => return Ok(None),
    };
    // "A database store saves only native text (.md, .txt, .json, .jsonl), so
    // any other text kind there (.yaml, .html, …) is shown, not edited."
    let is_database = resolved.mount_type.as_deref() == Some("database");
    let editable = if is_database {
        detect_native_text(&resolved.relative_path).is_some()
    } else {
        is_text_file(&resolved.relative_path)
    };
    if editable {
        return Ok(None);
    }
    // v4 `docMountFileLinks.findByMountPointAndPath` — the fallback joined read.
    if let Some(link) = DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_and_path_or_none(mount_point_id, &resolved.relative_path)
    {
        // The joined row's `l.updatedAt` (the link's own, not the file's).
        let updated_at: String = mount
            .query_row(
                "SELECT updatedAt FROM doc_mount_file_links WHERE id = ?1",
                [&link.id],
                |r| r.get(0),
            )
            .map_err(|e| DocError::Store(e.to_string()))?;
        return Ok(Some(DocumentViewTarget {
            mount_point_id: mount_point_id.to_string(),
            relative_path: link.relative_path,
            file_name: link.file_name,
            file_type: link.file_type,
            file_size_bytes: link.file_size_bytes,
            original_mime_type: link.original_mime_type,
            updated_at,
        }));
    }
    if is_database || resolved.absolute_path.is_empty() {
        return Ok(None);
    }
    // "A file on disk the store does not index (its kind is outside the
    // include patterns) is still there to look at."
    let stat = stat_resolved_file(resolved).map_err(|e| {
        DocError::Store(super::node_fs_error_message(
            &e,
            "stat",
            &resolved.absolute_path,
            None,
        ))
    })?;
    match stat {
        Some(s) if s.is_file => Ok(Some(DocumentViewTarget {
            mount_point_id: mount_point_id.to_string(),
            relative_path: resolved.relative_path.clone(),
            file_name: node_basename(&resolved.relative_path).to_string(),
            file_type: "blob".to_string(),
            file_size_bytes: s.size as i64,
            original_mime_type: None,
            updated_at: crate::clock::iso_from_unix_ms(s.mtime_ms),
        })),
        _ => Ok(None),
    }
}
