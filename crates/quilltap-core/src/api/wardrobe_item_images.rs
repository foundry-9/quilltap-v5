//! The wardrobe item-images route — v4 `app/api/v1/wardrobe/[itemId]/images/
//! route.ts` (`7c8572869`, #82): ONE surface for every tier, the container
//! riding in the query. `list` (GET) and the three dispatchable POST actions —
//! `generate`, `set-current`, `delete-image`; `upload` is a BINARY
//! `quilltap-web` route with no dispatch verb (it calls [`upload`] directly).
//!
//! P4.D255 landed this module with the contract-C1 §11 signatures and refusal
//! bodies; P4.D263 replaces them. **C1 AMENDMENT (the human, 2026-10-08):**
//! `generate` takes the engine's images-generate seams (its arm moved from
//! `ready_db()` to the seams accessor), and `list` / `set_current` / `delete`
//! are `async` (they write — v4's container resolve ensures a project's or
//! group's store, and the two actions patch the item), their arms `.await`ing.
//!
//! Every verb answers v4's order: the container query (400 — `id is required
//! for this scope`), the item's home (404 `Wardrobe item not found`), then the
//! action. A write against an ARCHIVED character's item answers 409 with v4's
//! INFO line; a picture that is not the item's own answers 400. The
//! `[Wardrobe Images v1]` lines run on the caller's thread (after the write
//! returns); the module's lines (`[WardrobeImages]`, `[WardrobeImageBridge]`)
//! are the service's.

use serde_json::{json, Map, Value};

use super::types::{ErrorKind, Response, WardrobeContainerScope};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::wardrobe_container::{
    resolve_wardrobe_item_home, scope_str, WardrobeItemHome,
};
use crate::services::wardrobe_image_bridge::WardrobeImageKind;
use crate::services::wardrobe_item_image_generation::{
    generate_wardrobe_item_image, GenerateWardrobeItemImageArgs, WardrobeImageGenerationFailure,
    WardrobeItemImageSeams, NO_WARDROBE_IMAGE_PROFILE_MESSAGE,
};
use crate::services::wardrobe_item_images::service::{
    add_wardrobe_item_image, delete_wardrobe_item_image, list_wardrobe_item_images,
    set_current_wardrobe_item_image, to_wardrobe_image_summary, AddWardrobeItemImageInput,
    ImageMint, ItemImageError,
};

const LOG_TAG: &str = "[Wardrobe Images v1]";

/// Zod 4's `min(1)` sentence for an EMPTY `id` (shared with the web edge's
/// scope-failure arm).
pub const CONTAINER_ID_EMPTY_ISSUE: &str = "Too small: expected string to have >=1 characters";

