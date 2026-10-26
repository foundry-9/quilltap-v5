//! `/api/v1/inventory/{itemId}/images` — the Inventory object-images REST edge
//! (P4.D325; v4 `app/api/v1/inventory/[itemId]/images/route.ts`, `4fbdad186`
//! at the `322a5488e` pin) — the wardrobe edge's twin WITHOUT `link-image`.
//! ONE route for every tier: the container rides in the query
//! (`?scope=…&id=…`).
//!
//! - `GET` → `{ current, images }`; `GET ?action=save-targets` → `{ albums }`
//!   (v4's `withActionDispatch({ 'save-targets' }, handleList)`: the dispatch
//!   runs FIRST, before the query is read).
//! - `POST ?action=generate | upload | set-current | delete-image |
//!   save-to-store` (NO default: a bare POST is `Action parameter required`,
//!   an unknown action `Unknown action: <a>`, both with `availableActions`).
//!
//! The order is v4's: the container query (400 — Zod's own sentences joined
//! `'; '`), the object's home (404 `Inventory object not found`), and only
//! THEN the body: `generate` reads it as TEXT (empty means `{}`, unparseable
//! is `Request body is not valid JSON` + v4's DEBUG); `set-current` /
//! `delete-image` `.parse` `{fileId}` INSIDE their try (a ZodError is the
//! middleware's `Validation error` 400; unparseable JSON its 500);
//! `save-to-store` reads `req.json().catch(() => ({}))`; `upload` parses
//! multipart. So the edge calls core's gate ([`resolve_home`]) once, then the
//! action's `…_on_home` form.
//!
//! **The 502.** v4 answers a non-refusal provider failure with 502; the
//! dispatch boundary has no 502 kind, so core carries it as `Internal` with
//! v4's `details: { trail, refused: false }`. [`render`] restores the 502 for
//! `generate` from exactly that shape (the wardrobe edge's idiom).

use axum::body::Bytes;
use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::inventory_item_images::{
    delete_on_home, generate_on_home, list_on_home, parse_generate_body, resolve_home,
    save_targets_on_home, save_to_store_on_home, set_current_on_home, upload_on_home,
};
use quilltap_core::api::types::{
    ErrorKind, InventoryContainer, InventoryScope, Response as CoreResponse,
};
use quilltap_core::db::runtime::Db;
use quilltap_core::inventory::item_images::InventoryItemHome;
use quilltap_core::services::wardrobe_image_bridge::WardrobeImageKind;
use serde_json::{json, Value};

use crate::multipart::FormData;
use crate::query::{dispatch_action, dispatch_required_action, first, QueryPairs};
use crate::state::SharedState;

/// v4's POST `withActionDispatch` map, in its literal order (`route.ts:222-230`).
const ACTIONS: [&str; 5] = [
    "generate",
    "upload",
    "set-current",
    "delete-image",
    "save-to-store",
];
/// v4's GET `withActionDispatch` map — `handleList` is the fallback.
const GET_ACTIONS: [&str; 1] = ["save-targets"];
const ROUTE: &str = "/api/v1/inventory/[itemId]/images";
/// v4 `containerQuerySchema`'s scope enum (`character, project, group,
/// general` — the wardrobe route's ORDER, not `InventoryScopeSchema`'s), so
/// Zod's sentence is byte-identical to the wardrobe edge's.
pub(crate) const SCOPE_ISSUE: &str =
    "Invalid option: expected one of \"character\"|\"project\"|\"group\"|\"general\"";
const LOG_TAG: &str = "[Inventory Images v1]";

fn json_response(status: StatusCode, body: &Value) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn error_json(status: StatusCode, message: &str) -> AxumResponse {
    json_response(status, &json!({ "error": message }))
}

/// [`crate::inventory_routes::render`], plus `generate`'s 502: an `Internal`
/// carrying `details.refused === false` is v4's provider failure.
pub fn render(generate: bool, resp: CoreResponse, success: StatusCode) -> (StatusCode, Value) {
    let provider_failed = generate
        && matches!(&resp, CoreResponse::Error(e)
            if e.kind == ErrorKind::Internal
                && e.details
                    .as_deref()
                    .and_then(|d| d.get("refused"))
                    .and_then(Value::as_bool)
                    == Some(false));
    let (status, body) = crate::inventory_routes::render(resp, success);
    if provider_failed {
        (StatusCode::BAD_GATEWAY, body)
    } else {
        (status, body)
    }
}

fn rendered(generate: bool, resp: CoreResponse, success: StatusCode) -> AxumResponse {
    let (status, body) = render(generate, resp, success);
    json_response(status, &body)
}

