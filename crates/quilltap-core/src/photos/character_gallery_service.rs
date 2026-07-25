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
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::{
    is_photos_relative_path, DocMountFileLinksRepository, LinkBlobInput,
};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::photos::keep_image_markdown::{
    basename_of_relative_path, build_kept_image_markdown, build_slug_and_filename,
    parse_kept_image_frontmatter, sha256_of_buffer, sha256_of_string, BuildKeptImageMarkdownInput,
    BuildSlugAndFilenameInput, KeptImageAttributionRole,
};
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::photos_paths::{build_photos_relative_path, PHOTOS_FOLDER};
use crate::photos::resolve_character_avatar::build_mount_file_url;
use crate::photos::save_image_to_album::{
    chunk_and_insert_extracted_text, resolve_unique_relative_path,
};

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

/// Output of the save legs (v4 `SaveToCharacterGalleryOutput`).
fn save_output(
    link_id: &str,
    mount_point_id: &str,
    relative_path: &str,
    kept_at: &str,
    sha256: &str,
) -> Value {
    json!({
        "linkId": link_id,
        "mountPointId": mount_point_id,
        "relativePath": relative_path,
        "keptAt": kept_at,
        "sha256": sha256,
    })
}

/// v4 `saveToCharacterGallery` — hard-link freshly-uploaded bytes into the
/// character's vault `photos/` folder (deduped by sha256; a re-upload of the same
/// image into the same vault is refused). `kept_at` is the injected ISO clock (v4
/// mints `new Date().toISOString()`; the differential freezes `Date` to pin it).
///
/// A WRITE — run inside `Db::write`.
#[allow(clippy::too_many_arguments)]
pub fn save_to_character_gallery(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    data: &[u8],
    filename: &str,
    mime_type: &str,
    caption: Option<&str>,
    tags: &[String],
    kept_at: &str,
) -> Result<Value, GalleryError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Err(GalleryError::CharacterNotFound);
    };
    let character_name = character.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(vault) = resolve_character_vault(mount, &character)? else {
        return Err(GalleryError::BadRequest(format!(
            "Character {character_id} has no linked database-backed vault"
        )));
    };

    if data.is_empty() {
        return Err(GalleryError::BadRequest("Uploaded image is empty".into()));
    }
    if !mime_type.starts_with("image/") {
        return Err(GalleryError::BadRequest(format!(
            "Unsupported MIME type for character gallery: {mime_type}"
        )));
    }

    let sha256 = sha256_of_buffer(data);

    // Re-upload guard: refuse a second copy of the same bytes in this character's
    // photos/ folder (v4 scans the sha's linkers for a photos-album link in this
    // vault).
    let summary = get_photo_link_summary_by_sha256(mount, &sha256)?;
    if let Some(linkers) = summary.get("linkers").and_then(Value::as_array) {
        if let Some(existing) = linkers.iter().find(|l| {
            l.get("mountPointId").and_then(Value::as_str) == Some(vault.mount_point_id.as_str())
                && is_photos_relative_path(l.get("relativePath").and_then(Value::as_str))
        }) {
            let rel = existing
                .get("relativePath")
                .and_then(Value::as_str)
                .unwrap_or("");
            return Err(GalleryError::BadRequest(format!(
                "Image already in {character_name}'s photo album at {rel}"
            )));
        }
    }

    let character_idv = character
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let markdown = build_kept_image_markdown(&BuildKeptImageMarkdownInput {
        generation_prompt: None,
        generation_revised_prompt: None,
        generation_model: None,
        scene_state: None,
        scene_state_malformed: false,
        linked_by_name: character_name,
        linked_by_id: character_idv.as_deref(),
        linked_by_role: Some(KeptImageAttributionRole::Character),
        tags,
        caption,
        kept_at,
    });
    let extracted_text_sha256 = sha256_of_string(&markdown);

    let slug = build_slug_and_filename(&BuildSlugAndFilenameInput {
        caption,
        generation_prompt: None,
        mime_type,
        kept_at,
    });
    // Prefer the uploader's filename when it carries an extension; else the slug.
    let desired_leaf = if filename.contains('.') {
        sanitize_leaf_name(filename)
    } else {
        slug.filename.clone()
    };
    let desired_path = build_photos_relative_path(&desired_leaf);
    let relative_path = resolve_unique_relative_path(mount, &vault.mount_point_id, &desired_path)?;
    let links = DocMountFileLinksRepository::new(mount);
    links.ensure_folder_path(&vault.mount_point_id, PHOTOS_FOLDER)?;

    let result = links.link_blob_content(&LinkBlobInput {
        mount_point_id: vault.mount_point_id.clone(),
        relative_path: relative_path.clone(),
        file_name: basename_of_relative_path(&relative_path),
        file_type: None,
        original_file_name: if filename.is_empty() {
            basename_of_relative_path(&relative_path)
        } else {
            filename.to_string()
        },
        original_mime_type: mime_type.to_string(),
        stored_mime_type: mime_type.to_string(),
        sha256: sha256.clone(),
        data: data.to_vec(),
        description: Some(caption.unwrap_or("").to_string()),
        conversion_status: None,
        extracted_text: Some(markdown.clone()),
        extracted_text_sha256: Some(extracted_text_sha256),
        extraction_status: Some("converted".to_string()),
    })?;

    chunk_and_insert_extracted_text(
        mount,
        &result.link_id,
        &vault.mount_point_id,
        &markdown,
        kept_at,
    )?;
    // invalidateMountPoint / emitDocumentWritten / refreshStats / enqueueEmbedding
    // are host-side recorded seams (no-op in the core).

    Ok(save_output(
        &result.link_id,
        &vault.mount_point_id,
        &relative_path,
        kept_at,
        &sha256,
    ))
}

