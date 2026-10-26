//! Wardrobe folders — the rules for the operator's filing inside a tier's
//! `Wardrobe/` folder (v4 `lib/wardrobe/folders.ts`, `4a38222e6`; P4.D279).
//!
//! A folder is a path relative to `Wardrobe/`: `''` for the root, otherwise
//! `Seg/Seg/Seg` with no leading or trailing slash. It is the file's path and
//! nothing else — derived from `relativePath` on read, written back as the
//! path on projection, never stored in frontmatter. Folders organise what the
//! operator sees; they carry no meaning for what a character can wear.
//!
//! The rules themselves are shared with the Inventory and live in
//! [`crate::inventory::item_folders`] (v4 `4fbdad186` lifted them into
//! `lib/mount-index/item-folders.ts`; P4.D324); everything here is a delegate
//! over [`WARDROBE_FOLDER_RULES`], and with `noun: "wardrobe"` every sentence
//! is byte-identical to the pre-lift one (the `wardrobe_folders_equivalence`
//! oracle is unchanged across the lift).

use crate::inventory::item_folders::{self, ItemFolder, ItemFolderRules};

pub use crate::inventory::item_folders::{
    folder_crumb, folder_name, folder_segments, is_same_folder, is_under_folder, join_folder,
    parent_folder, rebase_folder,
};

/// The deepest a wardrobe folder may nest (v4 `WARDROBE_FOLDER_MAX_DEPTH`).
pub const WARDROBE_FOLDER_MAX_DEPTH: usize = 8;

/// Segments a wardrobe folder may never use, compared case-insensitively
/// (v4 `WARDROBE_RESERVED_FOLDER_SEGMENTS`). `Wardrobe/images/<itemId>/` holds
/// item pictures.
pub const WARDROBE_RESERVED_FOLDER_SEGMENTS: &[&str] = &["images"];

/// The wardrobe's filing rules (v4 `WARDROBE_FOLDER_RULES`).
pub const WARDROBE_FOLDER_RULES: ItemFolderRules = ItemFolderRules {
    root: "Wardrobe",
    reserved: WARDROBE_RESERVED_FOLDER_SEGMENTS,
    max_depth: WARDROBE_FOLDER_MAX_DEPTH,
    noun: "wardrobe",
};

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

/// A folder as the browser and the folder routes see it (v4 `WardrobeFolder
/// = ItemFolder`).
pub type WardrobeFolder = ItemFolder;

/// v4 `WardrobeFolderNotFoundError`'s message.
pub fn not_found_message(folder: &str) -> String {
    item_folders::not_found_message(WARDROBE_FOLDER_RULES.noun, folder)
}

/// v4 `WardrobeFolderConflictError`'s default message.
pub fn conflict_default_message(folder: &str) -> String {
    item_folders::conflict_message(WARDROBE_FOLDER_RULES.noun, folder)
}

/// v4 `WardrobeItemNotInLocationError`'s message.
pub fn not_in_location_message(item_ids: &[String]) -> String {
    item_folders::not_in_location_message(WARDROBE_FOLDER_RULES.noun, item_ids)
}

/// Normalise an operator-typed folder path (v4 `normalizeWardrobeFolderPath`).
/// `None` / `''` (or whitespace) is the root.
pub fn normalize_wardrobe_folder_path(
    input: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    item_folders::normalize_item_folder_path(&WARDROBE_FOLDER_RULES, input)
        .map_err(WardrobeFolderPathError)
}

/// Normalise a folder an existing item already carries (v4
/// `normalizeWardrobeFolderPathOrRoot`): its normal form, or the root when it
/// can't be one.
pub fn normalize_wardrobe_folder_path_or_root(input: Option<&str>) -> String {
    item_folders::normalize_item_folder_path_or_root(&WARDROBE_FOLDER_RULES, input)
}

/// Normalise a folder *name* — one segment, as a rename takes it (v4
/// `normalizeWardrobeFolderName`).
pub fn normalize_wardrobe_folder_name(
    input: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    item_folders::normalize_item_folder_name(&WARDROBE_FOLDER_RULES, input)
        .map_err(WardrobeFolderPathError)
}

/// True when `path` is structurally safe to write under `Wardrobe/` (v4
/// `isSafeWardrobeFolderPath`).
pub fn is_safe_wardrobe_folder_path(path: &str) -> bool {
    item_folders::is_safe_item_folder_path(&WARDROBE_FOLDER_RULES, path)
}

/// Resolve an operator's requested folder against the folders a wardrobe
/// already has (v4 `resolveWardrobeFolder`).
pub fn resolve_wardrobe_folder(
    existing: &[String],
    requested: Option<&str>,
) -> Result<String, WardrobeFolderPathError> {
    item_folders::resolve_item_folder(&WARDROBE_FOLDER_RULES, existing, requested)
        .map_err(WardrobeFolderPathError)
}

/// True when `path` is already in normal form (v4 `isValidWardrobeFolderPath`).
pub fn is_valid_wardrobe_folder_path(path: &str) -> bool {
    item_folders::is_valid_item_folder_path(&WARDROBE_FOLDER_RULES, path)
}

/// The folder a `Wardrobe/` document lives in (v4 `folderOfWardrobePath`):
/// `Wardrobe/A/B/x.md` → `A/B`, `Wardrobe/x.md` → `''`.
pub fn folder_of_wardrobe_path(relative_path: &str) -> String {
    item_folders::folder_of_item_path(&WARDROBE_FOLDER_RULES, relative_path)
}

/// The store path of a wardrobe folder (v4 `wardrobeFolderStorePath`): `''` →
/// `Wardrobe`, `A/B` → `Wardrobe/A/B`.
pub fn wardrobe_folder_store_path(folder: &str) -> String {
    item_folders::item_folder_store_path(&WARDROBE_FOLDER_RULES, folder)
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
