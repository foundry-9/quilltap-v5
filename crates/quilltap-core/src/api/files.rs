//! The general files-family dispatch handlers (P4.6ae) — v4's
//! `app/api/v1/files/**` route logic the general Files SPA consumes: the file
//! listing (`serializeFileEntry`), the metadata-only move/promote
//! (`buildManagedFileResponse`), delete (owner + associations + stale-linkedTo
//! arms), and the folders CRUD surface. Ported over the P4.6ae db-layer additions
//! (`db/files.rs` / `db/folders.rs` / `folder_utils.rs`).
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! drives with the fixture's own user id on both sides; the engine passes
//! `SINGLE_USER_ID`. v4's per-row ownership collapses to a forbidden/notFound arm
//! for the single-user v5.
//!
//! **Two response shapes for a file, never unified** (v4 quirk 2):
//! [`serialize_file_entry`] (list — dual filename fields, RESOLVED folderPath,
//! `fileStatus||'ok'`, explicit nulls) vs [`build_managed_file_response`]
//! (move/promote — `filename` only, RAW folderPath, no
//! originalFilename/width/height/fileStatus).
//!
//! The `FILE_HAS_ASSOCIATIONS` 400 carries the itemized `associations` object on
//! the shared `CoreError` envelope (P4.6ah widened it with an additive optional
//! field), and the `dissociate=true` delete arm strips message attachments +
//! character default-image / avatar-override references before deleting
//! ([`dissociate_file_from_all`]). Best-effort storage side effects (createFolder /
//! deleteFolder / deleteFile) are DB-invisible and skipped, matching v4's
//! try/catch-and-log.

use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};

use crate::collation::locale_compare;
use crate::db::chats_messages::ChatMessagesRepository;
use crate::db::files::{
    CreateOptions, FileCreate, FileFull, FileUpdate, FilesRepository, MoveProjectId,
};
use crate::db::folders::{FolderCreate, FolderRow, FolderUpdate, FoldersRepository};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, DbError};
use crate::files::image_processing::can_resize_image;
use crate::folder_utils::{
    get_folder_name, get_parent_path, normalize_folder_path, resolve_effective_folder_path,
    validate_folder_path,
};
use crate::photos::keep_image_markdown::sha256_of_buffer;
use crate::services::file_storage::{
    write_project_file_to_mount_store, write_user_upload_to_mount_store, NotConfiguredPixelCodec,
    StoredBlob,
};

use super::types::{ErrorKind, FileAssocCharacter, FileAssocMessage, FileAssociations, Response};

// ===========================================================================
// Shared helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn forbidden() -> Response {
    // v4 `forbidden()` → `{ error: 'Forbidden' }`, 403.
    Response::error(ErrorKind::Forbidden, "Forbidden")
}

/// v4 `x || null` general-scope coercion for a dispatch projectId arg (empty →
/// none).
fn or_null(v: Option<&str>) -> Option<String> {
    match v {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// A JSON value from an `Option<String>` (`None` → explicit `null`).
fn str_or_null(v: &Option<String>) -> Value {
    match v {
        Some(s) => json!(s),
        None => Value::Null,
    }
}

// ===========================================================================
// The two file-response shapes (v4 quirk 2 — pin both, never unify)
// ===========================================================================

/// v4 `serializeFileEntry` (`files/shared.ts:45`) — the list shape. BOTH
/// `originalFilename` AND `filename` (= originalFilename); `filepath` =
/// `/api/v1/files/{id}`; folderPath RESOLVED via `resolveEffectiveFolderPath`;
/// `fileStatus` defaulted `'ok'`. **The read marshaling collapses NULL optional
/// columns to `undefined`, which `JSON.stringify` DROPS** — so a null
/// `projectId`/`description`/`width`/`height` OMITS its key (the P4.6p read
/// pattern). `folderPath`/`fileStatus` are always present (a string).
fn serialize_file_entry(f: &FileFull) -> Value {
    let file_status = f
        .file_status
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("ok");
    let mut m = Map::new();
    m.insert("id".into(), json!(f.id));
    m.insert("userId".into(), json!(f.user_id));
    m.insert("originalFilename".into(), json!(f.original_filename));
    m.insert("filename".into(), json!(f.original_filename));
    m.insert("filepath".into(), json!(format!("/api/v1/files/{}", f.id)));
    m.insert("mimeType".into(), json!(f.mime_type));
    m.insert("size".into(), json!(f.size));
    m.insert("category".into(), json!(f.category));
    if let Some(d) = &f.description {
        m.insert("description".into(), json!(d));
    }
    if let Some(p) = &f.project_id {
        m.insert("projectId".into(), json!(p));
    }
    m.insert(
        "folderPath".into(),
        json!(resolve_effective_folder_path(
            f.folder_path.as_deref(),
            f.storage_key.as_deref()
        )),
    );
    if let Some(w) = f.width {
        m.insert("width".into(), json!(w));
    }
    if let Some(h) = f.height {
        m.insert("height".into(), json!(h));
    }
    m.insert("fileStatus".into(), json!(file_status));
    m.insert("createdAt".into(), json!(f.created_at));
    m.insert("updatedAt".into(), json!(f.updated_at));
    Value::Object(m)
}

/// v4 `buildManagedFileResponse` over the `{...existing, ...patch}` merge (v4
/// `_update` returns the merged object, NOT a re-read). `filename` only, RAW
/// `folderPath`, no originalFilename/width/height/fileStatus. The move/promote
/// PATCH determines `projectId`: an explicitly-patched value (incl. `null`) is
/// PRESENT; an unpatched `null` (the read-marshaled `undefined`) is OMITTED (the
/// P4.6p mutation pattern). `updated_at` is the minted stamp (harness-blanked).
fn build_managed_response(
    f: &FileFull,
    new_filename: &str,
    new_folder_path: &str,
    project_id: &ManagedProjectId,
    updated_at: &str,
) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(f.id));
    m.insert("userId".into(), json!(f.user_id));
    m.insert("filename".into(), json!(new_filename));
    m.insert("filepath".into(), json!(format!("/api/v1/files/{}", f.id)));
    m.insert("mimeType".into(), json!(f.mime_type));
    m.insert("size".into(), json!(f.size));
    m.insert("category".into(), json!(f.category));
    match project_id {
        ManagedProjectId::Present(Some(p)) => {
            m.insert("projectId".into(), json!(p));
        }
        ManagedProjectId::Present(None) => {
            m.insert("projectId".into(), Value::Null);
        }
        ManagedProjectId::Omit => {}
    }
    m.insert("folderPath".into(), json!(new_folder_path));
    m.insert("createdAt".into(), json!(f.created_at));
    m.insert("updatedAt".into(), json!(updated_at));
    Value::Object(m)
}

/// How `projectId` appears in a merged managed response: `Present(Some)` = a
/// concrete value, `Present(None)` = an explicit `null` (patched), `Omit` = the
/// key is dropped (an unpatched null column).
enum ManagedProjectId {
    Present(Option<String>),
    Omit,
}

