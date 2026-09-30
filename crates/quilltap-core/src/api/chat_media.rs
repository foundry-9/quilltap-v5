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
use crate::photos::save_attribution::{parse_save_image_request, resolve_save_attribution};
use crate::photos::save_image_to_album::{
    save_image_to_album, FileBytesStore, SaveImageErrorCode, SaveImageSideEffects,
    SaveImageToAlbumInput,
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
/// profile resolves) and maps the outcome to v4's envelope. The completion,
/// embedding and cheap executor are supplied by the host driver (production) or
/// canned providers (the differential: the no-profile case skips the triggers;
/// the at-cadence case fires them and pins the summary fold + the fold-episode
/// pass).
#[allow(clippy::too_many_arguments)]
pub async fn message_resolve_external_turn<
    C: crate::model::completion::CompletionProvider + Sync,
    E: crate::model::embedding::EmbeddingProvider + Sync,
>(
    db: &Db,
    completion: &C,
    embedding: &E,
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
        embedding,
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
    // The SHARED body schema (v4 `SaveImageRequestSchema`, moved into
    // `save-image-to-album.ts` by `86d59660c`): this route's private uuid
    // schema — and its `fileId must be a UUID` / `mountPointId must be a UUID`
    // sentences — is DELETED, so a non-uuid id now passes the parse and fails
    // at the attachment guard below.
    let parsed = match parse_save_image_request(body) {
        Ok(p) => p,
        Err(message) => return bad_request(message),
    };
    let file_id = parsed.file_id;
    let mount_point_id = parsed.mount_point_id;
    let caption = parsed.caption;
    let tags = parsed.tags;

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

            // Who the save is attributed to — the one rule, shared with the
            // gallery's chat-scoped twin so both doors write the same byline
            // into the kept-image sidecar.
            let attribution =
                resolve_save_attribution(main, mount, &chat_owned, &mp_owned, &user_id_owned);

            let input = SaveImageToAlbumInput {
                mount_point_id: &mp_owned,
                file_id: &file_owned,
                caption: caption_owned.as_deref(),
                tags: &tags_owned,
                chat_id: Some(&chat_id_for_scene),
                attribution,
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
// === P4.D174 === The chat gallery (v4 `86d59660c`)
// ===========================================================================

/// v4 `GET /chats/[id]?action=gallery` → the BARE `{entries, counts, total}`
/// (v4 answers `NextResponse.json(gallery)`, not `successResponse`).
///
/// A missing chat is a 404 **before** the enumerator runs. An enumerator
/// failure is a 500 with v4's fixed sentence — NOT an empty roll: the
/// enumerator already fails soft at every pass, so anything that escapes it is
/// a real fault (pinned by v4's own `chat-gallery-actions.test.ts:136`).
pub fn chat_gallery(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    // v4's route reads the chat through `repos.chats.findById`, whose
    // `_findById` is `safeQuery(…, 'Error finding entity by ID', { id }, null)`
    // (`base.repository.ts:246`): a FAILED read logs that line and answers
    // `null`, and the route then answers `notFound('Chat')`. A 500 here was
    // v5's own invention (P4.88's escalation — the "a 404" half of P4.D174's
    // OPEN note), fixed at the round's unification.
    if crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        db.read_main(move |c| chats_read::find_by_id(c, &cid))
    })
    .is_none()
    {
        return not_found("Chat");
    }
    let cid = chat_id.to_string();
    let gallery = read_main_mount(db, |main, mount| {
        crate::photos::chat_gallery::get_chat_gallery(main, mount, &cid)
    });
    match gallery {
        Ok(gallery) => {
            let total = gallery.get("total").cloned().unwrap_or(Value::Null);
            let counts = gallery.get("counts").cloned().unwrap_or(Value::Null);
            tracing::debug!(
                chat_id = %chat_id,
                total = %total,
                counts = %counts,
                "[Chats v1] Gallery listed"
            );
            Response::ChatMedia(gallery)
        }
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Failed to list chat gallery");
            Response::error(ErrorKind::Internal, "Failed to list chat gallery")
        }
    }
}

