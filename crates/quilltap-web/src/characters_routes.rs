//! The characters multipart / binary REST routes (P4.6m) — v4-shaped HTTP routes
//! that quilltap-web's `/api/dispatch` action channel can't carry (raw bytes in,
//! raw bytes out). Thin edge code: parse the request, build the per-request disk
//! backend, call ONE ported core service; no business logic here.
//!
//! - `POST /api/v1/characters/{id}/photos` — multipart upload OR JSON
//!   `{fileId|linkId}` (v4 `characters/[id]/photos/route.ts` POST).
//! - `GET  /api/v1/characters/{id}?action=export&format=png` — the ST-card PNG
//!   (v4 `characters/[id]/handlers/get.ts`, the `format=png` leg).
//! - `POST /api/v1/characters?action=import` — a `.png` / `.json` ST card
//!   (v4 `characters/handlers/post.ts` `handleImport`).
//! - `POST /api/v1/characters?action=reset-builtins` — delete + re-seed the
//!   built-in pair (v4 `handlers/post.ts` `handleResetBuiltins`). Lives here,
//!   not on `/api/dispatch`: the avatar re-seed needs the host pixel codec,
//!   which (like every codec-needing leg) is wired at the web edge.

use std::collections::HashMap;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{characters, ErrorKind, Response};
use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::files::FilesRepository;
use quilltap_core::db::runtime::Db;
use quilltap_core::photos::character_gallery_service::{
    save_link_to_character_gallery, save_to_character_gallery, GalleryError,
};
use quilltap_core::services::file_storage::download_file;
use quilltap_core::services::image_job_storage::write_main_avatar_to_vault;
use quilltap_core::services::sillytavern::{
    create_st_character_png, export_st_character, parse_st_character_png,
};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::Value;

use crate::files_routes::{db_and_backend, error_json, not_found};
use crate::multipart::FormData;
use crate::state::SharedState;

/// The v4 400-keyword list (`photos/route.ts` catch): a thrown message containing
/// any of these is a client error.
const BAD_REQUEST_KEYWORDS: [&str; 7] = [
    "already in",
    "no linked database-backed vault",
    "Unsupported MIME type",
    "Uploaded image is empty",
    "not an image",
    "not found",
    "empty bytes",
];

/// A save failure: a structured [`GalleryError`] from a core write, or a raw
/// message from the route's own fileId glue (files lookup / download).
enum PhotoErr {
    Gallery(GalleryError),
    Message(String),
}
impl From<GalleryError> for PhotoErr {
    fn from(e: GalleryError) -> Self {
        PhotoErr::Gallery(e)
    }
}

/// v4's `photos/route.ts` catch → response routing. A raw message is routed by
/// keyword (`Character not found` → 404; the 400-keyword list → 400; else 500),
/// exactly as v4's `message.startsWith(...) / message.includes(...)` chain.
fn photo_error_response(err: PhotoErr) -> AxumResponse {
    let msg = match err {
        PhotoErr::Gallery(GalleryError::CharacterNotFound) => return not_found("Character"),
        PhotoErr::Gallery(GalleryError::BadRequest(m)) => m,
        PhotoErr::Gallery(GalleryError::Db(e)) => {
            return error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
        }
        PhotoErr::Message(m) => m,
    };
    if msg.starts_with("Character not found") {
        return not_found("Character");
    }
    if BAD_REQUEST_KEYWORDS.iter().any(|k| msg.contains(k)) {
        return error_json(StatusCode::BAD_REQUEST, &msg);
    }
    error_json(StatusCode::INTERNAL_SERVER_ERROR, &msg)
}