/// v4 folder GET row map (`folders/route.ts:158`) — the subset the folders list +
/// create echo (NOTE: omits `parentFolderId`).
fn serialize_folder(f: &FolderRow) -> Value {
    json!({
        "id": f.id,
        "userId": f.user_id,
        "path": f.path,
        "name": f.name,
        "projectId": str_or_null(&f.project_id),
        "createdAt": f.created_at,
        "updatedAt": f.updated_at,
    })
}

/// The folder object v4's CREATE arm echoes as `folder:` — the created entity
/// (`{...data, id, timestamps}`), so `parentFolderId`/`projectId` are ALWAYS
/// present (explicit `null` when unset).
fn serialize_folder_created(f: &FolderRow) -> Value {
    json!({
        "id": f.id,
        "userId": f.user_id,
        "path": f.path,
        "name": f.name,
        "parentFolderId": str_or_null(&f.parent_folder_id),
        "projectId": str_or_null(&f.project_id),
        "createdAt": f.created_at,
        "updatedAt": f.updated_at,
    })
}

/// The folder object v4's IDEMPOTENT arm echoes as `folder:` — a `findByPath`
/// READ, so a NULL `parentFolderId`/`projectId` marshals to `undefined` and its key
/// is DROPPED (the P4.6p read pattern).
fn serialize_folder_read(f: &FolderRow) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(f.id));
    m.insert("userId".into(), json!(f.user_id));
    m.insert("path".into(), json!(f.path));
    m.insert("name".into(), json!(f.name));
    if let Some(p) = &f.parent_folder_id {
        m.insert("parentFolderId".into(), json!(p));
    }
    if let Some(p) = &f.project_id {
        m.insert("projectId".into(), json!(p));
    }
    m.insert("createdAt".into(), json!(f.created_at));
    m.insert("updatedAt".into(), json!(f.updated_at));
    Value::Object(m)
}

// ===========================================================================
// GET /api/v1/files (the listing)
// ===========================================================================

/// v4 `handleGet` (`files/handlers/get.ts`): `findByUserId` → filter
/// (`general` → projectId null; else projectId) → folderPath filter (resolved ==
/// normalized) → **`createdAt` DESC** → `serializeFileEntry`. Body `{ files }`.
pub fn files_list(
    db: &Db,
    user_id: &str,
    project_id: Option<&str>,
    folder_path: Option<&str>,
    filter: Option<&str>,
) -> Response {
    let uid = user_id.to_string();
    let result = db.read_main(move |c| FilesRepository::new(c).find_by_user_id(&uid));
    let mut files = match result {
        Ok(f) => f,
        Err(e) => return internal(e),
    };

    if filter == Some("general") {
        files.retain(|f| f.project_id.is_none());
    } else if let Some(pid) = project_id.filter(|s| !s.is_empty()) {
        files.retain(|f| f.project_id.as_deref() == Some(pid));
    }

    if let Some(fp) = folder_path.filter(|s| !s.is_empty()) {
        let normalized = normalize_folder_path(Some(fp));
        files.retain(|f| {
            resolve_effective_folder_path(f.folder_path.as_deref(), f.storage_key.as_deref())
                == normalized
        });
    }

    // v4: `sort((a,b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())`
    // — a stable DESC sort by createdAt (uniform ISO-8601 → lexical == chronological;
    // ties keep the pre-sort rowid order).
    files.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let body: Vec<Value> = files.iter().map(serialize_file_entry).collect();
    Response::Files(json!({ "files": body }))
}

// ===========================================================================
// POST /api/v1/files/[id]?action=move|promote (metadata only)
// ===========================================================================

/// The move `projectId` tri-state the dispatch layer decodes (absent / null /
/// value): v4's `projectId === undefined ? file.projectId : projectId`.
pub enum MovePid {
    /// Field absent — keep the file's current projectId.
    Keep,
    /// Explicit `null` — clear to general.
    Clear,
    /// A concrete id — set it.
    Set(String),
}