/// v4 `POST /chats/[id]?action=save-image` (`actions/save-image.ts`) — the
/// chat-scoped twin of [`message_save_image`].
///
/// The message route asks *"is this image attached to this message"*. Half the
/// gallery has no message at all — a Lantern backdrop and an Aurora repaint post
/// no announcement when `alertCharactersOfLanternImages` is off (the default),
/// and a participant's standing portrait was never part of a turn — so this
/// route asks the question the gallery can answer: *"is this image in this
/// chat's gallery"*.
///
/// The refusal ladder, in v4's order: schema → 400 (the joined sentences); no
/// chat → 404; not in the gallery → 400 `Image is not in this chat`;
/// `ALREADY_SAVED` → **409** with `{error, code, relativePath, keptAt}` (the one
/// place the two doors differ — the message leg answers 400, deliberately);
/// any other save error → 400 with its message; anything else → 500.
#[allow(clippy::too_many_arguments)]
pub async fn chat_save_gallery_image(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    body: &Value,
    bytes: Arc<dyn FileBytesStore>,
    side_effects: Arc<dyn SaveImageSideEffects + Send + Sync>,
    kept_at: &str,
) -> Response {
    let parsed = match parse_save_image_request(body) {
        Ok(p) => p,
        Err(message) => return bad_request(message),
    };
    let file_id = parsed.file_id;
    let mount_point_id = parsed.mount_point_id;

    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        // v4 wraps the WHOLE handler in one `try` (`actions/save-image.ts:43,
        // 126-131`): anything that is not a `SaveImageToAlbumError` answers the
        // fixed `serverError('Failed to save image')`, never the DB's own text.
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[SaveGalleryImage] failed");
            return Response::error(ErrorKind::Internal, "Failed to save image");
        }
    };

    // The guard: the id must name a picture this conversation actually holds.
    // Without it the action would save any image in the instance into any album
    // on the strength of a chat id.
    let cid = chat_id.to_string();
    let entries = match read_main_mount(db, |main, mount| {
        crate::photos::chat_gallery::list_chat_gallery(main, mount, &cid)
    }) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[SaveGalleryImage] failed");
            return Response::error(ErrorKind::Internal, "Failed to save image");
        }
    };
    let Some(entry) = entries
        .iter()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(file_id.as_str()))
    else {
        tracing::info!(
            chat_id = %chat_id,
            file_id = %file_id,
            gallery_total = entries.len(),
            "[SaveGalleryImage] rejected: id is not in this chat gallery"
        );
        return bad_request("Image is not in this chat");
    };
    let id_kind = entry
        .get("idKind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source = entry
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let user_id_owned = user_id.to_string();
    let chat_owned = chat.clone();
    let mp_owned = mount_point_id.clone();
    let file_owned = file_id.clone();
    let caption_owned = parsed.caption.clone();
    let tags_owned = parsed.tags.clone();
    let chat_id_for_scene = chat_id.to_string();
    let kept_owned = kept_at.to_string();
    let source_for_log = source.clone();

    let result = db
        .write(move |ws| {
            let mount = super::mount_files::mount_conn(ws)?;
            let main = ws.main().connection();
            let attribution =
                resolve_save_attribution(main, mount, &chat_owned, &mp_owned, &user_id_owned);
            tracing::debug!(
                chat_id = %chat_id_for_scene,
                file_id = %file_owned,
                id_kind = %id_kind,
                source = %source_for_log,
                mount_point_id = %mp_owned,
                attribution_role = ?attribution.role,
                "[SaveGalleryImage] saving"
            );
            let input = SaveImageToAlbumInput {
                mount_point_id: &mp_owned,
                file_id: &file_owned,
                caption: caption_owned.as_deref(),
                tags: &tags_owned,
                chat_id: Some(&chat_id_for_scene),
                attribution,
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
        Ok(Ok(saved)) => {
            tracing::info!(
                chat_id = %chat_id,
                file_id = %file_id,
                source = %source,
                mount_point_id = %mount_point_id,
                relative_path = %saved.relative_path,
                link_id = %saved.link_id,
                "[SaveGalleryImage] saved"
            );
            Response::ChatMedia(json!({
                "saved": true,
                "mountPoint": saved.mount_point_name,
                "relativePath": saved.relative_path,
                "linkId": saved.link_id,
                "keptAt": saved.kept_at,
                "fileId": saved.file_id,
                "sha256": saved.sha256,
            }))
        }
        Ok(Err(err)) => {
            tracing::info!(
                chat_id = %chat_id,
                code = ?err.code,
                message = %err.message,
                "[SaveGalleryImage] rejected"
            );
            // Already in that album is not a failure of the request — it is the
            // answer to it, and the dialog says so in those words.
            if err.code == SaveImageErrorCode::AlreadySaved {
                // v4's 409 body is `{error, code, relativePath, keptAt}` — four
                // FLAT siblings. `code` rides the existing flat carrier (its
                // documented purpose: "the SPA error translation keys on this
                // first"); the two riders ride `already_saved`, which every
                // transport spreads beside `error` through
                // `CoreError::already_saved_wire_body` (§C.3 of the round
                // contract; the §3 unification review found them nested under
                // `details`, which no transport flattens).
                let mut resp = Response::error(ErrorKind::Conflict, err.message);
                if let Response::Error(e) = &mut resp {
                    e.code = Some("ALREADY_SAVED".to_string());
                    e.already_saved = Some(Box::new(crate::api::types::AlreadySavedRiders {
                        relative_path: err.existing_relative_path,
                        kept_at: err.existing_created_at,
                    }));
                }
                return resp;
            }
            bad_request(err.message)
        }
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[SaveGalleryImage] failed");
            Response::error(ErrorKind::Internal, "Failed to save image")
        }
    }
}

// ===========================================================================
// Group stores (v4 handleGetGroupStores — P4.9E3B tier-2 audit port)
// ===========================================================================

/// v4 `GET /chats/[id]?action=group-stores` → `{ stores }`: the enabled
/// database-backed stores (character vaults excluded) of every group a
/// user-CONTROLLED, non-removed character participant belongs to
/// (`actions/group-stores.ts:16`). The picker's "group stores" leg.
pub fn chat_group_stores(db: &Db, chat_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        let Some(chat) = chats_read::find_by_id(main, &chat_id_owned)? else {
            return Ok(None);
        };
        // Insertion-ordered dedupe (v4's Set iterates insertion order).
        let mut mount_ids: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
            for p in participants {
                if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                let Some(cid) = p
                    .get("characterId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                if p.get("controlledBy").and_then(Value::as_str) != Some("user") {
                    continue;
                }
                if p.get("status").and_then(Value::as_str) == Some("removed") {
                    continue;
                }
                for id in crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character(
                    main, mount, cid,
                ) {
                    if seen.insert(id.clone()) {
                        mount_ids.push(id);
                    }
                }
            }
        }

        let points = DocMountPointsRepository::new(mount);
        let mut stores: Vec<Value> = Vec::new();
        for id in &mount_ids {
            let Some(mp) = points.find_full_json_by_id(id)? else {
                continue;
            };
            if mp.get("enabled").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            if mp.get("mountType").and_then(Value::as_str) != Some("database") {
                continue;
            }
            if mp.get("storeType").and_then(Value::as_str) == Some("character") {
                continue;
            }
            stores.push(json!({
                "id": mp.get("id").cloned().unwrap_or(Value::Null),
                "name": mp.get("name").cloned().unwrap_or(Value::Null),
                "mountType": mp.get("mountType").cloned().unwrap_or(Value::Null),
                "storeType": mp
                    .get("storeType")
                    .and_then(Value::as_str)
                    .unwrap_or("documents"),
                "enabled": mp.get("enabled").cloned().unwrap_or(Value::Null),
            }));
        }
        Ok(Some(json!({ "stores": stores })))
    });
    match result {
        Ok(Some(body)) => Response::ChatDialog(body),
        Ok(None) => Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
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
/// attachments — the read-back half of [`chat_attach_mount_file`], since the
/// announcement message IS the attachment record and no link-table row exists.
/// (P4.6ab left this a bounded deferral; P4.9E4A closes it, because the attach
/// write is meaningless without the read that surfaces it.)
///
/// The `seenIds` set is what makes a double attach show up ONCE: v4 checks the
/// *attachment id* against the set and, on the modern path where the attachment
/// id IS the link id, adds that same id back — so the second announcement's
/// attachment is skipped. Note the asymmetry v4 carries: the legacy fallback
/// resolves an attachment id as a `fileId`, and then `seenIds` gains the LINK
/// id while the loop keeps testing attachment ids. Reproduced, not repaired.
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

    // ── The mount-file announcement walk (v4 `route.ts:460-497`) ──────────
    // Mount-file attachments are recorded only on Librarian announcement
    // messages (no link table). The walk that resolves them lives in
    // `photos::chat_gallery` alongside the chat gallery's own passes, so this
    // listing and the gallery can never disagree about which attachments a
    // message carries (v4 `86d59660c` lifted the same loop out of this route).
    {
        let seen: std::collections::HashSet<String> = files
            .iter()
            .filter_map(|f| f.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        // A message read that fails costs the mount-file attachments and
        // nothing else; the linked files are already in hand and are the better
        // half of the answer. This listing has always degraded rather than
        // 500'd here.
        let cid = chat_id.to_string();
        let events = match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    chat_id = %chat_id,
                    error = %e,
                    "[Chats v1 Files] Failed to read messages for attachment walk"
                );
                Vec::new()
            }
        };
        let mount_attachments =
            crate::photos::chat_gallery::resolve_message_attachment_entries_db(db, &events, &seen);
        for attachment in &mount_attachments {
            files.push(json!({
                "id": attachment.id,
                "filename": attachment.filename,
                "filepath": attachment.url,
                "mimeType": attachment.mime_type,
                "size": attachment.size,
                "url": attachment.url,
                "createdAt": attachment.created_at,
                "type": "mountFile",
            }));
        }
        tracing::debug!(
            chat_id = %chat_id,
            linked_files = seen.len(),
            mount_attachments = mount_attachments.len(),
            "[Chats v1 Files] Resolved mount-file attachments"
        );
    }

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
/// the DB effect is the metadata delete. NOTE (P4.44): unlike the library
/// `files::file_delete`, v4's chat-file DELETE + upload routes run NO
/// `cleanupThumbnails` (verified against `chat-files/[id]/route.ts` +
/// `chat-files-v2.ts`), so no thumbnail cleanup is wired here.
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

/// v4 `POST /api/v1/chats/[id]/files?action=link` (`handleLinkFile`) — link an
/// existing library file to the chat (JSON `{fileId}`). Verifies the chat + file
/// exist, appends the chatId to the file's `linkedTo` (idempotent), and echoes
/// the `{file}` shape. `addLink` returning "no write" means already-linked, NOT a
/// failure (v4's `addLink` still returns the file), so the echo reads the row.
pub async fn chat_file_link(db: &Db, _user_id: &str, chat_id: &str, file_id: &str) -> Response {
    use crate::db::files::FilesRepository;

    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    }
    // [P4.62(c) / P4.72] v4's guard is `!fileId || typeof fileId !== 'string'`
    // (`chats/[id]/files/route.ts` `handleLinkFile`), and it sits AFTER the
    // chat lookup — so a missing chat 404s before this 400 can happen. The
    // caller collapses absent / null / wrong-typed / empty to `""`, exactly as
    // v4's `!v || typeof v !== 'string'` does, and the refusal itself lives
    // here so the REST edge and the `/api/dispatch` entrance cannot answer
    // different sentences (the P4.60 one-home rule).
    //
    // It replaces a post-hoc rewrite at the web edge, which had to guess: it
    // turned EVERY non-`Chat not found` outcome into this 400 whenever the
    // fileId was invalid, so a genuine failure of the CHAT lookup — where v4
    // answers 500, never having reached its own file lookup — was reported as
    // the 400 too. No committed arm forces that failure, so the repair is
    // unmeasured; it is a consequence of putting the guard where v4 puts it.
    if file_id.is_empty() {
        return bad_request("fileId is required");
    }
    let fid = file_id.to_string();
    let file = match db.read_main(move |c| FilesRepository::new(c).find_full_by_id(&fid)) {
        Ok(Some(f)) => f,
        Ok(None) => return not_found("File"),
        Err(e) => return internal(e),
    };
    let fid = file_id.to_string();
    let cid = chat_id.to_string();
    if let Err(e) = db
        .write(move |ws| FilesRepository::new(ws.main().connection()).add_link(&fid, &cid))
        .await
    {
        return internal(e);
    }
    let filepath = format!("/api/v1/files/{}", file.id);
    Response::ChatMedia(json!({
        "file": {
            "id": file.id,
            "filename": file.original_filename,
            "filepath": filepath,
            "mimeType": file.mime_type,
            "size": file.size,
            "url": filepath,
        }
    }))
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

