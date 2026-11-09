//! The item images route, stated once for every kind of tiered item — the
//! wardrobe's garments and the Inventory's objects (v4
//! `lib/mount-index/item-images-route.ts`, `600c2906f`; P4.D377 item 11, the
//! survey's R-C). `api::wardrobe_item_images` and `api::inventory_item_images`
//! are the two KIND ADAPTERS over it (v4's `app/api/v1/wardrobe/[itemId]/images/
//! route.ts` and `…/inventory/[itemId]/images/route.ts`).
//!
//! **Shape (recorded, the order's item 11):** a struct of the kind's constants
//! ([`ItemImagesKind`] — `logTag`, `context` and v4's four `messages`) plus ONE
//! emitter per route line and the shared refusals, in v4's texts and `metaOf`
//! key order (`itemId`, `scope`, the action's own keys, `context`, then any key
//! a handler spreads AFTER the meta). The handlers' bodies stay in the adapters,
//! because the two kinds' homes, write services and wire bodies are different
//! Rust types (v4's generic `H` over one JS object shape); everything v4's
//! factory OWNS — the container-query sentences, the item-not-found 404 and its
//! DEBUG, the generate body's JSON / Zod arms, the generation-failure answer,
//! the save-to-store guards and every line text — lives here once. Wire bodies
//! hold (the four images families' body rows are the claim).
//!
//! What moved at the pin (`600c2906f`; pre-span v4 = v5 before this port): the
//! wardrobe's `Listed wardrobe item images` / `Uploaded wardrobe item image` /
//! `Set current wardrobe item image` / `Deleted wardrobe item image` / `Saved
//! wardrobe picture to a store` and the Inventory's `Listed object images` /
//! `Uploaded object image` / `Saved object picture to a store` all become the
//! kind-neutral `… item image(s)` / `Saved item picture to a store`; `scope`
//! joins EVERY meta and `context` closes it; and both kinds now log what only
//! one did — `Item not in the named container`, `Generate body is not valid
//! JSON`, `Generated item image`, `Generation failed`, and for the Inventory
//! `Set current item image`, `Deleted item image`, `Listed save targets`, the
//! two save refusals and `Save to store rejected`.
//!
//! Every line runs on the caller's thread after the read / write returns.

use serde_json::Value;

use super::types::{ErrorKind, Response};

/// v4 `ItemImagesKind`'s constants: the log prefix, the log `context`, and the
/// four kind-worded messages.
#[derive(Debug, Clone, Copy)]
pub struct ItemImagesKind {
    /// Log-message prefix, e.g. `[Wardrobe Images v1]`.
    pub log_tag: &'static str,
    /// Log `context`, e.g. `wardrobe`.
    pub context: &'static str,
    /// The 404 for an item not in the named container, e.g. `Wardrobe item`.
    pub item_not_found: &'static str,
    /// The 400 for a save-to-store of a picture the item does not show.
    pub foreign_picture: &'static str,
    /// The 400 for a generate body that is not JSON.
    pub invalid_json: &'static str,
    /// What the provider declined to draw, e.g. `this garment`.
    pub refused_subject: &'static str,
}

/// The wardrobe adapter's constants (v4 `app/api/v1/wardrobe/[itemId]/images/
/// route.ts`).
pub const WARDROBE_IMAGES: ItemImagesKind = ItemImagesKind {
    log_tag: "[Wardrobe Images v1]",
    context: "wardrobe",
    item_not_found: "Wardrobe item",
    foreign_picture: "That picture does not belong to this wardrobe item",
    invalid_json: "Invalid JSON body",
    refused_subject: "this garment",
};

/// The Inventory adapter's constants (v4 `app/api/v1/inventory/[itemId]/images/
/// route.ts`).
pub const INVENTORY_IMAGES: ItemImagesKind = ItemImagesKind {
    log_tag: "[Inventory Images v1]",
    context: "inventory",
    item_not_found: "Inventory object",
    foreign_picture: "That picture does not belong to this object",
    invalid_json: "Request body is not valid JSON",
    refused_subject: "this object",
};

/// Zod 4's sentence for a scope outside v4 `ItemImagesScopeSchema` —
/// `z.enum(['character', 'project', 'group', 'general'])`, ONE enum for both
/// kinds at the pin (`600c2906f`; the Inventory's route had read
/// `InventoryScopeSchema`'s `character, group, project, general` before).
pub const SCOPE_ISSUE: &str =
    "Invalid option: expected one of \"character\"|\"project\"|\"group\"|\"general\"";

/// Zod 4's `min(1)` sentence for an EMPTY `id`.
pub const CONTAINER_ID_EMPTY_ISSUE: &str = "Too small: expected string to have >=1 characters";