/// v4 `containerQuerySchema.safeParse({ scope: sp.get('scope') ?? undefined,
/// id: sp.get('id') ?? undefined })`, the issue messages joined `'; '`. The
/// refine runs after the non-fatal `min(1)` issue on `id`, but NOT after the
/// fatal enum failure on `scope` (the wardrobe twin's measurement).
pub fn read_container_query(pairs: &QueryPairs) -> Result<InventoryContainer, String> {
    let id = first(pairs, "id").map(str::to_string);
    let scope = match first(pairs, "scope") {
        Some("character") => InventoryScope::Character,
        Some("project") => InventoryScope::Project,
        Some("group") => InventoryScope::Group,
        Some("general") => InventoryScope::General,
        _ => {
            let mut issues = vec![SCOPE_ISSUE];
            if id.as_deref() == Some("") {
                issues.push(quilltap_core::api::wardrobe_item_images::CONTAINER_ID_EMPTY_ISSUE);
            }
            return Err(issues.join("; "));
        }
    };
    match quilltap_core::api::inventory_item_images::container_query_issue(scope, id.as_deref()) {
        None => Ok(InventoryContainer { scope, id }),
        Some(msg) => Err(msg),
    }
}

fn db_of(state: &SharedState) -> Result<Db, Box<AxumResponse>> {
    let Some(host) = state.host() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "server failed to start",
        )));
    };
    host.core().db().ok_or_else(|| {
        Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The database is locked. Unlock it to continue.",
        ))
    })
}

/// `GET /api/v1/inventory/{itemId}/images[?action=save-targets]`.
pub async fn inventory_item_images_get(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    let action = match dispatch_action(&pairs, &GET_ACTIONS, "GET", ROUTE) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let container = match read_container_query(&pairs) {
        Ok(c) => c,
        Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
    };
    let db = match db_of(&state) {
        Ok(db) => db,
        Err(r) => return *r,
    };
    let home = match resolve_home(&db, &container, &item_id).await {
        Ok(h) => h,
        Err(r) => return rendered(false, r, StatusCode::OK),
    };
    match action {
        None => rendered(false, list_on_home(&db, &home), StatusCode::OK),
        Some(_) => rendered(false, save_targets_on_home(&db), StatusCode::OK),
    }
}

/// `POST /api/v1/inventory/{itemId}/images?action=…`.
pub async fn inventory_item_images_post(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
    req: Request,
) -> AxumResponse {
    let action = match dispatch_required_action(&pairs, &ACTIONS, "POST", ROUTE) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let container = match read_container_query(&pairs) {
        Ok(c) => c,
        Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
    };
    let db = match db_of(&state) {
        Ok(db) => db,
        Err(r) => return *r,
    };
    let home = match resolve_home(&db, &container, &item_id).await {
        Ok(h) => h,
        Err(r) => return rendered(false, r, StatusCode::OK),
    };

    match action {
        "generate" => {
            let bytes = match Bytes::from_request(req, &state).await {
                Ok(b) => b,
                Err(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            };
            let text = String::from_utf8_lossy(&bytes);
            // v4: `if (text.trim())` — an empty / whitespace body is `{}`.
            let image_profile_id = if quilltap_core::jsstr::js_trim(&text).is_empty() {
                None
            } else {
                let json: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        tracing::debug!(
                            itemId = %item_id,
                            context = "inventory",
                            "{LOG_TAG} Generate body is not valid JSON"
                        );
                        return error_json(
                            StatusCode::BAD_REQUEST,
                            "Request body is not valid JSON",
                        );
                    }
                };
                match parse_generate_body(&json) {
                    Ok(id) => id,
                    Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
                }
            };
            let seams =
                quilltap_host::wardrobe_item_image::wardrobe_item_image_seams(&state.version);
            let resp = generate_on_home(
                &db,
                &seams,
                quilltap_core::api::engine::SINGLE_USER_ID,
                &home,
                image_profile_id.as_deref(),
            )
            .await;
            rendered(true, resp, StatusCode::CREATED)
        }
        "upload" => upload(&state, &db, &home, req).await,
        "set-current" | "delete-image" => {
            let bytes = match Bytes::from_request(req, &state).await {
                Ok(b) => b,
                Err(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            };
            // `fileIdBodySchema.parse(await req.json())` INSIDE the try:
            // unparseable JSON is not a ZodError — `inventoryErrorResponse`
            // rethrows it to the middleware's 500.
            let body: Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            };
            let resp = if action == "set-current" {
                set_current_on_home(&db, &home, &body).await
            } else {
                delete_on_home(&db, &home, &body).await
            };
            rendered(false, resp, StatusCode::OK)
        }
        "save-to-store" => {
            // v4 `await req.json().catch(() => ({}))`.
            let body: Value = match Bytes::from_request(req, &state).await {
                Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
                Err(_) => json!({}),
            };
            let parsed =
                match quilltap_core::photos::save_attribution::parse_save_image_request(&body) {
                    Ok(p) => p,
                    Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
                };
            let bytes: std::sync::Arc<
                dyn quilltap_core::photos::save_image_to_album::FileBytesStore,
            > = std::sync::Arc::new(quilltap_core::services::file_storage::ProductionFileBytes {
                db: db.clone(),
                backend: std::sync::Arc::new(quilltap_host::LocalStorageBackend::new(
                    state.base_dir.join("files"),
                )),
                codec: std::sync::Arc::new(quilltap_host::HostImageCodec),
            });
            let side_effects: std::sync::Arc<
                dyn quilltap_core::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
            > = std::sync::Arc::new(
                quilltap_core::photos::mount_embedding_effects::MountEmbeddingSideEffects::new(
                    db.clone(),
                ),
            );
            let resp = save_to_store_on_home(
                &db,
                bytes,
                side_effects,
                quilltap_core::api::engine::SINGLE_USER_ID,
                &home,
                parsed,
                &quilltap_core::clock::now_iso(),
            )
            .await;
            rendered(false, resp, StatusCode::OK)
        }
        _ => unreachable!("dispatch_required_action answers only listed actions"),
    }
}