/// The seams the upload's fire-and-forget `autoDescribeChatImageAttachment`
/// runs over (P4.120, dogfood #128) — the byte store, the vision-describe
/// driver (`None` → v4's `describe-failed` arm, the unwired-host posture) and
/// the post-write side effects (mount invalidation + embedding enqueue).
pub struct UploadAutoDescribe {
    pub bytes: Arc<dyn FileBytesStore>,
    pub describe: Option<Arc<dyn ImageDescribeDriver>>,
    pub side_effects: Arc<dyn SaveImageSideEffects + Send + Sync>,
}

/// [`chat_file_upload_with_auto_describe`] with no auto-describe seams — the
/// upload alone, as the differential families drive it. A new image row skips
/// the background describe with a DEBUG line.
pub async fn chat_file_upload(
    db: &Db,
    codec: std::sync::Arc<dyn crate::services::file_storage::PixelCodec>,
    user_id: &str,
    chat_id: &str,
    input: ChatFileUploadInput,
) -> Response {
    chat_file_upload_with_auto_describe(db, codec, user_id, chat_id, input, None).await
}

/// v4 `autoDescribeChatImageAttachment` as `uploadFileToProject` fires it
/// (`chat-files-v2.ts:425-437`): `void call({ fileEntryId, userId, repos })
/// .catch(warn)`. **No `chatId`** — v4's own call passes none, so the uncensored
/// vision fallback sees `chat = null` and only the global on-duty switch decides
/// (a Locked chat's upload can still reach the uncensored desk; that is v4's
/// gap, ported faithfully and filed as a candidate v4 note, not fixed).
///
/// Spawned onto the host's armed background spawner ([`crate::background`]),
/// AFTER the upload's write committed; never awaited, so the response is never
/// affected. Nothing armed (harness, CLI direct mode) → a DEBUG line, no call.
fn fire_upload_auto_describe(db: &Db, seams: Option<UploadAutoDescribe>, file_id: &str) {
    let Some(seams) = seams else {
        tracing::debug!(
            fileId = %file_id,
            "chat upload: no auto-describe seams wired; skipping the background describe"
        );
        return;
    };
    let db = db.clone();
    let id = file_id.to_string();
    let spawned = crate::background::spawn_background(Box::pin(async move {
        let result = crate::photos::auto_describe_attachment::auto_describe_chat_image_attachment(
            &db,
            &*seams.bytes,
            &*seams.side_effects,
            seams.describe.as_deref(),
            &id,
            None,
        )
        .await;
        if let Err(error) = result {
            tracing::warn!(
                module = "chat-files-v2",
                fileId = %id,
                error = %error,
                "Auto-describe failed for chat image upload"
            );
        }
    }));
    if !spawned {
        tracing::debug!(
            fileId = %file_id,
            "chat upload: no background spawner armed; skipping the background describe"
        );
    }
}

