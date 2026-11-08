//! The mount-files listing surface (P4.6v) — v4's
//! `GET /api/v1/mount-points/[id]/files` (`files/route.ts`). Returns
//! `{ files, folders }`: `files` is the store browser's listing
//! (`docMountFileLinks.listForBrowser` — v4 `aec626eff`, P4.D381: the
//! `DocMountBrowserEntry` rows, no `extractedText`, with `allow*`,
//! `linkGroupId` and `linkCount`); `folders` merges the
//! `doc_mount_folders` path set with, for filesystem/obsidian mounts, the
//! on-disk directory tree (`listFilesystemFolders`) so empty folders show.
//!
//! Lane C's DocumentPicker consumes this listing.

use std::collections::BTreeSet;

use crate::db::doc_mount_file_links::{DocMountBrowserEntry, DocMountFileLinksRepository};
use crate::db::doc_mount_folders::DocMountFoldersRepository;
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::runtime::Db;

use super::read_file::MountFileError;

/// The `{ files, folders }` list result.
pub struct MountFilesList {
    pub files: Vec<DocMountBrowserEntry>,
    pub folders: Vec<String>,
}

impl MountFilesList {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "files": self.files,
            "folders": self.folders,
        })
    }
}

/// v4 `matchesPattern` (`scanner.ts`): `*.ext` matches by suffix; otherwise any
/// path segment matching the pattern exactly. Segments split on the posix
/// separator (`/`).
pub fn matches_pattern(relative_path: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        // `*.ext` → endsWith(pattern.substring(1)).
        return relative_path.ends_with(suffix);
    }
    relative_path.split('/').any(|seg| seg == pattern)
}

/// v4 `listFilesystemFolders`: recursively enumerate directories under
/// `base_path`, skipping symlinks and excluded dirs, returning posix relative
/// paths (a directory is listed before its children — DFS pre-order).
pub fn list_filesystem_folders(base_path: &str, exclude_patterns: &[String]) -> Vec<String> {
    let mut results = Vec::new();
    walk_folders(base_path, exclude_patterns, "", &mut results);
    results
}

/// Node's `fs.readdir` ORDER (P4.D381, measured): libuv's `scandir` sorts the
/// entries by `strcmp` of their names (`uv__fs_scandir_sort`) — byte order —
/// where `std::fs::read_dir` answers the filesystem's raw order (APFS hands
/// back e.g. `z.xyz` before `blank.md`). v4's walkers all read through
/// `fs.readdir` (`lib/files/walk.ts`'s `walkTree`), so every v5 twin walks in
/// this order: the scanner's `walk_directory` (which fixes the link-insertion
/// order a no-`ORDER BY` listing reads back), `list_filesystem_folders`, and
/// the folder ops' stray scan.
pub(crate) fn read_dir_node_order(path: &str) -> std::io::Result<Vec<std::fs::DirEntry>> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(path)?.flatten().collect();
    entries.sort_by(|a, b| {
        a.file_name()
            .as_encoded_bytes()
            .cmp(b.file_name().as_encoded_bytes())
    });
    Ok(entries)
}

fn walk_folders(
    base_path: &str,
    exclude_patterns: &[String],
    current_relative: &str,
    results: &mut Vec<String>,
) {
    let current_absolute = if current_relative.is_empty() {
        base_path.to_string()
    } else {
        format!("{base_path}/{current_relative}")
    };
    let entries = match read_dir_node_order(&current_absolute) {
        Ok(e) => e,
        Err(_) => return, // v4 logs + returns [] on unreadable dirs.
    };
    for entry in entries {
        // v4 skips symlinks and non-directories.
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative_native = if current_relative.is_empty() {
            name.clone()
        } else {
            format!("{current_relative}/{name}")
        };
        if exclude_patterns
            .iter()
            .any(|p| matches_pattern(&relative_native, p))
        {
            continue;
        }
        // path.sep is '/' on posix, so relativeNative is already posix.
        results.push(relative_native.clone());
        walk_folders(base_path, exclude_patterns, &relative_native, results);
    }
}

/// v4 `GET /api/v1/mount-points/[id]/files` handler body.
pub fn mount_files_list(db: &Db, mount_point_id: &str) -> Result<MountFilesList, MountFileError> {
    let id = mount_point_id.to_string();
    let mp: Option<MountServiceInfo> = db.read_mount_index(move |conn| {
        DocMountPointsRepository::new(conn).find_service_info_by_id(&id)
    })?;
    let mp = mp.ok_or_else(|| MountFileError::Other("Mount point not found".to_string()))?;

    let id = mount_point_id.to_string();
    // v4 `files/route.ts:32-35` (`aec626eff`) reads both through fallbacks:
    // `docMountFileLinks.listForBrowser` (`Error listing file links for
    // browser`, `[]`) and `docMountFolders.findByMountPointId` (`Error finding
    // entities by filter`, `[]` — its outer line unreachable). Each WHOLE
    // checkout sits inside its home, as v4's `withRawDb` acquires inside it.
    let files = crate::db::fallback::browser_entries_or_empty(mount_point_id, || {
        db.read_mount_index(move |conn| {
            DocMountFileLinksRepository::new(conn).list_for_browser(&id)
        })
    });

    let id = mount_point_id.to_string();
    let folder_rows = crate::db::fallback::find_by_filter_or_empty("doc_mount_folders", || {
        db.read_mount_index(move |conn| {
            DocMountFoldersRepository::new(conn).find_by_mount_point_id(&id)
        })
    });

    // v4 uses a JS Set (insertion order); we merge into a sorted set so the
    // differential is stable regardless of fs.readdir order across per-side copies.
    let mut folder_set: BTreeSet<String> = BTreeSet::new();
    for row in &folder_rows {
        if !row.path.is_empty() {
            folder_set.insert(row.path.clone());
        }
    }
    // v4: `if (mountType !== 'database' && basePath)` — listFilesystemFolders
    // itself tolerates an unreadable/absent basePath (readdir error → []).
    if mp.mount_type != "database" {
        if let Some(base) = mp.base_path.as_deref() {
            for f in list_filesystem_folders(base, &mp.exclude_patterns) {
                folder_set.insert(f);
            }
        }
    }
    let folders: Vec<String> = folder_set.into_iter().collect();
    tracing::debug!(
        target: "quilltap::mount_index",
        mountPointId = %mount_point_id,
        files = files.len(),
        folders = folders.len(),
        "[Mount Points v1] Listed files for browser"
    );
    Ok(MountFilesList { files, folders })
}
