//! Pictures travelling with a transferred item — v4 `lib/wardrobe/
//! item-images.ts` (`7c8572869`, #82): `carryItemImages` (`:413-486`),
//! `PendingImageMove` (`:489-496`), `commitMovedImages` (`:504-524`),
//! `dropSourceImageLinks` (`:527-552`).
//!
//! - **move** (same item id): each picture is re-linked at the same relative
//!   path in the destination (the same bytes de-duplicate to one blob).
//!   Nothing else changes yet: the `files` rows keep their source storage key
//!   and the source links stay, so a transfer that fails part-way leaves the
//!   source item whole (the shared blob reads the same through either link).
//!   The returned [`PendingImageMove`] is applied by [`commit_moved_images`]
//!   once the item has landed and the source copy is gone.
//! - **copy** (fresh item id): each picture is linked under the new id, with a
//!   new `files` row linked to the new item; the old → new file id map lets the
//!   copy's `imageFileId` point at its own copy.
//!
//! v4 does not make a transfer transactional: a failure after the carry leaves
//! destination links behind, one after the deletes leaves rows pointing at the
//! source blob (`item-images.ts:401-408` documents only the "before the
//! deletes" safety). v5's transfer runs in ONE `Db::write` closure but does
//! not roll back on a `TransferError` either — the same shape.

use rusqlite::Connection;
use serde_json::json;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::files::{FileUpdate, FilesRepository};
use crate::db::DbError;
use crate::services::file_storage::{build_mount_blob_storage_key, parse_mount_blob_storage_key};
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_image_bridge::{
    delete_wardrobe_item_image_link, write_wardrobe_item_image, LeafSeed, WardrobeImageKind,
    WriteWardrobeItemImageInput,
};

use super::primitives::find_item_image_rows;
use super::service::create_file_row;

const LOG_CONTEXT: &str = "wardrobe.item-images";

/// v4 `carryItemImages`' `mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarryMode {
    Move,
    Copy,
}

impl CarryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CarryMode::Move => "move",
            CarryMode::Copy => "copy",
        }
    }
}

/// v4 `carryItemImages`' `args`.
#[derive(Debug, Clone)]
pub struct CarryArgs<'a> {
    pub mode: CarryMode,
    pub source_item_id: &'a str,
    pub destination_item_id: &'a str,
    pub destination_mount_point_id: &'a str,
    pub user_id: &'a str,
}

/// One source link a move drops once its row points at the destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceImageLink {
    pub mount_point_id: String,
    pub leaf_name: String,
}

/// One row a move re-points (v4 `PendingImageMove.repoints[]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRepoint {
    pub file_id: String,
    pub storage_key: String,
    /// The link to drop once the row points at the destination; `None` when
    /// the picture already lived in the destination mount.
    pub source_link: Option<SourceImageLink>,
}

/// v4 `PendingImageMove` — what a move still owes once its item has landed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingImageMove {
    pub repoints: Vec<ImageRepoint>,
}

/// v4 `carryItemImages`' `{ fileIdMap, pendingMove }`. The map keeps v4's
/// `Map` insertion order.
#[derive(Debug, Clone, Default)]
pub struct CarriedImages {
    pub file_id_map: Vec<(String, String)>,
    pub pending_move: PendingImageMove,
}

impl CarriedImages {
    /// v4 `fileIdMap.get(id)`.
    pub fn mapped(&self, file_id: &str) -> Option<&str> {
        self.file_id_map
            .iter()
            .find(|(old, _)| old == file_id)
            .map(|(_, new)| new.as_str())
    }
}

