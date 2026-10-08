//! The wardrobe image bridge — v4 `lib/file-storage/wardrobe-image-bridge.ts`
//! (`7c8572869`, #82; `b3f937076`): writes a wardrobe item's pictures into the
//! mount that holds the item's `Wardrobe/*.md` — a character vault, a group's
//! or project's official store, or Quilltap General — beside the markdown,
//! keyed by item id so a rename cannot orphan them:
//!
//! ```text
//! Wardrobe/images/<itemId>/<yyyymmdd-hhmmss>-<kind>-<8 hex>.webp
//! ```
//!
//! The bytes go through `link_blob_content`, which normalizes images to WebP,
//! de-duplicates by sha256 (an Import-from-image photograph shared by N pieces
//! is one blob behind N links) and mints the `doc_mount_files` /
//! `doc_mount_blobs` / `doc_mount_file_links` trio. The returned storage key is
//! the `mount-blob:{mountPointId}:{blobId}` shim every `files` row reader
//! already understands. The PATH helpers are P4.D255's
//! ([`crate::services::wardrobe_image_paths`], frozen for the round).
//!
//! The projection sweep touches `.md` documents only, so these blobs are never
//! mistaken for garments and never swept — which is also why they are not
//! renamed or deleted with the item: the item routes remove them explicitly
//! (`services::wardrobe_item_images`).
//!
//! **Structural NO-PORT (P4.D263 R-A): v4's job-child branch.** v4 routes a
//! write made in its job child to the parent (`writeWardrobeItemImage` →
//! `callHost('writeWardrobeItemImage', …)`, `wardrobe-image-bridge.ts:
//! 105-116`, with the DEBUG `Routing wardrobe image write to the parent`; the
//! parent's `host-rpc-dispatcher.ts:92-100`; `ipc-types.ts:161`), and guards
//! the link delete with `refuseInJobChild` (`:66-72`, `:186`) — because v4's
//! job child cannot hold the RW connection. v5 has no child process: the job
//! handler runs in-process and every write is a plain `Db::write` on the one
//! writer, from the route and the handler alike (the ruling the avatar and
//! Lantern bridges took — `image_job_storage.rs`' header). Nothing of either
//! branch is ported, and neither DEBUG nor the refusal can occur.
//!
//! **v4's realtime emits (`emitDocumentWritten` / `emitDocumentDeleted`).** v5
//! carries no per-document db-store event bus; a mount write's realtime
//! invalidation rides the write path's table hints (`realtime/job_topics.rs`
//! maps the `docMount*` tables to the `mountPoints` collection). The lane
//! record carries the measurement.

use rusqlite::Connection;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkBlobInput};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::services::file_storage::build_mount_blob_storage_key;
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_image_paths::{
    sanitize_leaf_name, wardrobe_item_image_folder, wardrobe_item_image_path,
};

const LOG_CONTEXT: &str = "file-storage.wardrobe-image-bridge";

/// v4 `WardrobeImageKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeImageKind {
    Generated,
    Uploaded,
    Imported,
}

impl WardrobeImageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeImageKind::Generated => "generated",
            WardrobeImageKind::Uploaded => "uploaded",
            WardrobeImageKind::Imported => "imported",
        }
    }

    /// v4 `SOURCE_BY_KIND` (`item-images.ts:205-209`) — the `files.source`.
    pub fn file_source(self) -> &'static str {
        match self {
            WardrobeImageKind::Generated => "GENERATED",
            WardrobeImageKind::Uploaded => "UPLOADED",
            WardrobeImageKind::Imported => "IMPORTED",
        }
    }

    /// v4 `carryItemImages`' inverse map (`item-images.ts:442`): `GENERATED` →
    /// generated, `IMPORTED` → imported, anything else → uploaded.
    pub fn from_file_source(source: &str) -> Self {
        match source {
            "GENERATED" => WardrobeImageKind::Generated,
            "IMPORTED" => WardrobeImageKind::Imported,
            _ => WardrobeImageKind::Uploaded,
        }
    }
}

