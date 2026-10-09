//! `/api/v1/wardrobe/{itemId}/images` — the wardrobe item-images REST edge
//! (P4.D263; v4 `app/api/v1/wardrobe/[itemId]/images/route.ts`, `7c8572869`).
//! ONE route for every tier: the container rides in the query (`?scope=…&id=…`).
//!
//! - `GET` → `{ current, images }`.
//! - `POST ?action=generate | upload | set-current | delete-image` (v4's
//!   `withActionDispatch` with NO default: a bare POST is `Action parameter
//!   required`, an unknown action `Unknown action: <a>`, both with
//!   `availableActions`).
//!
//! The order is v4's: the container query (400 — Zod's own sentences joined
//! `'; '`, then the refine's `id is required for this scope`), the item's home
//! (404), and only THEN the body (`generate` reads it as TEXT — empty means
//! `{}`, unparseable is `Invalid JSON body`; `set-current` / `delete-image`
//! parse `{fileId}`; `upload` parses multipart). So the edge calls core's
//! gate ([`resolve_home`]) once, then the action's `…_on_home` form —
//! never the dispatch verb, which would gate a second time.
//!
//! `upload` is a BINARY leg with no dispatch verb (contract C2 §6):
//! `Expected a multipart upload` (v4 CATCHES `req.formData()` here, unlike
//! the images collection), `No file provided`, v4's `validateImageFile`
//! sentences, `kind must be "uploaded" or "imported"`, then v4's
//! `convertToWebP` through the host codec and the item-image write → 201.
//!
//! **The 502.** v4 answers a non-refusal provider failure with 502; the frozen
//! dispatch boundary has no 502 kind, so core carries it as `Internal` with
//! v4's `details: { trail, refused: false }` (the `chat_media` precedent). This
//! edge restores the 502 for `generate` from exactly that shape
//! ([`render`]).

use axum::body::Bytes;
use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::types::{ErrorKind, Response as CoreResponse, WardrobeContainerScope};
use quilltap_core::api::wardrobe_item_images::{
    delete_on_home, generate_on_home, list_on_home, resolve_home, set_current_on_home,
    upload_on_home,
};
use quilltap_core::db::runtime::Db;
use quilltap_core::services::wardrobe_image_bridge::WardrobeImageKind;
use serde_json::{json, Value};

use crate::multipart::FormData;
use crate::query::{dispatch_required_action, first, QueryPairs};
use crate::state::SharedState;

/// v4's `withActionDispatch` map, in its literal order.
const ACTIONS: [&str; 4] = ["generate", "upload", "set-current", "delete-image"];
const ROUTE: &str = "/api/v1/wardrobe/[itemId]/images";
const SCOPE_ISSUE: &str =
    "Invalid option: expected one of \"character\"|\"project\"|\"group\"|\"general\"";

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

/// A dispatch [`CoreResponse`] → v4's HTTP status and body. `success` is the
/// action's success status (201 for `generate` / `upload`, else 200). An error
/// renders `{ error, details? }` (v4 `errorResponse`); `generate`'s
/// `Internal` carrying `details.refused === false` is v4's 502.
pub fn render(generate: bool, resp: CoreResponse, success: StatusCode) -> (StatusCode, Value) {
    match resp {
        CoreResponse::WardrobeItemImages(v) => (success, v),
        CoreResponse::Error(e) => {
            let provider_failed = generate
                && e.kind == ErrorKind::Internal
                && e.details
                    .as_deref()
                    .and_then(|d| d.get("refused"))
                    .and_then(Value::as_bool)
                    == Some(false);
            let status = match e.kind {
                _ if provider_failed => StatusCode::BAD_GATEWAY,
                ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                ErrorKind::NotFound => StatusCode::NOT_FOUND,
                ErrorKind::Conflict => StatusCode::CONFLICT,
                ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
                ErrorKind::Forbidden => StatusCode::FORBIDDEN,
                ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
                ErrorKind::Locked | ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let mut body = json!({ "error": e.message });
            if let Some(d) = e.details {
                body["details"] = *d;
            }
            (status, body)
        }
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": format!("unexpected response: {other:?}") }),
        ),
    }
}

