//! Wardrobe folders — the rules for the operator's filing inside a tier's
//! `Wardrobe/` folder (v4 `lib/wardrobe/folders.ts`, `4a38222e6`; P4.D279).
//!
//! A folder is a path relative to `Wardrobe/`: `''` for the root, otherwise
//! `Seg/Seg/Seg` with no leading or trailing slash. It is the file's path and
//! nothing else — derived from `relativePath` on read, written back as the
//! path on projection, never stored in frontmatter. Folders organise what the
//! operator sees; they carry no meaning for what a character can wear.
//!
//! Pure — no I/O, no logger. The one impure neighbour it leans on is
//! [`sanitize_file_name`] (v4 moved it to the client-safe
//! `lib/mount-index/sanitize-file-name.ts`; v5 keeps the byte twin where it
//! always was, in `vault_overlay`).
//!
//! Every `toLowerCase()` is `str::to_lowercase` (byte-identical for JS per
//! Phase 1), every `.trim()` is [`crate::jsstr::js_trim`], and every `.length`
//! / `.slice` counts UTF-16 code units as JS does.

use crate::jsstr::js_trim;
use crate::vault_overlay::sanitize_file_name;

/// The deepest a wardrobe folder may nest (v4 `WARDROBE_FOLDER_MAX_DEPTH`).
pub const WARDROBE_FOLDER_MAX_DEPTH: usize = 8;

/// Segments a wardrobe folder may never use, compared case-insensitively
/// (v4 `WARDROBE_RESERVED_FOLDER_SEGMENTS`). `Wardrobe/images/<itemId>/` holds
/// item pictures.
pub const WARDROBE_RESERVED_FOLDER_SEGMENTS: &[&str] = &["images"];

/// The folder every tier's items live under, as a path prefix.
const WARDROBE_ROOT: &str = "Wardrobe";

/// v4 `WardrobeFolderPathSchema`'s refine message — `Wardrobe folder must be
/// a clean path of at most ${WARDROBE_FOLDER_MAX_DEPTH} segments, none named
/// "images"`, interpolated.
pub const FOLDER_SCHEMA_MESSAGE: &str =
    "Wardrobe folder must be a clean path of at most 8 segments, none named \"images\"";

/// A folder path that breaks the rules (v4 `WardrobeFolderPathError`, a 400
/// at the route). The payload is v4's sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeFolderPathError(pub String);

impl std::fmt::Display for WardrobeFolderPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WardrobeFolderPathError {}

/// A folder as the browser and the folder routes see it (v4 `WardrobeFolder`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WardrobeFolder {
    /// Path relative to `Wardrobe/`, e.g. `Formal/Evening`. Never `''`.
    pub path: String,
    /// The last segment.
    pub name: String,
    /// Non-archived items directly inside (not in subfolders).
    #[serde(rename = "itemCount")]
    pub item_count: usize,
}

/// v4 `WardrobeFolderNotFoundError`'s message.
pub fn not_found_message(folder: &str) -> String {
    format!("Wardrobe folder not found: {folder}")
}

/// v4 `WardrobeFolderConflictError`'s default message.
pub fn conflict_default_message(folder: &str) -> String {
    format!("A wardrobe folder named \"{folder}\" already exists")
}

/// v4 `WardrobeItemNotInLocationError`'s message.
pub fn not_in_location_message(item_ids: &[String]) -> String {
    format!(
        "Not in this wardrobe (shared items are filed where they live): {}",
        item_ids.join(", ")
    )
}

/// JS `.length` — UTF-16 code units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JS `s.slice(n)` for a code-unit offset `n` (`n` never lands inside a
/// surrogate pair here: every caller's `n` is the length of an ASCII prefix or
/// of a whole-character prefix of `s`).
fn utf16_slice_from(s: &str, n: usize) -> &str {
    let mut units = 0usize;
    for (i, c) in s.char_indices() {
        if units >= n {
            return &s[i..];
        }
        units += c.len_utf16();
    }
    ""
}

fn is_reserved_segment(segment: &str) -> bool {
    let lower = segment.to_lowercase();
    WARDROBE_RESERVED_FOLDER_SEGMENTS
        .iter()
        .any(|reserved| *reserved == lower)
}

