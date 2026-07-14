//! Folder-path leaves — v4 `lib/files/folder-utils.ts`.
//!
//! The `self_inventory` leaves (W4.1d): [`IMAGE_FILE_EXTENSIONS`],
//! [`is_automatic_image_path`], and [`is_os_cruft_name`]; the list-files
//! normalization half (P4.6n): [`normalize_folder_path`],
//! [`derive_folder_path_from_storage_key`], [`resolve_effective_folder_path`]; and
//! the files-family folders hierarchy + validation (P4.6ae): [`get_folder_depth`],
//! [`get_parent_path`], [`get_folder_name`], [`validate_folder_path`]. The unported
//! remainder (`listFolders`, `joinFolderPath`) has no v5 consumer yet.
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

// ---------------------------------------------------------------------------
// Folder-path hierarchy + validation (P4.6ae — the files-family folders surface)
// ---------------------------------------------------------------------------

/// The depth level of a folder path (v4 `getFolderDepth`): `0` for root, `1` for
/// a top-level folder, etc. Counts the non-empty `/`-separated segments of the
/// normalized path.
pub fn get_folder_depth(path: &str) -> usize {
    let normalized = normalize_folder_path(Some(path));
    if normalized == "/" {
        return 0;
    }
    normalized.split('/').filter(|s| !s.is_empty()).count()
}

/// The parent folder path (v4 `getParentPath`): `/documents/reports/` →
/// `/documents/`, `/documents/` → `/`, `/` → `/`. Operates over the normalized
/// path: drop the trailing `/`, find the last `/`, and return up to and including
/// it; a last-slash at index 0 (or none) means the parent is root.
pub fn get_parent_path(path: &str) -> String {
    let normalized = normalize_folder_path(Some(path));
    if normalized == "/" {
        return "/".to_string();
    }
    // Remove the trailing slash (ASCII, always a char boundary), then find the
    // last remaining slash. v4's `lastSlashIndex <= 0` → root.
    let without_trailing = &normalized[..normalized.len() - 1];
    match without_trailing.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => without_trailing[..idx + 1].to_string(),
    }
}

/// The folder name — the last segment — of a path (v4 `getFolderName`):
/// `/documents/reports/` → `reports`, `/documents/` → `documents`, `/` → `""`.
pub fn get_folder_name(path: &str) -> String {
    let normalized = normalize_folder_path(Some(path));
    if normalized == "/" {
        return String::new();
    }
    let without_trailing = &normalized[..normalized.len() - 1];
    without_trailing
        .split('/')
        .rfind(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// The outcome of [`validate_folder_path`] — `Ok(())` for a valid path, or
/// `Err(message)` carrying v4's exact rejection string.
pub type FolderPathValidation = Result<(), String>;

/// Validate a folder path (v4 `validateFolderPath`): reject an empty string,
/// `..` traversal, invalid characters (`<>:"|?*` and C0 controls — slashes ARE
/// allowed here), depth > 10, or any segment longer than 100 chars. The `..` and
/// invalid-char checks run over the RAW path (before normalization), exactly like
/// v4; depth + segment-length run over the normalized form.
pub fn validate_folder_path(path: &str) -> FolderPathValidation {
    // v4: `if (!path || typeof path !== 'string')` — the empty string is the only
    // falsy value reachable with a `&str`.
    if path.is_empty() {
        return Err("Path must be a non-empty string".to_string());
    }
    if path.contains("..") {
        return Err("Path traversal (..) is not allowed".to_string());
    }
    // v4 `/[<>:"|?*\x00-\x1f]/` — note `/` and `\` are NOT rejected here.
    if path.chars().any(|c| {
        matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || ('\u{0}'..='\u{1f}').contains(&c)
    }) {
        return Err("Path contains invalid characters".to_string());
    }
    let normalized = normalize_folder_path(Some(path));
    if get_folder_depth(&normalized) > 10 {
        return Err("Path is too deeply nested (max 10 levels)".to_string());
    }
    for segment in normalized.split('/').filter(|s| !s.is_empty()) {
        // v4 `segment.length` is a UTF-16 code-unit count; use the same measure so
        // a 100-code-unit boundary matches byte-for-byte.
        if segment.encode_utf16().count() > 100 {
            return Err("Folder names must be 100 characters or less".to_string());
        }
    }
    Ok(())
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
    fn folder_depth_counts_segments() {
        assert_eq!(get_folder_depth("/"), 0);
        assert_eq!(get_folder_depth("/documents/"), 1);
        assert_eq!(get_folder_depth("/documents/reports/"), 2);
        assert_eq!(get_folder_depth("documents/reports"), 2);
    }

    #[test]
    fn parent_path_walks_up() {
        assert_eq!(get_parent_path("/documents/reports/"), "/documents/");
        assert_eq!(get_parent_path("/documents/"), "/");
        assert_eq!(get_parent_path("/"), "/");
        // Un-normalized input is normalized first.
        assert_eq!(get_parent_path("/a/b/c"), "/a/b/");
    }

    #[test]
    fn folder_name_last_segment() {
        assert_eq!(get_folder_name("/documents/reports/"), "reports");
        assert_eq!(get_folder_name("/documents/"), "documents");
        assert_eq!(get_folder_name("/"), "");
        assert_eq!(get_folder_name("docs"), "docs");
    }

    #[test]
    fn validate_folder_path_rules() {
        assert!(validate_folder_path("/documents/reports/").is_ok());
        assert!(validate_folder_path("/a b/c_d-e/").is_ok());
        assert_eq!(
            validate_folder_path("").unwrap_err(),
            "Path must be a non-empty string"
        );
        assert_eq!(
            validate_folder_path("/a/../b/").unwrap_err(),
            "Path traversal (..) is not allowed"
        );
        assert_eq!(
            validate_folder_path("/a<b/").unwrap_err(),
            "Path contains invalid characters"
        );
        // 11 levels deep → too nested.
        let deep = format!(
            "/{}/",
            (1..=11)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("/")
        );
        assert_eq!(
            validate_folder_path(&deep).unwrap_err(),
            "Path is too deeply nested (max 10 levels)"
        );
        // A 101-char segment.
        let long = format!("/{}/", "x".repeat(101));
        assert_eq!(
            validate_folder_path(&long).unwrap_err(),
            "Folder names must be 100 characters or less"
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
