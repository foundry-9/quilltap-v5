//! The courier + chat-images dispatch handlers (P4.6ab lane A) — a differential
//! port of v4's Salon courier/image route handlers, composed over the already-ported
//! services (each is WRAPPED here, never re-implemented):
//!
//!   - `messageResolveExternalTurn` / `messageCancelExternalTurn` — v4
//!     `chats/[id]/messages/[messageId]/route.ts` (`handleResolveExternalTurn` /
//!     `handleCancelExternalTurn`), over
//!     [`courier_transport::resolve_external_turn`] /
//!     [`courier_transport::cancel_external_turn`] (W4.4a4). The resolve settle's
//!     three post-turn triggers are v4-fire-and-forget and fire only when a
//!     connection profile resolves; the differential fixture resolves none, so the
//!     route's synchronous contract (the settle writes + the envelope) is what's
//!     pinned. The trigger machinery is independently tier-3 proven.
//!   - `messageSaveImage` — `handleSaveImage`, over
//!     [`save_image_to_album`](crate::photos::save_image_to_album) (W4.9b) behind the
//!     injected [`FileBytesStore`] bytes seam (the `keep_image` precedent).
//!   - `chatPhotoAlbums` — `handleGetPhotoAlbums` (`actions/photo-albums.ts`).
//!   - `chatAddToolResult` — `handleAddToolResult` (`actions/tools.ts`): the
//!     user-initiated `generate_image` recorder that mints a Prospero-authored TOOL
//!     message.
//!
//! Pinned by `courier_images_routes_equivalence`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::files::FilesRepository;
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_messages_read, chats_read, doc_mount_points::DocMountPointsRepository,
    instance_settings, project_doc_mount_links::ProjectDocMountLinksRepository, projects,
};
use crate::photos::keep_image_markdown::KeptImageAttributionRole;
use crate::photos::save_image_to_album::{
    save_image_to_album, FileBytesStore, SaveImageAttribution, SaveImageErrorCode,
    SaveImageSideEffects, SaveImageToAlbumInput,
};
use crate::services::courier_transport::{
    cancel_external_turn, CancelExternalTurnOutcome, ResolveExternalTurnOutcome,
};

use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// Nest [`Db::read_main`] + [`Db::read_mount_index`] (separate pools → no deadlock)
/// for the vault overlay reads (the `api::characters` precedent).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&Connection, &Connection) -> Result<T, crate::db::DbError>,
) -> Result<T, crate::db::DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// v4 `getCharacterVaultStore` — the overlaid character's
/// `characterDocumentMountPointId` → the mount point (must be `mountType='database'`
/// + `storeType='character'`). Returns `(mountPointId, mountPointName, characterName)`.
fn character_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Option<(String, String, String)> {
    let character = characters_read::find_by_id(main, mount, character_id).ok()??;
    let character_name = character.get("name").and_then(Value::as_str)?.to_string();
    let mp_id = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)?;
    let mp = DocMountPointsRepository::new(mount)
        .find_store_naming_by_id(mp_id)
        .ok()??;
    if mp.mount_type != "database" || mp.store_type.as_deref() != Some("character") {
        return None;
    }
    Some((mp.id, mp.name, character_name))
}

/// v4 auth `user` (single-user): `(name, username)` from the `users` row.
fn read_user(main: &Connection, user_id: &str) -> Option<(Option<String>, Option<String>)> {
    main.query_row(
        "SELECT name, username FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        },
    )
    .ok()
}

/// The message row (a `type=='message'` event) by id, from a fresh messages read.
fn find_message(messages: &[Value], message_id: &str) -> Option<Value> {
    messages
        .iter()
        .find(|m| {
            m.get("id").and_then(Value::as_str) == Some(message_id)
                && m.get("type").and_then(Value::as_str) == Some("message")
        })
        .cloned()
}

// ===========================================================================
// Courier: resolve / cancel external turn
// ===========================================================================

/// The courier-resolve driver seam (the swipe-generate precedent): a boxed-future
/// trait object the host implements over the Db + completion provider + cheap
/// executor, since the resolve settle re-enters the cheap-LLM triggers when a
/// connection profile resolves. `None` in the assembly → a loud refusal until the
/// unification wire. The host impl composes
/// [`resolve_external_turn`](crate::services::courier_transport::resolve_external_turn)
/// → [`resolve_outcome_to_response`].
pub type CourierResolveFuture<'a> = Pin<Box<dyn Future<Output = Response> + Send + 'a>>;