fn rendered(generate: bool, resp: CoreResponse, success: StatusCode) -> AxumResponse {
    let (status, body) = render(generate, resp, success);
    json_response(status, &body)
}

/// v4 `readContainerQuery(req)` — `containerQuerySchema.safeParse({ scope:
/// sp.get('scope') ?? undefined, id: sp.get('id') ?? undefined })`, the issue
/// messages joined `'; '`. MEASURED (the routes oracle): the refine runs after
/// the non-fatal `min(1)` issue on `id`, but NOT after the fatal enum failure
/// on `scope`.
pub fn read_container_query(
    pairs: &QueryPairs,
) -> Result<(WardrobeContainerScope, Option<String>), String> {
    let id = first(pairs, "id").map(str::to_string);
    let scope = match first(pairs, "scope") {
        Some("character") => WardrobeContainerScope::Character,
        Some("project") => WardrobeContainerScope::Project,
        Some("group") => WardrobeContainerScope::Group,
        Some("general") => WardrobeContainerScope::General,
        _ => {
            // The fatal enum issue: the refine does not run, `id`'s `min(1)`
            // still reports.
            let mut issues = vec![SCOPE_ISSUE];
            if id.as_deref() == Some("") {
                issues.push(quilltap_core::api::wardrobe_item_images::CONTAINER_ID_EMPTY_ISSUE);
            }
            return Err(issues.join("; "));
        }
    };
    // The id rules live in core (ONE copy of v4's sentences — the
    // `f5e953a3f` unification folded the edge's twin onto it).
    match quilltap_core::api::wardrobe_item_images::container_query_issue(scope, id.as_deref()) {
        None => Ok((scope, id)),
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

/// `GET /api/v1/wardrobe/{itemId}/images`.
pub async fn wardrobe_item_images_get(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    let (scope, container_id) = match read_container_query(&pairs) {
        Ok(q) => q,
        Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
    };
    let db = match db_of(&state) {
        Ok(db) => db,
        Err(r) => return *r,
    };
    let home = match resolve_home(&db, scope, container_id.as_deref(), &item_id).await {
        Ok(h) => h,
        Err(r) => return rendered(false, r, StatusCode::OK),
    };
    rendered(false, list_on_home(&db, scope, &home).await, StatusCode::OK)
}

/// `POST /api/v1/wardrobe/{itemId}/images?action=…`.
pub async fn wardrobe_item_images_post(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
    req: Request,
) -> AxumResponse {
    let action = match dispatch_required_action(&pairs, &ACTIONS, "POST", ROUTE) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let (scope, container_id) = match read_container_query(&pairs) {
        Ok(q) => q,
        Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
    };
    let db = match db_of(&state) {
        Ok(db) => db,
        Err(r) => return *r,
    };
    let home = match resolve_home(&db, scope, container_id.as_deref(), &item_id).await {
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
                    Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid JSON body"),
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
                scope,
                container_id.as_deref(),
                &home,
                image_profile_id.as_deref(),
            )
            .await;
            rendered(true, resp, StatusCode::CREATED)
        }
        "upload" => upload(&state, &db, scope, &home, req).await,
        "set-current" | "delete-image" => {
            let bytes = match Bytes::from_request(req, &state).await {
                Ok(b) => b,
                Err(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            };
            // v4 `fileIdBodySchema.parse(await req.json())` INSIDE the
            // action's try: unparseable JSON is not a ZodError, so
            // `mapWriteError` rethrows it — the middleware's 500.
            let body: Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            };
            let file_id = match parse_file_id_body(&body) {
                Ok(id) => id,
                Err(msg) => return error_json(StatusCode::BAD_REQUEST, &msg),
            };
            let resp = if action == "set-current" {
                set_current_on_home(&db, scope, &home, &file_id).await
            } else {
                delete_on_home(&db, scope, &home, &file_id).await
            };
            rendered(false, resp, StatusCode::OK)
        }
        _ => unreachable!("dispatch_required_action answers only listed actions"),
    }
}