/// `201 created(result)` — the raw save output as the JSON body.
fn created(body: Value) -> AxumResponse {
    (
        StatusCode::CREATED,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn bad_request(message: &str) -> AxumResponse {
    error_json(StatusCode::BAD_REQUEST, message)
}

/// v4 `POST /api/v1/characters/[id]/photos` — content-type dispatch: JSON →
/// `{fileId|linkId}` save-by-id; else multipart upload. All three legs land bytes
/// in the character's vault `photos/` folder via the ported gallery service.
pub async fn characters_photos_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    req: Request,
) -> AxumResponse {
    let (db, backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    if id.is_empty() {
        return bad_request("Missing character id");
    }

    let content_type = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let result = if content_type.contains("application/json") {
        // v4 `req.json()` → `saveByIdSchema` (exactly one of fileId/linkId).
        let bytes = match axum::body::to_bytes(req.into_body(), 32 * 1024 * 1024).await {
            Ok(b) => b,
            Err(_) => return bad_request("Invalid request body"),
        };
        let body: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return bad_request("Invalid JSON body"),
        };
        // v4 `saveByIdSchema.safeParse` → `badRequest(issues.join('; '))`. P4.60:
        // reading the four keys here with `and_then(Value::as_str)`/`as_array`
        // silently DROPPED a wrong-typed `caption`/`tags` and saved the photo
        // 201 where v4 refuses.
        let parsed = match quilltap_core::api::characters::parse_photo_save_by_id_body(&body) {
            Ok(p) => p,
            Err(message) => return bad_request(&message),
        };
        let caption = parsed.caption;
        let tags = parsed.tags;

        if let Some(link_id) = parsed.link_id {
            save_via_link(&db, &id, &link_id, caption, tags).await
        } else {
            // The refine guarantees exactly one of the two is named.
            let file_id = parsed.file_id.expect("the refine names one of the two");
            save_via_file_id(&db, &backend, &id, &file_id, caption, tags).await
        }
    } else {
        // v4 `req.formData().catch(() => null)`.
        let form = match FormData::from_request(req, &state).await {
            Ok(f) => f,
            Err(_) => {
                return bad_request("Request body must be multipart/form-data or JSON with fileId")
            }
        };
        let Some(file) = form.file("file") else {
            return bad_request("Missing \"file\" field in upload");
        };
        // v4: `fileField.name || 'upload'`, `fileField.type || 'application/octet-stream'`.
        let filename = file
            .filename
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("upload")
            .to_string();
        let mime = file
            .content_type
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("application/octet-stream")
            .to_string();
        // v4: `(formData.get('caption') as string | null) ?? null` — a present
        // field (even empty) is kept; absent → None.
        let caption = form.text("caption");
        let tags = form.all_text("tags");
        let data = file.bytes.clone();
        save_bytes(&db, &id, data, filename, mime, caption, tags).await
    };

    match result {
        Ok(body) => created(body),
        Err(e) => photo_error_response(e),
    }
}

/// v4 `saveLinkToCharacterGallery` leg (bytes from the source link's mount-blob).
async fn save_via_link(
    db: &Db,
    character_id: &str,
    link_id: &str,
    caption: Option<String>,
    tags: Vec<String>,
) -> Result<Value, PhotoErr> {
    let kept_at = quilltap_core::clock::now_iso();
    let cid = character_id.to_string();
    let lid = link_id.to_string();
    write_gallery(db, move |main, mount| {
        save_link_to_character_gallery(main, mount, &cid, &lid, caption.as_deref(), &tags, &kept_at)
    })
    .await
}

/// The multipart-upload leg (`saveToCharacterGallery` with buffered bytes).
async fn save_bytes(
    db: &Db,
    character_id: &str,
    data: Vec<u8>,
    filename: String,
    mime: String,
    caption: Option<String>,
    tags: Vec<String>,
) -> Result<Value, PhotoErr> {
    let kept_at = quilltap_core::clock::now_iso();
    let cid = character_id.to_string();
    write_gallery(db, move |main, mount| {
        save_to_character_gallery(
            main,
            mount,
            &cid,
            &data,
            &filename,
            &mime,
            caption.as_deref(),
            &tags,
            &kept_at,
        )
    })
    .await
}

/// v4 `saveFileToCharacterGallery` — resolve a legacy images-v2 `files` row,
/// guard it is an image, fetch the bytes via the two-mode `downloadFile`
/// (mount-blob: → the DB blob; else the disk backend), then delegate to
/// `saveToCharacterGallery`.
async fn save_via_file_id(
    db: &Db,
    backend: &LocalStorageBackend,
    character_id: &str,
    file_id: &str,
    caption: Option<String>,
    tags: Vec<String>,
) -> Result<Value, PhotoErr> {
    let fid = file_id.to_string();
    let entry = db
        .read_main(move |conn| FilesRepository::new(conn).find_by_id(&fid))
        .map_err(|e| PhotoErr::Message(e.to_string()))?;
    let Some(entry) = entry else {
        return Err(PhotoErr::Message(format!("Image not found: {file_id}")));
    };
    // v4: `category !== 'IMAGE' && !mimeType.startsWith('image/')`.
    if entry.category != "IMAGE" && !entry.mime_type.starts_with("image/") {
        return Err(PhotoErr::Message(format!("File {file_id} is not an image")));
    }
    let data = download_file(db, backend, &entry).map_err(PhotoErr::Message)?;
    if data.is_empty() {
        return Err(PhotoErr::Message(format!(
            "Image {file_id} has empty bytes"
        )));
    }
    let filename = entry.original_filename.clone();
    let mime = entry.mime_type.clone();
    save_bytes(db, character_id, data, filename, mime, caption, tags).await
}

/// Run a gallery write on the writer thread (both `main` + `mount` connections),
/// folding the `GalleryError` result out of the `DbError` transport.
async fn write_gallery<F>(db: &Db, f: F) -> Result<Value, PhotoErr>
where
    F: FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<Value, GalleryError>
        + Send
        + 'static,
{
    let inner = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| {
                    quilltap_core::db::DbError::Internal(
                        "the photo gallery requires the mount-index database".to_string(),
                    )
                })?
                .connection();
            let main = writers.main().connection();
            Ok(f(main, mount))
        })
        .await
        .map_err(|e| PhotoErr::Gallery(GalleryError::Db(e)))?;
    inner.map_err(PhotoErr::Gallery)
}