/// v4 `handleMoveFile` (`files/[id]/actions/move.ts`) via `handlePost`
/// (owner-preload → notFound). `{folderPath?, filename?, projectId?}`, ≥1 required;
/// `validateFilename`; a concrete projectId gates on `projects.findByIdRaw`
/// existence (never reads the store — a degraded store must not throw). Body
/// `{ data: ManagedFile }`.
pub async fn file_move(
    db: &Db,
    user_id: &str,
    file_id: &str,
    folder_path: Option<String>,
    filename: Option<String>,
    project_id: MovePid,
) -> Response {
    let uid = user_id.to_string();
    let fid = file_id.to_string();
    let out = db
        .write(move |ws| {
            let c = ws.main().connection();
            let repo = FilesRepository::new(c);
            let Some(file) = repo.find_full_by_id(&fid)? else {
                return Ok(not_found("File"));
            };
            if file.user_id != uid {
                return Ok(not_found("File"));
            }

            // v4: at least one of folderPath / filename / projectId provided.
            let project_provided = !matches!(project_id, MovePid::Keep);
            if folder_path.is_none() && filename.is_none() && !project_provided {
                return Ok(bad_request(
                    "At least one of folderPath, filename, or projectId must be provided",
                ));
            }

            // v4: `if (filename)` truthy → validate.
            if let Some(name) = filename.as_deref().filter(|s| !s.is_empty()) {
                if let Err(msg) = validate_filename(name) {
                    return Ok(bad_request(msg));
                }
            }

            // v4: concrete projectId → existence gate.
            if let MovePid::Set(pid) = &project_id {
                if !project_exists(c, pid)? {
                    return Ok(not_found("Project"));
                }
            }

            // folderPath: undefined → keep; provided → normalize + validate.
            let mut new_folder_path = file.folder_path.clone().unwrap_or_else(|| "/".to_string());
            if let Some(fp) = &folder_path {
                match normalize_and_validate_folder_path(Some(fp)) {
                    Ok(p) => new_folder_path = p,
                    Err(msg) => return Ok(bad_request(msg)),
                }
            }

            // originalFilename: `filename || file.originalFilename` (empty → current).
            let resolved_name = filename
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&file.original_filename)
                .to_string();

            // The WRITE disposition (Keep re-writes the current value — a no-op).
            let write_pid = match &project_id {
                MovePid::Keep => file
                    .project_id
                    .as_deref()
                    .map(MoveProjectId::Set)
                    .unwrap_or(MoveProjectId::Clear),
                MovePid::Clear => MoveProjectId::Clear,
                MovePid::Set(p) => MoveProjectId::Set(p),
            };
            // The RESPONSE disposition (v4 `{...existing, ...patch}`): an unpatched
            // null projectId is OMITTED; a patched value/null is PRESENT.
            let managed_pid = match &project_id {
                MovePid::Keep => match &file.project_id {
                    Some(p) => ManagedProjectId::Present(Some(p.clone())),
                    None => ManagedProjectId::Omit,
                },
                MovePid::Clear => ManagedProjectId::Present(None),
                MovePid::Set(p) => ManagedProjectId::Present(Some(p.clone())),
            };
            if !repo.apply_move_update(&fid, Some(&resolved_name), &new_folder_path, write_pid)? {
                return Ok(not_found("File"));
            }
            let data = build_managed_response(
                &file,
                &resolved_name,
                &new_folder_path,
                &managed_pid,
                &crate::clock::now_iso(),
            );
            Ok(Response::Files(json!({ "data": data })))
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

/// v4 `handlePromoteFile` (`files/[id]/actions/promote.ts`) via `handlePost`.
/// `{targetProjectId?, folderPath?}` → `projectId = target ?? null`, folder default
/// `/`. A truthy targetProjectId gates on existence. Body `{ data: ManagedFile }`.
pub async fn file_promote(
    db: &Db,
    user_id: &str,
    file_id: &str,
    target_project_id: Option<String>,
    folder_path: Option<String>,
) -> Response {
    let uid = user_id.to_string();
    let fid = file_id.to_string();
    let out = db
        .write(move |ws| {
            let c = ws.main().connection();
            let repo = FilesRepository::new(c);
            let Some(file) = repo.find_full_by_id(&fid)? else {
                return Ok(not_found("File"));
            };
            if file.user_id != uid {
                return Ok(not_found("File"));
            }

            let folder = match normalize_and_validate_folder_path(Some(
                folder_path.as_deref().unwrap_or("/"),
            )) {
                Ok(p) => p,
                Err(msg) => return Ok(bad_request(msg)),
            };

            // v4: `if (targetProjectId)` truthy → existence gate.
            let target = target_project_id.as_deref().filter(|s| !s.is_empty());
            if let Some(tpid) = target {
                if !project_exists(c, tpid)? {
                    return Ok(not_found("Project"));
                }
            }

            // v4: `projectId: targetProjectId ?? null` — ALWAYS patched (value or
            // null), so always PRESENT in the merged response.
            let (write_pid, managed_pid) = match target {
                Some(p) => (
                    MoveProjectId::Set(p),
                    ManagedProjectId::Present(Some(p.to_string())),
                ),
                None => (MoveProjectId::Clear, ManagedProjectId::Present(None)),
            };
            if !repo.apply_move_update(&fid, None, &folder, write_pid)? {
                return Ok(not_found("File"));
            }
            let data = build_managed_response(
                &file,
                &file.original_filename,
                &folder,
                &managed_pid,
                &crate::clock::now_iso(),
            );
            Ok(Response::Files(json!({ "data": data })))
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// DELETE /api/v1/files/[id]
// ===========================================================================

/// v4 `handleDeleteFile` (`files/[id]/actions/delete.ts`). Owner check
/// (`forbidden`); `?dissociate=` strips refs (message attachments + character
/// default-image / avatar-overrides + the file's own linkedTo); unforced
/// linked-file → the itemized `FILE_HAS_ASSOCIATIONS` 400 (real associations)
/// or a silent stale-linkedTo clear; then delete. Body `{ success: true }`.
pub async fn file_delete(
    db: &Db,
    user_id: &str,
    file_id: &str,
    force: bool,
    dissociate: bool,
) -> Response {
    // Owner + linked-state read.
    let fid = file_id.to_string();
    let mut file = match db.read_main(move |c| FilesRepository::new(c).find_full_by_id(&fid)) {
        Ok(Some(f)) => f,
        Ok(None) => return not_found("File"),
        Err(e) => return internal(e),
    };
    if file.user_id != user_id {
        return forbidden();
    }

    // v4: dissociate strips every reference (messages + character images + the
    // file's own linkedTo), then re-reads the row so the subsequent (now empty)
    // linkedTo check is a no-op.
    if dissociate && !file.linked_to.is_empty() {
        if let Err(e) = dissociate_file_from_all(db, user_id, &file).await {
            return internal(e);
        }
        let fid_reload = file.id.clone();
        match db.read_main(move |c| FilesRepository::new(c).find_full_by_id(&fid_reload)) {
            Ok(Some(reloaded)) => file = reloaded,
            Ok(None) => {}
            Err(e) => return internal(e),
        }
    }

    if !force && !dissociate && !file.linked_to.is_empty() {
        // Distinguish REAL associations from stale linkedTo.
        let assoc = match compute_associations(db, user_id, &file.id, &file.linked_to) {
            Ok(a) => a,
            Err(e) => return internal(e),
        };
        let has_real = !assoc.characters.is_empty() || !assoc.messages.is_empty();
        if has_real {
            // v4 `badRequest('File is linked to other items', {code, associations})`
            // — the itemized `getFileAssociations` result rides the envelope.
            return Response::error_with_associations(
                ErrorKind::BadRequest,
                "File is linked to other items",
                "FILE_HAS_ASSOCIATIONS",
                assoc,
            );
        }
        // Stale linkedTo — clear silently, then proceed (v4).
        let fid2 = file.id.clone();
        if let Err(e) = db
            .write(move |ws| {
                FilesRepository::new(ws.main().connection()).update(
                    &fid2,
                    &FileUpdate {
                        linked_to: Some(vec![]),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    },
                )
            })
            .await
        {
            return internal(e);
        }
    }

    // Storage + thumbnail deletes are best-effort DB-invisible side effects (v4
    // try/catch-and-log) — skipped.
    let fid3 = file.id.clone();
    match db
        .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid3))
        .await
    {
        Ok(true) => Response::Files(json!({ "success": true })),
        Ok(false) => not_found("File"),
        Err(e) => internal(e),
    }
}

/// v4 `dissociateFileFromAll` (`files/[id]/shared.ts:64`) — strip every reference
/// to `file`: for each `linkedTo` entity, the first matching message across the
/// user's chats has the attachment removed + a `[Attachment "…" deleted <ts>]`
/// note appended; characters using the file as `defaultImageId` are cleared; the
/// file id is filtered out of any `avatarOverrides`; finally the file's own
/// `linkedTo` is emptied. All in one writer transaction (v4 issues them
/// sequentially; the net DB state is identical). The `timestamp` is minted once
/// (v4 `new Date().toISOString()` at function start).
async fn dissociate_file_from_all(db: &Db, user_id: &str, file: &FileFull) -> Result<(), DbError> {
    let file_id = file.id.clone();
    let linked_to = file.linked_to.clone();
    let filename = {
        let n = file.original_filename.trim();
        if n.is_empty() {
            "unknown file".to_string()
        } else {
            file.original_filename.clone()
        }
    };
    let timestamp = crate::clock::now_iso();
    let user_id = user_id.to_string();

    db.write(move |ws| {
        let main = ws.main().connection();

        // 1. Message attachments: for each linked entity, find the FIRST message
        //    (by id) across the user's chats that carries the attachment; strip it
        //    and append the deletion note.
        let chats = chats_read::find_by_user_id(main, &user_id)?;
        let msgs_repo = ChatMessagesRepository::new(main);
        for entity_id in &linked_to {
            'chat: for chat in &chats {
                let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let messages = match chats_messages_read::get_messages(main, chat_id) {
                    Ok(m) => m,
                    Err(_) => continue, // v4 per-chat try/catch → skip.
                };
                let Some(message) = messages.iter().find(|m| {
                    m.get("id").and_then(Value::as_str) == Some(entity_id.as_str())
                        && m.get("type").and_then(Value::as_str) == Some("message")
                }) else {
                    continue;
                };
                let attachments: Vec<String> = message
                    .get("attachments")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if !attachments.iter().any(|a| a == &file_id) {
                    continue;
                }
                let note = format!("\n\n[Attachment \"{filename}\" deleted {timestamp}]");
                let content = message.get("content").and_then(Value::as_str).unwrap_or("");
                let filtered: Vec<String> =
                    attachments.into_iter().filter(|a| a != &file_id).collect();
                msgs_repo.update_message(
                    chat_id,
                    entity_id,
                    &json!({ "content": format!("{content}{note}"), "attachments": filtered }),
                )?;
                break 'chat; // v4 breaks after the first matching chat.
            }
        }

        // 2. Characters using the file as their default image → clear it.
        let mount = ws.mount_index().ok_or_else(|| {
            DbError::Key("dissociate requires the mount-index database".to_string())
        })?;
        let default_chars =
            characters_read::find_by_default_image_id(main, mount.connection(), &file_id)?;
        for c in &default_chars {
            if let Some(cid) = c.get("id").and_then(Value::as_str) {
                main.execute(
                    "UPDATE characters SET defaultImageId = NULL, updatedAt = ?1 WHERE id = ?2",
                    rusqlite::params![crate::clock::now_iso(), cid],
                )?;
            }
        }

        // 3. Characters whose avatarOverrides reference the file → filter it out.
        let override_chars =
            characters_read::find_by_avatar_override_image_id(main, mount.connection(), &file_id)?;
        for c in &override_chars {
            let Some(cid) = c.get("id").and_then(Value::as_str) else {
                continue;
            };
            // Read the RAW stored JSON so the rewrite preserves the exact shape.
            let raw: Option<String> = main
                .query_row(
                    "SELECT avatarOverrides FROM characters WHERE id = ?1",
                    rusqlite::params![cid],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            let mut overrides: Vec<Value> = raw
                .as_deref()
                .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
                .unwrap_or_default();
            overrides
                .retain(|o| o.get("imageId").and_then(Value::as_str) != Some(file_id.as_str()));
            let json_text = serde_json::to_string(&overrides)
                .map_err(|e| DbError::Key(format!("avatarOverrides serialize: {e}")))?;
            main.execute(
                "UPDATE characters SET avatarOverrides = ?1, updatedAt = ?2 WHERE id = ?3",
                rusqlite::params![json_text, crate::clock::now_iso(), cid],
            )?;
        }

        // 4. Clear the file's own linkedTo.
        FilesRepository::new(main).update(
            &file_id,
            &FileUpdate {
                linked_to: Some(vec![]),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await
}

/// The itemized associations for a file (v4 `getFileAssociations`,
/// `lib/files/get-file-associations.ts`) — characters using it as default/override
/// image (deduped by id, default-usage wins), and messages whose attachments
/// reference it (matched when the chatId OR messageId is in `linkedTo`). Exposed
/// (pub) so the route differential verifies the computation directly, and reused
/// on the `FILE_HAS_ASSOCIATIONS` delete envelope.
pub fn get_file_associations(
    db: &Db,
    user_id: &str,
    file_id: &str,
    linked_to: &[String],
) -> Result<Value, DbError> {
    let assoc = compute_associations(db, user_id, file_id, linked_to)?;
    serde_json::to_value(&assoc).map_err(|e| DbError::Key(format!("associations serialize: {e}")))
}

fn compute_associations(
    db: &Db,
    user_id: &str,
    file_id: &str,
    linked_to: &[String],
) -> Result<FileAssociations, DbError> {
    // Characters: default-image then avatar-override, deduped by id (default wins).
    let fid = file_id.to_string();
    let (default_chars, override_chars) = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let d = characters_read::find_by_default_image_id(main, mount, &fid)?;
            let o = characters_read::find_by_avatar_override_image_id(main, mount, &fid)?;
            Ok((d, o))
        })
    })?;

    let uid = user_id;
    let mut order: Vec<String> = Vec::new();
    let mut by_id: std::collections::HashMap<String, FileAssocCharacter> =
        std::collections::HashMap::new();
    let mut push_char = |c: &Value, usage: &str| {
        if c.get("userId").and_then(Value::as_str) != Some(uid) {
            return;
        }
        let Some(id) = c.get("id").and_then(Value::as_str) else {
            return;
        };
        if by_id.contains_key(id) {
            // v4: usage = existing==='default' ? 'default' : 'override' — i.e. keep
            // the existing usage (default sticks; override stays override).
        } else {
            order.push(id.to_string());
            by_id.insert(
                id.to_string(),
                FileAssocCharacter {
                    id: id.to_string(),
                    name: c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    usage: usage.to_string(),
                },
            );
        }
    };
    for c in &default_chars {
        push_char(c, "default");
    }
    for c in &override_chars {
        push_char(c, "override");
    }
    let characters: Vec<FileAssocCharacter> =
        order.into_iter().map(|id| by_id[&id].clone()).collect();

    // Messages: scan the user's chats for a message whose attachments include the
    // file AND whose chatId or messageId is in linkedTo.
    let uid_owned = user_id.to_string();
    let messages = db.read_main(|main| {
        let chats = chats_read::find_by_user_id(main, &uid_owned)?;
        let mut out: Vec<FileAssocMessage> = Vec::new();
        for chat in &chats {
            let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
                continue;
            };
            let chat_in_linked = linked_to.iter().any(|l| l == chat_id);
            let msgs = match chats_messages_read::get_messages(main, chat_id) {
                Ok(m) => m,
                Err(_) => continue, // v4 per-chat try/catch → skip.
            };
            for m in &msgs {
                if m.get("type").and_then(Value::as_str) != Some("message") {
                    continue;
                }
                let attaches = m
                    .get("attachments")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().any(|x| x.as_str() == Some(file_id)))
                    .unwrap_or(false);
                if !attaches {
                    continue;
                }
                let msg_id = m.get("id").and_then(Value::as_str).unwrap_or("");
                if chat_in_linked || linked_to.iter().any(|l| l == msg_id) {
                    out.push(FileAssocMessage {
                        chat_id: chat_id.to_string(),
                        chat_name: chat
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        message_id: msg_id.to_string(),
                    });
                }
            }
        }
        Ok(out)
    })?;

    Ok(FileAssociations {
        characters,
        messages,
    })
}

// ===========================================================================
// GET/POST /api/v1/files/folders
// ===========================================================================

/// v4 folders GET (`folders/route.ts:137`): `findByUserId` → projectId scope →
/// **`path.localeCompare` ASC** → the row map. Body `{ folders, count }`.
///
/// **Faithful v4 quirk:** the general scope (`projectId` absent) filters
/// `f.projectId === null` STRICTLY, but `findByUserId` marshals a NULL projectId
/// column to `undefined` — so general folders NEVER match and the general list is
/// ALWAYS empty. Reproduced here (the SPA folder tree for general files is empty on
/// both v4 and v5).
pub fn files_folders_list(db: &Db, user_id: &str, project_id: Option<&str>) -> Response {
    let uid = user_id.to_string();
    let result = db.read_main(move |c| FoldersRepository::new(c).find_by_user_id(&uid));
    let mut folders = match result {
        Ok(f) => f,
        Err(e) => return internal(e),
    };
    match or_null(project_id) {
        Some(pid) => folders.retain(|f| f.project_id.as_deref() == Some(pid.as_str())),
        // General scope: v4's strict `=== null` never matches the undefined-marshaled
        // column → empty.
        None => folders.clear(),
    }
    folders.sort_by(|a, b| locale_compare(&a.path, &b.path));
    let body: Vec<Value> = folders.iter().map(serialize_folder).collect();
    let count = body.len();
    Response::Files(json!({ "folders": body, "count": count }))
}

/// v4 `handleCreateFolder`: validate → idempotent (`findByPath` → `{folder,
/// alreadyExists:true}`) → `ensureParentFoldersExist` → create → `{folder,
/// alreadyExists:false}`.
pub async fn files_folder_create(
    db: &Db,
    user_id: &str,
    path: &str,
    project_id: Option<&str>,
) -> Response {
    if let Err(msg) = validate_folder_path(path) {
        return bad_request(msg);
    }
    let normalized = normalize_folder_path(Some(path));
    let uid = user_id.to_string();
    let pid = or_null(project_id);
    let out = db
        .write(move |ws| {
            let c = ws.main().connection();
            let repo = FoldersRepository::new(c);
            if let Some(existing) = repo.find_by_path(&uid, &normalized, pid.as_deref())? {
                return Ok(Response::Files(json!({
                    "folder": serialize_folder_read(&existing),
                    "alreadyExists": true,
                    "message": "Folder already exists",
                })));
            }
            let parent_id = ensure_parent_folders_exist(c, &uid, &normalized, pid.as_deref())?;
            let name = {
                let n = get_folder_name(&normalized);
                if n.is_empty() {
                    "Folder".to_string()
                } else {
                    n
                }
            };
            repo.create(
                &FolderCreate {
                    user_id: uid.clone(),
                    path: normalized.clone(),
                    name,
                    parent_folder_id: parent_id,
                    project_id: pid.clone(),
                },
                &Default::default(),
            )?;
            // Re-read for the minted id + timestamps; echo with the CREATED shape
            // (present nulls — v4 returns the created entity, not a fresh read).
            let created = repo.find_by_path(&uid, &normalized, pid.as_deref())?;
            Ok(match created {
                Some(f) => Response::Files(json!({
                    "folder": serialize_folder_created(&f),
                    "alreadyExists": false,
                    "message": "Folder created successfully",
                })),
                None => internal("folder vanished after create"),
            })
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

/// v4 `handleRenameFolder`: validate path + name → root-refusal → rename the
/// folder → `updatePathPrefix` for descendants → rewrite affected file folderPaths.
/// Body `{oldPath, newPath, foldersUpdated, filesUpdated}`.
pub async fn files_folder_rename(
    db: &Db,
    user_id: &str,
    path: &str,
    new_name: &str,
    project_id: Option<&str>,
) -> Response {
    if let Err(msg) = validate_folder_path(path) {
        return bad_request(msg);
    }
    let sanitized = match validate_folder_name(new_name) {
        Ok(s) => s,
        Err(msg) => return bad_request(msg),
    };
    let normalized = normalize_folder_path(Some(path));
    if normalized == "/" {
        return bad_request("Cannot rename root folder");
    }
    let parent_path = get_parent_path(&normalized);
    let new_path = normalize_folder_path(Some(&format!("{parent_path}{sanitized}")));
    if let Err(msg) = validate_folder_path(&new_path) {
        return bad_request(msg);
    }

    let uid = user_id.to_string();
    let pid = or_null(project_id);
    let out = db
        .write(move |ws| {
            let c = ws.main().connection();
            let folders = FoldersRepository::new(c);
            let Some(folder) = folders.find_by_path(&uid, &normalized, pid.as_deref())? else {
                return Ok(not_found("Folder"));
            };
            folders.update(
                &folder.id,
                &FolderUpdate {
                    name: Some(sanitized.clone()),
                    path: Some(new_path.clone()),
                    updated_at: crate::clock::now_iso(),
                },
            )?;
            let descendant_updated =
                folders.update_path_prefix(&uid, &normalized, &new_path, pid.as_deref())?;

            let files_repo = FilesRepository::new(c);
            let all_files = match pid.as_deref() {
                Some(p) => files_repo.find_by_project_id(&uid, p)?,
                None => files_repo.find_general_files(&uid)?,
            };
            let mut files_updated = 0usize;
            for file in &all_files {
                let old_file_path = file.folder_path.clone().unwrap_or_else(|| "/".to_string());
                if old_file_path == normalized || old_file_path.starts_with(&normalized) {
                    // v4's un-anchored `oldFilePath.replace(normalizedPath, newPath)`
                    // (first occurrence — the prefix is at index 0).
                    let new_file_path = old_file_path.replacen(&normalized, &new_path, 1);
                    files_repo.update(
                        &file.id,
                        &FileUpdate {
                            folder_path: Some(new_file_path),
                            updated_at: crate::clock::now_iso(),
                            ..Default::default()
                        },
                    )?;
                    files_updated += 1;
                }
            }
            Ok(Response::Files(json!({
                "oldPath": normalized,
                "newPath": new_path,
                "foldersUpdated": descendant_updated + 1,
                "filesUpdated": files_updated,
            })))
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

/// v4 `handleDeleteFolder`: validate → root-refusal → not-found → two DISTINCT
/// non-empty 400s (files vs subfolders) → delete. Body `{message, path}`.
pub async fn files_folder_delete(
    db: &Db,
    user_id: &str,
    path: &str,
    project_id: Option<&str>,
) -> Response {
    if let Err(msg) = validate_folder_path(path) {
        return bad_request(msg);
    }
    let normalized = normalize_folder_path(Some(path));
    if normalized == "/" {
        return bad_request("Cannot delete root folder");
    }
    let uid = user_id.to_string();
    let pid = or_null(project_id);
    let out = db
        .write(move |ws| {
            let c = ws.main().connection();
            let folders = FoldersRepository::new(c);
            let Some(folder) = folders.find_by_path(&uid, &normalized, pid.as_deref())? else {
                return Ok(not_found("Folder"));
            };
            let files_repo = FilesRepository::new(c);
            let all_files = match pid.as_deref() {
                Some(p) => files_repo.find_by_project_id(&uid, p)?,
                None => files_repo.find_general_files(&uid)?,
            };
            let in_folder = all_files
                .iter()
                .filter(|f| {
                    let fp = f.folder_path.clone().unwrap_or_else(|| "/".to_string());
                    fp == normalized || fp.starts_with(&normalized)
                })
                .count();
            if in_folder > 0 {
                return Ok(bad_request(format!(
                    "Folder contains {in_folder} file(s) and cannot be deleted"
                )));
            }
            if folders.has_children(&folder.id)? {
                return Ok(bad_request(
                    "Folder contains subfolders and cannot be deleted",
                ));
            }
            folders.delete(&folder.id)?;
            Ok(Response::Files(json!({
                "message": "Folder deleted successfully",
                "path": normalized,
            })))
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// POST /api/v1/files?action=upload (the general upload leg — saveFileEntry)
// ===========================================================================

/// v4 `handleUploadFile` (`files/actions/upload.ts`) over `saveFileEntry`
/// (`files/shared.ts:117`): normalize/validate the folder path, infer the mime,
/// **category hard-coded `'DOCUMENT'`** (v4 `upload.ts:64`), sanitize → sha256 →
/// overwrite-reuse (same userId+projectId+folderPath+filename) → project-store vs
/// Quilltap-Uploads-mount write → `create` (201) or `update` (200, reuses the
/// fileId). Body `{ data: serializeFileEntry }`. `link_ids` is v4's
/// `tags.map(t => t.tagId)` — used for BOTH `linkedTo` AND the file's `tags`
/// column (`upload.ts:55,66`).
///
/// The pixel work uses [`NotConfiguredPixelCodec`] (this dispatch path threads no
/// host codec — a `DOCUMENT` upload never transcodes, and an image upload takes
/// v4's own sharp-unavailable passthrough branch: original bytes, original mime).
#[allow(clippy::too_many_arguments)]
pub async fn file_upload(
    db: &Db,
    user_id: &str,
    filename: &str,
    content_type: &str,
    data: Vec<u8>,
    link_ids: Vec<String>,
    project_id: Option<String>,
    folder_path_raw: Option<String>,
) -> Response {
    let folder_path = match normalize_and_validate_folder_path(folder_path_raw.as_deref()) {
        Ok(p) => p,
        Err(msg) => return bad_request(msg),
    };
    let mime_type = infer_mime_type(filename, content_type);
    let uid = user_id.to_string();
    let filename = filename.to_string();
    let out = db
        .write(move |ws| -> Result<Response, DbError> {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .ok_or_else(|| {
                    DbError::Key("file upload requires the mount-index database".into())
                })?
                .connection();
            save_file_entry(
                main,
                mount,
                &uid,
                &filename,
                &data,
                &mime_type,
                project_id.as_deref(),
                &folder_path,
                "DOCUMENT",
                &link_ids,
            )
        })
        .await;
    match out {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

/// v4 `saveFileEntry` (`files/shared.ts:117`) — the shared upload chokepoint,
/// conn-level (runs on the writer). Returns the `{data}` body; the create-vs-
/// overwrite status (201/200) is derived at the web edge from `createdAt ==
/// updatedAt` (a create mints both equal; an overwrite bumps only `updatedAt`).
#[allow(clippy::too_many_arguments)]
fn save_file_entry(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    filename: &str,
    data: &[u8],
    mime_type: &str,
    project_id: Option<&str>,
    folder_path: &str,
    category: &str,
    link_ids: &[String],
) -> Result<Response, DbError> {
    let sanitized = sanitize_filename(filename);
    let sha256 = sha256_of_buffer(data);
    let files = FilesRepository::new(main);

    // findAndPrepareOverwrite: same-scope filename match reuses the fileId (its old
    // bytes + thumbnails are v4-best-effort-deleted — a DB-invisible side effect,
    // skipped here, matching v4's try/catch-and-log).
    let overwrite = files
        .find_by_filename_in_scope(user_id, project_id, folder_path, &sanitized)?
        .into_iter()
        .next();

    // Write the bytes (project store vs the Quilltap Uploads mount).
    let codec = NotConfiguredPixelCodec;
    let stored: StoredBlob = if let Some(pid) = project_id {
        write_project_file_to_mount_store(
            main,
            mount,
            &codec,
            pid,
            &sanitized,
            data,
            mime_type,
            Some(folder_path),
            None,
        )?
    } else {
        // v4: IMAGE → 'images', else → 'uploads'. (`shared.ts:152`.)
        let subfolder = if category == "IMAGE" {
            "images"
        } else {
            "uploads"
        };
        write_user_upload_to_mount_store(
            main, mount, &codec, &sanitized, data, mime_type, subfolder, None,
        )?
    };
    let storage_key = stored.storage_key();
    let stored_mime = stored.stored_mime_type;
    let stored_size = stored.size_bytes as f64;

    let file_id = if let Some(existing) = overwrite {
        let id = existing.id.clone();
        files.update(
            &id,
            &FileUpdate {
                sha256: Some(sha256),
                mime_type: Some(stored_mime),
                size: Some(stored_size),
                storage_key: Some(storage_key),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )?;
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        // v4 create mints createdAt == updatedAt (one `new Date().toISOString()`).
        let now = crate::clock::now_iso();
        files.create(
            &FileCreate {
                user_id: user_id.to_string(),
                sha256,
                original_filename: sanitized,
                mime_type: stored_mime,
                size: stored_size,
                width: None,
                height: None,
                is_plain_text: None,
                linked_to: link_ids.to_vec(),
                source: "UPLOADED".to_string(),
                category: category.to_string(),
                generation_prompt: None,
                generation_model: None,
                generation_revised_prompt: None,
                description: None,
                tags: link_ids.to_vec(),
                project_id: project_id.map(str::to_string),
                folder_path: Some(folder_path.to_string()),
                storage_key: Some(storage_key),
                file_status: "ok".to_string(),
            },
            &CreateOptions {
                id: id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        id
    };

    let entry = files
        .find_full_by_id(&file_id)?
        .ok_or_else(|| DbError::Key("uploaded file vanished after write".to_string()))?;
    Ok(Response::Files(
        json!({ "data": serialize_uploaded_file_entry(&entry) }),
    ))
}

/// The upload/overwrite echo (v4 `serializeFileEntry` over the CREATED/UPDATED
/// entity). Same shape as [`serialize_file_entry`] EXCEPT `projectId` is ALWAYS
/// present (null or value) — the create explicitly sets `projectId` (so it
/// survives as `null`), while `description`/`width`/`height` are NOT in the create
/// data and stay omitted (the P4.6k create-echo pattern).
fn serialize_uploaded_file_entry(f: &FileFull) -> Value {
    let mut m = serialize_file_entry(f);
    if let Value::Object(map) = &mut m {
        map.entry("projectId").or_insert(Value::Null);
    }
    m
}

// ===========================================================================
// POST /api/v1/files?action=generate-thumbnails|cleanup-stale|cleanup-orphans
// ===========================================================================

/// v4 `handleGenerateThumbnails` (`files/actions/generate-thumbnails.ts`) — the
/// owned + resizable filter (`total`) over the requested `file_ids`. The actual
/// generation is a host image-codec + disk-cache leg NOT wired at the dispatch
/// layer (the on-demand `?action=thumbnail` byte-GET route carries the codec and
/// pre-warms on demand — unaffected); at dispatch nothing is generated, so
/// `generated`/`cached`/`errors` are `0` (the batch pre-warm is a no-op here — an
/// enumerated codec leg; the differential pins only the DB-provable `total`). Body
/// `{total, generated, cached, errors}`.
pub fn files_generate_thumbnails(db: &Db, user_id: &str, file_ids: &[String]) -> Response {
    let ids: Vec<String> = file_ids.to_vec();
    let uid = user_id.to_string();
    let result = db.read_main(move |c| {
        let repo = FilesRepository::new(c);
        let mut valid = 0usize;
        for id in &ids {
            if let Some(f) = repo.find_full_by_id(id)? {
                if f.user_id == uid && can_generate_thumbnail(&f.mime_type) {
                    valid += 1;
                }
            }
        }
        Ok(valid)
    });
    let total = match result {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    // Generation deferred to the codec-bearing byte-GET route → dispatch no-op.
    Response::Files(json!({
        "total": total,
        "generated": 0,
        "cached": 0,
        "errors": 0,
    }))
}

/// v4 `handleCleanupStale` (`files/actions/cleanup-stale.ts`) — a file is stale
/// when its `storageKey` no longer resolves to bytes. Mount-blob keys are checked
/// in-DB ([`storage_key_exists`](crate::services::file_storage::storage_key_exists)
/// over the mount-index read pool); disk keys go through the injected `backend`.
/// The dispatch engine passes a [`NotConfiguredStorageBackend`] (disk keys then
/// record a per-file error, matching v4's try/catch — the enumerated fs leg, a
/// production degradation for legacy disk-key files); the differential injects an
/// all-missing backend mirroring v4's empty scratch disk. NULL/empty keys are
/// skipped. `dryRun` default true. Body `{total, stale, deleted, dryRun,
/// staleFiles, errors?}`.
pub async fn files_cleanup_stale(
    db: &Db,
    backend: &dyn crate::services::file_storage::StorageBackend,
    user_id: &str,
    dry_run: bool,
) -> Response {
    let uid = user_id.to_string();
    let files = match db.read_main(move |c| FilesRepository::new(c).find_by_user_id(&uid)) {
        Ok(f) => f,
        Err(e) => return internal(e),
    };
    let mut stale: Vec<(String, String)> = Vec::new(); // (id, originalFilename)
    let mut errors: Vec<String> = Vec::new();
    for f in &files {
        let Some(key) = f.storage_key.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        match crate::services::file_storage::storage_key_exists(db, backend, key) {
            Ok(true) => {}
            Ok(false) => stale.push((f.id.clone(), f.original_filename.clone())),
            Err(e) => errors.push(format!("Error checking file {}: {e}", f.id)),
        }
    }

    let mut deleted = 0usize;
    if !dry_run && !stale.is_empty() {
        for (id, _) in &stale {
            let fid = id.clone();
            match db
                .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid))
                .await
            {
                Ok(_) => deleted += 1,
                Err(_) => { /* v4 logs + continues */ }
            }
        }
    }

    let stale_files: Vec<Value> = stale
        .iter()
        .map(|(id, filename)| json!({ "id": id, "filename": filename }))
        .collect();
    let mut body = Map::new();
    body.insert("total".into(), json!(files.len()));
    body.insert("stale".into(), json!(stale.len()));
    body.insert("deleted".into(), json!(deleted));
    body.insert("dryRun".into(), json!(dry_run));
    body.insert("staleFiles".into(), json!(stale_files));
    // v4 `errors: errors.length > 0 ? errors : undefined` — omit when empty.
    if !errors.is_empty() {
        body.insert("errors".into(), json!(errors));
    }
    Response::Files(Value::Object(body))
}

/// v4 `handleCleanupOrphans` (`files/actions/cleanup-orphans.ts`): partition
/// `fileStatus:'orphaned'` rows into rescued (referenced by a character
/// default/override image, a non-empty `linkedTo`, or a sha256 shared with a
/// referenced file), duplicate (sha256 shared with a NON-orphaned tracked file),
/// and unique. `dryRun` returns the counts; a wet `delete` removes duplicates (+
/// unique when `mode=delete`); a wet `move` relocates unique to `/orphans/`
/// (fileStatus→ok, projectId→null) and deletes duplicates. Best-effort storage
/// deletes (mount-blob GC / disk) are v4-caught; the fixture seeds NULL-key
/// orphans so the metadata delete is the whole effect.
pub async fn files_cleanup_orphans(db: &Db, user_id: &str, mode: &str, dry_run: bool) -> Response {
    let uid = user_id.to_string();
    let (all_files, characters) = match db.read_main(|main| {
        db.read_mount_index(|mount| {
            let files = FilesRepository::new(main).find_by_user_id(&uid)?;
            let chars = characters_read::find_all(main, mount)?;
            Ok((files, chars))
        })
    }) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };

    // buildReferencedFileSets: character default/override image ids + any file with
    // a non-empty linkedTo → referencedIds; the sha256 of each referenced file →
    // referencedHashes.
    let mut referenced_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in &characters {
        if let Some(d) = c.get("defaultImageId").and_then(Value::as_str) {
            referenced_ids.insert(d.to_string());
        }
        if let Some(overrides) = c.get("avatarOverrides").and_then(Value::as_array) {
            for o in overrides {
                if let Some(img) = o.get("imageId").and_then(Value::as_str) {
                    referenced_ids.insert(img.to_string());
                }
            }
        }
    }
    for f in &all_files {
        if !f.linked_to.is_empty() {
            referenced_ids.insert(f.id.clone());
        }
    }
    // referencedHashes: sha256 of each referenced file. FileFull carries no sha256,
    // so re-read the hash for the referenced ids.
    let mut referenced_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for id in &referenced_ids {
        let fid = id.clone();
        if let Ok(Some(sha)) = db.read_main(move |c| {
            c.query_row(
                "SELECT sha256 FROM files WHERE id = ?1",
                rusqlite::params![fid],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(crate::db::DbError::from)
        }) {
            referenced_hashes.insert(sha);
        }
    }

    let is_referenced = |f: &FileFull, sha: &str| -> bool {
        referenced_ids.contains(&f.id)
            || !f.linked_to.is_empty()
            || (!sha.is_empty() && referenced_hashes.contains(sha))
    };

    // Re-read each file's sha for the partition (FileFull has none).
    let sha_of = |f: &FileFull| -> String {
        let fid = f.id.clone();
        db.read_main(move |c| {
            c.query_row(
                "SELECT sha256 FROM files WHERE id = ?1",
                rusqlite::params![fid],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(crate::db::DbError::from)
        })
        .ok()
        .flatten()
        .unwrap_or_default()
    };

    let mut rescued: Vec<FileFull> = Vec::new();
    let mut orphaned: Vec<FileFull> = Vec::new();
    let mut tracked_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for f in &all_files {
        let sha = sha_of(f);
        let is_orphan = f.file_status.as_deref() == Some("orphaned");
        if is_orphan {
            if is_referenced(f, &sha) {
                rescued.push(f.clone());
            } else {
                orphaned.push(f.clone());
            }
        } else {
            tracked_hashes.insert(sha);
        }
    }

    let duplicates: Vec<FileFull> = orphaned
        .iter()
        .filter(|f| tracked_hashes.contains(&sha_of(f)))
        .cloned()
        .collect();
    let unique: Vec<FileFull> = orphaned
        .iter()
        .filter(|f| !tracked_hashes.contains(&sha_of(f)))
        .cloned()
        .collect();
    let total_size: i64 = orphaned.iter().map(|f| f.size).sum();
    let duplicate_size: i64 = duplicates.iter().map(|f| f.size).sum();
    let unique_size: i64 = unique.iter().map(|f| f.size).sum();

    if dry_run {
        return Response::Files(json!({
            "orphanedCount": orphaned.len(),
            "rescuedCount": rescued.len(),
            "duplicateCount": duplicates.len(),
            "uniqueCount": unique.len(),
            "totalSize": total_size,
            "duplicateSize": duplicate_size,
            "uniqueSize": unique_size,
            "dryRun": true,
        }));
    }

    // Wet: rescue (fileStatus→ok), then delete duplicates (+ unique in delete
    // mode), then move unique (move mode only).
    let mut rescued_actual = 0usize;
    for f in &rescued {
        let fid = f.id.clone();
        if db
            .write(move |ws| {
                FilesRepository::new(ws.main().connection()).update(
                    &fid,
                    &FileUpdate {
                        file_status: Some("ok".to_string()),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    },
                )
            })
            .await
            .is_ok()
        {
            rescued_actual += 1;
        }
    }

    let mut deleted = 0usize;
    let mut moved = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let to_delete: Vec<&FileFull> = if mode == "delete" {
        duplicates.iter().chain(unique.iter()).collect()
    } else {
        duplicates.iter().collect()
    };
    for f in to_delete {
        // Best-effort storage delete is DB-invisible for NULL-key fixtures (v4
        // skips it); the metadata delete is the whole effect.
        let fid = f.id.clone();
        match db
            .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid))
            .await
        {
            Ok(_) => deleted += 1,
            Err(e) => errors.push(format!("Error deleting file {}: {e}", f.id)),
        }
    }
    if mode == "move" {
        for f in &unique {
            let fid = f.id.clone();
            match db
                .write(move |ws| {
                    FilesRepository::new(ws.main().connection()).apply_move_update(
                        &fid,
                        None,
                        "/orphans/",
                        MoveProjectId::Clear,
                    )?;
                    // apply_move_update doesn't set fileStatus — do it in the same tx.
                    ws.main().connection().execute(
                        "UPDATE files SET fileStatus = 'ok' WHERE id = ?1",
                        rusqlite::params![fid],
                    )?;
                    Ok::<_, DbError>(())
                })
                .await
            {
                Ok(_) => moved += 1,
                Err(e) => errors.push(format!("Error moving file {}: {e}", f.id)),
            }
        }
    }

    let mut body = Map::new();
    body.insert("orphanedCount".into(), json!(orphaned.len()));
    body.insert("rescuedCount".into(), json!(rescued_actual));
    body.insert("duplicateCount".into(), json!(duplicates.len()));
    body.insert("uniqueCount".into(), json!(unique.len()));
    body.insert("totalSize".into(), json!(total_size));
    body.insert("deleted".into(), json!(deleted));
    body.insert("moved".into(), json!(moved));
    body.insert("dryRun".into(), json!(false));
    body.insert("mode".into(), json!(mode));
    if !errors.is_empty() {
        body.insert("errors".into(), json!(errors));
    }
    Response::Files(Value::Object(body))
}

/// v4 `sync` action (`files/actions/sync.ts` → `reconcileFilesystem`) — the
/// unported `lib/file-storage/reconciliation` subsystem. Loud refusal.
pub fn files_sync() -> Response {
    Response::error(
        ErrorKind::Internal,
        "The files 'sync' action is recognized but not yet available (the \
         lib/file-storage/reconciliation subsystem is unported).",
    )
}

// ===========================================================================
// Local validators + helpers
// ===========================================================================

/// v4 `sanitizeFilename` (`files/shared.ts:217`) — `/[/\\:*?"<>|]/g` → `_`. NOTE:
/// narrower than [`crate::services::file_storage::safe_filename`] (no `__` collapse,
/// no trim, no control-char strip) — this is the upload-route sanitizer.
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect()
}

/// v4 `inferMimeType` (`files/shared.ts:85`) — a detected non-generic mime wins;
/// otherwise a small extension → mime map over the sanitized lowercased name,
/// falling back to `application/octet-stream`.
fn infer_mime_type(filename: &str, detected_mime_type: &str) -> String {
    if !detected_mime_type.is_empty() && detected_mime_type != "application/octet-stream" {
        return detected_mime_type.to_string();
    }
    let sanitized = sanitize_filename(filename).to_lowercase();
    let ext = sanitized.rsplit('.').next().unwrap_or("");
    match ext {
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// v4 `canGenerateThumbnail` (`thumbnail-utils.ts:62`) — `image/*` AND
/// [`can_resize_image`].
fn can_generate_thumbnail(mime_type: &str) -> bool {
    mime_type.starts_with("image/") && can_resize_image(mime_type)
}

/// v4 `validateFilename` (`files/[id]/shared.ts:49`) — `Ok(())` valid, `Err(msg)`.
fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("Filename cannot be empty".to_string());
    }
    if filename.encode_utf16().count() > 255 {
        return Err("Filename must be 255 characters or less".to_string());
    }
    if filename.chars().any(|c| {
        matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
            || ('\u{0}'..='\u{1f}').contains(&c)
    }) {
        return Err("Filename contains invalid characters".to_string());
    }
    Ok(())
}

/// v4 route-local `validateFolderName` (`folders/route.ts:54`) — returns the
/// trimmed name on success. Rejects empties, >100 chars, `<>:"|?*` + slashes +
/// C0 controls, and `.`/`..`.
fn validate_folder_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if trimmed.encode_utf16().count() > 100 {
        return Err("Folder name must be 100 characters or less".to_string());
    }
    if trimmed.chars().any(|c| {
        matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\')
            || ('\u{0}'..='\u{1f}').contains(&c)
    }) {
        return Err("Folder name contains invalid characters".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("Invalid folder name".to_string());
    }
    Ok(trimmed.to_string())
}

/// v4 `normalizeAndValidateFolderPath` (`files/shared.ts:66`) — normalize
/// `raw || '/'` then `validateFolderPath`; `Ok(normalized)` or `Err(message)`.
fn normalize_and_validate_folder_path(raw: Option<&str>) -> Result<String, String> {
    let folder_path = normalize_folder_path(Some(raw.filter(|s| !s.is_empty()).unwrap_or("/")));
    validate_folder_path(&folder_path)?;
    Ok(folder_path)
}

/// v4 `ensureParentFoldersExist` (`folders/route.ts:86`) — recursively create the
/// missing parent chain, returning the immediate parent's id (`None` at root).
/// Storage-folder creation is a best-effort DB-invisible side effect (skipped).
fn ensure_parent_folders_exist(
    c: &rusqlite::Connection,
    user_id: &str,
    path: &str,
    project_id: Option<&str>,
) -> Result<Option<String>, DbError> {
    if path == "/" {
        return Ok(None);
    }
    let parent_path = get_parent_path(path);
    if parent_path == "/" {
        return Ok(None);
    }
    let repo = FoldersRepository::new(c);
    if let Some(parent) = repo.find_by_path(user_id, &parent_path, project_id)? {
        return Ok(Some(parent.id));
    }
    let grandparent_id = ensure_parent_folders_exist(c, user_id, &parent_path, project_id)?;
    let name = {
        let n = get_folder_name(&parent_path);
        if n.is_empty() {
            "Folder".to_string()
        } else {
            n
        }
    };
    let id = repo.create(
        &FolderCreate {
            user_id: user_id.to_string(),
            path: parent_path,
            name,
            parent_folder_id: grandparent_id,
            project_id: project_id.map(str::to_string),
        },
        &Default::default(),
    )?;
    Ok(Some(id))
}

/// v4 `projects.findByIdRaw(id)` existence gate (never reads the store — a degraded
/// store must not throw). A raw main-DB existence probe.
fn project_exists(c: &rusqlite::Connection, project_id: &str) -> Result<bool, DbError> {
    let found: Option<i64> = c
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            rusqlite::params![project_id],
            |r| r.get::<_, i64>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(found.is_some())
}
