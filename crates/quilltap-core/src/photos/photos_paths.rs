//! Photo album path helpers — port of v4 `lib/photos/photos-paths.ts`.
//!
//! A character's photo album is a `photos/` subfolder inside their character
//! vault, not a separate mount point. Centralising the folder name keeps the LLM
//! tools, the chat GET resolver, and any future migration in lockstep.

/// The photo-album folder name inside a vault.
pub const PHOTOS_FOLDER: &str = "photos";

/// Compose a relative path inside a vault's `photos/` folder (v4
/// `buildPhotosRelativePath`).
pub fn build_photos_relative_path(filename: &str) -> String {
    format!("{PHOTOS_FOLDER}/{filename}")
}

/// True when a `doc_mount_file_links.relativePath` lives in a `photos/` folder
/// (v4 `isPhotosRelativePath`). Case-insensitive, matching the rest of the
/// mount-index lookups: the POSIX dirname is lower-cased and compared to
/// `photos` or a `photos/…` prefix.
pub fn is_photos_relative_path(relative_path: Option<&str>) -> bool {
    let Some(rel) = relative_path else {
        return false;
    };
    if rel.is_empty() {
        return false;
    }
    let folder = posix_dirname(rel).to_lowercase();
    folder == PHOTOS_FOLDER || folder.starts_with(&format!("{PHOTOS_FOLDER}/"))
}

/// Where `migrate-character-avatars-to-vaults-v1` parked a pre-existing portrait
/// (v4 `LEGACY_MAIN_AVATAR_PATH`).
pub const LEGACY_MAIN_AVATAR_PATH: &str = "images/avatar.webp";

/// True when a vault link is part of a character's **photo album** — what the
/// Aurora gallery tab shows and what the character-detail `photos` figure counts
/// (v4 `isCharacterAlbumRelativePath`, `4dcbe0d21`).
///
/// One predicate, two readers ([`crate::photos::character_gallery_service::
/// list_character_gallery`] and the `?action=stats` figure in
/// `api::characters::character_stats`), because a second copy is how the grid
/// and the count come to disagree — which is exactly what v4 had, and what this
/// replaces: both sites previously hand-rolled
/// `is_photos_relative_path || "images/avatar.webp" || starts_with("images/history/")`.
///
/// `images/history/` is deliberately excluded: those are avatar rolls — the
/// configuration cache's working stock — and they have their own section, fed by
/// [`crate::photos::avatar_rolls_service`]. The canonical `images/avatar.webp`
/// portrait *is* album material, so a character who has never kept anything
/// still has a face on the page.
pub fn is_character_album_relative_path(relative_path: Option<&str>) -> bool {
    let Some(rel) = relative_path.filter(|s| !s.is_empty()) else {
        return false;
    };
    if is_photos_relative_path(Some(rel)) {
        return true;
    }
    rel.to_lowercase() == LEGACY_MAIN_AVATAR_PATH
}

/// POSIX `path.dirname` (Node semantics): the substring up to the last `/`. A path
/// with no `/` → `"."`; a trailing-slash path drops it first. Matches v4's
/// `path.posix.dirname` for the relative paths this module handles.
fn posix_dirname(p: &str) -> String {
    // Node strips a single trailing '/' (except the root) before taking dirname.
    let trimmed = if p.len() > 1 {
        p.strip_suffix('/').unwrap_or(p)
    } else {
        p
    };
    match trimmed.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(idx) => trimmed[..idx].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_relative_path_detection() {
        assert!(is_photos_relative_path(Some("photos/a.webp")));
        assert!(is_photos_relative_path(Some("Photos/a.webp"))); // case-insensitive
        assert!(is_photos_relative_path(Some("photos/sub/a.webp")));
        assert!(!is_photos_relative_path(Some("a.webp"))); // dirname "."
        assert!(!is_photos_relative_path(Some("other/a.webp")));
        assert!(!is_photos_relative_path(Some("photosx/a.webp")));
        assert!(!is_photos_relative_path(None));
        assert!(!is_photos_relative_path(Some("")));
    }

    #[test]
    fn character_album_membership() {
        // The `photos/` arm delegates (every `is_photos_relative_path` case).
        assert!(is_character_album_relative_path(Some("photos/a.webp")));
        assert!(is_character_album_relative_path(Some("Photos/sub/a.webp")));
        // The canonical portrait, case-insensitively (v4 lower-cases the whole
        // path and compares).
        assert!(is_character_album_relative_path(Some("images/avatar.webp")));
        assert!(is_character_album_relative_path(Some("Images/Avatar.WEBP")));
        // The one behaviour change of v4 `4dcbe0d21`: an avatar roll is NOT
        // album material any more.
        assert!(!is_character_album_relative_path(Some(
            "images/history/roll-01.webp"
        )));
        assert!(!is_character_album_relative_path(Some(
            "IMAGES/HISTORY/roll-01.webp"
        )));
        // Neighbours that must not be swept in by a loose prefix test.
        assert!(!is_character_album_relative_path(Some("images/other.webp")));
        assert!(!is_character_album_relative_path(Some(
            "images/avatar.webp.bak"
        )));
        assert!(!is_character_album_relative_path(None));
        assert!(!is_character_album_relative_path(Some("")));
    }

    #[test]
    fn build_photos_relative_path_prefixes() {
        assert_eq!(build_photos_relative_path("x.png"), "photos/x.png");
    }
}