pub trait CourierResolveDriver: Send + Sync {
    fn resolve_external_turn(
        &self,
        chat_id: String,
        message_id: String,
        reply_content: String,
    ) -> CourierResolveFuture<'_>;
}

/// Map [`ResolveExternalTurnOutcome`] → v4's `handleResolveExternalTurn` envelope.
/// (The service performs every gate + the settle; this only translates the outcome —
/// the whole `api::chat_media` resolve function lives on the driver seam because the
/// settle re-enters the cheap-LLM triggers when a connection profile resolves.)
pub fn resolve_outcome_to_response(outcome: ResolveExternalTurnOutcome) -> Response {
    match outcome {
        ResolveExternalTurnOutcome::ChatNotFound => not_found("Chat"),
        ResolveExternalTurnOutcome::MessageNotFound => not_found("Message"),
        ResolveExternalTurnOutcome::NotAwaiting => {
            bad_request("Message is not awaiting an external reply")
        }
        ResolveExternalTurnOutcome::NotAssistant => {
            bad_request("Only assistant placeholder messages can be resolved")
        }
        ResolveExternalTurnOutcome::Resolved {
            message_id,
            participant_id,
        } => Response::ChatMedia(json!({
            "resolved": true,
            "messageId": message_id,
            "participantId": participant_id,
        })),
    }
}

/// v4 `handleResolveExternalTurn`. Wraps
/// [`resolve_external_turn`](crate::services::courier_transport::resolve_external_turn)
/// (the settle + the three post-turn triggers, which fire only when a connection
/// profile resolves) and maps the outcome to v4's envelope. The completion provider
/// and cheap executor are supplied by the host driver (production) or a mock (the
/// differential, whose fixture resolves no profile → triggers skipped).
#[allow(clippy::too_many_arguments)]
pub async fn message_resolve_external_turn<C: crate::model::completion::CompletionProvider>(
    db: &Db,
    completion: &C,
    executor: &crate::services::cheap_llm_exec::CheapLlmTaskExecutor,
    user_id: &str,
    chat_id: &str,
    message_id: &str,
    reply_content: &str,
    now_ms: i64,
) -> Response {
    match crate::services::courier_transport::resolve_external_turn(
        db,
        completion,
        executor,
        chat_id,
        message_id,
        reply_content,
        user_id,
        now_ms,
    )
    .await
    {
        Ok(outcome) => resolve_outcome_to_response(outcome),
        Err(e) => internal(e),
    }
}

