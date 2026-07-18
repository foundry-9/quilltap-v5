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
pub async fn photo_gallery_list<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
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
