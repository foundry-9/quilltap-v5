//! The user-photo-gallery dispatch surface (P4.9a) — the glue over
//! [`crate::photos::user_gallery_service`], which holds the differentialed port
//! of v4's `lib/photos/user-gallery-service.ts`.
//!
//! Ports the two v4 route files whole:
//!   - `app/api/v1/photos/route.ts` — `GET` (list) + `POST` (save)
//!   - `app/api/v1/photos/[id]/route.ts` — `GET` (entry) + `DELETE` (remove)
//!
//! ## Why the validation lives HERE and not in the service
//!
//! v4 splits the work the same way: the route Zod-parses, the service throws
//! plain `Error`s, and the ROUTE decides 400-vs-500 by substring-testing the
//! thrown message (`route.ts:92-99`, `[id]/route.ts:46-48`). The service port
//! therefore carries only [`UserGalleryError::Message`] and this module
//! replicates both the Zod messages and the substring chain — so a message v4
//! deliberately lets fall through to a 500 (the empty-bytes one) still does.
//!
//! ## The `Number(...)`-before-Zod quirk
//!
//! v4's list route reads `limit`/`offset` as
//! `url.searchParams.has(k) ? Number(url.searchParams.get(k)) : undefined`, so
//! JS coercion runs BEFORE `z.number().int().min(…)`: `?limit=abc` becomes
//! `NaN` and `?limit=1.5` stays fractional, and Zod rejects each with a
//! DIFFERENT message. The wire type is `f64` for exactly that reason, and
//! [`zod_int_in_range`] reproduces the split.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::photos::save_image_to_album::FileBytesStore;
use crate::photos::user_gallery_service::{
    get_user_gallery_entry, list_user_gallery, needs_query_embedding, remove_from_user_gallery,
    save_to_user_gallery, UserGalleryError,
};

use super::mount_files::mount_conn;
use super::types::{ErrorKind, Response};

/// v4's route-level failure lowering: the service's raw thrown message decides
/// the status by SUBSTRING (`route.ts:92-99` for save, `[id]/route.ts:46-48`
/// for delete). Anything unmatched is a 500 carrying the same message — which
/// is not an oversight: `Image {id} has empty bytes` is deliberately outside
/// both lists, because empty stored bytes are a server fault, not a user one.
fn lower(err: UserGalleryError, save_arm: bool, fallback: &str) -> Response {
    let message = match err {
        UserGalleryError::Message(m) => m,
        // A DB fault was never a v4 `throw new Error(userMessage)`; it lands in
        // the route's OUTER catch, which logs and answers its generic text.
        UserGalleryError::Db(_) => return Response::error(ErrorKind::Internal, fallback),
    };
    let user_correctable = if save_arm {
        // v4 `route.ts:92-99`.
        message.contains("already saved")
            || message.contains("not an image")
            || message.contains("not owned")
            || message.contains("Image not found")
            || message.contains("Quilltap Uploads mount has not been provisioned")
    } else {
        // v4 `[id]/route.ts:46-48`.
        message.contains("not in the user gallery") || message.contains("not a gallery entry")
    };
    if user_correctable {
        Response::error(ErrorKind::BadRequest, message)
    } else {
        Response::error(ErrorKind::Internal, message)
    }
}

/// v4's `z.number().int().min(lo).max(hi).optional()` applied to a value that
/// already went through `Number(...)`. Returns the Zod first-issue message on
/// rejection — Zod reports one issue per field and the route joins the issue
/// list, so with a single bad field the joined string IS this message.
///
/// The bound messages say "number", NOT the field name: Zod v4 renders the
/// ORIGIN (the schema type) there, so `?limit=500` and `?offset=-1` both read
/// "expected number to be …". Pinned by `list_limit_over_max` /
/// `list_offset_negative`.
fn zod_int_in_range(raw: f64, lo: i64, hi: Option<i64>) -> Result<i64, String> {
    if raw.is_nan() {
        // `Number('abc')` → NaN, which Zod v4 reports as its own received type.
        return Err("Invalid input: expected number, received NaN".to_string());
    }
    if raw.is_infinite() {
        return Err("Invalid input: expected number, received Infinity".to_string());
    }
    if raw.fract() != 0.0 {
        return Err("Invalid input: expected int, received number".to_string());
    }
    let v = raw as i64;
    if v < lo {
        return Err(format!("Too small: expected number to be >={lo}"));
    }
    if let Some(hi) = hi {
        if v > hi {
            return Err(format!("Too big: expected number to be <={hi}"));
        }
    }
    Ok(v)
}

