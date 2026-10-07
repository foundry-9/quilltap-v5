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
///
/// [P4.148 → dogfood #140] A `DbError::Sqlite` from the file repository is
/// still rendered bare — through [`super::item_error_text`] — so a constraint
/// failure reads as SQLite's own sentence, as v4's `error.message` does.
fn err_msg(e: DbError) -> String {
    super::item_error_text(&e)
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

/// v4's `_create` validation of a library folder (`base.repository.ts:
/// 350-368` over `FolderSchema`) — the ONE parse the `.qtap` import and the
/// backup restore run BEFORE the write (P4.161 Tier 2, R-A; the home a later
/// order may move beside `db::folders`). `item` is the create payload (the
/// raw `path` / `name` as they came; unknown keys stripped); the entity is
/// `{...item, id, createdAt, updatedAt}` with `claimed_id` the id the create
/// writes (`None` = minted). `Err` is the `ZodError.message`.
pub(crate) fn parse_create_folder(
    item: &Value,
    claimed_id: Option<&str>,
) -> Result<FolderCreate, String> {
    use crate::api::zod_issues::{zod_error_message, zod_folder_issues};
    let mut entity = item.as_object().cloned().unwrap_or_default();
    let now = crate::clock::now_iso();
    entity.insert(
        "id".into(),
        Value::String(
            claimed_id
                .unwrap_or("00000000-0000-0000-0000-000000000000")
                .into(),
        ),
    );
    entity.insert("createdAt".into(), Value::String(now.clone()));
    entity.insert("updatedAt".into(), Value::String(now));
    let issues = zod_folder_issues(&entity);
    if !issues.is_empty() {
        return Err(zod_error_message(&issues));
    }
    let e = Value::Object(entity);
    Ok(FolderCreate {
        user_id: s(&e, "userId"),
        path: s(&e, "path"),
        name: s(&e, "name"),
        parent_folder_id: os(&e, "parentFolderId"),
        project_id: os(&e, "projectId"),
    })
}

/// A refused folder create's three repository ERRORs (validate → `_create` →
/// the repository's wrap, `{userId, path}`).
pub(crate) fn log_refused_folder(user_id: &str, path: Option<&Value>, zod: &str) {
    let path = path.map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    super::log_refused_create("folders", zod);
    super::log_folder_create_wrap_failure(
        user_id,
        path.as_deref(),
        &DbError::Internal(zod.to_string()),
    );
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
        let out = (|| -> Result<(), String> {
            // `folder.projectId ? idMaps.projects.get(...) ?? folder.projectId
            // : null` — an unmapped project id is KEPT (a same-instance
            // re-import), unlike the null-on-miss FK remaps elsewhere. The
            // value is kept RAW (P4.161): a non-uuid / non-string id reaches
            // the schema, which refuses it.
            let project_id: Value = match folder.get("projectId") {
                Some(raw) if crate::api::system_qtap::js_truthy(Some(raw)) => raw
                    .as_str()
                    .and_then(|pid| id_maps.projects.get(pid))
                    .map_or_else(|| raw.clone(), |m| Value::String(m.to_string())),
                _ => Value::Null,
            };

            // A non-string path or project id finds nothing (v4's lookup
            // binds it as-is; SQLite's TEXT column never equals it).
            let existing = match (folder.get("path").and_then(Value::as_str), &project_id) {
                (Some(p), Value::String(pid)) => repo.find_by_path(user_id, p, Some(pid)),
                (Some(p), Value::Null) => repo.find_by_path(user_id, p, None),
                _ => Ok(None),
            }
            .map_err(err_msg)?;
            if let Some(existing) = existing {
                id_by_old_id.push((id_of(folder), existing.id));
                return Ok(());
            }

            let parent_folder_id = os(folder, "parentFolderId").and_then(|old| {
                id_by_old_id
                    .iter()
                    .find(|(k, _)| *k == old)
                    .map(|(_, v)| v.clone())
            });

            // v4 `ensureByPath({userId, path, name, parentFolderId,
            // projectId})` → `create` → `_create` validates the WHOLE entity
            // (P4.161 Tier 2 — v5 used to write `""` for a non-string path or
            // name and any project id as it came).
            let mut item = serde_json::Map::new();
            item.insert("userId".into(), Value::String(user_id.to_string()));
            for k in ["path", "name"] {
                if let Some(v) = folder.get(k) {
                    item.insert(k.into(), v.clone());
                }
            }
            item.insert(
                "parentFolderId".into(),
                parent_folder_id.map_or(Value::Null, Value::String),
            );
            item.insert("projectId".into(), project_id);
            let create = parse_create_folder(&Value::Object(item.clone()), None)
                .inspect_err(|zod| log_refused_folder(user_id, folder.get("path"), zod))?;

            // Find-or-create at the chokepoint (v4 `a5df98b3f`, bug 114). The
            // `find_by_path` above is the reuse-REPORTING branch, not the
            // uniqueness guarantee — that lives in `ensure_by_path` and the
            // unique index behind it.
            let created = repo.ensure_by_path(&create).map_err(err_msg)?;
            id_by_old_id.push((id_of(folder), created.id));
            imported += 1;
            Ok(())
        })();
        if let Err(text) = out {
            warnings.push(format!("Failed to import folder \"{path}\": {text}"));
            // v4 `import-files.ts:93` (P4.148 Tier 2 item 17).
            tracing::warn!(
                folderId = super::id_field(folder),
                path = %path,
                error = %text,
                "Failed to import folder"
            );
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
                // STRICT: v4 runs the import inside `withStrictRepositoryFailures`
                // (`execute.ts:430`), so a failed read fails the import.
                let events = crate::db::chats_messages_read::get_messages_strict(main, chat_id)?;
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
                    // Post-bridge truth, not what the archive claimed (see the
                    // module header). `sha256` joined that rule in 4.9.0 (v4
                    // `0b0617fee`, bug 117): the bridge transcodes bitmaps to
                    // WebP, and a row carrying the archive's pre-transcode hash
                    // cannot be joined to the mount blob it points at.
                    sha256: stored.sha256.clone(),
                    original_filename: original_filename.clone(),
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
                    // The avatar cache key travels AS-IS: never remapped, never derived
                    // (v4 `7fbf8a55b` — the vendored `qtap-export.schema.json` says so in
                    // as many words, and nothing on a receiving instance recomputes one).
                    generation_key: os(file, "generationKey"),
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
            Err(text) => {
                warnings.push(format!(
                    "Failed to import file \"{original_filename}\": {text}"
                ));
                // v4 `import-files.ts:292` (P4.148 Tier 2 item 17).
                tracing::warn!(fileId = super::id_field(file), error = %text, "Failed to import file");
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
