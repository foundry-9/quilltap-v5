//! v4 `lib/photos/photo-link-summary.ts` — the reverse index from an image's
//! content hash to every `doc_mount_file_links` row that hard-links those bytes.
//!
//! Empty (`{ count: 0, linkers: [] }`) when the sha isn't in the mount-index (a
//! legacy `files` image never written there — the common case). Consumed by the
//! character-gallery listing (per-entry `linkSummary`) and the save-path
//! re-upload guard.
//!
//! NB: `api::salon` carries a byte-identical private copy predating this shared
//! module; a later cleanup can unify them.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::doc_mount_file_links::{is_photos_relative_path, DocMountFileLinksRepository};
use crate::db::doc_mount_files::DocMountFilesRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::photos::keep_image_markdown::parse_kept_image_frontmatter;

fn empty() -> Value {
    json!({ "count": 0, "linkers": [] })
}

/// v4 `getPhotoLinkSummaryBySha256(sha256)` — resolve every mount-index hard link
/// for an image by its content hash. `mount` is a mount-index connection.
pub fn get_photo_link_summary_by_sha256(
    mount: &Connection,
    sha256: &str,
) -> Result<Value, DbError> {
    if sha256.is_empty() {
        return Ok(empty());
    }
    let Some(file_id) = DocMountFilesRepository::new(mount).find_by_sha256(sha256)? else {
        return Ok(empty());
    };
    let links = DocMountFileLinksRepository::new(mount).find_by_file_id(&file_id)?;
    if links.is_empty() {
        return Ok(empty());
    }
    let points = DocMountPointsRepository::new(mount);
    let mut cache: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut linkers: Vec<Value> = Vec::new();
    for link in &links {
        let (name, store_type) = match cache.get(&link.mount_point_id) {
            Some(v) => v.clone(),
            None => {
                let Some(mp) = points.find_store_naming_by_id(&link.mount_point_id)? else {
                    continue;
                };
                let v = (
                    mp.name,
                    mp.store_type.unwrap_or_else(|| "documents".to_string()),
                );
                cache.insert(link.mount_point_id.clone(), v.clone());
                v
            }
        };
        let fm = parse_kept_image_frontmatter(link.extracted_text.as_deref());
        linkers.push(json!({
            "linkId": link.id,
            "mountPointId": link.mount_point_id,
            "mountPointName": name,
            "mountStoreType": store_type,
            "relativePath": link.relative_path,
            "isPhotoAlbum": is_photos_relative_path(Some(&link.relative_path)),
            "linkedAt": link.created_at,
            "linkedBy": fm.linked_by,
            "linkedById": fm.linked_by_id,
            "caption": fm.caption,
            "tags": fm.tags,
        }));
    }
    Ok(json!({ "count": linkers.len(), "linkers": linkers }))
}