/// v4 `GET /api/v1/photos` — `successResponse(result)`, the payload RAW.
///
/// The embedding is generated HERE (v4's service calls
/// `generateEmbeddingForUser` inline) and handed to the sync service body, with
/// [`needs_query_embedding`] as the single shared predicate so the two can
/// never disagree about whether a vector was required.
///
/// `provider` is OPTIONAL because v4 only reaches the model on the query
/// branch: a plain listing must still work on an assembly with no embedding
/// seam (a spine-less host), while a SEARCH without one is the loud
/// not-assembled refusal.
pub async fn photo_gallery_list<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    q: Option<String>,
    tag: Option<Vec<String>>,
    limit: Option<f64>,
    offset: Option<f64>,
) -> Response {
    let limit = match limit.map(|r| zod_int_in_range(r, 1, Some(200))) {
        Some(Err(msg)) => return Response::error(ErrorKind::BadRequest, msg),
        Some(Ok(v)) => Some(v),
        None => None,
    };
    let offset = match offset.map(|r| zod_int_in_range(r, 0, None)) {
        Some(Err(msg)) => return Response::error(ErrorKind::BadRequest, msg),
        Some(Ok(v)) => Some(v),
        None => None,
    };
    // v4: `rawTags.length > 0 ? rawTags : undefined` — an absent `tag` and an
    // empty repeat-list are the same thing.
    let tags: Option<Vec<String>> = tag.filter(|t| !t.is_empty());

    let embedding = if needs_query_embedding(q.as_deref()) {
        let query = q.clone().expect("needs_query_embedding implies Some");
        let Some(provider) = provider else {
            return Response::error(
                ErrorKind::Internal,
                "memory embedding not assembled (photo-gallery search embedding-seam deferral)",
            );
        };
        match provider
            .generate_embedding_for_user(&query, user_id, None, EmbeddingPriority::Interactive)
            .await
        {
            Ok(e) => Some(e.embedding),
            // v4's service lets the embedding error propagate to the route's
            // outer catch, which logs and answers the generic 500 text.
            Err(_) => return Response::error(ErrorKind::Internal, "Failed to list gallery"),
        }
    } else {
        None
    };

    let read = db.read_mount_index(|conn| {
        Ok(list_user_gallery(
            conn,
            q.as_deref(),
            embedding.as_deref(),
            tags.as_deref(),
            limit,
            offset,
        ))
    });
    match read {
        Ok(Ok(v)) => Response::PhotoGallery(v),
        // v4's outer catch: `serverError('Failed to list gallery')` — a FIXED
        // string, deliberately not the thrown message.
        Ok(Err(_)) | Err(_) => Response::error(ErrorKind::Internal, "Failed to list gallery"),
    }
}

/// v4 `POST /api/v1/photos` — `created(result)`; the payload RAW at 201.
#[allow(clippy::too_many_arguments)]
pub async fn photo_gallery_save(
    db: &Db,
    bytes: Arc<dyn FileBytesStore>,
    user_id: &str,
    file_id: Option<Option<Value>>,
    caption: Option<String>,
    tags: Option<Vec<String>>,
    chat_id: Option<String>,
    kept_at: &str,
) -> Response {
    // v4 `fileId: z.string().min(1, 'fileId is required')`. The double option
    // preserves Zod's absent-vs-null message split (see the wire type).
    let file_id = match file_id.as_ref().map(Option::as_ref) {
        Some(Some(Value::String(s))) if !s.is_empty() => s.clone(),
        Some(Some(Value::String(_))) => {
            return Response::error(ErrorKind::BadRequest, "fileId is required")
        }
        other => {
            let received = match other {
                None => "undefined",
                Some(None) => "null",
                Some(Some(v)) => zod_received(v),
            };
            return Response::error(
                ErrorKind::BadRequest,
                format!("Invalid input: expected string, received {received}"),
            );
        }
    };
    let tags = tags.unwrap_or_default();
    let kept_at = kept_at.to_string();
    let user_id = user_id.to_string();
    let result = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let main = ws.main().connection();
            Ok(save_to_user_gallery(
                main,
                mount,
                &file_id,
                caption.as_deref(),
                &tags,
                chat_id.as_deref(),
                &user_id,
                bytes.as_ref(),
                &kept_at,
            ))
        })
        .await;
    match result {
        Ok(Ok(v)) => Response::PhotoGallery(v),
        Ok(Err(e)) => lower(e, true, "Failed to save image"),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to save image"),
    }
}

