//! Inventory object pictures — mirrors the wardrobe's (v4
//! `lib/inventory/item-images.ts`, `4fbdad186`; P4.D324).
//!
//! Each picture's bytes are a blob link under `Inventory/images/<itemId>/` in
//! the object's own tier mount, plus a `files` row (`linkedTo: [itemId]`,
//! category IMAGE). The history is `files.findByLinkedTo(itemId)`; the object
//! file carries only the current pick, `imageFileId`. The generic steps
//! (listing, the choice check, removal, transfer carry) are the wardrobe's,
//! called with the Inventory's images folder (R-16 — the primitives take
//! `images_folder` explicitly; v4 defaults it to the wardrobe's).
//!
//! An instance with no pictures of its own shows its archetype's, read-only
//! ([`list_inventory_images_with_fallthrough`]).
//!
//! Picture is for choosing, not for display (spec §2): the client shows it
//! only in pickers, the editor and the viewer.
//!
//! Every operation runs inside ONE `Db::write` closure at its caller (the
//! route verb, the generation): the bridge write, the `files` row and the
//! object's pointer land together.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::errors::InventoryError;
use super::location::{
    resolve_inventory_location, InventoryLocation, ResolveInventoryLocationOptions,
};
use super::pool::{load_pool_for_location, LoadPoolContext};
use super::types::{InventoryFields, InventoryRecord, InventoryScope, ResolvedInventoryItem};
use crate::db::files::FileRow;
use crate::db::DbError;
use crate::files::generated_file_row::{
    create_generated_file_row, GeneratedFileRowInput, GenerationRecord, StoredImageBytes,
};
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_image_bridge::{
    write_wardrobe_item_image, WardrobeImageKind, WriteWardrobeItemImageInput,
};
use crate::services::wardrobe_item_images::carry::{
    carry_item_images, commit_moved_images, CarriedImages, CarryArgs, CarryMode, PendingImageMove,
};
use crate::services::wardrobe_item_images::primitives::{
    assert_item_image_choice, cleanup_item_images, find_item_image_rows, remove_image_file,
    ItemImageChoiceError, ItemImageCleanupMeta,
};
use crate::services::wardrobe_item_images::service::{to_wardrobe_image_summary, ImageMint};

pub use crate::services::wardrobe_image_paths::INVENTORY_IMAGES_FOLDER;

const LOG_CONTEXT: &str = "inventory.item-images";

/// An object located in its tier, with the means to write it back (v4
/// `InventoryItemHome`).
#[derive(Debug, Clone)]
pub struct InventoryItemHome {
    pub location: InventoryLocation,
    pub record: InventoryRecord,
    /// The resolved object (an instance's archetype filled in).
    pub item: ResolvedInventoryItem,
}

/// The object's own pictures, newest first (v4 `listInventoryItemImages` —
/// the wardrobe's `files.findByLinkedTo` read: pictures of either kind are
/// told apart only by `linkedTo`).
pub fn list_inventory_item_images(
    main: &Connection,
    item_id: &str,
) -> Result<Vec<FileRow>, DbError> {
    find_item_image_rows(main, item_id)
}

/// v4 `InventoryImageSummary = WardrobeItemImageSummary & { inherited? }` —
/// the wardrobe summary, plus `inherited: true` (appended) for an archetype's
/// picture shown on an instance.
pub fn to_inventory_image_summary(file: &FileRow, inherited: bool) -> Value {
    let mut summary = to_wardrobe_image_summary(file);
    if inherited {
        if let Value::Object(m) = &mut summary {
            m.insert("inherited".into(), json!(true));
        }
    }
    summary
}

/// The pictures to show for an object (v4 `listInventoryImagesWithFallthrough`):
/// its own, or — for an instance with none — its archetype's, marked
/// `inherited` (read-only). DEBUG `[InventoryImages] Instance shows its
/// archetype's pictures` on the fall-through.
pub fn list_inventory_images_with_fallthrough(
    main: &Connection,
    item: &impl InventoryFields,
) -> Result<Vec<Value>, DbError> {
    let own = list_inventory_item_images(main, item.id())?;
    let Some(parent_id) = item.parent_id().filter(|_| own.is_empty()) else {
        return Ok(own
            .iter()
            .map(|f| to_inventory_image_summary(f, false))
            .collect());
    };
    let inherited = list_inventory_item_images(main, parent_id)?;
    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %item.id(),
        parentId = %parent_id,
        count = inherited.len(),
        "[InventoryImages] Instance shows its archetype's pictures"
    );
    Ok(inherited
        .iter()
        .map(|f| to_inventory_image_summary(f, true))
        .collect())
}

/// Refuse an `imageFileId` that is not one of the object's own pictures (v4
/// `assertInventoryImageChoice` = the wardrobe's).
pub fn assert_inventory_image_choice(
    main: &Connection,
    item_id: &str,
    file_id: Option<&str>,
) -> Result<(), ItemImageChoiceError> {
    assert_item_image_choice(main, item_id, file_id)
}