/// v4 `carryItemImages(repos, args)` — carry an item's pictures to another
/// mount for a transfer. A picture whose blob cannot be read is WARNed and
/// skipped (not carried — the transfers route then nulls a pointer at it).
/// `mint_file_id` mints a copy's fresh `files` row id (v4 `randomUUID()`),
/// `now_iso` stamps it, `seed` is the bridge's (unused on this path — every
/// carried picture keeps its leaf name — but the bridge takes one).
pub fn carry_item_images(
    main: &Connection,
    mount: &Connection,
    args: &CarryArgs<'_>,
    mint_file_id: &mut dyn FnMut() -> String,
    now_iso: &str,
    blob_webp: &dyn WebpTranscoder,
) -> Result<CarriedImages, DbError> {
    let mut out = CarriedImages::default();
    let images = find_item_image_rows(main, args.source_item_id)?;

    for file in &images {
        let parsed = parse_mount_blob_storage_key(file.storage_key.as_deref().unwrap_or(""));
        // v4 `file.storageKey ? await readMountBlob(storageKey) : null` —
        // `readMountBlob` answers null for a malformed key or a missing blob.
        let bytes = match &parsed {
            Some((_mp, blob_id)) => DocMountBlobsRepository::new(mount).read_data(blob_id)?,
            None => None,
        };
        let (Some((source_mount_point_id, _)), Some(bytes)) = (parsed, bytes) else {
            tracing::warn!(
                context = LOG_CONTEXT,
                fileId = %file.id,
                itemId = %args.source_item_id,
                "[WardrobeImages] Wardrobe image has no readable blob; not carried"
            );
            continue;
        };

        let written = write_wardrobe_item_image(
            mount,
            &WriteWardrobeItemImageInput {
                mount_point_id: args.destination_mount_point_id,
                item_id: args.destination_item_id,
                kind: WardrobeImageKind::from_file_source(&file.source),
                content: &bytes,
                content_type: &file.mime_type,
                description: file.description.as_deref(),
                leaf_name: Some(&file.original_filename),
            },
            &LeafSeed {
                now_ms: 0,
                uuid: String::new(),
            },
            blob_webp,
        )?;
        let storage_key =
            build_mount_blob_storage_key(args.destination_mount_point_id, &written.blob_id);

        match args.mode {
            CarryMode::Move => {
                out.pending_move.repoints.push(ImageRepoint {
                    file_id: file.id.clone(),
                    storage_key,
                    source_link: (source_mount_point_id != args.destination_mount_point_id).then(
                        || SourceImageLink {
                            mount_point_id: source_mount_point_id.clone(),
                            leaf_name: file.original_filename.clone(),
                        },
                    ),
                });
                out.file_id_map.push((file.id.clone(), file.id.clone()));
            }
            CarryMode::Copy => {
                // v4 `const { id, createdAt, updatedAt, ...rest } = file;
                // files.create({ ...rest, originalFilename, linkedTo, tags,
                // storageKey }, { id: randomUUID() })` — every other column
                // copied as it stands.
                let mut copy = file.clone();
                copy.id = mint_file_id();
                copy.original_filename = written.leaf_name.clone();
                copy.linked_to = vec![json!(args.destination_item_id)];
                copy.tags = vec![json!(args.destination_item_id)];
                copy.storage_key = Some(storage_key);
                copy.created_at = now_iso.to_string();
                copy.updated_at = now_iso.to_string();
                create_file_row(main, &copy)?;
                out.file_id_map.push((file.id.clone(), copy.id));
            }
        }
    }

    if !images.is_empty() {
        tracing::info!(
            context = LOG_CONTEXT,
            mode = args.mode.as_str(),
            sourceItemId = %args.source_item_id,
            destinationItemId = %args.destination_item_id,
            destinationMountPointId = %args.destination_mount_point_id,
            count = out.file_id_map.len(),
            "[WardrobeImages] Carried wardrobe item images for transfer"
        );
    }
    Ok(out)
}

/// v4 `commitMovedImages(repos, itemId, pending)` — finish a move's pictures
/// after the item has landed at the destination and left the source: point
/// each `files` row at its destination link, then drop the source links. A
/// row that fails to repoint keeps its source link (still readable there)
/// rather than being left pointing at nothing.
pub fn commit_moved_images(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    pending: &PendingImageMove,
    now_iso: &str,
) {
    let mut to_drop: Vec<SourceImageLink> = Vec::new();
    for repoint in &pending.repoints {
        let result = FilesRepository::new(main).update(
            &repoint.file_id,
            &FileUpdate {
                storage_key: Some(repoint.storage_key.clone()),
                updated_at: now_iso.to_string(),
                ..Default::default()
            },
        );
        match result {
            Ok(_) => {
                if let Some(link) = &repoint.source_link {
                    to_drop.push(link.clone());
                }
            }
            Err(e) => {
                let error = crate::db::fallback::error_text(&e);
                tracing::warn!(
                    context = LOG_CONTEXT,
                    itemId = %item_id,
                    fileId = %repoint.file_id,
                    error = %error,
                    "[WardrobeImages] Failed to repoint a moved wardrobe picture; keeping its source link"
                );
            }
        }
    }
    drop_source_image_links(mount, item_id, &to_drop);
}

/// v4 `dropSourceImageLinks(itemId, sourceLinks)` — after a move, drop the
/// source mount's links (`deleteWithGC`; the blob survives via the new link).
/// The bridge's delete never throws — in v4 its path read and its
/// `deleteWithGC` are both fallback calls (a failure logs and answers
/// "nothing"), and its `refuseInJobChild` guard cannot fire in the parent — so
/// v4's per-link WARN `Failed to drop a source image link after move` is
/// unreachable through v4's real code and pinned NEGATIVE here (§R.5). The
/// INFO counts the links that WENT.
pub fn drop_source_image_links(
    mount: &Connection,
    item_id: &str,
    source_links: &[SourceImageLink],
) {
    let mut dropped = 0usize;
    for link in source_links {
        if delete_wardrobe_item_image_link(mount, &link.mount_point_id, item_id, &link.leaf_name) {
            dropped += 1;
        }
    }
    if !source_links.is_empty() {
        tracing::info!(
            context = LOG_CONTEXT,
            itemId = %item_id,
            dropped = dropped,
            "[WardrobeImages] Dropped source image links after move"
        );
    }
}
