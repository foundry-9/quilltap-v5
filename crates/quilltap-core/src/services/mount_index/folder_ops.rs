//! Port of v4 `lib/mount-index/folder-ops.ts` (at `624d0d25c`: `aec626eff` +
//! `c34986421`, P4.D381) — folder delete, move and copy, across both storage
//! types and (for move and copy) between stores.
//!
//! Database-backed mounts delegate to the tested `database_store` folder
//! functions (explicit `doc_mount_folders` rows). Filesystem mounts operate on
//! disk and reconcile the affected `doc_mount_file_links` rows — and, since
//! `c34986421`, the scanner's `doc_mount_folders` rows — so search/read and the
//! listing keep resolving after the move. Folder *create* lives with the
//! folders route (`scanner::create_filesystem_folder` /
//! `database_store::create_database_folder`).
//!
//! Whole-folder work between stores, and a recursive delete, walk the folder
//! and call the per-file operations (`file_ops` — the PUBLISHED wrappers) — no
//! storage code of their own — so every file keeps the sha-verified,
//! strategy-picking behaviour of a single-file move or copy. A copy or
//! cross-store move that fails part-way is undone before the error is
//! returned.
//!
//! **No outer transaction** (v4 has none): the undo IS the partial-failure
//! semantics. These ops move bytes on disk as well as rows, and an outer
//! transaction would roll the DB rows back while the moved filesystem bytes
//! stayed where they landed — the index and the disk would disagree.
//!
//! v4's service logger is `MountIndex:FolderOps`; v5 logs under the module
//! family's shared target `quilltap::mount_index`. The lines run on the WRITER
//! thread when reached through the action route.

use crate::inventory::lint_scope::TitleLintScope;
use rusqlite::Connection;

use crate::db::database_store::{
    create_database_folder, delete_database_folder, move_database_folder, DbStoreErrorCode,
    StoreError,
};
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkUpdate};
use crate::db::doc_mount_folders::{DmfUpdate, DocMountFoldersRepository};
use crate::db::doc_mount_points::MountServiceInfo;

use super::blob_transcode::WebpTranscoder;
use super::converters::DocumentTextExtractor;
use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::file_ops::{resolve_fs_absolute, CopyOpts};
use super::path_utils::normalise_relative_path;
use super::read_file::MountFileError;
use super::realtime::{emit_document_moved, publish_mount_change};
use super::reindex_file::node_basename;
use serde_json::{json, Value};

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
        // v4 rethrows the refusal, which is neither a FileOpError nor a
        // DatabaseStoreError: the route's catch-all 500 (P4.D340 R-8).
        StoreError::TitleLint(r) => MountFileError::Other(r.message),
        // P4.D377: v4's store throws the shared FileOpError — the route's
        // `fileOpStatus` answers it (INVALID_PATH → 400).
        StoreError::FileOp(e) => MountFileError::FileOp(e),
    }
}

/// The per-file operations' dependencies (v4 reaches them through module
/// state; v5 threads them from the api layer, as `mount_file_copy` does).
pub struct FolderOpDeps<'a> {
    pub main: &'a Connection,
    pub extractor: &'a dyn DocumentTextExtractor,
    pub webp: &'a dyn WebpTranscoder,
}

/// An untyped fs failure (v4 rethrows the raw error → the route's 500 arm).
fn fs_err(e: std::io::Error) -> MountFileError {
    MountFileError::Other(e.to_string())
}