/// v4 `containerQuerySchema`'s two checks the typed verb can still fail: `id:
/// z.string().min(1).optional()` and the refine `scope === 'general' || !!id`
/// (`id is required for this scope`). MEASURED: Zod 4 still runs the refine
/// after the non-fatal `min(1)` issue, so an EMPTY id on a non-General scope
/// answers both sentences, joined `'; '`.
pub fn container_query_issue(is_general: bool, container_id: Option<&str>) -> Option<String> {
    let mut issues: Vec<&str> = Vec::new();
    if container_id == Some("") {
        issues.push(CONTAINER_ID_EMPTY_ISSUE);
    }
    if !is_general && container_id.is_none_or(str::is_empty) {
        issues.push("id is required for this scope");
    }
    (!issues.is_empty()).then(|| issues.join("; "))
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// v4 `findHome`'s miss: DEBUG `${logTag} Item not in the named container
/// {itemId, scope, context}` and the 404 `${itemNotFound} not found`.
pub fn item_not_found(kind: &ItemImagesKind, item_id: &str, scope: &str) -> Response {
    tracing::debug!(
        itemId = %item_id,
        scope,
        context = kind.context,
        "{} Item not in the named container",
        kind.log_tag
    );
    Response::error(
        ErrorKind::NotFound,
        format!("{} not found", kind.item_not_found),
    )
}

/// v4 `handleList`'s `current`: the item's pointer only when it names a listed
/// picture (a dangling pointer reads `null`).
pub fn listed_current<'a>(wanted: Option<&'a str>, listed: &[&str]) -> Option<&'a str> {
    wanted.filter(|w| listed.contains(w))
}

/// DEBUG `${logTag} Listed item images {itemId, scope, count, current, context}`.
pub fn log_listed(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    count: usize,
    current: Option<&str>,
) {
    tracing::debug!(
        itemId = %item_id,
        scope,
        count,
        current = current.unwrap_or("null"),
        context = kind.context,
        "{} Listed item images",
        kind.log_tag
    );
}

/// v4 `handleGenerate`'s body read: an empty / whitespace-only text body is
/// `{}` (the default profile); a non-JSON one is DEBUG `${logTag} Generate body
/// is not valid JSON {itemId, scope, context}` and the 400 `invalidJson`.
pub fn generate_body(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    text: &str,
) -> Result<Value, Response> {
    if crate::jsstr::js_trim(text).is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v) => Ok(v),
        Err(_) => {
            tracing::debug!(
                itemId = %item_id,
                scope,
                context = kind.context,
                "{} Generate body is not valid JSON",
                kind.log_tag
            );
            Err(bad_request(kind.invalid_json))
        }
    }
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

/// DEBUG `${logTag} Generated item image {itemId, scope, containerId, context,
/// fileId}` (v4 `{ ...meta, fileId }` — `fileId` after `context`).
pub fn log_generated(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    container_id: Option<&str>,
    file_id: &str,
) {
    tracing::debug!(
        itemId = %item_id,
        scope,
        containerId = container_id.unwrap_or("null"),
        context = kind.context,
        fileId = %file_id,
        "{} Generated item image",
        kind.log_tag
    );
}

/// v4's `kind.generationError` arm: DEBUG `${logTag} Generation failed {itemId,
/// scope, containerId, context, refused}`, then 422 `The image provider
/// declined to draw ${refusedSubject}` (a content refusal) or v4's 502 `Image
/// generation failed: <message>` (carried as [`ErrorKind::Internal`] — no 502
/// kind on the boundary; the web edge restores it), `details: { trail,
/// refused }`.
pub fn generation_failed(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    container_id: Option<&str>,
    message: &str,
    trail: Value,
    refused: bool,
) -> Response {
    tracing::debug!(
        itemId = %item_id,
        scope,
        containerId = container_id.unwrap_or("null"),
        context = kind.context,
        refused,
        "{} Generation failed",
        kind.log_tag
    );
    let (error_kind, text) = if refused {
        (
            ErrorKind::Unprocessable,
            format!(
                "The image provider declined to draw {}",
                kind.refused_subject
            ),
        )
    } else {
        (
            ErrorKind::Internal,
            format!("Image generation failed: {message}"),
        )
    };
    let mut response = Response::error(error_kind, text);
    if let Response::Error(e) = &mut response {
        e.details = Some(Box::new(
            serde_json::json!({ "trail": trail, "refused": refused }),
        ));
    }
    response
}

/// INFO `${logTag} Uploaded item image {itemId, scope, kind, context, fileId,
/// bytes}`.
pub fn log_uploaded(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    upload_kind: &str,
    file_id: &str,
    bytes: i64,
) {
    tracing::info!(
        itemId = %item_id,
        scope,
        kind = upload_kind,
        context = kind.context,
        fileId = %file_id,
        bytes,
        "{} Uploaded item image",
        kind.log_tag
    );
}

