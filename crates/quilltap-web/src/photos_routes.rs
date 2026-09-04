//! P4.9a REST edges (lane A): the user photo gallery. Each edge dispatches the
//! corresponding `Request` and UNWRAPS the dispatch envelope to v4's RAW route
//! body (the P4.6ah lesson — v4-shaped clients read the raw body, not the
//! tagged `Response`).
//!
//! - `GET    /api/v1/photos`       → `{entries, total, hasMore}`
//! - `POST   /api/v1/photos`       → the save receipt, at **201** (v4 `created`)
//! - `GET    /api/v1/photos/{id}`  → one entry
//! - `DELETE /api/v1/photos/{id}`  → `{deleted, fileGC}`
//!
//! ## Two things this edge must get right
//!
//! **`tag` repeats.** v4 reads it with `searchParams.getAll('tag')`, so
//! `?tag=a&tag=b` is a two-element filter. A `HashMap` query extractor would
//! silently keep one of them, so the extractor here is a `Vec<(String, String)>`
//! — serde_urlencoded preserves repeats in order.
//!
//! **`Number()` runs BEFORE Zod.** v4 reads `limit`/`offset` as
//! `searchParams.has(k) ? Number(searchParams.get(k)) : undefined`. So the edge
//! must hand the core a `f64` that is NaN for `?limit=abc` and fractional for
//! `?limit=1.5`, and let the core produce Zod's (different) message for each.
//! Parsing to an integer here would repair inputs v4 rejects — see
//! [`js_number`].

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a photo-gallery body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::PhotoGallery(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => error_to_http(e),
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

/// `searchParams.has(k) ? Number(searchParams.get(k)) : undefined`, over the
/// repeat-preserving pair list. `has`/`get` read the FIRST occurrence.
fn number_param(pairs: &[(String, String)], key: &str) -> Option<f64> {
    crate::query::first(pairs, key)
        // §3 of the consult-wire round: the local `js_number` twin retired at
        // unification in favour of the canonical `jsnum::number_from_str`
        // (lane P4.6bd lifted it; both lanes recorded this rider). The
        // canonical port is STRICTER and matches v4 where the twin did not:
        // JS takes no sign on a radix literal, so `Number('+0x10')` is NaN,
        // where the twin returned 16. NaN is a VALUE here, not a failure — v4
        // hands it to Zod, which produces its own message.
        .map(quilltap_core::jsnum::number_from_str)
}

// ===========================================================================
// /api/v1/photos
// ===========================================================================

pub async fn photos_list(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    let tag: Vec<String> = crate::query::all(&pairs, "tag")
        .into_iter()
        .map(str::to_string)
        .collect();
    let req = CoreRequest::PhotoGalleryList {
        q: crate::query::first(&pairs, "q").map(str::to_string),
        // v4: `rawTags.length > 0 ? rawTags : undefined`.
        tag: if tag.is_empty() { None } else { Some(tag) },
        limit: number_param(&pairs, "limit"),
        offset: number_param(&pairs, "offset"),
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn photos_save(State(state): State<SharedState>, body: String) -> AxumResponse {
    // Decode through the Request itself so the absent / explicit-null / value
    // tri-state on `fileId` is resolved by exactly ONE piece of code (the
    // `double_option` field), not re-implemented at the edge.
    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&body) else {
        return error_json(StatusCode::BAD_REQUEST, "Validation error");
    };
    map.retain(|k, _| matches!(k.as_str(), "fileId" | "caption" | "tags" | "chatId"));
    map.insert("type".into(), Value::String("photoGallerySave".into()));
    let Ok(req) = serde_json::from_value::<CoreRequest>(Value::Object(map)) else {
        return error_json(StatusCode::BAD_REQUEST, "Validation error");
    };

    match dispatch_core(&state, req).await {
        // v4 `created(result)` — 201, body RAW.
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/photos/{id}
// ===========================================================================

pub async fn photo_entry_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::PhotoGalleryEntryGet { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn photo_entry_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::PhotoGalleryEntryRemove { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/images/{id} (P4.9a2 — the image-info read)
// ===========================================================================

/// v4 `GET /api/v1/images/[id]` — the `{data: {…}}` image-info envelope the
/// deep detail modals read (`ImageDetailModal.tsx:43-46`,
/// `ChatGalleryImageViewModal.tsx:63`). RAW body at 200; both 404 arms answer
/// v4's `notFound('Image')`.
pub async fn image_info_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::ImageInfoGet { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