/// v4's `input.split(/[\\/]/).map(trim).filter(non-empty)`.
fn raw_segments(input: &str) -> Vec<&str> {
    input
        .split(['\\', '/'])
        .map(js_trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Normalise an operator-typed folder path (v4 `normalizeWardrobeFolderPath`):
/// trim, split on `/` (and `\`), drop empty segments, sanitise each with the
/// item-file rule. Errs for `.` / `..` / a reserved segment, or more than
/// [`WARDROBE_FOLDER_MAX_DEPTH`] segments. `None` / `''` (or whitespace) is the
/// root.
pub fn normalize_wardrobe_folder_path(
    input: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    let Some(input) = input else {
        return Ok(String::new());
    };
    let mut out: Vec<String> = Vec::new();
    for raw in raw_segments(input) {
        if raw == "." || raw == ".." {
            return Err(WardrobeFolderPathError(format!(
                "A wardrobe folder name may not be \"{raw}\""
            )));
        }
        let segment = sanitize_file_name(raw);
        if is_reserved_segment(&segment) {
            return Err(WardrobeFolderPathError(format!(
                "\"{segment}\" is reserved for item pictures and cannot be a wardrobe folder"
            )));
        }
        out.push(segment);
    }
    if out.len() > WARDROBE_FOLDER_MAX_DEPTH {
        return Err(WardrobeFolderPathError(format!(
            "Wardrobe folders nest at most {WARDROBE_FOLDER_MAX_DEPTH} deep"
        )));
    }
    Ok(out.join("/"))
}

/// Normalise a folder an existing item already carries (v4
/// `normalizeWardrobeFolderPathOrRoot`): its normal form, or the root when it
/// can't be one.
pub fn normalize_wardrobe_folder_path_or_root(input: Option<&str>) -> String {
    normalize_wardrobe_folder_path(input).unwrap_or_default()
}

/// Normalise a folder *name* — one segment, as a rename takes it (v4
/// `normalizeWardrobeFolderName`). Errs for an empty name or one with a `/`.
pub fn normalize_wardrobe_folder_name(
    input: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    if let Some(s) = input {
        if s.contains(['\\', '/']) {
            return Err(WardrobeFolderPathError(
                "A folder name may not contain \"/\"".to_string(),
            ));
        }
    }
    let name = normalize_wardrobe_folder_path(input)?;
    if name.is_empty() {
        return Err(WardrobeFolderPathError("A folder needs a name".to_string()));
    }
    Ok(name)
}

/// True when `path` is structurally safe to write under `Wardrobe/`, whatever
/// its spelling (v4 `isSafeWardrobeFolderPath`): no empty, `.` or `..`
/// segment, no backslash, not under the reserved first-level `images/`, within
/// the path cap. Every folder the reader can produce from a real store passes
/// — including hand-made ones the normaliser would respell (`Hats: Summer`,
/// `Formal/images`) — while an imported `../x` does not.
pub fn is_safe_wardrobe_folder_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if utf16_len(path) > 900 || path.contains('\\') {
        return false;
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments
        .iter()
        .any(|s| s.is_empty() || js_trim(s) != *s || *s == "." || *s == "..")
    {
        return false;
    }
    !is_reserved_segment(segments[0])
}

/// Resolve an operator's requested folder against the folders a wardrobe
/// already has (v4 `resolveWardrobeFolder`). A folder that exists (in any
/// casing) is answered exactly as stored — even a hand-made one the rules
/// would respell, so filing into a Scriptorium folder never quietly makes a
/// sanitised sibling. Otherwise the longest existing ancestor keeps its stored
/// spelling and only the new remainder is normalised (and the whole must stay
/// within the depth cap). Errs when the new part breaks the rules.
pub fn resolve_wardrobe_folder(
    existing: &[String],
    requested: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    let raw = raw_segments(requested.unwrap_or(""));
    if raw.is_empty() {
        return Ok(String::new());
    }

    // v4 `new Map(existing.map(p => [p.toLowerCase(), p]))` — a later
    // duplicate key REPLACES an earlier one.
    let mut by_key: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for path in existing {
        by_key.insert(path.to_lowercase(), path.as_str());
    }
    for depth in (1..=raw.len()).rev() {
        let key = raw[..depth].join("/").to_lowercase();
        let Some(stored) = by_key.get(&key) else {
            continue;
        };
        if depth == raw.len() {
            return Ok((*stored).to_string());
        }
        let rest = normalize_wardrobe_folder_path(Some(&raw[depth..].join("/")))?;
        let joined = join_folder(&[stored, &rest]);
        if folder_segments(&joined).len() > WARDROBE_FOLDER_MAX_DEPTH {
            return Err(WardrobeFolderPathError(format!(
                "Wardrobe folders nest at most {WARDROBE_FOLDER_MAX_DEPTH} deep"
            )));
        }
        return Ok(joined);
    }
    normalize_wardrobe_folder_path(Some(&raw.join("/")))
}

/// True when `path` is already in normal form (v4 `isValidWardrobeFolderPath`):
/// the root, or segments that are non-empty, unsanitisable-further, not
/// `.`/`..`/reserved, at most eight.
pub fn is_valid_wardrobe_folder_path(path: &str) -> bool {
    matches!(normalize_wardrobe_folder_path(Some(path)), Ok(n) if n == path)
}

/// Split a folder path into its segments (`''` → `[]`) (v4 `folderSegments`).
pub fn folder_segments(folder: &str) -> Vec<&str> {
    if folder.is_empty() {
        Vec::new()
    } else {
        folder.split('/').collect()
    }
}

/// The folder a `Wardrobe/` document lives in (v4 `folderOfWardrobePath`):
/// `Wardrobe/A/B/x.md` → `A/B`, `Wardrobe/x.md` → `''`. A path outside
/// `Wardrobe/` reads as the root. The prefix is compared lower-cased.
pub fn folder_of_wardrobe_path(relative_path: &str) -> String {
    let prefix = format!("{WARDROBE_ROOT}/");
    if !relative_path
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
    {
        return String::new();
    }
    let rest = utf16_slice_from(relative_path, utf16_len(&prefix));
    match rest.rfind('/') {
        None => String::new(),
        Some(slash) => rest[..slash].to_string(),
    }
}

/// The store path of a wardrobe folder (v4 `wardrobeFolderStorePath`): `''` →
/// `Wardrobe`, `A/B` → `Wardrobe/A/B`.
pub fn wardrobe_folder_store_path(folder: &str) -> String {
    if folder.is_empty() {
        WARDROBE_ROOT.to_string()
    } else {
        format!("{WARDROBE_ROOT}/{folder}")
    }
}

/// The folder above `folder`; the root's parent is the root (v4
/// `parentFolder`).
pub fn parent_folder(folder: &str) -> &str {
    match folder.rfind('/') {
        None => "",
        Some(slash) => &folder[..slash],
    }
}

/// The last segment of `folder`; `''` for the root (v4 `folderName`).
pub fn folder_name(folder: &str) -> &str {
    match folder.rfind('/') {
        None => folder,
        Some(slash) => &folder[slash + 1..],
    }
}

/// Join path pieces, skipping empty ones (v4 `joinFolder`):
/// `join_folder(&["", "x.md"])` → `x.md`.
pub fn join_folder(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

/// Folder names are case-insensitive (v4 `isSameFolder`): `Formal` and
/// `formal` are one folder. `None` reads as `''`.
pub fn is_same_folder(a: Option<&str>, b: Option<&str>) -> bool {
    a.unwrap_or("").to_lowercase() == b.unwrap_or("").to_lowercase()
}

/// True when `folder` is `ancestor` or nested anywhere beneath it (v4
/// `isUnderFolder`).
pub fn is_under_folder(folder: &str, ancestor: &str) -> bool {
    if ancestor.is_empty() {
        return true;
    }
    let f = folder.to_lowercase();
    let a = ancestor.to_lowercase();
    f == a || f.starts_with(&format!("{a}/"))
}

/// Re-root `folder` from under `from_ancestor` to under `to_ancestor` (v4
/// `rebaseFolder`; `Formal/Evening`, `Formal` → `Dress` gives `Dress/Evening`).
/// A folder not under `from_ancestor` is returned unchanged. The slice is by
/// `from_ancestor`'s LENGTH (code units) while the containment test is
/// case-blind — v4's shape, ported as written.
pub fn rebase_folder(folder: &str, from_ancestor: &str, to_ancestor: &str) -> String {
    if !is_under_folder(folder, from_ancestor) {
        return folder.to_string();
    }
    let rest = utf16_slice_from(folder, utf16_len(from_ancestor));
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    join_folder(&[to_ancestor, rest])
}

/// The display form of a folder (v4 `folderCrumb`): `Formal › Evening`.
pub fn folder_crumb(folder: &str) -> String {
    folder_segments(folder).join(" \u{203A} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_depth_rules() {
        assert_eq!(normalize_wardrobe_folder_path(None).unwrap(), "");
        assert_eq!(normalize_wardrobe_folder_path(Some("  ")).unwrap(), "");
        assert_eq!(
            normalize_wardrobe_folder_path(Some(" Formal / Evening ")).unwrap(),
            "Formal/Evening"
        );
        assert_eq!(normalize_wardrobe_folder_path(Some("a\\b")).unwrap(), "a/b");
        let deep = (0..9).map(|_| "x").collect::<Vec<_>>().join("/");
        assert_eq!(
            normalize_wardrobe_folder_path(Some(&deep)).unwrap_err().0,
            "Wardrobe folders nest at most 8 deep"
        );
        assert_eq!(
            normalize_wardrobe_folder_path(Some("Formal/.."))
                .unwrap_err()
                .0,
            "A wardrobe folder name may not be \"..\""
        );
        assert_eq!(
            normalize_wardrobe_folder_path(Some("Images"))
                .unwrap_err()
                .0,
            "\"Images\" is reserved for item pictures and cannot be a wardrobe folder"
        );
        assert!(!is_valid_wardrobe_folder_path("Formal/"));
        assert!(is_valid_wardrobe_folder_path(""));
        assert!(is_valid_wardrobe_folder_path("Formal/Evening"));
    }

    #[test]
    fn resolve_keeps_stored_spelling_and_normalises_the_remainder() {
        let existing = vec!["Hats: Summer".to_string(), "Formal".to_string()];
        assert_eq!(
            resolve_wardrobe_folder(&existing, Some("hats: summer")).unwrap(),
            "Hats: Summer"
        );
        assert_eq!(
            resolve_wardrobe_folder(&existing, Some("formal / Evening: Late")).unwrap(),
            "Formal/Evening_ Late"
        );
        assert_eq!(
            resolve_wardrobe_folder(&existing, Some("New: Thing")).unwrap(),
            "New_ Thing"
        );
        assert_eq!(resolve_wardrobe_folder(&existing, None).unwrap(), "");
    }

    #[test]
    fn path_helpers() {
        assert_eq!(folder_of_wardrobe_path("Wardrobe/A/B/x.md"), "A/B");
        assert_eq!(folder_of_wardrobe_path("Wardrobe/x.md"), "");
        assert_eq!(folder_of_wardrobe_path("wardrobe/A/x.md"), "A");
        assert_eq!(folder_of_wardrobe_path("Notes/x.md"), "");
        assert_eq!(wardrobe_folder_store_path(""), "Wardrobe");
        assert_eq!(wardrobe_folder_store_path("A/B"), "Wardrobe/A/B");
        assert_eq!(parent_folder("A/B"), "A");
        assert_eq!(parent_folder("A"), "");
        assert_eq!(folder_name("A/B"), "B");
        assert_eq!(folder_name(""), "");
        assert_eq!(join_folder(&["", "x.md"]), "x.md");
        assert!(is_same_folder(Some("Formal"), Some("formal")));
        assert!(is_same_folder(None, Some("")));
        assert!(is_under_folder("Formal/Evening", "formal"));
        assert!(!is_under_folder("Formality", "Formal"));
        assert_eq!(
            rebase_folder("Formal/Evening", "Formal", "Dress"),
            "Dress/Evening"
        );
        assert_eq!(rebase_folder("Formal", "Formal", ""), "");
        assert_eq!(rebase_folder("Other", "Formal", "Dress"), "Other");
        assert_eq!(folder_crumb("Formal/Evening"), "Formal \u{203A} Evening");
        assert!(is_safe_wardrobe_folder_path("Hats: Summer"));
        assert!(is_safe_wardrobe_folder_path("Formal/images"));
        assert!(!is_safe_wardrobe_folder_path("../x"));
        assert!(!is_safe_wardrobe_folder_path("images/x"));
        assert!(!is_safe_wardrobe_folder_path("a/ b"));
    }
}