/// v4 `GET /api/v1/photos/{id}` — `successResponse(entry)`, or `notFound`.
pub fn photo_gallery_entry_get(db: &Db, id: String) -> Response {
    // v4 `if (!id) return badRequest('Missing gallery entry id')`.
    if id.is_empty() {
        return Response::error(ErrorKind::BadRequest, "Missing gallery entry id");
    }
    let read = db.read_mount_index(|conn| Ok(get_user_gallery_entry(conn, &id)));
    match read {
        Ok(Ok(Some(v))) => Response::PhotoGallery(v),
        // v4 `notFound('Gallery entry')`.
        Ok(Ok(None)) => Response::error(ErrorKind::NotFound, "Gallery entry not found"),
        Ok(Err(_)) | Err(_) => Response::error(ErrorKind::Internal, "Failed to get gallery entry"),
    }
}

/// v4 `DELETE /api/v1/photos/{id}` — `{deleted: true, fileGC}`, or `notFound`.
pub async fn photo_gallery_entry_remove(db: &Db, id: String) -> Response {
    if id.is_empty() {
        return Response::error(ErrorKind::BadRequest, "Missing gallery entry id");
    }
    let result = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            Ok(remove_from_user_gallery(mount, &id))
        })
        .await;
    match result {
        Ok(Ok((true, file_gc))) => Response::PhotoGallery(json!({
            "deleted": true,
            "fileGC": file_gc,
        })),
        // v4 `if (!result.deleted) return notFound('Gallery entry')`.
        Ok(Ok((false, _))) => Response::error(ErrorKind::NotFound, "Gallery entry not found"),
        Ok(Err(e)) => lower(e, false, "Failed to delete gallery entry"),
        Err(_) => Response::error(ErrorKind::Internal, "Failed to delete gallery entry"),
    }
}

/// v4 `GET /api/v1/images/{id}` (`app/api/v1/images/[id]/route.ts:39-128`) —
/// the image-info read the deep detail modals hang off (P4.9a2).
///
/// The payload is `successResponse({ data: {…} })`, so the RAW body is the
/// `{data: {…}}` envelope. Five invariants, each pinned by the differential:
///
/// - `source` remapped `UPLOADED→upload` / `IMPORTED→import` /
///   `GENERATED→generated`, anything else (`SYSTEM`) falling back to `upload`
///   (`route.ts:62-64`).
/// - `tagType` is DERIVED, not stored: `CHARACTER` when the tagId is one of the
///   user's character ids, else `THEME` (`route.ts:66-74`).
/// - `characterGalleryLinks` is built ONLY when `sha256` is set, and ONLY from
///   linkers with `mountStoreType === 'character' && isPhotoAlbum`
///   (`route.ts:78-99`), resolved to characters via their vault mount id.
/// - 404 arms: missing file (`route.ts:43-45`) and a category that is neither
///   `IMAGE` nor `AVATAR` (`route.ts:48-50`).
/// - The nullable-optional columns (`width`, `height`, `generationPrompt`,
///   `generationModel`) are OMITTED when NULL — v4 hydrates NULL → `undefined`
///   and `JSON.stringify` drops the key (the P4.6p reads-omit-null rule).
pub fn image_info_get(db: &Db, user_id: &str, id: &str) -> Response {
    let read =
        db.read_main(|main| db.read_mount_index(|mount| image_info(main, mount, user_id, id)));
    match read {
        Ok(resp) => resp,
        // v4's outer catch: `serverError('Failed to fetch image')`.
        Err(_) => Response::error(ErrorKind::Internal, "Failed to fetch image"),
    }
}