/// v4 `containerQuerySchema`'s two checks the typed verb can still fail:
/// `id: z.string().min(1).optional()` and the refine `scope === 'general' ||
/// !!id`. MEASURED (the routes oracle): Zod 4 still runs the refine after the
/// non-fatal `min(1)` issue, so an EMPTY id on a non-General scope answers
/// both sentences, joined `'; '`.
pub fn container_query_issue(
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
) -> Option<String> {
    let mut issues: Vec<&str> = Vec::new();
    if container_id == Some("") {
        issues.push(CONTAINER_ID_EMPTY_ISSUE);
    }
    if scope != WardrobeContainerScope::General && container_id.is_none_or(str::is_empty) {
        issues.push("id is required for this scope");
    }
    (!issues.is_empty()).then(|| issues.join("; "))
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

fn not_found() -> Response {
    // v4 `notFound('Wardrobe item')`.
    Response::error(ErrorKind::NotFound, "Wardrobe item not found")
}

/// v4's middleware 500 for anything a handler rethrows.
fn internal() -> Response {
    Response::error(ErrorKind::Internal, "Internal server error")
}

fn mount_conn(ws: &crate::db::runtime::WriterSet) -> Result<&rusqlite::Connection, DbError> {
    Ok(ws
        .mount_index()
        .ok_or(DbError::PartitionUnavailable(
            crate::write_partition::WriteDbTarget::MountIndex,
        ))?
        .connection())
}

/// The route's `findHome`: the item's home in the named container, or
/// `None` (→ 404). Runs on the writer: a project/group resolve ensures the
/// store.
async fn find_home(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Result<Option<WardrobeItemHome>, DbError> {
    let (cid, iid) = (container_id.map(str::to_string), item_id.to_string());
    db.write(move |ws| {
        resolve_wardrobe_item_home(
            ws.main().connection(),
            mount_conn(ws)?,
            super::engine::SINGLE_USER_ID,
            scope,
            cid.as_deref(),
            &iid,
        )
    })
    .await
}

/// v4's `findHome` — the query check (400) then the item's home (404), as
/// ONE gate. `Err` is the response. A transport that must parse a body AFTER
/// the gate (the web route — v4 parses `req.text()` / `req.json()` /
/// `req.formData()` only once the home is found) calls this, then the action's
/// `…_on_home` form, so the container is resolved exactly once.
pub async fn resolve_home(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Result<WardrobeItemHome, Response> {
    if let Some(issue) = container_query_issue(scope, container_id) {
        return Err(bad_request(issue));
    }
    match find_home(db, scope, container_id, item_id).await {
        Ok(Some(home)) => Ok(home),
        Ok(None) => Err(not_found()),
        Err(e) => Err(super::types::db_error_response(e)),
    }
}

/// The write actions' log meta (v4 builds one object per action).
struct WriteMeta<'a> {
    item_id: &'a str,
    scope: WardrobeContainerScope,
    /// `generate` adds `containerId`; `upload` adds `kind`.
    extra: MetaExtra<'a>,
}

enum MetaExtra<'a> {
    None,
    ContainerId(Option<&'a str>),
    Kind(&'a str),
}

impl WriteMeta<'_> {
    fn log_archived(&self) {
        let message = format!("{LOG_TAG} Refused a picture write on an archived character");
        match self.extra {
            MetaExtra::None => tracing::info!(
                itemId = %self.item_id,
                scope = scope_str(self.scope),
                "{}",
                message
            ),
            MetaExtra::ContainerId(c) => tracing::info!(
                itemId = %self.item_id,
                scope = scope_str(self.scope),
                containerId = c.unwrap_or("null"),
                "{}",
                message
            ),
            MetaExtra::Kind(k) => tracing::info!(
                itemId = %self.item_id,
                scope = scope_str(self.scope),
                kind = k,
                "{}",
                message
            ),
        }
    }
}

/// v4 `mapWriteError(error, meta)` — the archived 409 (with its INFO), the
/// foreign 400, else the rethrow (→ 500).
fn map_write_error(e: ItemImageError, meta: &WriteMeta<'_>) -> Response {
    match e {
        ItemImageError::Archived { .. } => {
            meta.log_archived();
            Response::error(
                ErrorKind::Conflict,
                "This character is archived; their wardrobe cannot be changed",
            )
        }
        ItemImageError::Foreign { .. } => bad_request(NOT_THE_ITEMS_PICTURE),
        ItemImageError::Failed(_) => internal(),
    }
}

/// `GET …/images?scope=&id=` → `{ current, images }` — the item's pictures,
/// newest first; `current` is the item's pointer ONLY when it names a listed
/// picture (a dangling pointer reads `null`).
pub async fn list(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Response {
    match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => list_on_home(db, scope, &home).await,
        Err(r) => r,
    }
}

/// [`list`] past the gate.
pub async fn list_on_home(
    db: &Db,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
) -> Response {
    let item_id = home.item_id();
    let images = match db.read_main(|c| list_wardrobe_item_images(c, item_id)) {
        Ok(images) => images,
        Err(_) => return internal(),
    };
    let current = home
        .image_file_id()
        .filter(|id| images.iter().any(|f| f.id == *id));
    tracing::debug!(
        itemId = %item_id,
        scope = scope_str(scope),
        count = images.len(),
        current = current.unwrap_or("null"),
        "{LOG_TAG} Listed wardrobe item images"
    );
    Response::WardrobeItemImages(json!({
        "current": current,
        "images": images.iter().map(to_wardrobe_image_summary).collect::<Vec<_>>(),
    }))
}

/// `POST …/images?action=generate` → 201 `{ image, current, prompt, subject,
/// profile, rerouted, trail }` (the transport supplies the 201). Inside v4's
/// `trackActivity('image', …)`. Errors: no usable profile → 400 (the ONE
/// sentence); a refusal → 422 `The image provider declined to draw this
/// garment` with `details: { trail, refused }`; any other provider failure →
/// v4's 502 `Image generation failed: <message>` with the same `details`,
/// carried as [`ErrorKind::Internal`] (no 502 kind exists on the frozen
/// boundary — the `chat_media` precedent; the web route restores the 502).
pub async fn generate(
    db: &Db,
    seams: &super::images::ImagesGenerateSeams,
    user_id: &str,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
    image_profile_id: Option<&str>,
) -> Response {
    match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => {
            generate_on_home(
                db,
                &WardrobeItemImageSeams::from_images_generate(seams),
                user_id,
                scope,
                container_id,
                &home,
                image_profile_id,
            )
            .await
        }
        Err(r) => r,
    }
}

/// [`generate`] past the gate, over the wardrobe seams directly.
pub async fn generate_on_home(
    db: &Db,
    seams: &WardrobeItemImageSeams,
    user_id: &str,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    home: &WardrobeItemHome,
    image_profile_id: Option<&str>,
) -> Response {
    let result = crate::services::activity_registry::track_activity(
        crate::services::activity_kinds::ActivityKind::Image,
        generate_wardrobe_item_image(
            db,
            seams,
            &GenerateWardrobeItemImageArgs {
                user_id,
                home,
                container_id,
                image_profile_id,
            },
        ),
    )
    .await;
    let trail_value = |t: &Option<Vec<crate::services::route_trail::RouteAttempt>>| {
        t.as_ref()
            .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
            .unwrap_or(Value::Null)
    };
    match result {
        Ok(r) => {
            let mut body = Map::new();
            body.insert("image".into(), to_wardrobe_image_summary(&r.file));
            body.insert("current".into(), json!(r.file_id));
            body.insert("prompt".into(), json!(r.prompt));
            body.insert("subject".into(), json!(r.subject.as_str()));
            body.insert(
                "profile".into(),
                json!({ "id": r.profile_id, "name": r.profile_name }),
            );
            body.insert("rerouted".into(), json!(r.rerouted));
            body.insert("trail".into(), trail_value(&r.trail));
            Response::WardrobeItemImages(Value::Object(body))
        }
        Err(WardrobeImageGenerationFailure::NoProfile) => {
            bad_request(NO_WARDROBE_IMAGE_PROFILE_MESSAGE)
        }
        Err(WardrobeImageGenerationFailure::Generation {
            message,
            trail,
            refused,
        }) => {
            let (kind, text) = if refused {
                (
                    ErrorKind::Unprocessable,
                    "The image provider declined to draw this garment".to_string(),
                )
            } else {
                (
                    ErrorKind::Internal,
                    format!("Image generation failed: {message}"),
                )
            };
            let mut response = Response::error(kind, text);
            if let Response::Error(e) = &mut response {
                e.details = Some(Box::new(json!({
                    "trail": trail_value(&trail),
                    "refused": refused,
                })));
            }
            response
        }
        Err(WardrobeImageGenerationFailure::Item(e)) => map_write_error(
            e,
            &WriteMeta {
                item_id: home.item_id(),
                scope,
                extra: MetaExtra::ContainerId(container_id),
            },
        ),
    }
}

/// `POST …/images?action=upload` past the gate — the binary leg, called by the
/// web route after its multipart checks (`Expected a multipart upload`, `No
/// file provided`, `validateImageFile`, the `kind` 400) and the WebP
/// conversion. → `{ image, current }` (the transport supplies the 201). There
/// is no dispatch verb for upload (contract C2 §6).
#[allow(clippy::too_many_arguments)]
pub async fn upload_on_home(
    db: &Db,
    blob_webp: std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
    kind: WardrobeImageKind,
    content: Vec<u8>,
    content_type: String,
    width: Option<i64>,
    height: Option<i64>,
) -> Response {
    let mint = ImageMint::now();
    let h = home.clone();
    let written = db
        .write(move |ws| {
            let main = ws.main().connection();
            let mount = mount_conn(ws)?;
            Ok(add_wardrobe_item_image(
                main,
                mount,
                &h,
                &AddWardrobeItemImageInput {
                    user_id: super::engine::SINGLE_USER_ID,
                    kind,
                    content: &content,
                    content_type: &content_type,
                    width,
                    height,
                    generation_prompt: None,
                    generation_model: None,
                    generation_revised_prompt: None,
                },
                &mint,
                blob_webp.as_ref(),
            ))
        })
        .await;
    let item_id = home.item_id();
    let meta = WriteMeta {
        item_id,
        scope,
        extra: MetaExtra::Kind(kind.as_str()),
    };
    match written {
        Ok(Ok(added)) => {
            tracing::info!(
                itemId = %item_id,
                scope = scope_str(scope),
                kind = kind.as_str(),
                fileId = %added.file.id,
                bytes = added.file.size,
                "{LOG_TAG} Uploaded wardrobe item image"
            );
            Response::WardrobeItemImages(json!({
                "image": to_wardrobe_image_summary(&added.file),
                "current": added.file.id,
            }))
        }
        Ok(Err(e)) => map_write_error(e, &meta),
        Err(e) => map_write_error(e.into(), &meta),
    }
}

/// The `fileIdBodySchema` check the typed verb can still fail: an EMPTY id
/// (`z.string().min(1, 'fileId is required')`).
fn file_id_issue(file_id: &str) -> Option<Response> {
    file_id
        .is_empty()
        .then(|| bad_request("fileId is required"))
}

/// `POST …/images?action=set-current` → `{ current }`.
pub async fn set_current(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
    file_id: &str,
) -> Response {
    match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => set_current_on_home(db, scope, &home, file_id).await,
        Err(r) => r,
    }
}

