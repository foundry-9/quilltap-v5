//! One read of a database-backed store: every link row and every folder row,
//! reduced to the [`SyncEntry`] shape the planner takes.
//!
//! Port of v4 `lib/mount-index/sync/walk-store.ts` (`23da0b322`).
//!
//! The sha the store already keeps is the sha of the bytes as they would sit on
//! disk — `doc_mount_files.sha256` is a hard invariant on the content, and for
//! text documents it is `sha256OfString(content)`, which is the sha of the UTF-8
//! bytes a disk copy would hold. So the two sides hash the same thing and
//! nothing here has to read the bytes to compare them. Bytes are fetched only
//! when an action actually needs them.
//!
//! Dot-paths are dropped before planning: a link or folder whose path has a
//! segment beginning with `.` is invisible to the sync in both directions, so it
//! is neither materialised on disk nor deleted from the store.

use rusqlite::{params, Connection};

use super::sidecar::is_sidecar_path;
use super::types::{EntryKind, SyncEntry, SyncEntryMap};
use crate::db::DbError;

/// v4 `hasDotSegment` — true when any segment of a POSIX relative path begins
/// with a dot.
pub fn has_dot_segment(relative_path: &str) -> bool {
    relative_path.split('/').any(|seg| seg.starts_with('.'))
}

pub struct StoreWalkResult {
    pub entries: SyncEntryMap,
    pub warnings: Vec<String>,
    /// Store paths whose own name ends in the sidecar suffix. Excluded from the
    /// entry map and reported as conflicts — they are skipped in both directions
    /// rather than fought over.
    pub reserved_paths: Vec<String>,
}

/// v4 `walkStore(mountPoint)`.
///
/// The two reads are scoped SELECTs rather than the shared repository methods
/// for one reason each, both measured: v5's `LinkRow` does not carry
/// `descriptionUpdatedAt` and its `FolderRow` does not carry `updatedAt`, and
/// both are load-bearing here (the first decides which caption is newer, the
/// second is the folder's `lastModified`). The link SELECT keeps v5's
/// `join_query` access path — same tables, same join, same `WHERE`, no `ORDER
/// BY` — so the scan order is the one every other reader of that join sees, and
/// that order is what the manifest's key order records.
pub fn walk_store(conn: &Connection, mount_point_id: &str) -> Result<StoreWalkResult, DbError> {
    let mut entries = SyncEntryMap::new();
    let warnings: Vec<String> = Vec::new();
    let mut reserved_paths: Vec<String> = Vec::new();

    let mut folder_count = 0usize;
    {
        let mut stmt = conn.prepare(
            "SELECT id, path, createdAt, updatedAt FROM doc_mount_folders WHERE mountPointId = ?1",
        )?;
        let rows = stmt.query_map(params![mount_point_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (id, path, created_at, updated_at) = row?;
            folder_count += 1;
            // The root is the target directory itself (v4's falsy `!folder.path`
            // check, which an empty string satisfies).
            if path.is_empty() {
                continue;
            }
            if has_dot_segment(&path) {
                continue;
            }
            entries.insert(
                path.to_lowercase(),
                SyncEntry {
                    relative_path: path,
                    kind: Some(EntryKind::Folder),
                    last_modified: updated_at,
                    created_at: Some(created_at),
                    folder_id: Some(id),
                    ..SyncEntry::default()
                },
            );
        }
    }

    let mut link_count = 0usize;
    {
        let mut stmt = conn.prepare(
            "SELECT \
               l.id, l.fileId, l.relativePath, l.folderId, l.lastModified, l.createdAt, \
               l.description, l.descriptionUpdatedAt, l.linkGroupId, \
               f.sha256, f.fileSizeBytes, f.fileType \
             FROM doc_mount_file_links l \
             JOIN doc_mount_files f ON f.id = l.fileId \
             WHERE l.mountPointId = ?1",
        )?;
        let rows = stmt.query_map(params![mount_point_id], |row| {
            Ok(StoreLink {
                id: row.get(0)?,
                file_id: row.get(1)?,
                relative_path: row.get(2)?,
                folder_id: row.get(3)?,
                last_modified: row.get(4)?,
                created_at: row.get(5)?,
                description: row.get(6)?,
                description_updated_at: row.get(7)?,
                link_group_id: row.get(8)?,
                sha256: row.get(9)?,
                file_size_bytes: row.get(10)?,
                file_type: row.get(11)?,
            })
        })?;
        for row in rows {
            let link = row?;
            link_count += 1;
            if has_dot_segment(&link.relative_path) {
                continue;
            }

            // A store file whose own name ends in `.description.md` would
            // collide with the sidecar convention: the disk walk cannot tell the
            // two apart, and a round trip would attach it to a partner that does
            // not want it.
            if is_sidecar_path(&link.relative_path) {
                reserved_paths.push(link.relative_path);
                continue;
            }

            entries.insert(
                link.relative_path.to_lowercase(),
                SyncEntry {
                    relative_path: link.relative_path,
                    kind: Some(EntryKind::File),
                    sha256: Some(link.sha256),
                    size_bytes: Some(link.file_size_bytes),
                    last_modified: link.last_modified,
                    created_at: Some(link.created_at),
                    // v4 `link.description ?? ''` — the store side ALWAYS has a
                    // description (possibly empty), which is what distinguishes
                    // it from the disk side's "no sidecar here".
                    description: Some(link.description.unwrap_or_default()),
                    description_updated_at: link.description_updated_at,
                    link_id: Some(link.id),
                    file_id: Some(link.file_id),
                    link_group_id: link.link_group_id,
                    file_type: Some(link.file_type),
                    folder_id: link.folder_id,
                },
            );
        }
    }

    tracing::debug!(
        target: "quilltap::mount_index",
        mount_point_id = %mount_point_id,
        files = link_count,
        folders = folder_count,
        planned = entries.len(),
        "[Sync] Store walk complete",
    );

    Ok(StoreWalkResult {
        entries,
        warnings,
        reserved_paths,
    })
}

struct StoreLink {
    id: String,
    file_id: String,
    relative_path: String,
    folder_id: Option<String>,
    last_modified: String,
    created_at: String,
    description: Option<String>,
    description_updated_at: Option<String>,
    link_group_id: Option<String>,
    sha256: String,
    file_size_bytes: i64,
    file_type: String,
}