/// v4 `POST /api/v1/chats/[id]/files` (default multipart leg) → `uploadChatFile`
/// (`chat-files-v2.ts`, ported in [`crate::services::chat_files`]). Reads the
/// chat's projectId, decodes the base64 bytes, runs the upload (project
/// dup-detect + resolutions / non-project sha-dedup), and shapes the `{file}` |
/// `{duplicate, …}` body. A >10 MB overflow → v4's message-sniffed 400.
///
/// P4.120: a NEW image row also fires v4's background auto-describe over
/// `auto_describe` (see [`fire_upload_auto_describe`]) once the write has
/// committed and before the response is shaped.
pub async fn chat_file_upload_with_auto_describe(
    db: &Db,
    codec: std::sync::Arc<dyn crate::services::file_storage::PixelCodec>,
    user_id: &str,
    chat_id: &str,
    input: ChatFileUploadInput,
    auto_describe: Option<UploadAutoDescribe>,
) -> Response {
    use crate::services::chat_files::{upload_chat_file, ChatUploadError, ChatUploadOutcome};

    // v4: verify the chat belongs to the user; read its projectId.
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Decode the transport base64 (the web edge encodes the multipart file with
    // padded STANDARD base64).
    let data = {
        use base64::Engine;
        match base64::engine::general_purpose::STANDARD.decode(input.data.as_bytes()) {
            Ok(b) => b,
            Err(_) => return bad_request("Invalid base64 file data"),
        }
    };

    let outcome = upload_chat_file(
        db,
        codec,
        user_id,
        chat_id,
        project_id,
        input.filename,
        input.content_type,
        data,
        input.resolution,
        input.conflicting_file_id,
    )
    .await;

    match outcome {
        Ok(ChatUploadOutcome::Uploaded(f)) => {
            if f.auto_describe {
                fire_upload_auto_describe(db, auto_describe, &f.id);
            }
            Response::ChatMedia(json!({
                "file": {
                    "id": f.id,
                    "filename": f.filename,
                    "filepath": f.filepath,
                    "mimeType": f.mime_type,
                    "size": f.size,
                    "url": f.filepath,
                }
            }))
        }
        Ok(ChatUploadOutcome::Duplicate(d)) => Response::ChatMedia(json!({
            "duplicate": true,
            "conflictType": d.conflict_type,
            "existingFile": {
                "id": d.existing_id,
                "filename": d.existing_filename,
                "size": d.existing_size,
                "createdAt": d.existing_created_at,
                "sha256": d.existing_sha256,
            },
            "newFile": {
                "filename": d.new_filename,
                "size": d.new_size,
                "sha256": d.new_sha256,
            },
        })),
        Err(ChatUploadError::SizeExceeded) => {
            bad_request("File size exceeds maximum allowed size of 10 MB")
        }
        Err(ChatUploadError::Db(e)) => internal(e),
    }
}

// ===========================================================================
// P4.6ak: chatGetBackground — the story-background resolver
// ===========================================================================

/// The all-null body v4 returns when the chat has no `storyBackgroundImageId`
/// or its file row is missing (get.ts:182-184 / :187-193).
fn background_all_null() -> Value {
    json!({
        "backgroundUrl": Value::Null,
        "fileId": Value::Null,
        "filename": Value::Null,
        "sha256": Value::Null,
        "linkSummary": Value::Null,
    })
}

/// v4 `GET /api/v1/chats/[id]?action=get-background` (get.ts:173-211): resolve
/// the chat's story-background image to a serveable API URL + its photo-link
/// summary. Arms: chat missing → 404 `notFound('Chat')`; no
/// `storyBackgroundImageId` (falsy) → all-null; the file row missing → all-null
/// (v4 also warn-logs); else the resolved body `{backgroundUrl, fileId, filename,
/// sha256, linkSummary}`. `linkSummary` is `null` when the file has no sha256
/// (v4 `file.sha256 ? … : null`), else the mount-index reverse index (empty
/// `{count:0, linkers:[]}` for a legacy `files` image never written to a mount).
pub fn chat_get_background(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let body = read_main_mount(db, |main, mount| {
        let Some(chat) = chats_read::find_by_id(main, &cid)? else {
            return Ok(None); // v4 notFound('Chat')
        };
        // v4 `if (!chat.storyBackgroundImageId)` — falsy (absent/null/empty).
        let bg_id = chat
            .get("storyBackgroundImageId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let Some(bg_id) = bg_id else {
            return Ok(Some(background_all_null()));
        };
        let Some(file) = FilesRepository::new(main).find_by_id(bg_id)? else {
            return Ok(Some(background_all_null()));
        };
        let link_summary = if file.sha256.is_empty() {
            Value::Null
        } else {
            crate::photos::photo_link_summary::get_photo_link_summary_by_sha256(
                mount,
                &file.sha256,
            )?
        };
        Ok(Some(json!({
            "backgroundUrl": format!("/api/v1/files/{}", file.id),
            "fileId": file.id,
            "filename": file.original_filename,
            "sha256": file.sha256,
            "linkSummary": link_summary,
        })))
    });
    match body {
        Ok(Some(v)) => Response::ChatBackground(v),
        Ok(None) => not_found("Chat"),
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/chats/[id]?action=cost` (`handlers/get.ts:213-231`): load the
/// chat (missing → `notFound('Chat')`), then hand off to the ported read service
/// ([`crate::services::cost_estimation`]) — the detailed arm when the route's
/// `detailed` query param was the exact string `"true"`, else the aggregate arm.
///
/// The service itself never throws (both arms swallow into the zeros object), so
/// the route's own catch → `serverError('Failed to get cost breakdown')` is
/// unreachable here except through a `Db` failure, which surfaces as the same
/// [`internal`] shape the other reads use.
///
/// **The response body is RAW.** v4 answers `NextResponse.json(breakdown)` — NOT
/// the `successResponse` envelope nearly every other action arm uses. The REST
/// edge (`quilltap-web`) carries that faithfully.
pub fn chat_get_cost(db: &Db, chat_id: &str, detailed: bool) -> Response {
    let cid = chat_id.to_string();
    let body = db.read_main(move |main| {
        // v4's route checks the chat FIRST and 404s; the service would otherwise
        // answer the zeros object for a missing chat.
        if chats_read::find_by_id(main, &cid)?.is_none() {
            return Ok(None);
        }
        Ok(Some(if detailed {
            crate::services::cost_estimation::get_detailed_chat_cost_breakdown(main, &cid)
        } else {
            crate::services::cost_estimation::get_chat_cost_breakdown(main, &cid)
        }))
    });
    match body {
        Ok(Some(v)) => Response::ChatCost(v),
        Ok(None) => not_found("Chat"),
        Err(e) => internal(e),
    }
}

/// v4 `?action=regenerate-background` — `handleRegenerateBackground`
/// (`chats/[id]/actions/story-background.ts:19-86`). **Edge only:** every piece
/// of machinery below it was already ported, so this validates, resolves, and
/// enqueues — it does NOT generate. The generation JOB
/// ([`crate::services::story_background_job`], W4.9c) is registered live in the
/// host and picks the row up from the queue.
///
///   1. chat settings by USER → `!storyBackgroundsSettings?.enabled` → badRequest.
///   2. [`resolve_image_profile_for_chat`] → falsy → badRequest.
///   3. `chat.participants.filter(p => isParticipantPresent(p.status) && p.characterId)`
///      ([70505745a]: present = active/silent; the TRUTHY `characterId` conjunct
///      still drops an empty string) → empty → badRequest.
///   4. [`enqueue_story_background_generation`] — chat-level dedupe: any
///      PENDING/PROCESSING story-background job for this chat is REUSED
///      (`is_new = false`, same jobId), which is the only difference between the
///      two success messages.
///
/// v4's catch → `serverError('Failed to queue story background regeneration')`;
/// a `Db` failure lands there rather than on [`internal`], because that is the
/// string v4 answers.
pub async fn chat_regenerate_background(db: &Db, user_id: &str, chat_id: &str) -> Response {
    regenerate_background(db, user_id, chat_id, false).await
}

/// v4 `handleRegenerateBackground(chatId, chat, ctx, { forceUncensored })`
/// (`ce2f1dabf`, #77 widened it with the options bag): the plain action passes
/// `false`; "Try uncensored"'s background arm passes `true`, which spreads
/// `forceUncensored: true` LAST onto the queue payload. The two `[Chats v1]`
/// INFO lines (v4's, never emitted by v5 before — restored with the force
/// keys: `forceUncensored` on the queued line, `forceUncensoredRequested` on
/// the dedupe line, both present on a plain regenerate too).
async fn regenerate_background(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    force_uncensored: bool,
) -> Response {
    let cid = chat_id.to_string();
    let uid = user_id.to_string();

    // The route's own pre-step (`handlers/post.ts`: "Verify ownership first") —
    // a missing chat 404s before the action handler ever runs.
    let loaded = read_main_mount(db, {
        let cid = cid.clone();
        let uid = uid.clone();
        move |main, mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            let settings = crate::db::chat_settings::find_by_user_id(main, &uid)?;
            // Resolved under the same read so the profile tiers see one snapshot.
            let enabled = settings
                .as_ref()
                .and_then(|cs| cs.get("storyBackgroundsSettings"))
                .and_then(|s| s.get("enabled"))
                .and_then(Value::as_bool)
                == Some(true);
            let profile = if enabled {
                crate::services::image_profile_resolution::resolve_image_profile_for_chat(
                    main,
                    Some(mount),
                    &uid,
                    &chat,
                    settings.as_ref(),
                )
            } else {
                None
            };
            Ok(Some((chat, enabled, profile)))
        }
    });

    let (chat, enabled, image_profile_id) = match loaded {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Chat"),
        Err(_) => return server_error_regenerate(),
    };

    if !enabled {
        return bad_request(
            "Story backgrounds are not enabled. Enable them in Settings > Chat Settings > Story Backgrounds.",
        );
    }
    let Some(image_profile_id) = image_profile_id else {
        return bad_request(
            "No image profile available for story background generation. Configure an image profile in Chat Settings.",
        );
    };

    // `chat.participants.filter(p => isParticipantPresent(p.status) && p.characterId)
    //  .map(p => p.characterId!)`.
    //
    // [70505745a] Only participants actually in the scene. Absent and
    // (soft-)removed participants must never be painted into the background;
    // 'silent' counts as present — they are standing there, just not speaking.
    // The truthy `p.characterId` conjunct is unchanged (an empty-string
    // characterId still drops out).
    let character_ids: Vec<String> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|p| crate::chat_predicates::json_participant_is_present(p))
                .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if character_ids.is_empty() {
        return bad_request("No characters present in chat to generate background for.");
    }

    // `sceneContext: chat.title`, `projectId: chat.projectId ?? null`.
    let scene_context = chat.get("title").and_then(Value::as_str);
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);

    match crate::services::queue_service::enqueue_story_background_generation_with(
        db,
        &uid,
        &cid,
        &image_profile_id,
        &character_ids,
        scene_context,
        project_id,
        force_uncensored,
    )
    .await
    {
        Ok((job_id, is_new)) => {
            if is_new {
                tracing::info!(
                    target: "quilltap::chats",
                    chat_id = %chat_id,
                    job_id = %job_id,
                    image_profile_id = %image_profile_id,
                    character_count = character_ids.len(),
                    force_uncensored,
                    "[Chats v1] Queued story background regeneration"
                );
            } else {
                tracing::info!(
                    target: "quilltap::chats",
                    chat_id = %chat_id,
                    job_id = %job_id,
                    image_profile_id = %image_profile_id,
                    force_uncensored_requested = force_uncensored,
                    "[Chats v1] Story background generation already in progress"
                );
            }
            Response::ChatMedia(json!({
                "message": if is_new {
                    "Story background regeneration queued"
                } else {
                    "Story background generation already in progress"
                },
                "queued": true,
                "jobId": job_id,
            }))
        }
        Err(_) => server_error_regenerate(),
    }
}

/// v4's catch arm for the regenerate action.
fn server_error_regenerate() -> Response {
    Response::error(
        ErrorKind::Internal,
        "Failed to queue story background regeneration",
    )
}

/// The target the `[DangerousContent]` route lines of "Try uncensored" log
/// under (both verbs share it; the SERVICE's lines carry
/// `quilltap::concierge_retry_uncensored` and no prefix).
pub(crate) const RETRY_TARGET: &str = "quilltap::concierge_retry";

use crate::services::dangerous_content::provider_routing::ApiKeyResolver;
use crate::services::dangerous_content::retry_uncensored::{
    compose_retry_route_trail, resolve_image_retry_understudy, AnsweredBy, AnsweringProfile,
    RetryProfileKind,
};
// ===========================================================================
// P4.D228 — "Try uncensored" on a picture (v4 `retry-image-uncensored`,
// `app/api/v1/chats/[id]/actions/retry-image-uncensored.ts`, NEW at
// `ce2f1dabf`, #77)
// ===========================================================================

/// v4's union-refusal sentence (`badRequest(...)` on a failed `safeParse`).
pub const RETRY_IMAGE_BODY_REFUSAL: &str = "Expected { toolMessageId } or { kind: \"background\" }";

/// What a `retry-image-uncensored` body asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetryImageTarget {
    /// `{ toolMessageId }` — redraw a `generate_image` picture.
    Picture(String),
    /// `{ kind: 'background' }` — queue the Lantern's backdrop on the desk.
    Background,
}