/// [`set_current`] past the gate.
pub async fn set_current_on_home(
    db: &Db,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
    file_id: &str,
) -> Response {
    if let Some(r) = file_id_issue(file_id) {
        return r;
    }
    let (h, fid) = (home.clone(), file_id.to_string());
    let result = db
        .write(move |ws| {
            Ok(set_current_wardrobe_item_image(
                ws.main().connection(),
                mount_conn(ws)?,
                &h,
                &fid,
            ))
        })
        .await;
    let item_id = home.item_id();
    let meta = WriteMeta {
        item_id,
        scope,
        extra: MetaExtra::None,
    };
    match result {
        Ok(Ok(current)) => {
            tracing::info!(
                itemId = %item_id,
                scope = scope_str(scope),
                fileId = %file_id,
                "{LOG_TAG} Set current wardrobe item image"
            );
            Response::WardrobeItemImages(json!({ "current": current }))
        }
        Ok(Err(e)) => map_write_error(e, &meta),
        Err(e) => map_write_error(e.into(), &meta),
    }
}

/// `POST …/images?action=delete-image` → `{ current }` (the next-newest
/// becomes current when the deleted one was).
pub async fn delete(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
    file_id: &str,
) -> Response {
    match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => delete_on_home(db, scope, &home, file_id).await,
        Err(r) => r,
    }
}

