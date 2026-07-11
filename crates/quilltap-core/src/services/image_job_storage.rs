//! Storage bridges for the avatar / story-background job handlers (v4
//! `lib/file-storage/character-vault-bridge.ts` + `lantern-store-bridge.ts`, the
//! database-backed subset).
//!
//! Both bridges land generated image bytes in a database-backed document store
//! via the ported blob storage pipeline (v4 `storeMountFile`'s binary branch):
//! `normaliseBlobRelativePath` → `resolveUniqueRelativePath` (unique-suffix) →
//! `ensureFolderPath` → `linkBlobContent`, returning the same
//! `mount-blob:{mountPointId}:{blobId}` storage-key shim the file row records.
//!
//! The WebP transcode (v4 `transcodeToWebP` inside `storeMountFile`) is an
//! injected host seam: the handler already transcoded the bytes once via the
//! `ImageTranscoder` seam, so the bridge stores those bytes directly (in the
//! differential both transcode steps are pass-throughs — identical bytes/mime,
//! so `normaliseBlobRelativePath` is a no-op on the already-final path).
//!
//! Filesystem/Obsidian mounts and the `fileStorageManager.uploadFile` PROJECT
//! branches are NOT handled here — those are the host FsSeam (the handlers keep
//! the database-backed branches primary).

use rusqlite::Connection;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::{
    normalise_relative_path, DocMountFileLinksRepository, LinkBlobInput,
};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;

/// The result of a bridge write (v4 `Write*Result`, the subset the handlers use).
#[derive(Clone, Debug)]
pub struct WrittenImage {
    pub storage_key: String,
    #[allow(dead_code)]
    pub mount_point_id: String,
    pub blob_id: String,
    #[allow(dead_code)]
    pub relative_path: String,
    pub stored_mime_type: String,
    pub size_bytes: usize,
    #[allow(dead_code)]
    pub sha256: String,
}

