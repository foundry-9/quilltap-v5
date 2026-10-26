//! Inventory folders — the rules for the operator's filing inside a tier's
//! `Inventory/` folder (v4 `lib/inventory/folders.ts`, `4fbdad186`; P4.D324).
//! Exactly the wardrobe's rules: the path is the truth, `''` is the root, at
//! most eight segments, `images` reserved at any depth, case-insensitively
//! unique siblings.
//!
//! Every helper is a delegate over [`INVENTORY_FOLDER_RULES`]; the rules live
//! once in [`super::item_folders`].

use super::item_folders::{self, ItemFolder, ItemFolderRules};

pub use super::item_folders::{
    folder_crumb, folder_name, folder_segments, is_same_folder, is_under_folder, join_folder,
    parent_folder, rebase_folder,
};

/// The folder every tier's objects live under (the ONE source — the
/// character archive's kept-folder list reads it).
pub const INVENTORY_FOLDER: &str = "Inventory";

/// The deepest an Inventory folder may nest.
pub const INVENTORY_FOLDER_MAX_DEPTH: usize = 8;

/// `Inventory/images/<itemId>/` holds object pictures.
pub const INVENTORY_RESERVED_FOLDER_SEGMENTS: &[&str] = &["images"];

/// The Inventory's filing rules.
pub const INVENTORY_FOLDER_RULES: ItemFolderRules = ItemFolderRules {
    root: INVENTORY_FOLDER,
    reserved: INVENTORY_RESERVED_FOLDER_SEGMENTS,
    max_depth: INVENTORY_FOLDER_MAX_DEPTH,
    noun: "inventory",
};

/// v4 `InventoryFolderPathSchema`'s refine message, interpolated.
pub const INVENTORY_FOLDER_SCHEMA_MESSAGE: &str =
    "Inventory folder must be a clean path of at most 8 segments, none named \"images\"";

/// A folder as the browser and the folder routes see it.
pub type InventoryFolder = ItemFolder;

/// v4's four folder error classes (`InventoryFolderPathError` — a 400,
/// `InventoryFolderNotFoundError` — a 404, `InventoryFolderConflictError` — a
/// 409, `InventoryItemNotInLocationError` — a 400). `Display` is v4's
/// `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryFolderError {
    /// The path breaks the rules; the payload is v4's sentence.
    Path(String),
    /// The folder named doesn't exist; the payload is the folder.
    NotFound(String),
    /// The change collides with what's there; the payload is the MESSAGE
    /// (v4's default `A inventory folder named "…" already exists`, or the
    /// site's own sentence).
    Conflict(String),
    /// A filing request named objects filed in another tier.
    NotInLocation(Vec<String>),
}

impl InventoryFolderError {
    /// v4 `new InventoryFolderConflictError(folder)` — the default message.
    pub fn conflict_default(folder: &str) -> Self {
        InventoryFolderError::Conflict(item_folders::conflict_message("inventory", folder))
    }
}

impl std::fmt::Display for InventoryFolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InventoryFolderError::Path(message) | InventoryFolderError::Conflict(message) => {
                f.write_str(message)
            }
            InventoryFolderError::NotFound(folder) => {
                f.write_str(&item_folders::not_found_message("inventory", folder))
            }
            InventoryFolderError::NotInLocation(ids) => {
                f.write_str(&item_folders::not_in_location_message("inventory", ids))
            }
        }
    }
}

impl std::error::Error for InventoryFolderError {}

/// Normalise an operator-typed folder path.
pub fn normalize_inventory_folder_path(
    input: Option<&str>,
) -> Result<String, InventoryFolderError> {
    item_folders::normalize_item_folder_path(&INVENTORY_FOLDER_RULES, input)
        .map_err(InventoryFolderError::Path)
}

/// A folder an existing object carries, in normal form or the root.
pub fn normalize_inventory_folder_path_or_root(input: Option<&str>) -> String {
    item_folders::normalize_item_folder_path_or_root(&INVENTORY_FOLDER_RULES, input)
}

/// Normalise a folder *name* — one segment, as a rename takes it.
pub fn normalize_inventory_folder_name(
    input: Option<&str>,
) -> Result<String, InventoryFolderError> {
    item_folders::normalize_item_folder_name(&INVENTORY_FOLDER_RULES, input)
        .map_err(InventoryFolderError::Path)
}

/// True when `path` is structurally safe to write under `Inventory/`.
pub fn is_safe_inventory_folder_path(path: &str) -> bool {
    item_folders::is_safe_item_folder_path(&INVENTORY_FOLDER_RULES, path)
}

/// Resolve an operator's requested folder against the folders a tier already
/// has.
pub fn resolve_inventory_folder(
    existing: &[String],
    requested: Option<&str>,
) -> Result<String, InventoryFolderError> {
    item_folders::resolve_item_folder(&INVENTORY_FOLDER_RULES, existing, requested)
        .map_err(InventoryFolderError::Path)
}

/// True when `path` is already in normal form.
pub fn is_valid_inventory_folder_path(path: &str) -> bool {
    item_folders::is_valid_item_folder_path(&INVENTORY_FOLDER_RULES, path)
}

/// The folder an `Inventory/` document lives in: `Inventory/A/B/x.json` →
/// `A/B`.
pub fn folder_of_inventory_path(relative_path: &str) -> String {
    item_folders::folder_of_item_path(&INVENTORY_FOLDER_RULES, relative_path)
}

/// The store path of an Inventory folder: `''` → `Inventory`, `A/B` →
/// `Inventory/A/B`.
pub fn inventory_folder_store_path(folder: &str) -> String {
    item_folders::item_folder_store_path(&INVENTORY_FOLDER_RULES, folder)
}

/// The folder under `Inventory/` for a store folder path (or `None`).
pub fn folder_under_inventory_root(store_path: &str) -> Option<String> {
    item_folders::folder_under_item_root(&INVENTORY_FOLDER_RULES, store_path)
}

/// True for a document path that can hold an Inventory object: a `.json`
/// file anywhere under `Inventory/` except under `Inventory/images/`. NO
/// skip-file predicate (the wardrobe's skips `instructions.md`). Whether its
/// content parses is `is_inventory_item_document`'s question.
pub fn is_inventory_item_document_path(relative_path: &str) -> bool {
    item_folders::is_item_document_path(&INVENTORY_FOLDER_RULES, relative_path, ".json", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages() {
        assert_eq!(
            InventoryFolderError::NotFound("A/B".into()).to_string(),
            "Inventory folder not found: A/B"
        );
        assert_eq!(
            InventoryFolderError::conflict_default("A").to_string(),
            "A inventory folder named \"A\" already exists"
        );
        assert_eq!(
            InventoryFolderError::NotInLocation(vec!["x".into(), "y".into()]).to_string(),
            "Not in this inventory (shared items are filed where they live): x, y"
        );
        assert_eq!(
            normalize_inventory_folder_path(Some("images"))
                .unwrap_err()
                .to_string(),
            "\"images\" is reserved for item pictures and cannot be a inventory folder"
        );
        assert!(is_inventory_item_document_path(
            "Inventory/Sub/instructions.json"
        ));
    }
}
