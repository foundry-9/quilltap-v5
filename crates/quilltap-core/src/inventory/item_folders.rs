//! Item folders — the filing rules shared by every "folder of item files" in
//! a store: the wardrobe's `Wardrobe/` and the Inventory's `Inventory/` (v4
//! `lib/mount-index/item-folders.ts`, `4fbdad186`; P4.D324 — the LIFT).
//!
//! A folder is a path relative to the root folder: `''` for the root,
//! otherwise `Seg/Seg/Seg` with no leading or trailing slash. It is the file's
//! path and nothing else — derived from `relativePath` on read, written back
//! as the path on projection, never stored in the file.
//!
//! Every rule is parameterised by an [`ItemFolderRules`]: the root folder,
//! the segments reserved at any depth (`images/` holds item pictures) and the
//! depth cap. [`crate::wardrobe_folders`] and [`crate::inventory::folders`]
//! are one-line delegates over their own rules, so neither tier copies the
//! other. With `noun: "wardrobe"` every sentence here is byte-identical to the
//! pre-lift wardrobe sentence (the `wardrobe_folders_equivalence` regression
//! net proves it).
//!
//! Pure — no I/O, no logger. Every `toLowerCase()` is `str::to_lowercase`
//! (byte-identical for JS per Phase 1), every `.trim()` is
//! [`crate::jsstr::js_trim`], and every `.length` / `.slice` counts UTF-16
//! code units as JS does.

use crate::jsstr::js_trim;
use crate::vault_overlay::sanitize_file_name;

/// The rules one item tree files by (v4 `ItemFolderRules`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemFolderRules {
    /// The store folder every item lives under, e.g. `Wardrobe`.
    pub root: &'static str,
    /// Segments a folder may never use at any depth, lower-case.
    pub reserved: &'static [&'static str],
    /// The deepest a folder may nest.
    pub max_depth: usize,
    /// The noun error messages use, e.g. `wardrobe`.
    pub noun: &'static str,
}

/// A folder as the browser and the folder routes see it (v4 `ItemFolder`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ItemFolder {
    /// Path relative to the root folder, e.g. `Formal/Evening`. Never `''`.
    pub path: String,
    /// The last segment.
    pub name: String,
    /// Non-archived items directly inside (not in subfolders).
    #[serde(rename = "itemCount")]
    pub item_count: usize,
}

/// v4 `ItemFolderNotFoundError`'s message: `${Capitalised noun} folder not
/// found: ${folder}`.
pub fn not_found_message(noun: &str, folder: &str) -> String {
    format!("{} folder not found: {folder}", capitalise(noun))
}

/// v4 `ItemFolderConflictError`'s default message: `A ${noun} folder named
/// "${folder}" already exists`.
pub fn conflict_message(noun: &str, folder: &str) -> String {
    format!("A {noun} folder named \"{folder}\" already exists")
}

/// v4 `ItemNotInLocationError`'s message.
pub fn not_in_location_message(noun: &str, item_ids: &[String]) -> String {
    format!(
        "Not in this {noun} (shared items are filed where they live): {}",
        item_ids.join(", ")
    )
}

/// `A ${noun} folder name may not be "${raw}"`.
pub fn dot_segment_message(noun: &str, raw: &str) -> String {
    format!("A {noun} folder name may not be \"{raw}\"")
}

/// `"${segment}" is reserved for item pictures and cannot be a ${noun} folder`.
pub fn reserved_segment_message(noun: &str, segment: &str) -> String {
    format!("\"{segment}\" is reserved for item pictures and cannot be a {noun} folder")
}

/// `${Capitalised noun} folders nest at most ${maxDepth} deep`.
pub fn depth_message(rules: &ItemFolderRules) -> String {
    format!(
        "{} folders nest at most {} deep",
        capitalise(rules.noun),
        rules.max_depth
    )
}

/// `A folder name may not contain "/"`.
pub const SLASH_IN_NAME_MESSAGE: &str = "A folder name may not contain \"/\"";
/// `A folder needs a name`.
pub const EMPTY_NAME_MESSAGE: &str = "A folder needs a name";

/// JS `.length` — UTF-16 code units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JS `s.slice(n)` for a code-unit offset `n` (every caller's `n` is the
/// length of an ASCII prefix or of a whole-character prefix of `s`).
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

/// True when `segment` is reserved by `rules`, case-insensitively.
pub fn is_reserved_item_folder_segment(rules: &ItemFolderRules, segment: &str) -> bool {
    let lower = segment.to_lowercase();
    rules.reserved.iter().any(|reserved| *reserved == lower)
}