/// INFO `${logTag} Set current item image {itemId, scope, context, fileId}`.
pub fn log_set_current(kind: &ItemImagesKind, item_id: &str, scope: &str, file_id: &str) {
    tracing::info!(
        itemId = %item_id,
        scope,
        context = kind.context,
        fileId = %file_id,
        "{} Set current item image",
        kind.log_tag
    );
}

/// INFO `${logTag} Deleted item image {itemId, scope, context, fileId,
/// current}`.
pub fn log_deleted(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    file_id: &str,
    current: Option<&str>,
) {
    tracing::info!(
        itemId = %item_id,
        scope,
        context = kind.context,
        fileId = %file_id,
        current = current.unwrap_or("null"),
        "{} Deleted item image",
        kind.log_tag
    );
}

/// DEBUG `${logTag} Listed save targets {itemId, scope, count, context}`.
pub fn log_save_targets(kind: &ItemImagesKind, item_id: &str, scope: &str, count: usize) {
    tracing::debug!(
        itemId = %item_id,
        scope,
        count,
        context = kind.context,
        "{} Listed save targets",
        kind.log_tag
    );
}

/// v4 `handleSaveToStore`'s first guard: only the pictures this item shows
/// leave through this door — INFO `${logTag} Refused to save a picture that is
/// not the item's {itemId, scope, fileId, mountPointId, context}` and the 400
/// `foreignPicture`.
pub fn refuse_foreign_picture(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    file_id: &str,
    mount_point_id: &str,
) -> Response {
    tracing::info!(
        itemId = %item_id,
        scope,
        fileId = %file_id,
        mountPointId = %mount_point_id,
        context = kind.context,
        "{} Refused to save a picture that is not the item's",
        kind.log_tag
    );
    bad_request(kind.foreign_picture)
}

/// The second guard: an archived character's vault is a tombstone; nothing is
/// filed into it — INFO `${logTag} Refused to save into an archived
/// character's vault {…meta}` and the 409.
pub fn refuse_archived_vault(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    file_id: &str,
    mount_point_id: &str,
) -> Response {
    tracing::info!(
        itemId = %item_id,
        scope,
        fileId = %file_id,
        mountPointId = %mount_point_id,
        context = kind.context,
        "{} Refused to save into an archived character's vault",
        kind.log_tag
    );
    Response::error(
        ErrorKind::Conflict,
        "That store belongs to an archived character and cannot be written to",
    )
}

/// INFO `${logTag} Saved item picture to a store {itemId, scope, fileId,
/// mountPointId, context, relativePath, linkId}`.
pub fn log_saved(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    file_id: &str,
    mount_point_id: &str,
    relative_path: &str,
    link_id: &str,
) {
    tracing::info!(
        itemId = %item_id,
        scope,
        fileId = %file_id,
        mountPointId = %mount_point_id,
        context = kind.context,
        relativePath = %relative_path,
        linkId = %link_id,
        "{} Saved item picture to a store",
        kind.log_tag
    );
}

/// INFO `${logTag} Save to store rejected {itemId, scope, fileId, mountPointId,
/// context, code, message}` — v4's `message` key rides the `…Json` suffix rule
/// (a tracing `message` field clobbers the envelope; `chat_media`'s
/// `[SaveImage] rejected`, P4.170 D.1).
pub fn log_save_rejected(
    kind: &ItemImagesKind,
    item_id: &str,
    scope: &str,
    file_id: &str,
    mount_point_id: &str,
    code: &str,
    message: &str,
) {
    tracing::info!(
        itemId = %item_id,
        scope,
        fileId = %file_id,
        mountPointId = %mount_point_id,
        context = kind.context,
        code,
        messageJson = %serde_json::to_string(message).unwrap_or_default(),
        "{} Save to store rejected",
        kind.log_tag
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_container_query_sentences() {
        assert_eq!(container_query_issue(true, None), None);
        assert_eq!(
            container_query_issue(false, None).as_deref(),
            Some("id is required for this scope")
        );
        assert_eq!(
            container_query_issue(false, Some("")).as_deref(),
            Some(
                "Too small: expected string to have >=1 characters; id is required for this scope"
            )
        );
        assert_eq!(
            container_query_issue(true, Some("")).as_deref(),
            Some(CONTAINER_ID_EMPTY_ISSUE)
        );
    }

    #[test]
    fn listed_current_drops_a_dangling_pointer() {
        assert_eq!(listed_current(Some("a"), &["a", "b"]), Some("a"));
        assert_eq!(listed_current(Some("z"), &["a"]), None);
        assert_eq!(listed_current(None, &["a"]), None);
    }
}