// ===========================================================================
// POST /api/v1/characters/{id}?action=archive|rehydrate  (the CLI's JSON leg)
// ===========================================================================

/// v4 `POST /api/v1/characters/[id]` (`handlers/post.ts`). v5's SPA drives the
/// character JSON actions over `/api/dispatch`, but v4's CLI — ported as
/// `quilltap db characters archive|rehydrate` (P4.D66) — POSTs this URL with a
/// bare `fetch`, so the two verbs the CLI uses get a REST edge delegating into
/// the same P4.D65 dispatch arms. Success is v4's raw result bag
/// (`NextResponse.json(result)`); errors keep the arms' status + `{error}`
/// body (v4's `badRequest`/`serverError` envelope). Every other action stays
/// on `/api/dispatch` (the `characters_get` precedent below).
pub async fn characters_action_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    use quilltap_core::api::Request as CoreRequest;

    let req = match query.get("action").map(String::as_str) {
        Some("archive") => CoreRequest::CharacterArchive { character_id: id },
        Some("rehydrate") => CoreRequest::CharacterRehydrate { character_id: id },
        _ => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "This route serves ?action=archive and ?action=rehydrate only; \
                 the other JSON actions live on /api/dispatch",
            )
        }
    };
    match crate::text_replacements_routes::dispatch_core(&state, req).await {
        Ok(Response::Character(v)) => (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/json")],
            v.to_string(),
        )
            .into_response(),
        Ok(Response::Error(e)) => crate::text_replacements_routes::error_to_http(e),
        Ok(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
        Err(resp) => resp,
    }
}

// ===========================================================================
// GET /api/v1/characters/{id}/wardrobe  (the group-tier read)
// ===========================================================================

/// v4 `GET /api/v1/characters/[id]/wardrobe` (`8600c83f`). `?scope=group`
/// serves the shared items in the `Wardrobe/` folder of every store belonging
/// to a group this character is a member of — the group tier of the wearable
/// pool, as a standalone read for the client-side merge; no scope serves the
/// character's own vault items, as v4 does.
///
/// The same read is on `/api/dispatch` as `characterWardrobeList` (which is how
/// the SPA reaches it). This edge exists so the documented REST contract is
/// actually served: a client following `docs/developer/API.md` must get v4's
/// envelope from v4's path, not a 404.
pub async fn characters_wardrobe_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    // v4 `b86bb1a5` puts the dressing-instructions read on the SAME collection
    // route behind `?action=instructions` (`withActionDispatch`). The POST half
    // has no REST edge here because v5 never registered `.post` on this path —
    // it rides `POST /api/dispatch` as `characterWardrobeInstructionsSet`
    // (recorded, the P4.D112 dispatch-only precedent).
    let req = if query.get("action").map(String::as_str) == Some("instructions") {
        quilltap_core::api::Request::CharacterWardrobeInstructionsGet { character_id: id }
    } else {
        quilltap_core::api::Request::CharacterWardrobeList {
            character_id: id,
            scope: query.get("scope").cloned(),
            include_archived: crate::wardrobe_routes::read_include_archived(&query),
        }
    };
    match crate::text_replacements_routes::dispatch_core(&state, req).await {
        Ok(Response::Character(v)) => (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/json")],
            v.to_string(),
        )
            .into_response(),
        Ok(Response::Error(e)) => crate::text_replacements_routes::error_to_http(e),
        Ok(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
        Err(resp) => resp,
    }
}