/// v4 `deleteEmptyFolder` — refuses non-empty folders (parallels the file
/// delete's safety): callers must clear contents first. `{mountPointId, path}`.
fn delete_empty_folder(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<Value, MountFileError> {
    let mp = load_mount(conn, mount_point_id)?;
    let rel = normalise_relative_path(relative_path)?;

    if !mp.is_filesystem_mount() {
        // Throws DatabaseStoreError NOT_FOUND / NOT_EMPTY → mapped by fileOpStatus.
        delete_database_folder(conn, &mp.id, &rel).map_err(store_err)?;
        return Ok(json!({ "mountPointId": mp.id, "path": rel }));
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
    std::fs::remove_dir(&abs).map_err(fs_err)?;
    // c34986421: the scanner's row for the folder goes with it, or the listing
    // keeps it.
    let folders = DocMountFoldersRepository::new(conn);
    if let Some(row) = folders.find_by_mount_point_and_path_or_none(&mp.id, &rel) {
        folders.delete(&row.id)?;
        tracing::debug!(
            target: "quilltap::mount_index",
            mountPointId = %mp.id,
            path = %rel,
            "Removed filesystem folder row with the folder"
        );
    }
    Ok(json!({ "mountPointId": mp.id, "path": rel }))
}

/// v4 `moveFolderWithinMount` — move/rename a folder and everything under it,
/// within one store. `{mountPointId, fromPath, toPath}`.
fn move_folder_within_mount(
    main: &Connection,
    conn: &Connection,
    mount_point_id: &str,
    from_path: &str,
    to_path: &str,
) -> Result<Value, MountFileError> {
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
        move_database_folder(
            conn,
            &mp.id,
            &from_rel,
            &to_rel,
            TitleLintScope::Unlinted { main },
        )
        .map_err(store_err)?;
        return Ok(json!({
            "mountPointId": mp.id,
            "fromPath": from_rel,
            "toPath": to_rel,
        }));
    }

    // Filesystem mount: rename on disk, then rewrite the affected link rows so
    // search/read keep resolving. fs links live in doc_mount_file_links with the
    // path the scanner indexed; rewriting the prefix keeps them pointing at the
    // moved bytes until the next full scan.
    let from_abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &from_rel)?;
    let to_abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &to_rel)?;

    if std::fs::metadata(&from_abs).is_err() {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Folder not found: {from_rel}"),
            FileOpErrorCode::SourceNotFound,
        )));
    }
    // Case-only rename (lore → Lore): on a case-insensitive filesystem the
    // destination probe would find the source itself, so skip it — fs.rename
    // handles the casing change in place.
    let case_only_rename = from_rel.to_lowercase() == to_rel.to_lowercase();
    if !case_only_rename && std::fs::metadata(&to_abs).is_ok() {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination already exists: {to_rel}"),
            FileOpErrorCode::DestExists,
        )));
    }

    if let Some(parent) = std::path::Path::new(&to_abs).parent() {
        std::fs::create_dir_all(parent).map_err(fs_err)?;
    }
    std::fs::rename(&from_abs, &to_abs).map_err(fs_err)?;

    let old_prefix = format!("{from_rel}/");
    let new_prefix = format!("{to_rel}/");

    // c34986421: the scanner records a filesystem store's folders as rows too.
    // Carry them along in place (ids kept, so the moved links' folderId stays
    // right), or the old path lingers in the listing beside the new one. The
    // source row is an EXACT (case-sensitive) match, as v4's `f.path ===
    // fromRel` is — a v4 filing candidate (R-6), ported faithfully.
    let folders = DocMountFoldersRepository::new(conn);
    let folder_rows = folders.find_by_mount_point_id_or_empty(&mp.id);
    if let Some(source_row) = folder_rows.iter().find(|f| f.path == from_rel) {
        let dest_dir = node_posix_dirname(&to_rel);
        let parent_id = if dest_dir != "." {
            DocMountFileLinksRepository::new(conn).ensure_folder_path(&mp.id, dest_dir)?
        } else {
            None
        };
        folders.update(
            &source_row.id,
            &DmfUpdate {
                parent_id: parent_id.clone(),
                name: Some(node_basename(&to_rel).to_string()),
                path: Some(to_rel.clone()),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        if parent_id.is_none() {
            // v4's `parentId: null` — `DmfUpdate` cannot spell a NULL, so the
            // top-level arm clears it explicitly.
            conn.execute(
                "UPDATE doc_mount_folders SET parentId = NULL WHERE id = ?1",
                rusqlite::params![source_row.id],
            )
            .map_err(crate::db::DbError::from)?;
        }
        let descendants: Vec<_> = folder_rows
            .iter()
            .filter(|r| r.path.starts_with(&old_prefix))
            .collect();
        for row in &descendants {
            folders.update(
                &row.id,
                &DmfUpdate {
                    path: Some(format!("{new_prefix}{}", &row.path[old_prefix.len()..])),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
        }
        tracing::debug!(
            target: "quilltap::mount_index",
            mountPointId = %mp.id,
            fromPath = %from_rel,
            toPath = %to_rel,
            rows = 1 + descendants.len(),
            "Moved filesystem folder rows with the folder"
        );
    }

    let links_repo = DocMountFileLinksRepository::new(conn);
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
        emit_document_moved(&mp.id, &link.relative_path, &new_path);
    }

    Ok(json!({
        "mountPointId": mp.id,
        "fromPath": from_rel,
        "toPath": to_rel,
    }))
}

/// `path.posix.dirname`.
fn node_posix_dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(0) => "/",
        Some(i) => &p[..i],
        None => ".",
    }
}

// ============================================================================
// Whole-folder walks: recursive delete, copy, move between stores
// ============================================================================

struct FolderTree {
    /// The folder itself exists (a folder row / a directory, or files under it).
    exists: bool,
    /// Every indexed file under the folder, at any depth (repository order).
    files: Vec<String>,
    /// Every subfolder under the folder, at any depth (not the folder itself).
    folders: Vec<String>,
}

/// v4 `under(child, folder)`: the lower-cased child starts with the
/// lower-cased folder plus `/` — case-INSENSITIVE (`str::to_lowercase` is
/// byte-identical to JS's `toLowerCase` for these paths — Phase 1).
fn under(child: &str, folder: &str) -> bool {
    child
        .to_lowercase()
        .starts_with(&format!("{}/", folder.to_lowercase()))
}

fn depth_of(rel: &str) -> usize {
    rel.split('/').count()
}

/// v4 `readFolderTree`.
fn read_folder_tree(
    conn: &Connection,
    mp: &MountServiceInfo,
    rel: &str,
) -> Result<FolderTree, MountFileError> {
    let links = DocMountFileLinksRepository::new(conn).find_by_mount_point_id_or_empty(&mp.id);
    let files: Vec<String> = links
        .into_iter()
        .map(|l| l.relative_path)
        .filter(|p| under(p, rel))
        .collect();
    let (folders, folder_itself) = if mp.is_filesystem_mount() {
        let folders: Vec<String> = super::list::list_filesystem_folders(
            mp.base_path.as_deref().unwrap_or(""),
            &mp.exclude_patterns,
        )
        .into_iter()
        .filter(|f| under(f, rel))
        .collect();
        // v4 `fs.stat(resolveFsAbsolute(mp, rel))`: the resolve throws
        // synchronously (outside the stat's catch); a stat error is `false`.
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, rel)?;
        let itself = std::fs::metadata(&abs).map(|m| m.is_dir()).unwrap_or(false);
        (folders, itself)
    } else {
        let repo = DocMountFoldersRepository::new(conn);
        let folders: Vec<String> = repo
            .find_by_mount_point_id_or_empty(&mp.id)
            .into_iter()
            .map(|r| r.path)
            .filter(|p| under(p, rel))
            .collect();
        let itself = repo
            .find_by_mount_point_and_path_or_none(&mp.id, rel)
            .is_some();
        (folders, itself)
    };
    Ok(FolderTree {
        exists: folder_itself || !files.is_empty(),
        files,
        folders,
    })
}