fn choice_error(e: ItemImageChoiceError) -> InventoryError {
    match e {
        ItemImageChoiceError::Foreign { item_id, file_id } => {
            InventoryError::ForeignImage { item_id, file_id }
        }
        ItemImageChoiceError::Read(m) => InventoryError::Other(m),
    }
}

/// v4 `AddInventoryItemImageInput`.
#[derive(Debug, Clone)]
pub struct AddInventoryItemImageInput<'a> {
    pub user_id: &'a str,
    pub kind: WardrobeImageKind,
    pub content: &'a [u8],
    pub content_type: &'a str,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub generation_prompt: Option<&'a str>,
    pub generation_model: Option<&'a str>,
    pub generation_revised_prompt: Option<&'a str>,
}

/// v4 `addInventoryItemImage`'s `{ file, record }`.
#[derive(Debug, Clone)]
pub struct AddedInventoryItemImage {
    pub file: FileRow,
    pub record: Option<InventoryRecord>,
}

/// Store a new picture for the object and make it current (v4
/// `addInventoryItemImage`): the mount checked for writing FIRST (a tombstone
/// refuses), the bridge write under `Inventory/images/<id>/`, the `files`
/// row (`SOURCE_BY_KIND` — the wardrobe kind's file source), then ONE update
/// of `imageFileId`. INFO `[InventoryImages] Added inventory object image`.
pub fn add_inventory_item_image(
    main: &Connection,
    mount: &Connection,
    home: &InventoryItemHome,
    input: &AddInventoryItemImageInput<'_>,
    mint: &ImageMint,
    blob_webp: &dyn WebpTranscoder,
) -> Result<AddedInventoryItemImage, InventoryError> {
    let mount_point_id = home.location.writable_mount_point_id(main)?;
    // v4's CURLY quotes; `title || name`.
    let description = format!(
        "Inventory image for \u{201C}{}\u{201D}",
        home.item.title_or_name()
    );
    let written = write_wardrobe_item_image(
        mount,
        &WriteWardrobeItemImageInput {
            mount_point_id: &mount_point_id,
            item_id: home.record.id(),
            kind: input.kind,
            content: input.content,
            content_type: input.content_type,
            description: Some(&description),
            leaf_name: None,
            images_folder: INVENTORY_IMAGES_FOLDER,
        },
        &mint.leaf,
        blob_webp,
    )?;
    let file = create_generated_file_row(
        main,
        &GeneratedFileRowInput {
            user_id: input.user_id,
            id: Some(&mint.file_id),
            sha256: &written.sha256,
            original_filename: &written.leaf_name,
            stored: &StoredImageBytes {
                storage_key: written.storage_key.clone(),
                stored_mime_type: written.stored_mime_type.clone(),
                size_bytes: written.size_bytes,
            },
            width: input.width.map(|w| w as f64),
            height: input.height.map(|h| h as f64),
            linked_to: vec![home.record.id().to_string()],
            tags: vec![home.record.id().to_string()],
            source: Some(input.kind.file_source()),
            generation: Some(&GenerationRecord {
                prompt: input.generation_prompt.map(str::to_string),
                model: input.generation_model.map(str::to_string),
                revised_prompt: input.generation_revised_prompt.map(str::to_string),
            }),
            generation_key: None,
            description: None,
            project_id: None,
            folder_path: None,
            now_iso: &mint.now_iso,
        },
    )?;
    let record = home.location.update(
        main,
        mount,
        home.record.id(),
        image_patch(Some(&file.id)),
        &[],
    )?;
    tracing::info!(
        context = LOG_CONTEXT,
        scope = home.location.scope.as_str(),
        itemId = %home.record.id(),
        fileId = %file.id,
        kind = input.kind.as_str(),
        sizeBytes = written.size_bytes,
        "[InventoryImages] Added inventory object image"
    );
    Ok(AddedInventoryItemImage { file, record })
}

fn image_patch(file_id: Option<&str>) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert(
        "imageFileId".into(),
        file_id.map_or(Value::Null, |f| json!(f)),
    );
    patch
}

/// Make one of the object's own pictures current (v4
/// `setCurrentInventoryItemImage`): the choice check, the tombstone, the
/// update. DEBUG `[InventoryImages] Set current inventory object image`.
pub fn set_current_inventory_item_image(
    main: &Connection,
    mount: &Connection,
    home: &InventoryItemHome,
    file_id: &str,
) -> Result<String, InventoryError> {
    assert_inventory_image_choice(main, home.record.id(), Some(file_id)).map_err(choice_error)?;
    home.location.writable_mount_point_id(main)?;
    home.location.update(
        main,
        mount,
        home.record.id(),
        image_patch(Some(file_id)),
        &[],
    )?;
    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %home.record.id(),
        fileId = %file_id,
        "[InventoryImages] Set current inventory object image"
    );
    Ok(file_id.to_string())
}