/// What mints a fresh leaf name — v4's `new Date()` + `randomUUID()`
/// (`wardrobe-image-bridge.ts:125`), injected so the differentials can pin
/// both.
#[derive(Debug, Clone)]
pub struct LeafSeed {
    /// The write instant, unix milliseconds (UTC).
    pub now_ms: i64,
    /// A UUID whose first 8 characters are the leaf's random tail.
    pub uuid: String,
}

impl LeafSeed {
    /// The production seed: the wall clock and a fresh v4 UUID.
    pub fn now() -> Self {
        LeafSeed {
            now_ms: crate::clock::now_unix_ms(),
            uuid: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// v4 `timestampStem(now)` — `20261007-142233`: UTC, sortable, filename-safe
/// (`toISOString()`'s date with the dashes dropped, `-`, its time with the
/// colons dropped).
pub fn timestamp_stem(now_ms: i64) -> String {
    let iso = crate::clock::iso_from_unix_ms(now_ms);
    format!(
        "{}-{}",
        iso[0..10].replace('-', ""),
        iso[11..19].replace(':', "")
    )
}

/// v4 `WriteWardrobeItemImageInput`.
#[derive(Debug, Clone)]
pub struct WriteWardrobeItemImageInput<'a> {
    pub mount_point_id: &'a str,
    pub item_id: &'a str,
    pub kind: WardrobeImageKind,
    pub content: &'a [u8],
    pub content_type: &'a str,
    pub description: Option<&'a str>,
    /// Re-use an exact leaf name (transfer re-linking keeps the source's).
    /// `None` mints a fresh `<timestamp>-<kind>-<8 hex>.webp` with collision
    /// bumping.
    pub leaf_name: Option<&'a str>,
}

/// v4 `WriteWardrobeItemImageResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenWardrobeImage {
    pub storage_key: String,
    pub link_id: String,
    pub blob_id: String,
    pub relative_path: String,
    /// The leaf the link landed on — what the `files` row records as
    /// `originalFilename`.
    pub leaf_name: String,
    pub stored_mime_type: String,
    pub sha256: String,
    pub size_bytes: i64,
}

fn posix_basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// v4 `writeWardrobeItemImage(input)` — write one picture for `item_id` into
/// `mount_point_id`. The caller resolves the mount (and, for an archived
/// character, lets the resolver refuse). `blob_webp` is `link_blob_content`'s
/// normalization encoder (v4's one `sharp`).
pub fn write_wardrobe_item_image(
    mount: &Connection,
    input: &WriteWardrobeItemImageInput<'_>,
    seed: &LeafSeed,
    blob_webp: &dyn WebpTranscoder,
) -> Result<WrittenWardrobeImage, DbError> {
    let folder = wardrobe_item_image_folder(input.item_id);
    let relative_path = match input.leaf_name {
        // A given leaf is sanitized and used as-is — NO unique-suffix bump: a
        // transfer re-links the same picture at the same path.
        Some(leaf) => format!("{folder}/{}", sanitize_leaf_name(leaf)),
        // The random tail keeps two writes in the same second apart:
        // resolveUniqueRelativePath only checks, it does not reserve, and a
        // second linkBlobContent at the same path would overwrite the first's
        // link.
        None => {
            let tail: String = seed.uuid.chars().take(8).collect();
            let desired = format!(
                "{folder}/{}-{}-{tail}.webp",
                timestamp_stem(seed.now_ms),
                input.kind.as_str()
            );
            crate::photos::save_image_to_album::resolve_unique_relative_path(
                mount,
                input.mount_point_id,
                &desired,
            )?
        }
    };

    let links = DocMountFileLinksRepository::with_blob_codec(mount, blob_webp);
    links.ensure_folder_path(input.mount_point_id, &folder)?;

    // No transcode here: link_blob_content is the image-normalization
    // chokepoint and rewrites storedMimeType / relativePath to whatever it
    // actually stores.
    let basename = posix_basename(&relative_path);
    let linked = links.link_blob_content(&LinkBlobInput {
        mount_point_id: input.mount_point_id.to_string(),
        relative_path: relative_path.clone(),
        file_name: basename.clone(),
        file_type: None,
        original_file_name: Some(basename),
        original_mime_type: Some(input.content_type.to_string()),
        stored_mime_type: input.content_type.to_string(),
        sha256: crate::photos::keep_image_markdown::sha256_of_buffer(input.content),
        data: input.content.to_vec(),
        normalize_images: true,
        description: Some(input.description.unwrap_or("").to_string()),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
        last_modified: None,
        created_at: None,
    })?;

    // v4 `repos.docMountPoints.refreshStats(mountPointId).catch(() => {})` —
    // best-effort.
    let _ = DocMountPointsRepository::new(mount).refresh_stats(input.mount_point_id);

    let link = links
        .find_link_row_by_id(&linked.link_id)?
        .ok_or_else(|| DbError::Internal(format!("link {} vanished", linked.link_id)))?;
    let blob = DocMountBlobsRepository::new(mount).find_by_id(&linked.blob_id)?;

    let written = WrittenWardrobeImage {
        storage_key: build_mount_blob_storage_key(input.mount_point_id, &linked.blob_id),
        link_id: link.id.clone(),
        blob_id: linked.blob_id.clone(),
        leaf_name: posix_basename(&link.relative_path),
        relative_path: link.relative_path.clone(),
        stored_mime_type: blob
            .map(|b| b.stored_mime_type)
            .unwrap_or_else(|| input.content_type.to_string()),
        sha256: link.sha256.clone(),
        size_bytes: link.file_size_bytes,
    };

    tracing::debug!(
        context = LOG_CONTEXT,
        mountPointId = %input.mount_point_id,
        itemId = %input.item_id,
        kind = input.kind.as_str(),
        relativePath = %written.relative_path,
        blobId = %written.blob_id,
        sizeBytes = written.size_bytes,
        "[WardrobeImageBridge] Wrote wardrobe item image"
    );
    Ok(written)
}

/// v4 `deleteWardrobeItemImageLink(mountPointId, itemId, leafName)` — remove
/// one picture's link from a mount. `deleteWithGC` drops the blob only when
/// this was its last link (an imported photograph shared with sibling pieces
/// survives). `true` when a link was found. The path read is v4's fallback
/// `queryJoined` (a failed read is "no link"), the delete v4's fallback
/// `deleteWithGC` (a failed delete logs and counts as not collected).
pub fn delete_wardrobe_item_image_link(
    mount: &Connection,
    mount_point_id: &str,
    item_id: &str,
    leaf_name: &str,
) -> bool {
    let relative_path = wardrobe_item_image_path(item_id, leaf_name);
    let links = DocMountFileLinksRepository::new(mount);
    let Some(link) = links.find_by_mount_point_and_path_or_none(mount_point_id, &relative_path)
    else {
        tracing::debug!(
            context = LOG_CONTEXT,
            mountPointId = %mount_point_id,
            relativePath = %relative_path,
            "[WardrobeImageBridge] No link to delete"
        );
        return false;
    };
    let blob_collected = links.delete_with_gc_or_false(&link.id);
    // v4 `refreshStats(mountPointId).catch(() => {})` — best-effort.
    let _ = DocMountPointsRepository::new(mount).refresh_stats(mount_point_id);
    tracing::debug!(
        context = LOG_CONTEXT,
        mountPointId = %mount_point_id,
        relativePath = %relative_path,
        blobCollected = blob_collected,
        "[WardrobeImageBridge] Deleted wardrobe item image link"
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `timestampStem(now)` — `toISOString()`'s date and time, UTC.
    #[test]
    fn timestamp_stem_is_utc_compact() {
        // 2026-10-07T14:22:33.456Z
        assert_eq!(timestamp_stem(1_791_382_953_456), "20261007-142233");
        assert_eq!(timestamp_stem(0), "19700101-000000");
    }

    /// v4 `SOURCE_BY_KIND` and the carry's inverse map (an unknown source
    /// carries as `uploaded`).
    #[test]
    fn kind_maps() {
        for (k, s) in [
            (WardrobeImageKind::Generated, "GENERATED"),
            (WardrobeImageKind::Uploaded, "UPLOADED"),
            (WardrobeImageKind::Imported, "IMPORTED"),
        ] {
            assert_eq!(k.file_source(), s);
            assert_eq!(WardrobeImageKind::from_file_source(s), k);
        }
        assert_eq!(
            WardrobeImageKind::from_file_source("SYSTEM"),
            WardrobeImageKind::Uploaded
        );
    }
}
