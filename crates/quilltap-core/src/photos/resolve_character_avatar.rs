//! The URL half of v4 `lib/photos/resolve-character-avatar.ts` — resolve "the
//! bytes a character's avatar id points at" to a servable URL.
//!
//! After Phase 3 of the photo-albums work, `characters.defaultImageId`,
//! `characters.avatarOverrides[].imageId`, and `chats.characterAvatars[].imageId`
//! all hold `doc_mount_file_links.id` values (a vault link). Before Phase 3 they
//! held a legacy `files.id`. Both shapes must resolve until the `files` table
//! retires: [`resolve_character_avatar`] tries the vault link first, then the
//! legacy files row.
//!
//! Only the URL-returning path is ported (P4.4 unit 2's `enrichParticipantSummary`
//! and P4.6's Salon list need it); `readCharacterAvatarBuffer` (the SillyTavern
//! PNG export) is a tracked deferral to the import/export unit.

use rusqlite::Connection;

use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::files::FilesRepository;
use crate::db::DbError;
use crate::tools::photo::encode_uri;

/// Resolved avatar reference (the subset the URL path uses). `id` is the input id
/// so callers can compare against `character.defaultImageId` without a round-trip;
/// `url` is what to drop into an `<img src>`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedCharacterAvatar {
    /// The input id (vault link id or legacy file id, unchanged).
    pub id: String,
    /// Which storage layer owns the bytes.
    pub kind: AvatarKind,
    /// API URL that streams the bytes.
    pub url: String,
    /// MIME type of the bytes, when known.
    pub mime_type: Option<String>,
    /// SHA-256 of the bytes when available.
    pub sha256: Option<String>,
    /// For vault-link kind: the mount point id the link lives in.
    pub mount_point_id: Option<String>,
    /// For vault-link kind: the relative path within the mount.
    pub relative_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarKind {
    VaultLink,
    LegacyFile,
}

/// v4 `buildMountFileUrl` — the public URL the mount-point blob endpoint serves
/// for a `(mountPointId, relativePath)`.
pub fn build_mount_file_url(mount_point_id: &str, relative_path: &str) -> String {
    format!(
        "/api/v1/mount-points/{}/blobs/{}",
        mount_point_id,
        encode_uri(relative_path)
    )
}

/// v4 `buildLegacyFileUrl` — the legacy files-table URL for a file id.
pub fn build_legacy_file_url(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}")
}

fn null_if_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// v4 `resolveCharacterAvatar`: resolve a character-avatar id to its bytes' URL.
/// Tries the vault-link path first (post-Phase-3) and falls back to the legacy
/// `files` table. Returns `None` when neither lookup finds the id — callers treat
/// that as "no avatar; use whatever placeholder applies."
///
/// `main` is the MAIN-db connection (the `files` table); `mount` is the
/// mount-index connection (the vault link + its file row).
pub fn resolve_character_avatar(
    main: &Connection,
    mount: &Connection,
    id: Option<&str>,
) -> Result<Option<ResolvedCharacterAvatar>, DbError> {
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    // Path 1: vault-link id (post-Phase-3).
    let links = DocMountFileLinksRepository::new(mount);
    if let Some(link) = links.find_by_id_with_content(id)? {
        return Ok(Some(ResolvedCharacterAvatar {
            id: id.to_string(),
            kind: AvatarKind::VaultLink,
            url: build_mount_file_url(&link.mount_point_id, &link.relative_path),
            mime_type: null_if_empty(link.original_mime_type),
            sha256: null_if_empty(Some(link.sha256)),
            mount_point_id: Some(link.mount_point_id),
            relative_path: Some(link.relative_path),
        }));
    }

    // Path 2: legacy files-table id (pre-Phase-3 or just-imported).
    let files = FilesRepository::new(main);
    if let Some(file) = files.find_by_id(id)? {
        return Ok(Some(ResolvedCharacterAvatar {
            id: id.to_string(),
            kind: AvatarKind::LegacyFile,
            url: build_legacy_file_url(&file.id),
            mime_type: null_if_empty(Some(file.mime_type)),
            sha256: null_if_empty(Some(file.sha256)),
            mount_point_id: None,
            relative_path: None,
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_file_url_encodes_relative_path() {
        assert_eq!(
            build_mount_file_url("mp1", "photos/a b.webp"),
            "/api/v1/mount-points/mp1/blobs/photos/a%20b.webp"
        );
    }

    #[test]
    fn legacy_file_url() {
        assert_eq!(build_legacy_file_url("f1"), "/api/v1/files/f1");
    }
}
