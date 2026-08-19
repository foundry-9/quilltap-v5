//! v4 `import-files.ts` (`7189a968`) — the general file library importer:
//! folder tree first, then each file's bytes written back through the same
//! storage bridges the backup restore uses, and finally the metadata row.
//!
//! Two rules drive everything here (v4's module header, verbatim in spirit):
//!
//!  1. **`storageKey` never transfers.** The exporting instance's key points
//!     into its own storage (commonly `mount-blob:<mountPointId>:<blobId>`,
//!     naming a row in *that* instance's mount-index database). We discard it
//!     and record whatever key our own upload bridge hands back. The exported
//!     value survives as `_sourceStorageKey` provenance and nothing reads it.
//!  2. **Post-bridge mime/size win.** The bridges transcode bitmaps to WebP,
//!     so the archive's `mimeType`/`size` describe bytes that no longer exist
//!     once written. Recording the archive's values would re-introduce the
//!     "media_type X but bytes are Y" class of error.

use rusqlite::Connection;
use serde_json::Value;

use super::{id_of, IdMaps, ImportOptions};
use crate::db::files::FilesRepository;
use crate::db::folders::{FolderCreate, FoldersRepository};
use crate::db::DbError;
use crate::services::file_storage::PixelCodec;

pub(super) struct FileImportCounts {
    pub files: u32,
    pub folders: u32,
    pub skipped: u32,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn os(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A user-facing message off a `DbError` — the bridges' own sentences ride in
/// `DbError::Internal`, whose `Display` is the bare message, so v4's
/// `error.message` is carried verbatim with no error-type prefix in front of
/// it. (Before P4.50 these sentences rode in `DbError::Key`, whose `Display`
/// prepended "key derivation failed:"; this function existed to strip it.)
fn err_msg(e: DbError) -> String {
    e.to_string()
}

fn sa(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Recreate the folder tree (v4 `importFolders`). Parents come first (the
/// writer sorts by path length), so a child's `parentFolderId` always resolves
/// against a folder we have already created — or against one that already
/// existed here, which we reuse rather than duplicate: a folder is a location,
/// not content, so "duplicate" would just produce two identical paths.
fn import_folders(
    main: &Connection,
    user_id: &str,
    folders: &[Value],
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> (u32, Vec<(String, String)>) {
    let repo = FoldersRepository::new(main);
    let mut id_by_old_id: Vec<(String, String)> = Vec::new();
    let mut imported = 0u32;

    let mut sorted: Vec<&Value> = folders.iter().collect();
    sorted.sort_by_key(|f| s(f, "path").encode_utf16().count());

    for folder in sorted {
        let path = s(folder, "path");
        let out = (|| -> Result<(), DbError> {
            // `folder.projectId ? idMaps.projects.get(...) ?? folder.projectId
            // : null` — an unmapped project id is KEPT (a same-instance
            // re-import), unlike the null-on-miss FK remaps elsewhere.
            let project_id = os(folder, "projectId").map(|pid| {
                id_maps
                    .projects
                    .get(&pid)
                    .map(str::to_string)
                    .unwrap_or(pid)
            });

            if let Some(existing) = repo.find_by_path(user_id, &path, project_id.as_deref())? {
                id_by_old_id.push((id_of(folder), existing.id));
                return Ok(());
            }

            let parent_folder_id = os(folder, "parentFolderId").and_then(|old| {
                id_by_old_id
                    .iter()
                    .find(|(k, _)| *k == old)
                    .map(|(_, v)| v.clone())
            });

            let created = repo.create(
                &FolderCreate {
                    user_id: user_id.to_string(),
                    path: path.clone(),
                    name: s(folder, "name"),
                    parent_folder_id,
                    project_id,
                },
                &crate::db::folders::CreateOptions {
                    id: None,
                    created_at: None,
                    updated_at: None,
                },
            )?;
            id_by_old_id.push((id_of(folder), created));
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to import folder \"{path}\": {}",
                err_msg(e)
            ));
        }
    }

    (imported, id_by_old_id)
}

/// v4 `remapLinkedTo` — keep an id when the import remapped it, or when it
/// already names something on this instance; drop everything else. A dangling
/// id is not inert: `cascade_delete` reads `linkedTo` to decide whether a file
/// is still referenced, so a ghost reference can keep a genuinely orphaned
/// file alive forever. Message ids are the common companion of a chat id in
/// the same array (`chat-files-v2` writes `[chatId, messageId]`), so once a
/// chat resolves its message ids are checked too.
fn remap_linked_to(
    main: &Connection,
    mount: &Connection,
    linked_to: &[String],
    id_maps: &IdMaps,
    message_id_cache: &mut Vec<(String, Vec<String>)>,
) -> Result<(Vec<String>, usize), DbError> {
    let mut kept: Vec<String> = Vec::new();
    let mut resolved_chat_ids: Vec<String> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut dropped = 0usize;

    for id in linked_to {
        let mapped = id_maps
            .characters
            .get(id)
            .or_else(|| id_maps.chats.get(id))
            .or_else(|| id_maps.projects.get(id))
            .or_else(|| id_maps.groups.get(id));
        if let Some(mapped) = mapped {
            kept.push(mapped.to_string());
            if id_maps.chats.get(id).is_some() {
                resolved_chat_ids.push(mapped.to_string());
            }
            continue;
        }

        if crate::db::chats_read::find_by_id(main, id)?.is_some() {
            kept.push(id.clone());
            resolved_chat_ids.push(id.clone());
            continue;
        }
        // v4 `repos.characters.findById` / `repos.projects.findById` — an
        // existence test. The raw (no-vault) character read is the effective
        // check and cannot be sunk by one broken vault; the project overlay
        // read fails soft to "not found" (the preview's established idiom).
        if crate::db::characters_read::find_by_id_raw(main, id)?.is_some()
            || crate::db::projects::ProjectsRepository::new(main, mount)
                .find_by_id(id)
                .ok()
                .flatten()
                .is_some()
        {
            kept.push(id.clone());
            continue;
        }
        unresolved.push(id.clone());
    }

    for id in unresolved {
        let mut matched = false;
        for chat_id in &resolved_chat_ids {
            if !message_id_cache.iter().any(|(k, _)| k == chat_id) {
                let events = crate::db::chats_messages_read::get_messages(main, chat_id)?;
                let ids: Vec<String> = events
                    .iter()
                    .filter_map(|e| e.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect();
                message_id_cache.push((chat_id.clone(), ids));
            }
            let message_ids = message_id_cache
                .iter()
                .find(|(k, _)| k == chat_id)
                .map(|(_, v)| v);
            if message_ids.is_some_and(|ids| ids.iter().any(|m| m == &id)) {
                kept.push(id.clone());
                matched = true;
                break;
            }
        }
        if !matched {
            dropped += 1;
        }
    }

    Ok((kept, dropped))
}

/// v4 `importFiles` — metadata plus bytes. Files whose bytes never made it
/// into the archive (`_bytesMissing`) are skipped outright: a metadata row
/// with no content is a broken thumbnail waiting to happen.
#[allow(clippy::too_many_arguments)]
pub(super) fn import_files(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    user_id: &str,
    files: &[Value],
    folders: &[Value],
    options: &ImportOptions,
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> Result<FileImportCounts, DbError> {
    let (folders_imported, _folder_ids) = import_folders(main, user_id, folders, id_maps, warnings);

    let repo = FilesRepository::new(main);
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut dropped_links = 0usize;
    let mut message_id_cache: Vec<(String, Vec<String>)> = Vec::new();

    for file in files {
        let original_filename = s(file, "originalFilename");
        let out = (|| -> Result<bool, String> {
            if file
                .get("_bytesMissing")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || file
                    .get("dataBase64")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
            {
                warnings.push(format!(
                    "File \"{original_filename}\" was exported without its contents and was skipped."
                ));
                return Ok(false);
            }

            let file_id = id_of(file);
            if repo.find_by_id(&file_id).map_err(err_msg)?.is_some() {
                match options.conflict_strategy {
                    super::ConflictStrategy::Skip => return Ok(false),
                    super::ConflictStrategy::Overwrite => {
                        repo.delete(&file_id).map_err(err_msg)?;
                    }
                    // 'duplicate' falls through — create() mints a fresh id.
                    super::ConflictStrategy::Duplicate => {}
                }
            }

            let bytes =
                base64_decode(file.get("dataBase64").and_then(Value::as_str).unwrap_or(""))?;
            let project_id = os(file, "projectId").map(|pid| {
                id_maps
                    .projects
                    .get(&pid)
                    .map(str::to_string)
                    .unwrap_or(pid)
            });
            let mime = s(file, "mimeType");

            // Same two bridges the backup restore uses: project-bound files
            // land in their project's own store, everything else in the
            // Quilltap Uploads mount under `imported/`.
            let stored = match project_id.as_deref() {
                // v4's project branch goes through `fileStorageManager.uploadFile`,
                // whose catch wraps the bridge's sentence — carried verbatim.
                Some(pid) => crate::services::file_storage::write_project_file_to_mount_store(
                    mount,
                    codec,
                    pid,
                    &original_filename,
                    &bytes,
                    &mime,
                    Some(os(file, "folderPath").as_deref().unwrap_or("/")),
                    None,
                )
                .map_err(|e| {
                    format!(
                        "Failed to upload file '{original_filename}': {}",
                        err_msg(e)
                    )
                })?,
                // The uploads branch calls the bridge directly (no wrapper).
                None => crate::services::file_storage::write_user_upload_to_mount_store(
                    main,
                    mount,
                    codec,
                    &original_filename,
                    &bytes,
                    &mime,
                    "imported",
                    None,
                )
                .map_err(err_msg)?,
            };

            // v4's `storeMountFile` refreshes the mount's cached rollups
            // (fileCount / chunkCount / totalSizeBytes) best-effort after every
            // bridge write; v5's bridge deliberately leaves that to callers
            // (the file_storage module note), so the importer does it here.
            let _ = crate::db::doc_mount_points::DocMountPointsRepository::new(mount)
                .refresh_stats(&stored.mount_point_id);

            let (kept, dropped) = remap_linked_to(
                main,
                mount,
                &sa(file, "linkedTo"),
                id_maps,
                &mut message_id_cache,
            )
            .map_err(err_msg)?;
            dropped_links += dropped;

            let tags: Vec<String> = sa(file, "tags")
                .into_iter()
                .map(|t| id_maps.tags.get(&t).map(str::to_string).unwrap_or(t))
                .collect();

            repo.create(
                &crate::db::files::FileCreate {
                    user_id: user_id.to_string(),
                    sha256: s(file, "sha256"),
                    original_filename: original_filename.clone(),
                    // Post-bridge truth, not what the archive claimed.
                    mime_type: stored.stored_mime_type.clone(),
                    size: stored.size_bytes as f64,
                    width: file.get("width").and_then(Value::as_f64),
                    height: file.get("height").and_then(Value::as_f64),
                    is_plain_text: file.get("isPlainText").and_then(Value::as_bool),
                    linked_to: kept,
                    source: os(file, "source").unwrap_or_else(|| "UPLOADED".to_string()),
                    category: os(file, "category").unwrap_or_else(|| "DOCUMENT".to_string()),
                    generation_prompt: os(file, "generationPrompt"),
                    generation_model: os(file, "generationModel"),
                    generation_revised_prompt: os(file, "generationRevisedPrompt"),
                    description: os(file, "description"),
                    tags,
                    project_id: project_id.clone(),
                    folder_path: os(file, "folderPath"),
                    storage_key: Some(stored.storage_key()),
                    file_status: os(file, "fileStatus").unwrap_or_else(|| "ok".to_string()),
                },
                &crate::db::files::CreateOptions {
                    id: uuid::Uuid::new_v4().to_string(),
                    created_at: crate::clock::now_iso(),
                    updated_at: crate::clock::now_iso(),
                },
            )
            .map_err(err_msg)?;
            Ok(true)
        })();
        match out {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                warnings.push(format!(
                    "Failed to import file \"{original_filename}\": {e}"
                ));
                skipped += 1;
            }
        }
    }

    if dropped_links > 0 {
        warnings.push(format!(
            "{dropped_links} file link(s) pointed at entities that are not present on this instance and were dropped."
        ));
    }

    Ok(FileImportCounts {
        files: imported,
        folders: folders_imported,
        skipped,
    })
}

/// `Buffer.from(s, 'base64')` — Node's lenient decoder never throws; standard
/// alphabet with padding covers what our own writer emits.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| format!("invalid base64: {e}"))
}