/// v4 `createFolderAt`.
fn create_folder_at(
    conn: &Connection,
    mp: &MountServiceInfo,
    rel: &str,
) -> Result<(), MountFileError> {
    if mp.is_filesystem_mount() {
        super::scanner::create_filesystem_folder(mp.base_path.as_deref().unwrap_or(""), &mp.id, rel)
            .map_err(|e| match e {
                super::scanner::CreateFolderError::Escapes => {
                    MountFileError::Other("Folder path escapes mount point boundary".to_string())
                }
                super::scanner::CreateFolderError::BasePathUnavailable(b) => {
                    MountFileError::Other(b.to_string())
                }
                super::scanner::CreateFolderError::Io(m) => MountFileError::Other(m),
            })
    } else {
        create_database_folder(conn, &mp.id, rel)?;
        Ok(())
    }
}

/// v4 `unindexedFilesUnder` — files on disk under a filesystem folder that the
/// store does not index (the indexed set compared lowercased).
fn unindexed_files_under(
    mp: &MountServiceInfo,
    rel: &str,
    indexed: &[String],
) -> Result<Vec<String>, MountFileError> {
    let known: std::collections::HashSet<String> =
        indexed.iter().map(|p| p.to_lowercase()).collect();
    let mut found = Vec::new();
    fn walk(
        mp: &MountServiceInfo,
        dir_rel: &str,
        known: &std::collections::HashSet<String>,
        found: &mut Vec<String>,
    ) -> Result<(), MountFileError> {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, dir_rel)?;
        for (name, is_dir) in node_readdir(&abs)? {
            let child_rel = format!("{dir_rel}/{name}");
            if is_dir {
                walk(mp, &child_rel, known, found)?;
            } else if !known.contains(&child_rel.to_lowercase()) {
                found.push(child_rel);
            }
        }
        Ok(())
    }
    walk(mp, rel, &known, &mut found)?;
    Ok(found)
}

/// Node's `fs.readdir(dir, {withFileTypes: true})` order: libuv's `scandir`
/// SORTS the entries (`uv__fs_scandir_sort` = `strcmp` of the names — byte
/// order), where `std::fs::read_dir` answers the filesystem's raw order
/// (measured on APFS: the stray list of `notes/` came back
/// `z.xyz, blank.md`). `(name, is_dir)` pairs.
fn node_readdir(abs: &str) -> Result<Vec<(String, bool)>, MountFileError> {
    let entries =
        super::list::read_dir_node_order(abs).map_err(|e| MountFileError::Other(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|entry| {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (entry.file_name().to_string_lossy().into_owned(), is_dir)
        })
        .collect())
}

/// v4 `removeEmptyFolders` — a folder's (now empty) subfolders deepest first (a
/// STABLE sort by depth descending), then the folder. A filesystem folder with
/// no row of its own is fine to skip once empty of what the store knows
/// (`SOURCE_NOT_FOUND`), as is a database `NOT_FOUND`; anything else is the
/// caller's to hear.
fn remove_empty_folders(
    conn: &Connection,
    mp: &MountServiceInfo,
    rel: &str,
    subfolders: &[String],
) -> Result<(), MountFileError> {
    let mut ordered: Vec<&String> = subfolders.iter().collect();
    ordered.sort_by_key(|f| std::cmp::Reverse(depth_of(f)));
    for folder in ordered
        .into_iter()
        .map(String::as_str)
        .chain(std::iter::once(rel))
    {
        match delete_empty_folder(conn, &mp.id, folder) {
            Ok(_) => {}
            Err(MountFileError::FileOp(e)) if e.code == FileOpErrorCode::SourceNotFound => {}
            Err(MountFileError::Store(e)) if e.code == DbStoreErrorCode::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// v4 `deleteFolder` — without `recursive` the folder must be empty. With it,
/// every indexed file under it goes through the per-file delete (so shared
/// content is reclaimed only with its last link), then its folders, deepest
/// first. A filesystem folder holding files the store does not index is
/// refused before anything is deleted. `{mountPointId, path, deletedFiles}`.
pub fn delete_folder(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    recursive: bool,
) -> Result<Value, MountFileError> {
    if !recursive {
        let result = delete_empty_folder(conn, mount_point_id, relative_path)?;
        let id = result["mountPointId"]
            .as_str()
            .unwrap_or(mount_point_id)
            .to_string();
        publish_mount_change(&id);
        return Ok(json!({
            "mountPointId": result["mountPointId"],
            "path": result["path"],
            "deletedFiles": 0,
        }));
    }
    let mp = load_mount(conn, mount_point_id)?;
    let rel = normalise_relative_path(relative_path)?;
    let tree = read_folder_tree(conn, &mp, &rel)?;
    if !tree.exists {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Folder not found: {rel}"),
            FileOpErrorCode::SourceNotFound,
        )));
    }
    if mp.is_filesystem_mount() {
        let strays = unindexed_files_under(&mp, &rel, &tree.files)?;
        if !strays.is_empty() {
            let shown: Vec<&str> = strays.iter().take(3).map(String::as_str).collect();
            return Err(MountFileError::FileOp(FileOpError::new(
                format!(
                    "Folder holds {} file(s) the store does not index ({}{}); remove them on disk first.",
                    strays.len(),
                    shown.join(", "),
                    if strays.len() > 3 { ", …" } else { "" }
                ),
                FileOpErrorCode::Conflict,
            )));
        }
    }
    for file in &tree.files {
        super::file_ops::delete_file(conn, &mp.id, file)?;
    }
    remove_empty_folders(conn, &mp, &rel, &tree.folders)?;
    publish_mount_change(&mp.id);
    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %mp.id,
        path = %rel,
        files = tree.files.len(),
        folders = tree.folders.len(),
        "Deleted folder recursively"
    );
    Ok(json!({ "mountPointId": mp.id, "path": rel, "deletedFiles": tree.files.len() }))
}