// ===========================================================================
// GET /api/v1/characters/{id}?action=export  (the PNG binary leg + JSON leg)
// ===========================================================================

/// v4 `GET /api/v1/characters/[id]?action=export` (`handlers/get.ts` export
/// action). `format=png` → the ST-card PNG (bytes); else → the ST-card JSON,
/// both as `attachment` downloads. This route exists for the byte-out PNG leg
/// the `/api/dispatch` channel can't carry; the JSON leg is also the SPA's
/// dispatch `character_export` path (kept here for v4-faithful REST parity).
/// Any other `action` at this path → 400 (the JSON reads live on `/api/dispatch`).
pub async fn characters_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let (db, backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    if query.get("action").map(String::as_str) != Some("export") {
        return error_json(
            StatusCode::BAD_REQUEST,
            "This route serves ?action=export only; JSON reads are on /api/dispatch",
        );
    }

    // Ownership (single-user): the overlaid character must exist.
    let character = match resolve_character(&db, &id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Character"),
        Err(e) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // A tombstone export would be a pruned shell, and the full bundle already
    // sits in the library as an ARCHIVE file (spec §4.1, v4 `d553f72a`). v4
    // checks this inside the export action, so BOTH format legs refuse — this
    // route's PNG leg included.
    if quilltap_core::api::characters::is_archived(&character) {
        return error_json(
            StatusCode::BAD_REQUEST,
            "This character is archived; rehydrate them to export, or use their archive bundle.",
        );
    }

    // v4: `format = searchParams.get('format') || 'json'`.
    let format = query
        .get("format")
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("json");
    let name = character.get("name").and_then(Value::as_str).unwrap_or("");

    if format == "png" {
        // Read the avatar bytes the ST card embeds (defaultImageId → vault link
        // or legacy file); missing/unreadable → the placeholder (v4 warns).
        let avatar = character
            .get("defaultImageId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .and_then(|aid| read_avatar_bytes(&db, &backend, aid));
        let png = create_st_character_png(&character, avatar.as_deref());
        return (
            StatusCode::OK,
            [
                ("content-type", "image/png".to_string()),
                (
                    "content-disposition",
                    format!("attachment; filename=\"{name}.png\""),
                ),
            ],
            png,
        )
            .into_response();
    }

    // v4 JSON leg: `JSON.stringify(exportSTCharacter(character), null, 2)`.
    let card = export_st_character(&character);
    let body = serde_json::to_string_pretty(&card).unwrap_or_default();
    (
        StatusCode::OK,
        [
            ("content-type", "application/json".to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{name}.json\""),
            ),
        ],
        body,
    )
        .into_response()
}

/// Read the overlaid character (main + mount) by id — the api layer's
/// `read_main_mount` nesting (two pooled read connections; reads never contend).
fn resolve_character(db: &Db, id: &str) -> Result<Option<Value>, quilltap_core::db::DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| quilltap_core::db::characters_read::find_by_id(main, mount, id))
    })
}

// ===========================================================================
// POST /api/v1/characters?action=import  (the multipart .png / .json ST card)
// ===========================================================================