/// [`delete`] past the gate.
pub async fn delete_on_home(
    db: &Db,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
    file_id: &str,
) -> Response {
    if let Some(r) = file_id_issue(file_id) {
        return r;
    }
    let (h, fid) = (home.clone(), file_id.to_string());
    let result = db
        .write(move |ws| {
            Ok(delete_wardrobe_item_image(
                ws.main().connection(),
                mount_conn(ws)?,
                &h,
                &fid,
            ))
        })
        .await;
    let item_id = home.item_id();
    let meta = WriteMeta {
        item_id,
        scope,
        extra: MetaExtra::None,
    };
    match result {
        Ok(Ok(current)) => {
            tracing::info!(
                itemId = %item_id,
                scope = scope_str(scope),
                fileId = %file_id,
                current = current.as_deref().unwrap_or("null"),
                "{LOG_TAG} Deleted wardrobe item image"
            );
            Response::WardrobeItemImages(json!({ "current": current }))
        }
        Ok(Err(e)) => map_write_error(e, &meta),
        Err(e) => map_write_error(e.into(), &meta),
    }
}

// ===========================================================================
// === P4.D275 === the picture viewer's save to a document store (v4 `01a83539d`)
// ===========================================================================

/// The foreign-picture refusal — one sentence for the write actions'
/// `mapWriteError` arm and the save's own guard (v4 `route.ts:132` / `:312`).
const NOT_THE_ITEMS_PICTURE: &str = "That picture does not belong to this wardrobe item";

