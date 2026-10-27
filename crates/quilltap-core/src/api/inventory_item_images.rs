//! The Inventory object-images route — v4 `app/api/v1/inventory/[itemId]/
//! images/route.ts` (`4fbdad186` at the `322a5488e` pin; P4.D325), the twin of
//! [`super::wardrobe_item_images`] WITHOUT `link-image`: ONE surface for every
//! tier, the container riding in the query. `list` (GET), `save_targets`
//! (`GET ?action=save-targets`) and the dispatchable POST actions —
//! `generate`, `set-current`, `delete-image`, `save-to-store`; `upload` is a
//! BINARY `quilltap-web` leg with no dispatch verb (it calls
//! [`upload_on_home`] directly).
//!
//! Every verb answers v4's order: the container query (400 — `id is required
//! for this scope`), the object's home (404 `Inventory object not found`),
//! then the body and the action. The home is a READ (v4
//! `resolveInventoryItemHome` never provisions — unlike the wardrobe's, which
//! ensures a project / group store). Refusals map through
//! [`inventory_error_response`]; an `InventoryImageGenerationError` is 422
//! (refused) or v4's 502 (carried as `Internal` + `details` — the web edge
//! restores the 502). The `[Inventory Images v1]` lines run on the caller's
//! thread after the write returns.

use serde_json::{json, Map, Value};

use super::inventory_route_factory::{
    escaped, inventory_error_response, not_found, validation_error,
};
use super::types::{ErrorKind, InventoryContainer, InventoryScope, Response};
use super::wardrobe_item_images::CONTAINER_ID_EMPTY_ISSUE;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::inventory::item_image_generation::{
    generate_inventory_item_image, GenerateInventoryItemImageParams,
    InventoryImageGenerationFailure, NO_INVENTORY_IMAGE_PROFILE_MESSAGE,
};
use crate::inventory::item_images::{
    add_inventory_item_image, delete_inventory_item_image, list_inventory_images_with_fallthrough,
    list_inventory_item_images, resolve_inventory_item_home, set_current_inventory_item_image,
    AddInventoryItemImageInput, InventoryItemHome,
};
use crate::inventory::types::InventoryFields;
use crate::services::wardrobe_image_bridge::WardrobeImageKind;
use crate::services::wardrobe_item_image_generation::WardrobeItemImageSeams;
use crate::services::wardrobe_item_images::service::{to_wardrobe_image_summary, ImageMint};

const LOG_TAG: &str = "[Inventory Images v1]";

/// v4 `containerQuerySchema`'s two id checks the typed verb can still fail:
/// `id: z.string().min(1).optional()` and the refine `scope === 'general' ||
/// !!id`. Zod 4 runs the refine after the non-fatal `min(1)` issue, so an
/// EMPTY id on a non-General scope answers both sentences, joined `'; '` (the
/// wardrobe twin's measurement — one shared `min(1)` sentence).
pub fn container_query_issue(scope: InventoryScope, container_id: Option<&str>) -> Option<String> {
    let mut issues: Vec<&str> = Vec::new();
    if container_id == Some("") {
        issues.push(CONTAINER_ID_EMPTY_ISSUE);
    }
    if scope != InventoryScope::General && container_id.is_none_or(str::is_empty) {
        issues.push("id is required for this scope");
    }
    (!issues.is_empty()).then(|| issues.join("; "))
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

fn mount_conn(ws: &crate::db::runtime::WriterSet) -> Result<&rusqlite::Connection, DbError> {
    Ok(ws
        .mount_index()
        .ok_or(DbError::PartitionUnavailable(
            crate::write_partition::WriteDbTarget::MountIndex,
        ))?
        .connection())
}

/// v4's `findHome` — the query check (400) then the object's home (404), as
/// ONE gate. `Err` is the response. A transport that must parse a body AFTER
/// the gate (the web route) calls this, then the action's `…_on_home` form.
pub async fn resolve_home(
    db: &Db,
    container: &InventoryContainer,
    item_id: &str,
) -> Result<InventoryItemHome, Response> {
    if let Some(issue) = container_query_issue(container.scope, container.id.as_deref()) {
        return Err(bad_request(issue));
    }
    let scope = container.scope.tier();
    let cid = container.id.clone();
    let found = db.read_main(|main| {
        db.read_mount_index(|mount| {
            resolve_inventory_item_home(
                main,
                mount,
                super::engine::SINGLE_USER_ID,
                scope,
                cid.as_deref(),
                item_id,
            )
        })
    });
    match found {
        Ok(Some(home)) => Ok(home),
        Ok(None) => Err(not_found("Inventory object")),
        Err(e) => Err(escaped(e)),
    }
}

/// `GET …/images?scope=&id=` → `{ current, images }`.
pub async fn list(db: &Db, container: &InventoryContainer, item_id: &str) -> Response {
    match resolve_home(db, container, item_id).await {
        Ok(home) => list_on_home(db, &home),
        Err(r) => r,
    }
}

/// [`list`] past the gate — an instance with no pictures of its own lists its
/// archetype's, marked `inherited`; `current` is the object's pointer ONLY
/// when it names a listed picture.
pub fn list_on_home(db: &Db, home: &InventoryItemHome) -> Response {
    let images = match db.read_main(|c| list_inventory_images_with_fallthrough(c, &home.item)) {
        Ok(images) => images,
        Err(e) => return escaped(e),
    };
    let current = home.item.image_file_id().filter(|wanted| {
        images
            .iter()
            .any(|image| image.get("fileId").and_then(Value::as_str) == Some(*wanted))
    });
    tracing::debug!(
        itemId = %home.record.id(),
        count = images.len(),
        current = current.unwrap_or("null"),
        context = "inventory",
        "{LOG_TAG} Listed object images"
    );
    Response::Inventory(json!({ "current": current, "images": images }))
}

/// v4 `generateBodySchema` — `{ imageProfileId: z.string().min(1).nullable()
/// .optional() }` (an object; unknown keys stripped); the issues joined `'; '`.
pub fn parse_generate_body(v: &Value) -> Result<Option<String>, String> {
    let Some(obj) = v.as_object() else {
        return Err(format!(
            "Invalid input: expected object, received {}",
            super::zod_issues::zod_parsed_type(Some(v))
        ));
    };
    match obj.get("imageProfileId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => {
            Err("Too small: expected string to have >=1 characters".to_string())
        }
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!(
            "Invalid input: expected string, received {}",
            super::zod_issues::zod_parsed_type(Some(other))
        )),
    }
}