/// v4 `saveLinkToCharacterGallery` — hard-link an EXISTING vault link's bytes
/// (read from its mount-blob) into the target character's `photos/` folder. The
/// fully-DB-resolvable save-by-id leg.
#[allow(clippy::too_many_arguments)]
pub fn save_link_to_character_gallery(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    source_link_id: &str,
    caption: Option<&str>,
    tags: &[String],
    kept_at: &str,
) -> Result<Value, GalleryError> {
    let links = DocMountFileLinksRepository::new(mount);
    let Some(source_link) = links.find_by_id_with_content(source_link_id)? else {
        return Err(GalleryError::BadRequest(format!(
            "Image link not found: {source_link_id}"
        )));
    };
    let mime_type = source_link
        .original_mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !mime_type.starts_with("image/") {
        return Err(GalleryError::BadRequest(format!(
            "Link {source_link_id} is not an image"
        )));
    }
    let buffer = DocMountBlobsRepository::new(mount).read_data_by_file_id(&source_link.file_id)?;
    let buffer = buffer.filter(|b| !b.is_empty());
    let Some(buffer) = buffer else {
        return Err(GalleryError::BadRequest(format!(
            "Image link {source_link_id} has empty bytes"
        )));
    };
    let filename = source_link
        .original_file_name
        .clone()
        .unwrap_or_else(|| source_link.file_name.clone());
    save_to_character_gallery(
        main,
        mount,
        character_id,
        &buffer,
        &filename,
        &mime_type,
        caption,
        tags,
        kept_at,
    )
}

/// v4 `sanitizeLeafName` — strip path separators + unsafe chars, collapse runs of
/// `_`, trim leading/trailing `_`/`.`; empty → `'image'`.
fn sanitize_leaf_name(name: &str) -> String {
    // basename: the last `\`/`/` segment.
    let basename = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let mut cleaned = String::with_capacity(basename.len());
    let mut prev_underscore = false;
    for ch in basename.chars() {
        let unsafe_char = matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || (ch as u32) <= 0x1f
            || (ch as u32) == 0x7f;
        if unsafe_char {
            if !prev_underscore {
                cleaned.push('_');
                prev_underscore = true;
            }
        } else {
            cleaned.push(ch);
            prev_underscore = false;
        }
    }
    let trimmed = cleaned.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "image".to_string()
    } else {
        trimmed.to_string()
    }
}
