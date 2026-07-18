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

/// JS `Number(string)`, for the two numeric query params.
///
/// A local implementation rather than a shared one on purpose: the core already
/// carries a private twin in `tools::text_block_parser`, and lifting that into
/// `jsnum` would touch a file outside this lane's ownership. The consolidation
/// is recorded as a rider in this lane's status-log entry — it is a DRY debt,
/// not a behavior difference (both follow the same spec, and the arms this edge
/// can actually reach are pinned by `list_limit_nan` / `list_limit_fraction`).
///
/// `None` means NaN — which is a VALUE here, not a failure: v4 hands NaN to Zod
/// and Zod produces its own message for it.
fn js_number(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        // `Number('')` and `Number('   ')` are both 0.
        return Some(0.0);
    }
    match t {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let (negative, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = body.strip_prefix(prefix) {
            // JS radix literals take no sign: `Number('-0x1')` is NaN.
            if negative || digits.is_empty() {
                return None;
            }
            return u128::from_str_radix(digits, radix).ok().map(|v| v as f64);
        }
    }
    // Rust's `parse::<f64>` accepts `inf`/`nan`/`infinity`; JS does not.
    if t.chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E'))
    {
        return None;
    }
    t.parse::<f64>().ok()
}

/// `searchParams.has(k) ? Number(searchParams.get(k)) : undefined`, over the
/// repeat-preserving pair list. `has`/`get` read the FIRST occurrence.
fn number_param(pairs: &[(String, String)], key: &str) -> Option<f64> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| js_number(v).unwrap_or(f64::NAN))
}

/// `searchParams.get(k)` — the first occurrence, or absent.
fn string_param(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// `searchParams.getAll(k)` — every occurrence, in order.
fn all_params(pairs: &[(String, String)], key: &str) -> Vec<String> {
    pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

// ===========================================================================
// /api/v1/photos
// ===========================================================================

pub async fn photos_list(
    State(state): State<SharedState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> AxumResponse {
    let tag = all_params(&pairs, "tag");
    let req = CoreRequest::PhotoGalleryList {
        q: string_param(&pairs, "q"),
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