/// Delete one of the object's pictures; the next-newest becomes current (v4
/// `deleteInventoryItemImage`). Not one of its own → `ForeignImage`. When the
/// deleted picture was current, or the current pointer names no listed
/// picture, the first remaining (newest first) becomes current, else null.
/// INFO `[InventoryImages] Deleted inventory object image`.
pub fn delete_inventory_item_image(
    main: &Connection,
    mount: &Connection,
    home: &InventoryItemHome,
    file_id: &str,
) -> Result<Option<String>, InventoryError> {
    let images = list_inventory_item_images(main, home.record.id())?;
    let Some(target) = images.iter().find(|f| f.id == file_id) else {
        return Err(InventoryError::ForeignImage {
            item_id: home.record.id().to_string(),
            file_id: file_id.to_string(),
        });
    };
    home.location.writable_mount_point_id(main)?;
    remove_image_file(
        main,
        mount,
        home.record.id(),
        target,
        INVENTORY_IMAGES_FOLDER,
    )?;
    // `home.record.imageFileId ?? null`; the dangling test is v4's truthy
    // `current && !images.some(...)`.
    let mut current = home.record.image_file_id().map(str::to_string);
    let dangling = current
        .as_deref()
        .is_some_and(|c| !c.is_empty() && !images.iter().any(|f| f.id == c));
    if current.as_deref() == Some(file_id) || dangling {
        current = images
            .iter()
            .find(|f| f.id != file_id)
            .map(|f| f.id.clone());
        home.location.update(
            main,
            mount,
            home.record.id(),
            image_patch(current.as_deref()),
            &[],
        )?;
    }
    tracing::info!(
        context = LOG_CONTEXT,
        itemId = %home.record.id(),
        fileId = %file_id,
        current = current.as_deref().unwrap_or("null"),
        "[InventoryImages] Deleted inventory object image"
    );
    Ok(current)
}

/// Drop every picture of a deleted object (v4 `cleanupInventoryItemImages`).
/// Logs, never fails; answers the removed count.
pub fn cleanup_inventory_item_images(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    log_tag: &str,
    scope: InventoryScope,
    mount_point_id: &str,
) -> usize {
    cleanup_item_images(
        main,
        mount,
        item_id,
        log_tag,
        ItemImageCleanupMeta::Inventory {
            scope: scope.as_str(),
            mount_point_id,
        },
        INVENTORY_IMAGES_FOLDER,
    )
}

/// v4 `carryInventoryItemImages`'s args.
#[derive(Debug, Clone, Copy)]
pub struct CarryInventoryImagesArgs<'a> {
    pub mode: CarryMode,
    pub source_item_id: &'a str,
    pub destination_item_id: &'a str,
    pub destination_mount_point_id: &'a str,
    pub user_id: &'a str,
}

/// Carry an object's pictures to another mount for a transfer (v4
/// `carryInventoryItemImages` — the wardrobe's, in the Inventory's folder).
pub fn carry_inventory_item_images(
    main: &Connection,
    mount: &Connection,
    args: CarryInventoryImagesArgs<'_>,
    mint_file_id: &mut dyn FnMut() -> String,
    now_iso: &str,
    blob_webp: &dyn WebpTranscoder,
) -> Result<CarriedImages, DbError> {
    carry_item_images(
        main,
        mount,
        &CarryArgs {
            mode: args.mode,
            source_item_id: args.source_item_id,
            destination_item_id: args.destination_item_id,
            destination_mount_point_id: args.destination_mount_point_id,
            user_id: args.user_id,
            images_folder: INVENTORY_IMAGES_FOLDER,
        },
        mint_file_id,
        now_iso,
        blob_webp,
    )
}

/// Finish a move's pictures once the object has landed (v4
/// `commitMovedInventoryImages`).
pub fn commit_moved_inventory_images(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    pending: &PendingImageMove,
    now_iso: &str,
) {
    commit_moved_images(
        main,
        mount,
        item_id,
        pending,
        now_iso,
        INVENTORY_IMAGES_FOLDER,
    )
}

/// Find `item_id` in the named tier, resolved, or `None` when the tier does
/// not resolve or does not hold it (v4 `resolveInventoryItemHome`). A read: a
/// store that does not exist yet is not provisioned.
pub fn resolve_inventory_item_home(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: InventoryScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Result<Option<InventoryItemHome>, DbError> {
    let Some(location) = resolve_inventory_location(
        main,
        mount,
        scope,
        container_id,
        user_id,
        ResolveInventoryLocationOptions::default(),
    )?
    else {
        return Ok(None);
    };
    let Some(record) = location.find_item(mount, item_id)? else {
        return Ok(None);
    };
    let pool = load_pool_for_location(main, mount, &location, LoadPoolContext::default())?;
    let item = pool.resolve(&record);
    Ok(Some(InventoryItemHome {
        location,
        record,
        item,
    }))
}
