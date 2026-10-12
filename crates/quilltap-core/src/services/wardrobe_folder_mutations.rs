//! Folder mutations — the one way a wardrobe folder is created, renamed or
//! deleted (v4 `lib/wardrobe/folder-mutations.ts`, `4a38222e6`; P4.D279,
//! keystone C1 §11). The folder routes (all four tiers, through the route
//! factory) call these, so the normalisation rules (`wardrobe_folders`) and
//! the logging live in one place; the location does the work.
//!
//! Folders are the operator's filing. An item's folder is its path under
//! `Wardrobe/` and nothing else: a rename moves the files, a delete sends
//! them up a level, and nothing here ever deletes an item.

use rusqlite::Connection;

use crate::db::wardrobe_location::{WardrobeLocation, WardrobeLocationError};
use crate::jsstr::js_trim;
use crate::wardrobe_folders::{
    normalize_wardrobe_folder_name, WardrobeFolder, WardrobeFolderPathError,
};

/// v4 `path.trim().replace(/^\/+|\/+$/g, '')` — the folder named, matched as
/// stored (hand-made spellings included), with only the edges tidied.
fn trim_slashes(path: &str) -> &str {
    js_trim(path).trim_matches('/')
}

/// v4 `createWardrobeFolder(location, path)` — create a folder at a location
/// (an existing one, in any casing, is answered). DEBUG `[FolderMutations]
/// Created wardrobe folder`.
pub fn create_wardrobe_folder(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    path: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    let resolved = location.resolve_folder(mount, path)?;
    if resolved.is_empty() {
        return Err(WardrobeFolderPathError("A folder needs a name".to_string()).into());
    }
    let folder = location.create_folder(main, mount, &resolved)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        requested = %path,
        folder = %folder.path,
        itemCount = folder.item_count,
        context = "wardrobe",
        "[FolderMutations] Created wardrobe folder"
    );
    Ok(folder)
}

/// v4 `renameWardrobeFolder(location, path, name)` — rename a folder in place;
/// its items follow because the path is the truth. The folder named is an
/// existing one, matched as stored; the store answers a missing one with
/// [`WardrobeLocationError::FolderNotFound`] (a 404). DEBUG `[FolderMutations]
/// Renamed wardrobe folder`.
pub fn rename_wardrobe_folder(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    path: &str,
    name: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    let from = trim_slashes(path);
    let new_name = normalize_wardrobe_folder_name(Some(name))?;
    let folder = location.rename_folder(main, mount, from, &new_name)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        from = %from,
        to = %folder.path,
        itemCount = folder.item_count,
        context = "wardrobe",
        "[FolderMutations] Renamed wardrobe folder"
    );
    Ok(folder)
}

/// v4 `deleteWardrobeFolder(location, path)` — delete a folder, sending its
/// items and subfolders up to its parent; `(moved_items, moved_folders)`.
/// DEBUG `[FolderMutations] Deleted wardrobe folder`.
pub fn delete_wardrobe_folder(
    main: &Connection,
    mount: &Connection,
    location: &WardrobeLocation,
    path: &str,
) -> Result<(usize, usize), WardrobeLocationError> {
    let folder = trim_slashes(path);
    let deleted = location.delete_folder(main, mount, folder)?;
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        folder = %folder,
        parent = %deleted.parent,
        movedItems = deleted.moved_items,
        movedFolders = deleted.moved_folders,
        context = "wardrobe",
        "[FolderMutations] Deleted wardrobe folder"
    );
    Ok((deleted.moved_items, deleted.moved_folders))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_slashes_tidies_only_the_edges() {
        assert_eq!(trim_slashes("  /Formal/Evening//  "), "Formal/Evening");
        assert_eq!(trim_slashes("Hats: Summer"), "Hats: Summer");
        assert_eq!(trim_slashes("///"), "");
        // JS trim: U+00A0 counts as whitespace.
        assert_eq!(trim_slashes("\u{00a0}/Casual"), "Casual");
    }
}