/// v4's `retryImageSchema` — `z.union([z.object({ toolMessageId:
/// z.string().min(1) }), z.object({ kind: z.literal('background') })])` —
/// decoded with Zod's union semantics: the FIRST branch that parses wins, and
/// an object branch STRIPS unknown keys. So `{toolMessageId: 'x', kind:
/// 'background'}` is a PICTURE retry and `{toolMessageId: '', kind:
/// 'background'}` a BACKGROUND one. A decode that looked at `kind` first would
/// invert the former. `None` is v4's 400 ([`RETRY_IMAGE_BODY_REFUSAL`]).
pub fn decode_retry_image_body(body: &Value) -> Option<RetryImageTarget> {
    let obj = body.as_object()?;
    if let Some(id) = obj.get("toolMessageId").and_then(Value::as_str) {
        // `.min(1)` counts UTF-16 units; only the empty string fails it.
        if !id.is_empty() {
            return Some(RetryImageTarget::Picture(id.to_string()));
        }
    }
    (obj.get("kind").and_then(Value::as_str) == Some("background"))
        .then_some(RetryImageTarget::Background)
}

/// v4 `parseToolContent` — the stored TOOL content as an object, or `None`
/// (`JSON.parse` threw, or it parsed to a non-object / null).
fn parse_tool_content(content: Option<&str>) -> Option<Map<String, Value>> {
    let parsed: Value = serde_json::from_str(content?).ok()?;
    match parsed {
        Value::Object(o) => Some(o),
        // v4 `parsed && typeof parsed === 'object'` — an array IS an object
        // to JS; it then fails `toolName !== 'generate_image'` all the same.
        Value::Array(_) => Some(Map::new()),
        _ => None,
    }
}

fn conflict(reason: &str) -> Response {
    Response::error(ErrorKind::Conflict, reason)
}

/// v4 `handleRetryImageUncensored` — `POST /api/v1/chats/[id]?action=
/// retry-image-uncensored`, served as the dispatch verb
/// `chatRetryImageUncensored` (the REST arm answers the "ride POST
/// /api/dispatch" pointer, as its siblings do).
///
/// The order IS the contract: the chat (v4's `handlePost` 404s first) → the
/// union decode → the chat settings (read BEFORE branching) → the arm.
///
/// - **background** — the RESOLVED image profile excluded; `locked` /
///   `no-understudy` → 409 BEFORE any of `regenerate-background`'s own 400s;
///   the INFO fires even when the regenerate then 400s.
/// - **picture** — the TOOL row by id (404), `generate_image` with arguments
///   (400), the chat's RAW `imageProfileId` + the row's image-kind trail + the
///   content's provider+model excluded (409); the generator run
///   SYNCHRONOUSLY on the understudy with `primary_via: 'concierge'`; no image
///   → WARN + 502 (v5: [`ErrorKind::Internal`] carrying v4's `details`, see
///   the family's recorded divergence) and nothing saved; else the new TOOL
///   row filed at the original's `createdAt + 1 ms` beside it, the
///   `refusal-rerouted` announcement ONLY when the original trail has a
///   `refused` row (a soft refusal names nobody), and 200 `{ toolMessageId,
///   images, routeTrail }`.
///
/// Neither arm ever writes the chat's Concierge state.
pub async fn chat_retry_image_uncensored<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    image_generation: Option<&crate::tools::generate_image::ErasedImageGeneration>,
    user_id: &str,
    chat_id: &str,
    body: &Value,
) -> Response {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => chat,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let Some(target) = decode_retry_image_body(body) else {
        return bad_request(RETRY_IMAGE_BODY_REFUSAL);
    };
    let uid = user_id.to_string();
    let chat_settings =
        match db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid)) {
            Ok(s) => s,
            Err(e) => return internal(e),
        };

    match target {
        RetryImageTarget::Background => {
            retry_background_uncensored(
                db,
                api_keys,
                user_id,
                chat_id,
                &chat,
                chat_settings.as_ref(),
            )
            .await
        }
        RetryImageTarget::Picture(tool_message_id) => {
            retry_picture_uncensored(
                db,
                api_keys,
                image_generation,
                user_id,
                chat_id,
                &chat,
                chat_settings.as_ref(),
                &tool_message_id,
            )
            .await
        }
    }
}

/// The background arm (v4 `retry-image-uncensored.ts:71-88`).
async fn retry_background_uncensored<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    user_id: &str,
    chat_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
) -> Response {
    // The RESOLVED profile (`resolveImageProfileForChat`), where the picture
    // arm excludes the chat's raw column.
    let image_profile_id = {
        let (uid, chat, settings) = (user_id.to_string(), chat.clone(), chat_settings.cloned());
        match read_main_mount(db, move |main, mount| {
            Ok(
                crate::services::image_profile_resolution::resolve_image_profile_for_chat(
                    main,
                    Some(mount),
                    &uid,
                    &chat,
                    settings.as_ref(),
                ),
            )
        }) {
            Ok(id) => id,
            Err(e) => return internal(e),
        }
    };
    let gate = resolve_image_retry_understudy(
        db,
        api_keys,
        user_id,
        chat,
        chat_settings,
        &[image_profile_id.as_deref()],
        None,
        AnsweredBy::default(),
    )
    .await;
    match gate {
        Err(reason) => {
            tracing::info!(
                target: RETRY_TARGET,
                chat_id = %chat_id,
                reason = reason.as_str(),
                "[DangerousContent] Uncensored background retry refused"
            );
            conflict(reason.as_str())
        }
        Ok(understudy) => {
            tracing::info!(
                target: RETRY_TARGET,
                chat_id = %chat_id,
                understudy_profile_id = %understudy.profile.id,
                "[DangerousContent] Queueing a story background on the uncensored desk"
            );
            regenerate_background(db, user_id, chat_id, true).await
        }
    }
}

