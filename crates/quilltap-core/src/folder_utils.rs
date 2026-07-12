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

// ---------------------------------------------------------------------------
// Folder-path normalization + derivation (P4.6n — the list-files legacy branch)
// ---------------------------------------------------------------------------

/// Collapse runs of `/` into a single `/` — the effect of v4's `replace(/\/+/g, '/')`.
fn collapse_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for c in s.chars() {
        if c == '/' {
            if !prev_slash {
                out.push('/');
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    out
}

/// Normalize a folder path to v4's canonical form (v4 `normalizeFolderPath`):
/// starts + ends with `/`, no `..` segments, no duplicate slashes; `/` for
/// empty/invalid. The `..`-strip runs over the ORIGINAL (untrimmed) path, exactly
/// like v4's `path.replace(/\.\./g, '')`.
pub fn normalize_folder_path(path: Option<&str>) -> String {
    // v4: `if (!path || path.trim() === '') return '/';`
    match path {
        Some(s) if !s.trim().is_empty() => {}
        _ => return "/".to_string(),
    }
    let raw = path.unwrap();

    // Remove any ".." segments for security (over the original, untrimmed string).
    let mut normalized = raw.replace("..", "");
    // Ensure starts with /.
    if !normalized.starts_with('/') {
        normalized = format!("/{normalized}");
    }
    // Remove duplicate slashes.
    normalized = collapse_slashes(&normalized);
    // Ensure ends with / (unless it's just "/").
    if normalized != "/" && !normalized.ends_with('/') {
        normalized.push('/');
    }
    // If we ended up with just slashes or empty after cleaning, return root.
    if normalized.is_empty() || normalized.replace('/', "").is_empty() {
        return "/".to_string();
    }
    normalized
}

/// Derive a folderPath from a storageKey (v4 `deriveFolderPathFromStorageKey`).
/// Storage keys are `<scope>/[folder1/.../folderN/]<filename>`: the first segment
/// is the scope id, the last is the filename, the middle segments form the folder.
pub fn derive_folder_path_from_storage_key(storage_key: Option<&str>) -> String {
    // v4: `if (!storageKey) return '/';` — null/undefined/'' all fall through.
    let key = match storage_key {
        Some(s) if !s.is_empty() => s,
        _ => return "/".to_string(),
    };
    let parts: Vec<&str> = key.split('/').collect();
    if parts.len() > 2 {
        format!("/{}/", parts[1..parts.len() - 1].join("/"))
    } else {
        "/".to_string()
    }
}

/// Resolve the effective folderPath for a file (v4 `resolveEffectiveFolderPath`):
/// prefer a stored non-`/` folderPath (normalized); else derive from the
/// storageKey; else fall back to `folderPath || '/'`.
pub fn resolve_effective_folder_path(
    folder_path: Option<&str>,
    storage_key: Option<&str>,
) -> String {
    // v4: `if (folderPath && folderPath !== '/') return normalizeFolderPath(folderPath);`
    if let Some(fp) = folder_path {
        if !fp.is_empty() && fp != "/" {
            return normalize_folder_path(Some(fp));
        }
    }
    let derived = derive_folder_path_from_storage_key(storage_key);
    if derived != "/" {
        return derived;
    }
    // v4: `return folderPath || '/';`
    match folder_path {
        Some(fp) if !fp.is_empty() => fp.to_string(),
        _ => "/".to_string(),
    }
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

    #[test]
    fn normalize_folder_path_shapes() {
        assert_eq!(normalize_folder_path(None), "/");
        assert_eq!(normalize_folder_path(Some("")), "/");
        assert_eq!(normalize_folder_path(Some("   ")), "/");
        assert_eq!(normalize_folder_path(Some("/")), "/");
        assert_eq!(normalize_folder_path(Some("docs")), "/docs/");
        assert_eq!(
            normalize_folder_path(Some("/docs/reports")),
            "/docs/reports/"
        );
        assert_eq!(
            normalize_folder_path(Some("//docs//reports//")),
            "/docs/reports/"
        );
        // `..` segments stripped (leaving the surrounding slashes to collapse).
        assert_eq!(normalize_folder_path(Some("/a/../b/")), "/a/b/");
    }

    #[test]
    fn derive_folder_path_from_storage_key_middle_segments() {
        assert_eq!(derive_folder_path_from_storage_key(None), "/");
        assert_eq!(derive_folder_path_from_storage_key(Some("")), "/");
        assert_eq!(
            derive_folder_path_from_storage_key(Some("_general/story-backgrounds/image.png")),
            "/story-backgrounds/"
        );
        assert_eq!(
            derive_folder_path_from_storage_key(Some("_general/image.png")),
            "/"
        );
        assert_eq!(
            derive_folder_path_from_storage_key(Some("project123/docs/reports/file.md")),
            "/docs/reports/"
        );
    }

    #[test]
    fn resolve_effective_folder_path_precedence() {
        // Stored non-'/' folderPath wins (normalized).
        assert_eq!(
            resolve_effective_folder_path(Some("docs"), Some("user/x/y.png")),
            "/docs/"
        );
        // Empty/'/' folderPath → derive from storageKey.
        assert_eq!(
            resolve_effective_folder_path(Some("/"), Some("user/sub/y.png")),
            "/sub/"
        );
        assert_eq!(
            resolve_effective_folder_path(None, Some("user/sub/y.png")),
            "/sub/"
        );
        // Neither yields a subfolder → folderPath || '/'.
        assert_eq!(resolve_effective_folder_path(None, Some("user/y.png")), "/");
        assert_eq!(
            resolve_effective_folder_path(Some("/"), Some("user/y.png")),
            "/"
        );
    }
}