struct TransferPlan {
    source: MountServiceInfo,
    dest: MountServiceInfo,
    from_rel: String,
    to_rel: String,
    tree: FolderTree,
}

impl TransferPlan {
    /// v4 `rebase(p) = toRel + p.slice(fromRel.length)`.
    fn rebase(&self, p: &str) -> String {
        // `slice` by UTF-16 length in v4; the paths share `fromRel`'s prefix
        // (case-insensitively), so the byte offset of the same characters holds.
        let cut = p
            .char_indices()
            .nth(self.from_rel.chars().count())
            .map(|(i, _)| i)
            .unwrap_or(p.len());
        format!("{}{}", self.to_rel, &p[cut..])
    }
}

/// v4 `planFolderTransfer` — load both ends, normalise both paths, and refuse a
/// folder into itself or onto an existing one.
fn plan_folder_transfer(
    conn: &Connection,
    mount_point_id: &str,
    from_path: &str,
    dest_mount_point_id: Option<&str>,
    to_path: &str,
) -> Result<TransferPlan, MountFileError> {
    let source = load_mount(conn, mount_point_id)?;
    let dest = match dest_mount_point_id {
        Some(d) if !d.is_empty() && d != source.id => load_mount(conn, d)?,
        _ => source.clone(),
    };
    let from_rel = normalise_relative_path(from_path)?;
    let to_rel = normalise_relative_path(to_path)?;
    if dest.id == source.id
        && (from_rel.to_lowercase() == to_rel.to_lowercase() || under(&to_rel, &from_rel))
    {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("A folder cannot be put inside itself: {to_rel}"),
            FileOpErrorCode::InvalidPath,
        )));
    }
    let tree = read_folder_tree(conn, &source, &from_rel)?;
    if !tree.exists {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Folder not found: {from_rel}"),
            FileOpErrorCode::SourceNotFound,
        )));
    }
    if read_folder_tree(conn, &dest, &to_rel)?.exists {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination already exists: {to_rel}"),
            FileOpErrorCode::DestExists,
        )));
    }
    Ok(TransferPlan {
        source,
        dest,
        from_rel,
        to_rel,
        tree,
    })
}

/// v4 `makeDestFolders` — the destination folder and its subfolders,
/// shallowest first (a STABLE sort); answers what was made.
fn make_dest_folders(
    conn: &Connection,
    dest: &MountServiceInfo,
    to_rel: &str,
    subfolders: &[String],
) -> Result<Vec<String>, MountFileError> {
    let mut sorted: Vec<&String> = subfolders.iter().collect();
    sorted.sort_by_key(|f| depth_of(f));
    let mut made = Vec::new();
    for folder in std::iter::once(to_rel).chain(sorted.into_iter().map(String::as_str)) {
        create_folder_at(conn, dest, folder)?;
        made.push(folder.to_string());
    }
    Ok(made)
}