/// The picture arm (v4 `retry-image-uncensored.ts:90-226`).
#[allow(clippy::too_many_arguments)]
async fn retry_picture_uncensored<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    image_generation: Option<&crate::tools::generate_image::ErasedImageGeneration>,
    user_id: &str,
    chat_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
    tool_message_id: &str,
) -> Response {
    use crate::services::route_trail::{RouteAttempt, RouteAttemptOutcome};
    use crate::services::tool_execution::{GeneratedImage, ToolMessage, ToolMetadata};

    // v4 `repos.chats.getMessages` — the FALLBACK read (a `safeQuery` that
    // answers `[]` on a failed query), so a read error reads as "not found".
    let cid = chat_id.to_string();
    let messages = db
        .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid))
        .unwrap_or_default();
    let Some(tool_message) = messages.into_iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("id").and_then(Value::as_str) == Some(tool_message_id)
    }) else {
        return not_found("Tool message");
    };
    if tool_message.get("role").and_then(Value::as_str) != Some("TOOL") {
        return not_found("Tool message");
    }
    let stored = parse_tool_content(tool_message.get("content").and_then(Value::as_str));
    let arguments = stored
        .as_ref()
        .and_then(|s| s.get("arguments"))
        .filter(|a| crate::api::system_qtap::js_truthy(Some(a)))
        .cloned();
    let (Some(stored), Some(arguments)) = (
        stored.filter(|s| s.get("toolName").and_then(Value::as_str) == Some("generate_image")),
        arguments,
    ) else {
        return bad_request("Only generate_image pictures can be retried uncensored");
    };

    let trail = tool_message.get("routeTrail").filter(|t| !t.is_null());
    let gate = resolve_image_retry_understudy(
        db,
        api_keys,
        user_id,
        chat,
        chat_settings,
        &[chat.get("imageProfileId").and_then(Value::as_str)],
        trail,
        AnsweredBy {
            provider: stored.get("provider").and_then(Value::as_str),
            model_name: stored.get("model").and_then(Value::as_str),
        },
    )
    .await;
    let understudy = match gate {
        Ok(u) => u,
        Err(reason) => {
            tracing::info!(
                target: RETRY_TARGET,
                chat_id = %chat_id,
                tool_message_id = %tool_message_id,
                reason = reason.as_str(),
                "[DangerousContent] Uncensored picture retry refused"
            );
            return conflict(reason.as_str());
        }
    };
    tracing::info!(
        target: RETRY_TARGET,
        chat_id = %chat_id,
        tool_message_id = %tool_message_id,
        understudy_profile_id = %understudy.profile.id,
        understudy_name = %understudy.profile.name,
        "[DangerousContent] Retrying a picture on the uncensored desk"
    );

    let Some(runner) = image_generation else {
        return Response::error(
            ErrorKind::Internal,
            "image generation not assembled (image-generation seam deferral)",
        );
    };
    let participant_id = tool_message
        .get("participantId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let result = runner
        .run(
            db,
            &crate::tools::generate_image::ImageGenerationToolInput::from_arguments(&arguments),
            &crate::tools::generate_image::ImageToolExecutionContext {
                user_id: user_id.to_string(),
                profile_id: understudy.profile.id.clone(),
                chat_id: Some(chat_id.to_string()),
                calling_participant_id: participant_id.clone(),
                primary_via: Some(crate::services::route_trail::RouteAttemptVia::Concierge),
            },
        )
        .await;

    if !result.success || result.images.is_empty() {
        tracing::warn!(
            target: RETRY_TARGET,
            chat_id = %chat_id,
            tool_message_id = %tool_message_id,
            error = result.error.as_deref(),
            message = result.message.as_deref(),
            "[DangerousContent] Uncensored picture retry did not produce an image"
        );
        // v4 `errorResponse(msg, 502, { code: result.error })`: `details` is
        // always present — `{}` when the code is undefined (JSON drops the
        // key, `details !== undefined` keeps the object).
        let mut details = Map::new();
        if let Some(code) = &result.error {
            details.insert("code".into(), json!(code));
        }
        let message = result
            .message
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "The uncensored desk could not produce the picture".to_string());
        let mut response = Response::error(ErrorKind::Internal, message);
        if let Response::Error(e) = &mut response {
            e.details = Some(Box::new(Value::Object(details)));
        }
        return response;
    }

    // The original's refusals, then whatever the understudy's own call sheet
    // says (non-empty only when it, too, was refused and rerouted on). The
    // BODY carries the understudy's rows in the chokepoint's in-memory key
    // order (v4 returns them un-reparsed); the saved row re-parses through the
    // schema on both sides.
    let prior: Vec<Value> = trail
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|a| a.get("outcome").and_then(Value::as_str) != Some("answered"))
        .cloned()
        .collect();
    let body_trail: Vec<Value> = if !result.route_trail.is_empty() {
        prior
            .into_iter()
            .chain(
                result
                    .route_trail
                    .iter()
                    .map(RouteAttempt::to_chokepoint_value),
            )
            .collect()
    } else {
        compose_retry_route_trail(
            trail,
            AnsweringProfile {
                id: &understudy.profile.id,
                name: &understudy.profile.name,
                provider: &understudy.profile.provider,
                model_name: &understudy.profile.model_name,
            },
            RetryProfileKind::Image,
        )
    };
    // P4.130 (P4.D228's NIT, RE-PREMISED): the NIT read "the saved trail drops
    // a prior row that fails the typed parse while the 200 body returns it".
    // It cannot happen on v4: `getMessages` `safeParse`s every event, and a
    // `routeTrail` element failing `RouteAttemptSchema` skips the WHOLE TOOL
    // message, so the route answers 404 `Tool message not found` before any
    // trail code runs. v5 now validates at the same READ
    // (`chats_messages_read::zod_shape_failure`, the strict twin
    // `zod_route_attempt_failure`), so every prior row reaching here already
    // passed it and every other row is one the chokepoint just built. The
    // filter stays as a defensive no-op, held to the SAME strict twin ahead of
    // the lenient typed reader (`RouteAttempt::from_value`, which reads `null`
    // as absent and checks neither the uuid nor the length).
    let saved_trail: Vec<RouteAttempt> = body_trail
        .iter()
        .filter(|row| crate::api::zod_issues::zod_route_attempt_failure(row).is_none())
        .filter_map(RouteAttempt::from_value)
        .collect();

    let generated: Vec<GeneratedImage> = result
        .images
        .iter()
        .map(|img| GeneratedImage {
            id: img.id.clone(),
            filename: img.filename.clone(),
            filepath: img.filepath.clone().unwrap_or_else(|| img.url.clone()),
            mime_type: img
                .mime_type
                .clone()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "image/png".to_string()),
            // `img.size || 0` — NaN and 0 both fall through.
            size: img.size.filter(|s| *s != 0.0 && !s.is_nan()).unwrap_or(0.0),
            width: img.width,
            height: img.height,
            sha256: img.sha256.clone(),
        })
        .collect();
    let images_json: Vec<Value> = generated
        .iter()
        .map(|g| {
            let mut o = Map::new();
            o.insert("id".into(), json!(g.id));
            o.insert("filename".into(), json!(g.filename));
            o.insert("filepath".into(), json!(g.filepath));
            o.insert("mimeType".into(), json!(g.mime_type));
            o.insert("size".into(), crate::db::js_number_to_json(g.size));
            if let Some(w) = g.width {
                o.insert("width".into(), crate::db::js_number_to_json(w));
            }
            if let Some(h) = g.height {
                o.insert("height".into(), crate::db::js_number_to_json(h));
            }
            if let Some(s) = &g.sha256 {
                o.insert("sha256".into(), json!(s));
            }
            Value::Object(o)
        })
        .collect();
    let retry_tool_message = ToolMessage {
        tool_name: "generate_image".to_string(),
        success: true,
        content: format!("Generated {} image(s)", generated.len()),
        arguments: Some(arguments),
        call_id: None,
        anchor_offset: None,
        seq: None,
        metadata: Some(ToolMetadata {
            provider: result.provider.clone(),
            model: result.model.clone(),
            expanded_prompt: result.expanded_prompt.clone(),
            route_trail: saved_trail,
        }),
    };
    let character_id = participant_id.as_deref().and_then(|pid| {
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
            })
            .and_then(|p| p.get("characterId").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    // Filed a millisecond after the original, so the new picture sits beside
    // the one it answers rather than at the foot of the transcript. v4's
    // `new Date(new Date(createdAt).getTime() + 1).toISOString()` — a stored
    // stamp without milliseconds reformats to `.001Z`; an unparseable one is
    // v4's `RangeError: Invalid time value`.
    let Some(original_ms) = tool_message
        .get("createdAt")
        .and_then(Value::as_str)
        .and_then(crate::clock::iso_to_ms)
    else {
        return internal("Invalid time value");
    };
    let created_at = crate::clock::iso_from_unix_ms(original_ms + 1);
    let image_count = generated.len();
    let (cid, uid) = (chat_id.to_string(), user_id.to_string());
    let saved = db
        .write(move |w| {
            crate::services::tool_execution::save_tool_messages_with_options(
                w.main(),
                &cid,
                &uid,
                &[retry_tool_message],
                &generated,
                character_id.as_deref(),
                participant_id.as_deref(),
                None,
                crate::services::tool_execution::SaveToolMessagesOptions {
                    created_at: Some(created_at),
                },
            )
        })
        .await;
    let saved = match saved {
        Ok(s) => s,
        Err(e) => return internal(e),
    };

    // The Concierge says so when the original was a refusal he could not get
    // past. A soft refusal (a sanitized picture that "succeeded") has nobody
    // to name as refusing, so it passes without an announcement.
    let refused = trail
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|a| {
                a.get("outcome").and_then(Value::as_str)
                    == Some(RouteAttemptOutcome::Refused.as_str())
            })
        })
        .cloned();
    if let Some(refused) = &refused {
        crate::services::concierge_notifications::post_concierge_refusal_announcement(
            db,
            chat_id,
            crate::services::concierge_notifications::ConciergeRefusalKind::RefusalRerouted,
            &crate::services::concierge_notifications::ConciergeRefusalDetails {
                refusing_provider: refused
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                refusing_model: refused
                    .get("modelName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                answering_profile_name: Some(understudy.profile.name.clone()),
                purpose: crate::services::concierge_notifications::ConciergeRefusalPurpose::Tool,
                reason: None,
            },
        )
        .await;
    }

    tracing::info!(
        target: RETRY_TARGET,
        chat_id = %chat_id,
        original_tool_message_id = %tool_message_id,
        tool_message_id = saved.first_tool_message_id.as_deref(),
        image_count,
        announced = refused.is_some(),
        "[DangerousContent] Uncensored picture retry posted"
    );

    Response::ChatMedia(json!({
        "toolMessageId": saved.first_tool_message_id,
        "images": images_json,
        "routeTrail": body_trail,
    }))
}

