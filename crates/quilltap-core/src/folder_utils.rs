//! Folder-path leaves — v4 `lib/files/folder-utils.ts`.
//!
//! Only the leaves the `self_inventory` tool consumes are ported here (W4.1d):
//! [`IMAGE_FILE_EXTENSIONS`], [`is_automatic_image_path`], and [`is_os_cruft_name`].
//! The folder-path normalization/validation half of v4's file (`normalizeFolderPath`,
//! `listFolders`, `validateFolderPath`, …) has no `self_inventory` consumer and is
//! left for a later wave.
//!
//! ## Faithful v4 shapes
//!
//! * `IMAGE_FILE_EXTENSIONS` is the exact lowercase, leading-dot list v4 exports.
//! * [`is_automatic_image_path`] matches an image file located *inside* one of the
//!   auto-generated image folders (`character-avatars` / `story-backgrounds`) by
//!   **segment** (not substring), so `my-character-avatars-notes` is NOT a match.
//! * [`is_os_cruft_name`] drops dot-files/dot-folders plus the explicit
//!   `thumbs.db` / `desktop.ini` / `__macosx` names (compared case-insensitively
//!   via `to_lowercase`, byte-identical to JS `toLowerCase` for this ASCII set).

/// Folder segments that contain auto-generated images (avatars, story backgrounds).
/// v4 `GENERATED_IMAGE_FOLDER_SEGMENTS`.
const GENERATED_IMAGE_FOLDER_SEGMENTS: &[&str] = &["character-avatars", "story-backgrounds"];

/// Common image file extensions (lowercase, with leading dot) — v4
/// `IMAGE_FILE_EXTENSIONS`, verbatim and in order.
pub const IMAGE_FILE_EXTENSIONS: &[&str] = &[
    ".webp", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".bmp", ".tiff", ".avif",
];

/// The lowercase extension of a basename, or `""` when the name has no `.`
/// (v4's `basename.includes('.') ? basename.slice(basename.lastIndexOf('.')).toLowerCase() : ''`).
/// The slice runs from the LAST `.` to the end, inclusive of the dot; `to_lowercase`
/// is byte-identical to JS `toLowerCase` for these ASCII extensions.
fn lowercase_ext(basename: &str) -> String {
    match basename.rfind('.') {
        Some(idx) => basename[idx..].to_lowercase(),
        None => String::new(),
    }
}

/// True when `basename`'s extension is one of the known image extensions
/// (v4 `isImageFileName` in `self-inventory/helpers.ts`, hoisted here so the shared
/// image-extension predicate lives with the extension table).
pub fn is_image_file_name(basename: &str) -> bool {
    let ext = lowercase_ext(basename);
    IMAGE_FILE_EXTENSIONS.contains(&ext.as_str())
}

/// True when `relative_path` is an image file located inside one of the
/// auto-generated image folders (v4 `isAutomaticImagePath`). Uses segment matching
/// (not substring) to avoid false positives on names like `my-character-avatars-notes`.
pub fn is_automatic_image_path(relative_path: &str) -> bool {
    let segments: Vec<&str> = relative_path.split('/').collect();
    let basename = segments.last().copied().unwrap_or("");
    if !is_image_file_name(basename) {
        return false;
    }
    segments
        .iter()
        .any(|seg| GENERATED_IMAGE_FOLDER_SEGMENTS.contains(seg))
}

/// True for OS-generated cruft that should never be shown to an LLM or user
/// (v4 `isOsCruftName`): any dot-prefixed name, plus the explicit
/// `thumbs.db` / `desktop.ini` / `__macosx` set (case-insensitive).
pub fn is_os_cruft_name(basename: &str) -> bool {
    if basename.starts_with('.') {
        return true;
    }
    const EXPLICIT_CRUFT: &[&str] = &["thumbs.db", "desktop.ini", "__macosx"];
    EXPLICIT_CRUFT.contains(&basename.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extensions_match() {
        assert!(is_image_file_name("avatar.webp"));
        assert!(is_image_file_name("AVATAR.PNG")); // lowercased before compare
        assert!(is_image_file_name("a.b.jpg"));
        assert!(!is_image_file_name("notes.md"));
        assert!(!is_image_file_name("noext"));
        assert!(!is_image_file_name(".hidden")); // ext is ".hidden", not an image
    }

    #[test]
    fn automatic_image_path_segment_matching() {
        assert!(is_automatic_image_path("character-avatars/x.webp"));
        assert!(is_automatic_image_path("a/story-backgrounds/b/y.png"));
        // image, but not in a generated folder
        assert!(!is_automatic_image_path("Knowledge/pic.png"));
        // generated-folder *substring*, not a whole segment → NOT a match
        assert!(!is_automatic_image_path("my-character-avatars-notes/x.png"));
        // in a generated folder, but not an image
        assert!(!is_automatic_image_path("character-avatars/readme.md"));
        // root-level image
        assert!(!is_automatic_image_path("avatar.webp"));
    }

    #[test]
    fn os_cruft_names() {
        assert!(is_os_cruft_name(".DS_Store"));
        assert!(is_os_cruft_name(".hidden"));
        assert!(is_os_cruft_name("Thumbs.db")); // case-insensitive
        assert!(is_os_cruft_name("thumbs.db"));
        assert!(is_os_cruft_name("desktop.ini"));
        assert!(is_os_cruft_name("__MACOSX"));
        assert!(!is_os_cruft_name("Knowledge.md"));
        assert!(!is_os_cruft_name("avatar.webp"));
    }
}