/// v4 `copyFolder` — copy a folder and everything in it, to another path in
/// the same store or into another store. Each file is a per-file copy
/// (dedup-linked where both ends are database stores, a byte copy across
/// storage types); empty subfolders are made too. Refused onto an existing
/// folder; undone on failure. v4's `FolderTransferResult`.
pub fn copy_folder(
    conn: &Connection,
    deps: &FolderOpDeps<'_>,
    mount_point_id: &str,
    from_path: &str,
    dest_mount_point_id: Option<&str>,
    to_path: &str,
) -> Result<Value, MountFileError> {
    let plan = plan_folder_transfer(
        conn,
        mount_point_id,
        from_path,
        dest_mount_point_id,
        to_path,
    )?;
    let rebased: Vec<String> = plan.tree.folders.iter().map(|f| plan.rebase(f)).collect();
    let made_folders = make_dest_folders(conn, &plan.dest, &plan.to_rel, &rebased)?;
    let mut copied: Vec<String> = Vec::new();
    let outcome = (|| -> Result<(), MountFileError> {
        for file in &plan.tree.files {
            let result = super::file_ops::copy_file(
                conn,
                &CopyOpts {
                    source_mount_point_id: plan.source.id.clone(),
                    source_path: file.clone(),
                    dest_mount_point_id: plan.dest.id.clone(),
                    dest_path: plan.rebase(file),
                    force: false,
                },
                deps.extractor,
                deps.webp,
            )?;
            copied.push(result.dest_path);
        }
        Ok(())
    })();
    if let Err(err) = outcome {
        tracing::warn!(
            target: "quilltap::mount_index",
            mountPointId = %plan.source.id,
            fromRel = %plan.from_rel,
            destMountPointId = %plan.dest.id,
            toRel = %plan.to_rel,
            copied = copied.len(),
            error = %err,
            "Folder copy failed; undoing"
        );
        for file in &copied {
            let _ = super::file_ops::delete_file(conn, &plan.dest.id, file);
        }
        let made_sub: Vec<String> = made_folders
            .iter()
            .filter(|f| **f != plan.to_rel)
            .cloned()
            .collect();
        let _ = remove_empty_folders(conn, &plan.dest, &plan.to_rel, &made_sub);
        publish_mount_change(&plan.dest.id);
        return Err(err);
    }
    publish_mount_change(&plan.dest.id);
    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %plan.source.id,
        fromRel = %plan.from_rel,
        destMountPointId = %plan.dest.id,
        toRel = %plan.to_rel,
        files = copied.len(),
        folders = made_folders.len(),
        "Copied folder"
    );
    Ok(json!({
        "mountPointId": plan.source.id,
        "fromPath": plan.from_rel,
        "destMountPointId": plan.dest.id,
        "toPath": plan.to_rel,
        "files": copied.len(),
        "folders": made_folders.len(),
    }))
}

