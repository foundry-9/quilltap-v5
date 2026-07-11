//! v4 `lib/photos/character-gallery-service.ts` — the backend for the
//! `GET / POST / DELETE /api/v1/characters/[id]/photos` REST surface (the Aurora
//! `EmbeddedPhotoGallery`). Photos live in the character vault's `photos/` folder.
//!
//! This module ports the JSON legs: `listCharacterGallery` (list),
//! `removeFromCharacterGallery` (delete via the GC-safe `deleteWithGC`
//! chokepoint), and — added in the save unit — `saveToCharacterGallery` /
//! `saveFileToCharacterGallery` / `saveLinkToCharacterGallery`. The multipart
//! upload leg (raw bytes) is the quilltap-web route.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::characters_read;
use crate::db::doc_mount_file_links::{is_photos_relative_path, DocMountFileLinksRepository};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::photos::keep_image_markdown::parse_kept_image_frontmatter;
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::resolve_character_avatar::build_mount_file_url;

/// v4 `DEFAULT_LIMIT` for the gallery listing.
const DEFAULT_LIMIT: i64 = 60;

/// The error surface the routes map to responses. `CharacterNotFound` →
/// `notFound('Character')`; `BadRequest` → the message verbatim; `Db` → 500.
#[derive(Debug)]
pub enum GalleryError {
    CharacterNotFound,
    BadRequest(String),
    Db(DbError),
}
impl From<DbError> for GalleryError {
    fn from(e: DbError) -> Self {
        GalleryError::Db(e)
    }
}

/// A resolved character vault (v4 `getCharacterVaultStore` → `CharacterVaultTarget`).
pub(crate) struct CharacterVault {
    pub mount_point_id: String,
}

/// v4 `getCharacterVaultStore(characterId)` — the overlaid character →
/// `characterDocumentMountPointId` → the mount point (must be
/// `mountType='database'` + `storeType='character'`). `Ok(None)` when the
/// character has no linked database-backed vault.
pub(crate) fn resolve_character_vault(
    mount: &Connection,
    character: &Value,
) -> Result<Option<CharacterVault>, DbError> {
    let Some(mp_id) = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let Some(mp) = DocMountPointsRepository::new(mount).find_store_naming_by_id(mp_id)? else {
        return Ok(None);
    };
    if mp.mount_type != "database" || mp.store_type.as_deref() != Some("character") {
        return Ok(None);
    }
    Ok(Some(CharacterVault {
        mount_point_id: mp.id,
    }))
}

/// v4 `listCharacterGallery` — every photo in the character's vault `photos/`
/// folder, plus the historic `images/avatar.webp` portrait + `images/history/*`,
/// most-recent first. `{ entries, total, hasMore }`. An absent/broken vault →
/// `{ entries: [], total: 0, hasMore: false }`.
pub fn list_character_gallery(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, GalleryError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Err(GalleryError::CharacterNotFound);
    };
    let Some(vault) = resolve_character_vault(mount, &character)? else {
        return Ok(json!({ "entries": [], "total": 0, "hasMore": false }));
    };

    // v4: `Math.max(1, Math.min(limit ?? 60, 200))` / `Math.max(0, offset ?? 0)`.
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 200);
    let effective_offset = offset.unwrap_or(0).max(0);

    let all_links =
        DocMountFileLinksRepository::new(mount).find_by_mount_point_id(&vault.mount_point_id)?;
    let mut gallery_links: Vec<_> = all_links
        .into_iter()
        .filter(|l| {
            if is_photos_relative_path(Some(&l.relative_path)) {
                return true;
            }
            let lower = l.relative_path.to_lowercase();
            lower == "images/avatar.webp" || lower.starts_with("images/history/")
        })
        .collect();

    // Most-recent first: `b.createdAt.localeCompare(a.createdAt)` — a stable
    // desc string compare on the ISO createdAt.
    gallery_links.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let total = gallery_links.len() as i64;
    let start = effective_offset.min(total) as usize;
    let end = (effective_offset + effective_limit).min(total) as usize;
    let page = &gallery_links[start..end];

    let mut entries: Vec<Value> = Vec::with_capacity(page.len());
    for link in page {
        let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
        let link_summary = get_photo_link_summary_by_sha256(mount, &link.sha256)?;
        // caption = meta.caption ?? (description.trim() nonempty ? description : null).
        let caption = match meta.caption {
            Some(c) => Value::String(c),
            None => match link.description.as_deref() {
                Some(d) if !crate::jsstr::js_trim(d).is_empty() => Value::String(d.to_string()),
                _ => Value::Null,
            },
        };
        entries.push(json!({
            "linkId": link.id,
            "mountPointId": vault.mount_point_id,
            "relativePath": link.relative_path,
            "fileName": link.file_name,
            "blobUrl": build_mount_file_url(&vault.mount_point_id, &link.relative_path),
            "mimeType": link.original_mime_type,
            "sha256": link.sha256,
            "fileSizeBytes": link.file_size_bytes,
            "keptAt": link.created_at,
            "caption": caption,
            "tags": meta.tags,
            "linkSummary": link_summary,
        }));
    }

    let has_more = effective_offset + (page.len() as i64) < total;
    Ok(json!({ "entries": entries, "total": total, "hasMore": has_more }))
}

/// v4 `removeFromCharacterGallery` — remove a photo from the character's gallery
/// via the GC-safe `deleteWithGC` chokepoint. If the link was the character's
/// `defaultImageId` (or appeared in `avatarOverrides[].imageId`), those pointers
/// are nulled/filtered first. Returns `(deleted, fileGC)`; `(false, false)` when
/// the link is absent or belongs to another mount (the route maps to 404).
///
/// A WRITE — run inside `Db::write` (both `main` + `mount` are writable).
pub fn remove_from_character_gallery(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    link_id: &str,
) -> Result<(bool, bool), GalleryError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Err(GalleryError::CharacterNotFound);
    };
    let Some(vault) = resolve_character_vault(mount, &character)? else {
        return Err(GalleryError::BadRequest(format!(
            "Character {character_id} has no linked database-backed vault"
        )));
    };

    let links = DocMountFileLinksRepository::new(mount);
    let Some(link) = links.find_by_id_with_content(link_id)? else {
        return Ok((false, false));
    };
    if link.mount_point_id != vault.mount_point_id {
        return Ok((false, false));
    }

    // Clear avatar pointers before the link disappears (v4 nulls defaultImageId
    // and filters avatarOverrides).
    let mut patch = serde_json::Map::new();
    if character.get("defaultImageId").and_then(Value::as_str) == Some(link_id) {
        patch.insert("defaultImageId".into(), Value::Null);
    }
    let overrides: Vec<Value> = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let filtered: Vec<Value> = overrides
        .iter()
        .filter(|o| o.get("imageId").and_then(Value::as_str) != Some(link_id))
        .cloned()
        .collect();
    if filtered.len() != overrides.len() {
        patch.insert("avatarOverrides".into(), Value::Array(filtered));
    }
    if !patch.is_empty() {
        crate::db::vault_character_update::update_character(main, mount, character_id, &patch)?;
    }

    let file_gc = links.delete_with_gc(link_id)?;
    Ok((true, file_gc))
}
