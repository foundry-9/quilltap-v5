//! The wardrobe-picture PATH helpers — v4 `lib/file-storage/
//! wardrobe-image-bridge.ts:46-58` (`7c8572869`, #82; P4.D255). FROZEN for
//! the round (contract C1 §7): the bridge's WRITE path is P4.D263's
//! `services::wardrobe_image_bridge`; the `.qtap` image import (P4.D264) needs
//! only these paths.
//!
//! An item's pictures live in the mount that holds its `Wardrobe/*.md`,
//! beside the markdown, keyed by item id so a rename cannot orphan them:
//! `Wardrobe/images/<itemId>/<yyyymmdd-hhmmss>-<kind>-<8 hex>.webp`.

/// v4 `sanitizeLeafName` (`bridge-path-helpers.ts:34`) — the ONE shared copy
/// (the four private copies elsewhere stay as they are).
pub use crate::services::file_storage::sanitize_leaf_name;

/// v4 `WARDROBE_IMAGES_FOLDER` — where an item's pictures live inside its mount.
pub const WARDROBE_IMAGES_FOLDER: &str = "Wardrobe/images";

/// v4 `wardrobeItemImageFolder(itemId)` — `Wardrobe/images/<itemId>`, the id
/// through [`sanitize_leaf_name`].
pub fn wardrobe_item_image_folder(item_id: &str) -> String {
    format!("{WARDROBE_IMAGES_FOLDER}/{}", sanitize_leaf_name(item_id))
}

/// v4 `wardrobeItemImagePath(itemId, leafName)` — the mount-relative path of
/// one picture (the leaf name is NOT re-sanitized, as v4's is not).
pub fn wardrobe_item_image_path(item_id: &str, leaf_name: &str) -> String {
    format!("{}/{leaf_name}", wardrobe_item_image_folder(item_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let id = "7d8c4a4e-5b1d-4f3a-9a2e-1c2b3d4e5f60";
        assert_eq!(
            wardrobe_item_image_folder(id),
            format!("Wardrobe/images/{id}")
        );
        assert_eq!(
            wardrobe_item_image_path(id, "20261007-142233-generated-abcd1234.webp"),
            format!("Wardrobe/images/{id}/20261007-142233-generated-abcd1234.webp")
        );
    }

    /// An id carrying a path separator cannot escape the folder: v4's
    /// `sanitizeLeafName` keeps only the LAST path component.
    #[test]
    fn an_id_with_a_slash_is_sanitized() {
        assert_eq!(
            wardrobe_item_image_folder("../x/evil"),
            "Wardrobe/images/evil"
        );
        assert_eq!(wardrobe_item_image_folder("a:b"), "Wardrobe/images/a_b");
        assert_eq!(wardrobe_item_image_folder(""), "Wardrobe/images/unnamed");
    }
}