/// v4's `input.split(/[\\/]/).map(trim).filter(non-empty)`.
fn raw_segments(input: &str) -> Vec<&str> {
    input
        .split(['\\', '/'])
        .map(js_trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Split a folder path into its segments (`''` → `[]`).
pub fn folder_segments(folder: &str) -> Vec<&str> {
    if folder.is_empty() {
        Vec::new()
    } else {
        folder.split('/').collect()
    }
}

/// Normalise an operator-typed folder path: trim, split on `/` (and `\`),
/// drop empty segments, sanitise each with the item-file rule. Errs (with v4's
/// sentence) for `.` / `..` / a reserved segment, or more than
/// `rules.max_depth` segments. `None` / `''` (or whitespace) is the root.
pub fn normalize_item_folder_path(
    rules: &ItemFolderRules,
    input: Option<&str>,
) -> Result<String, String> {
    let Some(input) = input else {
        return Ok(String::new());
    };
    let mut out: Vec<String> = Vec::new();
    for raw in raw_segments(input) {
        if raw == "." || raw == ".." {
            return Err(dot_segment_message(rules.noun, raw));
        }
        let segment = sanitize_file_name(raw);
        if is_reserved_item_folder_segment(rules, &segment) {
            return Err(reserved_segment_message(rules.noun, &segment));
        }
        out.push(segment);
    }
    if out.len() > rules.max_depth {
        return Err(depth_message(rules));
    }
    Ok(out.join("/"))
}

/// Normalise a folder an existing item already carries (a transfer's source
/// folder, which the Scriptorium may have made by hand): its normal form, or
/// the root when it can't be one.
pub fn normalize_item_folder_path_or_root(rules: &ItemFolderRules, input: Option<&str>) -> String {
    normalize_item_folder_path(rules, input).unwrap_or_default()
}

/// Normalise a folder *name* — one segment, as a rename takes it.
pub fn normalize_item_folder_name(
    rules: &ItemFolderRules,
    input: Option<&str>,
) -> Result<String, String> {
    if let Some(s) = input {
        if s.contains(['\\', '/']) {
            return Err(SLASH_IN_NAME_MESSAGE.to_string());
        }
    }
    let name = normalize_item_folder_path(rules, input)?;
    if name.is_empty() {
        return Err(EMPTY_NAME_MESSAGE.to_string());
    }
    Ok(name)
}

/// True when `path` is structurally safe to write under the root, whatever
/// its spelling: no empty, `.` or `..` segment, no backslash, not under a
/// reserved first-level segment, within the path cap. Every folder the reader
/// can produce from a real store passes — including hand-made ones the
/// normaliser would respell — while an imported `../x` does not.
pub fn is_safe_item_folder_path(rules: &ItemFolderRules, path: &str) -> bool {
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
    !is_reserved_item_folder_segment(rules, segments[0])
}

/// Resolve an operator's requested folder against the folders a tier already
/// has. A folder that exists (in any casing) is answered exactly as stored.
/// Otherwise the longest existing ancestor keeps its stored spelling and only
/// the new remainder is normalised (and the whole must stay within the cap).
pub fn resolve_item_folder(
    rules: &ItemFolderRules,
    existing: &[String],
    requested: Option<&str>,
) -> Result<String, String> {
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
        let rest = normalize_item_folder_path(rules, Some(&raw[depth..].join("/")))?;
        let joined = join_folder(&[stored, &rest]);
        if folder_segments(&joined).len() > rules.max_depth {
            return Err(depth_message(rules));
        }
        return Ok(joined);
    }
    normalize_item_folder_path(rules, Some(&raw.join("/")))
}

/// True when `path` is already in normal form.
pub fn is_valid_item_folder_path(rules: &ItemFolderRules, path: &str) -> bool {
    matches!(normalize_item_folder_path(rules, Some(path)), Ok(n) if n == path)
}

/// The folder a document under the root lives in: `Root/A/B/x.md` → `A/B`,
/// `Root/x.md` → `''`. A path outside the root reads as the root. The prefix
/// is compared lower-cased.
pub fn folder_of_item_path(rules: &ItemFolderRules, relative_path: &str) -> String {
    let prefix = format!("{}/", rules.root);
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

/// The store path of a folder: `''` → `Root`, `A/B` → `Root/A/B`.
pub fn item_folder_store_path(rules: &ItemFolderRules, folder: &str) -> String {
    if folder.is_empty() {
        rules.root.to_string()
    } else {
        format!("{}/{folder}", rules.root)
    }
}

/// The folder path under the root for a store folder path, or `None` when
/// the path is the root itself, outside it, or under a reserved first
/// segment.
pub fn folder_under_item_root(rules: &ItemFolderRules, store_path: &str) -> Option<String> {
    let prefix = format!("{}/", rules.root);
    if !store_path
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
    {
        return None;
    }
    let folder = utf16_slice_from(store_path, utf16_len(&prefix));
    if folder.is_empty() {
        return None;
    }
    if is_reserved_item_folder_segment(rules, folder_segments(folder)[0]) {
        return None;
    }
    Some(folder.to_string())
}

/// True for a document path that can hold an item: a file ending `ext`
/// anywhere under the root, except under a reserved first-level segment
/// (`images/`) and except a file named `skip_file_name` (case-insensitive) at
/// any depth. Content is not consulted — see each tier's document predicate.
pub fn is_item_document_path(
    rules: &ItemFolderRules,
    relative_path: &str,
    ext: &str,
    skip_file_name: Option<&dyn Fn(&str) -> bool>,
) -> bool {
    let prefix = format!("{}/", rules.root).to_lowercase();
    let path_lower = relative_path.to_lowercase();
    if !path_lower.starts_with(&prefix) {
        return false;
    }
    let rest = utf16_slice_from(&path_lower, utf16_len(&prefix));
    if rest.is_empty() || !rest.ends_with(&ext.to_lowercase()) {
        return false;
    }
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return false;
    }
    if segments.len() > 1 && is_reserved_item_folder_segment(rules, segments[0]) {
        return false;
    }
    !skip_file_name.is_some_and(|skip| skip(segments[segments.len() - 1]))
}

/// The folder above `folder`; the root's parent is the root.
pub fn parent_folder(folder: &str) -> &str {
    match folder.rfind('/') {
        None => "",
        Some(slash) => &folder[..slash],
    }
}

/// The last segment of `folder`; `''` for the root.
pub fn folder_name(folder: &str) -> &str {
    match folder.rfind('/') {
        None => folder,
        Some(slash) => &folder[slash + 1..],
    }
}

/// Join path pieces, skipping empty ones: `join_folder(&["", "x.md"])` →
/// `x.md`.
pub fn join_folder(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

/// Folder names are case-insensitive: `Formal` and `formal` are one folder.
/// `None` reads as `''`.
pub fn is_same_folder(a: Option<&str>, b: Option<&str>) -> bool {
    a.unwrap_or("").to_lowercase() == b.unwrap_or("").to_lowercase()
}

/// True when `folder` is `ancestor` or nested anywhere beneath it.
pub fn is_under_folder(folder: &str, ancestor: &str) -> bool {
    if ancestor.is_empty() {
        return true;
    }
    let f = folder.to_lowercase();
    let a = ancestor.to_lowercase();
    f == a || f.starts_with(&format!("{a}/"))
}

/// Re-root `folder` from under `from_ancestor` to under `to_ancestor`
/// (`Formal/Evening`, `Formal` → `Dress` gives `Dress/Evening`). A folder not
/// under `from_ancestor` is returned unchanged. The slice is by
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

/// The display form of a folder: `Formal › Evening`.
pub fn folder_crumb(folder: &str) -> String {
    folder_segments(folder).join(" \u{203A} ")
}

/// v4's private `capitalise` — the first UTF-16 unit upper-cased (every noun
/// is ASCII, so the first char).
pub fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: ItemFolderRules = ItemFolderRules {
        root: "Inventory",
        reserved: &["images"],
        max_depth: 8,
        noun: "inventory",
    };

    #[test]
    fn messages_follow_the_noun() {
        assert_eq!(
            normalize_item_folder_path(&RULES, Some("a/../b")).unwrap_err(),
            // v4's `A ${noun}` — "A inventory", grammar and all.
            "A inventory folder name may not be \"..\""
        );
        assert_eq!(
            not_found_message("inventory", "X"),
            "Inventory folder not found: X"
        );
        assert_eq!(
            depth_message(&RULES),
            "Inventory folders nest at most 8 deep"
        );
        assert_eq!(
            conflict_message("inventory", "X"),
            "A inventory folder named \"X\" already exists"
        );
    }

    #[test]
    fn document_paths() {
        assert!(is_item_document_path(
            &RULES,
            "Inventory/a.json",
            ".json",
            None
        ));
        assert!(is_item_document_path(
            &RULES,
            "inventory/Sub/A.JSON",
            ".json",
            None
        ));
        assert!(!is_item_document_path(
            &RULES,
            "Inventory/images/x/a.json",
            ".json",
            None
        ));
        assert!(is_item_document_path(
            &RULES,
            "Inventory/images.json",
            ".json",
            None
        ));
        assert!(!is_item_document_path(
            &RULES,
            "Inventory/a.md",
            ".json",
            None
        ));
        assert!(!is_item_document_path(
            &RULES,
            "Inventory//a.json",
            ".json",
            None
        ));
        let skip = |name: &str| name == "instructions.md";
        assert!(!is_item_document_path(
            &RULES,
            "Inventory/Sub/instructions.md",
            ".md",
            Some(&skip)
        ));
    }

    #[test]
    fn under_root() {
        assert_eq!(folder_under_item_root(&RULES, "Inventory"), None);
        assert_eq!(folder_under_item_root(&RULES, "Inventory/"), None);
        assert_eq!(
            folder_under_item_root(&RULES, "inventory/A/B").as_deref(),
            Some("A/B")
        );
        assert_eq!(folder_under_item_root(&RULES, "Inventory/Images/x"), None);
        assert_eq!(folder_under_item_root(&RULES, "Other/x"), None);
        assert_eq!(capitalise("wardrobe"), "Wardrobe");
        assert_eq!(capitalise(""), "");
    }
}
