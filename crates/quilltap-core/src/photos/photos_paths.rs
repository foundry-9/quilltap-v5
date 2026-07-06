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
    fn build_photos_relative_path_prefixes() {
        assert_eq!(build_photos_relative_path("x.png"), "photos/x.png");
    }
}