/// v4 `generateBodySchema` — `{ imageProfileId: z.string().min(1).nullable()
/// .optional() }` (an object; unknown keys stripped).
fn parse_generate_body(v: &Value) -> Result<Option<String>, String> {
    let Some(obj) = v.as_object() else {
        return Err(zod_object_expected(v));
    };
    match obj.get("imageProfileId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => {
            Err("Too small: expected string to have >=1 characters".to_string())
        }
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!(
            "Invalid input: expected string, received {}",
            zod_received(other)
        )),
    }
}

/// v4 `fileIdBodySchema` — `{ fileId: z.string().min(1, 'fileId is required') }`.
fn parse_file_id_body(v: &Value) -> Result<String, String> {
    let Some(obj) = v.as_object() else {
        return Err(zod_object_expected(v));
    };
    match obj.get("fileId") {
        Some(Value::String(s)) if s.is_empty() => Err("fileId is required".to_string()),
        Some(Value::String(s)) => Ok(s.clone()),
        None => Err("Invalid input: expected string, received undefined".to_string()),
        Some(other) => Err(format!(
            "Invalid input: expected string, received {}",
            zod_received(other)
        )),
    }
}

/// Zod 4's `received` word for a JSON value.
fn zod_received(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn zod_object_expected(v: &Value) -> String {
    format!(
        "Invalid input: expected object, received {}",
        zod_received(v)
    )
}

/// The `upload` leg (v4 `handleUpload`, `route.ts:202-238`).
async fn upload(
    state: &SharedState,
    db: &Db,
    scope: WardrobeContainerScope,
    home: &quilltap_core::services::wardrobe_container::WardrobeItemHome,
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
    // v4 `uploadKindSchema.safeParse(form.get('kind') ?? undefined)` — absent →
    // the `'uploaded'` default; a FILE part or any other string refuses.
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
        scope,
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

    /// v4 `containerQuerySchema`: the enum, the `min(1)`, the refine — and the
    /// refine only once the object parsed.
    #[test]
    fn the_container_query() {
        assert_eq!(
            read_container_query(&pairs(&[("scope", "general")])).unwrap(),
            (WardrobeContainerScope::General, None)
        );
        assert_eq!(
            read_container_query(&pairs(&[("scope", "project")])).unwrap_err(),
            "id is required for this scope"
        );
        assert_eq!(read_container_query(&pairs(&[])).unwrap_err(), SCOPE_ISSUE);
        assert_eq!(
            read_container_query(&pairs(&[("scope", "planet"), ("id", "")])).unwrap_err(),
            format!("{SCOPE_ISSUE}; Too small: expected string to have >=1 characters")
        );
        assert_eq!(
            read_container_query(&pairs(&[("scope", "project"), ("id", "")])).unwrap_err(),
            "Too small: expected string to have >=1 characters; id is required for this scope"
        );
        assert_eq!(
            read_container_query(&pairs(&[("scope", "general"), ("id", "")])).unwrap_err(),
            "Too small: expected string to have >=1 characters"
        );
    }

    /// The 502 is restored for `generate` ONLY, from core's `Internal` +
    /// `refused: false`; a refusal is 422; anything else keeps its kind.
    #[test]
    fn generate_restores_v4s_502() {
        let mut failed = CoreResponse::error(ErrorKind::Internal, "Image generation failed: x");
        if let CoreResponse::Error(e) = &mut failed {
            e.details = Some(Box::new(json!({ "trail": null, "refused": false })));
        }
        let (status, body) = render(true, failed.clone(), StatusCode::CREATED);
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body,
            json!({ "error": "Image generation failed: x", "details": { "trail": null, "refused": false } })
        );
        assert_eq!(
            render(false, failed, StatusCode::OK).0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        let plain = CoreResponse::error(ErrorKind::Internal, "Internal server error");
        assert_eq!(
            render(true, plain, StatusCode::CREATED).0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