/// A SQLite numeric cell → the JSON number JS would serialize (`9.0` → `9`).
/// `size`/`width`/`height` have REAL affinity, so an integer-valued cell is a
/// float on disk; better-sqlite3 hands v4 a JS `Number` and `JSON.stringify`
/// collapses it.
fn numeric_cell(row: &rusqlite::Row<'_>, idx: usize) -> Result<Value, rusqlite::Error> {
    Ok(match row.get_ref(idx)? {
        rusqlite::types::ValueRef::Null => Value::Null,
        rusqlite::types::ValueRef::Integer(i) => Value::from(i),
        rusqlite::types::ValueRef::Real(f) => crate::db::js_number_to_json(f),
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                idx,
                other.data_type(),
                "numeric column expected".into(),
            ))
        }
    })
}

fn image_info(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    id: &str,
) -> Result<Response, crate::db::DbError> {
    use rusqlite::OptionalExtension;

    struct ImageRow {
        sha256: Option<String>,
        original_filename: String,
        mime_type: String,
        size: Value,
        width: Value,
        height: Value,
        category: String,
        source: String,
        generation_prompt: Option<String>,
        generation_model: Option<String>,
        tags: Option<String>,
        created_at: String,
        updated_at: String,
    }

    let row = main
        .query_row(
            "SELECT sha256, originalFilename, mimeType, size, width, height, category, \
                    source, generationPrompt, generationModel, tags, createdAt, updatedAt \
             FROM files WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(ImageRow {
                    sha256: row.get(0)?,
                    original_filename: row.get(1)?,
                    mime_type: row.get(2)?,
                    size: numeric_cell(row, 3)?,
                    width: numeric_cell(row, 4)?,
                    height: numeric_cell(row, 5)?,
                    category: row.get(6)?,
                    source: row.get(7)?,
                    generation_prompt: row.get(8)?,
                    generation_model: row.get(9)?,
                    tags: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(crate::db::DbError::from)?;

    // v4 `if (!image) return notFound('Image')` (`route.ts:43-45`).
    let Some(row) = row else {
        return Ok(Response::error(ErrorKind::NotFound, "Image not found"));
    };

    // v4 re-validates every read against `FileEntrySchema` inside `safeQuery`'s
    // null-fallback (`base.repository.ts:121` / `:246`): a row that fails the
    // parse reads as ABSENT, so the route 404s it. `sha256` is
    // `z.string().length(64)` — the one arm the fixture stages (a NULL/short
    // hash on a legacy-shaped row); mirror it rather than the whole schema.
    let sha256 = match row.sha256.as_deref() {
        Some(s) if s.len() == 64 => s.to_string(),
        _ => return Ok(Response::error(ErrorKind::NotFound, "Image not found")),
    };

    // v4 `if (image.category !== 'IMAGE' && image.category !== 'AVATAR')`
    // (`route.ts:48-50`).
    if row.category != "IMAGE" && row.category != "AVATAR" {
        return Ok(Response::error(ErrorKind::NotFound, "Image not found"));
    }

    let characters = crate::db::characters_read::find_by_user_id(main, mount, user_id)?;

    // v4 `route.ts:56-59` — the usage counts over the character roster.
    let characters_using_as_default = characters
        .iter()
        .filter(|c| c["defaultImageId"].as_str() == Some(id))
        .count();
    let chat_avatar_overrides: usize = characters
        .iter()
        .map(|c| {
            c["avatarOverrides"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|o| o["imageId"].as_str() == Some(id))
                        .count()
                })
                .unwrap_or(0)
        })
        .sum();

    // v4 `route.ts:62-64` — "Map source to old format".
    let source = match row.source.as_str() {
        "UPLOADED" => "upload",
        "IMPORTED" => "import",
        "GENERATED" => "generated",
        _ => "upload",
    };

    // v4 `route.ts:66-74` — tagType is derived from the roster, never stored.
    let character_ids: std::collections::HashSet<&str> =
        characters.iter().filter_map(|c| c["id"].as_str()).collect();
    let tags: Vec<String> = row
        .tags
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let tags_json: Vec<Value> = tags
        .iter()
        .map(|tag_id| {
            let tag_type = if character_ids.contains(tag_id.as_str()) {
                "CHARACTER"
            } else {
                "THEME"
            };
            json!({ "tagId": tag_id, "tagType": tag_type })
        })
        .collect();

    // v4 `route.ts:78-99` — which character vaults hold a copy of these bytes?
    // The `if (image.sha256)` guard is trivially true after the validation
    // mirror above (a parsed row always carries 64 hex chars); kept for shape.
    let mut character_gallery_links: Vec<Value> = Vec::new();
    if !sha256.is_empty() {
        let mut mount_to_character: std::collections::HashMap<&str, (&str, &str)> =
            std::collections::HashMap::new();
        for c in &characters {
            if let Some(mp) = c["characterDocumentMountPointId"].as_str() {
                mount_to_character.insert(
                    mp,
                    (
                        c["id"].as_str().unwrap_or_default(),
                        c["name"].as_str().unwrap_or_default(),
                    ),
                );
            }
        }
        let summary =
            crate::photos::photo_link_summary::get_photo_link_summary_by_sha256(mount, &sha256)?;
        if let Some(linkers) = summary["linkers"].as_array() {
            for linker in linkers {
                if linker["mountStoreType"].as_str() == Some("character")
                    && linker["isPhotoAlbum"].as_bool() == Some(true)
                {
                    if let Some((char_id, char_name)) = linker["mountPointId"]
                        .as_str()
                        .and_then(|mp| mount_to_character.get(mp))
                    {
                        character_gallery_links.push(json!({
                            "characterId": char_id,
                            "characterName": char_name,
                            "linkId": linker["linkId"],
                        }));
                    }
                }
            }
        }
    }

    // The `{data: {…}}` envelope, keys in v4's literal order (`route.ts:101-123`)
    // — nullable-optional keys inserted conditionally at their literal position.
    let mut data = serde_json::Map::new();
    data.insert("id".into(), Value::String(id.to_string()));
    data.insert("userId".into(), Value::String(user_id.to_string()));
    data.insert("filename".into(), Value::String(row.original_filename));
    data.insert(
        "filepath".into(),
        // v4 `getFilePath(image)` — always the API route path
        // (`lib/api/middleware/file-path.ts:29-31`).
        Value::String(format!("/api/v1/files/{id}")),
    );
    data.insert("mimeType".into(), Value::String(row.mime_type));
    data.insert("size".into(), row.size);
    if !row.width.is_null() {
        data.insert("width".into(), row.width);
    }
    if !row.height.is_null() {
        data.insert("height".into(), row.height);
    }
    data.insert("source".into(), Value::String(source.to_string()));
    if let Some(gp) = row.generation_prompt {
        data.insert("generationPrompt".into(), Value::String(gp));
    }
    if let Some(gm) = row.generation_model {
        data.insert("generationModel".into(), Value::String(gm));
    }
    data.insert("createdAt".into(), Value::String(row.created_at));
    data.insert("updatedAt".into(), Value::String(row.updated_at));
    data.insert("tags".into(), Value::Array(tags_json));
    data.insert(
        "characterGalleryLinks".into(),
        Value::Array(character_gallery_links),
    );
    data.insert(
        "_count".into(),
        json!({
            "charactersUsingAsDefault": characters_using_as_default,
            "chatAvatarOverrides": chat_avatar_overrides,
        }),
    );

    Ok(Response::PhotoGallery(json!({ "data": data })))
}

/// Zod v4's `received` word for a non-string JSON value.
fn zod_received(v: &Value) -> &'static str {
    match v {
        // Unreachable via the double option (an explicit null is Some(None)),
        // kept so the mapping stays total.
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