/// v4 `getCharacterVaultStore`: the database-backed character vault mount id for
/// a character, or `None` (no vault, a filesystem/obsidian mount, or a non-
/// `character` store). Fails soft. Reads the slim `characterDocumentMountPointId`
/// pointer (unaffected by the overlay) + the mount row's type.
pub fn resolve_character_vault_mount(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Option<String> {
    let character = crate::db::characters_read::find_by_id_raw(main, character_id)
        .ok()
        .flatten()?;
    let mount_point_id = character
        .get("characterDocumentMountPointId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let repo = DocMountPointsRepository::new(mount);
    let row = repo
        .find_by_id_for_docedit(&mount_point_id)
        .ok()
        .flatten()?;
    if row.mount_type != "database" {
        return None;
    }
    let store_type = repo.find_store_type_by_id(&mount_point_id).ok().flatten()?;
    if store_type != "character" {
        return None;
    }
    Some(mount_point_id)
}

/// v4 `getLanternBackgroundsStore`: the global Lantern Backgrounds mount id, or
/// `None` (unprovisioned, deleted, or non-database). Fails soft.
pub fn resolve_lantern_backgrounds_mount(main: &Connection, mount: &Connection) -> Option<String> {
    let id = crate::db::instance_settings::get_lantern_backgrounds_mount_point_id(main)
        .ok()
        .flatten()?;
    let row = DocMountPointsRepository::new(mount)
        .find_by_id_for_docedit(&id)
        .ok()
        .flatten()?;
    if row.mount_type != "database" {
        return None;
    }
    Some(id)
}

/// v4 `writeCharacterAvatarToVault({ kind: 'history' })`: append a generated
/// avatar under `images/history/` with unique-suffix collision bumping. Throws
/// (`Err`) when the character has no linked database-backed vault. The bytes are
/// the already-transcoded (WebP-seam) image; `content_type` is their mime.
pub fn write_character_avatar_to_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    filename: &str,
    content: &[u8],
    content_type: &str,
    description: Option<&str>,
) -> Result<WrittenImage, DbError> {
    let Some(mount_point_id) = resolve_character_vault_mount(main, mount, character_id) else {
        return Err(DbError::Key(format!(
            "Character {character_id} has no linked database-backed vault"
        )));
    };
    let safe = sanitize_leaf_name(filename);
    let desired = format!("images/history/{safe}");
    store_blob_to_mount(
        mount,
        &mount_point_id,
        &desired,
        content,
        content_type,
        &safe,
        description,
    )
}

/// The result of a main-avatar vault write (v4 `WriteAvatarResult`, the subset
/// the ST import consumes).
#[derive(Clone, Debug)]
pub struct AvatarWrite {
    /// `doc_mount_file_links.id` — what the character's `defaultImageId` is set to.
    pub link_id: String,
    pub stored_mime_type: String,
    pub sha256: String,
    #[allow(dead_code)]
    pub size_bytes: usize,
}

/// v4 `writeCharacterAvatarToVault({ kind: 'main' })`: overwrite the canonical
/// main avatar at `images/avatar.webp` in place (delete-then-insert — the
/// existing link is `deleteWithGC`'d so its blob is reclaimed when it was the
/// last reference, then the replacement is linked). Image MIME types the codec
/// can decode are transcoded to WebP; everything else is stored as-is (the same
/// `transcodeToWebP` policy the project bridge uses — the WebP bytes are the
/// declared `[[w4-9c-image-job-handlers]]` codec seam). Throws (`Err`) when the
/// character has no linked database-backed vault.
///
/// A WRITE — run inside `Db::write` (both `main` + `mount` writable).
#[allow(clippy::too_many_arguments)]
pub fn write_main_avatar_to_vault(
    main: &Connection,
    mount: &Connection,
    codec: &dyn crate::services::file_storage::PixelCodec,
    character_id: &str,
    filename: &str,
    content: &[u8],
    content_type: &str,
    description: Option<&str>,
) -> Result<AvatarWrite, DbError> {
    let Some(mount_point_id) = resolve_character_vault_mount(main, mount, character_id) else {
        return Err(DbError::Key(format!(
            "Character {character_id} has no linked database-backed vault"
        )));
    };
    // transcodeToWebP (the codec seam): PNG/JPEG/… → WebP, else pass-through.
    let transcoded = crate::services::file_storage::transcode_to_webp(
        codec,
        content,
        content_type,
        crate::services::file_storage::TRANSCODE_WEBP_QUALITY,
    );

    const MAIN_AVATAR_PATH: &str = "images/avatar.webp";
    let links = DocMountFileLinksRepository::new(mount);
    links.ensure_folder_path(&mount_point_id, "images")?;
    // Drop any existing link at the canonical main path (GC-safe — takes the
    // blob when it was the last reference).
    if let Some(existing) = links.find_by_mount_point_and_path(&mount_point_id, MAIN_AVATAR_PATH)? {
        links.delete_with_gc(&existing.id)?;
    }

    let safe_original_name = sanitize_leaf_name(filename);
    let written = links.link_blob_content(&LinkBlobInput {
        mount_point_id: mount_point_id.clone(),
        relative_path: MAIN_AVATAR_PATH.to_string(),
        file_name: "avatar.webp".to_string(),
        // v4 omits fileType here (the repo defaults it, like the other bridges).
        file_type: None,
        original_file_name: safe_original_name,
        original_mime_type: content_type.to_string(),
        stored_mime_type: transcoded.stored_mime_type.clone(),
        sha256: transcoded.sha256.clone(),
        data: transcoded.data.clone(),
        // v4: `description: input.description ?? ''`.
        description: Some(description.unwrap_or("").to_string()),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
    })?;

    Ok(AvatarWrite {
        link_id: written.link_id,
        stored_mime_type: transcoded.stored_mime_type,
        sha256: transcoded.sha256,
        size_bytes: transcoded.size_bytes,
    })
}

/// v4 `writeLanternBackgroundToMountStore`: write into the Lantern Backgrounds
/// mount under `<subfolder>/`. Throws (`Err`) when the mount is unprovisioned.
pub fn write_lantern_background_to_mount_store(
    main: &Connection,
    mount: &Connection,
    filename: &str,
    content: &[u8],
    content_type: &str,
    subfolder: &str,
    description: Option<&str>,
) -> Result<WrittenImage, DbError> {
    let Some(mount_point_id) = resolve_lantern_backgrounds_mount(main, mount) else {
        return Err(DbError::Key(
            "Lantern Backgrounds mount has not been provisioned".to_string(),
        ));
    };
    let safe = sanitize_leaf_name(filename);
    let desired = format!("{subfolder}/{safe}");
    store_blob_to_mount(
        mount,
        &mount_point_id,
        &desired,
        content,
        content_type,
        &safe,
        description,
    )
}

/// v4 `storeMountFile`'s database binary-blob branch (the `unique-suffix`
/// collision strategy, `transcodeImages` being a pass-through here — the caller
/// already transcoded). Reproduces: normalise → transcode(pass-through) →
/// `normaliseBlobRelativePath` → `resolveUniqueRelativePath` → `ensureFolderPath`
/// → `linkBlobContent`.
#[allow(clippy::too_many_arguments)]
fn store_blob_to_mount(
    mount: &Connection,
    mount_point_id: &str,
    desired_relative_path: &str,
    content: &[u8],
    content_type: &str,
    original_file_name: &str,
    description: Option<&str>,
) -> Result<WrittenImage, DbError> {
    let rel = normalise_relative_path(desired_relative_path)?;
    // Pass-through transcode: storedMimeType == originalMimeType, sha over bytes.
    let stored_mime_type = content_type.to_string();
    let sha256 = sha256_of_buffer(content);

    let mut final_path = normalise_blob_relative_path(&rel, &stored_mime_type);
    // `unique-suffix` collision strategy.
    final_path = resolve_unique_relative_path(mount, mount_point_id, &final_path)?;

    let links = DocMountFileLinksRepository::new(mount);
    // ensureFolderPath when the path has a real parent folder.
    if let Some(dir) = posix_dirname(&final_path) {
        links.ensure_folder_path(mount_point_id, &dir)?;
    }

    let file_name = posix_basename(&final_path);
    let written = links.link_blob_content(&LinkBlobInput {
        mount_point_id: mount_point_id.to_string(),
        relative_path: final_path.clone(),
        file_name,
        // v4 `detectBlobFileType(finalPath)` → `blob` for a .png/.webp image.
        file_type: Some(detect_blob_file_type(&final_path)),
        original_file_name: original_file_name.to_string(),
        original_mime_type: content_type.to_string(),
        stored_mime_type: stored_mime_type.clone(),
        sha256: sha256.clone(),
        data: content.to_vec(),
        description: description.map(str::to_string),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
    })?;

    Ok(WrittenImage {
        storage_key: format!("mount-blob:{mount_point_id}:{}", written.blob_id),
        mount_point_id: mount_point_id.to_string(),
        blob_id: written.blob_id,
        relative_path: final_path,
        stored_mime_type,
        size_bytes: content.len(),
        sha256,
    })
}

/// v4 `detectBlobFileType`: `.pdf`/`.docx` else `blob`.
fn detect_blob_file_type(relative_path: &str) -> String {
    let ext = path_extname(relative_path).to_lowercase();
    match ext.as_str() {
        ".pdf" => "pdf".to_string(),
        ".docx" => "docx".to_string(),
        _ => "blob".to_string(),
    }
}

/// v4 `normaliseBlobRelativePath`: force a `.webp` extension when the stored mime
/// is `image/webp`.
fn normalise_blob_relative_path(relative_path: &str, stored_mime_type: &str) -> String {
    if stored_mime_type != "image/webp" {
        return relative_path.to_string();
    }
    if relative_path.to_lowercase().ends_with(".webp") {
        return relative_path.to_string();
    }
    let last_dot = relative_path.rfind('.');
    let last_slash = relative_path.rfind('/');
    match last_dot {
        Some(dot) if last_slash.map(|s| dot > s).unwrap_or(true) => {
            format!("{}.webp", &relative_path[..dot])
        }
        _ => format!("{relative_path}.webp"),
    }
}

/// v4 `resolveUniqueRelativePath` (`unique-suffix`): a free path under a mount,
/// bumping `(2)`, `(3)`, … up to 999. The sha1-tagged fallback past 999 is a
/// documented non-deterministic seam (never hit by the corpus — the differential
/// pins the clock + generates one image).
fn resolve_unique_relative_path(
    mount: &Connection,
    mount_point_id: &str,
    desired: &str,
) -> Result<String, DbError> {
    let blobs = DocMountBlobsRepository::new(mount);
    if blobs
        .find_by_mount_point_and_path(mount_point_id, desired)?
        .is_none()
    {
        return Ok(desired.to_string());
    }
    let dir = posix_dirname(desired);
    let (stem, ext) = split_stem_ext(desired);
    let prefix = match &dir {
        Some(d) => format!("{d}/"),
        None => String::new(),
    };
    for attempt in 2..=999u32 {
        let candidate = format!("{prefix}{stem} ({attempt}){ext}");
        if blobs
            .find_by_mount_point_and_path(mount_point_id, &candidate)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    // Non-deterministic seam past 999 — never reached by the corpus.
    Ok(format!("{prefix}{stem}-dup{ext}"))
}

/// v4 `sanitizeLeafName`: strip path components (split on `\`/`/`, take last),
/// replace `UNSAFE_LEAF_CHARS` (`/[\/\\:*?"<>|\x00-\x1f\x7f]/`) with `_`, collapse
/// `_{2,}` → `_`, trim leading/trailing `_`/`.`, `'unnamed'` when empty.
fn sanitize_leaf_name(filename: &str) -> String {
    let basename = filename.rsplit(['\\', '/']).next().unwrap_or(filename);
    let mut safe: String = basename
        .chars()
        .map(|c| if is_unsafe_leaf_char(c) { '_' } else { c })
        .collect();
    while safe.contains("__") {
        safe = safe.replace("__", "_");
    }
    let trimmed = safe.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// v4 `UNSAFE_LEAF_CHARS = /[\/\\:*?"<>|\x00-\x1f\x7f]/`.
fn is_unsafe_leaf_char(c: char) -> bool {
    matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        || (c as u32) <= 0x1F
        || (c as u32) == 0x7F
}

/// v4 `sha256OfBuffer`: lowercase hex SHA-256.
fn sha256_of_buffer(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// POSIX `path.dirname` for a blob path (`None` when there's no folder segment —
/// v4's `folderDir !== '.' && folderDir !== ''` gate).
fn posix_dirname(path: &str) -> Option<String> {
    match path.rfind('/') {
        Some(i) if i > 0 => Some(path[..i].to_string()),
        _ => None,
    }
}

/// POSIX `path.basename`.
fn posix_basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// POSIX `path.extname` (the last `.segment`, incl. the dot, of the basename;
/// `""` when the basename has no dot or is a dotfile).
fn path_extname(path: &str) -> String {
    let base = posix_basename(path);
    match base.rfind('.') {
        Some(i) if i > 0 => base[i..].to_string(),
        _ => String::new(),
    }
}

/// `(stem, ext)` split of a path's basename (POSIX), preserving the directory
/// prefix separately (handled by the caller).
fn split_stem_ext(path: &str) -> (String, String) {
    let base = posix_basename(path);
    match base.rfind('.') {
        Some(i) if i > 0 => (base[..i].to_string(), base[i..].to_string()),
        _ => (base, String::new()),
    }
}