/// v4 `fileIdBodySchema` — `{ fileId: z.string().min(1, 'fileId is required') }`,
/// parsed with `.parse` INSIDE the action's try: the ZodError reaches
/// `inventoryErrorResponse`, which rethrows it to the middleware's
/// `Validation error` 400 (issue array and all).
pub fn parse_file_id_body(v: &Value) -> Result<String, Response> {
    use super::zod_issues::ZodIssue;
    let path = || vec![json!("fileId")];
    let issue = match v.as_object() {
        None => ZodIssue::invalid_type("object", vec![], Some(v)),
        Some(obj) => match obj.get("fileId") {
            Some(Value::String(s)) if !s.is_empty() => return Ok(s.clone()),
            Some(Value::String(_)) => {
                ZodIssue::too_small_string_message(json!(1), path(), "fileId is required")
            }
            other => ZodIssue::invalid_type("string", path(), other),
        },
    };
    Err(validation_error(&[issue]))
}

/// `POST …/images?action=generate` → 201 `{ image, current, prompt, profile,
/// rerouted, trail }` (the transport supplies the 201). 💸 one provider call.
pub async fn generate(
    db: &Db,
    seams: &super::images::ImagesGenerateSeams,
    user_id: &str,
    container: &InventoryContainer,
    item_id: &str,
    image_profile_id: Option<&str>,
) -> Response {
    let home = match resolve_home(db, container, item_id).await {
        Ok(home) => home,
        Err(r) => return r,
    };
    // The typed field skips v4's body parse; its one remaining check is the
    // `min(1)` on an EMPTY id.
    if image_profile_id == Some("") {
        return bad_request("Too small: expected string to have >=1 characters");
    }
    generate_on_home(
        db,
        &WardrobeItemImageSeams::from_images_generate(seams),
        user_id,
        &home,
        image_profile_id,
    )
    .await
}