/// The `upload` leg (v4 `handleUpload`).
async fn upload(
    state: &SharedState,
    db: &Db,
    home: &InventoryItemHome,
    req: Request,
) -> AxumResponse {
    let form = match FormData::from_request(req, state).await {
        Ok(f) => f,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Expected a multipart upload"),
    };
    // v4 `form.get('file')` + `instanceof File`.
    let Some(file) = form.first("file").filter(|f| f.filename.is_some()) else {
        return error_json(StatusCode::BAD_REQUEST, "No file provided");
    };
    let content_type = file.content_type.clone().unwrap_or_default();
    if let Err(msg) =
        quilltap_core::api::images::validate_image_file(&content_type, file.bytes.len())
    {
        return error_json(StatusCode::BAD_REQUEST, &msg);
    }
    // v4 `uploadKindSchema.safeParse(form.get('kind') ?? undefined)`.
    let kind = match form.first("kind") {
        None => WardrobeImageKind::Uploaded,
        Some(f) if f.filename.is_none() && f.bytes == b"uploaded" => WardrobeImageKind::Uploaded,
        Some(f) if f.filename.is_none() && f.bytes == b"imported" => WardrobeImageKind::Imported,
        Some(_) => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "kind must be \"uploaded\" or \"imported\"",
            )
        }
    };
    let codec = quilltap_host::HostImageCodec;
    let converted = quilltap_core::services::file_storage::convert_to_webp(
        &codec,
        &file.bytes,
        &content_type,
        file.filename.as_deref().unwrap_or(""),
    );
    let blob_webp: std::sync::Arc<
        dyn quilltap_core::services::mount_index::blob_transcode::WebpTranscoder,
    > = std::sync::Arc::new(
        quilltap_core::services::mount_index::normalize_blob_image::PixelCodecWebp(
            std::sync::Arc::new(quilltap_host::HostImageCodec),
        ),
    );
    let resp = upload_on_home(
        db,
        blob_webp,
        home,
        kind,
        converted.buffer,
        converted.mime_type,
        converted.width,
        converted.height,
    )
    .await;
    rendered(false, resp, StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(q: &[(&str, &str)]) -> QueryPairs {
        q.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_scope_sentence_is_the_wardrobe_routes() {
        // Both v4 routes' `containerQuerySchema` list the scopes in the same
        // order, so the sentence is one literal on two edges.
        assert_eq!(
            SCOPE_ISSUE,
            "Invalid option: expected one of \"character\"|\"project\"|\"group\"|\"general\""
        );
    }

    #[test]
    fn the_container_query() {
        assert_eq!(
            read_container_query(&pairs(&[("scope", "general")])).unwrap(),
            InventoryContainer {
                scope: InventoryScope::General,
                id: None
            }
        );
        assert_eq!(
            read_container_query(&pairs(&[("scope", "project")])).unwrap_err(),
            "id is required for this scope"
        );
        assert_eq!(read_container_query(&pairs(&[])).unwrap_err(), SCOPE_ISSUE);
        assert_eq!(
            read_container_query(&pairs(&[("scope", "project"), ("id", "")])).unwrap_err(),
            "Too small: expected string to have >=1 characters; id is required for this scope"
        );
    }

    /// The 502 is restored for `generate` ONLY; the factory's renderer keeps 500.
    #[test]
    fn generate_restores_v4s_502() {
        let mut failed = CoreResponse::error(ErrorKind::Internal, "Image generation failed: x");
        if let CoreResponse::Error(e) = &mut failed {
            e.details = Some(Box::new(json!({ "trail": null, "refused": false })));
        }
        let (status, body) = render(true, failed.clone(), StatusCode::CREATED);
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body.to_string(),
            r#"{"error":"Image generation failed: x","details":{"trail":null,"refused":false}}"#
        );
        assert_eq!(
            render(false, failed.clone(), StatusCode::OK).0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            crate::inventory_routes::render(failed, StatusCode::OK).0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn the_action_lists_are_v4s() {
        assert_eq!(
            ACTIONS,
            [
                "generate",
                "upload",
                "set-current",
                "delete-image",
                "save-to-store"
            ]
        );
        assert!(!ACTIONS.contains(&"link-image"));
        assert_eq!(GET_ACTIONS, ["save-targets"]);
    }
}