/// v4 `POST /api/v1/characters?action=import` (`handlers/post.ts` `handleImport`).
/// Multipart `file` (a `.png` ST card or a `.json` ST card) → create the
/// character (the ported `character_import` spine) and, for a PNG, land its bytes
/// as the imported avatar in the new vault (`write_main_avatar_to_vault`,
/// codec-injected) and set `defaultImageId`. Avatar failure is NON-FATAL (v4:
/// the character is kept without a portrait). The JSON-body import (no avatar) is
/// the SPA's dispatch `character_import` path; this route is the multipart leg.
pub async fn characters_import_post(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
    req: Request,
) -> AxumResponse {
    let (db, _backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    match query.get("action").map(String::as_str) {
        Some("import") => {}
        Some("reset-builtins") => return characters_reset_builtins(&db).await,
        _ => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "This route serves ?action=import and ?action=reset-builtins only; \
                 character creation is on /api/dispatch",
            );
        }
    }

    let content_type = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !content_type.contains("multipart/form-data") {
        return error_json(
            StatusCode::BAD_REQUEST,
            "The import route takes multipart/form-data; JSON imports are on /api/dispatch",
        );
    }

    let form = match FormData::from_request(req, &state).await {
        Ok(f) => f,
        Err(_) => return bad_request("No file provided"),
    };
    let Some(file) = form.file("file") else {
        return bad_request("No file provided");
    };
    let name_lower = file.filename.as_deref().unwrap_or("").to_lowercase();
    let is_png = file.content_type.as_deref() == Some("image/png") || name_lower.ends_with(".png");
    let is_json =
        file.content_type.as_deref() == Some("application/json") || name_lower.ends_with(".json");

    let (character_data, png_bytes): (Value, Option<Vec<u8>>) = if is_png {
        match parse_st_character_png(&file.bytes) {
            Some(data) => (data, Some(file.bytes.clone())),
            None => return bad_request("Invalid SillyTavern PNG file"),
        }
    } else if is_json {
        // v4 `JSON.parse` — a throw propagates to the outer catch → 500.
        match serde_json::from_slice(&file.bytes) {
            Ok(v) => (v, None),
            Err(_) => {
                return error_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to import character",
                )
            }
        }
    } else {
        return bad_request("Unsupported file type. Please upload PNG or JSON");
    };

    // Create through the ported import spine (importSTCharacter + create).
    let uid = quilltap_core::api::SINGLE_USER_ID;
    let mut echo = match characters::character_import(&db, uid, character_data).await {
        Response::Character(v) => v,
        Response::Error(e) => return response_error(e),
        _ => {
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to import character",
            )
        }
    };

    // The PNG leg lands the portrait in the new vault (non-fatal per v4).
    if let Some(png) = png_bytes {
        let created_id = echo
            .get("character")
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = echo
            .get("character")
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("avatar")
            .to_string();
        let filename = format!("{name}.png");
        if let Some(link_id) = write_avatar(&db, &created_id, filename, png).await {
            set_default_image_id(&db, &created_id, &link_id).await;
            if let Some(c) = echo.get_mut("character").and_then(Value::as_object_mut) {
                c.insert("defaultImageId".into(), Value::String(link_id));
            }
        }
    }

    created(echo)
}

/// Write the imported PNG as the character's main vault avatar. Returns the new
/// link id, or `None` on any failure (v4 keeps the character without a portrait).
async fn write_avatar(
    db: &Db,
    character_id: &str,
    filename: String,
    png: Vec<u8>,
) -> Option<String> {
    let cid = character_id.to_string();
    let written = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| {
                    quilltap_core::db::DbError::Internal("no mount-index database".into())
                })?
                .connection();
            let main = writers.main().connection();
            Ok(write_main_avatar_to_vault(
                main,
                mount,
                &HostImageCodec,
                &cid,
                &filename,
                &png,
                "image/png",
                None,
            ))
        })
        .await
        .ok()?;
    written.ok().map(|a| a.link_id)
}