/// `GET …/images?scope=&id=&action=save-targets` → `{ albums }` (v4
/// `handleSaveTargets`, `route.ts:288-294`): the item's home (the 400 / 404
/// gate), then every enabled store a picture may be filed in
/// ([`list_all_photo_album_options`](crate::photos::photo_album_options::list_all_photo_album_options)).
pub async fn save_targets(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Response {
    match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => save_targets_on_home(db, scope, &home),
        Err(r) => r,
    }
}

/// [`save_targets`] past the gate — v4's DEBUG `[Wardrobe Images v1] Listed
/// save targets` `{itemId, scope, count}` on the caller's thread.
pub fn save_targets_on_home(
    db: &Db,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
) -> Response {
    let albums = crate::photos::photo_album_options::list_all_photo_album_options(db);
    tracing::debug!(
        itemId = %home.item_id(),
        scope = scope_str(scope),
        count = albums.len(),
        "{LOG_TAG} Listed save targets"
    );
    Response::WardrobeItemImages(json!({ "albums": albums }))
}

/// The dispatch verb's typed fields rebuilt as the body v4's
/// `SaveImageRequestSchema` sees (P4.D275 R-H): `caption` / `tags` arrive RAW
/// (`double_option`) — absent → the key omitted, a JSON `null` → `null` (which
/// the parser refuses, as Zod does), a value → as sent (the parser, not serde,
/// refuses a wrong type).
pub fn save_to_store_body(
    file_id: String,
    mount_point_id: String,
    caption: Option<Option<Value>>,
    tags: Option<Option<Value>>,
) -> Value {
    let mut body = Map::new();
    body.insert("fileId".into(), Value::String(file_id));
    body.insert("mountPointId".into(), Value::String(mount_point_id));
    for (key, value) in [("caption", caption), ("tags", tags)] {
        if let Some(value) = value {
            body.insert(key.into(), value.unwrap_or(Value::Null));
        }
    }
    Value::Object(body)
}

