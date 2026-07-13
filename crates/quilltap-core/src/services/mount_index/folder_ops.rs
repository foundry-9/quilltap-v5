//! Port of v4 `lib/mount-index/folder-ops.ts` — folder delete + move across
//! both storage types. Database-backed mounts delegate to the proven
//! `database_store` folder functions; filesystem mounts operate on disk and
//! reconcile the affected link rows so search/read keep resolving. Folder
//! *create* lives with the folders route (`scanner::create_filesystem_folder`
//! / `database_store::create_database_folder`).

use rusqlite::Connection;

use crate::db::database_store::{delete_database_folder, move_database_folder, StoreError};
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkUpdate};
use crate::db::doc_mount_points::MountServiceInfo;

use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::file_ops::resolve_fs_absolute;
use super::path_utils::normalise_relative_path;
use super::read_file::MountFileError;
use super::reindex_file::node_basename;

fn load_mount(conn: &Connection, mount_point_id: &str) -> Result<MountServiceInfo, MountFileError> {
    let mp = crate::db::doc_mount_points::DocMountPointsRepository::new(conn)
        .find_service_info_by_id(mount_point_id)?;
    mp.ok_or_else(|| {
        MountFileError::FileOp(FileOpError::new(
            format!("Mount point not found: {mount_point_id}"),
            FileOpErrorCode::MountNotFound,
        ))
    })
}

fn store_err(e: StoreError) -> MountFileError {
    match e {
        StoreError::Store(s) => MountFileError::Store(s),
        StoreError::Db(d) => MountFileError::Db(d),
    }
}

/// v4 `deleteFolder` — delete an EMPTY folder only (parallels the file
/// delete's safety; callers must clear contents first). Returns
/// `{mountPointId, path}`.
pub fn delete_folder(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<serde_json::Value, MountFileError> {
    let mp = load_mount(conn, mount_point_id)?;
    let rel = normalise_relative_path(relative_path)?;

    if !mp.is_filesystem_mount() {
        // Throws DatabaseStoreError NOT_FOUND / NOT_EMPTY → mapped by fileOpStatus.
        delete_database_folder(conn, &mp.id, &rel).map_err(store_err)?;
        return Ok(serde_json::json!({ "mountPointId": mp.id, "path": rel }));
    }

    let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &rel)?;
    let entries = match std::fs::read_dir(&abs) {
        Ok(e) => e.count(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Folder not found: {rel}"),
                FileOpErrorCode::SourceNotFound,
            )));
        }
        Err(e) => return Err(MountFileError::Other(e.to_string())),
    };
    if entries > 0 {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Folder is not empty: {rel}. Only empty folders can be deleted."),
            FileOpErrorCode::Conflict,
        )));
    }
    std::fs::remove_dir(&abs).map_err(|e| MountFileError::Other(e.to_string()))?;
    Ok(serde_json::json!({ "mountPointId": mp.id, "path": rel }))
}

/// v4 `moveFolder` — move/rename a folder and everything under it (same-mount
/// only). Filesystem mounts rename on disk, then rewrite the affected links'
/// path prefixes so search/read keep resolving until the next full scan.
/// Returns `{mountPointId, fromPath, toPath}`.
pub fn move_folder(
    conn: &Connection,
    mount_point_id: &str,
    from_path: &str,
    to_path: &str,
) -> Result<serde_json::Value, MountFileError> {
    let mp = load_mount(conn, mount_point_id)?;
    let from_rel = normalise_relative_path(from_path)?;
    let to_rel = normalise_relative_path(to_path)?;

    if from_rel == to_rel {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Source and destination are the same folder: {to_rel}"),
            FileOpErrorCode::InvalidPath,
        )));
    }

    if !mp.is_filesystem_mount() {
        move_database_folder(conn, &mp.id, &from_rel, &to_rel).map_err(store_err)?;
        return Ok(serde_json::json!({
            "mountPointId": mp.id,
            "fromPath": from_rel,
            "toPath": to_rel,
        }));
    }

    let from_abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &from_rel)?;
    let to_abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &to_rel)?;

    if !std::path::Path::new(&from_abs).exists() {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Folder not found: {from_rel}"),
            FileOpErrorCode::SourceNotFound,
        )));
    }
    if std::path::Path::new(&to_abs).exists() {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination already exists: {to_rel}"),
            FileOpErrorCode::DestExists,
        )));
    }

    if let Some(parent) = std::path::Path::new(&to_abs).parent() {
        std::fs::create_dir_all(parent).map_err(|e| MountFileError::Other(e.to_string()))?;
    }
    std::fs::rename(&from_abs, &to_abs).map_err(|e| MountFileError::Other(e.to_string()))?;

    // Rewrite the affected link rows' path prefixes (v4's why-comment: fs links
    // carry the path the scanner indexed; the prefix rewrite keeps them pointing
    // at the moved bytes until the next full scan).
    let links_repo = DocMountFileLinksRepository::new(conn);
    let old_prefix = format!("{from_rel}/");
    let new_prefix = format!("{to_rel}/");
    let links = links_repo.find_by_mount_point_id(&mp.id)?;
    for link in &links {
        let Some(rest) = link.relative_path.strip_prefix(&old_prefix) else {
            continue;
        };
        let new_path = format!("{new_prefix}{rest}");
        links_repo.update(
            &link.id,
            &LinkUpdate {
                relative_path: Some(new_path.clone()),
                file_name: Some(node_basename(&new_path).to_string()),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        // v4 emitDocumentMoved — the watcher deferral (see file_ops header).
    }

    Ok(serde_json::json!({
        "mountPointId": mp.id,
        "fromPath": from_rel,
        "toPath": to_rel,
    }))
}