// ===========================================================================
// P4.9E4A — attach-mount-file (v4 `handleAttachMountFile`,
// `app/api/v1/chats/[id]/files/route.ts:250-346`)
// ===========================================================================

/// The host seam for one vision describe — v4 `generateImageDescription(file,
/// repos, userId)` (`lib/chat/file-attachment-fallback.ts:483`). Only the
/// composing host holds the completion provider and the image transcoder the
/// describe rides, so the dispatch layer reaches it
/// erased (the `ConsultRunner` / `announcement_preview` / `RegenerateTitleDriver`
/// precedent). `None` on the assembly → the ladder resolves to `''` with a warn
/// and the attach still succeeds, which is v4's own posture for **every**
/// describe failure on this path.
///
/// ⚠ LIVE means real money: one vision-LLM call per attach of an undescribed
/// image (the cached / kept-image / non-image arms never reach the driver).
pub trait ImageDescribeDriver: Send + Sync {
    /// `chat_id` is v4's `ImageDescriptionOptions.chatId` (`3b463d6b1`, #76):
    /// the chat the image is being described for, whose Concierge state
    /// decides the uncensored vision fallback.
    fn describe<'a>(
        &'a self,
        file: crate::services::file_fallback::FallbackFile,
        chat_id: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = crate::services::file_fallback::FallbackResult> + Send + 'a>>;
}

/// v4 `ensureImageDescription` (`files/route.ts:188-248`) — resolve whatever
/// description ends up associated with the blob: cached, freshly generated, or
/// `''` on any failure. Every failure arm is warn-and-continue, exactly as v4.
///
/// **A v4 quirk carried, not fixed:** [`generate_image_description`]'s
/// persisted-text tier looks the file up in `files` by `file.id`, but this path
/// passes a `doc_mount_blobs` id (v4 `files/route.ts:216`) — a disjoint id
/// space, so that tier is an effective miss here and the ladder always falls
/// through to vision. Matched deliberately.
async fn ensure_image_description(
    db: &Db,
    describe: Option<&Arc<dyn ImageDescribeDriver>>,
    // v4 `3b463d6b1`: `ensureImageDescription(repos, userId, chatId, blob)`.
    chat_id: &str,
    blob: &crate::db::doc_mount_blobs::BlobWithLink,
) -> String {
    use crate::db::doc_mount_blobs::DocMountBlobsRepository;

    if !blob.stored_mime_type.to_lowercase().starts_with("image/") {
        return String::new();
    }
    let existing = crate::jsstr::js_trim(&blob.description);
    if !existing.is_empty() {
        return existing.to_string();
    }

    let blob_id = blob.id.clone();
    let bytes =
        match db.read_mount_index(move |c| DocMountBlobsRepository::new(c).read_data(&blob_id)) {
            Ok(Some(b)) => b,
            // v4: `readData` throw → warn + ''; a null buffer → '' as well.
            Ok(None) => return String::new(),
            Err(e) => {
                tracing::warn!(
                    blob_id = %blob.id,
                    error = %e,
                    "[Chats v1 Files] Failed to read blob bytes for description"
                );
                return String::new();
            }
        };

    let Some(describe) = describe else {
        tracing::warn!(
            blob_id = %blob.id,
            "[Chats v1 Files] No ImageDescribeDriver assembled — attaching without a \
             vision description (v4's own any-failure arm)"
        );
        return String::new();
    };

    let data = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    };
    let result = describe
        .describe(
            crate::services::file_fallback::FallbackFile {
                id: blob.id.clone(),
                // v4 `files/route.ts:220` passes `blob.originalFileName` raw — a
                // nullable column (the import writes NULL when the bundle has none),
                // so `undefined` reaches the describer there; the nearest String
                // here is empty. (The `f45a517a9` round's unification: the joined
                // blob row read this column as a bare `String` and a NULL took the
                // whole attach down with `Invalid column type Null`.)
                filename: blob.original_file_name.clone().unwrap_or_default(),
                mime_type: blob.stored_mime_type.clone(),
                data: Some(data),
            },
            Some(chat_id),
        )
        .await;

    // v4 checks the UNTRIMMED description for emptiness, then trims.
    let raw = match (&result.type_, result.image_description.as_deref()) {
        (crate::services::file_fallback::FallbackType::ImageDescription, Some(d))
            if !d.is_empty() =>
        {
            d
        }
        _ => {
            tracing::warn!(
                blob_id = %blob.id,
                error = ?result.error,
                "[Chats v1 Files] Image description generation did not return a description"
            );
            return String::new();
        }
    };
    let description = crate::jsstr::js_trim(raw).to_string();

    // Cache it on the ATTACHED link (v4 `chats/[id]/files/route.ts:238`,
    // `0c14fd61f`). Bug 157's hazard was exactly here: the old no-linkId form
    // resolved `WHERE fileId = ? LIMIT 1`, so for a blob carried at several
    // locations the caption landed on whichever link SQLite handed back — not
    // necessarily the one being attached. The read at the top of this function
    // takes `blob.description` off the joined ATTACHED link, so writing
    // anywhere else meant the next attach found it still blank and re-ran the
    // vision model, every time. `blob.link_id` is the same row, read and
    // written.
    let blob_id = blob.id.clone();
    let link_id = blob.link_id.clone();
    let cached = description.clone();
    if let Err(e) = db
        .write(move |ws| {
            DocMountBlobsRepository::new(super::mount_files::mount_conn(ws)?)
                .update_description(&blob_id, &cached, &link_id)
                .map(|_| ())
        })
        .await
    {
        tracing::warn!(
            blob_id = %blob.id,
            error = %e,
            "[Chats v1 Files] Failed to persist generated description"
        );
    }
    description
}