/// v4 `moveFolder` — within one store a single folder move (the database rows
/// or the directory, every link following); into another store each file is a
/// per-file move — a database → database move keeps every link's identity and
/// hard-link group — then the emptied source folders go; a move that fails
/// part-way moves what it had moved back. v4's `FolderTransferResult` (within
/// one store: `{mountPointId, fromPath, toPath, destMountPointId}`, no counts).
pub fn move_folder(
    conn: &Connection,
    deps: &FolderOpDeps<'_>,
    mount_point_id: &str,
    from_path: &str,
    dest_mount_point_id: Option<&str>,
    to_path: &str,
) -> Result<Value, MountFileError> {
    let within = match dest_mount_point_id {
        None => true,
        Some(d) => d.is_empty() || d == mount_point_id,
    };
    if within {
        let result = move_folder_within_mount(deps.main, conn, mount_point_id, from_path, to_path)?;
        let id = result["mountPointId"]
            .as_str()
            .unwrap_or(mount_point_id)
            .to_string();
        publish_mount_change(&id);
        return Ok(json!({
            "mountPointId": result["mountPointId"],
            "fromPath": result["fromPath"],
            "toPath": result["toPath"],
            "destMountPointId": id,
        }));
    }
    let plan = plan_folder_transfer(
        conn,
        mount_point_id,
        from_path,
        dest_mount_point_id,
        to_path,
    )?;
    if plan.source.is_filesystem_mount() {
        let strays = unindexed_files_under(&plan.source, &plan.from_rel, &plan.tree.files)?;
        if !strays.is_empty() {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!(
                    "Folder holds {} file(s) the store does not index; they cannot move with it.",
                    strays.len()
                ),
                FileOpErrorCode::Conflict,
            )));
        }
    }
    let rebased: Vec<String> = plan.tree.folders.iter().map(|f| plan.rebase(f)).collect();
    let made_folders = make_dest_folders(conn, &plan.dest, &plan.to_rel, &rebased)?;
    let mut moved: Vec<(String, String)> = Vec::new();
    let outcome = (|| -> Result<(), MountFileError> {
        for file in &plan.tree.files {
            let result = super::file_ops::move_file(
                conn,
                &plan.source.id,
                file,
                &plan.dest.id,
                &plan.rebase(file),
                deps.extractor,
                deps.webp,
            )?;
            moved.push((file.clone(), result.dest_path));
        }
        Ok(())
    })();
    if let Err(err) = outcome {
        tracing::warn!(
            target: "quilltap::mount_index",
            mountPointId = %plan.source.id,
            fromRel = %plan.from_rel,
            destMountPointId = %plan.dest.id,
            toRel = %plan.to_rel,
            moved = moved.len(),
            error = %err,
            "Folder move failed; moving back"
        );
        for (from, to) in moved.iter().rev() {
            if let Err(undo) = super::file_ops::move_file(
                conn,
                &plan.dest.id,
                to,
                &plan.source.id,
                from,
                deps.extractor,
                deps.webp,
            ) {
                tracing::error!(
                    target: "quilltap::mount_index",
                    from = %to,
                    to = %from,
                    error = %undo,
                    "Could not move a file back after a failed folder move"
                );
            }
        }
        let made_sub: Vec<String> = made_folders
            .iter()
            .filter(|f| **f != plan.to_rel)
            .cloned()
            .collect();
        let _ = remove_empty_folders(conn, &plan.dest, &plan.to_rel, &made_sub);
        publish_mount_change(&plan.source.id);
        publish_mount_change(&plan.dest.id);
        return Err(err);
    }
    // NOT swallowed (v4 awaits it bare on success).
    remove_empty_folders(conn, &plan.source, &plan.from_rel, &plan.tree.folders)?;
    publish_mount_change(&plan.source.id);
    publish_mount_change(&plan.dest.id);
    tracing::info!(
        target: "quilltap::mount_index",
        mountPointId = %plan.source.id,
        fromRel = %plan.from_rel,
        destMountPointId = %plan.dest.id,
        toRel = %plan.to_rel,
        files = moved.len(),
        folders = made_folders.len(),
        "Moved folder between stores"
    );
    Ok(json!({
        "mountPointId": plan.source.id,
        "fromPath": plan.from_rel,
        "destMountPointId": plan.dest.id,
        "toPath": plan.to_rel,
        "files": moved.len(),
        "folders": made_folders.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P4.D340 R-8 (e): v4 rethrows the refusal from `moveFolder`; it is
    /// neither a `FileOpError` nor a `DatabaseStoreError`, so the route's
    /// catch-all answers 500 — v5's `Other` arm.
    #[test]
    fn a_title_lint_refusal_is_the_untyped_arm() {
        let r = crate::inventory::errors::InventoryRefusal::title_shadow(
            "Lantern",
            &[
                crate::inventory::errors::InventoryTitleCollision::KindNameShadow {
                    kind_name: "lantern".into(),
                },
            ],
        );
        match store_err(StoreError::TitleLint(r.clone())) {
            MountFileError::Other(m) => assert_eq!(m, r.message),
            other => panic!("{other:?}"),
        }
    }
}

// === P4.D381: v4's `folder-ops-walks.test.ts` (8 `it`s) + `folder-ops-fs-move
// .test.ts` (3) vectors, and every folder-op line's capture pin + silence leg.
// (The same vectors run against v4's REAL code in `mount_ops_equivalence`.) ===
#[cfg(test)]
#[cfg(unix)]
mod p4d381_walk_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    struct Fx {
        _dir: tempfile::TempDir,
        base: std::path::PathBuf,
        main: crate::db::Writer,
        mount: crate::db::Writer,
        db_a: String,
        db_b: String,
    }

    /// A provisioned instance: two database stores (`db_a`, `db_b`) and a
    /// filesystem store `p4d381-fs` over `<tmp>/tree` holding `notes/a.md`,
    /// `notes/sub/b.md`, `notes/sub/c.md` (scanned, so indexed + folder rows),
    /// with `db_a` holding `docs/x.md` and `docs/deep/y.md`.
    fn fixture() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let main =
            crate::db::Writer::open_writable(&dir.path().join("quilltap.db"), PEPPER).unwrap();
        let mount =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        let base = dir.path().join("tree");
        std::fs::create_dir_all(base.join("notes/sub")).unwrap();
        std::fs::write(base.join("notes/a.md"), "# A\n\nAlpha.\n").unwrap();
        std::fs::write(base.join("notes/sub/b.md"), "# B\n\nBravo.\n").unwrap();
        std::fs::write(base.join("notes/sub/c.md"), "# C\n\nCharlie.\n").unwrap();
        let db_a: String = mount
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        mount
            .connection()
            .execute_batch(&format!(
                "CREATE TEMP TABLE m AS SELECT * FROM doc_mount_points WHERE id = '{db_a}'; \
                 UPDATE m SET id = 'p4d381-b', name = 'P4D381 B'; \
                 INSERT INTO doc_mount_points SELECT * FROM m; \
                 UPDATE m SET id = 'p4d381-fs', name = 'P4D381 FS', mountType = 'filesystem', \
                   basePath = '{}'; \
                 INSERT INTO doc_mount_points SELECT * FROM m;",
                base.display()
            ))
            .unwrap();
        let links = DocMountFileLinksRepository::new(mount.connection());
        links
            .write_database_document(&db_a, "docs/x.md", "# X\n")
            .unwrap();
        links
            .write_database_document(&db_a, "docs/deep/y.md", "# Y\n")
            .unwrap();
        let fs = crate::db::doc_mount_points::DocMountPointsRepository::new(mount.connection())
            .find_service_info_by_id("p4d381-fs")
            .unwrap()
            .unwrap();
        let ex = super::super::converters::default_text_extractor();
        let r = super::super::scanner::scan_mount_point(mount.connection(), &fs, ex.as_ref());
        assert_eq!(r.files_new, 3, "{r:?}");
        Fx {
            _dir: dir,
            base,
            main,
            mount,
            db_a,
            db_b: "p4d381-b".into(),
        }
    }

    fn run<T>(fx: &Fx, f: impl FnOnce(&FolderOpDeps<'_>) -> T) -> (T, Vec<String>) {
        let ex = super::super::converters::default_text_extractor();
        let deps = FolderOpDeps {
            main: fx.main.connection(),
            extractor: ex.as_ref(),
            webp: super::super::normalize_blob_image::blob_codec_or_refusing(None),
        };
        crate::test_support::captured_with(|| f(&deps))
    }

    fn paths(fx: &Fx, mp: &str) -> Vec<String> {
        let mut v: Vec<String> = DocMountFileLinksRepository::new(fx.mount.connection())
            .find_by_mount_point_id_or_empty(mp)
            .into_iter()
            .map(|l| l.relative_path)
            .collect();
        v.sort();
        v
    }

    fn folder_paths(fx: &Fx, mp: &str) -> Vec<(String, Option<String>)> {
        let mut v: Vec<(String, Option<String>)> =
            DocMountFoldersRepository::new(fx.mount.connection())
                .find_by_mount_point_id_or_empty(mp)
                .into_iter()
                .map(|r| (r.path, r.parent_id))
                .collect();
        v.sort();
        v
    }

    fn has(lines: &[String], msg: &str) -> bool {
        lines.iter().any(|l| l.contains(msg))
    }

    // ── copyFolder ──

    #[test]
    fn copies_the_tree_into_another_store_at_its_rebased_paths() {
        let fx = fixture();
        let (r, lines) = run(&fx, |d| {
            copy_folder(
                fx.mount.connection(),
                d,
                &fx.db_a,
                "docs",
                Some(&fx.db_b),
                "copied",
            )
        });
        let r = r.unwrap();
        assert_eq!(
            r.to_string(),
            format!(
                r#"{{"mountPointId":"{}","fromPath":"docs","destMountPointId":"p4d381-b","toPath":"copied","files":2,"folders":2}}"#,
                fx.db_a
            )
        );
        assert_eq!(
            paths(&fx, &fx.db_b),
            vec!["copied/deep/y.md", "copied/x.md"]
        );
        assert!(lines.iter().any(|l| l.starts_with(&format!(
            "INFO quilltap::mount_index Copied folder mountPointId={} fromRel=docs destMountPointId=p4d381-b toRel=copied files=2 folders=2",
            fx.db_a
        ))), "{lines:?}");
        assert!(!has(&lines, "Folder copy failed; undoing"), "silence leg");
    }

    #[test]
    fn refuses_an_existing_destination_and_a_folder_into_itself() {
        let fx = fixture();
        let (r, _) = run(&fx, |d| {
            copy_folder(
                fx.mount.connection(),
                d,
                &fx.db_a,
                "docs",
                None,
                "docs/inner",
            )
        });
        assert!(r
            .unwrap_err()
            .to_string()
            .contains("A folder cannot be put inside itself: docs/inner"));
        let (r, _) = run(&fx, |d| {
            copy_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "notes",
                None,
                "notes/sub",
            )
        });
        assert!(r
            .unwrap_err()
            .to_string()
            .contains("cannot be put inside itself"));
        let (r, lines) = run(&fx, |d| {
            copy_folder(
                fx.mount.connection(),
                d,
                &fx.db_a,
                "docs/deep",
                None,
                "docs",
            )
        });
        assert_eq!(
            r.unwrap_err().to_string(),
            "Destination already exists: docs"
        );
        assert!(!has(&lines, "Copied folder"), "a refusal logs no INFO");
    }

    #[test]
    fn undoes_the_copies_when_one_file_fails() {
        let fx = fixture();
        let c = fx.base.join("notes/sub/c.md");
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o000)).unwrap();
        let (r, lines) = run(&fx, |d| {
            copy_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "notes/sub",
                Some(&fx.db_b),
                "undo",
            )
        });
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(r.is_err());
        assert!(
            paths(&fx, &fx.db_b).is_empty(),
            "the landed copy was undone"
        );
        assert!(
            folder_paths(&fx, &fx.db_b).is_empty(),
            "the made folders were removed"
        );
        assert!(lines.iter().any(|l| l.starts_with(
            "WARN quilltap::mount_index Folder copy failed; undoing mountPointId=p4d381-fs fromRel=notes/sub destMountPointId=p4d381-b toRel=undo copied=1 error="
        )), "{lines:?}");
        assert!(!has(&lines, "INFO quilltap::mount_index Copied folder"));
    }

    // ── moveFolder ──

    #[test]
    fn within_one_store_is_the_single_folder_move() {
        let fx = fixture();
        let (r, lines) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                &fx.db_a,
                "docs",
                Some(&fx.db_a),
                "papers",
            )
        });
        assert_eq!(
            r.unwrap().to_string(),
            format!(
                r#"{{"mountPointId":"{0}","fromPath":"docs","toPath":"papers","destMountPointId":"{0}"}}"#,
                fx.db_a
            )
        );
        assert!(paths(&fx, &fx.db_a).iter().any(|p| p == "papers/deep/y.md"));
        assert!(!has(&lines, "Moved folder between stores"), "silence leg");
    }

    #[test]
    fn into_another_store_moves_each_file_and_clears_the_emptied_source_folders() {
        let fx = fixture();
        let (r, lines) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                &fx.db_a,
                "docs",
                Some(&fx.db_b),
                "moved",
            )
        });
        let v = r.unwrap();
        assert_eq!(
            (v["files"].as_u64(), v["folders"].as_u64()),
            (Some(2), Some(2))
        );
        assert!(paths(&fx, &fx.db_a).iter().all(|p| !p.starts_with("docs/")));
        assert!(folder_paths(&fx, &fx.db_a)
            .iter()
            .all(|(p, _)| !p.starts_with("docs")));
        assert_eq!(paths(&fx, &fx.db_b), vec!["moved/deep/y.md", "moved/x.md"]);
        assert!(lines.iter().any(|l| l.starts_with(&format!(
            "INFO quilltap::mount_index Moved folder between stores mountPointId={} fromRel=docs destMountPointId=p4d381-b toRel=moved files=2 folders=2",
            fx.db_a
        ))), "{lines:?}");
        assert!(
            !has(&lines, "Folder move failed; moving back"),
            "silence leg"
        );
    }

    #[test]
    fn moves_files_back_when_one_fails() {
        let fx = fixture();
        let c = fx.base.join("notes/sub/c.md");
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o000)).unwrap();
        let (r, lines) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "notes/sub",
                Some(&fx.db_b),
                "undo",
            )
        });
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(r.is_err());
        assert!(paths(&fx, &fx.db_b).is_empty(), "the moved file came back");
        assert!(
            fx.base.join("notes/sub/b.md").is_file(),
            "its bytes are back on disk"
        );
        assert!(lines.iter().any(|l| l.starts_with(
            "WARN quilltap::mount_index Folder move failed; moving back mountPointId=p4d381-fs fromRel=notes/sub destMountPointId=p4d381-b toRel=undo moved=1 error="
        )), "{lines:?}");
        assert!(
            !has(&lines, "Could not move a file back"),
            "the move-back succeeded"
        );
    }

    // ── deleteFolder ──

    #[test]
    fn recursive_deletes_every_file_then_the_folders_deepest_first() {
        let fx = fixture();
        let (r, lines) = run(&fx, |_| {
            delete_folder(fx.mount.connection(), &fx.db_a, "docs", true)
        });
        assert_eq!(r.unwrap()["deletedFiles"], 2);
        assert!(folder_paths(&fx, &fx.db_a)
            .iter()
            .all(|(p, _)| !p.starts_with("docs")));
        assert!(lines.iter().any(|l| l == &format!(
            "INFO quilltap::mount_index Deleted folder recursively mountPointId={} path=docs files=2 folders=1",
            fx.db_a
        )), "{lines:?}");
    }

    #[test]
    fn without_recursive_is_the_empty_folder_delete() {
        let fx = fixture();
        let (r, lines) = run(&fx, |_| {
            delete_folder(fx.mount.connection(), &fx.db_a, "docs", false)
        });
        assert!(r.is_err(), "a non-empty folder is refused");
        assert!(!has(&lines, "Deleted folder recursively"), "silence leg");
    }

    // ── the filesystem-store folder rows (c34986421) ──

    #[test]
    fn fs_move_moves_the_directory_the_link_rows_and_the_folder_rows() {
        let fx = fixture();
        let (r, lines) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "notes",
                None,
                "papers",
            )
        });
        r.unwrap();
        assert!(fx.base.join("papers/sub/b.md").is_file());
        assert_eq!(
            paths(&fx, "p4d381-fs"),
            vec!["papers/a.md", "papers/sub/b.md", "papers/sub/c.md"]
        );
        let folders: Vec<String> = folder_paths(&fx, "p4d381-fs")
            .into_iter()
            .map(|f| f.0)
            .collect();
        assert_eq!(folders, vec!["papers", "papers/sub"]);
        assert!(lines.iter().any(|l| l
            == "DEBUG quilltap::mount_index Moved filesystem folder rows with the folder mountPointId=p4d381-fs fromPath=notes toPath=papers rows=2"), "{lines:?}");
    }

    #[test]
    fn fs_move_a_nested_folder_to_the_top_leaving_its_old_parent_row_alone() {
        let fx = fixture();
        let (r, _) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "notes/sub",
                None,
                "sub",
            )
        });
        r.unwrap();
        let folders = folder_paths(&fx, "p4d381-fs");
        assert!(
            folders.contains(&("notes".to_string(), None)),
            "{folders:?}"
        );
        assert!(
            folders.contains(&("sub".to_string(), None)),
            "moved to the top: parent NULL ({folders:?})"
        );
        // Silence: a folder with no row of its own logs no row move.
        std::fs::create_dir_all(fx.base.join("loose")).unwrap();
        let (r, lines) = run(&fx, |d| {
            move_folder(
                fx.mount.connection(),
                d,
                "p4d381-fs",
                "loose",
                None,
                "loose2",
            )
        });
        r.unwrap();
        assert!(!has(&lines, "Moved filesystem folder rows"), "{lines:?}");
    }

    #[test]
    fn fs_delete_removes_the_directory_and_its_folder_row() {
        let fx = fixture();
        for f in ["b.md", "c.md"] {
            super::super::file_ops::delete_file(
                fx.mount.connection(),
                "p4d381-fs",
                &format!("notes/sub/{f}"),
            )
            .unwrap();
        }
        let (r, lines) = run(&fx, |_| {
            delete_folder(fx.mount.connection(), "p4d381-fs", "notes/sub", false)
        });
        r.unwrap();
        assert!(!fx.base.join("notes/sub").exists());
        assert!(folder_paths(&fx, "p4d381-fs")
            .iter()
            .all(|(p, _)| p != "notes/sub"));
        assert!(lines.iter().any(|l| l
            == "DEBUG quilltap::mount_index Removed filesystem folder row with the folder mountPointId=p4d381-fs path=notes/sub"), "{lines:?}");
        // Silence: a directory with no row.
        std::fs::create_dir_all(fx.base.join("rowless")).unwrap();
        let (r, lines) = run(&fx, |_| {
            delete_folder(fx.mount.connection(), "p4d381-fs", "rowless", false)
        });
        r.unwrap();
        assert!(!has(&lines, "Removed filesystem folder row"), "{lines:?}");
    }

    #[test]
    fn under_is_case_insensitive_and_rebase_keeps_the_tail() {
        assert!(under("Notes/A.md", "notes"));
        assert!(!under("notes", "notes"));
        assert!(!under("notesX/a", "notes"));
        assert_eq!(depth_of("a/b/c"), 3);
    }
}
// === /P4.D381 ===
