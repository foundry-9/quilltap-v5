//! The mount-files listing surface (P4.6v) — v4's
//! `GET /api/v1/mount-points/[id]/files` (`files/route.ts`). Returns
//! `{ files, folders }`: `files` is the full `DocMountFileLinkWithContent` set
//! (`docMountFiles.findByMountPointId`); `folders` merges the
//! `doc_mount_folders` path set with, for filesystem/obsidian mounts, the
//! on-disk directory tree (`listFilesystemFolders`) so empty folders show.
//!
//! Lane C's DocumentPicker consumes this listing.

use std::collections::BTreeSet;

use crate::db::doc_mount_files::DocMountFilesRepository;
use crate::db::doc_mount_folders::DocMountFoldersRepository;
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::runtime::Db;

use super::read_file::MountFileError;

/// The `{ files, folders }` list result.
pub struct MountFilesList {
    pub files: Vec<serde_json::Value>,
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
    let entries = match std::fs::read_dir(&current_absolute) {
        Ok(e) => e,
        Err(_) => return, // v4 logs + returns [] on unreadable dirs.
    };
    for entry in entries.flatten() {
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
    let files = db.read_mount_index(move |conn| {
        DocMountFilesRepository::new(conn).find_links_with_content_json_by_mount_point_id(&id)
    })?;

    let id = mount_point_id.to_string();
    let folder_rows = db.read_mount_index(move |conn| {
        DocMountFoldersRepository::new(conn).find_by_mount_point_id(&id)
    })?;

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
    Ok(MountFilesList {
        files,
        folders: folder_set.into_iter().collect(),
    })
}