/// v4 `POST /api/v1/chats/[id]/files?action=attach-mount-file`
/// (`handleAttachMountFile`) — attach a document-store file to the chat by
/// posting the Librarian's attachment announcement.
///
/// **The announcement message IS the attachment record** (`writer.ts:152-160`):
/// the `doc_mount_file_links` row id rides as the synthetic message's single
/// `attachments` entry; no link-table row is written. The only durable writes on
/// the path are that announcement and (when vision runs) the blob-description
/// cache. There is deliberately **no dedupe** — attaching the same
/// `(mountPointId, relativePath)` twice posts two announcements carrying the
/// same `mountFileId`; the dedupe lives on the read side
/// ([`chat_files_list`]'s `seenIds`).
pub async fn chat_attach_mount_file(
    db: &Db,
    describe: Option<&Arc<dyn ImageDescribeDriver>>,
    chat_id: &str,
    mount_point_id: &str,
    relative_path: &str,
) -> Response {
    use crate::db::doc_mount_blobs::DocMountBlobsRepository;
    use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
    use crate::services::librarian_notifications::{
        post_librarian_attach_announcement, LibrarianAttachAnnouncement,
    };

    // v4's outer POST resolves the chat BEFORE the action dispatch
    // (`route.ts:34-38`), so `Chat not found` precedes the body validation.
    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    }

    // `!mountPointId || typeof mountPointId !== 'string'` — the web edge coerces
    // a missing / non-string field to `""`, so an empty value covers both arms.
    if mount_point_id.is_empty() {
        return bad_request("mountPointId is required");
    }
    if relative_path.is_empty() {
        return bad_request("relativePath is required");
    }

    let (mp, rp) = (mount_point_id.to_string(), relative_path.to_string());
    let mount_file = match db.read_mount_index(move |c| {
        DocMountFileLinksRepository::new(c).find_by_mount_point_and_path(&mp, &rp)
    }) {
        Ok(Some(f)) => f,
        Ok(None) => return not_found("Mount-point file"),
        Err(e) => return internal(e),
    };

    let (mp, rp) = (mount_point_id.to_string(), relative_path.to_string());
    let blob = match db.read_mount_index(move |c| {
        DocMountBlobsRepository::new(c).find_by_mount_point_and_path(&mp, &rp)
    }) {
        Ok(Some(b)) => b,
        Ok(None) => {
            // Native-text fall-through (v4 bug 38, `7bcd8515`): a .md/.txt/.json
            // PUT into a database store becomes a document (`doc_mount_documents`,
            // no blob row). The picker lists them, so a blob-only attach path
            // 404'd on exactly those. Serve the document to the Librarian instead
            // — its text is what the LLM needs, and `load_mount_file_as_attachment`
            // resolves the same document back to bytes.
            if let Some(text_mime) =
                crate::services::mount_index::path_utils::native_text_attachment_mime(relative_path)
            {
                let fid = mount_file.file_id.clone();
                let has_document = db
                    .read_mount_index(move |c| {
                        crate::db::doc_mount_documents::DocMountDocumentsRepository::new(c)
                            .find_content_by_file_id(&fid)
                    })
                    .ok()
                    .flatten()
                    .is_some();
                if has_document {
                    return attach_mount_document(
                        db,
                        chat_id,
                        mount_point_id,
                        relative_path,
                        &mount_file,
                        text_mime,
                    )
                    .await;
                }
            }
            tracing::warn!(
                chat_id = %chat_id,
                mount_point_id = %mount_point_id,
                relative_path = %relative_path,
                "[Chats v1 Files] Mount file has no blob or document row, refusing to attach"
            );
            return not_found("Mount-point file blob");
        }
        Err(e) => return internal(e),
    };

    // Tolerant: v4 `mountPoint?.name ?? null` — a missing mount point is not an error.
    let mp = mount_point_id.to_string();
    let mount_point_name = db
        .read_mount_index(move |c| DocMountPointsRepository::new(c).find_id_and_name_by_id(&mp))
        .ok()
        .flatten()
        .map(|(_, name)| name);

    // The three-source description ladder. For kept images (anything under a
    // `photos/` folder) the link's extractedText already carries the original
    // generation prompt, scene snapshot, and saver caption — richer, by
    // construction, than vision could produce, and free.
    let mut description = String::new();
    let mut description_source = "empty";
    if crate::photos::photos_paths::is_photos_relative_path(Some(relative_path)) {
        if let Some(from_markdown) =
            crate::photos::keep_image_markdown::build_attach_description_from_kept_image(
                mount_file.extracted_text.as_deref(),
            )
        {
            description = from_markdown;
            description_source = "kept-image-markdown";
        }
    }
    if description.is_empty() {
        let had_cached = !crate::jsstr::js_trim(&blob.description).is_empty();
        description = ensure_image_description(db, describe, chat_id, &blob).await;
        if !description.is_empty() {
            description_source = if had_cached {
                "vision-llm-cached"
            } else {
                "vision-llm-generated"
            };
        }
    }

    // v4 `files/route.ts:383`: `blob.originalFileName || mountFile.fileName` —
    // JS truthiness, so both NULL and `''` fall to the file name.
    let display_title = match blob.original_file_name.as_deref() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => mount_file.file_name.clone(),
    };
    let announcement = post_librarian_attach_announcement(
        db,
        &LibrarianAttachAnnouncement {
            chat_id: chat_id.to_string(),
            display_title: display_title.clone(),
            file_path: relative_path.to_string(),
            mount_point: mount_point_name,
            mount_file_id: mount_file.id.clone(),
            mime_type: blob.stored_mime_type.clone(),
            description: Some(description.clone()),
        },
    )
    .await;

    let Some(announcement) = announcement else {
        return internal("Failed to post Librarian attachment announcement");
    };

    let url = format!(
        "/api/v1/mount-points/{}/blobs/{}",
        mount_point_id,
        crate::tools::photo::encode_uri(relative_path)
    );
    tracing::info!(
        chat_id = %chat_id,
        mount_file_id = %mount_file.id,
        mount_point_id = %mount_point_id,
        relative_path = %relative_path,
        description_included = !description.is_empty(),
        description_source = %description_source,
        "[Chats v1 Files] Mount-point file attached via Librarian"
    );

    Response::ChatMedia(json!({
        "file": {
            "id": mount_file.id,
            "filename": display_title,
            "filepath": url,
            "mimeType": blob.stored_mime_type,
            "size": blob.size_bytes,
            "url": url,
            "type": "mountFile",
        },
        "announcement": {
            "id": announcement.get("id").cloned().unwrap_or(Value::Null),
            "createdAt": announcement.get("createdAt").cloned().unwrap_or(Value::Null),
        },
    }))
}

/// v4 `handleAttachMountDocument` (`chats/[id]/files/route.ts`, bug 38): attach a
/// native-text document (a `.md`/`.txt`/`.json` in a database store, held in
/// `doc_mount_documents` with no blob). Posts the SAME Librarian announcement as
/// the blob path — the catalogue entry, not the bytes, is what rides into chat
/// history — carrying the link id so `load_mount_file_as_attachment` resolves it
/// back to text. The description is empty (no image to describe) and the URL is
/// the `/files/` document route (not `/blobs/`).
async fn attach_mount_document(
    db: &Db,
    chat_id: &str,
    mount_point_id: &str,
    relative_path: &str,
    mount_file: &crate::db::doc_mount_file_links::LinkRow,
    mime_type: &str,
) -> Response {
    use crate::db::doc_mount_points::DocMountPointsRepository;
    use crate::services::librarian_notifications::{
        post_librarian_attach_announcement, LibrarianAttachAnnouncement,
    };

    let mp = mount_point_id.to_string();
    let mount_point_name = db
        .read_mount_index(move |c| DocMountPointsRepository::new(c).find_id_and_name_by_id(&mp))
        .ok()
        .flatten()
        .map(|(_, name)| name);
    // v4: `mountFile.originalFileName || mountFile.fileName` — JS `||`, so an
    // empty originalFileName also falls back (same idiom as the sibling sites).
    let display_title = mount_file
        .original_file_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&mount_file.file_name)
        .to_string();

    let announcement = post_librarian_attach_announcement(
        db,
        &LibrarianAttachAnnouncement {
            chat_id: chat_id.to_string(),
            display_title: display_title.clone(),
            file_path: relative_path.to_string(),
            mount_point: mount_point_name,
            mount_file_id: mount_file.id.clone(),
            mime_type: mime_type.to_string(),
            description: Some(String::new()),
        },
    )
    .await;

    let Some(announcement) = announcement else {
        return internal("Failed to post Librarian attachment announcement");
    };

    let url = format!(
        "/api/v1/mount-points/{}/files/{}",
        mount_point_id,
        crate::tools::photo::encode_uri(relative_path)
    );
    tracing::info!(
        chat_id = %chat_id,
        mount_file_id = %mount_file.id,
        mount_point_id = %mount_point_id,
        relative_path = %relative_path,
        mime_type = %mime_type,
        "[Chats v1 Files] Mount-point document attached via Librarian"
    );

    Response::ChatMedia(json!({
        "file": {
            "id": mount_file.id,
            "filename": display_title,
            "filepath": url,
            "mimeType": mime_type,
            "size": mount_file.file_size_bytes,
            "url": url,
            "type": "mountFile",
        },
        "announcement": {
            "id": announcement.get("id").cloned().unwrap_or(Value::Null),
            "createdAt": announcement.get("createdAt").cloned().unwrap_or(Value::Null),
        },
    }))
}

#[cfg(test)]
mod gallery_route_degrade_tests {
    use super::*;
    use crate::db::runtime::DbPaths;

    /// Throwaway pepper for a fresh encrypted instance (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provisioned instance with a `chats` column RENAMED, so the route's
    /// own chat read fails at prepare on BOTH sides (v4's `_findById` names its
    /// columns too; a DROPPED table would not do — v4's `getCollection()`
    /// recreates it and answers `null` without logging). v4's `safeQuery` shape:
    /// a failed `findById` is a `null`, not a thrown error.
    fn instance_without_chats() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        {
            let w = crate::db::Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
            w.connection()
                .execute_batch("ALTER TABLE \"chats\" RENAME COLUMN \"title\" TO \"title_x\"")
                .unwrap();
        }
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    /// P4.88's escalation, pinned: a failed chat read answers v4's 404 with
    /// v4's `Error finding entity by ID` line, never a 500.
    #[test]
    fn a_failed_chat_read_is_v4s_404_not_a_500() {
        let (_dir, db) = instance_without_chats();
        let lines = crate::test_support::captured(|| {
            let got = chat_gallery(&db, "c0000000-0000-4000-8000-000000000001");
            assert_eq!(got, not_found("Chat"), "{got:?}");
        });
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with(
                    "ERROR quilltap::db Error finding entity by ID collection=chats id=c0000000-0000-4000-8000-000000000001 "
                )),
            "v4's safeQuery line must be logged: {lines:#?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Failed to list chat gallery")),
            "the old 500 sentence must be gone: {lines:#?}"
        );
    }
}