/// v4 `handleCancelExternalTurn` — delete the placeholder + unpause (no chained turn).
/// Wraps [`cancel_external_turn`] (no model boundary → wired live in dispatch).
pub async fn message_cancel_external_turn(db: &Db, chat_id: &str, message_id: &str) -> Response {
    let now_ms = crate::clock::now_unix_ms();
    match cancel_external_turn(db, chat_id, message_id, now_ms).await {
        Ok(CancelExternalTurnOutcome::ChatNotFound) => not_found("Chat"),
        Ok(CancelExternalTurnOutcome::MessageNotFound) => not_found("Message"),
        Ok(CancelExternalTurnOutcome::NotAwaiting) => {
            bad_request("Message is not awaiting an external reply")
        }
        Ok(CancelExternalTurnOutcome::Cancelled { message_id }) => Response::ChatMedia(json!({
            "cancelled": true,
            "messageId": message_id,
        })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Save image to a chosen album (v4 handleSaveImage)
// ===========================================================================

/// v4 `handleSaveImage`. Gates (chat/message existence + `attachments.includes(fileId)`),
/// resolves attribution (a participant character vault matching `mountPointId`, else the
/// active/first user persona, else the auth user, else "Quilltap"), then
/// [`save_image_to_album`] behind the injected bytes seam. `kept_at` is the injected ISO
/// clock. Returns v4's `{ saved, mountPoint, relativePath, linkId, keptAt, fileId, sha256 }`
/// or `badRequest` on a [`SaveImageToAlbumError`].
#[allow(clippy::too_many_arguments)]
pub async fn message_save_image(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    message_id: &str,
    body: &Value,
    bytes: Arc<dyn FileBytesStore>,
    side_effects: Arc<dyn SaveImageSideEffects + Send + Sync>,
    kept_at: &str,
) -> Response {
    // v4 saveImageSchema: fileId uuid, mountPointId uuid, caption?, tags?.
    let file_id = match body.get("fileId").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return bad_request("fileId must be a UUID"),
    };
    let mount_point_id = match body.get("mountPointId").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return bad_request("mountPointId must be a UUID"),
    };
    let caption = body
        .get("caption")
        .and_then(Value::as_str)
        .map(str::to_string);
    let tags: Vec<String> = body
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    // --- gates ---
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let chat_id_owned = chat_id.to_string();
    let messages = match db.read_main(move |c| chats_messages_read::get_messages(c, &chat_id_owned))
    {
        Ok(m) => m,
        Err(e) => return internal(e),
    };
    let Some(message) = find_message(&messages, message_id) else {
        return not_found("Message");
    };
    let attached = message
        .get("attachments")
        .and_then(Value::as_array)
        .map(|a| a.iter().any(|v| v.as_str() == Some(file_id.as_str())))
        .unwrap_or(false);
    if !attached {
        return bad_request("Image is not attached to this message");
    }

    // --- attribution + save (writer thread holds both connections) ---
    let user_id_owned = user_id.to_string();
    let chat_owned = chat.clone();
    let mp_owned = mount_point_id.clone();
    let file_owned = file_id.clone();
    let caption_owned = caption.clone();
    let tags_owned = tags.clone();
    let chat_id_for_scene = chat_id.to_string();
    let kept_owned = kept_at.to_string();

    let result = db
        .write(move |ws| {
            let mount = super::mount_files::mount_conn(ws)?;
            let main = ws.main().connection();

            // Attribution: prefer a participant character whose vault == mountPointId.
            let mut attribution: Option<SaveImageAttribution> = None;
            if let Some(participants) = chat_owned.get("participants").and_then(Value::as_array) {
                for p in participants {
                    if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                        continue;
                    }
                    let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some((vault_mp_id, vault_mp_name, char_name)) =
                        character_vault(main, mount, cid)
                    else {
                        continue;
                    };
                    if vault_mp_id != mp_owned {
                        continue;
                    }
                    attribution = Some(SaveImageAttribution {
                        // v4: character?.name ?? vault.mountPointName.
                        name: if char_name.is_empty() {
                            vault_mp_name
                        } else {
                            char_name
                        },
                        id: Some(cid.to_string()),
                        role: KeptImageAttributionRole::Character,
                    });
                    break;
                }
            }
            // Else the active-impersonated / first user-controlled persona, else the auth
            // user, else "Quilltap".
            if attribution.is_none() {
                let mut user_persona_name: Option<String> = None;
                let mut user_persona_id: Option<String> = None;
                let active_typing = chat_owned
                    .get("activeTypingParticipantId")
                    .and_then(Value::as_str);
                let participants = chat_owned
                    .get("participants")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let is_user =
                    |p: &Value| p.get("controlledBy").and_then(Value::as_str) == Some("user");
                let active = active_typing.and_then(|id| {
                    participants
                        .iter()
                        .find(|p| p.get("id").and_then(Value::as_str) == Some(id) && is_user(p))
                        .cloned()
                });
                let fallback = participants.iter().find(|p| is_user(p)).cloned();
                if let Some(up) = active.or(fallback) {
                    if let Some(cid) = up.get("characterId").and_then(Value::as_str) {
                        if let Ok(Some(ch)) = characters_read::find_by_id(main, mount, cid) {
                            if let Some(name) = ch.get("name").and_then(Value::as_str) {
                                user_persona_name = Some(name.to_string());
                                user_persona_id =
                                    ch.get("id").and_then(Value::as_str).map(str::to_string);
                            }
                        }
                    }
                }
                let (user_name, _username) =
                    read_user(main, &user_id_owned).unwrap_or((None, None));
                attribution = Some(SaveImageAttribution {
                    name: user_persona_name
                        .or(user_name)
                        .unwrap_or_else(|| "Quilltap".to_string()),
                    id: user_persona_id.or_else(|| Some(user_id_owned.clone())),
                    role: KeptImageAttributionRole::User,
                });
            }

            let input = SaveImageToAlbumInput {
                mount_point_id: &mp_owned,
                file_id: &file_owned,
                caption: caption_owned.as_deref(),
                tags: &tags_owned,
                chat_id: Some(&chat_id_for_scene),
                attribution: attribution.unwrap(),
            };
            Ok(save_image_to_album(
                main,
                mount,
                &input,
                &*bytes,
                &*side_effects,
                &kept_owned,
            ))
        })
        .await;

    match result {
        Ok(Ok(saved)) => Response::ChatMedia(json!({
            "saved": true,
            "mountPoint": saved.mount_point_name,
            "relativePath": saved.relative_path,
            "linkId": saved.link_id,
            "keptAt": saved.kept_at,
            "fileId": saved.file_id,
            "sha256": saved.sha256,
        })),
        Ok(Err(err)) => match err.code {
            // v4 surfaces SaveImageToAlbumError as badRequest(message).
            SaveImageErrorCode::ImageNotFound
            | SaveImageErrorCode::NotAnImage
            | SaveImageErrorCode::EmptyBytes
            | SaveImageErrorCode::MountNotFound
            | SaveImageErrorCode::AlreadySaved => bad_request(err.message),
        },
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Photo albums (v4 handleGetPhotoAlbums)
// ===========================================================================

/// v4 `GET /chats/[id]?action=photo-albums` → `{ albums: PhotoAlbumOption[] }`.
/// Participant character vaults, then the project official store + its linked
/// document stores, then Quilltap General; exactly one option is flagged
/// `isDefault`.
pub fn chat_photo_albums(db: &Db, chat_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let Some(chat) = chats_read::find_by_id(main, &chat_id_owned)? else {
            return Ok(None);
        };
        let points = DocMountPointsRepository::new(mount);
        let mut options: Vec<Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // 1. Participant character vaults.
        if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
            for p in participants {
                if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                if p.get("status").and_then(Value::as_str) == Some("removed") {
                    continue;
                }
                let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                    continue;
                };
                let Some((mp_id, mp_name, char_name)) = character_vault(main, mount, cid) else {
                    continue;
                };
                if seen.contains(&mp_id) {
                    continue;
                }
                let display = if char_name.is_empty() {
                    mp_name
                } else {
                    char_name
                };
                options.push(json!({
                    "mountPointId": mp_id,
                    "name": display,
                    "kind": "character",
                    "characterId": cid,
                    "participantId": p.get("id").and_then(Value::as_str),
                    "isUserCharacter": p.get("controlledBy").and_then(Value::as_str) == Some("user"),
                }));
                seen.insert(mp_id);
            }
        }

        // 2. Project official store + 3. linked document stores.
        if let Some(project_id) = chat.get("projectId").and_then(Value::as_str) {
            if let Some(Some(official)) =
                projects::find_official_mount_point_id_raw(main, project_id)?
            {
                if !seen.contains(&official) {
                    if let Some(mp) = points.find_store_naming_by_id(&official)? {
                        options.push(json!({
                            "mountPointId": mp.id,
                            "name": mp.name,
                            "kind": "project",
                        }));
                        seen.insert(mp.id);
                    }
                }
            }
            let links =
                ProjectDocMountLinksRepository::new(mount).find_by_project_id(project_id)?;
            for mp_id in links {
                if seen.contains(&mp_id) {
                    continue;
                }
                let Some(mp) = points.find_store_naming_by_id(&mp_id)? else {
                    continue;
                };
                options.push(json!({
                    "mountPointId": mp.id,
                    "name": mp.name,
                    "kind": "document-store",
                }));
                seen.insert(mp.id);
            }
        }

        // 4. Quilltap General.
        if let Some(general_id) = instance_settings::get_general_mount_point_id(main)? {
            if !seen.contains(&general_id) {
                if let Some(mp) = points.find_store_naming_by_id(&general_id)? {
                    options.push(json!({
                        "mountPointId": mp.id,
                        "name": mp.name,
                        "kind": "general",
                    }));
                    seen.insert(mp.id);
                }
            }
        }

        // 5. Default selection: active impersonated user char → first user char →
        //    general → first.
        let active_typing = chat
            .get("activeTypingParticipantId")
            .and_then(Value::as_str);
        let default_idx = options
            .iter()
            .position(|o| {
                o.get("kind").and_then(Value::as_str) == Some("character")
                    && active_typing.is_some()
                    && o.get("participantId").and_then(Value::as_str) == active_typing
                    && o.get("isUserCharacter").and_then(Value::as_bool) == Some(true)
            })
            .or_else(|| {
                options.iter().position(|o| {
                    o.get("kind").and_then(Value::as_str) == Some("character")
                        && o.get("isUserCharacter").and_then(Value::as_bool) == Some(true)
                })
            })
            .or_else(|| {
                options
                    .iter()
                    .position(|o| o.get("kind").and_then(Value::as_str) == Some("general"))
            })
            .or(if options.is_empty() { None } else { Some(0) });
        if let Some(i) = default_idx {
            options[i]
                .as_object_mut()
                .unwrap()
                .insert("isDefault".into(), Value::Bool(true));
        }

        Ok(Some(Value::Array(options)))
    });
    match result {
        Ok(Some(albums)) => Response::ChatMedia(json!({ "albums": albums })),
        Ok(None) => not_found("Chat"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Add tool result (v4 handleAddToolResult)
// ===========================================================================

/// v4 `POST /chats/[id]?action=add-tool-result`. Mints a TOOL message: user-initiated
/// results render as Prospero-authored standalone bubbles (`systemSender:'prospero'`,
/// `systemKind:'tool-run'`, `operatorName`); character-initiated stay attached with no
/// Staff sender. Returns `{ success, message }`. The minted id/createdAt are remapped by
/// the differential.
pub async fn chat_add_tool_result(db: &Db, user_id: &str, chat_id: &str, body: &Value) -> Response {
    // v4 toolResultSchema: tool (string), initiatedBy enum default 'user', prompt?,
    // result?, images? [{id, filename}].
    let tool = match body.get("tool").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return bad_request("tool is required"),
    };
    let initiated_by = body
        .get("initiatedBy")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
    let is_user = initiated_by == "user";

    let chat_id_owned = chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => {} // v4 addMessage does not gate on chat existence here.
        Err(e) => return internal(e),
    }

    let user_id_owned = user_id.to_string();
    let operator_name = if is_user {
        match db.read_main(move |c| Ok::<_, crate::db::DbError>(read_user(c, &user_id_owned))) {
            Ok(Some((name, username))) => name.or(username),
            Ok(None) => None,
            Err(e) => return internal(e),
        }
    } else {
        None
    };

    // content = JSON.stringify({tool, toolName, initiatedBy, operatorName, prompt,
    // result, images, success}) — insertion-ordered, undefined keys dropped.
    let mut content = Map::new();
    content.insert("tool".into(), json!(tool));
    content.insert("toolName".into(), json!(tool));
    content.insert("initiatedBy".into(), json!(initiated_by));
    if is_user {
        // operatorName is present (may be null if the user has no name/username).
        content.insert(
            "operatorName".into(),
            operator_name
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
    }
    if let Some(prompt) = body.get("prompt").filter(|v| !v.is_null()) {
        content.insert("prompt".into(), prompt.clone());
    }
    if let Some(result) = body.get("result").filter(|v| !v.is_null()) {
        content.insert("result".into(), result.clone());
    }
    if let Some(images) = body.get("images").filter(|v| !v.is_null()) {
        content.insert("images".into(), images.clone());
    }
    // success = user ? true : result?.success ?? false.
    let success = if is_user {
        true
    } else {
        body.get("result")
            .and_then(|r| r.get("success"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    content.insert("success".into(), json!(success));
    let content_str = serde_json::to_string(&Value::Object(content)).unwrap();

    let minted_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    // v4 `addMessage` returns the built message object (systemSender/systemKind
    // present as null for character-initiated — NOT re-marshaled from a DB read,
    // which would drop the nulls). Persist it, then return the built object.
    let message = json!({
        "type": "message",
        "id": minted_id,
        "role": "TOOL",
        "systemSender": if is_user { Value::String("prospero".into()) } else { Value::Null },
        "systemKind": if is_user { Value::String("tool-run".into()) } else { Value::Null },
        "content": content_str,
        "createdAt": now,
        "attachments": Value::Array(vec![]),
    });
    let event: crate::db::chats_messages::ChatEventInput =
        match serde_json::from_value(message.clone()) {
            Ok(e) => e,
            Err(e) => return internal(format!("tool-result parse: {e}")),
        };
    let write_chat_id = chat_id.to_string();
    if let Err(e) = db
        .write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
        .await
    {
        return internal(e);
    }
    Response::ChatMedia(json!({ "success": true, "message": message }))
}

// ===========================================================================
// Chat files: list / delete / upload
// ===========================================================================

/// v4 `GET /api/v1/chats/[id]/files` → `{ files }`: every `files` row linked to the
/// chat, mapped to `{id, filename, filepath, mimeType, size, url, createdAt, type}`
/// (`type` = `source=='GENERATED' ? 'generatedImage' : 'chatFile'`), newest first.
///
/// v4 also walks the chat's messages for Librarian mount-file *announcement*
/// attachments (no link table) — a bounded deferral this round (the courier/images
/// fixture carries none; the Salon gallery consumes uploaded + generated files). It
/// is enumerated in the lane report.
pub fn chat_files_list(db: &Db, chat_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let _ = &chat;
    let chat_id_owned = chat_id.to_string();
    let rows = db.read_main(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, originalFilename, mimeType, size, source, createdAt FROM files \
             WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1)",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![chat_id_owned], |r| {
                let id: String = r.get(0)?;
                let filename: String = r.get(1)?;
                let mime_type: String = r.get(2)?;
                let size: i64 = r.get::<_, Option<f64>>(3)?.unwrap_or(0.0) as i64;
                let source: String = r.get(4)?;
                let created_at: String = r.get(5)?;
                let filepath = format!("/api/v1/files/{id}");
                Ok(json!({
                    "id": id,
                    "filename": filename,
                    "filepath": filepath,
                    "mimeType": mime_type,
                    "size": size,
                    "url": filepath,
                    "createdAt": created_at,
                    "type": if source == "GENERATED" { "generatedImage" } else { "chatFile" },
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, crate::db::DbError>(rows)
    });
    let mut files = match rows {
        Ok(f) => f,
        Err(e) => return internal(e),
    };
    // Newest first (v4 sorts by createdAt desc).
    files.sort_by(|a, b| {
        let at = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
        let bt = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
        bt.cmp(at)
    });
    Response::ChatMedia(json!({ "files": files }))
}

/// v4 `DELETE /api/v1/chat-files/[id]` — resolve the file, derive its chat from
/// `linkedTo` (an entry starting `chat-` or exactly 36 chars), verify the chat
/// exists (v4 `unauthorized` when absent), delete the row → `{ success: true }`. The
/// storage delete (`fileStorageManager.deleteFile`) is a host seam v4 error-swallows;
/// the DB effect is the metadata delete.
pub async fn chat_file_delete(db: &Db, _user_id: &str, file_id: &str) -> Response {
    let id = file_id.to_string();
    let linked: Option<Option<String>> = match db.read_main(move |conn| {
        conn.query_row(
            "SELECT linkedTo FROM files WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(crate::db::DbError::from)
    }) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let Some(linked_json) = linked else {
        return not_found("File");
    };
    let linked_ids: Vec<String> = linked_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let Some(chat_id) = linked_ids
        .into_iter()
        .find(|l| l.starts_with("chat-") || l.len() == 36)
    else {
        return bad_request("File is not associated with a chat");
    };
    let chat_id_owned = chat_id.clone();
    match db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned)) {
        Ok(Some(_)) => {}
        Ok(None) => return Response::error(ErrorKind::Unauthorized, "Unauthorized"),
        Err(e) => return internal(e),
    }
    let id = file_id.to_string();
    let deleted = db
        .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&id))
        .await;
    match deleted {
        Ok(true) => Response::ChatMedia(json!({ "success": true })),
        Ok(false) => not_found("File"),
        Err(e) => internal(e),
    }
}

/// Input for the web-edge chat-file upload leg (v4 `uploadChatFile` inputs).
pub struct ChatFileUploadInput {
    pub filename: String,
    pub content_type: String,
    /// base64 of the file bytes.
    pub data: String,
    pub resolution: Option<String>,
    pub conflicting_file_id: Option<String>,
}

/// v4 `POST /api/v1/chats/[id]/files` (default multipart leg) → `uploadChatFile`.
///
/// **OPEN (loud deferral this round).** The upload port writes bytes through the
/// host storage seam (`writeUserUploadToMountStore` / `fileStorageManager.uploadFile`
/// — a `FileBytesStore`-style boundary) with the project duplicate-detection branch;
/// it lands with the multipart web route in a follow-up. Enumerated in the lane report.
pub async fn chat_file_upload(
    _db: &Db,
    _user_id: &str,
    _chat_id: &str,
    _input: ChatFileUploadInput,
) -> Response {
    Response::error(
        ErrorKind::Internal,
        "The chat-file upload leg is recognized but not yet available.",
    )
}