/// `POST …/images?scope=&id=&action=save-to-store` → `{ saved, mountPoint,
/// relativePath, linkId, keptAt, fileId, sha256 }` (v4 `handleSaveToStore`,
/// `route.ts:297-339`): the gate, then v4's shared `SaveImageRequestSchema`
/// over `body` (400, the joined sentences), then [`save_to_store_on_home`].
/// `kept_at` is the injected ISO clock.
#[allow(clippy::too_many_arguments)]
pub async fn save_to_store(
    db: &Db,
    bytes: std::sync::Arc<dyn crate::photos::save_image_to_album::FileBytesStore>,
    side_effects: std::sync::Arc<
        dyn crate::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
    >,
    user_id: &str,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
    body: &Value,
    kept_at: &str,
) -> Response {
    let home = match resolve_home(db, scope, container_id, item_id).await {
        Ok(home) => home,
        Err(r) => return r,
    };
    let parsed = match crate::photos::save_attribution::parse_save_image_request(body) {
        Ok(p) => p,
        Err(message) => return bad_request(message),
    };
    save_to_store_on_home(
        db,
        bytes,
        side_effects,
        user_id,
        scope,
        &home,
        parsed,
        kept_at,
    )
    .await
}

/// [`save_to_store`] past the gate and the body parse, in v4's order:
///
/// 1. the picture must be one of the item's own (`listWardrobeItemImages`'
///    `files.id`s) — else INFO `Refused to save a picture that is not the
///    item's` and the 400;
/// 2. the target must not be an ARCHIVED character's vault — else INFO
///    `Refused to save into an archived character's vault` and the 409 `That
///    store belongs to an archived character and cannot be written to` (no
///    `code`);
/// 3. [`save_image_to_album`](crate::photos::save_image_to_album::save_image_to_album)
///    with the item's title as the default caption (an EMPTY caption is kept
///    — v4's `??`), no chat (no scene snapshot), and the operator's byline
///    ([`operator_attribution`](crate::photos::save_attribution::operator_attribution));
/// 4. INFO `Saved wardrobe picture to a store` and the shared 200 body; a
///    `SaveImageToAlbumError` is INFO `Save to store rejected` and the shared
///    error response (409 `ALREADY_SAVED` with its riders, else 400); anything
///    else is v4's middleware 500.
///
/// v4's why, carried (`route.ts:24-29`): the ITEM is never written, so the
/// archived-character write guard does not apply — an archived character's
/// item may be copied OUT. Every line runs on the caller's thread.
#[allow(clippy::too_many_arguments)]
pub async fn save_to_store_on_home(
    db: &Db,
    bytes: std::sync::Arc<dyn crate::photos::save_image_to_album::FileBytesStore>,
    side_effects: std::sync::Arc<
        dyn crate::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
    >,
    user_id: &str,
    scope: WardrobeContainerScope,
    home: &WardrobeItemHome,
    parsed: crate::photos::save_attribution::SaveImageRequest,
    kept_at: &str,
) -> Response {
    use crate::photos::save_image_response::{
        save_image_error_code, save_image_error_response, saved_image_body,
    };
    let item_id = home.item_id();
    let (file_id, mount_point_id) = (parsed.file_id.as_str(), parsed.mount_point_id.as_str());

    let images = match db.read_main(|c| list_wardrobe_item_images(c, item_id)) {
        Ok(images) => images,
        Err(_) => return internal(),
    };
    if !images.iter().any(|f| f.id == file_id) {
        tracing::info!(
            itemId = %item_id,
            scope = scope_str(scope),
            fileId = %file_id,
            mountPointId = %mount_point_id,
            "{LOG_TAG} Refused to save a picture that is not the item's"
        );
        return bad_request(NOT_THE_ITEMS_PICTURE);
    }

    // v4's `getArchivedCharacterVaultMountPointIds()` — its `_findAll` read
    // answers `[]` on a failure.
    let archived = crate::db::fallback::find_all_or_empty("characters", || {
        db.read_main(
            crate::services::mount_index::document_text_search::archived_character_vault_mount_point_ids,
        )
    });
    if archived.iter().any(|id| id == mount_point_id) {
        tracing::info!(
            itemId = %item_id,
            scope = scope_str(scope),
            fileId = %file_id,
            mountPointId = %mount_point_id,
            "{LOG_TAG} Refused to save into an archived character's vault"
        );
        return Response::error(
            ErrorKind::Conflict,
            "That store belongs to an archived character and cannot be written to",
        );
    }

    let attribution = match db.read_main(|c| {
        Ok(crate::photos::save_attribution::operator_attribution(
            c, user_id,
        ))
    }) {
        Ok(a) => a,
        Err(_) => return internal(),
    };
    let caption = parsed
        .caption
        .clone()
        .unwrap_or_else(|| home.item_title().to_string());
    let tags = parsed.tags.clone();
    let (mp, fid, kept) = (
        mount_point_id.to_string(),
        file_id.to_string(),
        kept_at.to_string(),
    );
    let result = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let input = crate::photos::save_image_to_album::SaveImageToAlbumInput {
                mount_point_id: &mp,
                file_id: &fid,
                caption: Some(&caption),
                tags: &tags,
                chat_id: None,
                attribution,
            };
            Ok(crate::photos::save_image_to_album::save_image_to_album(
                ws.main().connection(),
                mount,
                &input,
                &*bytes,
                &*side_effects,
                &kept,
            ))
        })
        .await;
    match result {
        Ok(Ok(saved)) => {
            tracing::info!(
                itemId = %item_id,
                scope = scope_str(scope),
                fileId = %file_id,
                mountPointId = %mount_point_id,
                relativePath = %saved.relative_path,
                linkId = %saved.link_id,
                "{LOG_TAG} Saved wardrobe picture to a store"
            );
            Response::WardrobeItemImages(saved_image_body(&saved))
        }
        Ok(Err(err)) => {
            tracing::info!(
                itemId = %item_id,
                scope = scope_str(scope),
                fileId = %file_id,
                mountPointId = %mount_point_id,
                code = save_image_error_code(err.code),
                // v4's `message` key renamed — see `chat_media`'s
                // `[SaveImage] rejected` (the envelope clobber).
                errorMessage = %err.message,
                "{LOG_TAG} Save to store rejected"
            );
            save_image_error_response(err)
        }
        Err(_) => internal(),
    }
}
// === end P4.D275 ===

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photos::save_attribution::parse_save_image_request;

    fn parsed(
        caption: Option<Option<Value>>,
        tags: Option<Option<Value>>,
    ) -> Result<crate::photos::save_attribution::SaveImageRequest, String> {
        parse_save_image_request(&save_to_store_body("f".into(), "m".into(), caption, tags))
    }

    /// R-H over the dispatch verb's tri-states: absent → no caption (the
    /// handler then takes the item's title) and no tags; a JSON `null` → v4's
    /// Zod refusal; a wrong type → the parser's refusal; `''` kept.
    #[test]
    fn the_dispatch_body_reaches_v4s_parser() {
        let ok = parsed(None, None).unwrap();
        assert_eq!(ok.caption, None);
        assert!(ok.tags.is_empty());
        assert_eq!(
            serde_json::to_string(&save_to_store_body("f".into(), "m".into(), None, None)).unwrap(),
            r#"{"fileId":"f","mountPointId":"m"}"#
        );
        assert_eq!(
            parsed(Some(None), None).unwrap_err(),
            "Invalid input: expected string, received null"
        );
        assert_eq!(
            parsed(None, Some(None)).unwrap_err(),
            "Invalid input: expected array, received null"
        );
        assert_eq!(
            parsed(Some(None), Some(None)).unwrap_err(),
            "Invalid input: expected string, received null; Invalid input: expected array, received null"
        );
        assert_eq!(
            parsed(Some(Some(serde_json::json!(5))), None).unwrap_err(),
            "Invalid input: expected string, received number"
        );
        assert_eq!(
            parsed(None, Some(Some(serde_json::json!(["a", 1])))).unwrap_err(),
            "Invalid input: expected string, received number"
        );
        let kept = parsed(
            Some(Some(serde_json::json!(""))),
            Some(Some(serde_json::json!(["a"]))),
        )
        .unwrap();
        assert_eq!(kept.caption.as_deref(), Some(""));
        assert_eq!(kept.tags, vec!["a".to_string()]);
    }
}
