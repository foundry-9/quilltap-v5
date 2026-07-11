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

use axum::extract::{Path, Request, State};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::db::files::FilesRepository;
use quilltap_core::db::runtime::Db;
use quilltap_core::photos::character_gallery_service::{
    save_link_to_character_gallery, save_to_character_gallery, GalleryError,
};
use quilltap_core::services::file_storage::download_file;
use quilltap_host::LocalStorageBackend;
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
        let file_id = body
            .get("fileId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let link_id = body
            .get("linkId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if file_id.is_some() == link_id.is_some() {
            // Neither or both — v4's refine.
            return bad_request("Provide exactly one of fileId or linkId");
        }
        let caption = json_caption(&body);
        let tags = json_tags(&body);

        if let Some(link_id) = link_id {
            save_via_link(&db, &id, link_id, caption, tags).await
        } else {
            save_via_file_id(&db, &backend, &id, file_id.unwrap(), caption, tags).await
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
                    quilltap_core::db::DbError::Key(
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

/// v4 `caption: z.string().nullable().optional()` → `caption ?? null`. A JSON
/// `null` or absent → `None`; a string → itself.
fn json_caption(body: &Value) -> Option<String> {
    body.get("caption")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// v4 `tags: z.array(z.string()).optional()`.
fn json_tags(body: &Value) -> Vec<String> {
    body.get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