/// [`generate`] past the gate and the body parse, inside v4's
/// `trackActivity('image', …)`. No usable profile → 400 (the ONE sentence); a
/// refusal → 422 `The image provider declined to draw this object` with
/// `details: { trail, refused }`; any other provider failure → v4's 502
/// `Image generation failed: <message>` with the same details, carried as
/// [`ErrorKind::Internal`] (no 502 kind on the boundary — the web edge
/// restores it); else [`inventory_error_response`].
pub async fn generate_on_home(
    db: &Db,
    seams: &WardrobeItemImageSeams,
    user_id: &str,
    home: &InventoryItemHome,
    image_profile_id: Option<&str>,
) -> Response {
    let result = crate::services::activity_registry::track_activity(
        crate::services::activity_kinds::ActivityKind::Image,
        generate_inventory_item_image(
            db,
            seams,
            GenerateInventoryItemImageParams {
                user_id,
                home,
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
            // `const file = await ctx.repos.files.findById(result.fileId)` —
            // the new picture is linked to the object, so it is among its rows.
            let file = match db.read_main(|c| list_inventory_item_images(c, home.record.id())) {
                Ok(rows) => rows.into_iter().find(|f| f.id == r.file_id),
                Err(e) => return escaped(e),
            };
            let mut body = Map::new();
            body.insert(
                "image".into(),
                match &file {
                    Some(f) => to_wardrobe_image_summary(f),
                    None => json!({ "fileId": r.file_id }),
                },
            );
            body.insert("current".into(), json!(r.file_id));
            body.insert("prompt".into(), json!(r.prompt));
            body.insert(
                "profile".into(),
                json!({ "id": r.profile_id, "name": r.profile_name }),
            );
            body.insert("rerouted".into(), json!(r.rerouted));
            body.insert("trail".into(), trail_value(&r.trail));
            Response::Inventory(Value::Object(body))
        }
        Err(InventoryImageGenerationFailure::NoProfile) => {
            bad_request(NO_INVENTORY_IMAGE_PROFILE_MESSAGE)
        }
        Err(InventoryImageGenerationFailure::Generation {
            message,
            trail,
            refused,
        }) => {
            let (kind, text) = if refused {
                (
                    ErrorKind::Unprocessable,
                    "The image provider declined to draw this object".to_string(),
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
        Err(InventoryImageGenerationFailure::Item(e)) => inventory_error_response(e),
    }
}

/// `POST …/images?action=upload` past the gate — the binary leg, called by the
/// web route after its multipart checks and the WebP conversion. → `{ image,
/// current }` (the transport supplies the 201). No dispatch verb (C7).
#[allow(clippy::too_many_arguments)]
pub async fn upload_on_home(
    db: &Db,
    blob_webp: std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
    home: &InventoryItemHome,
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
            Ok(add_inventory_item_image(
                main,
                mount,
                &h,
                &AddInventoryItemImageInput {
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
    match written {
        Ok(Ok(added)) => {
            tracing::info!(
                itemId = %home.record.id(),
                fileId = %added.file.id,
                context = "inventory",
                "{LOG_TAG} Uploaded object image"
            );
            Response::Inventory(json!({
                "image": to_wardrobe_image_summary(&added.file),
                "current": added.file.id,
            }))
        }
        Ok(Err(e)) => inventory_error_response(e),
        Err(e) => escaped(e),
    }
}

/// `POST …/images?action=set-current` `{ fileId }` → `{ current }`.
pub async fn set_current(
    db: &Db,
    container: &InventoryContainer,
    item_id: &str,
    body: &Value,
) -> Response {
    match resolve_home(db, container, item_id).await {
        Ok(home) => set_current_on_home(db, &home, body).await,
        Err(r) => r,
    }
}

/// [`set_current`] past the gate (the body parse is the action's).
pub async fn set_current_on_home(db: &Db, home: &InventoryItemHome, body: &Value) -> Response {
    let file_id = match parse_file_id_body(body) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let h = home.clone();
    let result = db
        .write(move |ws| {
            Ok(set_current_inventory_item_image(
                ws.main().connection(),
                mount_conn(ws)?,
                &h,
                &file_id,
            ))
        })
        .await;
    match result {
        Ok(Ok(current)) => Response::Inventory(json!({ "current": current })),
        Ok(Err(e)) => inventory_error_response(e),
        Err(e) => escaped(e),
    }
}

/// `POST …/images?action=delete-image` `{ fileId }` → `{ current }`.
pub async fn delete(
    db: &Db,
    container: &InventoryContainer,
    item_id: &str,
    body: &Value,
) -> Response {
    match resolve_home(db, container, item_id).await {
        Ok(home) => delete_on_home(db, &home, body).await,
        Err(r) => r,
    }
}

/// [`delete`] past the gate.
pub async fn delete_on_home(db: &Db, home: &InventoryItemHome, body: &Value) -> Response {
    let file_id = match parse_file_id_body(body) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let h = home.clone();
    let result = db
        .write(move |ws| {
            Ok(delete_inventory_item_image(
                ws.main().connection(),
                mount_conn(ws)?,
                &h,
                &file_id,
            ))
        })
        .await;
    match result {
        Ok(Ok(current)) => Response::Inventory(json!({ "current": current })),
        Ok(Err(e)) => inventory_error_response(e),
        Err(e) => escaped(e),
    }
}

/// `GET …/images?action=save-targets` → `{ albums }` (no route line).
pub async fn save_targets(db: &Db, container: &InventoryContainer, item_id: &str) -> Response {
    match resolve_home(db, container, item_id).await {
        Ok(_) => save_targets_on_home(db),
        Err(r) => r,
    }
}

/// [`save_targets`] past the gate.
pub fn save_targets_on_home(db: &Db) -> Response {
    let albums = crate::photos::photo_album_options::list_all_photo_album_options(db);
    Response::Inventory(json!({ "albums": albums }))
}

/// `POST …/images?action=save-to-store` → v4's `savedImageResponse` (v4
/// `handleSaveToStore`): the gate, then the shared `SaveImageRequestSchema`
/// over `body` (400, the joined sentences), then [`save_to_store_on_home`].
#[allow(clippy::too_many_arguments)]
pub async fn save_to_store(
    db: &Db,
    bytes: std::sync::Arc<dyn crate::photos::save_image_to_album::FileBytesStore>,
    side_effects: std::sync::Arc<
        dyn crate::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
    >,
    user_id: &str,
    container: &InventoryContainer,
    item_id: &str,
    body: &Value,
    kept_at: &str,
) -> Response {
    let home = match resolve_home(db, container, item_id).await {
        Ok(home) => home,
        Err(r) => return r,
    };
    let parsed = match crate::photos::save_attribution::parse_save_image_request(body) {
        Ok(p) => p,
        Err(message) => return bad_request(message),
    };
    save_to_store_on_home(db, bytes, side_effects, user_id, &home, parsed, kept_at).await
}

/// [`save_to_store`] past the gate and the body parse, in v4's order: only the
/// object's own pictures (or, for an instance with none, the archetype's it
/// shows) leave through this door — else 400 `That picture does not belong to
/// this object`; an ARCHIVED character's vault is the 409; then
/// `saveImageToAlbum` with `caption ?? (title || name)`, `tags ?? []` and the
/// operator's byline → INFO → the shared 200 body; a `SaveImageToAlbumError`
/// is the shared error response (409 `ALREADY_SAVED` with its riders, else
/// 400); anything else v4's middleware 500. (v4's Inventory handler logs none
/// of the wardrobe twin's refusal lines.)
pub async fn save_to_store_on_home(
    db: &Db,
    bytes: std::sync::Arc<dyn crate::photos::save_image_to_album::FileBytesStore>,
    side_effects: std::sync::Arc<
        dyn crate::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
    >,
    user_id: &str,
    home: &InventoryItemHome,
    parsed: crate::photos::save_attribution::SaveImageRequest,
    kept_at: &str,
) -> Response {
    use crate::photos::save_image_response::{save_image_error_response, saved_image_body};
    let item_id = home.record.id().to_string();
    let (file_id, mount_point_id) = (parsed.file_id.clone(), parsed.mount_point_id.clone());

    let allowed = match db.read_main(|c| list_inventory_images_with_fallthrough(c, &home.item)) {
        Ok(images) => images,
        Err(e) => return escaped(e),
    };
    if !allowed
        .iter()
        .any(|image| image.get("fileId").and_then(Value::as_str) == Some(file_id.as_str()))
    {
        return bad_request("That picture does not belong to this object");
    }
    // v4's `getArchivedCharacterVaultMountPointIds()` — its `_findAll` read
    // answers `[]` on a failure.
    let archived = crate::db::fallback::find_all_or_empty("characters", || {
        db.read_main(
            crate::services::mount_index::document_text_search::archived_character_vault_mount_point_ids,
        )
    });
    if archived.contains(&mount_point_id) {
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
        // v4 rethrows; the context middleware answers a broken store's 503.
        Err(e) => return escaped(e),
    };
    // `caption ?? (title || name)`.
    let caption = parsed.caption.clone().unwrap_or_else(|| {
        home.item
            .title()
            .filter(|t| !t.is_empty())
            .unwrap_or(home.item.name())
            .to_string()
    });
    let tags = parsed.tags.clone();
    let (mp, fid, kept) = (mount_point_id.clone(), file_id.clone(), kept_at.to_string());
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
                fileId = %file_id,
                mountPointId = %mount_point_id,
                context = "inventory",
                "{LOG_TAG} Saved object picture to a store"
            );
            Response::Inventory(saved_image_body(&saved))
        }
        Ok(Err(err)) => save_image_error_response(err),
        // v4 rethrows a non-`SaveImageToAlbumError`: a broken store is the
        // middleware's 503, anything else the generic 500 (`escaped`).
        Err(e) => escaped(e),
    }
}