/// v4 `handleResetBuiltins` (`characters/handlers/post.ts:196`): cascade-delete
/// the built-in pair (no exclusive chats/images), re-import the embedded seed
/// with the seed→preserved id remap, re-seed the avatars, and echo v4's response
/// shape. The whole reset runs inside ONE `Db::write` closure (both connections),
/// matching the service's contract; the codec is the host image stack.
async fn characters_reset_builtins(db: &Db) -> AxumResponse {
    use quilltap_core::services::quilltap_import::reset::{reset_builtins, ResetError};

    let uid = quilltap_core::api::SINGLE_USER_ID;
    let outcome = db
        .write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| {
                    quilltap_core::db::DbError::Internal("no mount-index database".into())
                })?
                .connection();
            let main = writers.main().connection();
            Ok(reset_builtins(main, mount, &HostImageCodec, uid))
        })
        .await;

    let result = match outcome {
        Ok(Ok(r)) => r,
        // v4's per-cause error mapping: a failed built-in delete → 500 with the
        // named message; a missing/unparseable embedded seed → 400; anything
        // else → the generic catch-all 500.
        Ok(Err(ResetError::DeleteFailed { name })) => {
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to delete {name} during reset"),
            );
        }
        Ok(Err(ResetError::Seed(_))) => {
            return bad_request("Built-in character seed data is unavailable");
        }
        Ok(Err(_)) | Err(_) => {
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to reset built-in characters",
            );
        }
    };

    // v4's `QuilltapExportCounts` key set, in declaration order (`execute.ts:71`);
    // the port only ever counts characters/memories — the rest stay 0.
    let counts_json = |c: &quilltap_core::services::quilltap_import::ImportCounts| {
        serde_json::json!({
            "characters": c.characters,
            "chats": 0,
            "messages": 0,
            "roleplayTemplates": 0,
            "connectionProfiles": 0,
            "imageProfiles": 0,
            "embeddingProfiles": 0,
            "tags": 0,
            "memories": c.memories,
            "projects": 0,
            "groups": 0,
        })
    };
    let ids_record = |pairs: &[(String, Option<String>)]| {
        let mut m = serde_json::Map::new();
        for (name, id) in pairs {
            m.insert(
                name.clone(),
                id.clone().map(Value::String).unwrap_or(Value::Null),
            );
        }
        Value::Object(m)
    };

    let body = serde_json::json!({
        "success": true,
        "deletedCharacterIds": result.deleted_character_ids,
        "preservedIds": ids_record(&result.preserved_ids),
        "postResetIds": ids_record(&result.post_reset_ids),
        "remappedIdCount": result.remapped_id_count,
        "importResult": {
            "success": result.import.success,
            "imported": counts_json(&result.import.imported),
            "skipped": counts_json(&result.import.skipped),
            "warnings": result.import.warnings,
            "importedCharacterIds": result.import.imported_character_ids,
        },
    });
    (StatusCode::OK, axum::Json(body)).into_response()
}

/// Set the character's `defaultImageId` slim column (v4 `repos.characters.update`).
async fn set_default_image_id(db: &Db, character_id: &str, link_id: &str) {
    let cid = character_id.to_string();
    let lid = link_id.to_string();
    let _ = db
        .write(move |writers| {
            writers.main().connection().execute(
                "UPDATE characters SET defaultImageId = ?1 WHERE id = ?2",
                rusqlite::params![lid, cid],
            )?;
            Ok(())
        })
        .await;
}

/// Map a dispatch `Response::Error` to its HTTP response ({error: message};
/// the store-unavailable refusal answers v4's exact `{error, <entity>Id}`
/// 503 body instead — P4.23).
fn response_error(e: quilltap_core::api::types::CoreError) -> AxumResponse {
    let status = match e.kind {
        ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorKind::Locked => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if let Some(body) = e.unavailable_wire_body() {
        return (
            status,
            [("content-type", "application/json")],
            body.to_string(),
        )
            .into_response();
    }
    error_json(status, &e.message)
}

/// v4 `readCharacterAvatarBuffer` — the bytes a character-avatar id points at.
/// Path 1: a vault link → its blob bytes (pure DB). Path 2: a legacy `files`
/// row → the two-mode `download_file`. `None` when neither resolves (→ the
/// placeholder). Missing mount-index partition is treated as "no vault link".
fn read_avatar_bytes(db: &Db, backend: &LocalStorageBackend, id: &str) -> Option<Vec<u8>> {
    // Path 1: vault link.
    let idv = id.to_string();
    let link = db
        .read_mount_index(move |c| {
            DocMountFileLinksRepository::new(c).find_by_id_with_content(&idv)
        })
        .ok()
        .flatten();
    if let Some(link) = link {
        let fid = link.file_id.clone();
        return db
            .read_mount_index(move |c| DocMountBlobsRepository::new(c).read_data_by_file_id(&fid))
            .ok()
            .flatten();
    }
    // Path 2: legacy files row.
    let idv = id.to_string();
    let entry = db
        .read_main(move |c| FilesRepository::new(c).find_by_id(&idv))
        .ok()
        .flatten()?;
    download_file(db, backend, &entry).ok()
}
